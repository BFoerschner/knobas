//! The sync scheduler: what runs, when, how often, and what happens when it
//! fails.
//!
//! Spec §3: a per-source sync schedule (default every 5 min) and *Sync now*;
//! credential health with 401 detection; diagnostics. §14: **the UI never
//! blocks on a source** -- every run happens here, on its own connection, and
//! reports through events.
//!
//! # What this module is *not* allowed to know
//!
//! It must not depend on `tauri`, and it must not depend on any
//! `knobas-source-*` adapter crate. Both would be convenient and both would
//! cost the property §3a is built on: the SPI is transport-agnostic so an
//! adapter can later run out of process, and an engine that links the adapters
//! is an engine that cannot host one. So the four things it needs from the
//! outside world arrive as traits -- [`AdapterRegistry`] (a stored instance ⇒
//! `Box<dyn Source>`), [`RunConnections`] (a connection outside every pool),
//! [`SecretStore`] and [`SyncEvents`] -- and `knobas-app` supplies the
//! concrete four.
//!
//! # Tasks
//!
//! Runs are spawned with `tokio::spawn` from a task the app started with
//! `tauri::async_runtime::spawn`, so they land on Tauri's runtime (which *is*
//! tokio) without this crate naming it.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use knobas_core::activity::ActivityRow;
use knobas_secrets::SecretStore;
use knobas_source::instance::SourceInstance;
use knobas_source::{Source, SourceDescriptor, SourceError};
use sqlx::{PgConnection, PgPool};
use tokio::sync::{Mutex, Notify, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::config::{self, AuthState, CredentialHealth};
use crate::progress::{Attach, Observed, ProgressSink, SyncPhase, SyncProgress, Watchers};
use crate::run_log::{self, RunResult, SyncOutcome, SyncTrigger};
use crate::{SyncError, run_backfill, run_from_stored_cursor};

pub use crate::run_log::SourceSyncStatus;

/// Turns a stored configuration into a live adapter.
///
/// Implemented in `knobas-app` over the compiled-in adapter table (P6: every
/// adapter crate exposes `descriptor_template()` and `build(SourceInstance)`).
pub trait AdapterRegistry: Send + Sync + 'static {
    /// One descriptor **template** per compiled-in adapter kind, with
    /// `id == adapter_kind`: what the Add-source form is generated from, and
    /// what the launcher reads kind metadata out of, without instantiating an
    /// adapter or touching the keychain.
    fn descriptors(&self) -> Vec<SourceDescriptor>;

    /// # Errors
    /// [`SourceError::Protocol`] if no adapter answers to the instance's
    /// `kind`, or if the adapter rejected the configuration.
    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError>;
}

/// Where a run gets the connection it holds for its whole duration.
///
/// **Not a `PgPool`, and that is interfaces §10.6(c).** A run keeps its
/// transaction -- and the source's advisory lock -- open for as long as the
/// remote system takes to answer. Drawn from a pool, that is a connection the
/// rest of the application is competing for; worse, an adapter loop with no
/// reachable exit pins it and the lock until the process dies. Each run
/// therefore opens a connection of its own and closes it when it is done, and
/// [`SYNC_CONCURRENCY`] is what bounds how many exist at once.
///
/// Implemented in `knobas-app` over `knobas_db::embedded::Connector`. A trait
/// rather than the connector itself for the reason the other three are traits:
/// this crate must stay testable with nothing but a `PgPool` in sight.
#[async_trait::async_trait]
pub trait RunConnections: Send + Sync + 'static {
    /// # Errors
    /// [`sqlx::Error`] if the connection cannot be opened.
    async fn open(&self) -> Result<PgConnection, sqlx::Error>;
}

/// Where the scheduler's coarse state goes.
///
/// Implemented in `knobas-app` over `AppHandle::emit`. **Coarse only**: at most
/// a handful of messages per run (roadmap §4 -- events are not for throughput).
/// Per-item progress goes on a [`ProgressSink`] and nowhere else.
pub trait SyncEvents: Send + Sync + 'static {
    /// `sync:state`, on every run transition (start / finish / fail).
    fn sync_state(&self, status: SourceSyncStatus);
    /// `source:health`, **on a health change only**.
    fn source_health(&self, health: CredentialHealth);
    /// `activity:new` -- the status bar's "latest change" line (§2). At most one
    /// per run, which is well inside the ≥ 1 s coalescing rule.
    fn activity_new(&self, row: ActivityRow);
}

/// Everything a run needs. Cloned into each spawned task via `Arc`.
pub struct SchedulerDeps {
    /// The scheduler's **own** pool -- see [`Scheduler::start`]. Used for
    /// bookkeeping only: opening and closing the log row, credential health,
    /// the status read, the post-commit activity line. The run's own work is
    /// on a connection from [`connections`](Self::connections).
    pub pool: PgPool,
    pub connections: Arc<dyn RunConnections>,
    pub registry: Arc<dyn AdapterRegistry>,
    pub secrets: Arc<dyn SecretStore>,
    pub events: Arc<dyn SyncEvents>,
}

/// The status of every configured source, id order.
///
/// One statement: the open run, the newest finished run, and the derived next
/// time. `case` rather than a `where`, because a disabled source still appears
/// in the sources view -- it just has no next run.
const STATUS: &str = r"
select c.id as source_id,
       r.id is not null as running,
       -- The run this status is *about*: the one in flight, or the last one to
       -- finish. Not `r.id` alone -- `last_finished_at` and `last_outcome`
       -- already describe that finished run, so leaving its id out made the
       -- terminal `sync:state` unattributable: a progress bar keyed on the run
       -- id it was handed could not tell which run had just ended. `running` is
       -- what says which of the two this is.
       coalesce(r.id, f.id) as run_id,
       r.started_at,
       f.finished_at as last_finished_at,
       f.outcome     as last_outcome,
       c.backoff_until,
       case
         when r.id is not null then null
         when not c.enabled then null
         when c.auth_state in ('unauthorized', 'missing_secret') then null
         else coalesce(
                greatest(
                  f.finished_at + make_interval(secs => c.sync_interval_secs::double precision),
                  c.backoff_until
                ),
                now())
       end as next_run_at
  from knobas.source_config c
  left join lateral (
      select id, started_at from knobas.sync_run
       where source_id = c.id and finished_at is null
       order by started_at desc, id desc limit 1
  ) r on true
  left join lateral (
      select id, finished_at, outcome from knobas.sync_run
       where source_id = c.id and finished_at is not null
       order by started_at desc, id desc limit 1
  ) f on true
 where ($1::text is null or c.id = $1)
 order by c.id
";

#[derive(sqlx::FromRow)]
struct StatusRow {
    source_id: String,
    running: bool,
    run_id: Option<i64>,
    started_at: Option<DateTime<Utc>>,
    last_finished_at: Option<DateTime<Utc>>,
    last_outcome: Option<String>,
    backoff_until: Option<DateTime<Utc>>,
    next_run_at: Option<DateTime<Utc>>,
}

impl From<StatusRow> for SourceSyncStatus {
    fn from(r: StatusRow) -> Self {
        SourceSyncStatus {
            running: r.running,
            source_id: r.source_id,
            run_id: r.run_id,
            started_at: r.started_at,
            last_finished_at: r.last_finished_at,
            last_outcome: r.last_outcome.as_deref().map(run_log::outcome_from_db),
            next_run_at: r.next_run_at,
            backoff_until: r.backoff_until,
        }
    }
}

