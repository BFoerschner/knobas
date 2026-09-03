//! The real [`Steps`]: the write queue, the link store and the mirror.
//!
//! Everything here is IO, and deliberately nothing else -- the orchestrator
//! next door holds the policy, and this is what it holds policy *over*. That
//! split is what lets a fake stand in for this file and the flow's real
//! behaviour still be the thing under test.
//!
//! It does not name `WriteOp`: `dispatch` forwards the payload the plan
//! composed to `crate::sources::write_queue::submit`, which is where a write op
//! is decoded and checked against what the source declares.

use async_trait::async_trait;
use knobas_core::entity::EntityRef;
use knobas_core::write_queue::{self, WriteState};
use sqlx::PgPool;

use super::{Landing, Linked, Steps};
use crate::sources::SourcesState;
use crate::{IpcError, IpcErrorCode};

/// The mirrored kinds the flow reads back.
///
/// Generic kinds, from `CONTEXT.md`'s list -- not one adapter's vocabulary.
/// Every adapter that mirrors a repository's refs declares them under these
/// names, which is what lets these two reads be written once.
const KIND_BRANCH: &str = "branch";
const KIND_PR: &str = "pr";

/// The flow's window onto the world.
pub struct Queue<'a> {
    pub state: &'a SourcesState,
}

#[async_trait]
impl Steps for Queue<'_> {
    async fn dispatch(&self, payload: &serde_json::Value) -> Result<(i64, Landing), IpcError> {
        // The same call the *Comment* button makes. A start-work step is not a
        // privileged write: it queues, it can be held, it can be refused, and
        // it shows up in the pending-writes panel like anything else.
        let queued = crate::sources::write_queue::submit(self.state, payload.clone()).await?;
        // The row came back **as queued**, before the attempt -- which is the
        // honest shape for that call and says nothing about the outcome. Read
        // it again for what actually happened.
        let landing = self.landing_of(queued.id).await?;
        Ok((queued.id, landing))
    }

    async fn landing_of(&self, write_id: i64) -> Result<Landing, IpcError> {
        let row = write_queue::get(&self.state.pool, write_id).await?;
        Ok(match row {
            None => Landing::Withdrawn,
            Some(row) => match row.state {
                WriteState::Sent => Landing::Sent,
                WriteState::Pending => Landing::Waiting { detail: row.detail },
                WriteState::Held => Landing::Held { detail: row.detail },
                WriteState::Refused => Landing::Refused {
                    detail: row
                        .detail
                        .unwrap_or_else(|| "the source refused the write".to_owned()),
                },
                WriteState::Discarded => Landing::Withdrawn,
            },
        })
    }

    async fn link(&self, from: &str, to: &str, relation: &str) -> Result<Linked, IpcError> {
        match crate::commands::entity::create_link_inner(
            &self.state.pool,
            from,
            to,
            Some(relation),
            None,
        )
        .await
        {
            Ok(_) => Ok(Linked::Made),
            // The pair already carries this relation. For a step whose job is
            // that the relationship *exists*, that is the outcome, not a
            // failure -- and it is what makes this step idempotent under retry
            // without a read of its own.
            Err(error) if error.code == IpcErrorCode::Conflict => Ok(Linked::Already),
            Err(error) => Err(error),
        }
    }

    async fn branch(&self, repo: &EntityRef, name: &str) -> Result<Option<String>, IpcError> {
        // Narrowed to the repository by an id prefix before the kind filter:
        // `sync.item` has no index on either and the prefix is the selective
        // half.
        found(
            &self.state.pool,
            BRANCH_BY_NAME,
            &like_prefix(repo),
            KIND_BRANCH,
            name,
        )
        .await
    }

    async fn pull_request(&self, repo: &EntityRef, head: &str) -> Result<Option<String>, IpcError> {
        found(
            &self.state.pool,
            PULL_REQUEST_BY_HEAD,
            &like_prefix(repo),
            KIND_PR,
            head,
        )
        .await
    }

    async fn refresh(&self, source: &str) {
        // Wait for the run, rather than triggering and hoping: the very next
        // thing the caller does is look for the pull request this run is
        // fetching. One implementation of that wait, in
        // `crate::sources::write_queue::resync`, because #289's protocol
        // publish needs the identical thing for the identical reason -- a
        // create answers no address, so the mirror is the only way to name
        // what was made, and reading it before the run has finished is reading
        // it too early.
        crate::sources::write_queue::resync(self.state, source).await;
    }
}

