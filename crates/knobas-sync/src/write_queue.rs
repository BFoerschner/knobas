//! The write queue's flush loop -- **the one place knobas calls
//! `Source::write`** (issue #42).
//!
//! `knobas_core::write_queue` owns the rows; this owns what happens to them.
//! The split is forced: `knobas-source` depends on `knobas-core`, so the store
//! cannot see the SPI, and the flush loop has to live in a crate that sees
//! both. This is that crate.
//!
//! ## The choke point
//!
//! Application code never calls `Source::write`. It calls [`submit`], which
//! queues the write and then tries it; everything after that is this module's
//! business. A queue that can be bypassed is not a queue, so the rule is
//! enforced rather than documented: `tests/write_choke_point.rs` reads the
//! tree and fails if a second call site appears.
//!
//! ## What the loop guarantees
//!
//! * **Ordering is per entity, not global.** One entity's writes go in the
//!   order they were queued; a stalled write blocks its own successors and
//!   nothing else. Different entities and different sources never wait on one
//!   another. `knobas_core::write_queue::due` is where that lives -- it hands
//!   out the oldest pending write of each entity and no more.
//! * **Nothing is sent over a target that moved.** Immediately before each
//!   write, the target is re-read and re-projected; if it differs from the
//!   snapshot taken when the write was queued, the write is *held* instead.
//! * **A held write is terminal until the user acts.** No timeout, no
//!   auto-apply, no auto-discard. Nothing in this module could express one --
//!   [`due`](knobas_core::write_queue::due) does not return held rows, so a
//!   held write is invisible to the loop by construction rather than by a rule
//!   the loop has to remember.
//! * **A refusal is not a blip.** ADR-0004's structured status on
//!   `SourceError` is what separates a fault that will pass from one that will
//!   not; only the first is retried.
//! * **Every state change writes one activity line**, through
//!   `knobas_core::activity::record` -- the single activity writer.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use knobas_core::activity;
use knobas_core::entity::EntityRef;
use knobas_core::write_queue::{self as store, QueuedWrite, WaitReason};
use knobas_source::{Source, SourceError, WriteOp};

use crate::config;
use crate::scheduler::{RunFailure, SchedulerDeps};

/// Who the activity stream credits a queued write to.
///
/// `"user"`, not `"sync:<id>"`: a write is something a person did, and the
/// scheduler reads its own status-bar line back by `actor = 'sync:<id>'`
/// (`emit_latest_activity`), so borrowing that actor would have a flushed
/// comment impersonate the source's last sync.
const ACTOR: &str = "user";