/// # Errors
/// [`sqlx::Error`] if the query fails.
pub async fn status_all(pool: &PgPool) -> Result<Vec<SourceSyncStatus>, sqlx::Error> {
    let rows: Vec<StatusRow> = sqlx::query_as(STATUS)
        .bind(Option::<&str>::None)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// # Errors
/// [`sqlx::Error`] if the query fails.
pub async fn status_for(pool: &PgPool, id: &str) -> Result<Option<SourceSyncStatus>, sqlx::Error> {
    let row: Option<StatusRow> = sqlx::query_as(STATUS)
        .bind(Some(id))
        .fetch_optional(pool)
        .await?;
    Ok(row.map(Into::into))
}

/// The error text a missing keychain item produces.
///
/// A constant, not a literal in two places: [`settle`] distinguishes
/// `missing_secret` from `unauthorized` by it, and two spellings that drift
/// apart would silently downgrade "you never entered a credential" to "your
/// credential was rejected" -- different sentences, different UI offer.
pub(crate) const MISSING_SECRET_MESSAGE: &str = "no stored credential -- re-enter it";

/// Why a run did not produce a report.
///
/// `pub(crate)` because the write queue's flush loop builds an adapter through
/// the same [`build_source`] seam and has to classify the same failures --
/// with the queue's own vocabulary, not the run log's, since a source that
/// cannot be built means "these writes wait", not "this run failed".
#[derive(Debug)]
pub(crate) enum RunFailure {
    NotConfigured,
    MissingSecret,
    Secret(String),
    Source(SourceError),
    Sync(SyncError),
    Db(sqlx::Error),
}

impl RunFailure {
    /// How the run is logged, and therefore whether it backs off.
    ///
    /// A `Sink` failure is a *database* failure the SPI could only report
    /// through the adapter's channel, so it is `Error`, not a source fault
    /// -- backing off a remote system because the local disk is full would be
    /// the wrong story in the diagnostics view.
    fn outcome(&self) -> SyncOutcome {
        match self {
            RunFailure::MissingSecret => SyncOutcome::Unauthorized,
            RunFailure::Sync(error) => SyncOutcome::of(error),
            RunFailure::Source(SourceError::Unauthorized { .. }) => SyncOutcome::Unauthorized,
            RunFailure::Source(SourceError::Unreachable(_)) => SyncOutcome::Unreachable,
            RunFailure::NotConfigured
            | RunFailure::Secret(_)
            | RunFailure::Source(_)
            | RunFailure::Db(_) => SyncOutcome::Error,
        }
    }

    pub(crate) fn message(&self) -> String {
        match self {
            RunFailure::NotConfigured => "the source has no configuration row".to_owned(),
            RunFailure::MissingSecret => MISSING_SECRET_MESSAGE.to_owned(),
            RunFailure::Secret(detail) => format!("keychain: {detail}"),
            RunFailure::Source(error) => error.to_string(),
            RunFailure::Sync(error) => error.to_string(),
            RunFailure::Db(error) => error.to_string(),
        }
    }
}

/// Build the adapter for a stored configuration, fetching its secret.
///
/// `pub(crate)`: `crate::write_queue` builds its adapter here too, so that a
/// flush and a sync agree on what a configured source *is* -- including which
/// auth kinds need a keychain entry at all.
pub(crate) async fn build_source(
    deps: &SchedulerDeps,
    cfg: &config::SourceConfigRow,
) -> Result<Box<dyn Source>, RunFailure> {
    let secret = match cfg.auth_kind.method() {
        // A source that needs no credential at all: `--demo` must work against
        // an empty keychain (§14a), and the keychain is not even asked.
        None => None,
        Some(_) => {
            let stored = knobas_secrets::spawn::get(&deps.secrets, &cfg.id)
                .await
                .map_err(|e| RunFailure::Secret(e.to_string()))?;
            Some(stored.ok_or(RunFailure::MissingSecret)?.value)
        }
    };
    let instance = SourceInstance {
        id: cfg.id.clone(),
        kind: cfg.adapter_kind.clone(),
        display_name: cfg.display_name.clone(),
        base_url: cfg.base_url.clone(),
        auth: cfg.auth_kind.method(),
        secret,
        config: cfg.config.clone(),
    };
    deps.registry.build(instance).map_err(RunFailure::Source)
}

/// What a run does with the position the source has stored.
///
/// Not a spelling of `SyncTrigger`, and deliberately a second axis: the
/// trigger says *why* a run happened and is written to `knobas.sync_run` for
/// the diagnostics list, while this says what the run *does* with the stored
/// position and is never stored. The two are near enough to one another that
/// it is worth saying where they part: every trigger but
/// [`SyncTrigger::Backfill`] implies [`RunMode::Incremental`], but the reverse
/// does not hold in the other direction for free -- `Backfill` is the mode,
/// and the trigger only records that a run started in it. `run_once` reaches
/// the engine with neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RunMode {
    /// Resume from the stored position: every scheduled run, *Sync now*, and
    /// a source's first sync (which is cursor-less because nothing is stored
    /// yet, not because the run asked for that).
    #[default]
    Incremental,
    /// Ignore the stored position and re-read the source from the top, without
    /// sweeping -- [`crate::run_backfill`], which is where the reasoning is.
    Backfill,
}

impl From<SyncTrigger> for RunMode {
    /// The mode a run started for this reason is in.
    ///
    /// Many-to-one, and **derived rather than passed**: `backfill` in the log
    /// is read as "this run could not have reconciled", which is only true of
    /// `RunMode::Backfill`, so the two must never be able to disagree. They
    /// once travelled as two parameters with a `debug_assert` pairing them,
    /// which is a guard compiled out of the shipped binary -- exactly where a
    /// mislabelled `swept` would mislead. Deriving makes the pairing
    /// structural instead. `execute_run` still takes the mode, because it is
    /// below the log and the trigger does not reach it.
    fn from(trigger: SyncTrigger) -> Self {
        match trigger {
            SyncTrigger::Backfill => Self::Backfill,
            SyncTrigger::Schedule | SyncTrigger::Manual | SyncTrigger::FirstRun => {
                Self::Incremental
            }
        }
    }
}

/// Run one source, and say what happened. Writes nothing to the log itself --
/// [`settle`] does that -- so a test can assert on the verdict directly.
pub async fn execute_run(
    deps: &SchedulerDeps,
    source_id: &str,
    run_id: i64,
    mode: RunMode,
    progress: Option<Arc<dyn ProgressSink>>,
    started: std::time::Instant,
) -> RunResult {
    // `started` comes from the caller so that every message about this run --
    // including the terminal one the ticker sends after `settle` -- measures
    // the same thing: how long the user has been waiting.
    if let Some(sink) = progress.as_ref() {
        report(
            sink,
            run_id,
            source_id,
            SyncPhase::Started,
            0,
            started,
            None,
        );
    }

    match attempt(deps, source_id, run_id, mode, progress.as_ref(), started).await {
        Ok(report) => RunResult {
            outcome: SyncOutcome::Ok,
            counts: run_log::RunCounts::of(&report),
            error: None,
        },
        Err(failure) => {
            tracing::warn!(source_id, run_id, failure = %failure.message(), "sync run failed");
            RunResult {
                outcome: failure.outcome(),
                counts: run_log::RunCounts::default(),
                error: Some(failure.message()),
            }
        }
    }
}

/// Send one progress message.
///
/// Every phase this crate emits goes through here, which is what makes
/// "emitted at the moment it is reached" checkable rather than a claim:
/// `elapsed_ms` is always measured from the run's start, never passed in as a
/// placeholder.
fn report(
    sink: &Arc<dyn ProgressSink>,
    run_id: i64,
    source_id: &str,
    phase: SyncPhase,
    items: u64,
    started: std::time::Instant,
    message: Option<String>,
) {
    sink.report(SyncProgress {
        run_id,
        source_id: source_id.to_owned(),
        phase,
        items,
        elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        message,
    });
}

