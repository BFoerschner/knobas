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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use chrono::{DateTime, Utc};
use knobas_core::activity::ActivityRow;
use knobas_secrets::SecretStore;
use knobas_source::instance::SourceInstance;
use knobas_source::{Source, SourceDescriptor, SourceError};
use sqlx::{PgConnection, PgPool};
use tokio::sync::{Mutex, Notify, RwLock, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::background::Background;
use crate::config::{self, AuthState, CredentialHealth};
use crate::progress::{Attach, Observed, ProgressSink, SyncPhase, SyncProgress, Watchers};
use crate::run_log::{self, RunResult, SyncOutcome, SyncTrigger};
use crate::{SyncError, run_backfill, run_from_stored_cursor};

pub use crate::run_log::SourceSyncStatus;

/// Turns a stored configuration into a live adapter, and answers what each
/// adapter kind declares.
///
/// Implemented in `knobas-app` over the compiled-in adapter table and the
/// installed external adapters (P6: every adapter crate exposes
/// `descriptor_template()` and `build(SourceInstance)`). Everything a read
/// takes from the registry is here, so a read can take `&dyn AdapterRegistry`
/// and a test can hand it a registry of its own (#1321). A read that builds an
/// adapter takes `&Arc<dyn AdapterRegistry>`, for [`build_off_runtime`].
pub trait AdapterRegistry: Send + Sync + 'static {
    /// One descriptor **template** per adapter kind the registry holds, with
    /// `id == adapter_kind`: the kinds, declared paths, roles and write ops
    /// of each. What the Add-source form is generated from, and what the
    /// reads take kind metadata from, without instantiating an adapter or
    /// touching the keychain.
    fn descriptors(&self) -> Vec<SourceDescriptor>;

    /// The Docs projection of an adapter kind (#859), `None` for a kind that
    /// draws no pages or that the registry does not hold.
    fn doc_projection(&self, _kind: &str) -> Option<knobas_source::docs::DocProjection> {
        None
    }

    /// The Docs writes of an adapter kind (#862), `None` for a kind knobas
    /// writes no pages for or that the registry does not hold.
    fn doc_writes(&self, _kind: &str) -> Option<knobas_source::docs::DocWrites> {
        None
    }

    /// The registry itself, for a caller that must reach its concrete type:
    /// the install flow adds, replaces and removes adapters, which the trait
    /// has no method for (#1323). `None` for a registry that is not meant to
    /// be installed into, which every test registry is.
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        None
    }

    /// # Errors
    /// [`SourceError::Protocol`] if no adapter answers to the instance's
    /// `kind`, or if the adapter rejected the configuration.
    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError>;
}

/// [`AdapterRegistry::build`] on the blocking pool. Every caller on the
/// runtime builds an adapter through here (#1705, ruling D9).
///
/// `build` is synchronous, and the `Source` SPI keeps it so. For an external
/// adapter it instantiates the component and runs its `descriptor()` on a
/// thread of its own, then joins that thread (ADR-0028). A caller on a
/// runtime worker would hold the worker for as long as the component takes,
/// which only the adapter's deadline bounds (600 s by default). A compiled-in
/// adapter's build is a constructor, so the hop is its only cost.
///
/// A build that panicked, or that the runtime's shutdown cancelled before it
/// started, is a [`SourceError::Protocol`] with no status: the adapter's own
/// fault, which IPC answers as `internal` and a run logs as `error`. A caller
/// dropped while the build runs does not cut it off. The build finishes on its
/// blocking thread and the adapter it made is dropped there.
///
/// # Errors
/// What [`AdapterRegistry::build`] answers, or the protocol error above.
pub async fn build_off_runtime(
    registry: &Arc<dyn AdapterRegistry>,
    instance: SourceInstance,
) -> Result<Box<dyn Source>, SourceError> {
    let registry = Arc::clone(registry);
    tokio::task::spawn_blocking(move || registry.build(instance))
        .await
        .map_err(|error| {
            SourceError::protocol(format!("building the adapter did not finish: {error}"))
        })?
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
    /// When the ticker looks. [`SchedulerTiming::default()`] everywhere a
    /// user runs this; the scheduler-loop suite hands in milliseconds so it
    /// does not wait out the production clock (#412).
    pub timing: SchedulerTiming,
    /// The quit (#1924). [`Scheduler::start`] makes it the scheduler's
    /// cancel, so [`Scheduler::shutdown`] is what begins it, and every flush
    /// handed these deps stops between writes from then on.
    pub quit: Quit,
}

/// Whether the quit has begun, carried on [`SchedulerDeps`] (#1924).
///
/// Every write queue flush takes the deps, so every flush reads this:
/// the scheduler's flush task, a submit, a worklog, *Flush now*, *Send mine
/// anyway*, *Edit and send*. One that took a stop of its own could be handed
/// one that never fires, and the background send, *Flush now* and the
/// foreground submits were: each went on starting writes after the quit's
/// cancel. A comment submitted once the quit had begun was sent during it,
/// and one still on the wire at the grace's end was aborted with its row
/// pending, so the next start sent it again.
///
/// It also keeps the writes the quit **cut on the wire** (#1932): a send the
/// grace ran out on, aborted before its outcome was recorded. The shutdown
/// holds each before the pool closes, as a send that got no answer is held
/// (#1125), so the next start does not send it again blind.
///
/// Nothing outside this crate can begin it: a caller that wants the quit
/// calls [`Scheduler::shutdown`]. Deps no scheduler was started with keep a
/// quit that never begins, which is right, since nothing quits them.
#[derive(Debug, Default)]
pub struct Quit {
    token: CancellationToken,
    cut: std::sync::Mutex<Vec<knobas_core::write_queue::QueuedWrite>>,
}

impl Quit {
    /// Whether [`Scheduler::shutdown`] has begun.
    pub(crate) fn has_begun(&self) -> bool {
        self.token.is_cancelled()
    }

    /// The token itself: cancelled by [`Scheduler::shutdown`], selected
    /// against between writes.
    pub(crate) fn token(&self) -> &CancellationToken {
        &self.token
    }

    /// Keep `write` as one the quit cut on the wire.
    pub(crate) fn keep_cut(&self, write: knobas_core::write_queue::QueuedWrite) {
        self.cut
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(write);
    }

    /// Every write the quit cut on the wire so far, taken.
    pub(crate) fn take_cut(&self) -> Vec<knobas_core::write_queue::QueuedWrite> {
        std::mem::take(
            &mut *self
                .cut
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }
}

/// The ticker's clock: how long it waits before its first look for due
/// sources, how long between looks after that, and how long a quit waits for
/// what is still running.
///
/// A value on [`SchedulerDeps`] rather than constants read directly, so a
/// test can drive the loop at millisecond ticks, and can give a quit a grace
/// longer than its own patience (#1920): a quit that then returns at all has
/// waited for nothing, which no clock reading can say. The defaults are the
/// production numbers ([`STARTUP_DELAY`], [`TICK`], [`SHUTDOWN_GRACE`]) and
/// nothing in the shipped binary constructs anything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedulerTiming {
    /// Before the first look. See [`STARTUP_DELAY`] for the two jobs it does.
    pub startup_delay: Duration,
    /// Between looks. See [`TICK`] for why it is fixed rather than computed.
    pub tick: Duration,
    /// From the quit's cancel to the abort of what is still running. See
    /// [`SHUTDOWN_GRACE`].
    pub shutdown_grace: Duration,
}