/// Why a flush could not even begin.
///
/// Deliberately **not** a new `SyncError` variant: `From<SyncError> for
/// IpcError` lives in `crates/knobas-app/src/error.rs`, which §10.8 freezes,
/// and a new arm there would make this a frozen-surface change for no gain.
/// Every variant here decomposes into a type that already has a mapping.
#[derive(Debug, thiserror::Error)]
pub enum FlushError {
    #[error("source {0:?} is not configured")]
    NotConfigured(String),
    /// The queue's own store failed.
    #[error(transparent)]
    Store(#[from] knobas_core::CoreError),
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
}

/// The entity a write op targets.
///
/// Public because the desktop shell needs it to hold an *edited* write to the
/// target the original named: an amendment that repointed a queued comment at
/// another ticket would inherit that ticket's place in the queue and the
/// snapshot taken of the first one.
///
/// **No wildcard arm, deliberately** -- the same device, for the same reason,
/// as `WriteOp::identifier`. `WriteOp` grows per milestone (ADR-0006), and a
/// wildcard here would let a new variant reach the queue with no target: it
/// would be ordered against nothing and hold against nothing, silently. The
/// compiler is the reminder.
#[must_use]
pub fn target_entity(op: &WriteOp) -> &str {
    match op {
        WriteOp::Comment { entity, .. } => entity,
    }
}

/// Whether a fault will pass on its own, and under which name it waits.
///
/// This is ADR-0004's distinction put to work, and it is the difference
/// between an interruption and a data loss:
///
/// * `Unauthorized` -- the credential was refused, or there is none. It passes
///   when a human re-enters one, so the write waits rather than dying.
/// * `Unreachable` -- the server did not answer. It passes on its own.
/// * `Protocol` **with a server-fault status** (408 timeout, 429 rate limit,
///   5xx) -- the source is having a moment, not refusing the operation.
///   `knobas-http` has already retried these, so reaching here means the
///   moment is lasting; waiting is still right.
/// * anything else -- the source refused *this write*. A 400, a 404, a 422, or
///   a fault knobas raised itself. Retrying it forever would be pretending a
///   permanent answer is a temporary one, which is what story 19 forbids.
fn retryable(error: &SourceError) -> Option<WaitReason> {
    match error {
        SourceError::Unauthorized { .. } => Some(WaitReason::Unauthorized),
        SourceError::Unreachable(_) => Some(WaitReason::Unreachable),
        SourceError::Protocol {
            status: Some(status),
            ..
        } if *status == 408 || *status == 429 || *status >= 500 => Some(WaitReason::Unreachable),
        SourceError::Protocol { .. } | SourceError::Sink(_) => None,
    }
}

/// Queue an outbound write, then try to deliver it at once.
///
/// **The one way to ask knobas to write to a source.** Even a write that will
/// obviously succeed is queued first: one path means the queue is always the
/// record of what was asked for, and a write that lands is simply one that
/// settles milliseconds later. A "send it directly if the source is up" fast
/// path would be a second write path, which ADR-0006 is explicit there is not.
///
/// The returned row is the write **as queued**, before the attempt -- read it
/// back with `knobas_core::write_queue::get` for the outcome. That is the
/// honest shape: the caller asked to queue a write and that is what happened,
/// and a UI that reported "sent" from this return value would be reporting a
/// hope.
///
/// # Errors
///
/// [`FlushError::Store`] or [`FlushError::Db`] if the queue's own writes fail.
/// A source that cannot take the write is **not** an error here -- that is the
/// entire point.
pub async fn submit(
    deps: &SchedulerDeps,
    source_id: &str,
    op: WriteOp,
) -> Result<QueuedWrite, FlushError> {
    let entity = EntityRef::parse(target_entity(&op)).map_err(|e| {
        // An op whose target is not an entity id cannot be ordered or held
        // against anything. It is knobas' own bug, not a source fault.
        FlushError::NotConfigured(e.to_string())
    })?;
    let identifier = op.identifier().to_owned();
    let target = store::target_of(&deps.pool, &entity).await?;
    let snapshot = store::project(&identifier, target.as_ref());
    let payload =
        serde_json::to_value(&op).map_err(|e| FlushError::NotConfigured(e.to_string()))?;

    let queued = store::queue(
        &deps.pool,
        source_id,
        &entity,
        &identifier,
        payload,
        snapshot,
    )
    .await?;
    announce(deps, "queued", &queued).await;

    // Try it now. A failure here is the queue working, not the call failing.
    flush_source(deps, source_id).await?;
    Ok(queued)
}

/// Flush every source that owes something.
///
/// One source's failure never stops another's: each is flushed on its own and
/// its errors are logged rather than raised, which is what story 21 asks for
/// -- one broken credential must not stop everything. Sequential rather than
/// concurrent, deliberately: the queue is small, and each source's flush takes
/// its own lock anyway (see [`source_lock`]).
pub async fn flush_all(deps: &SchedulerDeps) {
    let sources = match config::list(&deps.pool).await {
        Ok(sources) => sources,
        Err(error) => {
            tracing::warn!(%error, "the write queue could not list sources");
            return;
        }
    };
    for source in sources {
        if let Err(error) = flush_source(deps, &source.id).await {
            tracing::warn!(source = %source.id, %error, "write queue flush failed");
        }
    }
}

/// One flush of a source at a time, process-wide.
///
/// Two flushers that both read [`due`](store::due) see the *same* head write
/// and both call `Source::write`. The `where state = 'pending'` guard on
/// [`sent`](store::sent) stops the second from settling the row -- it does not
/// stop the second from posting the comment. A queue whose job is not to lose
/// an edit must not post it twice either, and the two flushers that actually
/// overlap are ordinary: [`submit`] flushes immediately, and the scheduler's
/// tick flushes every five seconds.
///
/// In-process rather than a database lock, deliberately. `pg_advisory_xact_lock`
/// would mean holding a transaction open across the adapter's HTTP call, which
/// is the shape §10.6(c) had the sync engine move *away* from; a session-level
/// `pg_advisory_lock` would be released only by an explicit unlock, so any
/// early return would hand a pooled connection back with the lock still held
/// and poison it for whoever got it next. A process-wide map has neither
/// failure mode, and there is one scheduler per profile in one process --
/// which `config::due` being whole-database already assumes.
///
/// Per source, not global: two sources must still flush independently
/// (story 21).
fn source_lock(source_id: &str) -> Arc<tokio::sync::Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> = OnceLock::new();
    let locks = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut locks = locks
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    Arc::clone(
        locks
            .entry(source_id.to_owned())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
    )
}