/// Milliseconds since a run started, saturating.
fn elapsed_ms(started: std::time::Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// The terminal message for a run, built from how the run ended.
///
/// **One constructor, two callers, and that is ADR-0005's *faithfully*.** The
/// run builds this from the verdict it has just recorded; a caller that
/// attaches after the run is over builds it from the row that verdict was
/// written to ([`ending_of_record`]). Two constructors would be two chances for
/// live and late to disagree, and the one that disagreed would be the one
/// nobody was watching being built.
fn ending(
    run_id: i64,
    source_id: &str,
    outcome: SyncOutcome,
    error: Option<String>,
    items: u64,
    elapsed_ms: u64,
) -> SyncProgress {
    SyncProgress {
        run_id,
        source_id: source_id.to_owned(),
        // The failure phase, not merely "it is over": the first-run wizard's
        // *Retry* / *Skip for now* offer is driven by this field and by the
        // message beside it, so a wizard that joined late reaches the same
        // panel as one that watched the failure happen.
        phase: if outcome == SyncOutcome::Ok {
            SyncPhase::Finished
        } else {
            SyncPhase::Failed
        },
        items,
        elapsed_ms,
        message: error,
    }
}

/// The ending a run that has already finished owes a caller who has only just
/// arrived, read back out of the run's log row.
///
/// `None` means *nothing faithful can be said about that run*, and there are
/// two ways to get there: the row has been [pruned](run_log::prune), or it is
/// still open. An open row is a run whose process died mid-flight --
/// `outcome` is null while a run is in flight, so there is no recorded outcome
/// to synthesise from at all. [`run_log::reconcile_abandoned`] closes those at
/// the next start; until then the honest answer to a caller is a run of its
/// own, not a guess about somebody else's.
async fn ending_of_record(
    pool: &PgPool,
    source_id: &str,
    run_id: i64,
) -> Result<Option<SyncProgress>, sqlx::Error> {
    let Some(row) = run_log::get(pool, run_id).await? else {
        return Ok(None);
    };
    let (Some(finished_at), Some(outcome)) = (row.finished_at, row.outcome) else {
        return Ok(None);
    };
    Ok(Some(ending(
        run_id,
        source_id,
        outcome,
        row.error,
        u64::try_from(row.upserted).unwrap_or(0),
        u64::try_from((finished_at - row.started_at).num_milliseconds()).unwrap_or(0),
    )))
}

async fn attempt(
    deps: &SchedulerDeps,
    source_id: &str,
    run_id: i64,
    mode: RunMode,
    progress: Option<&Arc<dyn ProgressSink>>,
    started: std::time::Instant,
) -> Result<crate::SyncReport, RunFailure> {
    let cfg = config::get(&deps.pool, source_id)
        .await
        .map_err(RunFailure::Db)?
        .ok_or(RunFailure::NotConfigured)?;
    let source = build_source(deps, &cfg).await?;

    // §10.6(c): the run's transaction and advisory lock go on this, and it
    // belongs to nobody else. Opened *after* the refusals above, so a source
    // that was never going to run costs no connection at all.
    let mut conn = deps.connections.open().await.map_err(RunFailure::Db)?;

    // The decorator exists whenever the run has somewhere to report to.
    // From the scheduler that is *every* run: ADR-0005 lets a caller attach to
    // a run already in flight, so whether anybody is watching is not settled
    // when the run starts and a run that decided at the top would have nowhere
    // to put a later arrival. The cost of an unwatched run is one message built
    // per throttled tick and handed to an empty set (P3 is about what crosses
    // the bridge, and nothing does). `None` is for a caller driving a run
    // directly -- the tests -- and then nothing is built at all. `Fetching` and
    // `Writing` are emitted from inside the decorator, where they are true.
    // Two independent choices, composed rather than enumerated: whether the
    // run is watched, and which entry point it goes through. Crossing them in
    // one `match` gave four arms, two of which built the same decorator, and
    // made `(Some(sink), Backfill)` look like a case somebody had to reach for
    // it to be live code.
    let observed =
        progress.map(|sink| Observed::new(source.as_ref(), run_id, started, Arc::clone(sink)));
    let watched: &dyn Source = match &observed {
        Some(o) => o,
        None => source.as_ref(),
    };
    let report = match mode {
        RunMode::Incremental => run_from_stored_cursor(&mut conn, &deps.pool, watched).await,
        RunMode::Backfill => run_backfill(&mut conn, &deps.pool, watched).await,
    };

    // Explicit, and not left to `Drop`: `PgConnection::drop` closes the socket
    // without a terminate message, and a run that ends every five minutes
    // should hand its backend back politely. A failure here is not the run's.
    if let Err(error) = sqlx::Connection::close(conn).await {
        tracing::debug!(source_id, %error, "closing the run's connection failed");
    }
    report.map_err(RunFailure::Sync)
}

/// Close the run: log it, judge the credential, set or clear the backoff, emit.
///
/// Every step is best-effort *after* the sync itself committed. The data is
/// already durable at this point, so a failed bookkeeping statement is warned
/// about and the rest still runs -- refusing to record the outcome because the
/// prune failed would leave a run open for ever.
pub async fn settle(deps: &SchedulerDeps, source_id: &str, run_id: i64, result: &RunResult) {
    if let Err(error) = run_log::finish(&deps.pool, run_id, result).await {
        tracing::warn!(source_id, run_id, %error, "the run finished, but its log row did not close");
    }
    if let Err(error) = run_log::prune(&deps.pool, source_id, run_log::KEEP_RUNS).await {
        tracing::warn!(source_id, %error, "pruning the sync log failed");
    }

    apply_health_and_backoff(deps, source_id, result).await;

    if result.outcome == SyncOutcome::Ok && result.counts.touched() > 0 {
        emit_latest_activity(deps, source_id).await;
    }
    emit_state(deps, source_id).await;
}

/// Whether a source holds `source_id` **now**, as [`Scheduler::forget_source`]
/// asks it immediately before arming or applying a purge (#154).
///
/// A free function over the answer rather than the read itself, because the
/// third arm is the one that matters and no integration test can reach it: it
/// needs `config::get` to fail while `config::purge_items` still works, which
/// one pool cannot produce. So the decision is separated from the query and
/// pinned directly.
///
/// * `Ok(true)` -- a source exists under the id again. Neither destructive act
///   happens: the user's newest instruction about the id wins, which is the
///   principle [`Scheduler::source_added`] already ratified, applied at the
///   same layer through the other door.
/// * `Ok(false)` -- the id is free, which is what `delete_source` expects to
///   find. Today's behaviour, unchanged.
/// * `Err(_)` -- **warn and proceed as if the id were free.** Skipping the
///   purge on a database blip would reopen #127's symptom (a deleted source's
///   items live in `sync.live_item` for ever) on an error that occurs alone far
///   more often than it occurs together with the re-add race. With this arm, a
///   wrong purge needs the race *and* a read failure at that instant; that
///   conjunction is the accepted residual, recorded on `delete_source`'s
///   guarantee.
fn a_source_holds_the_id_again(found: &Result<bool, sqlx::Error>, source_id: &str) -> bool {
    match found {
        Ok(true) => {
            tracing::info!(
                source_id,
                "a source was added back under this id before the delete finished; \
                 its mirror is not purged"
            );
            true
        }
        Ok(false) => false,
        Err(error) => {
            tracing::warn!(
                source_id,
                %error,
                "could not check whether a source still holds this id; purging as \
                 the delete asked"
            );
            false
        }
    }
}

/// Re-apply a deleted source's purge, now that the run which was in flight when
/// it was deleted has committed (#127).
///
/// The same statement `delete_source`'s own purge runs -- [`config::purge_items`]
/// is the one copy of it, so the two cannot drift apart on what a tombstone
/// means.
///
/// Best-effort like everything else after a run's own transaction: a purge that
/// fails is warned about rather than raised, because there is nobody left to
/// raise it to. `delete_source` returned long ago. It is also not retried, so
/// a failure here is the one way the guarantee `delete_source` documents can
/// come up short; the log line is what a reader chasing that symptom finds.
///
/// **Not `sweep`**, though that is what #127's ruling calls it in prose: this
/// crate already uses that word for the glossary's Sweep -- the reconcile pass
/// that tombstones what a full sync no longer emitted (`SyncReport::swept`,
/// `run_locked`'s `sweep_kinds`) -- and `CONTEXT.md`'s entry for it names
/// "purge" as the word to avoid *for that concept*. Two meanings of `sweep` in
/// one crate, one of them destructive, is the reading mistake worth spending a
/// longer name to remove.
async fn purge_again(deps: &SchedulerDeps, source_id: &str) {
    match config::purge_items(&deps.pool, source_id).await {
        Ok(()) => tracing::info!(
            source_id,
            "purged the mirror a deleted source's in-flight run wrote back"
        ),
        Err(error) => tracing::warn!(
            source_id,
            %error,
            "a deleted source's mirror could not be purged; its items are still searchable"
        ),
    }
}

async fn apply_health_and_backoff(deps: &SchedulerDeps, source_id: &str, result: &RunResult) {
    let (state, detail) = match result.outcome {
        SyncOutcome::Ok => (Some(AuthState::Ok), None),
        SyncOutcome::Unauthorized if result.error.as_deref() == Some(MISSING_SECRET_MESSAGE) => {
            (Some(AuthState::MissingSecret), result.error.as_deref())
        }
        SyncOutcome::Unauthorized => (Some(AuthState::Unauthorized), result.error.as_deref()),
        SyncOutcome::Unreachable => (Some(AuthState::Unreachable), result.error.as_deref()),
        // A protocol bug or a local database failure says nothing about the
        // credential; leaving `auth_state` alone keeps the sources view honest.
        SyncOutcome::Error => (None, None),
    };
    if let Some(state) = state {
        match config::set_health(&deps.pool, source_id, state, detail, None).await {
            Ok(Some((health, changed))) if changed => deps.events.source_health(health),
            Ok(_) => {}
            Err(error) => tracing::warn!(source_id, %error, "recording credential health failed"),
        }
    }

    let backoff = if result.outcome == SyncOutcome::Ok {
        config::clear_backoff(&deps.pool, source_id).await
    } else if result.outcome.backs_off() {
        // The rung is derived from the log, so it survives a restart with no
        // in-memory counter to lose.
        match run_log::failures_since_last_ok(&deps.pool, source_id).await {
            Ok(failures) => {
                let wait = config::backoff_after(failures);
                let until = Utc::now()
                    + chrono::Duration::from_std(wait)
                        .unwrap_or_else(|_| chrono::Duration::hours(1));
                tracing::info!(
                    source_id,
                    failures,
                    seconds = wait.as_secs(),
                    "backing the source off"
                );
                config::set_backoff(&deps.pool, source_id, until).await
            }
            Err(error) => Err(error),
        }
    } else {
        // `unauthorized`: no retry is scheduled at all (P7). Whatever backoff
        // was there stays -- clearing it would make a source that was already
        // failing look ready the moment its credential is fixed by something
        // other than `set_source_secret`, which is the one path that clears it.
        Ok(())
    };
    if let Err(error) = backoff {
        tracing::warn!(source_id, %error, "recording the backoff failed");
    }
}

/// The newest activity line one source's syncs wrote.
///
/// Scoped by actor, not "the newest line, if it happens to be ours". Six
/// sources finishing within a second of each other is ordinary, and a global
/// `limit 1` would hand five of them somebody else's line -- or, once it was
/// filtered out, nothing at all.
///
/// `knobas_core::activity` has a global read and a per-*entity* read; a run is
/// about a source and carries no entity id, so neither fits. The column list is
/// the same one, which is the part that would drift -- and it drifts loudly:
/// `ActivityRow` is a `FromRow`, so a renamed column fails this query at run
/// time and `scheduler_run::a_healthy_run_is_logged_ok_...`, which asserts
/// every field of the row it received, is what notices.
const LATEST_BY_ACTOR: &str = "select id, at, actor, verb, entity_id, detail
     from knobas.activity
    where actor = $1
    order by at desc, id desc
    limit 1";

/// Re-read the line the run wrote and hand it to the status bar.
///
/// Read back rather than reconstructed: `run_inner` owns the shape of that line
/// (its `detail` is the serialized report), and a second hand-built copy is how
/// the two drift apart.
async fn emit_latest_activity(deps: &SchedulerDeps, source_id: &str) {
    let actor = format!("sync:{source_id}");
    let row: Result<Option<ActivityRow>, _> = sqlx::query_as(LATEST_BY_ACTOR)
        .bind(&actor)
        .fetch_optional(&deps.pool)
        .await;
    match row {
        Ok(Some(row)) => deps.events.activity_new(row),
        Ok(None) => {}
        Err(error) => tracing::warn!(source_id, %error, "reading back the activity line failed"),
    }
}

/// Emit the current state for one source. Called at the start of a run (right
/// after its log row exists) and again from [`settle`].
async fn emit_state(deps: &SchedulerDeps, source_id: &str) {
    match status_for(&deps.pool, source_id).await {
        Ok(Some(status)) => deps.events.sync_state(status),
        Ok(None) => {}
        Err(error) => tracing::warn!(source_id, %error, "reading sync status for the event failed"),
    }
}

// -- the ticker ---------------------------------------------------------------

/// Connections the scheduler's own pool gets.
///
/// Its own, deliberately: the application's five are the UI's, and a scheduler
/// doing its bookkeeping -- opening a run row, closing it, writing health,
/// reading status -- while six sources are mid-sync must not be competing for
/// them. The *runs* take no pool connection at all (§10.6(c),
/// [`RunConnections`]), so this pool is sized for bookkeeping, not for runs.
pub const SYNC_POOL_SIZE: u32 = 4;

/// Runs allowed at once.
///
/// The only thing bounding how many connections the scheduler's runs hold,
/// now that they do not come from a pool: without it, twenty configured
/// sources going due together would be twenty backends and twenty concurrent
/// remote calls. Three is a desktop's worth of parallelism, and it leaves the
/// bookkeeping pool untouched by design rather than by arithmetic.
pub const SYNC_CONCURRENCY: usize = 3;

/// How often the ticker looks for due sources. One indexed query; a fixed
/// interval is cheaper to reason about than a computed sleep, and five seconds
/// is well inside the smallest interval a user can configure (60 s).
const TICK: Duration = Duration::from_secs(5);

/// How long the ticker waits before its first look.
///
/// Two jobs: it keeps the first `sync:state` from racing the webview's
/// listeners (roadmap §4 gotcha 9 -- `sync_status()` on mount is the
/// authoritative read, the event is a hint), and it keeps a cold start from
/// competing with the first-run wizard for the same source.
const STARTUP_DELAY: Duration = Duration::from_secs(2);

/// How long shutdown waits for runs to notice the cancellation before aborting
/// them. A cancelled run only has to unwind to its next await point.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(3);

/// Upper bound on closing the scheduler's pool. Belt and braces: the runs are
/// already cancelled by the time this is reached.
const POOL_CLOSE_TIMEOUT: Duration = Duration::from_secs(2);

/// What `delete_source` asked to happen to a deleted source's mirror.
///
/// Carried into the scheduler rather than staying in `delete_source`, because
/// the delete's own purge is not the last word: a run of that source can still
/// be in flight, and its commit lands *after* the purge. See
/// [`Scheduler::forget_source`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purge {
    /// *Remove source and its items*. The mirror goes, and goes again if a run
    /// still in flight writes any of it back.
    Items,
    /// *Remove source, keep items* -- a real choice in the sources view
    /// (interfaces §3, Delete), and the reason a foreign key from
    /// `sync.item` to `source_config` could never have stood in for this.
    Keep,
}