/// The branch of a repository whose own name is `$3`.
///
/// `title` is the branch's name: contract §4.1 makes every adapter build it
/// from the item, and `map.rs` puts the bare branch name there. Reading it
/// rather than reconstructing the entity id is what keeps this statement out of
/// the business of how a branch's key is spelled -- `owner/repo@refs/heads/x`
/// for Gitea, something else for the next adapter.
const BRANCH_BY_NAME: &str = "select entity_id from sync.live_item
      where entity_id like $1 and kind = $2 and title = $3
      order by entity_id limit 1";

/// The pull request of a repository opened from head branch `$3`.
///
/// **A source-shaped read outside an adapter, and it is here because there is
/// no other way.** `Source::write` answers nothing, so the pull request knobas
/// has just created is found by reading it back; §4.1 guarantees `title`,
/// `body_text`, `updated_at` and a verbatim `payload`, and the head branch
/// lives only in the last of those. That is the same seam #43 recorded for a
/// ticket's status, met here in the read direction. It is confined to this one
/// statement deliberately: a source that spells its pull requests differently
/// needs one more `coalesce` here and nothing else anywhere.
const PULL_REQUEST_BY_HEAD: &str = "select entity_id from sync.live_item
      where entity_id like $1 and kind = $2 and payload->'head'->>'ref' = $3
      order by entity_id limit 1";

/// The repository's id as a `like` prefix, with `like`'s own metacharacters
/// escaped.
///
/// `_` is common in repository names and matches *any* character in a
/// pattern, so an unescaped `gitea:acme/payout_service%` would also match
/// `payoutXservice` -- and a look-before-write that found the wrong
/// repository's branch would settle a step "succeeded" against an effect that
/// does not exist. Postgres' default escape character is `\`, and the value
/// is bound, so escaping the three metacharacters is the whole job.
///
/// The escaping itself is [`escape_like`], shared since #289.
fn like_prefix(repo: &EntityRef) -> String {
    format!("{}%", escape_like(&repo.to_string()))
}

/// The escaping half of [`like_prefix`], without the trailing `%`.
///
/// Split out for #289, which needs the same escaping for a **source id**:
/// `commands::entity::ticket_titled` narrows a look-back-after-write to one
/// source's corpus with `<escaped source id>:%`, and source ids carry
/// underscores just as repository names do. It wants the escaping and its own
/// separator, not this function's bare `%` -- and trimming the `%` back off
/// would be wrong for an id that ends in one, since its own `%` is escaped to
/// `\%` and a trim cannot tell the two apart. So the escaping is shared and
/// each caller spells its own pattern.
pub(crate) fn escape_like(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 4);
    for ch in value.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// One entity id, or `None`.
///
/// Every value is a bound parameter and `sql` is a `&'static str` -- the rule
/// `commands/entity.rs` states for this crate, which sqlx now enforces at the
/// type level.
async fn found(
    pool: &PgPool,
    sql: &'static str,
    prefix: &str,
    kind: &str,
    value: &str,
) -> Result<Option<String>, IpcError> {
    sqlx::query_scalar::<_, String>(sql)
        .bind(prefix)
        .bind(kind)
        .bind(value)
        .fetch_optional(pool)
        .await
        .map_err(IpcError::internal)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The look-before-write must not find another repository's branch. `_`
    /// matches any character in a `like` pattern, and repository names carry
    /// underscores all the time.
    #[test]
    fn a_repository_id_is_matched_literally_not_as_a_pattern() {
        let repo = EntityRef::new("gitea", "acme/payout_service");
        assert_eq!(like_prefix(&repo), "gitea:acme/payout\\_service%");
        let plain = EntityRef::new("gitea", "acme/payouts");
        assert_eq!(like_prefix(&plain), "gitea:acme/payouts%");
        // The same escaping a source id gets (#289), without the `%` that
        // would make a trailing one indistinguishable from an escaped one.
        assert_eq!(escape_like("wiki_two"), "wiki\\_two");
        assert_eq!(escape_like("odd%"), "odd\\%");
    }
}