/// Flush what one source is owed.
///
/// Loops while it makes progress: `due` hands out one write per entity, so a
/// second pass is what lets an entity's *next* write follow the one that just
/// landed. It stops the moment a pass delivers nothing, which is what keeps a
/// stuck queue from spinning.
///
/// # Errors
///
/// [`FlushError::Db`] or [`FlushError::Store`] if the queue's own reads or
/// writes fail. A source that cannot be built -- no configuration, no
/// credential -- is **not** an error: its writes wait, with the reason
/// recorded, which is the whole point of the queue.
pub async fn flush_source(deps: &SchedulerDeps, source_id: &str) -> Result<(), FlushError> {
    let lock = source_lock(source_id);
    let _flushing = lock.lock().await;

    if store::due(&deps.pool, source_id).await?.is_empty() {
        // Nothing owed: do not build an adapter or touch the keychain.
        return Ok(());
    }

    let source = match build(deps, source_id).await {
        Ok(source) => source,
        Err(reason) => {
            // The adapter could not be built at all. Every write this source
            // owes waits for the same reason, rather than one of them failing
            // and the rest silently not being tried.
            for write in store::due(&deps.pool, source_id).await? {
                waited(deps, &write, reason.0, &reason.1).await?;
            }
            return Ok(());
        }
    };

    loop {
        let due = store::due(&deps.pool, source_id).await?;
        if due.is_empty() {
            return Ok(());
        }
        let mut progressed = false;
        for write in due {
            if attempt(deps, source.as_ref(), &write).await? {
                progressed = true;
            }
        }
        if !progressed {
            return Ok(());
        }
    }
}

/// *I know, and I still mean it* (story 13): release a held write and try it.
///
/// The version the user was shown becomes the one the write is measured
/// against, so this flush sends it rather than holding it on the same change
/// again. A change arriving *after* they looked holds it again -- what they
/// consented to overwrite is what they saw.
///
/// `None` if the write was not held.
///
/// # Errors
///
/// As [`flush_source`].
pub async fn apply_anyway(
    deps: &SchedulerDeps,
    id: i64,
) -> Result<Option<QueuedWrite>, FlushError> {
    let Some(released) = store::apply_anyway(&deps.pool, id).await? else {
        return Ok(None);
    };
    announce(deps, "released", &released).await;
    flush_source(deps, &released.source_id).await?;
    Ok(Some(released))
}

