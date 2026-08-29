//! The reverse direction: a merged pull request moves its ticket to In Review
//! (issue #44, stories 17-19).
//!
//! The forgotten half. A pull request gets merged and the ticket sits in In
//! Progress until somebody notices at standup, so knobas does it -- but only
//! within limits it can state:
//!
//! * **Only on links knobas holds.** A merged pull request with no knobas link
//!   to a ticket does nothing at all (story 18). This is what keeps an
//!   automatic status change from ever reaching a ticket the user did not
//!   connect, and it is a join rather than a filter -- there is no code path
//!   here that could touch an unlinked ticket.
//! * **Driven by mirrored state, not by polling a source.** A pull request
//!   becomes merged in the mirror during an ordinary sync; this is a pass over
//!   the mirror afterwards, the same shape `suggest::detect` has and for the
//!   same reason -- there is no per-item hook in the sync engine, and adding
//!   one to serve this would be a second mechanism.
//! * **Once.** See [`ALREADY_FOLLOWED`].
//!
//! # No table, and that is the point
//!
//! What stops this firing every pass is `knobas.write_queue` itself: a
//! `transition` row against the ticket carrying this status -- in **any** state,
//! including the terminal ones -- is the record that knobas has already
//! followed that merge. Rows there are never deleted (migration 0005), so the
//! memory is exactly as durable as a column would be; it is inspectable in the
//! pending-writes panel, which is where a user would look for it; and it cannot
//! drift from the write it is a memory of, because it *is* that write.
//!
//! One consequence, stated rather than discovered: the first pass after a user
//! links an already-merged pull request to a ticket will move that ticket. That
//! is the intended reading of story 17 -- the pull request is merged and the
//! ticket is not In Review -- rather than an accident of having no timestamp to
//! compare against.

use knobas_core::entity::EntityRef;
use sqlx::PgPool;

use super::{Landing, Steps};
use crate::IpcError;

/// The kinds this pass joins. Generic kinds from `CONTEXT.md`, not one
/// adapter's vocabulary.
const KIND_PR: &str = "pr";
const KIND_TICKET: &str = "ticket";

/// A merged pull request and the ticket a knobas link joins it to.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct Merged {
    pub pr_id: String,
    pub ticket_id: String,
}

/// The pairs this pass will act on.
///
/// Read undirected -- `entries_of` is undirected and so is the panel, so which
/// end the user happened to draw from is not a fact this may depend on.
///
/// **`payload->>'merged'` is a source-shaped read outside an adapter.** There
/// is no adapter-independent way to ask whether a pull request is merged: §4.1
/// guarantees `title`, `body_text`, `updated_at` and a verbatim `payload`, and
/// merged-ness lives only in the last of those. It is the same seam #43
/// recorded for a ticket's status. Confined to this one statement, and written
/// so a second spelling is one more `or` here and nothing else anywhere.
///
/// The `not exists` is [`ALREADY_FOLLOWED`]'s half of the same statement.
const MERGED_AND_LINKED: &str = "
select distinct pr.entity_id as pr_id, t.entity_id as ticket_id
  from knobas.confirmed_link l
  join sync.live_item pr
    on pr.entity_id in (l.from_id, l.to_id) and pr.kind = $1
  join sync.live_item t
    on t.entity_id in (l.from_id, l.to_id) and t.kind = $2
 where (pr.payload->>'merged')::text = 'true'
   and not exists (
         select 1 from knobas.write_queue w
          where w.entity_id = t.entity_id
            and w.op = 'transition'
            and w.payload->'Transition'->>'status' = $3)
 order by ticket_id, pr_id";

/// Why the `not exists` above is the whole memory this feature needs.
///
/// A `transition` row against the ticket carrying this status is the record
/// that knobas has already followed a merge for it, whatever became of that
/// write: sent, refused, held, or discarded by the user. All four are decisions
/// that have been made, and re-queueing over any of them would be knobas
/// arguing with the user or with the source.
///
/// The path `payload->'Transition'->>'status'` is `WriteOp`'s serde shape, and
/// it is pinned by `plan`'s
/// `the_transition_payload_is_shaped_the_way_the_reverse_direction_reads_it`
/// so a rename of the variant or the field fails a test rather than silently
/// making this `not exists` match nothing -- which would transition a ticket
/// on every pass.
pub const ALREADY_FOLLOWED: &str = "a transition to this status is already in the write queue";