impl From<bool> for Purge {
    /// From `delete_source`'s own `purge_items` flag, and **derived rather than
    /// passed alongside it**: the delete and the purge that finishes it have to
    /// mean the same thing, and a second `if` at the call site is where they
    /// stop doing so. Same reasoning as `RunMode::from(SyncTrigger)`.
    fn from(purge_items: bool) -> Purge {
        if purge_items {
            Purge::Items
        } else {
            Purge::Keep
        }
    }
}

/// Why a trigger did not start a run.
#[derive(Debug, thiserror::Error)]
pub enum TriggerError {
    #[error("no source with id {0:?}")]
    UnknownSource(String),
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
    #[error("the scheduler is shutting down")]
    ShuttingDown,
}

/// The running scheduler.
///
/// Held once, by `knobas-app`'s `SourcesState`.
pub struct Scheduler {
    inner: Arc<Inner>,
}

/// One source's most recent run, as [`Claims::runs`] holds it.
///
/// The flag is the *claim on the source's first sync*, and it is what makes
/// **spent when taken** true wherever the taking happened. A caller that asks
/// for [`SyncTrigger::FirstRun`] with a channel of its own -- in practice the
/// first-run wizard, the only caller of `sync_now_with_progress` -- is asking
/// about *the source's first sync*, and it is entitled to that answer once.
/// Being enrolled in the run while it is still going is that answer just as
/// much as being served the run's recorded ending afterwards is, so both spend
/// the claim.
///
/// Spending it on the live join is the whole point: the wizard's *Retry* is the
/// same command asking a second time, and a *Retry* that was handed the failure
/// it is retrying is a button that does nothing. Marking the claim only on the
/// after-the-fact path left that hole open in every interleaving where the
/// wizard watched its own run fail, which is the ordinary one.
///
/// A trigger with no sink -- the ticker's wake -- never claims: it wants work
/// done, and a run of its own is what it should get when the last one is over.
struct RunEntry {
    watchers: Arc<Watchers>,
    first_run_claimed: bool,
}