/// *Edit and send* (story 15): replace a held or refused write's payload with
/// one the user has just written, and try it.
///
/// The snapshot is retaken here rather than reusing what the user was shown:
/// they have edited in response to the change, so the target as it stands now
/// is what they are answering.
///
/// `None` if the write has already settled.
///
/// # Errors
///
/// As [`flush_source`].
pub async fn amend(
    deps: &SchedulerDeps,
    id: i64,
    op: WriteOp,
) -> Result<Option<QueuedWrite>, FlushError> {
    let entity = EntityRef::parse(target_entity(&op))
        .map_err(|e| FlushError::NotConfigured(e.to_string()))?;
    let target = store::target_of(&deps.pool, &entity).await?;
    let snapshot = store::project(op.identifier(), target.as_ref());
    let payload =
        serde_json::to_value(&op).map_err(|e| FlushError::NotConfigured(e.to_string()))?;

    let Some(amended) = store::amend(&deps.pool, id, payload, snapshot).await? else {
        return Ok(None);
    };
    announce(deps, "amended", &amended).await;
    flush_source(deps, &amended.source_id).await?;
    Ok(Some(amended))
}

/// Withdraw a write -- cancelling a pending one (story 7) or conceding a held
/// one (story 14).
///
/// `None` if there was nothing open left to withdraw.
///
/// # Errors
///
/// [`FlushError::Store`] if the statement fails.
pub async fn discard(deps: &SchedulerDeps, id: i64) -> Result<Option<QueuedWrite>, FlushError> {
    let Some(discarded) = store::discard(&deps.pool, id).await? else {
        return Ok(None);
    };
    announce(deps, "discarded", &discarded).await;
    Ok(Some(discarded))
}

/// Attempt one write. `Ok(true)` if it was delivered, so the caller knows the
/// entity's queue moved and its next write may be tried.
///
/// The order inside is load-bearing: **hold detection comes first**, so a
/// write over a moved target never reaches the network at all.
async fn attempt(
    deps: &SchedulerDeps,
    source: &dyn Source,
    write: &QueuedWrite,
) -> Result<bool, FlushError> {
    let entity = match EntityRef::parse(&write.entity_id) {
        Ok(entity) => entity,
        Err(error) => {
            // Unqueueable rather than undeliverable: nothing will make this
            // id parse, so it is a refusal rather than a wait.
            refused(deps, write, &error.to_string()).await?;
            return Ok(false);
        }
    };

    // Hold detection: the target as it stands *now*, against the snapshot
    // taken when the write was queued.
    let current = store::project(
        &write.op,
        store::target_of(&deps.pool, &entity).await?.as_ref(),
    );
    if current != write.target_snapshot {
        if let Some(held) = store::hold(&deps.pool, write.id, current).await? {
            announce(deps, "held", &held).await;
        }
        return Ok(false);
    }

    let op: WriteOp = match serde_json::from_value(write.payload.clone()) {
        Ok(op) => op,
        Err(error) => {
            refused(deps, write, &format!("undecodable write: {error}")).await?;
            return Ok(false);
        }
    };

    // ---------------------------------------------------------------------
    // The one call to `Source::write` in knobas. Everything above decides
    // whether it may happen; `tests/write_choke_point.rs` is what keeps this
    // the only place it does.
    // ---------------------------------------------------------------------
    match source.write(op).await {
        Ok(()) => {
            if let Some(sent) = store::sent(&deps.pool, write.id).await? {
                announce(deps, "sent", &sent).await;
                return Ok(true);
            }
            // The row settled under us -- the user discarded it while it was
            // in flight. The write landed; there is nothing left to record
            // against a row that no longer expects it.
            Ok(false)
        }
        Err(error) => {
            match retryable(&error) {
                Some(reason) => waited(deps, write, reason, &error.to_string()).await?,
                None => refused(deps, write, &error.to_string()).await?,
            }
            Ok(false)
        }
    }
}

