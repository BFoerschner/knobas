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
//! enforced rather than documented: `tests/it/write_choke_point.rs` reads the
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
use knobas_core::delivery::Projection;
use knobas_core::entity::EntityRef;
use knobas_core::payload::StatePath;
use knobas_core::write_queue::{self as store, QueuedWrite, WaitReason, WriteState};
use knobas_http::LocalQuota;
use knobas_source::{Source, SourceError, WriteOp, WriteReceipt};

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
///   `knobas-http` has already retried these for a write that may be sent
///   twice, so reaching here means the moment is lasting; waiting is still
///   right. A write sent once (`Request::no_retry`, #1122) was asked once,
///   and a `503` or `429` says the source turned it away, so it too waits.
/// * `Unconfirmed` -- a write sent once reached the source and was never
///   answered (#1125). [`attempt`] holds it before it asks here, unless its
///   delivery says a resend is harmless; for those, and for an adapter
///   [`build`] that
///   failed with it, it is `Unreachable`.
/// * `Protocol` **with no status, when knobas' own rate limit held the
///   request** before it was sent (`held_on`, #1729) -- nothing reached
///   the source, so nothing was refused. It passes on its own, like an
///   unreachable source. The error alone cannot say so: a status-less
///   `Protocol` is also a fault an adapter raised itself, so the mark comes
///   from [`knobas_http::watch_local_quota`] around the call, never from the
///   message.
/// * anything else -- the source refused *this write*. A 400, a 404, a 422, or
///   a fault knobas raised itself. Retrying it forever would be pretending a
///   permanent answer is a temporary one, which is what story 19 forbids.
///
/// `Moved` waits on neither: [`attempt`] holds the write before it asks here
/// (#822), and it is listed only so this match stays exhaustive.
fn retryable(error: &SourceError, held_on: Option<&LocalQuota>) -> Option<WaitReason> {
    match error {
        SourceError::Unauthorized { .. } => Some(WaitReason::Unauthorized),
        SourceError::Unreachable(_) | SourceError::Unconfirmed { .. } => {
            Some(WaitReason::Unreachable)
        }
        SourceError::Protocol {
            status: Some(status),
            ..
        } if *status == 408 || *status == 429 || *status >= 500 => Some(WaitReason::Unreachable),
        SourceError::Protocol { status: None, .. } if held_on.is_some() => {
            Some(WaitReason::Unreachable)
        }
        SourceError::Protocol { .. } | SourceError::Sink(_) | SourceError::Moved { .. } => None,
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
/// The try reaches this write's entity, in queue order, and nothing else the
/// source owes ([`flush_entity_of`], #1797).
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
    let queued = queue(deps, source_id, op).await?;
    // Try it now. A failure here is the queue working, not the call failing.
    flush_entity(deps, &queued).await?;
    Ok(queued)
}

/// Queue an outbound write **without trying to deliver it**, and hand back the
/// row.
///
/// [`submit`] is this plus a flush, and is what almost everything wants. This
/// half exists on its own for one caller and one reason: **nothing may be sent
/// to a source before knobas' own record of it exists.**
///
/// The worklog is that caller (issue #280). It keeps a local copy of what it
/// sent, and the copy is also what marks the blocks it covers as spent -- so a
/// crash in the gap between the send and the copy would leave a worklog on the
/// ticket with no trace of it here and the same afternoon offered for logging
/// again. `submit` has that gap by construction; this half plus
/// [`flush_entity_of`] does not.
///
/// The pair is therefore: `queue`, write the local record against this row's
/// id, then [`flush_entity_of`]. A crash between them leaves a pending write and
/// a copy that says so -- which is the ordinary state of every worklog logged
/// while Jira is down, and the next flush finishes it.
///
/// **The id Jira answers with is a separate problem and is solved separately**
/// (`knobas_core::write_queue::sent`): the settle records it on the queue row
/// as well as on the copy, so a flush the scheduler happens to run first
/// cannot lose it.
///
/// **Not a second write path.** Nothing here reaches a source: the flush loop
/// is still the only thing that calls `Source::write`, and a row queued and
/// never flushed by its caller is picked up by the next flush of that source
/// like any other pending write.
///
/// # Errors
///
/// As [`submit`].
pub async fn queue(
    deps: &SchedulerDeps,
    source_id: &str,
    op: WriteOp,
) -> Result<QueuedWrite, FlushError> {
    let entity = EntityRef::parse(op.target()).map_err(|e| {
        // An op whose target is not an entity id cannot be ordered or held
        // against anything. It is knobas' own bug, not a source fault.
        FlushError::NotConfigured(e.to_string())
    })?;
    let payload =
        serde_json::to_value(&op).map_err(|e| FlushError::NotConfigured(e.to_string()))?;
    let snapshot = project_now(deps, &op, &entity, &payload).await?;

    let queued = store::queue(
        &deps.pool,
        source_id,
        &entity,
        op.identifier(),
        payload,
        snapshot,
    )
    .await?;
    announce(deps, "queued", &queued).await;
    Ok(queued)
}

/// Flush every source that owes something: the scheduler's flush task, on
/// every tick.
///
/// One source's failure never stops another's: each is flushed on its own and
/// its errors are logged rather than raised, which is what story 21 asks for
/// -- one broken credential must not stop everything. Sequential rather than
/// concurrent, deliberately: the queue is small, and each source's writes take
/// its own lock anyway (see [`gate`]). The scheduler runs it on a task of its
/// own, so however long it takes, nobody's sync waits.
///
/// A source whose health needs a human -- the credential was refused, or
/// there is none -- is skipped whole, the test `Scheduler::trigger_all` makes
/// before a sync (#1628). That is a cheap first filter, not the rule: a Kuma
/// write can be refused while its sync is healthy (the account is a second
/// credential), so what keeps a refused write from going again is
/// [`owed`], read per row.
///
/// It stops between writes once the quit has begun (#1629, spec #1618 ruling
/// A7), as every flush does ([`Quit`](crate::scheduler::Quit), #1924).
/// Between writes and not inside one. A write already on the wire is let
/// finish, so its `sent` lands; `Scheduler::shutdown`'s grace is what bounds
/// it, and its abort is what leaves that one request at risk, the window
/// ADR-0012 discloses. No later write is started.
pub async fn flush_all(deps: &SchedulerDeps) {
    flush_every(deps, Pass::Owed).await;
}

/// *Flush now* from the queue panel (#1628): one source, or every source when
/// `source_id` is `None`, sending each head write **once, whatever it is
/// waiting on** -- a refused credential, the ladder after an unreachable
/// server. It is a person saying "try it", so nothing is skipped for the
/// source's health either.
///
/// Once per press: only the first pass ignores the waits. A write that fails
/// again is recorded as waiting and is not sent again by the passes that
/// follow, nor by the tick.
///
/// # Errors
///
/// As [`flush_source`], for one source. Every source's errors are logged, as
/// [`flush_all`]'s are.
pub async fn flush_now(deps: &SchedulerDeps, source_id: Option<&str>) -> Result<(), FlushError> {
    match source_id {
        Some(id) => flush(deps, id, Pass::Now, Reach::Source).await,
        None => {
            flush_every(deps, Pass::Now).await;
            Ok(())
        }
    }
}

/// Which head writes a pass sends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pass {
    /// Those [`owed`] says are due: the tick, a submit, a release.
    Owed,
    /// Every one, once: *Flush now*.
    Now,
}

