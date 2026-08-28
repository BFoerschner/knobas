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

use std::collections::HashMap;
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
use crate::progress::{Observed, ProgressSink, SyncPhase, SyncProgress};
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
#[derive(Debug)]
enum RunFailure {
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

    fn message(&self) -> String {
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
async fn build_source(
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
/// trigger says *why* a run happened (schedule, *Sync now*, first run) and is
/// written to `knobas.sync_run` for the diagnostics list, while this says what
/// the run does and is never stored. A backfill is triggered manually and logs
/// as `Manual`; adding a trigger spelling for it would need a migration to
/// widen `sync_run_trigger_chk`, and would still be answering a different
/// question.
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

    // The decorator only exists when somebody attached a channel: a scheduled
    // run allocates nothing and reports nothing per item (P3). `Fetching` and
    // `Writing` are emitted from inside it, where they are true.
    let report = match (progress, mode) {
        (Some(sink), RunMode::Incremental) => {
            let observed = Observed::new(source.as_ref(), run_id, started, Arc::clone(sink));
            run_from_stored_cursor(&mut conn, &deps.pool, &observed).await
        }
        (Some(sink), RunMode::Backfill) => {
            let observed = Observed::new(source.as_ref(), run_id, started, Arc::clone(sink));
            run_backfill(&mut conn, &deps.pool, &observed).await
        }
        (None, RunMode::Incremental) => {
            run_from_stored_cursor(&mut conn, &deps.pool, source.as_ref()).await
        }
        (None, RunMode::Backfill) => run_backfill(&mut conn, &deps.pool, source.as_ref()).await,
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

struct Inner {
    deps: SchedulerDeps,
    permits: Semaphore,
    /// source id → the run id currently in flight for it. The advisory lock
    /// would serialise two runs anyway; this stops the *second one existing*,
    /// which is what keeps the log honest and the UI showing one progress bar.
    inflight: Mutex<HashMap<String, i64>>,
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
            inflight: Mutex::new(HashMap::new()),
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
    /// harmless.
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
            .trigger(source_id, trigger, RunMode::Incremental, progress)
            .await
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
    /// Logged as [`SyncTrigger::Manual`]: only a person starts one, and the
    /// trigger vocabulary is a database check constraint, so a spelling of its
    /// own would need a migration.
    ///
    /// # Errors
    /// [`TriggerError`].
    pub async fn backfill(&self, source_id: &str) -> Result<i64, TriggerError> {
        self.inner
            .trigger(source_id, SyncTrigger::Manual, RunMode::Backfill, None)
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
                    .trigger(&cfg.id, SyncTrigger::Manual, RunMode::Incremental, None)
                    .await?,
            );
        }
        Ok(ids)
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
        mode: RunMode,
        progress: Option<Arc<dyn ProgressSink>>,
    ) -> Result<i64, TriggerError> {
        if self.cancel.is_cancelled() {
            return Err(TriggerError::ShuttingDown);
        }
        // The whole check-and-claim under one lock: two `sync_now` calls
        // arriving together must not both decide the source is idle.
        let mut inflight = self.inflight.lock().await;
        if let Some(run_id) = inflight.get(source_id) {
            return Ok(*run_id);
        }
        if config::get(&self.deps.pool, source_id).await?.is_none() {
            return Err(TriggerError::UnknownSource(source_id.to_owned()));
        }
        let run_id = run_log::start(&self.deps.pool, source_id, trigger).await?;
        inflight.insert(source_id.to_owned(), run_id);
        drop(inflight);

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
        let handle = tokio::spawn(async move { inner.run_task(id, run_id, mode, progress).await });
        let mut tasks = self.tasks.lock().await;
        tasks.retain(|task| !task.is_finished());
        tasks.push(handle);
        Ok(run_id)
    }

    /// One spawned run: take a permit, do the work (or give up when cancelled),
    /// settle, release the source.
    async fn run_task(
        self: Arc<Self>,
        source_id: String,
        run_id: i64,
        mode: RunMode,
        progress: Option<Arc<dyn ProgressSink>>,
    ) {
        // The permit is what caps concurrency. Acquired *after* the log row
        // exists, so a queued run is visible as "running" in the UI rather
        // than as nothing at all.
        // Started when the task did, not when it got a permit: a run queued
        // behind the concurrency cap has been waiting, and the bar should say
        // so.
        let started = std::time::Instant::now();
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
                        &self.deps, &source_id, run_id, mode, progress.clone(), started,
                    ) => result,
                }
            }
        };
        // Dropping the run future above is what rolls its transaction back and
        // closes its connection -- which is why `settle` can still write.

        settle(&self.deps, &source_id, run_id, &result).await;
        if let Some(sink) = progress {
            // `started`, not `0`: the terminal message is the one a progress
            // bar shows as the run's duration, and a hardcoded zero made it
            // report every run as instantaneous.
            report(
                &sink,
                run_id,
                &source_id,
                if result.outcome == SyncOutcome::Ok {
                    SyncPhase::Finished
                } else {
                    SyncPhase::Failed
                },
                u64::try_from(result.counts.upserted).unwrap_or(0),
                started,
                result.error.clone(),
            );
        }
        self.inflight.lock().await.remove(&source_id);
        self.wake.notify_one();
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
                    if let Err(error) = inner
                        .trigger(&source.id, trigger, RunMode::Incremental, None)
                        .await
                    {
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
