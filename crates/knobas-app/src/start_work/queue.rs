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
use knobas_sync::SyncTrigger;
use knobas_sync::progress::{ProgressSink, SyncPhase, SyncProgress};
use sqlx::PgPool;
use std::sync::Arc;

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
            &format!("{repo}%"),
            KIND_BRANCH,
            name,
        )
        .await
    }

    async fn pull_request(&self, repo: &EntityRef, head: &str) -> Result<Option<String>, IpcError> {
        found(
            &self.state.pool,
            PULL_REQUEST_BY_HEAD,
            &format!("{repo}%"),
            KIND_PR,
            head,
        )
        .await
    }

    async fn refresh(&self, source: &str) {
        // Wait for the run, rather than triggering and hoping: the very next
        // thing the caller does is look for the pull request this run is
        // fetching. ADR-0005 guarantees the sink is told how the run ended,
        // including when the id handed back belongs to a run already in
        // flight, so this cannot wait for something that will never speak.
        let (done, wait) = tokio::sync::oneshot::channel();
        let sink = Arc::new(Ending {
            done: std::sync::Mutex::new(Some(done)),
        });
        if self
            .state
            .scheduler
            .trigger(source, SyncTrigger::Manual, Some(sink))
            .await
            .is_err()
        {
            return;
        }
        // A failure to wait is a mirror that may be stale, which the step
        // reports as "no pull request has reached the mirror yet" and the user
        // retries. It is never a reason to fail the flow.
        let _ = wait.await;
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

/// A sink that resolves when its run ends.
///
/// ADR-0005: a run id always comes with an ending, so exactly one terminal
/// message arrives here. The `Option` is what makes a second one -- which the
/// ADR says cannot happen, and which this must survive if it ever did -- a
/// no-op rather than a panic inside a sink, which the sync crate would have to
/// catch.
struct Ending {
    done: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

impl ProgressSink for Ending {
    fn report(&self, progress: SyncProgress) {
        if !matches!(progress.phase, SyncPhase::Finished | SyncPhase::Failed) {
            return;
        }
        let sender = self
            .done
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(sender) = sender {
            let _ = sender.send(());
        }
    }
}