/// Everything this scheduler remembers about a source *id*, under one lock.
///
/// One lock and not two, and that is load-bearing rather than tidy. The two
/// halves are read against each other: [`Scheduler::forget_source`] has to know
/// whether the deleted source's run is still going before it decides between
/// arming a purge and applying one, and the run has to claim any armed purge
/// and close its watchers without that decision changing underneath it. With a
/// lock each there is an interleaving -- the run claims nothing, then the
/// delete arms a purge nobody will ever claim -- and the mirror keeps rows the
/// user deleted.
struct Claims {
    /// source id → the run this scheduler last started for it, and the set of
    /// sinks watching that run ([`Watchers`]).
    ///
    /// While the run is going this is the in-flight claim: the advisory lock
    /// would serialise two runs anyway, but this stops the *second one
    /// existing*, which is what keeps the log honest and the UI showing one
    /// progress bar. A second trigger is enrolled in the run it found instead
    /// of being handed an id and then silence (ADR-0005).
    ///
    /// **The entry outlives the run**, and that is the other half of ADR-0005:
    /// once the watchers are closed the entry is what lets the next trigger
    /// discover that the run it would have been handed is already over, and
    /// serve its caller that run's recorded ending rather than starting a
    /// second sync over an already-mirrored corpus. It is replaced when a run
    /// starts and removed when one is found closed, so the map is bounded by
    /// the number of sources, not by the number of runs.
    ///
    /// **It does not outlive the source.** That is the one boundary the
    /// outliving stops at, and it is not incidental: an entry is keyed by the
    /// user's chosen source id, `knobas.sync_run` deliberately keeps no foreign
    /// key to `source_config`, and a source deleted and added again under the
    /// same id is therefore a *different* source wearing an id whose run
    /// history is still readable. Left alone, the new source's first-run wizard
    /// could be handed the deleted source's run -- knobas' first sentence about
    /// a brand-new source describing something the user threw away.
    /// [`Scheduler::forget_source`] is where that life ends, and `delete_source`
    /// is what calls it.
    ///
    /// Held with the pending purges under one lock ([`Claims`]), because
    /// "is a run of this source still going?" and "does its mirror still owe
    /// somebody a purge?" have to be asked and answered together.
    runs: HashMap<String, RunEntry>,
    /// Source ids deleted with `purge_items: true` while a run of them was
    /// **still in flight** -- the purge that run's late commit is about to
    /// undo, waiting to be applied again once it has (#127).
    ///
    /// In memory and not a table, deliberately: the only writer that can undo
    /// the purge is that run's own uncommitted transaction, and a process that
    /// dies takes the transaction with it, server-side. There is nothing left
    /// to purge after a crash, so there is nothing to make durable.
    ///
    /// An entry is removed by the run that claims it, and by
    /// [`Scheduler::source_added`] when the user puts a source back under that
    /// id -- their newest instruction about the id wins, and a purge firing
    /// after a re-add would take the *new* source's first mirror.
    ///
    /// **What is left behind, and why it is left:** a run whose task dies
    /// without reaching the claim -- a panicking adapter, or the abort
    /// `shutdown` falls back on -- leaves its intent here until the process
    /// ends. That is one `String`, and on the usual version of that path the
    /// run's transaction went with the task, so there is nothing it would have
    /// purged. **Two versions are not that**, and both leave a deleted
    /// source's items live in `sync.live_item`: a panic *after* the run
    /// committed and before the claim, which is `settle` panicking (`settle`
    /// catches its own errors, so that is a bug elsewhere); and `shutdown`
    /// aborting a task that had committed but not yet reached the claim when
    /// [`SHUTDOWN_GRACE`] ran out. Neither carries machinery here, and
    /// durability would not buy any: the process is going away, the id is
    /// already gone from `source_config`, and nothing on the next start would
    /// have a run of it to hang the claim on.
    pending_purges: HashSet<String>,
}