/// The jsonb path [`MERGED_AND_LINKED`] reads a queued transition's status at.
///
/// Named so `plan`'s pin can assert the statement and the payload agree; a
/// statement that quietly matched nothing would transition a ticket on every
/// pass, which is the one failure this whole `not exists` exists to prevent.
pub const MERGED_AND_LINKED_PATH: &str = "payload->'Transition'->>'status'";

/// Follow every merged pull request knobas holds a link for.
///
/// Answers how many tickets were moved -- the number the shell announces, so
/// an automatic change is visible rather than mysterious (story 19). A ticket
/// whose source will not take the write is counted: the write is queued, the
/// user can see it, and it will go.
///
/// A pair whose transition the queue refuses outright -- a source that does not
/// declare `transition`, a status that source cannot reach -- is **skipped
/// rather than raised**. One ticket that cannot move must not stop the rest,
/// which is the same shape `flush_all` gives one broken source.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the pass's own read fails.
pub async fn follow_merges(
    pool: &PgPool,
    steps: &dyn Steps,
    status: &str,
) -> Result<u32, IpcError> {
    let mut moved = 0;
    for pair in due(pool, status).await? {
        let Ok(ticket) = EntityRef::parse(&pair.ticket_id) else {
            continue;
        };
        let payload = super::plan::transition(&ticket, status);
        match steps.dispatch(&payload).await {
            // Queued or delivered are both "knobas has followed this merge".
            // The queue is what makes the second one eventually true.
            Ok((_, Landing::Sent | Landing::Waiting { .. })) => moved += 1,
            Ok((_, other)) => {
                tracing::info!(
                    ticket = %pair.ticket_id, pr = %pair.pr_id, landing = ?other,
                    "a merged pull request's transition did not go"
                );
            }
            Err(error) => {
                tracing::info!(
                    ticket = %pair.ticket_id, pr = %pair.pr_id, %error,
                    "a merged pull request's ticket cannot be transitioned"
                );
            }
        }
    }
    Ok(moved)
}

/// The pairs a pass would act on right now.
///
/// Public so the negative case -- an unlinked merged pull request yields
/// nothing -- can be asserted without dispatching anything, which is the half
/// of this feature that matters most: it is what stops knobas touching tickets
/// the user never connected.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the query fails.
pub async fn due(pool: &PgPool, status: &str) -> Result<Vec<Merged>, IpcError> {
    sqlx::query_as::<_, Merged>(MERGED_AND_LINKED)
        .bind(KIND_PR)
        .bind(KIND_TICKET)
        .bind(status)
        .fetch_all(pool)
        .await
        .map_err(IpcError::internal)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The named path and the statement that reads it are one fact. Without
    /// this they are two strings that agree until somebody edits one.
    #[test]
    fn the_statement_reads_the_path_this_module_names() {
        assert!(
            MERGED_AND_LINKED.contains(MERGED_AND_LINKED_PATH),
            "the memory's `not exists` no longer reads {MERGED_AND_LINKED_PATH}, so it \
             matches nothing and every pass would transition the ticket again"
        );
    }

    /// The join is what makes story 18 structural: only a *link* can bring a
    /// ticket into this pass. A filter could be relaxed; a join cannot be
    /// without rewriting the statement, which is a diff a reviewer reads.
    #[test]
    fn only_a_link_can_bring_a_ticket_into_the_pass() {
        assert!(
            MERGED_AND_LINKED.contains("from knobas.confirmed_link"),
            "the pass must start from links knobas holds, never from the mirror at large"
        );
        assert!(
            !MERGED_AND_LINKED.contains("knobas.proposed_link"),
            "a proposal is not a link the user connected -- accepting it is"
        );
    }
}