/// Which of a source's head writes a flush looks at.
#[derive(Clone, Copy, Debug)]
enum Reach<'w> {
    /// Every entity's: the flush task, *Flush now*, [`flush_source`].
    Source,
    /// One entity's, the entity of the write a submit, an amend or *Send
    /// mine anyway* is for ([`flush_entity_of`], #1797).
    ///
    /// The source takes one write at a time, and the flush task holds it for
    /// one write, not a pass (#1629). A flush that reached the whole source
    /// would take turns with the task through every write it owes, which on
    /// an unreachable source is one send budget each, before reaching the
    /// write a person just made; this one waits for the send on the wire and
    /// then makes its own. The task sends the rest on its next tick. The
    /// entity's writes go in queue order, as `due` hands them out: those
    /// queued before this one first, and those after it once it lands, as
    /// a released head lets its successors follow.
    Entity(&'w str),
}

async fn flush_every(deps: &SchedulerDeps, pass: Pass) {
    let sources = match config::list(&deps.pool).await {
        Ok(sources) => sources,
        Err(error) => {
            tracing::warn!(%error, "the write queue could not list sources");
            return;
        }
    };
    for source in sources {
        if deps.quit.has_begun() {
            return;
        }
        if pass == Pass::Owed && source.health.state.needs_human() {
            continue;
        }
        if let Err(error) = flush(deps, &source.id, pass, Reach::Source).await {
            tracing::warn!(source = %source.id, %error, "write queue flush failed");
        }
    }
}

/// Whether a pending head write is due at `now`, or is waiting something out
/// (#1628, spec #1618 ruling A2).
///
/// * Never tried, or released since -- due.
/// * `unauthorized` -- **never** due on a clock. A refused credential stays
///   refused; only a person acting on the source ([`release_unauthorized`])
///   or *Flush now* sends it
///   again. This is `docs/development.md`'s rule for syncs, "a 401 never
///   retries", for the same reason: the lockout.
/// * `unreachable` -- due once the sync side's backoff ladder
///   ([`config::backoff_after`]) has passed since `attempted_at`, with the
///   row's `attempts` as the rung. That includes a write whose answer was
///   lost and whose resend is harmless (Kuma's pause and resume, #1627),
///   each attempt of which may spend a minute.
fn owed(write: &QueuedWrite, now: chrono::DateTime<chrono::Utc>) -> bool {
    match write.wait_reason {
        None => true,
        Some(WaitReason::Unauthorized) => false,
        Some(WaitReason::Unreachable) => write.attempted_at.is_none_or(|at| {
            let rung = config::backoff_after(i64::from(write.attempts));
            chrono::Duration::from_std(rung).is_ok_and(|rung| now - at >= rung)
        }),
    }
}

/// The head writes of `source_id` this pass sends.
async fn heads(
    deps: &SchedulerDeps,
    source_id: &str,
    pass: Pass,
    reach: Reach<'_>,
) -> Result<Vec<QueuedWrite>, FlushError> {
    let mut due = store::due(&deps.pool, source_id).await?;
    if let Reach::Entity(entity) = reach {
        due.retain(|write| write.entity_id == entity);
    }
    if pass == Pass::Owed {
        let now = chrono::Utc::now();
        due.retain(|write| owed(write, now));
    }
    Ok(due)
}

/// A person acted on what `source_id`'s writes wait for -- a new secret or,
/// for Kuma, the account beside it; a new address or configuration -- so its
/// writes waiting `unauthorized` are owed again, and the next flush sends them
/// (#1628).
///
/// Also what tells a flush already under way that a refusal it is about to
/// record is stale: the 401 it holds was the answer to the credential just
/// replaced, so the write is left released rather than re-reasoned
/// `unauthorized` -- which would have it wait for a change that has already
/// happened. The two serialise on the source's count of releases (see
/// [`gate`]); a flush never waits for this, nor this for a flush.
///
/// # Errors
///
/// [`FlushError::Store`] if the release fails.
pub async fn release_unauthorized(
    pool: &sqlx::PgPool,
    source_id: &str,
) -> Result<Vec<QueuedWrite>, FlushError> {
    let gate = gate(source_id);
    let mut releases = gate.releases.lock().await;
    *releases += 1;
    Ok(store::release_unauthorized(pool, source_id).await?)
}