impl Default for SchedulerTiming {
    fn default() -> Self {
        SchedulerTiming {
            startup_delay: STARTUP_DELAY,
            tick: TICK,
            shutdown_grace: SHUTDOWN_GRACE,
        }
    }
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
       -- already describe that finished run, so leaving its id out would leave
       -- the listing saying that a source last ran without saying which run
       -- that was. `running` is what says which of the two this is.
       --
       -- This is the **listing's** answer, and only the listing's (#304). It
       -- is by construction about the source rather than about any one run --
       -- `r` takes whichever run is open, `f` whichever finished one started
       -- last -- so an *event* about a particular run cannot be read out of
       -- it; `emit_state` composes that from the run itself.
       coalesce(r.id, f.id) as run_id,
       r.started_at,
       f.finished_at as last_finished_at,
       f.outcome     as last_outcome,
       c.backoff_until,
       -- Whether that finished run wrote anything (#759; `SyncRunRow::changed`
       -- is the same sum); null with no run.
       (f.upserted + f.deleted + f.swept) > 0 as last_changed,
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
      select id, finished_at, outcome, upserted, deleted, swept from knobas.sync_run
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
    last_changed: Option<bool>,
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
            last_changed: r.last_changed,
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
    /// The sync failed on a send knobas' own rate limit held past the send
    /// budget, as [`knobas_http::watch_local_quota`] saw it (#1980). Logged
    /// [`run_log::LOCAL_QUOTA_PREFIX`] first, which is what keeps the
    /// backoff off it.
    LocalQuota(SyncError),
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
            RunFailure::Source(SourceError::Unreachable(_) | SourceError::Unconfirmed { .. }) => {
                SyncOutcome::Unreachable
            }
            RunFailure::NotConfigured
            | RunFailure::Secret(_)
            | RunFailure::Source(_)
            | RunFailure::LocalQuota(_)
            | RunFailure::Db(_) => SyncOutcome::Error,
        }
    }

    pub(crate) fn message(&self) -> String {
        match self {
            RunFailure::NotConfigured => "the source has no configuration row".to_owned(),
            RunFailure::MissingSecret => MISSING_SECRET_MESSAGE.to_owned(),
            RunFailure::Secret(detail) => {
                format!(
                    "{} {detail}",
                    knobas_core::write_queue::KEYCHAIN_DETAIL_PREFIX
                )
            }
            RunFailure::Source(error) => error.to_string(),
            RunFailure::Sync(error) => error.to_string(),
            RunFailure::LocalQuota(error) => format!("{}{error}", run_log::LOCAL_QUOTA_PREFIX),
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
    let instance = instance_for(deps, cfg).await?;
    build_off_runtime(&deps.registry, instance)
        .await
        .map_err(RunFailure::Source)
}

/// What [`build_source`] builds from: the stored configuration and its secret.
/// The keychain read is here, so a run that builds twice reads it once.
async fn instance_for(
    deps: &SchedulerDeps,
    cfg: &config::SourceConfigRow,
) -> Result<SourceInstance, RunFailure> {
    // One keychain read, both halves. The account is the optional second
    // credential of envelope version 2 (issue #452) and rides in the same
    // item, so asking for it is not a second trip to a store that may be
    // locked -- and a source configured with an account gets the same instance
    // whether a sync or a flush built it.
    let (secret, account) = match cfg.auth_kind.method() {
        // A source that needs no credential at all: `--demo` must work against
        // an empty keychain (§14a), and the keychain is not even asked.
        None => (None, None),
        Some(_) => {
            let stored = knobas_secrets::spawn::get(
                &deps.secrets,
                &knobas_secrets::KeychainAccount::source(&cfg.id),
            )
            .await
            // The store's own words; `message` says they are the keychain's
            // (#1946).
            .map_err(|e| RunFailure::Secret(e.to_string()))?;
            let stored = stored.ok_or(RunFailure::MissingSecret)?;
            (Some(stored.value), stored.account)
        }
    };
    Ok(SourceInstance {
        id: cfg.id.clone(),
        kind: cfg.adapter_kind.clone(),
        display_name: cfg.display_name.clone(),
        base_url: cfg.base_url.clone(),
        auth: cfg.auth_kind.method(),
        secret,
        account,
        config: cfg.config.clone(),
    })
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
    /// yet, not because the run asked for that). The one exception is a run
    /// that records a discovered field id: `attempt` relabels it a backfill
    /// first and then runs it as one (#889).
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
    /// below the log and the trigger does not reach it. `attempt` turns an
    /// incremental run into a backfill only after the log row says so
    /// (`run_log::relabel_as_backfill`, #889), so the pairing still holds.
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

    tracing::info!(source_id, run_id, ?mode, "sync started");
    let result = match attempt(deps, source_id, run_id, mode, progress.as_ref(), started).await {
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
    };
    // Duration here rather than in `settle`: `started` is the user's wait,
    // the same clock every progress message uses.
    tracing::info!(
        source_id,
        run_id,
        outcome = ?result.outcome,
        duration_ms = elapsed_ms(started),
        "sync finished"
    );
    result
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
    crate::progress::deliver(
        run_id,
        sink.as_ref(),
        SyncProgress {
            run_id,
            source_id: source_id.to_owned(),
            phase,
            items,
            elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            message,
            walked: None,
            of: None,
            unit: None,
        },
    );
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
        walked: None,
        of: None,
        unit: None,
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

/// The config properties an adapter discovers from its far end and that a
/// reader should never have to type (#297 Epic Link, #811 Sprint).
const DISCOVERABLE: [&str; 2] = ["epic_link_field", "sprint_field"];

/// How long a sync waits for the field discovery before it goes on without it.
///
/// Discovery is three reads on a healthy instance. On one that is down, the
/// first of them would spend the HTTP client's whole retry budget (45 s)
/// before the sync itself spends another, so a failing run would take twice
/// as long. Past this the run syncs as it would have, and asks again next time.
const DISCOVERY_BUDGET: Duration = Duration::from_secs(10);

/// The discoverable keys a far end answered and had nothing for, in this
/// process (#1702, ruling D7).
///
/// A Jira without Jira Software has no Epic Link or Sprint field, and without
/// this every scheduled run asked it again: a *Test connection* (three reads,
/// up to [`DISCOVERY_BUDGET`]) each time. Kept in memory, not stored: a
/// restart asks once more, which is the price of needing no migration (D7).
/// Only an answer is remembered. A refusal or a timeout is asked again next
/// run, since the far end may let that run in.
///
/// Keyed by [`FarEnd`], so a source pointed somewhere else is asked again. A
/// credential changed in the keychain is not part of the key, so a source
/// whose new credential could see the field learns it at the next restart, or
/// from the reader typing it in. A handful of entries per source at most, so
/// nothing is evicted.
static ANSWERED_NOTHING: std::sync::LazyLock<std::sync::Mutex<HashSet<(FarEnd, &'static str)>>> =
    std::sync::LazyLock::new(Default::default);

/// What decides a discovery's answer: the source, its adapter, its address
/// and its config **without** the discoverable keys, so recording one key does
/// not make the other look like a new question.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FarEnd {
    source_id: String,
    adapter_kind: String,
    base_url: String,
    config: String,
}

impl FarEnd {
    fn of(cfg: &config::SourceConfigRow) -> FarEnd {
        let mut config = cfg.config.clone();
        if let Some(map) = config.as_object_mut() {
            for key in DISCOVERABLE {
                map.remove(key);
            }
        }
        FarEnd {
            source_id: cfg.id.clone(),
            adapter_kind: cfg.adapter_kind.clone(),
            base_url: cfg.base_url.clone(),
            config: config.to_string(),
        }
    }
}

/// [`ANSWERED_NOTHING`], read or written. A poisoned lock is still a set of
/// keys: the worst a half-finished insert leaves is one more question.
fn answered_nothing() -> std::sync::MutexGuard<'static, HashSet<(FarEnd, &'static str)>> {
    ANSWERED_NOTHING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A source saved before its adapter could discover a field id gets that id
/// once, at its next sync (#889).
///
/// Only for keys the adapter's own `config_schema` declares, the stored config
/// leaves empty, and the far end has not already answered nothing for
/// ([`ANSWERED_NOTHING`]). So a source with nothing to find is asked once per
/// process, and an id the reader typed is never asked about, let alone
/// replaced ([`config::fill_missing`] repeats that rule inside the one
/// statement). The discovery is best-effort: a source that fails or times out
/// here ([`DISCOVERY_BUDGET`]) syncs as it did before, and the run reports the
/// real failure itself.
///
/// Returns the ids found for empty keys, or `None` when there is nothing to
/// record, so the caller builds the adapter again only then. Nothing is
/// written here: [`record_discovered`] does that after the run succeeds.
async fn discover(
    deps: &SchedulerDeps,
    cfg: &config::SourceConfigRow,
    source: &dyn Source,
) -> Option<std::collections::BTreeMap<String, String>> {
    let declared = deps
        .registry
        .descriptors()
        .into_iter()
        .find(|d| d.id == cfg.adapter_kind)
        .map(|d| d.config_schema);
    let far_end = FarEnd::of(cfg);
    let wanted: Vec<&'static str> = {
        let answered = answered_nothing();
        DISCOVERABLE
            .into_iter()
            .filter(|key| {
                declared
                    .as_ref()
                    .is_some_and(|schema| schema["properties"].get(*key).is_some())
                    && !config::config_names(&cfg.config, key)
                    && !answered.contains(&(far_end.clone(), *key))
            })
            .collect()
    };
    if wanted.is_empty() {
        return None;
    }
    let found: std::collections::BTreeMap<String, String> =
        match tokio::time::timeout(DISCOVERY_BUDGET, source.test_connection()).await {
            Ok(Ok(info)) => {
                let found: std::collections::BTreeMap<String, String> = info
                    .discovered
                    .into_iter()
                    .filter(|(key, _)| wanted.contains(&key.as_str()))
                    .collect();
                let mut answered = answered_nothing();
                for key in wanted.iter().filter(|key| !found.contains_key(**key)) {
                    answered.insert((far_end.clone(), *key));
                }
                found
            }
            Ok(Err(error)) => {
                tracing::debug!(source_id = %cfg.id, %error, "field discovery skipped");
                return None;
            }
            Err(_) => {
                tracing::debug!(source_id = %cfg.id, "field discovery timed out");
                return None;
            }
        };
    (!found.is_empty()).then_some(found)
}

/// The config the run that records `found` builds its adapter from: the stored
/// one with each found id under its (empty) key.
fn with_discovered(
    config: &serde_json::Value,
    found: &std::collections::BTreeMap<String, String>,
) -> serde_json::Value {
    let mut widened = config.clone();
    if !widened.is_object() {
        widened = serde_json::json!({});
    }
    if let Some(map) = widened.as_object_mut() {
        for (key, id) in found {
            map.insert(key.clone(), serde_json::Value::String(id.clone()));
        }
    }
    widened
}

/// Record the ids a successful run synced with. [`config::fill_missing`]
/// writes a key only where the row is still empty, so an id the reader typed
/// during the run wins and the next run uses it. Best-effort: on a failure the
/// next run discovers again.
async fn record_discovered(
    deps: &SchedulerDeps,
    source_id: &str,
    found: &std::collections::BTreeMap<String, String>,
) {
    match config::fill_missing(&deps.pool, source_id, found).await {
        Ok(_) => tracing::info!(source_id, keys = ?found.keys().collect::<Vec<_>>(),
            "recorded the field ids the source discovered"),
        Err(error) => {
            tracing::warn!(source_id, %error, "recording discovered field ids failed");
        }
    }
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
    let mut mode = mode;
    // One keychain read for the run, also when the adapter is built twice.
    let instance = instance_for(deps, &cfg).await?;
    let mut source = build_off_runtime(&deps.registry, instance.clone())
        .await
        .map_err(RunFailure::Source)?;
    let discovered = discover(deps, &cfg, source.as_ref()).await;
    if let Some(found) = &discovered {
        let widened = SourceInstance {
            config: with_discovered(&instance.config, found),
            ..instance
        };
        source = build_off_runtime(&deps.registry, widened)
            .await
            .map_err(RunFailure::Source)?;
        // A new field id widens what the adapter keeps of every ticket, and an
        // incremental run re-fetches only what changed upstream, so a ticket
        // nobody touched would keep `sprints: []` for ever. The run that
        // records the id is therefore a backfill (CONTEXT.md), and is logged
        // as one. A source with no stored position reads from the top anyway.
        // The id is recorded only once that run has succeeded, so a run that
        // fails leaves the config as it was and the next one tries again.
        // The log is relabelled first and the mode follows only if that
        // worked, so the two never disagree (see `From<SyncTrigger>`); on a
        // failed relabel the run stays incremental and *Backfill* is the
        // reader's remedy, as it was before #889.
        if mode == RunMode::Incremental && cfg.cursor.is_some() {
            match run_log::relabel_as_backfill(&deps.pool, run_id).await {
                Ok(()) => mode = RunMode::Backfill,
                Err(error) => {
                    tracing::warn!(source_id, run_id, %error, "relabelling the run as a backfill failed");
                }
            }
        }
    }

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
    let observed = progress.map(|sink| {
        Observed::new(
            source.as_ref(),
            source_id,
            run_id,
            started,
            Arc::clone(sink),
        )
    });
    let watched: &dyn Source = match &observed {
        Some(o) => o,
        None => source.as_ref(),
    };
    // Watched for knobas' own rate limit (#1980): a send it held past the
    // budget fails the sync with a status-less `Protocol`, which is also what
    // an adapter's own refusal looks like, and only the mark tells them apart.
    // The sync alone: the adapter is built on a blocking thread the watch
    // cannot see into, and discovery's failure is not the run's. Boxed: the
    // watch nests the sync's future one level deeper, and every caller's
    // future holds this one, which took `knobas-app`'s layout past the
    // compiler's query depth.
    let sync: std::pin::Pin<Box<dyn Future<Output = _> + Send + '_>> = match mode {
        RunMode::Incremental => Box::pin(run_from_stored_cursor(&mut conn, &deps.pool, watched)),
        RunMode::Backfill => Box::pin(run_backfill(&mut conn, &deps.pool, watched)),
    };
    let (report, held_on) = knobas_http::watch_local_quota(sync).await;

    // Explicit, and not left to `Drop`: `PgConnection::drop` closes the socket
    // without a terminate message, and a run that ends every five minutes
    // should hand its backend back politely. A failure here is not the run's.
    if let Err(error) = sqlx::Connection::close(conn).await {
        tracing::debug!(source_id, %error, "closing the run's connection failed");
    }
    if let (Ok(_), Some(found)) = (&report, &discovered) {
        record_discovered(deps, source_id, found).await;
    }
    report.map_err(|error| match error {
        SyncError::Source(SourceError::Protocol { status: None, .. }) if held_on.is_some() => {
            RunFailure::LocalQuota(error)
        }
        error => RunFailure::Sync(error),
    })
}

