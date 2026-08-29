//! The sync engine: one run of a [`Source`] into Postgres.
//!
//! [`run_once`] is the whole write side of a sync. It hands the adapter a
//! [`Sink`] that batches its items into `knobas.entity` and `sync.item`, all in
//! **one transaction** -- a run either lands completely or not at all, so a
//! failure halfway through cannot leave the store holding half a source's
//! world with a fresh cursor written over the gap.
//!
//! Two tables per item, because they answer different questions:
//!
//! * `knobas.entity` is the durable identity. Links, notes and activity point
//!   at it, so a remote deletion only sets `deleted_at` -- the row itself stays
//!   and everything referencing it survives.
//! * `sync.item` is the synced mirror: title, body, author, raw payload, and
//!   the generated `fts` column search reads. It is refreshed wholesale on
//!   every run, including for a tombstoned entity, so the UI can still render
//!   the last-known title of something that vanished upstream. "Deleted"
//!   therefore lives on the entity alone, and the way to respect it is to read
//!   the mirror through the **`sync.live_item`** view (migration 0002), which
//!   has the `knobas.entity` join and the `deleted_at is null` filter built in.
//!   Anything reading `sync.item` directly has to filter for itself, and a
//!   smart list that forgets offers rows that no longer exist.
//!
//! The engine takes a `PgPool` rather than opening one: it is called from the
//! app, from a scheduler and from tests, none of which want a second database.
//!
//! # What the engine refuses
//!
//! An adapter is a plugin, and a plugin regresses. The SPI's contract battery
//! catches that in the adapter's own test suite; the engine catches it at the
//! moment it would corrupt the store, because entity ids are global and
//! `sync.item.entity_id` is a primary key -- one adapter emitting another's ids
//! would silently overwrite its rows. So [`run_once`] rejects an unusable
//! source id before it opens a transaction, and the sink rejects an item whose
//! id does not round-trip, is outside the source's namespace, or carries a kind
//! the descriptor never declared.

pub mod config;
pub mod health;
#[cfg(any(test, feature = "test-util"))]
pub mod mirror;
pub mod progress;
pub mod run_log;
pub mod scheduler;
pub mod stats;
pub mod write_queue;

pub use health::{AuthState, CredentialHealth};
pub use progress::{ProgressSink, SyncPhase, SyncProgress};
pub use run_log::{RunCounts, RunResult, SourceSyncStatus, SyncOutcome, SyncTrigger};

use std::collections::{BTreeMap, HashMap, HashSet};

use knobas_core::activity;
use knobas_core::entity::EntityRef;
use knobas_source::{Cursor, Sink, Source, SourceError, SyncItem};
use sqlx::{Connection, PgConnection, PgPool, Postgres, Transaction};

/// How many items one pair of round trips writes.
///
/// The sink buffers up to this many items and then writes them as two
/// array-valued statements, so a 5,000-item source costs 20 round trips rather
/// than 10,000. Public because it is the boundary a source's behaviour changes
/// at -- a test that means to exercise a mid-run flush has to cross it.
pub const BATCH: usize = 500;

/// What one run wrote.
///
/// Counts are per *entity*, deduplicated across the whole run: an adapter that
/// emits the same entity twice contributes one. `deleted` is a subset of
/// `upserted` -- every item is written to both tables and `deleted` says how
/// many of them ended the run tombstoned -- so after a full sync,
/// `upserted == count(sync.item where source_id = <id>)`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncReport {
    pub source_id: String,
    /// Distinct entities written this run.
    pub upserted: u64,
    /// How many of them ended the run tombstoned.
    pub deleted: u64,
    /// Rows a **full** sync tombstoned because this run did not see them
    /// (hard-delete reconciliation). Counts only rows of a kind that declared
    /// `full_sync_exhaustive` *and* emitted something this run, so it is 0 for
    /// an incremental run, for a kind that did not declare the flag, and for a
    /// kind that declared it and emitted nothing -- see [`SWEEP`].
    ///
    /// A run over a source with both kinds of kind reports one number for the
    /// exhaustive half; the budgeted half is never in it. What that leaves
    /// unreachable is on [`run_once`], under *Limitations*.
    pub swept: u64,
    /// Where the source says the next run should resume.
    pub cursor: Cursor,
}