/// One write of a source at a time, process-wide.
///
/// Two flushers that both read [`due`](store::due) see the *same* head write
/// and both call `Source::write`. The `where state = 'pending'` guard on
/// [`sent`](store::sent) stops the second from settling the row -- it does not
/// stop the second from posting the comment. A queue whose job is not to lose
/// an edit must not post it twice either, and the two flushers that actually
/// overlap are ordinary: [`submit`] flushes immediately, and the scheduler's
/// flush task runs on every tick.
///
/// **Held per write, not per pass** (#1629): a submit waits for the write on
/// the wire, not for every write the tick's pass still owes, which on an
/// unreachable source is one send budget each. So whoever takes the lock
/// re-reads the row first ([`turn`]): a write another flusher attempted, or
/// that settled, since this one read it is not sent again.
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
///
/// The gate also counts the source's releases (#1628), which
/// [`release_unauthorized`] bumps and [`waited`] reads before it records a
/// refusal. A lock of its own rather than the flush lock: a credential saved
/// while a flush is out must not wait for that flush's HTTP calls.
///
/// Process-wide per source id, and the id alone: not per database or per
/// pool. Production has one database and unique source ids, so that is
/// right there. In tests every test in a binary shares the map, so tests
/// that flush or release the same id share one release count, and one
/// test's credential save makes another's 401 look stale (#1752, #1769).
/// Give each test its own source id.
fn gate(source_id: &str) -> Arc<Gate> {
    static GATES: OnceLock<Mutex<HashMap<String, Arc<Gate>>>> = OnceLock::new();
    let gates = GATES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut gates = gates
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    Arc::clone(gates.entry(source_id.to_owned()).or_default())
}

/// One source's share of the flush loop's process-wide state; see [`gate`].
#[derive(Default)]
struct Gate {
    /// Held across one turn at a write ([`turn`]): the re-read, then the
    /// attempt and its settle, or the wait a failed build records.
    flush: tokio::sync::Mutex<()>,
    /// How many times [`release_unauthorized`] has run for the source in this
    /// process.
    releases: tokio::sync::Mutex<u64>,
}

/// The credential a flush built its adapter with, named by the source's
/// count of releases when the flush read it.
struct CredentialSeen<'a> {
    gate: &'a Gate,
    releases: u64,
}

/// Flush what one source is owed: every head write [`owed`] says is due.
/// The flush task's reach; a submit's is [`flush_entity_of`]'s, one entity.
///
/// Loops while it makes progress: `due` hands out one write per entity, so a
/// second pass is what lets an entity's *next* write follow the one that just
/// landed. It stops the moment a pass delivers nothing, which is what keeps a
/// stuck queue from spinning; and a write that failed in one pass is not owed
/// in the next, because its wait was just recorded. A head another flusher
/// landed, or the reader withdrew, while this one waited for the lock gives
/// its turn to the entity's next write ([`turn`]), so that write is this
/// flusher's to send.
///
/// # Errors
///
/// [`FlushError::Db`] or [`FlushError::Store`] if the queue's own reads or
/// writes fail. A source that cannot be built -- no configuration, no
/// credential -- is **not** an error: its writes wait, with the reason
/// recorded, which is the whole point of the queue.
pub async fn flush_source(deps: &SchedulerDeps, source_id: &str) -> Result<(), FlushError> {
    flush(deps, source_id, Pass::Owed, Reach::Source).await
}

/// Flush the entity write `id` is for: its head writes, as [`flush_source`]
/// would, in queue order and as far as they go, and nothing else its source
/// owes (#1797).
///
/// What a submit, an amend and *Send mine anyway* run, and what a caller
/// that [`queue`]d a write sends it with. It waits for the source's write
/// on the wire, if the flush task has one out, and not for the writes the
/// task owes after it; those go on the task's next tick. Nothing happens if
/// no write carries `id`.
///
/// # Errors
///
/// As [`flush_source`].
pub async fn flush_entity_of(deps: &SchedulerDeps, id: i64) -> Result<(), FlushError> {
    match store::get(&deps.pool, id).await? {
        Some(write) => flush_entity(deps, &write).await,
        None => Ok(()),
    }
}

/// [`flush_entity_of`] with the row in hand.
async fn flush_entity(deps: &SchedulerDeps, write: &QueuedWrite) -> Result<(), FlushError> {
    let reach = Reach::Entity(&write.entity_id);
    flush(deps, &write.source_id, Pass::Owed, reach).await
}

/// Every flush ends here, and every one stops between writes once the quit
/// has begun ([`turn`] selects against [`SchedulerDeps::quit`], #1924): no
/// caller hands it a stop, so none can hand it one that never fires.
async fn flush(
    deps: &SchedulerDeps,
    source_id: &str,
    pass: Pass,
    reach: Reach<'_>,
) -> Result<(), FlushError> {
    let gate = gate(source_id);

    let mut due = heads(deps, source_id, pass, reach).await?;
    if due.is_empty() {
        // Nothing owed: do not build an adapter or touch the keychain. A
        // source whose every write waits on a refused credential stops here.
        return Ok(());
    }

    // Read before the build, which reads the keychain: a change after this
    // may not be in the adapter.
    let seen = CredentialSeen {
        gate: &gate,
        releases: *gate.releases.lock().await,
    };
    let source = match build(deps, source_id).await {
        Ok(source) => source,
        Err(reason) => {
            // The adapter could not be built at all. Every write this source
            // owes waits for the same reason, rather than one of them failing
            // and the rest silently not being tried.
            for write in due {
                match turn(deps, &gate, &write, pass).await? {
                    Turn::Go(_sending, write) => {
                        waited(deps, &seen, &write, reason.0, &reason.1).await?;
                    }
                    Turn::Gone => {}
                    Turn::Stopped => return Ok(()),
                }
            }
            return Ok(());
        }
    };

    loop {
        let mut progressed = false;
        for write in due {
            match turn(deps, &gate, &write, pass).await? {
                Turn::Go(_sending, write) => {
                    if attempt(deps, &seen, source.as_ref(), &write).await? {
                        progressed = true;
                    }
                }
                Turn::Gone => {}
                Turn::Stopped => return Ok(()),
            }
        }
        if !progressed {
            return Ok(());
        }
        // Only the first pass of a *Flush now* sends what is waiting: the
        // writes it just failed are not owed now, so this is not a second
        // send of anything.
        due = heads(deps, source_id, Pass::Owed, reach).await?;
        if due.is_empty() {
            return Ok(());
        }
    }
}

