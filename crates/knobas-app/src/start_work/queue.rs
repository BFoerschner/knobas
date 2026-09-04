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
use knobas_core::payload::Declarations;
use knobas_core::write_queue::{self, WriteState};
use sqlx::PgPool;

use super::{Landing, Linked, PullRequestOnHead, Steps};
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

    async fn pull_request(
        &self,
        repo: &EntityRef,
        head: &str,
    ) -> Result<PullRequestOnHead, IpcError> {
        // The declaration is resolved here rather than threaded through
        // `Steps`: it is a property of the running binary's registry, which
        // this implementation has and the trait's other implementations --
        // the fake in the seam tests -- have no use for. `merge::follow_merges`
        // takes it as an argument instead because it is one pass over the whole
        // corpus and the caller already holds it; this is two lookups per flow,
        // and the listing behind `declared_paths` is the same round trip the
        // inbox makes per read.
        let declarations =
            crate::sources::paths::declared_paths(&self.state.pool, self.state.registry.as_ref())
                .await?;
        pull_request_on_head(&self.state.pool, &declarations, repo, head).await
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

/// Every pull request of a repository opened from head branch `$3`, each with
/// what its own source says about whether it is merged.
///
/// **A source-shaped read outside an adapter, and it is here because there is
/// no other way.** `Source::write` answers nothing, so the pull request knobas
/// has just created is found by reading it back; §4.1 guarantees `title`,
/// `body_text`, `updated_at` and a verbatim `payload`, and the head branch
/// lives only in the last of those. That is the same seam #43 recorded for a
/// ticket's status, met here in the read direction. It is confined to this one
/// statement deliberately: a source that spells its pull requests differently
/// needs one more `coalesce` here and nothing else anywhere.
///
/// # Why it is a list and no longer `limit 1` (#359)
///
/// A head branch is re-usable, and the mirror deliberately holds merged pull
/// requests -- `state=all`, with the reason written at
/// `knobas_source_gitea::client::Client::pulls`. So `order by entity_id limit
/// 1` picked **lexicographically among every pull request that head ever
/// had**, merged ones included: on a head whose pull requests are `#7`
/// (merged) and `#8` (open) it answers `#7`, which settled the create step
/// "already open" against a merge and then drew the ticket's link to it.
///
/// The merged flag is read at the path the source **declares** (#277,
/// `$4`), not at a spelling written here: Gitea says its pull requests carry a
/// boolean `merged` and the reverse direction in `super::merge` already follows
/// that same declaration, so the two halves of the pull-request story cannot
/// disagree about what merged means. A source that declares no flag resolves
/// to `null` for every row, which
/// [`PullRequestOnHead`](super::PullRequestOnHead) turns into `Unknown` --
/// the miss ADR-0007 requires, pinned there and by
/// `a_source_that_declares_no_merged_flag_yields_no_open_pull_request`.
///
/// `order by i.entity_id` survives as the tie-break among rows the caller
/// cannot otherwise tell apart, so a head that somehow carries two open pull
/// requests -- two bases, which a forge does allow -- answers the same one on
/// every call rather than whichever the planner reached first.
const PULL_REQUEST_BY_HEAD: &str = concat!(
    "select i.entity_id, ",
    knobas_core::declared_flag!("$4", "merged"),
    " as merged
       from sync.live_item i
      where i.entity_id like $1 and i.kind = $2
        and i.payload->'head'->>'ref' = $3
      order by i.entity_id"
);

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

/// What the mirror holds from one head branch, resolved through the
/// declarations `declarations` carries.
///
/// Public, and taking a pool rather than a [`Queue`], because this is the half
/// of the look-before-write that a test can drive against a real mirror: the
/// [`Steps`] fake stands where the queue does and therefore cannot witness the
/// statement above. `Queue::pull_request` is this function plus the lookup of
/// the declarations.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
pub async fn pull_request_on_head(
    pool: &PgPool,
    declarations: &Declarations,
    repo: &EntityRef,
    head: &str,
) -> Result<PullRequestOnHead, IpcError> {
    let rows: Vec<(String, Option<bool>)> = sqlx::query_as(PULL_REQUEST_BY_HEAD)
        .bind(like_prefix(repo))
        .bind(KIND_PR)
        .bind(head)
        .bind(declarations.as_param())
        .fetch_all(pool)
        .await
        .map_err(IpcError::internal)?;
    Ok(pick_open(&rows))
}

/// Which of a head's pull requests the flow may act on, given each one's
/// declared merged flag.
///
/// **The rule, written once**, because it has two callers: this module, over
/// what [`PULL_REQUEST_BY_HEAD`] answered, and the [`Steps`] fake in
/// `tests/start_work.rs`, over the mirror a test dictated. A fake that
/// classified for itself would be a second opinion about what "open" means,
/// and the seam tests would then pass against a rule the real read does not
/// have.
///
/// `None` is the declared read missing -- an undeclared source, a flag that is
/// not a JSON boolean -- and it never becomes [`PullRequestOnHead::Open`]. See
/// that type for why that direction and not the other.
#[must_use]
pub fn pick_open(rows: &[(String, Option<bool>)]) -> PullRequestOnHead {
    if let Some((id, _)) = rows.iter().find(|(_, merged)| *merged == Some(false)) {
        return PullRequestOnHead::Open(id.clone());
    }
    if let Some((id, _)) = rows.iter().find(|(_, merged)| *merged == Some(true)) {
        return PullRequestOnHead::OnlyMerged(id.clone());
    }
    PullRequestOnHead::Unknown
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

    /// The pinned failure direction (ADR-0007 requirement 3, and #359's whole
    /// question): a pull request whose merged-ness the source declares no path
    /// for is **not** offered as open. It is `Unknown`, so the flow dispatches
    /// and lets the source refuse a duplicate, rather than settling a step
    /// against a record it cannot read.
    #[test]
    fn a_pull_request_of_unreadable_merged_ness_is_never_the_open_one() {
        let unreadable = [("gitea:acme/api#7".to_owned(), None)];
        assert_eq!(pick_open(&unreadable), PullRequestOnHead::Unknown);
        assert_eq!(pick_open(&[]), PullRequestOnHead::Unknown);
    }

    /// The bug: lexicographic order over a re-used head puts the *merged* `#7`
    /// ahead of the open `#8`, so picking the first row is picking a merge.
    /// The open one is chosen however the rows are ordered.
    #[test]
    fn an_open_pull_request_wins_over_a_merged_one_from_the_same_head() {
        let merged_first = [
            ("gitea:acme/api#7".to_owned(), Some(true)),
            ("gitea:acme/api#8".to_owned(), Some(false)),
        ];
        assert_eq!(
            pick_open(&merged_first),
            PullRequestOnHead::Open("gitea:acme/api#8".to_owned())
        );
        let merged_last = [
            ("gitea:acme/api#8".to_owned(), Some(false)),
            ("gitea:acme/api#7".to_owned(), Some(true)),
        ];
        assert_eq!(
            pick_open(&merged_last),
            PullRequestOnHead::Open("gitea:acme/api#8".to_owned())
        );
    }

    /// A head whose every pull request is merged names one, so the step that
    /// refuses can say which -- rather than answering the same "nothing here"
    /// an empty mirror does.
    #[test]
    fn a_head_whose_pull_requests_are_all_merged_names_one() {
        let merged = [
            ("gitea:acme/api#7".to_owned(), Some(true)),
            ("gitea:acme/api#8".to_owned(), Some(true)),
        ];
        assert_eq!(
            pick_open(&merged),
            PullRequestOnHead::OnlyMerged("gitea:acme/api#7".to_owned())
        );
    }
}