/// Why a run failed.
///
/// The database appears twice on purpose: [`Source`] surfaces a sink failure as
/// [`SourceError::Sink`] because that is the only channel the SPI gives an
/// adapter, so every failed *item write* arrives as [`SyncError::Source`],
/// while statements the engine issues around the adapter arrive as
/// [`SyncError::Db`].
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    /// The descriptor's id cannot be used as an entity namespace. Raised
    /// before the transaction opens: nothing was read or written.
    #[error("source id {id:?} is unusable: {reason}")]
    BadSourceId { id: String, reason: &'static str },
    /// The source has no `knobas.source_config` row, so there is nowhere to
    /// resume from and nowhere to store the new position. Only
    /// [`run_from_stored_cursor`] raises it; [`run_once`] with an explicit
    /// cursor still syncs an unconfigured source (a test, an ad-hoc import).
    #[error("source {id:?} is not configured")]
    NotConfigured { id: String },
    /// The adapter failed, or propagated a sink failure back to us.
    #[error("source: {0}")]
    Source(#[from] SourceError),
    /// A statement the engine issued failed.
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
}

/// Reject a source id that cannot serve as an entity namespace.
///
/// Checked against the descriptor rather than trusted, because the namespace
/// guard in the sink is only as good as the id it compares against: a source
/// calling itself `jira:eu` would make `jira:eu:PAY-1` parse as namespace
/// `jira`, and one calling itself `note` would write into the local notes'
/// namespace. Both defeat the guard completely.
fn check_source_id(id: &str) -> Result<(), SyncError> {
    let bad = |reason| {
        Err(SyncError::BadSourceId {
            id: id.to_owned(),
            reason,
        })
    };
    if id.trim().is_empty() {
        return bad("blank");
    }
    if id.contains(':') {
        return bad("contains ':', which would split it into a different namespace");
    }
    // The list lives beside `EntityRef`, not here: the SPI's contract battery
    // rejects the same ids at certification time, and two copies of it are how
    // an adapter passes its own suite and then fails every real sync.
    if knobas_core::entity::is_reserved_namespace(id) {
        return bad("reserved for knobas-local entities");
    }
    Ok(())
}

/// Pull everything `source` changed since `cursor` into `pool`.
///
/// `cursor` is the one a previous run returned in its [`SyncReport`], or `None`
/// for a full sync. On success the new cursor is persisted into
/// `knobas.source_config.cursor` **if a row for this source exists** -- a run
/// against an unconfigured source (a test, an ad-hoc import) syncs fine and
/// stores no position, and never invents a configuration row for itself.
///
/// The activity line is written after the commit, from the same pool but
/// outside the transaction: it records what happened, so it must not be rolled
/// back with the run it is describing, and must not be written before that run
/// is durable. A run that changed nothing -- no entities, no new cursor --
/// writes no line: a five-minute scheduler would otherwise bury the log under
/// 288 "synced nothing" entries per source per day.
///
/// Because it is written after the commit, a failure to write it **does not
/// fail the run**: it is logged at `warn` and the report is returned. The sync
/// is durable at that point, and reporting it as failed would tell a scheduler
/// to run it again and the UI to show an error over data that landed.
///
/// # Concurrency
///
/// The run takes `pg_advisory_xact_lock` on the source id, so two runs of the
/// same source serialise instead of interleaving their upserts. Different
/// sources run concurrently, and cannot collide anyway: the sink refuses items
/// outside the source's own namespace.
///
/// A run **pins one pool connection for its whole duration**, which for a real
/// adapter means for as long as the remote system takes to answer. A scheduler
/// syncing many sources at once must cap its concurrency below the pool size
/// (or hold a pool of its own), or the sync will starve the UI's queries.
///
/// # Limitations
///
/// **Hard deletes are inexpressible in three cases**, and reading only the
/// first is how a reader comes away expecting the sweep to fire far more often
/// than it does. Each is deliberate, and [`SyncReport::swept`] states the same
/// three rules from the other side.
///
/// **(1) A non-exhaustive kind is never swept.** A full sync reconciles only
/// the kinds that declared `full_sync_exhaustive` (ADR-0003). For every other
/// kind -- Gitea's budgeted `commit` and `pr`, TeamCity's windowed `build` and
/// `build_config` -- "stopped being returned" and "deleted upstream" are the
/// same observation, so the row stays live and the mirror keeps what it last
/// saw. The ways to close it are a reconcile call on the `Sink` SPI (rejected
/// in ADR-0003 -- `Sink` staying write-only is the verified reason the
/// tombstone deferral was sound) or an adapter that stops budgeting the kind,
/// which is a decision about that adapter's read path. Until then the stale
/// row is the accepted cost, and it is the cheap side of the trade: sweeping a
/// budgeted kind would tombstone everything past the cap on every full sync.
///
/// **(2) An exhaustive kind that emitted nothing is never swept either** --
/// not even when the same run emitted plenty of another kind. That is the
/// emptiness guard in `run_locked`, and its consequence is that a kind whose
/// corpus goes to *zero* upstream keeps every row of it live indefinitely: a
/// Gitea source whose last repository is deleted, or whose `owners[]` is
/// narrowed to owners that hold no repositories, goes on holding that `repo`
/// row and the branch rows under it, however many full syncs run afterwards.
/// (`owners: []` is not that case -- an empty list means *every* repository
/// the token can see, per `GiteaConfig::owners`.)
///
/// This is the deliberate side of a trade the engine cannot win. An empty
/// listing and a credential that quietly lost its scope are the same 200 with
/// the same empty body, so the alternative is tombstoning a whole kind on a
/// token change. A stale row is the cheap error; a wiped corpus is not.
///
/// **(3) An incremental run reconciles nothing, of any kind.** The sweep is
/// gated on `cursor.is_none()`, and a run that resumed from a position has not
/// seen the whole source. This bounds (1) and (2) rather than adding to them,
/// but it is the one that decides how often any of this happens at all:
/// scheduled runs go through [`run_from_stored_cursor`], so in a running
/// installation a cursor-less run is a source's *first* sync and *Load demo
/// data*, and little else. A repository deleted upstream is therefore retired
/// the next time that source syncs in full and emits a repository (case 2) --
/// which, absent a cleared cursor, may be never.
///
/// # Errors
///
/// * [`SyncError::BadSourceId`] if the descriptor's id cannot be a namespace.
///   Raised first: nothing is read or written.
/// * [`SyncError::Source`] if the adapter failed, or if it propagated one of
///   our own sink failures (a rejected item, or a failed write). The
///   transaction is rolled back: nothing is written and the cursor is left
///   where it was.
/// * [`SyncError::Db`] if the advisory lock, the cursor update or the commit
///   failed.
///
/// A failed activity line is deliberately *not* in that list -- see above.
pub async fn run_once(
    pool: &PgPool,
    source: &dyn Source,
    cursor: Option<Cursor>,
) -> Result<SyncReport, SyncError> {
    run_inner(
        pool,
        Host::Pool(pool),
        source,
        CursorSource::Explicit(cursor),
    )
    .await
}

/// Run `source` from the position `knobas.source_config` recorded for it, on a
/// connection that belongs to this run alone.
///
/// This is what the scheduler calls, and two things separate it from
/// [`run_once`].
///
/// **The cursor is read inside the lock.** Two triggers arriving together -- a
/// scheduler tick and a *Sync now* -- serialise, and the second resumes from
/// the position the first stored instead of repeating its fetch. (M0 read it
/// before the lock; the carry-over records the cost: on a 40,000-issue Jira, a
/// wasted full re-fetch.)
///
/// **The lock and the transaction live on `conn`, not on a pool.** That is
/// interfaces §10.6(c): a run holds its transaction open for as long as the
/// remote system takes to answer, so a run on a *pooled* connection is a run
/// competing with every query the UI makes for the connection the network has
/// parked. Worse than slow -- an adapter loop with no reachable exit pins that
/// connection and that source's lock until the process dies.
///
/// The signature does **not** enforce this on its own, and an earlier version
/// of this paragraph claimed it did: `sqlx::pool::PoolConnection` derefs to
/// `PgConnection`, so a determined caller can still hand one over. What makes
/// the property hold is where the scheduler gets its connections --
/// [`RunConnections::open`](crate::scheduler::RunConnections::open), whose one
/// implementation is `knobas_db::Connector::connect`, which belongs to no pool.
/// The signature is what makes that the obvious thing to pass;
/// `tests/dedicated.rs` is what checks it, by measuring the pool from outside
/// while a run is parked.
///
/// `pool` is used for exactly one thing: the activity line, written **after**
/// the commit, when nothing is held. It is a single insert on a connection
/// borrowed for microseconds, and it goes through `knobas_core::activity` so
/// that this crate does not carry a second copy of the activity schema.
///
/// # Errors
///
/// As [`run_once`], plus [`SyncError::NotConfigured`] when the source has no
/// configuration row: it has nowhere to store a position, so every later run
/// would sync everything again, for ever, with nothing to show that anything
/// was wrong.
pub async fn run_from_stored_cursor(
    conn: &mut PgConnection,
    pool: &PgPool,
    source: &dyn Source,
) -> Result<SyncReport, SyncError> {
    run_inner(pool, Host::Dedicated(conn), source, CursorSource::Stored).await
}

/// **Backfill** `source`: hand the adapter no position at all, whatever is
/// stored, and do not sweep.
///
/// A backfill is a deliberate full sync whose purpose is re-fetching *unchanged*
/// items after the fetched payload widened (CONTEXT.md). It exists because
/// nothing else can do that job: an incremental run re-fetches what changed
/// **upstream**, and widening a `fields=` list changes nothing upstream, so an
/// item nobody has touched keeps the narrower record for ever. Issue #32 is the
/// worked example -- Jira's `BASE_FIELDS` gained `parent`, `issuelinks`,
/// `resolution` and three more, and every already-mirrored issue needed
/// re-reading before epic membership could be built on top of `payload`.
///
/// # Why it does not sweep
///
/// This is the one way a backfill is not simply [`run_once`] with `None`, and
/// it is deliberate. The sweep tombstones every row of an exhaustive kind that
/// a cursor-less run did not return (ADR-0003) -- right for a source's *first*
/// sync, wrong here for two reasons that compound:
///
/// * **It is not what was asked for.** An operator asking for a wider payload
///   is not asking the engine to judge which items still exist. A backfill that
///   reconciles as a side effect makes "re-read the source" an operation nobody
///   can run without also accepting deletions they never inspected.
/// * **The failure mode has no error to catch.** A credential that quietly
///   loses sight of a project, or a `projects` list naming one that was
///   archived, answers with a *smaller corpus and a 200*. There is no status
///   code, no `SourceError`, nothing for a classification to branch on -- and
///   the sweep's own emptiness guard does not help, because the other projects
///   still emitted plenty. Before this entry point existed, a cursor-less run
///   in a live installation was a source's first sync and *Load demo data*,
///   which bounded the exposure; making backfills routine removes that bound.
///   A stale row is the cheap error and a tombstoned corpus is not, which is
///   the same trade *(2)* under [`run_once`]'s *Limitations* already makes.
///
/// Reconciliation therefore stays where it was: a source's first sync, and
/// [`run_once`] with an explicit `None`.
///
/// That is also why a run started this way is logged under
/// [`SyncTrigger::Backfill`] rather than `Manual` (`scheduler::Scheduler::backfill`):
/// the sweep is the one behaviour a backfill *removes*, so "which run produced
/// this tombstone count" has to be answerable from the log, and it is not if a
/// backfill and *Sync now* write the same word.
///
/// [`SyncTrigger::Backfill`]: crate::run_log::SyncTrigger::Backfill
///
/// The position the run comes back with **is** stored, exactly as any other
/// run's is, so the next scheduled run is incremental again. Clearing the
/// stored cursor instead -- the obvious spelling of "run without one" -- would
/// leave the next scheduled run a full sweeping sync, which is precisely what
/// the paragraph above refuses.
///
/// # Errors
///
/// As [`run_from_stored_cursor`], including [`SyncError::NotConfigured`]: a
/// backfill needs somewhere to store the position it comes back with, and the
/// refusal happens before the source is read.
pub async fn run_backfill(
    conn: &mut PgConnection,
    pool: &PgPool,
    source: &dyn Source,
) -> Result<SyncReport, SyncError> {
    run_inner(pool, Host::Dedicated(conn), source, CursorSource::Backfill).await
}

/// Where a run's transaction -- and therefore its advisory lock -- lives.
///
/// The distinction exists because of interfaces §10.6(c). It is not a
/// performance knob: which of these a run uses decides whether a slow remote
/// system can starve the rest of the application.
enum Host<'h> {
    /// The caller's pool. [`run_once`] only, and only because its callers do
    /// no network work worth speaking of: *Load demo data* reads a fixture
    /// compiled into the binary, and an ad-hoc import in a test is over in
    /// milliseconds. A scheduled run must never come through here.
    Pool(&'h PgPool),
    /// A connection this run owns for its whole duration, drawn from no pool
    /// (`knobas_db::Connector::connect`). What every scheduled run gets.
    Dedicated(&'h mut PgConnection),
}

/// Where a run gets the position it resumes from.
///
/// Both entry points delegate to one `run_inner`, so [`run_once`] keeps its M0
/// **signature and cursor semantics** exactly -- `demo_load` passes an explicit
/// `None` and still gets a full sync from that `None` -- while the scheduler's
/// entry point gets the read under the lock.
///
/// It is *not* byte-for-byte the M0 run, and the difference has a caller:
/// `run_inner` now sweeps after a full sync, over the kinds that declared
/// `full_sync_exhaustive`, and every one of the mock's kinds declares it -- so
/// *Load demo data* tombstones `mock:` entities that the fixture stopped
/// emitting. That is the intended behaviour
/// -- a demo corpus should not accumulate items the fixture no longer has --
/// but it is new in M1, and a reader comparing this against M0 needs to know
/// the sweep is the thing that changed.
enum CursorSource {
    /// The caller decided: [`run_once`]'s argument, unchanged from M0.
    Explicit(Option<Cursor>),
    /// Read from `knobas.source_config` **inside the run's own lock**.
    Stored,
    /// [`run_backfill`]: the stored row is read (for the same
    /// [`SyncError::NotConfigured`] refusal `Stored` makes) and then
    /// deliberately not used, and the sweep is off.
    ///
    /// This is what splits two things that were one: *the adapter was handed
    /// no position* and *this run may reconcile*. `Explicit(None)` and
    /// `Stored`-with-nothing-stored are both, a backfill is only the first.
    Backfill,
}

impl CursorSource {
    /// Whether a run started this way is allowed to tombstone what it did not
    /// return -- assuming it also turns out to be cursor-less, which is
    /// checked separately in `run_locked`.
    ///
    /// Both conditions, never one: dropping the cursor-less half sweeps on an
    /// incremental run, and dropping this half sweeps on a backfill. The
    /// reasons they exist are different and neither implies the other, which
    /// is why they are two expressions and not one flag.
    fn may_sweep(&self) -> bool {
        !matches!(self, CursorSource::Backfill)
    }

    /// Whether the position stored for the source is read for the
    /// [`SyncError::NotConfigured`] refusal and then thrown away, rather than
    /// resumed from.
    ///
    /// A method beside [`Self::may_sweep`] rather than a `matches!` inlined in
    /// `run_locked`, because the two are the whole of what a backfill *is* and
    /// a reader should find them together. They are still two, not one flag:
    /// each has its own reason and neither implies the other.
    fn discards_the_stored_position(&self) -> bool {
        matches!(self, CursorSource::Backfill)
    }
}

async fn run_inner(
    pool: &PgPool,
    host: Host<'_>,
    source: &dyn Source,
    from: CursorSource,
) -> Result<SyncReport, SyncError> {
    let descriptor = source.descriptor();
    check_source_id(&descriptor.id)?;
    let source_id = descriptor.id;
    // Two sets out of one list: everything the sink will accept, and the
    // subset the sweep may act on. Both are read here, before `entity_kinds`
    // is consumed.
    let exhaustive: HashSet<String> = descriptor
        .entity_kinds
        .iter()
        .filter(|kind| kind.full_sync_exhaustive)
        .map(|kind| kind.id.clone())
        .collect();
    let kinds: HashSet<String> = descriptor
        .entity_kinds
        .into_iter()
        .map(|kind| kind.id)
        .collect();

    let mut tx = match host {
        Host::Pool(pool) => pool.begin().await?,
        Host::Dedicated(conn) => conn.begin().await?,
    };

    // Every exit from the locked section goes through here, and the rollback
    // is **explicit**.
    //
    // Dropping a `Transaction` does not send a `ROLLBACK`; it queues one for
    // the next time the connection is used. On a pooled connection the pool
    // flushes it as the connection goes back, which is why M0 could drop and
    // forget. On a run's own connection (§10.6(c)) there is no pool to do it,
    // so a failed run would leave `pg_advisory_xact_lock` **held** until that
    // connection happened to be used again -- and the next run of the same
    // source would block on it for ever. Found by a test that failed one run
    // and then ran another on the same connection.
    let locked = run_locked(&mut tx, source, &source_id, kinds, exhaustive, from).await;
    let locked = match locked {
        Ok(locked) => locked,
        Err(error) => {
            if let Err(rollback) = tx.rollback().await {
                // The lock goes with the connection either way -- closing it
                // aborts the transaction server-side -- so this is a
                // diagnostic, not a second failure to report over the first.
                tracing::warn!(source_id, %rollback, "rolling the failed run back failed");
            }
            return Err(error);
        }
    };
    tx.commit().await?;

    let Locked {
        cursor,
        upserted,
        deleted,
        swept,
        previous,
    } = locked;
    let report = SyncReport {
        source_id,
        upserted,
        deleted,
        swept,
        cursor,
    };
    let changed_nothing = report.upserted == 0
        && report.deleted == 0
        && report.swept == 0
        && previous.as_deref() == Some(&report.cursor);
    if !changed_nothing
        && let Err(error) = activity::record(
            pool,
            &format!("sync:{}", report.source_id),
            "synced",
            // A run is about a source, not about any one of the entities it
            // touched, so the line carries no entity id.
            None,
            // The report *is* the detail; hand-copying its fields here is how
            // the two drift apart.
            serde_json::to_value(&report).expect("a SyncReport serializes"),
        )
        .await
    {
        // Warned about, never raised. Everything this run wrote is already
        // durable, so a failure here is one missing log line -- and reporting
        // it as a failed sync would make the caller believe none of it landed:
        // a scheduler would repeat a run that already happened, and the UI
        // would show an error over data sitting in the database.
        tracing::warn!(
            source_id = %report.source_id,
            %error,
            "the sync committed, but its activity line did not"
        );
    }
    Ok(report)
}

/// What the locked section produced, before the commit.
struct Locked {
    cursor: Cursor,
    upserted: u64,
    deleted: u64,
    swept: u64,
    /// The position the run started from, for the "changed nothing" test.
    previous: Option<Cursor>,
}

/// Everything that happens under the source's advisory lock.
///
/// Factored out of [`run_inner`] so that **one** place decides between commit
/// and rollback: a `?` in here returns to a caller that always closes the
/// transaction, rather than dropping it and leaving the lock queued behind an
/// unsent `ROLLBACK`.
async fn run_locked(
    tx: &mut Transaction<'_, Postgres>,
    source: &dyn Source,
    source_id: &str,
    kinds: HashSet<String>,
    exhaustive: HashSet<String>,
    from: CursorSource,
) -> Result<Locked, SyncError> {
    // Held until this transaction ends, however it ends. Two runs of one source
    // would otherwise take the same rows in whatever order their batches
    // happened to fall in, and a run holds its locks across every batch.
    sqlx::query("select pg_advisory_xact_lock(hashtext($1::text))")
        .bind(source_id)
        .execute(&mut **tx)
        .await?;

    // Inside the lock, deliberately: see `run_from_stored_cursor`.
    // Read before `from` is consumed below, and kept apart on purpose: what a
    // run resumes from and whether it may reconcile are two questions, and a
    // backfill answers them differently (see `CursorSource::may_sweep`).
    let may_sweep = from.may_sweep();
    let discards_the_stored_position = from.discards_the_stored_position();
    // `cursor` is what the adapter is handed; `previous` is where the source
    // stood before this run. The same value for every run but a backfill,
    // which is the point of separating them: a backfill *has* a position and
    // deliberately does not resume from it, and reporting `previous` as `None`
    // there would tell the activity log that a source with a stored position
    // had none.
    let (cursor, previous) = match from {
        CursorSource::Explicit(cursor) => (cursor.clone(), cursor),
        CursorSource::Stored | CursorSource::Backfill => {
            let row: Option<(Option<Cursor>,)> =
                sqlx::query_as("select cursor from knobas.source_config where id = $1")
                    .bind(source_id)
                    .fetch_optional(&mut **tx)
                    .await?;
            // Nothing was read from the source and nothing written; the caller
            // rolls back, which is what releases the lock.
            let Some((stored,)) = row else {
                return Err(SyncError::NotConfigured {
                    id: source_id.to_owned(),
                });
            };
            // A backfill reads the row for that refusal -- so a source with
            // nowhere to store a position costs no network traffic -- and then
            // throws the position away. That is the whole of what makes it
            // cursor-less.
            if discards_the_stored_position {
                (None, stored)
            } else {
                (stored.clone(), stored)
            }
        }
    };
    let full_sync = cursor.is_none();

    let (cursor, upserted, deleted, emitted) = {
        let mut sink = PgSink::new(tx, source_id.to_owned(), kinds);
        let cursor = source.sync(cursor, &mut sink).await?;
        // The adapter is done, so whatever is still buffered belongs to this
        // run: flush it before the cursor claims to cover it.
        sink.flush().await?;
        (cursor, sink.upserted, sink.deleted, sink.emitted)
    };

    // Reconcile what a full sync did not see. Inside the same transaction as
    // the writes, so a failure rolls the tombstones back with them.
    //
    // Four conditions, and dropping any one of them alone is a bug:
    //  * `full_sync` -- an incremental run has not seen the whole source;
    //  * `may_sweep` -- a **backfill** is a cursor-less run that must not
    //    reconcile. It has seen the whole source, so `full_sync` cannot stand
    //    in for this: the reason is that reconciling is not what was asked for,
    //    and that the way a backfill goes wrong (a credential that quietly
    //    narrowed answers with a smaller corpus and a 200) produces no error
    //    for anything to catch. See `run_backfill`;
    //  * `exhaustive` -- a *bounded* full sync (TeamCity: newest N builds per
    //    configuration) does not return everything, so absence is not deletion.
    //    Read **per kind** (ADR-0003): one adapter routinely walks some kinds
    //    in full and budgets others, and a source-wide answer is wrong for it
    //    in both directions;
    //  * the kind emitted something -- a full sync that emitted nothing *of a
    //    kind* is indistinguishable from an adapter that silently failed, and
    //    sweeping there would tombstone that whole kind. Per kind for the same
    //    reason the gate is: a repository listing that came back empty while
    //    the branch walk succeeded would otherwise pass a source-wide test.
    //
    // The two sets are intersected rather than checked in sequence, so a kind
    // only reaches the sweep on both counts at once. Intersection is the only
    // operator that is safe in both directions: `exhaustive` alone drops the
    // emptiness guard, `emitted` alone drops the gate and sweeps budgeted
    // kinds, and a union is both bugs at once.
    let sweep_kinds: Vec<String> = emitted.intersection(&exhaustive).cloned().collect();
    // `!sweep_kinds.is_empty()` is a **saved round trip, not a safety check**,
    // and no test covers it -- `i.kind = any('{}')` matches nothing, so
    // dropping it changes only whether an empty statement is issued. Every
    // correctness property here rests on the intersection above; a reader
    // looking for what stops the sweep firing should look there and not at
    // this condition.
    let swept = if full_sync && may_sweep && !sweep_kinds.is_empty() {
        sqlx::query(SWEEP)
            .bind(source_id)
            .bind(&sweep_kinds)
            .execute(&mut **tx)
            .await?
            .rows_affected()
    } else {
        0
    };

    sqlx::query("update knobas.source_config set cursor = $1 where id = $2")
        .bind(&cursor)
        .bind(source_id)
        .execute(&mut **tx)
        .await?;

    Ok(Locked {
        cursor,
        upserted,
        deleted,
        swept,
        previous,
    })
}

/// Hard-delete reconciliation for a full sync (interfaces §1, point 4), run
/// only over the kinds that declared `full_sync_exhaustive` **and** emitted
/// something this run -- see `run_locked`.
///
/// `$2` is that kind list, and it is why this is `i.kind = any($2)` rather
/// than a bare source filter: a source's budgeted kinds share the table with
/// its exhaustive ones, and a sweep that took the whole source would tombstone
/// every commit past `commits_per_repo` on every full sync (ADR-0003).
///
/// No `last_seen_at` column is needed, and adding one would be a second truth:
/// `ITEM_UPSERT` stamps `synced_at = now()`, and `now()` is the **transaction**
/// timestamp -- one value for every row this run wrote. So inside this very
/// transaction, `synced_at < now()` is precisely "this run did not touch it".
/// (The M0 carry-over already records that property, as an accepted consequence
/// of long runs stamping every item with the run's start.)
///
/// Driven from `sync.item` rather than from `knobas.entity`: the mirror is
/// indexed by `(source_id, …)`, so this touches one source's rows instead of
/// scanning every entity knobas holds.
///
/// `deleted_at is null` keeps the **first** deletion's timestamp, exactly as
/// `ENTITY_UPSERT` does -- a tombstone restamped by every later run would
/// report a month-old deletion as fresh for ever -- and is also what makes
/// `swept` count *new* tombstones rather than every already-dead row. The
/// mirror row is left alone on purpose: the UI still renders the last-known
/// title of something that vanished upstream.
const SWEEP: &str = r#"
update knobas.entity e
   set deleted_at = now()
  from sync.item i
 where i.entity_id = e.id
   and i.source_id = $1
   and i.kind = any($2)
   and i.synced_at < now()
   and e.deleted_at is null
"#;

/// The [`Sink`] the engine hands to an adapter: buffers items and writes them
/// into the caller's transaction in batches.
///
/// Borrows the transaction rather than owning it so that [`run_once`] keeps the
/// commit -- a sink that could commit would let an adapter make a partial sync
/// durable by dropping it.
struct PgSink<'t, 'c> {
    tx: &'t mut Transaction<'c, Postgres>,
    /// The descriptor id, which is also the namespace every item must be in.
    source_id: String,
    /// The kinds the descriptor declared; nothing else may be emitted.
    kinds: HashSet<String>,
    buf: Vec<SyncItem>,
    /// Every entity id written this run, against whether its most recent write
    /// tombstoned it. Run-scoped rather than per-batch because the counts end
    /// up in a durable activity line, and an entity re-emitted in a later batch
    /// is still one row -- counting it twice would report 501 rows written for
    /// a source that has 500. One entry per distinct entity, so a very large
    /// source pays for this in memory; batching alone cannot dedupe a run.
    seen: HashMap<String, bool>,
    /// Every kind this run actually wrote a row of. The sweep's emptiness
    /// guard reads it: a kind absent from here emitted nothing, so this run
    /// proves nothing about what that kind still holds. A set rather than a
    /// count because that is the whole question -- "did anything of this kind
    /// arrive" -- and it is bounded by the descriptor's kind list, unlike
    /// `seen`.
    emitted: HashSet<String>,
    upserted: u64,
    deleted: u64,
}

impl<'t, 'c> PgSink<'t, 'c> {
    fn new(
        tx: &'t mut Transaction<'c, Postgres>,
        source_id: String,
        kinds: HashSet<String>,
    ) -> Self {
        Self {
            tx,
            source_id,
            kinds,
            buf: Vec::new(),
            seen: HashMap::new(),
            emitted: HashSet::new(),
            upserted: 0,
            deleted: 0,
        }
    }

    /// Whether this item is this source's to emit, and addressable at all.
    fn check(&self, item: &SyncItem) -> Result<(), SourceError> {
        let id = item.entity.to_string();
        // `EntityRef::new` does not validate, so an adapter can hand us a blank
        // or unparseable key; the store would then hold an id nothing can
        // address. Round-tripping through `parse` is the same check the SPI's
        // contract battery makes.
        let parsed = EntityRef::parse(&id).map_err(|e| SourceError::Sink(e.to_string()))?;
        if parsed != item.entity {
            return Err(SourceError::Sink(format!(
                "entity {id:?} does not round-trip: namespace {:?} is not addressable",
                item.entity.namespace
            )));
        }
        if parsed.namespace != self.source_id {
            return Err(SourceError::Sink(format!(
                "source {:?} emitted {id} outside its namespace",
                self.source_id
            )));
        }
        // The UI renders this source's items from the kinds it declared, so an
        // item of an undeclared kind is one nothing knows how to show.
        if !self.kinds.contains(&item.kind) {
            return Err(SourceError::Sink(format!(
                "source {:?} emitted {id} of undeclared kind {:?}",
                self.source_id, item.kind
            )));
        }
        // ...and a kind knobas *owns* is not a source's to emit whatever its
        // descriptor claims. The sibling of the namespace guard above, and the
        // reason it is a second check rather than a widening of that one: the
        // namespace guard compares an item against the source id it was given,
        // so a source legitimately called `jira` passes it while emitting
        // `jira:x` of kind `note`. That is not a sweep hazard -- migration
        // `0006` is what makes that impossible, and it works on the id -- but
        // it is a mirrored row claiming to be the thing knobas owns, in the
        // launcher's note group, in `type:note`, and unwritable through any
        // note command.
        if knobas_core::entity::is_owned_kind(&item.kind) {
            return Err(SourceError::Sink(format!(
                "source {:?} emitted {id} of kind {:?}, which knobas owns and no source mirrors",
                self.source_id, item.kind
            )));
        }
        Ok(())
    }

    /// Write everything buffered.
    ///
    /// Failures surface as [`SourceError::Sink`] from both call sites -- the
    /// adapter's mid-run flush and [`run_once`]'s final one -- so the same
    /// database failure is reported the same way wherever it happens.
    async fn flush(&mut self) -> Result<(), SourceError> {
        self.write_batch()
            .await
            .map_err(|e| SourceError::Sink(e.to_string()))
    }

    /// One batch, as two array-valued upserts.
    async fn write_batch(&mut self) -> Result<(), sqlx::Error> {
        if self.buf.is_empty() {
            return Ok(());
        }
        // Keyed by entity id, so an adapter that pushes the same entity twice
        // in one batch gets last-write-wins instead of Postgres' "ON CONFLICT
        // DO UPDATE command cannot affect row a second time". Ordered, so a
        // batch's writes have a stable order.
        let batch: BTreeMap<String, SyncItem> = self
            .buf
            .drain(..)
            .map(|item| (item.entity.to_string(), item))
            .collect();

        let n = batch.len();
        let mut ids = Vec::with_capacity(n);
        let mut kinds = Vec::with_capacity(n);
        let mut titles = Vec::with_capacity(n);
        let mut bodies = Vec::with_capacity(n);
        let mut authors = Vec::with_capacity(n);
        let mut updated = Vec::with_capacity(n);
        let mut deleted = Vec::with_capacity(n);
        let mut payloads = Vec::with_capacity(n);
        let mut web_urls = Vec::with_capacity(n);
        for (id, item) in batch {
            ids.push(id);
            kinds.push(item.kind);
            titles.push(item.title);
            bodies.push(item.body_text);
            authors.push(item.author);
            updated.push(item.updated_at);
            deleted.push(item.deleted);
            payloads.push(item.payload);
            web_urls.push(item.web_url);
        }

        // The entity first: `sync.item.entity_id` references it.
        sqlx::query(ENTITY_UPSERT)
            .bind(&ids)
            .bind(&kinds)
            .bind(&titles)
            .bind(&updated)
            .bind(&deleted)
            .execute(&mut **self.tx)
            .await?;
        sqlx::query(ITEM_UPSERT)
            .bind(&ids)
            .bind(&kinds)
            .bind(&titles)
            .bind(&bodies)
            .bind(&authors)
            .bind(&updated)
            .bind(&payloads)
            .bind(&web_urls)
            .bind(&self.source_id)
            .execute(&mut **self.tx)
            .await?;

        // Counted only once the rows are actually in the transaction.
        for (id, tombstoned) in ids.iter().zip(&deleted) {
            self.count(id, *tombstoned);
        }
        self.emitted.extend(kinds);
        Ok(())
    }

    /// Fold one written row into the run's counts, at most once per entity.
    fn count(&mut self, id: &str, tombstoned: bool) {
        if let Some(was) = self.seen.get_mut(id) {
            // Same entity again in a later batch: it is still one row, but the
            // later write decides whether that row is now a tombstone.
            if *was != tombstoned {
                *was = tombstoned;
                if tombstoned {
                    self.deleted += 1;
                } else {
                    self.deleted -= 1;
                }
            }
            return;
        }
        self.seen.insert(id.to_owned(), tombstoned);
        self.upserted += 1;
        if tombstoned {
            self.deleted += 1;
        }
    }
}

/// `knobas.entity`: identity, kind, title, and the deletion tombstone.
///
/// The left join reads the row this upsert is about to replace, which is what
/// lets both timestamps keep their history:
///
/// * `updated_at` keeps the stored value when the source does not say when the
///   item changed (`SyncItem::updated_at` is `None`), instead of restamping it
///   to `now()` on every run and making an untouched item look freshly edited.
///   Only a genuinely new row falls back to `now()`.
/// * `deleted_at` keeps the *first* deletion's timestamp rather than being
///   restamped by every later run, and is cleared when the item comes back --
///   sources do resurrect things, and a stale tombstone would hide a live
///   entity.
///
/// Reading the old row and writing the new one is only atomic because
/// [`run_once`] holds the source's advisory lock: same-source runs cannot
/// interleave, and no other source may write this namespace.
const ENTITY_UPSERT: &str = r#"
with incoming as (
  select *
    from unnest($1::text[], $2::text[], $3::text[], $4::timestamptz[], $5::bool[])
         as t(id, kind, title, updated_at, deleted)
)
insert into knobas.entity (id, kind, title, updated_at, deleted_at)
select i.id, i.kind, i.title,
       coalesce(i.updated_at, old.updated_at, now()),
       case when i.deleted then coalesce(old.deleted_at, now()) end
  from incoming i
       left join knobas.entity old on old.id = i.id
    on conflict (id) do update set
       kind       = excluded.kind,
       title      = excluded.title,
       updated_at = excluded.updated_at,
       deleted_at = excluded.deleted_at
"#;

/// `sync.item`: the mirror, refreshed wholesale. `synced_at` is when this run
/// saw the item; `item_updated_at` is when the source says it changed.
///
/// `item_updated_at` keeps the stored value when the source does not say, for
/// the same reason and by the same left join as `knobas.entity.updated_at`:
/// the two columns hold the same fact, and a mirror whose timestamp disagreed
/// with its entity's on identical input would be a trap for anything reading
/// either. It stays null only while the source has never dated the item --
/// unlike the entity's, which is `not null` and falls back to `now()` on a
/// genuinely new row.
///
/// `web_url` is refreshed wholesale like the title, **not** coalesced like
/// `item_updated_at`: an adapter that stops reporting a URL is reporting that
/// there is no page, and the two timestamps coalesce only because they hold the
/// same fact as `knobas.entity.updated_at`.
const ITEM_UPSERT: &str = r#"
with incoming as (
  select *
    from unnest($1::text[], $2::text[], $3::text[], $4::text[], $5::text[],
                $6::timestamptz[], $7::jsonb[], $8::text[])
         as t(id, kind, title, body_text, author, item_updated_at, payload, web_url)
)
insert into sync.item
       (entity_id, source_id, kind, title, body_text, author, item_updated_at,
        synced_at, payload, web_url)
select i.id, $9, i.kind, i.title, i.body_text, i.author,
       coalesce(i.item_updated_at, old.item_updated_at), now(), i.payload, i.web_url
  from incoming i
       left join sync.item old on old.entity_id = i.id
    on conflict (entity_id) do update set
       source_id       = excluded.source_id,
       kind            = excluded.kind,
       title           = excluded.title,
       body_text       = excluded.body_text,
       author          = excluded.author,
       item_updated_at = excluded.item_updated_at,
       synced_at       = excluded.synced_at,
       payload         = excluded.payload,
       web_url         = excluded.web_url
"#;

#[async_trait::async_trait]
impl Sink for PgSink<'_, '_> {
    /// Buffer one item, writing a full batch through as it fills.
    ///
    /// # Errors
    ///
    /// [`SourceError::Sink`] -- the only channel the SPI gives a sink -- if the
    /// item is not this source's to emit, or if the write failed. Either way
    /// the adapter must propagate it and abandon the sync, which rolls the
    /// whole run back.
    async fn item(&mut self, item: SyncItem) -> Result<(), SourceError> {
        self.check(&item)?;
        self.buf.push(item);
        if self.buf.len() >= BATCH {
            self.flush().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_id_must_be_usable_as_a_namespace() {
        check_source_id("jira").unwrap();
        check_source_id("uptime-kuma").unwrap();
        for bad in ["", "   ", "jira:eu", ":", "CTX"] {
            assert!(check_source_id(bad).is_err(), "{bad:?} should be refused");
        }
        // Iterated rather than spelled out again: the engine has to refuse
        // every namespace knobas keeps, including any added later.
        for reserved in knobas_core::entity::RESERVED_NAMESPACES {
            assert!(
                check_source_id(reserved).is_err(),
                "{reserved:?} should be refused"
            );
        }
    }

    /// `SyncReport` crosses the bridge -- `demo_load` returns one -- so its
    /// shape is contract. The **exact key set**, for the reason the M0
    /// carry-over spells out: a test that only walks a hardcoded list of
    /// fields cannot see a Rust field added with no TypeScript counterpart,
    /// and `swept` was exactly such an addition.
    #[test]
    fn the_report_shape_matches_its_typescript_mirror() {
        let report = SyncReport {
            source_id: "mock".to_owned(),
            upserted: 12,
            deleted: 1,
            swept: 2,
            cursor: r#"{"v":1}"#.to_owned(),
        };
        crate::mirror::assert_shape(
            include_str!("../../../app/src/lib/ipc/sources.ts"),
            "SyncReport",
            &serde_json::to_value(&report).expect("a report serializes"),
            &["cursor", "deleted", "source_id", "swept", "upserted"],
        );
    }

    fn unit_item(source_id: &str, n: usize) -> SyncItem {
        SyncItem {
            entity: EntityRef::new(source_id, &format!("U-{n}")),
            kind: "ticket".to_owned(),
            title: format!("unit {n}"),
            body_text: String::new(),
            author: None,
            updated_at: None,
            payload: serde_json::json!({}),
            web_url: None,
            deleted: false,
        }
    }

    async fn mirrored(tx: &mut Transaction<'_, Postgres>, source_id: &str) -> i64 {
        let (rows,): (i64,) = sqlx::query_as("select count(*) from sync.item where source_id = $1")
            .bind(source_id)
            .fetch_one(&mut **tx)
            .await
            .unwrap();
        rows
    }

    /// A full batch is written **while the adapter is still syncing**, not
    /// saved up until it returns.
    ///
    /// Everything else about batching rests on this: the run-scoped counters
    /// only differ from per-batch ones once a boundary has been crossed
    /// mid-run, and the rollback test is vacuous unless rows really were
    /// written before the failure. Driving the sink directly is the only way
    /// to observe the moment, because the transaction it writes into is
    /// invisible from any other connection and gone by the time `run_once`
    /// returns.
    #[tokio::test]
    async fn a_full_batch_is_written_while_the_adapter_is_still_running() {
        let pool = knobas_db::test_util::test_pool().await;
        knobas_db::migrate::run(&pool).await.unwrap();
        let source_id = format!("unit-{}", uuid::Uuid::new_v4());

        let mut tx = pool.begin().await.unwrap();
        let mid_run = {
            let kinds = HashSet::from(["ticket".to_owned()]);
            let mut sink = PgSink::new(&mut tx, source_id.clone(), kinds);
            for n in 0..BATCH {
                sink.item(unit_item(&source_id, n)).await.unwrap();
            }
            assert!(
                sink.buf.is_empty(),
                "a full batch must be flushed, not left buffered"
            );
            assert_eq!(sink.upserted, BATCH as u64);
            // Read back through the sink's own transaction: the adapter has not
            // returned and nothing has committed.
            mirrored(sink.tx, &source_id).await
        };
        assert_eq!(
            mid_run, BATCH as i64,
            "the batch must already be in the transaction"
        );

        // ...and it is still only in the transaction.
        tx.rollback().await.unwrap();
        let (after,): (i64,) =
            sqlx::query_as("select count(*) from sync.item where source_id = $1")
                .bind(&source_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(after, 0);
    }
}