struct Inner {
    deps: SchedulerDeps,
    permits: Semaphore,
    claims: Mutex<Claims>,
    /// Poked when something changed that might make a source due (a finished
    /// run, a new source, a re-entered credential), so the UI does not wait out
    /// a tick.
    wake: Notify,
    cancel: CancellationToken,
    /// Handles to join (or abort) at shutdown.
    ///
    /// Pruned on every push: a run adds one and never removes it, so a process
    /// left open for a week at a five-minute interval would accumulate
    /// thousands of finished handles for the sake of a shutdown that only
    /// cares about the live ones.
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl Scheduler {
    /// Reconcile whatever the last process left open, then start ticking.
    ///
    /// # Errors
    /// [`sqlx::Error`] if the reconciliation fails -- that one is worth
    /// refusing to start over: it is a single `update` on an indexed
    /// predicate, and a database that cannot serve it will not serve a run
    /// either.
    pub async fn start(deps: SchedulerDeps) -> Result<Scheduler, sqlx::Error> {
        // **Before the first tick, and only then.** It closes every run row
        // with no `finished_at`, which after this point would include the runs
        // this scheduler is itself about to open.
        let closed = run_log::reconcile_abandoned(&deps.pool).await?;
        if closed > 0 {
            tracing::warn!(closed, "closed sync runs left open by a previous process");
        }

        let inner = Arc::new(Inner {
            deps,
            permits: Semaphore::new(SYNC_CONCURRENCY),
            claims: Mutex::new(Claims {
                runs: HashMap::new(),
                pending_purges: HashSet::new(),
            }),
            wake: Notify::new(),
            cancel: CancellationToken::new(),
            tasks: Mutex::new(Vec::new()),
        });

        let ticker = tokio::spawn(tick_loop(Arc::clone(&inner)));
        inner.tasks.lock().await.push(ticker);
        Ok(Scheduler { inner })
    }

    /// Start a run for one source and return its `sync_run.id` **immediately**
    /// (P3): the row exists before the task is spawned, so the caller has
    /// something to watch without waiting on the network.
    ///
    /// A source already running is not started again -- the id of the run in
    /// flight comes back instead, which makes a double-click on *Sync now*
    /// harmless. **The caller's sink comes with it** (ADR-0005): whichever of
    /// those two things happened, the id it is handed is one it can watch, and
    /// it will be told how that run ended. A caller that never learns is a
    /// caller with no way to tell a working sync from a hung one, which is why
    /// there is no timeout anywhere below this line -- a long first sync of a
    /// large Jira is indistinguishable from a hang by wall-clock.
    ///
    /// [`SyncTrigger::FirstRun`] asks for something slightly different and says
    /// so by its spelling: *the source's first sync*, not *a sync*. If that has
    /// already happened -- `add_source` wakes the scheduler, so on a fast
    /// source it can be over before the wizard's own trigger arrives -- the
    /// caller is handed that run and its recorded ending instead of a second
    /// run over a corpus that is already mirrored.
    ///
    /// # Errors
    /// [`TriggerError`].
    pub async fn trigger(
        &self,
        source_id: &str,
        trigger: SyncTrigger,
        progress: Option<Arc<dyn ProgressSink>>,
    ) -> Result<i64, TriggerError> {
        self.inner.trigger(source_id, trigger, progress).await
    }

    /// **Backfill** one source: the same run in every respect but one -- the
    /// stored position is not used, so the source is re-read from the top and
    /// every mirrored item is rewritten with whatever the adapter's *current*
    /// query returns. [`crate::run_backfill`] carries the reasoning, including
    /// why it does not sweep.
    ///
    /// It goes through the scheduler rather than around it because everything
    /// the scheduler does for a run matters more here, not less: it is the
    /// longest run a source ever does. The concurrency permit, the in-flight
    /// dedupe (a double-click must not start two full re-reads), the
    /// `sync_run` row the diagnostics view shows, the backoff and credential
    /// health `settle` applies, and above all the run's **own** connection
    /// (§10.6(c)) -- a full re-read on a pooled connection is exactly the
    /// stall that requirement exists to prevent.
    ///
    /// Logged under [`SyncTrigger::Backfill`], its own spelling in
    /// `sync_run_trigger_chk` since migration 0004 -- and that spelling is
    /// what *puts* the run in [`RunMode::Backfill`], so no caller can produce
    /// one without the other. Not `Manual`, though only
    /// a person starts one: a backfill is the single run mode that is
    /// **forbidden to reconcile**, so it is the run whose `swept` count is
    /// always `0` by construction, and the one question anybody asks of a
    /// surprising tombstone count in the diagnostics list is which run
    /// produced it. Logged as `Manual` a backfill is indistinguishable from
    /// *Sync now*, and that question has no answer.
    ///
    /// # Errors
    /// [`TriggerError`].
    pub async fn backfill(&self, source_id: &str) -> Result<i64, TriggerError> {
        self.inner
            .trigger(source_id, SyncTrigger::Backfill, None)
            .await
    }

    /// Trigger every enabled source that does not need a human, id order.
    ///
    /// # Errors
    /// [`TriggerError`].
    pub async fn trigger_all(&self) -> Result<Vec<i64>, TriggerError> {
        if self.inner.cancel.is_cancelled() {
            return Err(TriggerError::ShuttingDown);
        }
        let mut ids = Vec::new();
        for cfg in config::list(&self.inner.deps.pool).await? {
            let needs_human = matches!(
                cfg.health.state,
                AuthState::Unauthorized | AuthState::MissingSecret
            );
            if !cfg.enabled || needs_human {
                continue;
            }
            ids.push(
                self.inner
                    .trigger(&cfg.id, SyncTrigger::Manual, None)
                    .await?,
            );
        }
        Ok(ids)
    }

    /// Drop everything this scheduler remembers about a source, because the
    /// source is gone.
    ///
    /// **Where a [`RunEntry`]'s life ends.** The entry outliving its *run* is
    /// deliberate ([`Claims::runs`]); outliving its *source* is not, and nothing
    /// else would ever notice, because `knobas.sync_run` has no foreign key to
    /// `source_config` on purpose -- deleting a source must not rewrite its
    /// history -- so a run of the deleted source is still readable under an id
    /// a *new* source may now hold. Without this, adding a source back under a
    /// deleted one's id could hand the first-run wizard the earlier source's
    /// run and the ending recorded for it.
    ///
    /// Called by `delete_source`, which is the only place a source is deleted.
    ///
    /// **It cancels nothing.** A run of the deleted source that is still in
    /// flight keeps its own handle on its watchers and closes them itself, so a
    /// caller enrolled before the deletion is still told how that run ended --
    /// ADR-0005 is about the caller, not about the configuration row. What goes
    /// is only this scheduler's claim on the *id*, so the next trigger for it
    /// is about whatever holds that id now.
    ///
    /// # The purge outlives the delete, because the run does (#127)
    ///
    /// `purge` is what `delete_source` was asked to do with the mirror, and it
    /// is here because the delete's own purge is **not** the last word. A run
    /// only checks that its source still exists once, at the top of
    /// `run_locked`, before a byte of network traffic; after that it fetches
    /// for as long as the remote system takes and then commits. A delete that
    /// commits anywhere in that window purges a mirror the run is about to
    /// write back -- and the entity upsert's `deleted_at = excluded.deleted_at`
    /// clears the tombstone the purge set, so the items the user deleted come
    /// back **live in `sync.live_item`**, for a source with no configuration
    /// row and nothing left that will ever sync or tombstone them again.
    ///
    /// So, under the one lock ([`Claims`]) and against the run's own state:
    ///
    /// * **the run is still going** -- arm the purge. The run applies it when
    ///   it settles, after its transaction has committed, and before it tells
    ///   anybody it is over.
    /// * **the last run of this id is over** -- apply the purge here. Its
    ///   commit may have landed after `delete_source`'s, in which case those
    ///   rows are sitting in the mirror right now. Cheap and unconditional
    ///   rather than conditioned on comparing two commit times, which nothing
    ///   here can do honestly.
    /// * **this scheduler has no run of the id at all** -- nothing to do. No
    ///   run exists that could write, and none can start: `trigger` reads
    ///   `source_config` under this same lock and answers
    ///   [`TriggerError::UnknownSource`] for a source that is gone.
    ///
    /// **It still cancels nothing**, and the re-applied purge never touches
    /// [`Watchers`]: the enrolled caller is told how the run really ended, and
    /// the ending is about the *run* (it did upsert N items) while the purge is
    /// about the *mirror*.
    ///
    /// # Neither, if a source holds the id again (#154)
    ///
    /// Both branches above are about a source that is *gone*, and
    /// `delete_source`'s three steps are not atomic together: it purges and
    /// commits, deletes the keychain item, and only then calls this. An
    /// `add_source` for the same id that commits inside that window has already
    /// called [`source_added`](Self::source_added) -- the door that voids an
    /// armed purge -- by the time this runs, so nothing downstream would clear
    /// what is armed here and the new source's first sync goes when the old
    /// run settles. And it does not heal: [`config::purge_items`] takes
    /// `sync.item` rows and tombstones entities, never touching the cursor on
    /// `knobas.source_config`, so the new source is left with a cursor advanced
    /// past a corpus that is gone until somebody orders a backfill by hand.
    ///
    /// So before either destructive act, and under this same lock, this asks
    /// whether a source exists under the id *now*
    /// ([`a_source_holds_the_id_again`]). One does: the user's newest
    /// instruction about the id wins, exactly as `source_added` already rules,
    /// and neither branch is taken.
    ///
    /// **`claims.runs.remove` stays unconditional**, above the question. #119's
    /// rule is that an entry never outlives the source it was made for, and
    /// unconditional removal is what the sequential order -- delete finishes,
    /// then add -- produces anyway. The residual, in the race only: a fresh
    /// entry belonging to the new source's already-started run can be stripped,
    /// costing a duplicated run (which the advisory lock serialises) or a
    /// wizard that starts a sync of its own instead of being served an ending.
    /// No data is lost, and it is the same shape as the pre-existing #119
    /// window.
    pub async fn forget_source(&self, source_id: &str, purge: Purge) {
        // Held across the purge, not dropped before it. Between a release and
        // the statement, `add_source` plus a wake could start a run for a
        // source re-created under this id, and the purge would take the *new*
        // source's first mirror with it. `trigger` already holds this lock
        // across its own database work, so this is the shape of the lock, not a
        // new one.
        let mut claims = self.inner.claims.lock().await;
        let entry = claims.runs.remove(source_id);
        if purge == Purge::Keep {
            return;
        }
        // **Does a source hold this id right now?** (#154) The delete's own
        // steps are not atomic together: `delete_source` purges and commits,
        // deletes the keychain item, and only then arrives here. An
        // `add_source` for the same id that commits inside that window has
        // already run [`source_added`](Self::source_added) -- the door that
        // clears an armed purge -- so nothing downstream would clear the intent
        // armed below, and the source the user has just created loses its first
        // sync when the old run settles. Asked here, under the same lock, for
        // the same reason `source_added` takes it: `add_source` calls
        // `source_added` only after `crud::add` committed, so every add orders
        // one of two ways against this critical section -- its `source_added`
        // completed first, and this read sees the row it committed before that;
        // or it runs after, and clears what this armed. There is no third
        // interleaving in which the new source's data is at stake, because no
        // run of it can start inside the window: `trigger` reads
        // `source_config` under this same lock.
        let found = config::get(&self.inner.deps.pool, source_id)
            .await
            .map(|row| row.is_some());
        if a_source_holds_the_id_again(&found, source_id) {
            return;
        }
        // `attach(None)` asks the run's own state whether it is still open, and
        // it is the only honest way to ask: see [`Watchers::attach`]. The run
        // claims its purge and closes its watchers under *this* lock, so
        // `Joined` here means the claim has not happened yet and will.
        match entry {
            Some(entry) if entry.watchers.attach(None) == Attach::Joined => {
                claims.pending_purges.insert(source_id.to_owned());
            }
            Some(_) => purge_again(&self.inner.deps, source_id).await,
            None => {}
        }
    }

    /// A source now exists under this id, so nothing this scheduler still meant
    /// to do to the *old* one's mirror applies any more.
    ///
    /// Called by `add_source`. The race it closes is delete-then-add under one
    /// id while the deleted source's run is still in flight: the purge armed by
    /// [`forget_source`](Self::forget_source) would otherwise fire after the
    /// new source's first sync and take that sync's items with it. The user's
    /// newest instruction about the id wins.
    ///
    /// **What stays accepted**, because clearing the intent is what accepts it:
    /// the old run's late commit merges into the re-added source's mirror,
    /// under item ids the old source minted. It is the narrow window "delete,
    /// add again under the same id, old sync still running", the rows are the
    /// same shape the new source writes, and the new source's next full sync
    /// reconciles them.
    pub async fn source_added(&self, source_id: &str) {
        self.inner
            .claims
            .lock()
            .await
            .pending_purges
            .remove(source_id);
    }

    /// Look for due sources now rather than at the next tick.
    pub fn wake(&self) {
        self.inner.wake.notify_one();
    }

    /// Stop ticking, cancel every run, and close the scheduler's pool --
    /// **bounded**, so quitting during a 30-second remote call is not a
    /// 30-second hang (carry-over).
    ///
    /// Idempotent: `RunEvent::ExitRequested` can fire more than once.
    pub async fn shutdown(&self) {
        self.inner.cancel.cancel();

        let handles = std::mem::take(&mut *self.inner.tasks.lock().await);
        let aborts: Vec<_> = handles
            .iter()
            .map(tokio::task::JoinHandle::abort_handle)
            .collect();
        let joined = async {
            for handle in handles {
                let _ = handle.await;
            }
        };
        if tokio::time::timeout(SHUTDOWN_GRACE, joined).await.is_err() {
            tracing::warn!("a sync run did not stop within the grace period: aborting it");
            for abort in aborts {
                abort.abort();
            }
        }

        // Only now, with every run's task stopped: `close()` waits for
        // in-flight queries, and waiting for one parked on a remote system is
        // exactly the stall this ordering removes.
        if tokio::time::timeout(POOL_CLOSE_TIMEOUT, self.inner.deps.pool.close())
            .await
            .is_err()
        {
            tracing::warn!("the sync pool did not close in time; the postmaster will reap it");
        }
    }
}

impl Inner {
    async fn trigger(
        self: &Arc<Self>,
        source_id: &str,
        trigger: SyncTrigger,
        progress: Option<Arc<dyn ProgressSink>>,
    ) -> Result<i64, TriggerError> {
        // Derived, never passed alongside: see `impl From<SyncTrigger> for
        // RunMode`. This is what makes "the log says `backfill`" and "the run
        // was forbidden to sweep" the same statement.
        let mode = RunMode::from(trigger);
        if self.cancel.is_cancelled() {
            return Err(TriggerError::ShuttingDown);
        }
        // *This* caller is asking about the source's first sync rather than for
        // a sync: the spelling says which, and the channel says there is
        // somebody to answer. See [`RunEntry`] for why it is spent on the live
        // join as well as on the served-from-record one.
        let asks_for_the_first_sync = trigger == SyncTrigger::FirstRun && progress.is_some();
        // The whole check-and-claim under one lock: two `sync_now` calls
        // arriving together must not both decide the source is idle.
        let mut claims = self.claims.lock().await;
        if let Some(entry) = claims.runs.get_mut(source_id) {
            let run_id = entry.watchers.run_id();
            // Enrolled or not, under the run's own lock -- so a sink offered a
            // microsecond before the ending still hears it, and one offered a
            // microsecond after is told so rather than enrolled into a run that
            // has already said its last word.
            if entry.watchers.attach(progress.clone()) == Attach::Joined {
                // Enrolment *is* the answer to "what happened to this source's
                // first sync?", so it spends the claim: whoever asks next --
                // the wizard's *Retry*, which is the same command asking a
                // second time -- gets work rather than this run again.
                entry.first_run_claimed |= asks_for_the_first_sync;
                return Ok(run_id);
            }
            let unclaimed = !entry.first_run_claimed;
            // That run is over, so the entry has no claim on the source any
            // more and the next trigger must not find it.
            claims.runs.remove(source_id);
            // ...but this trigger may still want it, if nobody spent the claim
            // while the run was going. A caller asking for the source's *first
            // sync* is asking about a job, not for a job: hand it that run and
            // the ending the log recorded for it. Every other trigger --
            // *Sync now*, the ticker, a backfill -- wants work done, and falls
            // through to start a run of its own.
            if asks_for_the_first_sync
                && unclaimed
                && let Some(sink) = progress.as_ref()
                && let Some(ending) = ending_of_record(&self.deps.pool, source_id, run_id).await?
            {
                // Through `deliver` like every other delivery in this crate:
                // this one runs in the caller's own stack and under the `runs`
                // lock, so an uncontained panic here unwound straight out of
                // `trigger` into the command that called it -- and it is the
                // ADR-0005 path, which is the last one that should be the
                // exception.
                crate::progress::deliver(run_id, sink.as_ref(), ending);
                return Ok(run_id);
            }
        }
        if config::get(&self.deps.pool, source_id).await?.is_none() {
            return Err(TriggerError::UnknownSource(source_id.to_owned()));
        }
        let run_id = run_log::start(&self.deps.pool, source_id, trigger).await?;
        let watchers = Watchers::for_run(run_id);
        // The caller that started the run is a watcher like any other; nothing
        // below this line knows which of them it was.
        watchers.attach(progress);
        claims.runs.insert(
            source_id.to_owned(),
            RunEntry {
                watchers: Arc::clone(&watchers),
                // Starting the run is being served the first sync too: the
                // wizard that got here first is watching the run it asked for,
                // and its *Retry* must not be handed this one back.
                first_run_claimed: asks_for_the_first_sync,
            },
        );
        drop(claims);

        // **Before the spawn, and therefore before this returns.** `sync_now`
        // hands the caller a run id (P3) and the docs promise `sync:state` says
        // `running` for it; emitting from inside the spawned task would make
        // that promise a race the frontend loses on a busy machine. The row
        // exists, so the status read is accurate -- including for a run that is
        // still waiting on a permit, which *is* running as far as the UI is
        // concerned.
        emit_state(&self.deps, source_id).await;

        let inner = Arc::clone(self);
        let id = source_id.to_owned();
        let handle = tokio::spawn(async move { inner.run_task(id, run_id, mode, watchers).await });
        let mut tasks = self.tasks.lock().await;
        tasks.retain(|task| !task.is_finished());
        tasks.push(handle);
        Ok(run_id)
    }