/// Build the adapter for a source, or say why every write it owes must wait.
///
/// A missing configuration or a missing credential is *not* a lost write: the
/// user re-enters the credential and the queue drains. Both therefore come
/// back as a [`WaitReason`] rather than as an error.
async fn build(
    deps: &SchedulerDeps,
    source_id: &str,
) -> Result<Box<dyn Source>, (WaitReason, String)> {
    let cfg = match config::get(&deps.pool, source_id).await {
        Ok(Some(cfg)) => cfg,
        Ok(None) => {
            return Err((
                WaitReason::Unauthorized,
                format!("no source with id {source_id:?}"),
            ));
        }
        Err(error) => return Err((WaitReason::Unreachable, error.to_string())),
    };
    crate::scheduler::build_source(deps, &cfg)
        .await
        .map_err(|failure| {
            // Only a database fault is worth calling "the server did not
            // answer". Everything else here -- no credential, a configuration
            // the adapter refuses, a keychain that would not open -- is
            // something a person has to act on, which is what `Unauthorized`
            // means to the shell.
            let reason = match &failure {
                RunFailure::Db(_) => WaitReason::Unreachable,
                RunFailure::Source(error) => retryable(error).unwrap_or(WaitReason::Unauthorized),
                _ => WaitReason::Unauthorized,
            };
            (reason, failure.message())
        })
}

/// One activity line per state change (story 20), through the single activity
/// writer.
///
/// A failure is logged, never raised: the queue's own record is the row, and
/// losing the narration must not turn a delivered write into a failed one --
/// the same treatment `run_inner` gives a run's activity line.
async fn announce(deps: &SchedulerDeps, verb: &str, write: &QueuedWrite) {
    let entity = match EntityRef::parse(&write.entity_id) {
        Ok(entity) => entity,
        Err(_) => return,
    };
    let detail = serde_json::json!({
        "write_id": write.id,
        "source_id": write.source_id,
        "op": write.op,
        "state": write.state.as_str(),
        "reason": write.wait_reason.map(|r| r.as_str()),
        "detail": write.detail,
    });
    match activity::record(&deps.pool, ACTOR, verb, Some(&entity), detail).await {
        Ok(row) => deps.events.activity_new(row),
        Err(error) => tracing::warn!(%error, verb, "the write queue's activity line failed"),
    }
}

/// The write could not be delivered, but the fault will pass.
async fn waited(
    deps: &SchedulerDeps,
    write: &QueuedWrite,
    reason: WaitReason,
    detail: &str,
) -> Result<(), FlushError> {
    if let Some(waiting) = store::wait(&deps.pool, write.id, reason, Some(detail)).await? {
        announce(deps, "waiting", &waiting).await;
    }
    Ok(())
}

/// The source refused the write. It stops being offered.
async fn refused(
    deps: &SchedulerDeps,
    write: &QueuedWrite,
    detail: &str,
) -> Result<(), FlushError> {
    if let Some(refused) = store::refuse(&deps.pool, write.id, detail).await? {
        announce(deps, "refused", &refused).await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ADR-0004's distinction is the one this feature rests on, so it is
    /// pinned rather than left to the reader of a `match`: a credential and a
    /// silent server wait, and a refusal does not.
    #[test]
    fn a_refusal_and_a_blip_are_told_apart_by_status() {
        for (error, expected) in [
            (
                SourceError::Unauthorized { status: Some(401) },
                Some(WaitReason::Unauthorized),
            ),
            (
                SourceError::Unauthorized { status: Some(403) },
                Some(WaitReason::Unauthorized),
            ),
            (SourceError::unauthorized(), Some(WaitReason::Unauthorized)),
            (
                SourceError::Unreachable("timed out".to_owned()),
                Some(WaitReason::Unreachable),
            ),
            (
                SourceError::Protocol {
                    status: Some(429),
                    message: "slow down".to_owned(),
                },
                Some(WaitReason::Unreachable),
            ),
            (
                SourceError::Protocol {
                    status: Some(503),
                    message: "maintenance".to_owned(),
                },
                Some(WaitReason::Unreachable),
            ),
            (
                SourceError::Protocol {
                    status: Some(400),
                    message: "no comments on this issue type".to_owned(),
                },
                None,
            ),
            (
                SourceError::Protocol {
                    status: Some(404),
                    message: "gone".to_owned(),
                },
                None,
            ),
            (
                SourceError::protocol("this adapter does not accept comment"),
                None,
            ),
        ] {
            assert_eq!(retryable(&error), expected, "{error:?}");
        }
    }
}