/// What a flusher finds when its turn at one write comes ([`turn`]).
enum Turn<'g> {
    /// The row to send, as it stands now: the write this flusher read, still
    /// untouched and owed, or, if that one closed, its entity's next head.
    /// Send it, holding the source's lock until it has settled.
    /// Boxed: a queue row is far larger than the other two arms.
    Go(tokio::sync::MutexGuard<'g, ()>, Box<QueuedWrite>),
    /// Nothing to send this turn: the write is held or refused, another
    /// flusher attempted it since it was read, or it is waiting something out
    /// ([`owed`]); or it settled or was withdrawn and its entity has no
    /// pending head after it that this pass sends.
    Gone,
    /// The quit came first: start nothing more.
    Stopped,
}

/// Take the source's lock for one write -- or the quit, whichever comes
/// first -- and re-read the write under it (#1629).
///
/// The row as it stands now is what is sent, not the copy the pass read: an
/// amend that landed in between is the version the reader means. It is
/// [`Turn::Gone`] if it is held or refused, if its `attempted_at` or
/// `attempts` moved (every attempt bumps both, under this lock), or, on a
/// pass of the tick's kind, if it is no longer [`owed`].
///
/// A write another flusher landed, or the reader withdrew, while this one
/// waited hands its turn to its entity's next write, re-read as `heads`
/// reads it (#1807). Without that, a submit whose entity's earlier write the
/// flush task was sending would make no progress and return untried, and its
/// own write would wait for the rest of the task's pass on the source. A
/// write another flusher attempted and failed is still no progress: it is
/// still the entity's head, and the attempt this flusher meant was just made.
async fn turn<'g>(
    deps: &SchedulerDeps,
    gate: &'g Gate,
    read: &QueuedWrite,
    pass: Pass,
) -> Result<Turn<'g>, FlushError> {
    let sending = tokio::select! {
        biased;
        () = deps.quit.token().cancelled() => return Ok(Turn::Stopped),
        sending = gate.flush.lock() => sending,
    };
    let Some(now) = store::get(&deps.pool, read.id).await? else {
        return Ok(Turn::Gone);
    };
    if !now.state.is_open() {
        // Another flusher landed it, or the reader withdrew it, while this
        // one waited: the entity's next write is its head now, and this turn
        // is that write's (#1807). Read under the lock, so as it stands; one
        // entity's, so never another's; the head, so in queue order.
        let next = heads(deps, &read.source_id, pass, Reach::Entity(&read.entity_id)).await?;
        return Ok(match next.into_iter().next() {
            Some(next) => Turn::Go(sending, Box::new(next)),
            None => Turn::Gone,
        });
    }
    let untouched = now.state == WriteState::Pending
        && now.attempted_at == read.attempted_at
        && now.attempts == read.attempts;
    if !untouched || (pass == Pass::Owed && !owed(&now, chrono::Utc::now())) {
        return Ok(Turn::Gone);
    }
    Ok(Turn::Go(sending, Box::new(now)))
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
    flush_entity(deps, &released).await?;
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
    let entity =
        EntityRef::parse(op.target()).map_err(|e| FlushError::NotConfigured(e.to_string()))?;
    let payload =
        serde_json::to_value(&op).map_err(|e| FlushError::NotConfigured(e.to_string()))?;
    let snapshot = project_now(deps, &op, &entity, &payload).await?;

    let Some(amended) = store::amend(&deps.pool, id, payload, snapshot).await? else {
        return Ok(None);
    };
    announce(deps, "amended", &amended).await;
    flush_entity(deps, &amended).await?;
    Ok(Some(amended))
}

/// Withdraw a write -- cancelling a pending one (story 7) or conceding a held
/// one (story 14).
///
/// A `log_work` write takes its local worklog copy with it and gives the
/// blocks it covered back, so the time returns to *unlogged* rather than
/// reading as held for good (#328). That happens in the one statement
/// `store::discard` is, where its own docs argue it; nothing here has to
/// remember to do it, which is the point.
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

/// The state paths the target's source declares for the target's kind
/// (`KindPaths::state`, #666), for the ops whose hold reads them
/// ([`Projection::reads_declared_state`]): an `edit_pull_request` that closes or
/// reopens holds on the declared state rather than the whole record (#706),
/// and a comment, an approval or a review holds on nothing else (#752).
/// Empty for every other op, for a target that is gone, and for a source
/// whose adapter is not compiled in or declares nothing.
///
/// The **template** descriptor, keyed by the entity's namespace (the source
/// instance id), as `knobas-app`'s `declared_paths` reads it: where a source
/// keeps its state is a property of the adapter kind, so this needs no
/// secret and no built adapter, and the queue and the flush read it alike.
async fn declared_state(
    deps: &SchedulerDeps,
    entity: &EntityRef,
    shape: Projection,
    target: Option<&store::Target>,
) -> Result<Vec<StatePath>, FlushError> {
    let Some(target) = target.filter(|_| shape.reads_declared_state()) else {
        return Ok(Vec::new());
    };
    let Some(source) = config::get(&deps.pool, &entity.namespace).await? else {
        return Ok(Vec::new());
    };
    Ok(deps
        .registry
        .descriptors()
        .into_iter()
        .filter(|d| d.adapter_kind == source.adapter_kind)
        .flat_map(|d| d.payload_paths)
        .find(|paths| paths.kind == target.kind)
        .map(|paths| paths.state)
        .unwrap_or_default())
}