/// Close the run: log it, judge the credential, set or clear the backoff, emit.
/// A run the quit cancelled is logged and emitted, and judged on nothing
/// (#1629).
///
/// Every step is best-effort *after* the sync itself committed. The data is
/// already durable at this point, so a failed bookkeeping statement is warned
/// about and the rest still runs -- refusing to record the outcome because the
/// prune failed would leave a run open for ever.
pub async fn settle(deps: &SchedulerDeps, source_id: &str, run_id: i64, result: &RunResult) {
    if let Err(error) = run_log::finish(&deps.pool, run_id, result).await {
        tracing::warn!(source_id, run_id, %error, "the run finished, but its log row did not close");
    }
    if let Err(error) = run_log::prune(
        &deps.pool,
        source_id,
        run_log::KEEP_RUNS,
        run_log::KEEP_WINDOW_SECS,
    )
    .await
    {
        tracing::warn!(source_id, %error, "pruning the sync log failed");
    }

    apply_health_and_backoff(deps, source_id, result).await;

    if result.outcome == SyncOutcome::Ok && result.counts.touched() > 0 {
        emit_latest_activity(deps, source_id).await;
    }
    emit_state(deps, source_id, run_id, Transition::Ended(result.outcome)).await;
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
    // A run the quit cancelled (#1629, spec #1618 ruling A8) is the reader's
    // act, not the source's fault: no verdict on the credential, no clearing
    // of a stored `unreachable` it did not re-witness, and the backoff left
    // as it was. Its row still ends, above, and its ending is still sent
    // (ADR-0005); `failures_since_last_ok` does not count it either.
    if result.was_cancelled() {
        return;
    }
    // A run knobas' own rate limit stopped (#1980) failed on a request the
    // source was never sent. Judged on the credential like any error run,
    // below, but the backoff is left as it was: holding the source off would
    // be knobas backing off from itself. `failures_since_last_ok` does not
    // count it either.
    let held_by_knobas = result.stopped_on_local_quota();
    let (state, detail) = match result.outcome {
        SyncOutcome::Ok => (Some(AuthState::Ok), None),
        SyncOutcome::Unauthorized if result.error.as_deref() == Some(MISSING_SECRET_MESSAGE) => {
            (Some(AuthState::MissingSecret), result.error.as_deref())
        }
        SyncOutcome::Unauthorized => (Some(AuthState::Unauthorized), result.error.as_deref()),
        SyncOutcome::Unreachable => (Some(AuthState::Unreachable), result.error.as_deref()),
        // A protocol bug or a local database failure says nothing about the
        // credential, so no verdict is written.
        SyncOutcome::Error => (None, None),
    };
    let recorded = match state {
        Some(state) => config::set_health(&deps.pool, source_id, state, detail, None).await,
        // But a stored `unreachable` is a claim about the far end this run did
        // not re-witness, and left in place it reads over the run's own error
        // on the sources view for as long as the error repeats (#1477). That
        // one state is cleared.
        None => config::clear_unreachable(&deps.pool, source_id).await,
    };
    match recorded {
        Ok(Some((health, changed))) if changed => deps.events.source_health(health),
        Ok(_) => {}
        Err(error) => tracing::warn!(source_id, %error, "recording credential health failed"),
    }

    let backoff = if result.outcome == SyncOutcome::Ok {
        config::clear_backoff(&deps.pool, source_id).await
    } else if result.outcome.backs_off() && !held_by_knobas {
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
        // A run the local quota stopped lands here too, and keeps whatever
        // backoff the source's own failures set.
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

/// Which transition an emit is about -- and, for the one that ends a run, how
/// it ended.
///
/// Passed in rather than inferred from the log row, because the row can
/// disagree with the truth on exactly one path: [`settle`] warns and carries on
/// when the `update` that closes the row fails, and a run whose row stayed open
/// is still a run that is over. Inferring `running` from `finished_at` there
/// would leave the UI spinning on a run nobody is doing.
#[derive(Debug, Clone, Copy)]
enum Transition {
    /// The run's log row exists and it is about to execute.
    Started,
    /// The run is over, with this verdict.
    Ended(SyncOutcome),
}

/// Emit the state of **one run** for one source. Called at the start of a run
/// (right after its log row exists) and again from [`settle`].
///
/// Built from the run it is emitting for, not from a bare source-level read
/// (#304). [`status_for`]'s laterals take *whichever* run is open for the
/// source and *whichever* finished one started last, so a second run for the
/// same source decided what this event carried: an open neighbour (two
/// schedulers over one `source_config`), or -- once
/// [`run_log::reconcile_abandoned`] has closed every open row at start -- a
/// *finished* neighbour dated ahead of this one. ADR-0005 promises that
/// whoever holds a run id is told how *that* run ended; an emit naming another
/// run breaks the pairing for every listener keyed on `run_id`, the first-run
/// wizard's DONE panel included.
///
/// Two reads, each answering only what it can:
///
/// * the **run's** log row -- the timestamp of the transition being announced,
///   in the database's clock, so the event and the diagnostics list agree
///   about when this run started, or ended;
/// * the **source's** status -- `backoff_until`, and `next_run_at` on the emit
///   that *ends* a run: both belong to the source and to no run, and the emit
///   that starts one keeps `None`, because a run in flight has nothing to
///   count down to. Plus the previous run's ending, which is what
///   `last_outcome` means until this run has an ending of its own.
async fn emit_state(deps: &SchedulerDeps, source_id: &str, run_id: i64, transition: Transition) {
    let source = match status_for(&deps.pool, source_id).await {
        Ok(Some(source)) => source,
        // Deleted while its run was in flight: there is no source left to
        // report a state for, and `sync_status` would not list one either.
        Ok(None) => return,
        Err(error) => {
            tracing::warn!(source_id, %error, "reading sync status for the event failed");
            return;
        }
    };
    // Best-effort, like everything else on this path: without the row the
    // event still names the right run, and its timestamp comes from this
    // process's clock instead of the database's.
    let row = match run_log::get(&deps.pool, run_id).await {
        Ok(row) => row,
        Err(error) => {
            tracing::warn!(source_id, run_id, %error, "reading the run for the event failed");
            None
        }
    };
    deps.events.sync_state(state_of_run(
        source_id,
        run_id,
        transition,
        row.as_ref(),
        &source,
    ));
}

/// The `sync:state` payload for one run: the run's own identity, timing and
/// ending; the source's own backoff and countdown.
///
/// Separated from the two reads above so the composition -- which field comes
/// from which of them -- is assertable without a database.
fn state_of_run(
    source_id: &str,
    run_id: i64,
    transition: Transition,
    row: Option<&run_log::SyncRunRow>,
    source: &SourceSyncStatus,
) -> SourceSyncStatus {
    match transition {
        Transition::Started => {
            let base = SourceSyncStatus::started(source_id, run_id);
            SourceSyncStatus {
                started_at: row.map(|r| r.started_at).or(base.started_at),
                // The *source's*: this run has no ending yet, and
                // `last_outcome` is about the last run that had one.
                last_finished_at: source.last_finished_at,
                last_outcome: source.last_outcome,
                last_changed: source.last_changed,
                backoff_until: source.backoff_until,
                // `next_run_at` is deliberately not taken from the source: it
                // is the countdown, and a run in flight has nothing to count
                // down to. `started` already says `None`.
                ..base
            }
        }
        Transition::Ended(outcome) => {
            let base = SourceSyncStatus::finished(source_id, run_id, outcome);
            SourceSyncStatus {
                // The run's other timestamp, `started_at`, stays `None`:
                // "RFC 3339, while `running`" is what it is documented as on
                // both sides of the bridge (`SourceSyncStatus`, and the
                // TypeScript mirror), and a terminal event that filled it in
                // would be offering a start time as if the run were still
                // going. `last_finished_at` is this run's own time here.
                last_finished_at: row.and_then(|r| r.finished_at).or(base.last_finished_at),
                // This run's own counts. Without its row the event cannot say,
                // and `None` is what a listener treats as "maybe" (#759).
                last_changed: row.map(run_log::SyncRunRow::changed),
                // Source-level, and both need `source_config`: when the next
                // run is due, and how long a failing source is held off for.
                next_run_at: source.next_run_at,
                backoff_until: source.backoff_until,
                ..base
            }
        }
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
/// remote calls. Five, so three long Jira, Confluence or Gitea syncs cannot
/// keep Kuma and TeamCity from polling; monitor polls can wait again only when
/// five or more heavy sources sync at once (#1189). One shared pool, no lanes.
/// It is not bounded by [`SYNC_POOL_SIZE`]: runs hold their own connections and
/// leave the bookkeeping pool untouched by design rather than by arithmetic.
pub const SYNC_CONCURRENCY: usize = 5;

/// How often the ticker looks for due sources, unless [`SchedulerTiming`]
/// says otherwise. One indexed query; a fixed interval is cheaper to reason
/// about than a computed sleep.
///
/// A source runs up to one tick after it falls due, so the tick must stay
/// well under the smallest interval a user can configure. That floor is each
/// adapter's own `min_sync_interval_secs` (#1187): TeamCity declares 20 s,
/// Uptime Kuma 45 s, every other adapter
/// [`knobas_source::DEFAULT_MIN_SYNC_INTERVAL_SECS`] (60 s). Five seconds is a
/// quarter of the lowest compiled-in floor today. An external adapter's floor
/// is whatever its descriptor says, and nothing here bounds it from below:
/// against a floor near five seconds, a run up to one tick late nearly doubles
/// the interval.
const TICK: Duration = Duration::from_secs(5);

/// How long the ticker waits before its first look, unless
/// [`SchedulerTiming`] says otherwise.
///
/// Two jobs: it keeps the first `sync:state` from racing the webview's
/// listeners (roadmap §4 gotcha 9 -- `sync_status()` on mount is the
/// authoritative read, the event is a hint), and it keeps a cold start from
/// competing with the first-run wizard for the same source.
const STARTUP_DELAY: Duration = Duration::from_secs(2);

/// How long shutdown waits for runs to notice the cancellation before aborting
/// them, and for a background send to finish (#1633, ruling B1). A cancelled
/// run only has to unwind to its next await point; a send at the source is
/// let finish so its `sent` mark lands before the pool closes.
///
/// The default for [`SchedulerTiming::shutdown_grace`], which is what
/// [`Scheduler::shutdown`] reads. Public so a test can size a wait past it
/// rather than restate it.
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(3);

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
/// Held once, by `knobas-app`'s `SourcesState`. `Clone` is a second handle
/// onto the same scheduler, not a second scheduler: the background send a
/// queued write gets (issue #752) carries one into its task.
#[derive(Clone)]
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
    /// How many runs found every permit taken and are waiting for one: see
    /// [`Scheduler::waiting_for_a_permit`].
    waiting: AtomicUsize,
    claims: Mutex<Claims>,
    /// Poked when something changed that might make a source due (a finished
    /// run, a new source, a re-entered credential), so the UI does not wait out
    /// a tick.
    wake: Notify,
    /// Poked by the ticker on every tick: the write queue's flush runs on a
    /// task of its own ([`flush_loop`], #1629), so a write that spends its
    /// send budget holds no source's schedule. A poke while a flush is under
    /// way leaves one permit, so the flush runs once more after it, not once
    /// per tick it missed.
    flush_due: Notify,
    /// [`SchedulerDeps::quit`]'s token, the one [`Scheduler::shutdown`]
    /// cancels: a clone, so a flush handed `deps` and this scheduler stop on
    /// the same cancel (#1924).
    cancel: CancellationToken,
    /// Handles to join (or abort) at shutdown.
    ///
    /// Pruned on every push: a run adds one and never removes it, so a process
    /// left open for a week at a five-minute interval would accumulate
    /// thousands of finished handles for the sake of a shutdown that only
    /// cares about the live ones.
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
    /// The sends and link resolutions on this scheduler's pool that are not
    /// runs ([`Scheduler::background`]): awaited by `shutdown` beside the
    /// runs, before the pool closes.
    background: Background,
    /// Held for reading by every [`trigger`](Inner::trigger) from its first
    /// cancellation check until its run is spawned and in `tasks` (#1633,
    /// SYNC-5); `shutdown` takes it for writing once, after cancelling, so it
    /// never takes `tasks` while a trigger that began before the quit has a
    /// run row open and no task yet.
    door: RwLock<()>,
    /// Asked for a pass after every run that committed (#778): the mirror
    /// tables a sync grew by more than a fifth are analyzed, on a connection
    /// of their own, so a fresh install's reads plan with statistics within
    /// seconds of its first sync rather than whenever autovacuum gets there.
    analyzer: crate::analyze::Analyzer,
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
        Self::start_with_background(deps, Background::new()).await
    }

    /// [`start`](Self::start), holding `background` as its
    /// [`background`](Self::background) set: for a caller whose
    /// [`SyncEvents`] spawn into it and are built before the scheduler is.
    ///
    /// # Errors
    /// As [`start`](Self::start).
    pub async fn start_with_background(
        deps: SchedulerDeps,
        background: Background,
    ) -> Result<Scheduler, sqlx::Error> {
        // **Before the first tick, and only then.** It closes every run row
        // with no `finished_at`, which after this point would include the runs
        // this scheduler is itself about to open.
        let closed = run_log::reconcile_abandoned(&deps.pool).await?;
        if closed > 0 {
            tracing::warn!(closed, "closed sync runs left open by a previous process");
        }

        let analyzer = crate::analyze::Analyzer::new(Arc::clone(&deps.connections));
        let cancel = deps.quit.token().clone();
        let inner = Arc::new(Inner {
            deps,
            analyzer,
            permits: Semaphore::new(SYNC_CONCURRENCY),
            waiting: AtomicUsize::new(0),
            claims: Mutex::new(Claims {
                runs: HashMap::new(),
                pending_purges: HashSet::new(),
            }),
            wake: Notify::new(),
            flush_due: Notify::new(),
            cancel,
            tasks: Mutex::new(Vec::new()),
            background,
            door: RwLock::new(()),
        });

        let ticker = tokio::spawn(tick_loop(Arc::clone(&inner)));
        // Beside the runs in `tasks`, so `shutdown` joins it within the same
        // grace and aborts it at its end. Not through the door: it opens no
        // run row, so there is nothing a quit could leave half-started.
        let flusher = tokio::spawn(flush_loop(Arc::clone(&inner)));
        inner.tasks.lock().await.extend([ticker, flusher]);
        Ok(Scheduler { inner })
    }

    /// What the scheduler was built with.
    ///
    /// The write queue's flush loop takes the same [`SchedulerDeps`] -- it
    /// builds an adapter through the same seam and writes through the same
    /// pool -- and the desktop shell holds a `Scheduler`, not the deps it was
    /// handed. Exposing them is what lets a `#[tauri::command]` reach
    /// [`crate::write_queue`] without `SourcesState` growing a second copy of
    /// four fields that must not drift from these.
    #[must_use]
    pub fn deps(&self) -> &SchedulerDeps {
        &self.inner.deps
    }

    /// The analyze pass every committed run asks for (#778), for a caller
    /// that has to know it is done.
    #[must_use]
    pub fn analyzer(&self) -> &crate::analyze::Analyzer {
        &self.inner.analyzer
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
        self.inner
            .trigger(source_id, trigger, progress)
            .await
            .map(Triggered::run_id)
    }

    /// Sync one source and **wait for a run that could have seen what was
    /// just written** (issues #289, #358).
    ///
    /// [`trigger`](Self::trigger) starts or joins a run and answers straight
    /// away, which is right for a write that has just landed: nothing is
    /// waiting on the mirror, and story 15 only asks that the app stop
    /// disagreeing with itself within the second.
    ///
    /// A caller that is about to *look for what its write created* needs the
    /// other behaviour, and the reason is `start_work::queue::Queue::refresh`'s
    /// in as many words: what a create made has an entity only once a sync has
    /// seen it (even where its receipt names it, #1949), so the only way to
    /// link one is to read the mirror, and reading it before the run that
    /// fetches it has finished is reading it too early.
    ///
    /// # Why waiting once is not enough (#358)
    ///
    /// Waiting for *a* run is not waiting for one that could have seen the
    /// write. [`trigger`](Self::trigger) dedupes against the run in flight --
    /// which is what makes a double-clicked *Sync now* harmless, and a burst of
    /// writes cost one run rather than one each -- and the same dedupe hands
    /// this caller a run that **began before the write did**.
    ///
    /// That is not a corner: `knobas-app`'s `sources::write_queue::flush` --
    /// the crate above this one, so named rather than linked -- starts exactly
    /// such a run after every write that lands, so a flow which writes twice
    /// and then looks for what it made is the ordinary case. Start-work is one (issue
    /// #44): a branch, a pull request, then find the pull request. The branch's
    /// refresh was still going, this joined it, its ending arrived faithfully
    /// -- and it had nothing to say about a pull request that did not exist
    /// when it started. The step failed about one run in five (#358).
    ///
    /// So a **joined** run is waited out and a second one asked for, and that
    /// second one cannot be stale in the same way: a source holds at most one
    /// run at a time ([`Claims::runs`]), so a run in flight when this asks
    /// again is one that began after the first ended -- which is after this
    /// call, and therefore after the write.
    ///
    /// **Two waits at the most, and no polling.** The caller is still told "the
    /// mirror is as fresh as one sync can make it" exactly once and then gives
    /// up, which is what keeps a step that is stuck distinguishable from one
    /// that is slow (story 11).
    ///
    /// **What that costs, and who pays it.** The second run is paid whenever
    /// one was in flight, and that is the ordinary case for every caller this
    /// has -- not just start-work. That same `flush` triggers its refresh the
    /// moment a write lands, and each caller asks for this immediately after a
    /// write, so all three now wait out two incremental syncs where they
    /// waited out one: start-work's link step (issue #44), #289's protocol
    /// publish, and the ticket an action item is turned into. For start-work
    /// the run they used to wait out was the wrong one; for the other two it
    /// usually was not, and this cannot tell those apart, because `flush`
    /// discards which of the two its own trigger got. A source with nothing in
    /// flight still costs a single run: `flush`'s run finished before this
    /// asked, or there was none because the source refused the write.
    ///
    /// ADR-0005 guarantees a run id always comes with an ending -- including
    /// the id of a run already in flight -- so neither wait can be for
    /// something that will never speak.
    ///
    /// Every run this asks for is an *incremental sync* in `CONTEXT.md`'s
    /// sense, from the stored cursor; [`backfill`](Self::backfill) is still
    /// the only cursor-less run. The name carries the two things this adds to
    /// [`trigger`](Self::trigger) and nothing else: the **wait**, and that
    /// what it waits for is a run **after the write** (#375).
    ///
    /// Answers the id of the run it waited on last.
    ///
    /// # Errors
    /// [`TriggerError`], from either trigger.
    pub async fn sync_after_write(&self, source_id: &str) -> Result<i64, TriggerError> {
        match self.trigger_and_wait(source_id).await? {
            Triggered::Started(run_id) => Ok(run_id),
            Triggered::Existing(_) => self
                .trigger_and_wait(source_id)
                .await
                .map(Triggered::run_id),
        }
    }

    /// Ask for a manual sync, wait for its ending, and say whether the run was
    /// this caller's own or one it was enrolled in.
    async fn trigger_and_wait(&self, source_id: &str) -> Result<Triggered, TriggerError> {
        let (done, wait) = tokio::sync::oneshot::channel();
        let sink = Arc::new(RunEnded {
            done: std::sync::Mutex::new(Some(done)),
        });
        let triggered = self
            .inner
            .trigger(source_id, SyncTrigger::Manual, Some(sink))
            .await?;
        let _ = wait.await;
        Ok(triggered)
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
            .map(Triggered::run_id)
    }

    /// Enrol `progress` in the source's run in flight, and answer that run's
    /// id; `None`, enrolling nobody, when no run is in flight (#1384).
    ///
    /// The same join [`trigger`](Self::trigger) makes for a second caller,
    /// without the trigger: a page that opens on a source mid-run asks to
    /// watch, not for work, so an idle source stays idle. Under the `runs`
    /// lock and through [`Watchers::attach`], so the sink either hears the
    /// run's remaining progress and its ending or is told there is none: a run
    /// that ended a microsecond earlier answers `None` and sends nothing. The
    /// first-sync claim is left alone; watching is not asking about the first
    /// sync.
    pub async fn watch(&self, source_id: &str, progress: Arc<dyn ProgressSink>) -> Option<i64> {
        let claims = self.inner.claims.lock().await;
        let entry = claims.runs.get(source_id)?;
        (entry.watchers.attach(Some(progress)) == Attach::Joined).then(|| entry.watchers.run_id())
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
            if !cfg.enabled || cfg.health.state.needs_human() {
                continue;
            }
            ids.push(
                self.inner
                    .trigger(&cfg.id, SyncTrigger::Manual, None)
                    .await?
                    .run_id(),
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

    /// Where a task on this scheduler's pool that is not a sync run goes -- a
    /// write's send behind an answer already given, a start-work flow's
    /// steps -- so the quit waits for it before closing the pool, and refuses
    /// it once the quit has begun.
    #[must_use]
    pub fn background(&self) -> &Background {
        &self.inner.background
    }

    /// How many runs are waiting for a permit: triggered, their row open, and
    /// all [`SYNC_CONCURRENCY`] permits taken when they asked (#1189). Counted
    /// from the moment a run finds none free until it gets one or is
    /// cancelled, so a run counted here cannot reach its adapter until a
    /// permit is let go. A test reads here that a run is held by the cap
    /// rather than not yet arrived (#1927).
    #[must_use]
    pub fn waiting_for_a_permit(&self) -> usize {
        self.inner.waiting.load(Ordering::SeqCst)
    }

    /// How many sinks are enrolled in the source's run in flight, each to hear
    /// its progress and its ending; `0` when no run is in flight. A caller
    /// that joined the run with a sink ([`watch`](Self::watch), a trigger
    /// that hands one, [`sync_after_write`](Self::sync_after_write)) is one of
    /// them from the moment it joined, so a test reads here that a caller has
    /// joined before it lets the run end (#1927). A trigger without a sink
    /// joins without being counted.
    pub async fn watching(&self, source_id: &str) -> usize {
        let claims = self.inner.claims.lock().await;
        claims
            .runs
            .get(source_id)
            .map_or(0, |entry| entry.watchers.enrolled())
    }

    /// Whether [`shutdown`](Self::shutdown) has begun: from its first step on,
    /// nothing new starts.
    #[must_use]
    pub fn is_shutting_down(&self) -> bool {
        self.inner.cancel.is_cancelled()
    }

    /// Look for due sources now rather than at the next tick.
    pub fn wake(&self) {
        self.inner.wake.notify_one();
    }

    /// Stop ticking, cancel every run, wait for the background sends, and
    /// close the scheduler's pool -- **bounded**, so quitting during a
    /// 30-second remote call is not a 30-second hang (carry-over).
    ///
    /// Cancel first (#1633, ruling B1): no trigger starts a run and the
    /// [`background`](Self::background) set takes no task after it. Then
    /// every trigger that began before it is let reach its spawn (the door),
    /// so its run is in `tasks` or its row ended. Then the runs and the set
    /// are awaited together. One grace ([`SchedulerTiming::shutdown_grace`],
    /// [`SHUTDOWN_GRACE`] outside tests) from the cancel covers
    /// the door and both waits, and what is left at its end is aborted, and
    /// awaited until it is gone. A write that was still at its source then
    /// is held as one the source may have (#1932, `write_queue::hold_cut`),
    /// so the next start does not send it again. Only then the pool.
    ///
    /// Idempotent: `RunEvent::ExitRequested` can fire more than once.
    pub async fn shutdown(&self) {
        self.inner.cancel.cancel();
        let deadline = tokio::time::Instant::now() + self.inner.deps.timing.shutdown_grace;
        let background = self.inner.background.close(deadline);
        if tokio::time::timeout_at(deadline, self.inner.door.write())
            .await
            .is_err()
        {
            tracing::warn!("a trigger did not reach its spawn within the grace period");
        }

        let mut handles = std::mem::take(&mut *self.inner.tasks.lock().await);
        let runs = async {
            let joined = async {
                for handle in &mut handles {
                    let _ = handle.await;
                }
            };
            if tokio::time::timeout_at(deadline, joined).await.is_err() {
                tracing::warn!("a sync run did not stop within the grace period: aborting it");
                for handle in &handles {
                    handle.abort();
                }
                // Until each is gone: an aborted task's future is dropped on
                // its own worker, and a send it cut is kept as it drops. A
                // handle joined above is finished, and polling it again
                // would panic.
                for handle in handles.iter_mut().filter(|handle| !handle.is_finished()) {
                    let _ = handle.await;
                }
            }
        };
        tokio::join!(runs, background);

        // With the pool still open, and bounded as its close is: a row is one
        // statement.
        if tokio::time::timeout(
            POOL_CLOSE_TIMEOUT,
            crate::write_queue::hold_cut(&self.inner.deps),
        )
        .await
        .is_err()
        {
            tracing::warn!("the writes the quit cut could not all be held in time");
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

/// One run counted in [`Inner::waiting`] for as long as it waits for a
/// permit, however the wait ends: given a permit, cancelled, or the task
/// aborted at the quit's grace.
struct Waiting<'a>(&'a AtomicUsize);

impl<'a> Waiting<'a> {
    fn enter(waiting: &'a AtomicUsize) -> Waiting<'a> {
        waiting.fetch_add(1, Ordering::SeqCst);
        Waiting(waiting)
    }
}

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// What a trigger did: started the run, or handed the caller one it did not.
///
/// The distinction is invisible to nearly every caller -- *Sync now* wants the
/// mirror re-read and does not care who is doing it -- and load-bearing for
/// exactly one: [`Scheduler::sync_after_write`], whose caller has just written
/// something and is about to look for it. A run it did not start is a run
/// that may have begun before that write, and no ending of such a run says
/// anything about it.
///
/// `Existing` covers both ways of being handed somebody else's run: enrolled in
/// one still in flight, and served the record of one already over. Neither
/// began after the call, which is the only property the distinction is about.
///
/// **Not a retreat from ADR-0005's "no caller has to know which of those
/// happened".** That sentence is about the *ending*, and the ending is still
/// promised identically on all three paths -- which is what lets
/// `sync_after_write` wait on a joined run at all. This says something else,
/// and only inside this crate: whether the run began before or after the
/// call. A caller asking "did the mirror get re-read?" still must not ask;
/// the one asking "could a sync have seen what I just wrote?" has no other
/// way to know, and used to guess wrong one time in five (#358).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Triggered {
    Started(i64),
    Existing(i64),
}

impl Triggered {
    fn run_id(self) -> i64 {
        match self {
            Triggered::Started(id) | Triggered::Existing(id) => id,
        }
    }
}

/// A progress sink that resolves when its run ends.
///
/// The `Option` makes a second terminal message -- which ADR-0005 says cannot
/// happen, and which this must survive if it ever did -- a no-op rather than a
/// panic inside a sink this crate would then have to catch.
struct RunEnded {
    done: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

impl ProgressSink for RunEnded {
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

impl Inner {
    async fn trigger(
        self: &Arc<Self>,
        source_id: &str,
        trigger: SyncTrigger,
        progress: Option<Arc<dyn ProgressSink>>,
    ) -> Result<Triggered, TriggerError> {
        // Derived, never passed alongside: see `impl From<SyncTrigger> for
        // RunMode`. This is what makes "the log says `backfill`" and "the run
        // was forbidden to sweep" the same statement.
        let mode = RunMode::from(trigger);
        // Held until the run is in `tasks`: see `door`.
        let _door = self.door.read().await;
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
                return Ok(Triggered::Existing(run_id));
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
                return Ok(Triggered::Existing(run_id));
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
        emit_state(&self.deps, source_id, run_id, Transition::Started).await;

        let inner = Arc::clone(self);
        let id = source_id.to_owned();
        let mut tasks = self.tasks.lock().await;
        // **The quit began while this trigger was on its way here** (#1633,
        // SYNC-5): no run is spawned, and the row this trigger opened is
        // ended here, the way a run cancelled before its permit ends --
        // settled, its watchers told, the source released. `shutdown` is
        // waiting at the door, so the pool is still open for it.
        if self.cancel.is_cancelled() {
            drop(tasks);
            inner.run_task(id, run_id, mode, watchers).await;
            return Err(TriggerError::ShuttingDown);
        }
        let handle = tokio::spawn(async move { inner.run_task(id, run_id, mode, watchers).await });
        tasks.retain(|task| !task.is_finished());
        tasks.push(handle);
        Ok(Triggered::Started(run_id))
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
        // Ends the run however this task ends. `Watchers::close` is
        // first-call-wins, so the real ending below still wins on every
        // ordinary path; this one fires only when the task never reached it --
        // a panicking adapter, or the abort `shutdown` falls back on. Without
        // it a caller holding that run's id waits for ever, and the source is
        // never released: the same failure ADR-0005 is about, arriving through
        // a door the happy path cannot see. Until the verdict below is in, it
        // also records the ending (#1638), so the row and the `sync:state`
        // ending exist whatever the adapter did.
        let mut closing = Closing {
            watchers: Arc::clone(&watchers),
            source_id: source_id.clone(),
            run_id,
            started,
            recorder: Some(Arc::clone(&self)),
        };
        let permit = match self.permits.try_acquire() {
            Ok(permit) => Some(permit),
            // Every permit is taken: counted while it waits, so the cap is
            // something a reader can see holding a run, not only the absence
            // of one more run in the adapter (#1927).
            Err(_) => {
                let _waiting = Waiting::enter(&self.waiting);
                tokio::select! {
                    biased;
                    () = self.cancel.cancelled() => None,
                    permit = self.permits.acquire() => permit.ok(),
                }
            }
        };

        let result = match permit {
            None => RunResult::cancelled(),
            Some(_permit) => {
                tokio::select! {
                    biased;
                    () = self.cancel.cancelled() => RunResult::cancelled(),
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

        // From here this task records its own ending, so the guard only closes
        // the watchers if the task dies on the way: once, never a second row
        // write or a second `sync:state` ending.
        closing.recorder = None;
        self.end_run(&source_id, run_id, &result, &watchers, started)
            .await;
    }

    /// Record a run's verdict and tell everyone watching how it ended: settle,
    /// the purge a delete left for it, the ending (which releases the source),
    /// the wake. The tail of [`run_task`](Self::run_task), and what the
    /// [`Closing`] guard runs for a task that died before it (#1638).
    async fn end_run(
        &self,
        source_id: &str,
        run_id: i64,
        result: &RunResult,
        watchers: &Watchers,
        started: std::time::Instant,
    ) {
        settle(&self.deps, source_id, run_id, result).await;
        // Only a run that committed changed the mirror. Not awaited: the
        // pass is a background task, and the run's ending must not wait on it.
        if result.outcome == SyncOutcome::Ok {
            self.analyzer.request();
        }
        let ending = ending(
            run_id,
            source_id,
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
            if claims.pending_purges.remove(source_id) {
                purge_again(&self.deps, source_id).await;
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

/// What a run whose task died before its verdict is reported as, in its row and
/// in its ending.
///
/// Not a claim about the data beyond what is true: the run's transaction went
/// with the task, so nothing it had fetched landed. It says no more than that
/// the run did not get to say how it went. Until #1638 it read "without
/// recording an outcome"; the guard now records this one.
const RUN_STOPPED_MESSAGE: &str = "the run stopped before it could finish";

/// The verdict [`Closing`] records for a run whose task died before its own:
/// an error, on the ladder like any other (#1638). A panic is an adapter bug,
/// and a bug that recurs every run backs off like one that answers an error.
fn stopped() -> RunResult {
    RunResult {
        outcome: SyncOutcome::Error,
        counts: run_log::RunCounts::default(),
        error: Some(RUN_STOPPED_MESSAGE.to_owned()),
    }
}

/// Ends a run if its task ends without doing so itself.
///
/// A guard rather than a `catch_unwind` or a second code path: this is the only
/// construction that also covers the abort in [`Scheduler::shutdown`], and it
/// cannot be forgotten by a later edit to `run_task` the way a line at the
/// bottom of the function can.
///
/// **Two halves** (#1638, spec #1619). While `recorder` is set -- the task has
/// not reached its verdict -- a drop hands [`Inner::end_run`] to the
/// scheduler's [`Background`] set with [`stopped`]: the row is finished, the
/// backoff and the `sync:state` ending follow as for any failed run, and the
/// watchers are closed after the row, as on the ordinary path. That is the
/// panicking adapter. Once the set has closed -- the quit began, and the abort
/// in `shutdown` comes after that -- the spawn is refused and only the second
/// half runs: the watchers are closed here, now, and the row is
/// `reconcile_abandoned`'s at the next start, since the pool is about to close
/// under any write. The second half is also what a task that dies inside
/// `end_run` gets, and what the spawned `end_run` itself carries if it is
/// aborted.
///
/// The panicking-adapter case runs this drop *during an unwind*, where a second
/// panic escaping it aborts the process. [`Watchers::close`] is written not to
/// panic for that reason -- it contains a misbehaving sink rather than letting
/// it out -- and the spawn is only attempted inside a runtime, where
/// [`Background::spawn`] does not panic, so this guard is safe on the very path
/// it exists for.
struct Closing {
    watchers: Arc<Watchers>,
    source_id: String,
    run_id: i64,
    started: std::time::Instant,
    /// The scheduler, while the ending is still this guard's to record.
    recorder: Option<Arc<Inner>>,
}

impl Drop for Closing {
    fn drop(&mut self) {
        if let Some(inner) = self.recorder.take()
            && tokio::runtime::Handle::try_current().is_ok()
        {
            // Carries the second half into the task: if the task is aborted
            // or dropped unrun, its drop closes the watchers.
            let fallback = Closing {
                watchers: Arc::clone(&self.watchers),
                source_id: self.source_id.clone(),
                run_id: self.run_id,
                started: self.started,
                recorder: None,
            };
            let background = inner.background.clone();
            let spawned = background.spawn(async move {
                inner
                    .end_run(
                        &fallback.source_id,
                        fallback.run_id,
                        &stopped(),
                        &fallback.watchers,
                        fallback.started,
                    )
                    .await;
            });
            if spawned {
                return;
            }
        }
        let verdict = stopped();
        self.watchers.close(ending(
            self.run_id,
            &self.source_id,
            verdict.outcome,
            verdict.error,
            0,
            elapsed_ms(self.started),
        ));
    }
}

/// Look for due sources every [`SchedulerTiming::tick`], or whenever something
/// pokes `wake`.
async fn tick_loop(inner: Arc<Inner>) {
    let SchedulerTiming {
        startup_delay,
        tick,
        ..
    } = inner.deps.timing;
    tokio::select! {
        () = inner.cancel.cancelled() => return,
        () = tokio::time::sleep(startup_delay) => {}
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

        // The write queue drains on the same tick (issue #42, story 9:
        // "recovery needs no ceremony from me"). Here rather than after a
        // successful sync, because the two are independent: a source whose
        // sync is disabled still owes its writes, and a credential re-entered
        // while nothing is due would otherwise leave the queue sitting until
        // the next scheduled run.
        //
        // Woken, not run: the flush is on its own task (`flush_loop`, #1629,
        // spec #1618 ruling A7). A pass can be every due write of every
        // source, each of which may spend the 45 s send budget, and inline
        // here it held every source's schedule for as long.
        inner.flush_due.notify_one();

        tokio::select! {
            biased;
            () = inner.cancel.cancelled() => return,
            () = inner.wake.notified() => {}
            () = tokio::time::sleep(tick) => {}
        }
    }
}

/// The write queue's flush, woken by every tick ([`Inner::flush_due`]).
///
/// What a started flush is: every head write each source owes, source after
/// source, each send bounded only by the adapter's budget (45 s). What a quit
/// does to it: the cancel is selected against between writes, so no write is
/// started after it; the one on the wire is let finish inside
/// [`SHUTDOWN_GRACE`], so its `sent` lands, and is aborted at the grace's end
/// and held as one the source may have (#1932), not sent again blind.
async fn flush_loop(inner: Arc<Inner>) {
    loop {
        tokio::select! {
            biased;
            () = inner.cancel.cancelled() => return,
            () = inner.flush_due.notified() => {}
        }
        crate::write_queue::flush_all(&inner.deps).await;
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

    /// **Which field of an emit comes from the run, and which from the
    /// source** (#304) -- the composition [`state_of_run`] exists to make
    /// assertable without a database.
    ///
    /// Every value here is distinct, and the source's are the ones a
    /// *neighbouring* run would have put in front of `status_for`: if a field
    /// is taken from the wrong side, no two of them can look the same.
    #[test]
    fn an_emits_identity_comes_from_its_run_and_its_countdown_from_the_source() {
        let t = |m: i64| Utc::now() + chrono::Duration::minutes(m);
        let (started, finished, other_started, other_finished, next, backoff) =
            (t(1), t(2), t(3), t(4), t(5), t(6));
        let row = run_log::SyncRunRow {
            id: 7,
            source_id: "s".into(),
            trigger: run_log::SyncTrigger::Manual,
            started_at: started,
            finished_at: Some(finished),
            outcome: Some(SyncOutcome::Ok),
            upserted: 0,
            deleted: 0,
            swept: 0,
            error: None,
            cursor_after: None,
        };
        // What `status_for` answers with a neighbour in front of this run.
        let source = SourceSyncStatus {
            source_id: "s".into(),
            running: true,
            run_id: Some(99),
            started_at: Some(other_started),
            last_finished_at: Some(other_finished),
            last_outcome: Some(SyncOutcome::Unauthorized),
            next_run_at: Some(next),
            backoff_until: Some(backoff),
            last_changed: Some(true),
        };

        let running = state_of_run("s", 7, Transition::Started, Some(&row), &source);
        assert_eq!(running.run_id, Some(7), "the run this emit is about");
        assert!(running.running);
        assert_eq!(running.started_at, Some(started), "this run's start");
        assert_eq!(
            running.last_finished_at,
            Some(other_finished),
            "source-level: this run has not finished, so `last_` is the last \
             run that did"
        );
        assert_eq!(running.last_outcome, Some(SyncOutcome::Unauthorized));
        assert_eq!(running.backoff_until, Some(backoff), "the source's");
        assert_eq!(
            running.last_changed,
            Some(true),
            "source-level, as `last_outcome` is: the last run that finished"
        );
        assert_eq!(
            running.next_run_at, None,
            "a run in flight has nothing to count down to"
        );

        let ended = state_of_run(
            "s",
            7,
            Transition::Ended(SyncOutcome::Unreachable),
            Some(&row),
            &source,
        );
        assert_eq!(ended.run_id, Some(7));
        assert!(!ended.running);
        assert_eq!(
            ended.started_at, None,
            "`started_at` is for a run in flight"
        );
        assert_eq!(ended.last_finished_at, Some(finished), "this run's finish");
        assert_eq!(
            ended.last_outcome,
            Some(SyncOutcome::Unreachable),
            "the verdict `settle` is applying, not the row's and not the \
             source's: a run whose row failed to close is still over"
        );
        assert_eq!(ended.next_run_at, Some(next), "the source's countdown");
        assert_eq!(ended.backoff_until, Some(backoff));
        assert_eq!(
            ended.last_changed,
            Some(false),
            "this run's own counts, all zero -- not the source's earlier run (#759)"
        );
        let wrote = run_log::SyncRunRow {
            swept: 1,
            ..row.clone()
        };
        assert_eq!(
            state_of_run(
                "s",
                7,
                Transition::Ended(SyncOutcome::Ok),
                Some(&wrote),
                &source
            )
            .last_changed,
            Some(true),
            "a run that swept one row changed the mirror"
        );

        // Without the row -- pruned, or the read failed -- the identity still
        // holds and the timestamp falls back to this process's clock.
        let blind = state_of_run("s", 7, Transition::Started, None, &source);
        assert_eq!(blind.run_id, Some(7));
        assert!(
            blind.started_at.is_some() && blind.started_at != Some(other_started),
            "not the neighbour's start time: {:?}",
            blind.started_at
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

    /// The timing a production [`SchedulerDeps`] gets when nobody chooses one
    /// is the timing the app shipped with before it became a choice: a two
    /// second startup delay, a five second tick (#412) and a three second
    /// grace at quit (#1633). The literals here
    /// are the spec's numbers, not the constants -- a test that compared the
    /// default to `TICK` would pass through any edit to `TICK`.
    #[test]
    fn the_default_timing_is_the_production_timing() {
        assert_eq!(
            SchedulerTiming::default(),
            SchedulerTiming {
                startup_delay: Duration::from_secs(2),
                tick: Duration::from_secs(5),
                shutdown_grace: Duration::from_secs(3),
            }
        );
    }
}