    /// One spawned run: take a permit, do the work (or give up when cancelled),
    /// settle, tell everyone watching how it ended -- which is also what
    /// releases the source.
    async fn run_task(
        self: Arc<Self>,
        source_id: String,
        run_id: i64,
        mode: RunMode,
        watchers: Arc<Watchers>,
    ) {
        // The permit is what caps concurrency. Acquired *after* the log row
        // exists, so a queued run is visible as "running" in the UI rather
        // than as nothing at all.
        // Started when the task did, not when it got a permit: a run queued
        // behind the concurrency cap has been waiting, and the bar should say
        // so.
        let started = std::time::Instant::now();
        // Closes the watchers however this task ends. `Watchers::close` is
        // first-call-wins, so the real ending below still wins on every
        // ordinary path; this one fires only when the task never reached it --
        // a panicking adapter, or the abort `shutdown` falls back on. Without
        // it a caller holding that run's id waits for ever, and the source is
        // never released: the same failure ADR-0005 is about, arriving through
        // a door the happy path cannot see.
        let _closing = Closing {
            watchers: Arc::clone(&watchers),
            source_id: source_id.clone(),
            run_id,
            started,
        };
        let permit = tokio::select! {
            biased;
            () = self.cancel.cancelled() => None,
            permit = self.permits.acquire() => permit.ok(),
        };

        let result = match permit {
            None => cancelled_result(),
            Some(_permit) => {
                tokio::select! {
                    biased;
                    () = self.cancel.cancelled() => cancelled_result(),
                    result = execute_run(
                        &self.deps,
                        &source_id,
                        run_id,
                        mode,
                        Some(Arc::clone(&watchers) as Arc<dyn ProgressSink>),
                        started,
                    ) => result,
                }
            }
        };
        // Dropping the run future above is what rolls its transaction back and
        // closes its connection -- which is why `settle` can still write.

        settle(&self.deps, &source_id, run_id, &result).await;
        let ending = ending(
            run_id,
            &source_id,
            result.outcome,
            result.error.clone(),
            u64::try_from(result.counts.upserted).unwrap_or(0),
            elapsed_ms(started),
        );
        {
            // **The purge the delete could not finish (#127).** This run's
            // transaction has committed by now -- dropping the run future
            // above is what settles it either way -- so if the source was
            // deleted with `purge_items` while this run was fetching, the rows
            // it just wrote are the ones that undo the purge, tombstones and
            // all. Applied *before* the ending, so a caller that has been told
            // the run is over is looking at a mirror this run no longer owns.
            let mut claims = self.claims.lock().await;
            if claims.pending_purges.remove(&source_id) {
                purge_again(&self.deps, &source_id).await;
            }
            // The ending, and with it the release of the source: a trigger that
            // finds these watchers closed knows the run is over and may start
            // one of its own. Sent *after* `settle`, so the row a late arrival
            // reads says the same thing this message does.
            //
            // `started`, not `0` -- the terminal message is the one a progress
            // bar shows as the run's duration, and a hardcoded zero made it
            // report every run as instantaneous.
            //
            // Under the same lock as the claim above, and that is the whole of
            // what makes the arming race-free: `forget_source` decides between
            // arming a purge and applying one by asking whether these watchers
            // are still open. Split the claim from the close and a delete
            // landing between them sees an open run, arms a purge, and nothing
            // ever claims it.
            watchers.close(ending);
        }
        self.wake.notify_one();
    }
}

/// What a run whose task died without settling is reported as.
///
/// Not a lie about the data: the run's transaction went with the task, so
/// nothing it had fetched landed. It is a lie about nothing else either --
/// there is no outcome recorded for such a run, which is exactly what the
/// sentence says.
const RUN_STOPPED_MESSAGE: &str = "the run stopped without recording an outcome";

/// Closes a run's watchers if its task ends without doing so itself.
///
/// A guard rather than a `catch_unwind` or a second code path: this is the only
/// construction that also covers the abort in [`Scheduler::shutdown`], and it
/// cannot be forgotten by a later edit to `run_task` the way a line at the
/// bottom of the function can.
///
/// The panicking-adapter case runs this drop *during an unwind*, where a second
/// panic escaping it aborts the process. [`Watchers::close`] is written not to
/// panic for that reason -- it contains a misbehaving sink rather than letting
/// it out -- so this guard is safe on the very path it exists for.
struct Closing {
    watchers: Arc<Watchers>,
    source_id: String,
    run_id: i64,
    started: std::time::Instant,
}

impl Drop for Closing {
    fn drop(&mut self) {
        self.watchers.close(ending(
            self.run_id,
            &self.source_id,
            SyncOutcome::Error,
            Some(RUN_STOPPED_MESSAGE.to_owned()),
            0,
            elapsed_ms(self.started),
        ));
    }
}

/// The verdict for a run the user's quit interrupted.
///
/// Logged as an error rather than silently dropped: the diagnostics view is
/// where "why is there a gap in my sync history" gets answered.
fn cancelled_result() -> RunResult {
    RunResult {
        outcome: SyncOutcome::Error,
        counts: run_log::RunCounts::default(),
        error: Some("cancelled: knobas is shutting down".to_owned()),
    }
}

/// Look for due sources every [`TICK`], or whenever something pokes `wake`.
async fn tick_loop(inner: Arc<Inner>) {
    tokio::select! {
        () = inner.cancel.cancelled() => return,
        () = tokio::time::sleep(STARTUP_DELAY) => {}
    }

    loop {
        match config::due(&inner.deps.pool).await {
            Ok(due) => {
                for source in due {
                    let trigger = if source.first_run {
                        SyncTrigger::FirstRun
                    } else {
                        SyncTrigger::Schedule
                    };
                    // `UnknownSource` here means the source was deleted between
                    // the query and the claim: nothing to report.
                    if let Err(error) = inner.trigger(&source.id, trigger, None).await {
                        match error {
                            TriggerError::ShuttingDown => return,
                            other => {
                                tracing::warn!(source_id = %source.id, %other, "could not start a due run");
                            }
                        }
                    }
                }
            }
            Err(error) => tracing::warn!(%error, "looking for due sources failed"),
        }

        tokio::select! {
            biased;
            () = inner.cancel.cancelled() => return,
            () = inner.wake.notified() => {}
            () = tokio::time::sleep(TICK) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two states the status read excludes from `next_run_at` are the two
    /// [`config::due`] excludes from scheduling, spelled the same way. A
    /// countdown to a run the scheduler will never start is a lie the sources
    /// view would render, and neither spelling is checked by any Rust type.
    #[test]
    fn the_states_with_no_next_run_are_spelled_the_way_the_enum_spells_them() {
        for state in [AuthState::Unauthorized, AuthState::MissingSecret] {
            assert!(
                STATUS.contains(&format!("'{}'", state.as_str())),
                "{state:?} needs a human, so it has no next run: {STATUS}"
            );
        }
        for state in [AuthState::Ok, AuthState::Unreachable, AuthState::Unknown] {
            assert!(
                !STATUS.contains(&format!("'{}'", state.as_str())),
                "{state:?} is retryable, so it still counts down"
            );
        }
    }

    /// The cap must stay strictly below the pool: the scheduler's bookkeeping
    /// runs while every permitted run is out, and a scheduler that cannot
    /// record what it is doing is worse than a slow one.
    #[test]
    fn the_concurrency_cap_leaves_the_bookkeeper_a_connection() {
        assert!(
            SYNC_CONCURRENCY < usize::try_from(SYNC_POOL_SIZE).unwrap(),
            "{SYNC_CONCURRENCY} runs against a pool of {SYNC_POOL_SIZE}"
        );
    }

    /// **A database blip is not a reason to skip a purge** (#154).
    ///
    /// The guard in [`Scheduler::forget_source`] exists to spare a source the
    /// user added back under a deleted one's id. It must not spare the deleted
    /// source itself when the read simply fails: `Err` alone is far commoner
    /// than `Err` *plus* the re-add race, and treating it as "a source exists"
    /// would leave a deleted source's items live in `sync.live_item` for ever
    /// -- #127's symptom, reopened by the fix for #154.
    ///
    /// Pinned here rather than in an integration test because no integration
    /// test can reach this arm: it needs `config::get` to fail while
    /// `config::purge_items` still works, and one pool cannot produce that.
    #[test]
    fn a_failed_existence_check_purges_rather_than_skipping() {
        assert!(
            a_source_holds_the_id_again(&Ok(true), "s"),
            "a source exists under the id again: the newest instruction about \
             it wins and neither destructive act happens"
        );
        assert!(
            !a_source_holds_the_id_again(&Ok(false), "s"),
            "the id is free, which is what the delete expects: purge"
        );
        assert!(
            !a_source_holds_the_id_again(&Err(sqlx::Error::PoolClosed), "s"),
            "the read failed, so nothing here knows a source exists; purging \
             as the delete asked is the arm that keeps #127 closed"
        );
    }

    /// The ticker's period has to be well inside the shortest schedule a user
    /// can configure, or a source is late by up to one tick every time.
    #[test]
    fn the_tick_is_far_shorter_than_the_shortest_configurable_interval() {
        assert!(
            TICK.as_secs() * 4 <= u64::from(config::MIN_SYNC_INTERVAL_SECS),
            "a {}s tick against a {}s floor",
            TICK.as_secs(),
            config::MIN_SYNC_INTERVAL_SECS
        );
    }
}