/// The op's target as the mirror holds it now, projected by the shape its
/// delivery states: the snapshot a queued write is taken with and the value a
/// held one is compared against.
async fn project_now(
    deps: &SchedulerDeps,
    op: &WriteOp,
    entity: &EntityRef,
    payload: &serde_json::Value,
) -> Result<serde_json::Value, FlushError> {
    let shape = op.delivery().projection;
    let target = store::target_of(&deps.pool, entity).await?;
    let state = declared_state(deps, entity, shape, target.as_ref()).await?;
    Ok(store::project(
        op.identifier(),
        shape,
        payload,
        target.as_ref(),
        &state,
    ))
}

/// Attempt one write. `Ok(true)` if it was delivered, so the caller knows the
/// entity's queue moved and its next write may be tried.
///
/// The order inside is load-bearing: **hold detection comes first**, so a
/// write over a moved target never reaches the network at all.
///
/// It is the one decider of how a write is delivered, and it reads every
/// answer off [`WriteOp::delivery`] (ADR-0006, amended 2026-10-19): the
/// target it projects, the projection a hold compares, whether a send that
/// got no answer may go again, and, through [`unclaimed`], whether a
/// withdrawal in flight is disclosed.
///
/// # Resending a send that got no answer (#1125)
///
/// [`Delivery::resend_harmless`](knobas_core::delivery::Delivery::resend_harmless) is `true`
/// where a second arrival changes nothing or is refused by the source,
/// which is the #1122 audit's "retried" column plus Kuma's pause and resume
/// (setting a state that is already set). Such a write answered
/// [`SourceError::Unconfirmed`] waits as `Unreachable` and goes on the ladder
/// ([`owed`]). `false` where the source would make a second one: a second comment,
/// ticket, page, worklog, monitor, approval, review or build. Those are held,
/// and the reader checks the source, then sends again or discards.
///
/// A transition is `false`: Jira usually refuses the repeat, but not on a
/// workflow that loops back to its own status.
///
/// **No look-before-write is tried.** The queue holds no read that finds a
/// created thing by a marker the payload carries: Jira answers a create with
/// no id the queue keeps, a comment carries no knobas marker, and a title is
/// not an address (see [`unclaimed`]). Holding is the honest answer
/// until one exists.
async fn attempt(
    deps: &SchedulerDeps,
    seen: &CredentialSeen<'_>,
    source: &dyn Source,
    write: &QueuedWrite,
) -> Result<bool, FlushError> {
    // Decoded before hold detection, which reads the op's delivery: a row
    // nothing decodes is refused at once rather than held first.
    let op: WriteOp = match serde_json::from_value(write.payload.clone()) {
        Ok(op) => op,
        Err(error) => {
            refused(deps, write, &format!("undecodable write: {error}")).await?;
            return Ok(false);
        }
    };
    let delivery = op.delivery();
    let entity = match EntityRef::parse(op.target()) {
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
    let current = project_now(deps, &op, &entity, &write.payload).await?;
    if current != write.target_snapshot {
        if let Some(held) = store::hold(&deps.pool, write.id, current).await? {
            announce(deps, "held", &held).await;
        }
        return Ok(false);
    }

    // ---------------------------------------------------------------------
    // The one call to `Source::write` in knobas. Everything above decides
    // whether it may happen; `tests/it/write_choke_point.rs` is what keeps this
    // the only place it does.
    //
    // Delivery is at-least-once, on purpose (ADR-0012): if knobas dies
    // between this call returning and `sent` landing below, the row stays
    // pending and the next flush re-sends a write that already arrived. No
    // key rides on the call and no dedupe guards it -- the ADR records why,
    // and what would reopen the question.
    //
    // A send that never got an answer is a different window (#1125): the
    // source may have acted, and a write a second arrival would duplicate is
    // held for the reader rather than re-sent. See the `Unconfirmed` arm.
    //
    // Watched for knobas' own rate limit (#1729): a request it held past the
    // send budget fails like a refusal and was never sent. See `retryable`.
    // ---------------------------------------------------------------------
    let (written, held_on) = knobas_http::watch_local_quota(source.write(op)).await;
    match written {
        Ok(receipt) => {
            // The receipt, and the settle, in one statement: `sent` is where
            // a worklog's local copy learns the id Jira gave it (#280), and
            // this is the only moment that id exists. An adapter with nothing
            // to say answers `WriteReceipt::none()` and the settle is exactly
            // what it always was.
            if let Some(sent) =
                store::sent(&deps.pool, write.id, receipt.remote_id.as_deref()).await?
            {
                announce(deps, "sent", &sent).await;
                return Ok(true);
            }
            // The row settled under us -- the user withdrew it while it was
            // in flight. The write landed anyway, and this is the only moment
            // knobas can know that, so it says so rather than returning
            // quietly (issue #336).
            unclaimed(deps, write, delivery.can_leave_unclaimed, &receipt).await;
            Ok(false)
        }
        // The source says the target moved under the write (#822): a held
        // write, exactly as if the mirror had seen it, with what the source
        // holds now as the other half of "both versions side by side". The
        // reader's half is the payload. `store::apply_anyway` re-sends it
        // against `base`.
        Err(SourceError::Moved { base, current }) => {
            let theirs = store::moved(&write.op, write.target_snapshot.clone(), &base, &current);
            if let Some(held) = store::hold(&deps.pool, write.id, theirs).await? {
                announce(deps, "held", &held).await;
            }
            Ok(false)
        }
        // The write reached the source and no answer says whether it acted
        // (#1125). Sending it again on the next tick could make a second
        // ticket or comment, minutes after the first: the double-apply #1122
        // took out of the transport. So it is held, with the source to check
        // named, and *Send again* (`apply_anyway`) is the reader's.
        Err(error @ SourceError::Unconfirmed { .. }) if !delivery.resend_harmless => {
            let source = config::get(&deps.pool, &write.source_id)
                .await?
                .map_or_else(|| write.source_id.clone(), |cfg| cfg.display_name);
            let theirs = store::unconfirmed(
                &write.op,
                write.target_snapshot.clone(),
                &source,
                &error.to_string(),
            );
            if let Some(held) = store::hold(&deps.pool, write.id, theirs).await? {
                announce(deps, "held", &held).await;
            }
            Ok(false)
        }
        Err(error) => {
            match retryable(&error, held_on.as_ref()) {
                Some(reason) => waited(deps, seen, write, reason, &error.to_string()).await?,
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
                RunFailure::Source(error) => {
                    retryable(error, None).unwrap_or(WaitReason::Unauthorized)
                }
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
    announce_with(deps, verb, write, None).await;
}

/// [`announce`], plus the id the source gave what it made.
///
/// One named key rather than a bag the caller fills: the six fields below are
/// what every line this module writes answers, so a reader does not have to
/// know which verb they are looking at to find the write, and a merge wide
/// enough to add a seventh is wide enough to overwrite one of the six.
///
/// `None` writes no key at all rather than a `null` -- the distinction
/// [`unclaimed`] rests on, since a source that named nothing and a source
/// that named nothing *knowable* are the same silence.
async fn announce_with(
    deps: &SchedulerDeps,
    verb: &str,
    write: &QueuedWrite,
    remote_id: Option<&str>,
) {
    // The log line for every state change a write makes (sent, refused, held,
    // ...): the one place all of them pass, and before the parse below so an
    // id nothing can parse is still on disk. #747.
    tracing::info!(
        write_id = write.id,
        source_id = %write.source_id,
        op = %write.op,
        verb,
        detail = write.detail.as_deref().unwrap_or(""),
        "write queue"
    );
    let entity = match EntityRef::parse(&write.entity_id) {
        Ok(entity) => entity,
        Err(_) => return,
    };
    let mut detail = serde_json::json!({
        "write_id": write.id,
        "source_id": write.source_id,
        "op": write.op,
        "state": write.state.as_str(),
        "reason": write.wait_reason.map(|r| r.as_str()),
        "detail": write.detail,
    });
    if let (Some(remote_id), Some(object)) = (remote_id, detail.as_object_mut()) {
        object.insert("remote_id".to_owned(), remote_id.into());
    }
    match activity::record(&deps.pool, ACTOR, verb, Some(&entity), detail).await {
        Ok(row) => deps.events.activity_new(row),
        Err(error) => tracing::warn!(%error, verb, "the write queue's activity line failed"),
    }
}

/// The write could not be delivered, but the fault will pass.
///
/// A refusal is not recorded when the source was released since the adapter
/// was built (#1628): it answered the credential that was replaced, and the
/// write [`release_unauthorized`] released stays released for the next flush
/// to try with the new one. The count is held across the statement, so a
/// release lands wholly before it or wholly after.
async fn waited(
    deps: &SchedulerDeps,
    seen: &CredentialSeen<'_>,
    write: &QueuedWrite,
    reason: WaitReason,
    detail: &str,
) -> Result<(), FlushError> {
    let releases = seen.gate.releases.lock().await;
    if reason == WaitReason::Unauthorized && *releases != seen.releases {
        return Ok(());
    }
    let waiting = store::wait(&deps.pool, write.id, reason, Some(detail)).await?;
    drop(releases);
    if let Some(waiting) = waiting {
        // The row always records the attempt (`attempted_at`, `attempts`,
        // `detail`); the activity stream gets a line only when the *reason* is
        // news. Story 20 promises a line per change, and a source that stays
        // down is tried again on every rung of the ladder, so announcing each
        // attempt would bury the day's record under a line an hour, for ever,
        // while a source is gone.
        // `write` is the row before this attempt, so a first failure (no
        // reason yet) and a change of fault both still announce.
        if write.wait_reason != Some(reason) {
            announce(deps, "waiting", &waiting).await;
        }
    }
    Ok(())
}

/// The write landed after the user withdrew it, and what it made is still at
/// the source with nothing in knobas claiming it (issue #336).
///
/// # Why here and nowhere else
///
/// This is the only moment the fact exists. **At discard time knobas cannot
/// know it**: `knobas_core::write_queue::discard` says so in its own doc
/// comment -- the flush loop's per-source lock does not hold a discard back,
/// and every writer of `attempts` bumps it *after* the call, so a write in
/// flight is indistinguishable from one that was never tried. A dialog at that
/// end could only have offered a coin flip. Here the receipt is in hand and
/// [`store::sent`] has just come back empty, which together mean one thing and
/// not two: the source took the write, and the row it belonged to is no longer
/// open.
///
/// # What it may claim
///
/// The state is **read off the row** rather than inferred from the empty
/// settle. Withdrawal is the only transition a person can land in that window
/// today -- the loop is the only writer of `held` and `refused` and it holds
/// the source lock across the call -- but "the settle matched nothing" and
/// "the user withdrew it" are different statements, and only the second is
/// what this line says.
///
/// # Which ops
///
/// Those whose delivery says
/// [`can_leave_unclaimed`](knobas_core::delivery::Delivery::can_leave_unclaimed), which the
/// caller passes in as `can_leave`: `create_ticket`, `create_page`,
/// `create_monitor` and `source_action`. The op is checked before the row is
/// re-read, so an ordinary write that landed under a withdrawal costs nothing
/// and says nothing: it arrived, which is what was asked for.
///
/// The question each op has to answer is not "did the write land after the
/// user withdrew it" -- that is true of any op, and the branch in [`attempt`]
/// knows it without asking. It is narrower: *did the source make something new
/// that knobas cannot name?* A `comment` and an `update_page` change a thing
/// the mirror already holds, a `transition` and an `approve` change a field, a
/// build is not an artefact -- for all of those the write simply arrived,
/// which is what the user asked for. A ticket and a page make something that exists only
/// at the source until the next sync, with nothing linking it to what asked
/// for it, and ADR-0012 singles out the first as the op a duplicate files
/// twice.
///
/// **`create_branch` and `create_pull_request` are deliberately not here, and
/// the reason is the withdrawn row rather than the 409** (issue #353, which
/// made the decision #336 left open; #333's table had only the *re-send*
/// answer, which is a different question).
///
/// Both open something against a ref the caller named -- `create_branch`'s
/// `name`, `create_pull_request`'s `head` -- and the withdrawal keeps it:
/// `knobas_core::write_queue::discard` keeps the row *with its payload*, "so
/// what was withdrawn is still answerable". That ref is an address and not
/// merely a label. A branch's name **is** its identity at the source, and
/// Gitea holds at most one open pull request per `head`/`base` pair (`an open
/// pull request already exists for this pair` is among the refusals
/// `knobas_source_gitea::write::create_pull_request` documents). So the next
/// sync mirrors an artefact the withdrawn row can still be matched to -- and
/// knobas already owns both reads and already depends on them:
/// `knobas_app::start_work::queue`'s `BRANCH_BY_NAME` and
/// `PULL_REQUEST_BY_HEAD` are how the start-work flow looks before it writes,
/// and how its link step finds a pull request at all. Retrying a
/// `create_pull_request` step whose write was withdrawn in flight reads
/// *every* pull request the mirror holds from that head, settles the step
/// "already open" against the one it read as open, and the link step then
/// draws the edge back to the ticket that asked for it. The Gitea guarantee
/// above is what makes the head an address at all, and what it promises is
/// **at most one open** pull request per `head`/`base` pair rather than one
/// per head. That is why `queue::pick_open` reads a record's state instead of
/// taking a head's word for it (#359), and why the withdrawn row's address and that
/// rule cannot disagree: neither claims more than the forge promises. The
/// artefact is reclaimable while it is open, which is the state a withdrawal
/// in flight leaves it in, so a line saying nothing claims it would be false.
///
/// **The reclaiming is start-work's; the address is the op's.** A
/// `create_pull_request` submitted straight through `submit_write` has no flow
/// to retry it, so nothing walks that path on its own. What is true of the op
/// either way is the part this answer turns on -- the row still names where to
/// look, and the read that looks is already written -- and start-work is the
/// caller that has one, which is the caller this op exists for (#44).
///
/// **The address is not a perfect key, and it does not have to be.**
/// `PULL_REQUEST_BY_HEAD` reads `sync.live_item`, which holds closed and
/// merged pull requests as deliberately as open ones, so a repository that
/// re-used a head branch matches more than one. That ambiguity belongs to the
/// look-before-write and the withdrawal adds nothing to it -- and since #359
/// the look-before-write resolves it rather than taking whichever sorted
/// first: it settles a step "already open" only against a record it read as
/// open, and where it cannot tell, nothing is settled and the create goes to
/// the source to refuse. Neither reading changes what this answer needs from the
/// row, which is narrower still: name one repository's pull requests from one
/// named branch, which is a set a person can look at.
/// `base` narrows it further; a `title` does not narrow to one artefact at
/// all, since any number of them can carry the same one -- which is exactly
/// why `create_ticket`, whose row carries a title and no address, needs the
/// line. The tree already draws that same line, in the read direction:
/// `knobas_app::protocol::ticket_titled` is the read-back for a
/// created ticket, and its own doc says why it is the weaker one --
/// "`start_work`'s precedent matched a head branch, and this matches free
/// prose".
///
/// **The property is the address, not "the server named nothing".** Gitea
/// answers `WriteReceipt::none()` for both ops on the *success* path too, so a
/// pull request's server-assigned number is nothing the withdrawal took away
/// -- knobas never has it, and reads it back out of the mirror either way.
/// `create_ticket` can leave one because after the withdrawal nothing
/// recovers *which* ticket: "the next sync cannot tell a ticket it made from
/// one a colleague made" (below), and a title is not an address
/// the way a head branch is.
///
/// **`create_monitor` can leave one, and the receipt is not what decides
/// it** (issue #453). Uptime Kuma answers `add` with the `monitorID` it
/// minted, so the disclosure line can point at the monitor -- exactly
/// `create_page`'s shape, which can also leave one and also answers an id.
/// What decides is the artefact, not the silence: a monitor made and then
/// withdrawn from is a check running against somebody's estate that no
/// knobas row claims, and the *name* it was made under is a label rather than
/// an address -- Uptime Kuma holds any number of monitors called the same
/// thing, which is why `testenv/kuma-monitor.sh` deletes by name before it
/// adds. That is the `title` reading above, not the head branch's.
///
/// **A source action can leave one because knobas cannot say** (#1194): what
/// it does is the adapter's alone, so whether it made something no row
/// names is unknown, and a disclosure that turns out needless costs a line
/// where a missing one costs an artefact nobody knows about.
///
/// `remote_id` rides along **only when the source named what it made**. That
/// is `WriteReceipt`'s own rule and it splits the two ops this exists for:
/// Confluence's `create_page` answers an id, so the line can point at the
/// page; Jira's `create_ticket` answers `WriteReceipt::none()` on purpose --
/// a created ticket is addressed by reading the mirror back -- so the line
/// says a ticket was made and withdrawn and says nothing about which.
/// Inventing an id here would be knobas naming an artefact it never saw.
///
/// **The line does not copy the payload**, for the reason `discard` gives for
/// keeping the row: the withdrawn write is still there, with what was asked
/// for in it, and `write_id` is what points at it. A create's payload is the
/// whole body of a page, and the activity log is not where a second copy of it
/// belongs.
///
/// **This is a disclosure, not a repair.** Nothing reconciles the artefact:
/// knobas has no key on it, the next sync cannot tell a ticket it made from
/// one a colleague made, and `create_page`'s id names a page that is not in
/// the mirror until that sync. What the user gets is the record that it
/// happened, on the container it happened under, which is where a person
/// looking for it would look.
///
/// **Infallible, like the line it writes.** Both the re-read and the record
/// are logged and dropped on failure rather than raised, which is
/// [`announce`]'s own rule taken one step back: the write has already been
/// delivered by the time this runs, and a database that will not answer must
/// not turn a delivered write into a failed flush. Nothing downstream branches
/// on it -- [`attempt`] returns `Ok(false)` either way, because the queue did
/// not move.
async fn unclaimed(
    deps: &SchedulerDeps,
    write: &QueuedWrite,
    can_leave: bool,
    receipt: &WriteReceipt,
) {
    if !can_leave {
        return;
    }
    let settled = match store::get(&deps.pool, write.id).await {
        Ok(Some(settled)) => settled,
        Ok(None) => return,
        Err(error) => {
            tracing::warn!(%error, write = write.id, "the withdrawn write could not be re-read");
            return;
        }
    };
    if settled.state != WriteState::Discarded {
        return;
    }
    announce_with(deps, "unclaimed", &settled, receipt.remote_id.as_deref()).await;
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

    fn waiting(reason: Option<WaitReason>, attempts: i32, at: &str) -> QueuedWrite {
        let at: chrono::DateTime<chrono::Utc> = at.parse().unwrap();
        QueuedWrite {
            id: 1,
            source_id: "jira".to_owned(),
            entity_id: "jira:PAY-1".to_owned(),
            op: "comment".to_owned(),
            payload: serde_json::Value::Null,
            target_snapshot: serde_json::Value::Null,
            state: WriteState::Pending,
            wait_reason: reason,
            detail: None,
            queued_at: at,
            attempted_at: (attempts > 0).then_some(at),
            attempts,
            held_snapshot: None,
            settled_at: None,
            source_enabled: true,
        }
    }

    fn at(time: &str) -> chrono::DateTime<chrono::Utc> {
        time.parse().unwrap()
    }

    /// #1628: an unreachable write is due on P7's ladder -- one minute after
    /// its first failure, two after its second, an hour from the fifth on --
    /// and a refused one is never due on a clock.
    #[test]
    fn an_unreachable_wait_is_owed_on_the_ladder_and_a_refused_one_never() {
        let failed = "2026-10-22T10:00:00Z";
        for (attempts, not_yet, due) in [
            (1, "2026-10-22T10:00:59Z", "2026-10-22T10:01:00Z"),
            (2, "2026-10-22T10:01:59Z", "2026-10-22T10:02:00Z"),
            (5, "2026-10-22T10:59:59Z", "2026-10-22T11:00:00Z"),
            (40, "2026-10-22T10:59:59Z", "2026-10-22T11:00:00Z"),
        ] {
            let write = waiting(Some(WaitReason::Unreachable), attempts, failed);
            assert!(!owed(&write, at(not_yet)), "{attempts} failures, {not_yet}");
            assert!(owed(&write, at(due)), "{attempts} failures, {due}");
        }

        let refused = waiting(Some(WaitReason::Unauthorized), 1, failed);
        assert!(!owed(&refused, at("2027-10-22T10:00:00Z")));

        assert!(owed(&waiting(None, 0, failed), at(failed)), "never tried");
        assert!(
            owed(&waiting(None, 3, failed), at(failed)),
            "released by a new credential"
        );
    }

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
            // A write a proxy redirected, which `knobas_http` refuses rather
            // than follow (#1640): an address the reader has to fix.
            (
                SourceError::Protocol {
                    status: Some(301),
                    message: "HTTP 301 Moved Permanently: the write was redirected to /x/"
                        .to_owned(),
                },
                None,
            ),
            (
                SourceError::protocol("this adapter does not accept comment"),
                None,
            ),
        ] {
            assert_eq!(retryable(&error, None), expected, "{error:?}");
        }
    }

    /// #1729: the quota's mark turns only a status-less `Protocol` into a
    /// wait. A status the source answered is still what decides, and a
    /// sink's failure is not the network's. The quota is a real one: a
    /// one-a-second limiter whose burst is spent has no room at once.
    #[tokio::test]
    async fn the_quotas_mark_waits_out_only_a_status_less_protocol() {
        let limiter =
            knobas_http::InstanceLimiter::for_url("http://quota-1729.invalid", 1, 1).unwrap();
        limiter
            .until_ready_within(std::time::Duration::from_secs(5))
            .await
            .unwrap();
        let quota = limiter
            .until_ready_within(std::time::Duration::ZERO)
            .await
            .expect_err("the burst is spent");
        for (error, expected) in [
            (
                SourceError::protocol("knobas' local rate limit had no room"),
                Some(WaitReason::Unreachable),
            ),
            (
                SourceError::Protocol {
                    status: Some(400),
                    message: "no comments on this issue type".to_owned(),
                },
                None,
            ),
            (SourceError::Sink("disk full".to_owned()), None),
        ] {
            assert_eq!(retryable(&error, Some(&quota)), expected, "{error:?}");
        }
    }
}
