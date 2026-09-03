//! The ticker. These tests use a deliberately slow adapter so "concurrent" and
//! "cancelled" are observable rather than inferred.
//!
//! These tests serialise themselves -- see [`serially`]. They are not merely
//! slow together: `config::due` is whole-database by design (one scheduler per
//! profile), so two tickers running at once in this binary each pick up the
//! other's sources, and "due once, run once" becomes "run twice". A
//! `--test-threads=1` in a comment is a rule nobody enforces; a lock in the
//! test body is one `cargo test --workspace` cannot skip.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use knobas_secrets::MemoryStore;
use knobas_source::instance::SourceInstance;
use knobas_source::{Cursor, Sink, Source, SourceDescriptor, SourceError};
use knobas_sync::config::{self, AuthKind, InsertConfig};
use knobas_sync::progress::{ProgressSink, SyncPhase, SyncProgress};
use knobas_sync::run_log::{self, SyncTrigger};
use knobas_sync::scheduler::{
    AdapterRegistry, Purge, RunConnections, SYNC_CONCURRENCY, Scheduler, SchedulerDeps,
    SourceSyncStatus, SyncEvents,
};
use sqlx::PgPool;

/// An adapter that sleeps in `sync`, and records the high-water mark of how
/// many of its instances were inside `sync` at once.
struct Slow {
    id: String,
    inside: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    dwell: Duration,
    /// A function, not a value: `SourceError` is not `Clone` (frozen SPI).
    fault: Option<fn() -> SourceError>,
}

#[async_trait::async_trait]
impl Source for Slow {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_kind: "slow".into(),
            name: "Slow".into(),
            capabilities: Vec::new(),
            adapter_version: "0.1.0".into(),
            auth_methods: Vec::new(),
            write_ops: Vec::new(),
            entity_kinds: vec![knobas_source::KindInfo {
                id: "ticket".into(),
                label: "T".into(),
                plural: "T".into(),
                monogram: "SL".into(),
                full_sync_exhaustive: true,
            }],
            config_schema: serde_json::json!({ "type": "object", "properties": {} }),
            // Nothing declared: this stand-in has no payload shapes to
            // read, so every path-driven read misses on it (#277).
            payload_paths: Vec::new(),
        }
    }
    async fn test_connection(&self) -> Result<knobas_source::ConnectionInfo, SourceError> {
        Ok(knobas_source::ConnectionInfo::default())
    }
    async fn sync(
        &self,
        _c: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        let now = self.inside.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        tokio::time::sleep(self.dwell).await;
        self.inside.fetch_sub(1, Ordering::SeqCst);
        if let Some(fault) = self.fault {
            return Err(fault());
        }
        sink.item(knobas_source::SyncItem {
            entity: knobas_core::entity::EntityRef::new(&self.id, "S-1"),
            kind: "ticket".into(),
            title: "slow".into(),
            body_text: String::new(),
            author: None,
            updated_at: None,
            payload: serde_json::json!({}),
            web_url: None,
            deleted: false,
        })
        .await?;
        Ok(r#"{"v":1,"n":1}"#.into())
    }
    async fn write(&self, _op: knobas_source::WriteOp) -> Result<(), SourceError> {
        Err(SourceError::protocol("read-only"))
    }
}

struct SlowRegistry {
    inside: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    dwell: Duration,
    fault: Option<fn() -> SourceError>,
}

impl AdapterRegistry for SlowRegistry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        Vec::new()
    }
    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        Ok(Box::new(Slow {
            id: instance.id,
            inside: Arc::clone(&self.inside),
            peak: Arc::clone(&self.peak),
            dwell: self.dwell,
            fault: self.fault,
        }))
    }
}

struct Silent;
impl SyncEvents for Silent {
    fn sync_state(&self, _s: SourceSyncStatus) {}
    fn source_health(&self, _h: knobas_sync::config::CredentialHealth) {}
    fn activity_new(&self, _r: knobas_core::activity::ActivityRow) {}
}

struct TestConnections(knobas_db::embedded::Connector);

#[async_trait::async_trait]
impl RunConnections for TestConnections {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        self.0.connect().await
    }
}

/// Serialises the tests in this binary.
///
/// Tokio's mutex rather than `std`'s, because the guard is held across the
/// whole test and therefore across every await in it -- which is what
/// `clippy::await_holding_lock` exists to forbid. It also does not poison, so
/// one failing test does not turn the other seven red and hide which one broke.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn serially() -> tokio::sync::MutexGuard<'static, ()> {
    SERIAL.lock().await
}

/// One caller's channel: everything it was told about a run, in order.
///
/// A test's stand-in for `knobas-app`'s `ChannelSink`, and the only way to say
/// what a caller *observed* -- which is the whole subject of ADR-0005. Reading
/// the log row instead would answer a different question, and would answer it
/// for a caller that heard nothing at all.
#[derive(Default)]
struct Heard(std::sync::Mutex<Vec<SyncProgress>>);

impl ProgressSink for Heard {
    fn report(&self, progress: SyncProgress) {
        self.0.lock().unwrap().push(progress);
    }
}

impl Heard {
    fn new() -> Arc<Heard> {
        Arc::new(Heard::default())
    }

    /// The same recorder, as the trait object `trigger` takes.
    fn sink(self: &Arc<Self>) -> Arc<dyn ProgressSink> {
        Arc::clone(self) as Arc<dyn ProgressSink>
    }

    fn all(&self) -> Vec<SyncProgress> {
        self.0.lock().unwrap().clone()
    }

    /// The run's ending, if this caller has been given one yet.
    fn ending(&self) -> Option<SyncProgress> {
        self.all()
            .into_iter()
            .find(|p| matches!(p.phase, SyncPhase::Finished | SyncPhase::Failed))
    }
}

/// Wait for the ending ADR-0005 promises this caller, and fail loudly without
/// one.
///
/// Polling the recorder rather than the log row: `settle` writes `finished_at`
/// before the run tells anybody, so a test that waited on the row would pass
/// over a caller that was never told -- which is precisely the defect.
async fn await_ending(heard: &Arc<Heard>) -> SyncProgress {
    for _ in 0..200 {
        if let Some(ending) = heard.ending() {
            return ending;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!(
        "no ending in five seconds; the caller was handed a run id it cannot \
         observe. Heard: {:?}",
        heard.all()
    );
}

/// A pool for the test's own reads, and a scheduler pool the scheduler will
/// close on shutdown. Two, because `Scheduler::shutdown` closes the one it was
/// given and a closed pool cannot serve the assertions afterwards.
async fn pools() -> (PgPool, PgPool) {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let scheduler_pool = knobas_db::test_util::test_pool().await;
    (pool, scheduler_pool)
}

/// Sources that no *other* test's ticker will pick up.
///
/// The database is shared per test binary and `config::due` is whole-database
/// by design (one scheduler per profile), so a source left enabled is a source
/// the next test's ticker syncs. Every test therefore cleans up after itself --
/// see [`retire`].
async fn seed(pool: &PgPool, n: usize) -> Vec<String> {
    let mut ids = Vec::new();
    for _ in 0..n {
        let id = format!("tick-{}", uuid::Uuid::new_v4().simple());
        config::insert(
            pool,
            &InsertConfig {
                id: id.clone(),
                adapter_kind: "slow".into(),
                display_name: "Slow".into(),
                base_url: String::new(),
                auth_kind: AuthKind::None,
                config: serde_json::json!({}),
                sync_interval_secs: 60,
                enabled: true,
            },
        )
        .await
        .unwrap();
        ids.push(id);
    }
    ids
}

/// Sources the ticker will not adopt, for the tests that drive both trigger
/// paths by hand.
///
/// The ADR-0005 tests below play the wizard and the wake against each other and
/// count the runs that result; a third, uncontrolled actor starting runs of its
/// own would make "exactly one run" a coin toss on a slow machine. Held off the
/// schedule rather than left free-running, because `config::due` respects
/// `backoff_until` and [`Scheduler::trigger`] deliberately does not -- which is
/// exactly the asymmetry these tests need. See [`retire`] for why that lever is
/// the backoff and no longer `enabled`.
async fn seed_quiet(pool: &PgPool, n: usize) -> Vec<String> {
    let ids = seed(pool, n).await;
    retire(pool, &ids).await;
    ids
}

/// Hold every source this test made off the schedule, so the next test's ticker
/// does not adopt them.
///
/// **`backoff_until`, not `enabled = false`** (issue #202). This used to disable
/// the source, which was a free lever while `enabled` meant only "the scheduler
/// may sync this". Migration `0012` gave it a second meaning -- a disabled
/// source's items leave `sync.live_item`, and therefore every reader -- so the
/// old mechanism made the mirror *invisible* to tests whose subject is the
/// purge, and four of them failed on an assertion about visibility they never
/// meant to make.
///
/// `config::due` clamps the schedule up by `backoff_until`, and
/// `Scheduler::trigger` consults neither it nor `enabled`, so this keeps
/// exactly the asymmetry these tests need while leaving the source *normal*:
/// enabled, mirrored, searchable. Nothing in this file asserts on
/// `backoff_until`, which is what makes it free to borrow.
async fn retire(pool: &PgPool, ids: &[String]) {
    for id in ids {
        sqlx::query(
            "update knobas.source_config
                set backoff_until = now() + interval '1 hour'
              where id = $1",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }
}

async fn deps(pool: PgPool, dwell: Duration) -> (SchedulerDeps, Arc<AtomicUsize>) {
    let (deps, peak, _inside) = slow_deps(pool, dwell).await;
    (deps, peak)
}

/// The same deps as [`deps`], returning the *inside* counter instead of the
/// peak: how many runs are in the adapter's `sync` right now.
///
/// What a test needs it for is timing it cannot otherwise have. `trigger`
/// returns as soon as the run's log row exists, but the run has not yet read
/// `knobas.source_config` -- and that read is the one existence check a run
/// makes, so a test that deletes the source before it happens gets a run that
/// refuses and writes nothing, which is not the interleaving it meant to
/// arrange. `Slow` bumps this counter on entry to `sync`, which is *after*
/// that read, so waiting for it puts the test in the window it is about.
async fn deps_watching_the_adapter(
    pool: PgPool,
    dwell: Duration,
) -> (SchedulerDeps, Arc<AtomicUsize>) {
    let (deps, _peak, inside) = slow_deps(pool, dwell).await;
    (deps, inside)
}

async fn slow_deps(
    pool: PgPool,
    dwell: Duration,
) -> (SchedulerDeps, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let connector = knobas_db::test_util::test_connector().await;
    let peak = Arc::new(AtomicUsize::new(0));
    let inside = Arc::new(AtomicUsize::new(0));
    (
        SchedulerDeps {
            pool,
            connections: Arc::new(TestConnections(connector)),
            registry: Arc::new(SlowRegistry {
                inside: Arc::clone(&inside),
                peak: Arc::clone(&peak),
                dwell,
                fault: None,
            }),
            secrets: Arc::new(MemoryStore::new()),
            events: Arc::new(Silent),
        },
        peak,
        inside,
    )
}

/// Carry-over: a sync wave must not be able to consume everything at once. The
/// cap is what makes that a property rather than a hope -- and with runs on
/// their own connections it is the *only* bound on how many exist.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_runs_never_exceed_the_cap() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed(&pool, SYNC_CONCURRENCY + 3).await;
    let (deps, peak) = deps(sched_pool, Duration::from_millis(600)).await;

    let scheduler = Scheduler::start(deps).await.unwrap();
    for id in &ids {
        scheduler
            .trigger(id, SyncTrigger::Manual, None)
            .await
            .unwrap();
    }
    // Long enough for every triggered run to have finished.
    tokio::time::sleep(Duration::from_secs(6)).await;
    scheduler.shutdown().await;

    // **Equality, not `<=`.** Three sources more than the cap are triggered at
    // once against a 600 ms dwell, so the semaphore is what decides the peak
    // and the peak is what it decides. A `<=` here would read the very constant
    // it is checking: raising `SYNC_CONCURRENCY` to 30 raises the bound *and*
    // the peak, and the test stays green while the cap is gone. (Observed --
    // that mutation survived the first draft of this assertion.)
    assert_eq!(
        peak.load(Ordering::SeqCst),
        SYNC_CONCURRENCY,
        "{} sources triggered at once must saturate the cap of {SYNC_CONCURRENCY} \
         and never exceed it",
        ids.len()
    );
    for id in &ids {
        let row = &run_log::list(&pool, Some(id), 1).await.unwrap()[0];
        assert!(row.finished_at.is_some(), "{id} never finished");
    }
    retire(&pool, &ids).await;
}

/// P3: `sync_now` returns the run id at once. A second *Sync now* while the
/// first is still going must not start a second run of the same source -- the
/// advisory lock would serialise them anyway, and the user would be watching
/// two progress bars for one job.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_second_trigger_joins_the_run_already_in_flight() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed(&pool, 1).await;
    let id = ids[0].clone();
    let (deps, _) = deps(sched_pool, Duration::from_millis(600)).await;

    let scheduler = Scheduler::start(deps).await.unwrap();
    let first = scheduler
        .trigger(&id, SyncTrigger::Manual, None)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let second = scheduler
        .trigger(&id, SyncTrigger::Manual, None)
        .await
        .unwrap();
    assert_eq!(
        first, second,
        "the caller is handed the run that is already going"
    );

    tokio::time::sleep(Duration::from_secs(2)).await;
    scheduler.shutdown().await;
    assert_eq!(
        run_log::list(&pool, Some(&id), 10).await.unwrap().len(),
        1,
        "one run, one row"
    );
    retire(&pool, &ids).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn triggering_a_source_that_does_not_exist_is_refused_without_a_log_row() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let (deps, _) = deps(sched_pool, Duration::from_millis(10)).await;
    let scheduler = Scheduler::start(deps).await.unwrap();

    let missing = format!("nope-{}", uuid::Uuid::new_v4().simple());
    match scheduler.trigger(&missing, SyncTrigger::Manual, None).await {
        Err(knobas_sync::scheduler::TriggerError::UnknownSource(got)) => assert_eq!(got, missing),
        other => panic!("expected UnknownSource, got {other:?}"),
    }
    assert!(
        run_log::list(&pool, Some(&missing), 10)
            .await
            .unwrap()
            .is_empty()
    );
    scheduler.shutdown().await;
}

/// The ticker picks a due source up on its own, without anyone triggering it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_ticker_runs_a_due_source_by_itself() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed(&pool, 1).await;
    let id = ids[0].clone();
    let (deps, _) = deps(sched_pool, Duration::from_millis(50)).await;

    let scheduler = Scheduler::start(deps).await.unwrap();
    // A never-run source is due immediately; the first tick is one STARTUP_DELAY
    // away, and the second one TICK after that -- so this window covers both and
    // would catch a ticker that started the same source twice.
    tokio::time::sleep(Duration::from_secs(9)).await;
    scheduler.shutdown().await;

    let runs = run_log::list(&pool, Some(&id), 10).await.unwrap();
    assert_eq!(
        runs.len(),
        1,
        "due once, run once -- not once per tick: {runs:?}"
    );
    assert_eq!(runs[0].trigger, SyncTrigger::FirstRun);
    // And it is not due again: the interval runs from the finish.
    assert!(config::due(&pool).await.unwrap().iter().all(|d| d.id != id));
    retire(&pool, &ids).await;
}

/// Carry-over: "quitting mid-sync stalls on `pool.close()` until the run's
/// transaction drains". A 30-second Jira page would be a 30-second hang on
/// Cmd-Q. Shutdown must be bounded, and the interrupted run must not be left
/// looking like it is still going.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shutdown_during_a_long_run_returns_promptly_and_leaves_no_open_run() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed(&pool, 1).await;
    let id = ids[0].clone();
    // Far longer than the shutdown grace period.
    let (deps, _) = deps(sched_pool, Duration::from_secs(30)).await;

    let scheduler = Scheduler::start(deps).await.unwrap();
    scheduler
        .trigger(&id, SyncTrigger::Manual, None)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    let started = std::time::Instant::now();
    scheduler.shutdown().await;
    let took = started.elapsed();
    // Well inside the three-second grace period, because the run **notices**
    // the cancellation at its next await point rather than being waited out and
    // then aborted. A bound of ten seconds would be satisfied by the abort
    // backstop alone, so it would say nothing about the token -- that mutation
    // survived the first draft of this test.
    assert!(
        took < Duration::from_secs(2),
        "shutdown took {took:?}; a cancelled run stops at its next await, it is \
         not waited out"
    );

    // And it closed its **own** row on the way out. The next start's
    // `reconcile_abandoned` is the backstop for a process that was killed, not
    // the normal path: a clean quit must not leave the diagnostics view showing
    // a run as still going until the app is launched again.
    let row = &run_log::list(&pool, Some(&id), 1).await.unwrap()[0];
    assert!(
        row.finished_at.is_some(),
        "the cancelled run left its log row open"
    );
    assert_eq!(
        row.outcome,
        Some(knobas_sync::run_log::SyncOutcome::Error),
        "an interrupted run is an error in the diagnostics view, not a gap"
    );
    assert!(
        row.error
            .as_deref()
            .unwrap_or_default()
            .contains("cancelled"),
        "the row should say why it stopped: {:?}",
        row.error
    );
    assert_eq!(
        run_log::reconcile_abandoned(&pool).await.unwrap(),
        0,
        "nothing was left for the reconciler to close"
    );

    // The write itself never landed: the run was cancelled inside its
    // transaction, so the rollback is what the store saw.
    let (items,): (i64,) = sqlx::query_as("select count(*) from sync.item where source_id = $1")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(items, 0);
    retire(&pool, &ids).await;
}

/// The other half of the same carry-over, and the one worth measuring: a
/// cancelled run must not leave its connection holding the source's advisory
/// lock, or the *next* launch's first run of that source blocks for ever.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_cancelled_run_leaves_the_source_lock_free() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed(&pool, 1).await;
    let id = ids[0].clone();
    let (deps, _) = deps(sched_pool, Duration::from_secs(30)).await;

    let scheduler = Scheduler::start(deps).await.unwrap();
    scheduler
        .trigger(&id, SyncTrigger::Manual, None)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    scheduler.shutdown().await;

    let (free,): (bool,) = tokio::time::timeout(
        Duration::from_secs(5),
        sqlx::query_as("select pg_try_advisory_lock(hashtext($1::text))")
            .bind(&id)
            .fetch_one(&pool),
    )
    .await
    .expect("asking for the lock must not block")
    .unwrap();
    assert!(
        free,
        "the cancelled run's connection is still holding the source's lock"
    );
    retire(&pool, &ids).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn triggering_after_shutdown_is_refused_rather_than_spawning_into_a_closed_pool() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed(&pool, 1).await;
    let id = ids[0].clone();
    let (deps, _) = deps(sched_pool, Duration::from_millis(10)).await;

    let scheduler = Scheduler::start(deps).await.unwrap();
    scheduler.shutdown().await;
    assert!(matches!(
        scheduler.trigger(&id, SyncTrigger::Manual, None).await,
        Err(knobas_sync::scheduler::TriggerError::ShuttingDown)
    ));
    scheduler.shutdown().await; // idempotent
    retire(&pool, &ids).await;
}

/// `sync_all` skips what it must not touch: a disabled source, and one whose
/// credential needs a human (P7 -- retrying a 401 on a timer is how an account
/// gets locked out).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn trigger_all_skips_the_disabled_and_the_ones_needing_a_human() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed(&pool, 3).await;
    let (runnable, disabled, rejected) = (&ids[0], &ids[1], &ids[2]);

    config::patch(
        &pool,
        disabled,
        &config::PatchConfig {
            enabled: Some(false),
            ..config::PatchConfig::default()
        },
    )
    .await
    .unwrap();
    config::set_health(
        &pool,
        rejected,
        knobas_sync::config::AuthState::Unauthorized,
        Some("401"),
        None,
    )
    .await
    .unwrap();

    let (deps, _) = deps(sched_pool, Duration::from_millis(20)).await;
    let scheduler = Scheduler::start(deps).await.unwrap();
    let started = scheduler.trigger_all().await.unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    scheduler.shutdown().await;

    // Other tests' sources may be in the same database, so the assertion is
    // about *these three* rather than about the count.
    for (id, should_run) in [(runnable, true), (disabled, false), (rejected, false)] {
        let runs = run_log::list(&pool, Some(id), 10).await.unwrap();
        assert_eq!(
            !runs.is_empty(),
            should_run,
            "{id}: expected run={should_run}, got {} rows",
            runs.len()
        );
    }
    assert!(!started.is_empty());
    retire(&pool, &ids).await;
}

/// `trigger` says `running` **before it returns**.
///
/// P3 hands the caller a run id at once and the command's docs promise
/// `sync:state` carries the run; emitting from inside the spawned task would
/// make that a race the frontend loses on a busy machine -- and loses silently,
/// because the terminal event still arrives. The assertion is therefore taken
/// the instant `trigger` returns, with nothing awaited in between.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_trigger_says_running_before_it_returns() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed(&pool, 1).await;
    let id = ids[0].clone();

    #[derive(Default)]
    struct Recorder(std::sync::Mutex<Vec<SourceSyncStatus>>);
    impl SyncEvents for Recorder {
        fn sync_state(&self, s: SourceSyncStatus) {
            self.0.lock().unwrap().push(s);
        }
        fn source_health(&self, _h: knobas_sync::config::CredentialHealth) {}
        fn activity_new(&self, _r: knobas_core::activity::ActivityRow) {}
    }

    let events = Arc::new(Recorder::default());
    let connector = knobas_db::test_util::test_connector().await;
    let scheduler = Scheduler::start(SchedulerDeps {
        pool: sched_pool,
        connections: Arc::new(TestConnections(connector)),
        registry: Arc::new(SlowRegistry {
            inside: Arc::new(AtomicUsize::new(0)),
            peak: Arc::new(AtomicUsize::new(0)),
            dwell: Duration::from_secs(5),
            fault: None,
        }),
        secrets: Arc::new(MemoryStore::new()),
        events: Arc::clone(&events) as Arc<dyn SyncEvents>,
    })
    .await
    .unwrap();

    let run_id = scheduler
        .trigger(&id, SyncTrigger::Manual, None)
        .await
        .unwrap();
    // No await between the trigger and this read.
    let seen = events.0.lock().unwrap().clone();
    let first = seen
        .first()
        .expect("a running sync:state must be emitted before trigger returns");
    assert!(first.running);
    assert_eq!(first.run_id, Some(run_id));
    assert_eq!(first.source_id, id);
    assert!(
        first.next_run_at.is_none(),
        "a run in flight has no next time to count down to"
    );

    scheduler.shutdown().await;
    retire(&pool, &ids).await;
}

/// The `running` `sync:state` names **the run that just started**, even when
/// another run for the same source is already open (#304).
///
/// `status_for`'s `r` lateral takes whichever run is open for the source,
/// `started_at desc` -- so a second open run decides what every emit for that
/// source carries. That happens whenever more than one scheduler is live over
/// one `source_config` (the app's test binary), and it is what makes an id a
/// caller was handed unobservable: ADR-0005's ending is delivered by run id.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_running_state_names_its_own_run_not_another_open_one() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    // Off the schedule: this test is about what `trigger` emits, and a ticker
    // starting a run of its own would put a third id in play.
    let ids = seed_quiet(&pool, 1).await;
    let id = ids[0].clone();

    #[derive(Default)]
    struct Recorder(std::sync::Mutex<Vec<SourceSyncStatus>>);
    impl SyncEvents for Recorder {
        fn sync_state(&self, s: SourceSyncStatus) {
            self.0.lock().unwrap().push(s);
        }
        fn source_health(&self, _h: knobas_sync::config::CredentialHealth) {}
        fn activity_new(&self, _r: knobas_core::activity::ActivityRow) {}
    }

    let events = Arc::new(Recorder::default());
    let connector = knobas_db::test_util::test_connector().await;
    let scheduler = Scheduler::start(SchedulerDeps {
        pool: sched_pool,
        connections: Arc::new(TestConnections(connector)),
        registry: Arc::new(SlowRegistry {
            inside: Arc::new(AtomicUsize::new(0)),
            peak: Arc::new(AtomicUsize::new(0)),
            dwell: Duration::from_secs(5),
            fault: None,
        }),
        secrets: Arc::new(MemoryStore::new()),
        events: Arc::clone(&events) as Arc<dyn SyncEvents>,
    })
    .await
    .unwrap();

    // *After* `Scheduler::start`, which closes every open run it finds
    // (`reconcile_abandoned`). Dated ahead, because the lateral takes the
    // newest open run and this test must not depend on which of two rows
    // Postgres returns first.
    let (neighbour,): (i64,) = sqlx::query_as(
        "insert into knobas.sync_run (source_id, trigger, started_at)
         values ($1, 'schedule', now() + interval '1 minute') returning id",
    )
    .bind(&id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let run_id = scheduler
        .trigger(&id, SyncTrigger::Manual, None)
        .await
        .unwrap();
    // No await between the trigger and this read, as in
    // `a_trigger_says_running_before_it_returns`.
    let seen = events.0.lock().unwrap().clone();
    let first = seen
        .first()
        .expect("a running sync:state must be emitted before trigger returns");
    assert!(first.running);
    assert_eq!(
        first.run_id,
        Some(run_id),
        "the emit must name the run `trigger` returned ({run_id}), not the \
         neighbour ({neighbour}) that happens to be open too"
    );
    let row = run_log::get(&pool, run_id)
        .await
        .unwrap()
        .expect("the run trigger opened");
    assert_eq!(
        first.started_at,
        Some(row.started_at),
        "and it carries that run's start time, not the neighbour's"
    );

    scheduler.shutdown().await;
    // Close the neighbour by hand. `retire` holds the *source* off the
    // schedule, but nothing holds off the next `Scheduler::start` in this
    // binary: `reconcile_abandoned` is whole-database, so an open row left
    // here would be closed by the next test's scheduler and counted as one of
    // its own abandoned runs.
    sqlx::query("update knobas.sync_run set finished_at = now(), outcome = 'error' where id = $1")
        .bind(neighbour)
        .execute(&pool)
        .await
        .unwrap();
    retire(&pool, &ids).await;
}

/// **Every phase the mirror declares is actually emitted by a real run**, and
/// no message claims the run took no time at all.
///
/// The declaration half -- that the five spellings agree across the bridge --
/// is `progress::the_phase_names_match_their_typescript_mirror`. This is the
/// stronger half, and it was missing: `Started` was declared on both sides and
/// sent by nobody, `Writing` was sent from the scheduler *after* the run had
/// finished (and on the failure path, where nothing was written), and the
/// terminal message carried a hardcoded `elapsed_ms: 0`. A phase the UI can
/// never observe invites a frontend to wait for a state that never arrives.
///
/// Driven through `Scheduler::trigger`, because the terminal message is the
/// ticker's: a test that drove only `execute_run` could not see
/// `Finished`/`Failed` at all, which is how the zero survived. The expected set
/// is read out of the TypeScript union rather than listed here -- a list this
/// test owns is a list this test can quietly shrink.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_declared_phase_is_actually_emitted() {
    let _serial = serially().await;
    let declared = knobas_sync::mirror::declared_union(
        include_str!("../../../app/src/lib/ipc/sources.ts"),
        "SyncPhase",
    );

    // A healthy run and a failing one: `Finished` and `Failed` are mutually
    // exclusive, so neither alone covers the union.
    let (healthy, healthy_ids) = phases_of(None).await;
    let (failed, failed_ids) =
        phases_of(Some(|| SourceError::Unreachable("simulated".into()))).await;

    let mut seen: Vec<String> = healthy.iter().chain(failed.iter()).cloned().collect();
    seen.sort();
    seen.dedup();
    let mut want = declared.clone();
    want.sort();
    assert_eq!(
        seen, want,
        "the mirror declares {declared:?}; real runs emit {seen:?}. A phase \
         nothing sends is a state the UI can wait for for ever."
    );

    assert!(
        healthy.contains(&"finished".to_owned()) && !healthy.contains(&"failed".to_owned()),
        "a healthy run ends Finished: {healthy:?}"
    );
    assert!(
        failed.contains(&"failed".to_owned()) && !failed.contains(&"finished".to_owned()),
        "a failed run ends Failed: {failed:?}"
    );
    assert!(
        !failed.contains(&"writing".to_owned()),
        "nothing was written, so nothing may say it was: {failed:?}"
    );
    assert_eq!(
        healthy.first().map(String::as_str),
        Some("started"),
        "the first thing a watcher hears is that the run started: {healthy:?}"
    );

    let (pool, _) = pools().await;
    retire(&pool, &healthy_ids).await;
    retire(&pool, &failed_ids).await;
}

/// One run through the whole ticker, as wire spellings, plus the source ids to
/// retire afterwards.
async fn phases_of(fault: Option<fn() -> SourceError>) -> (Vec<String>, Vec<String>) {
    let (pool, sched_pool) = pools().await;
    let ids = seed(&pool, 1).await;
    let id = ids[0].clone();
    let connector = knobas_db::test_util::test_connector().await;
    let scheduler = Scheduler::start(SchedulerDeps {
        pool: sched_pool,
        connections: Arc::new(TestConnections(connector)),
        registry: Arc::new(SlowRegistry {
            inside: Arc::new(AtomicUsize::new(0)),
            peak: Arc::new(AtomicUsize::new(0)),
            // Long enough that `elapsed_ms` cannot round to zero on a fast
            // machine, short enough not to slow the suite.
            dwell: Duration::from_millis(30),
            fault,
        }),
        secrets: Arc::new(MemoryStore::new()),
        events: Arc::new(Silent),
    })
    .await
    .unwrap();

    let sink = Heard::new();
    let run_id = scheduler
        .trigger(&id, SyncTrigger::Manual, Some(sink.sink()))
        .await
        .unwrap();

    // Wait for the terminal message rather than for the log row: it is the last
    // thing the run does, so anything earlier can read a half-finished list.
    await_ending(&sink).await;
    scheduler.shutdown().await;

    let seen = sink.all();
    assert!(!seen.is_empty(), "a channel that was attached saw nothing");
    assert!(
        seen.iter().all(|p| p.run_id == run_id && p.source_id == id),
        "every message names its run: {seen:?}"
    );
    let terminal = seen.last().expect("a terminal message");
    assert!(
        terminal.elapsed_ms > 0,
        "the terminal message reports the run as instantaneous -- the \
         hardcoded zero this test exists to catch: {terminal:?}"
    );

    let phases = seen
        .iter()
        .map(|p| {
            serde_json::to_value(p.phase)
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    (phases, ids)
}

/// **The backfill has a trigger spelling of its own, and it is not `manual`.**
///
/// Ratified by Björn 2026-08-28 alongside the non-sweeping rule, and the two
/// are one decision: a backfill is the only run that is *forbidden* to
/// tombstone, so its `swept` count is `0` by construction. The question anybody
/// asks of a surprising tombstone count in the diagnostics list is which run
/// produced it, and a backfill logged as `manual` is indistinguishable from
/// *Sync now* -- so that question has no answer, and a reader would credit a
/// sweeping manual run's tombstones to a run that cannot sweep. Migration 0004
/// widened `sync_run_trigger_chk` for exactly this.
///
/// Both runs, in one test, because the claim is a *distinction*: asserting
/// `backfill` alone would still pass if `Manual` had been renamed. The rows
/// have to differ, and the manual one has to still say `manual`.
///
/// **Two sources, one each, deliberately.** The same source twice would need
/// the first run to be over before the second is triggered, and "over" has two
/// meanings here that do not coincide: `settle` writes `finished_at` and only
/// then does `run_task` close the run's watchers, so a poll on the log row can
/// win that race and the second trigger -- arriving while the watchers are
/// still open -- is enrolled in the first run and handed its id. (Observed --
/// the first draft of this test failed exactly there, `left: 5, right: 5`.)
/// ADR-0005 did not close this window: it changed what the second caller *hears*
/// (an ending, rather than silence), not when the source stops being claimed.
/// Two sources have nothing to serialise.
///
/// This goes through [`Scheduler::backfill`] rather than `execute_run`, which
/// is the only way the spelling is under test at all -- `execute_run`'s callers
/// hand it a trigger, so a test there asserts its own argument back.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_backfill_is_logged_under_its_own_trigger() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed(&pool, 2).await;
    let (synced, backfilled) = (ids[0].clone(), ids[1].clone());
    let (deps, _) = deps(sched_pool, Duration::from_millis(10)).await;

    let scheduler = Scheduler::start(deps).await.unwrap();
    let manual_run = scheduler
        .trigger(&synced, SyncTrigger::Manual, None)
        .await
        .unwrap();
    let backfill_run = scheduler.backfill(&backfilled).await.unwrap();
    await_finish(&pool, manual_run).await;
    await_finish(&pool, backfill_run).await;
    scheduler.shutdown().await;

    let logged = |runs: Vec<run_log::SyncRunRow>, run_id: i64| {
        runs.into_iter()
            .find(|r| r.id == run_id)
            .unwrap_or_else(|| panic!("no log row for run {run_id}"))
            .trigger
    };
    let manual_rows = run_log::list(&pool, Some(&synced), 10).await.unwrap();
    let backfill_rows = run_log::list(&pool, Some(&backfilled), 10).await.unwrap();

    assert_ne!(
        manual_run, backfill_run,
        "two runs, or there is nothing to tell apart"
    );
    assert_eq!(
        logged(backfill_rows, backfill_run),
        SyncTrigger::Backfill,
        "a backfill must be readable as one in the log, not as a *Sync now*"
    );
    assert_eq!(
        logged(manual_rows, manual_run),
        SyncTrigger::Manual,
        "*Sync now* keeps its own spelling"
    );
    retire(&pool, &ids).await;
}

/// Wait for one run's log row to be finished. Polls rather than sleeping a
/// fixed span: the row is written by the spawned task, and a fixed sleep is
/// either a flake or slow. Note what this does *not* promise -- see the two
/// meanings of "over" on the test above.
async fn await_finish(pool: &PgPool, run_id: i64) {
    for _ in 0..200 {
        let finished = run_log::list(pool, None, 50)
            .await
            .unwrap()
            .iter()
            .any(|r| r.id == run_id && r.finished_at.is_some());
        if finished {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("run {run_id} never finished");
}

// -- ADR-0005: a run id always comes with an ending ---------------------------

/// **A run holds a set of sinks, and the second one does not displace the
/// first.**
///
/// The dedupe that hands a second caller the run already in flight is the
/// behaviour that makes a double-clicked *Sync now* harmless, and it used to
/// drop that caller's sink on the floor: an id, and then silence. Both callers
/// hear the run now -- its progress while it runs and its ending when it ends
/// -- and neither costs the other anything.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_callers_watching_one_run_both_hear_it_end() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed(&pool, 1).await;
    let id = ids[0].clone();
    let (deps, _) = deps(sched_pool, Duration::from_millis(600)).await;
    let scheduler = Scheduler::start(deps).await.unwrap();

    let starter = Heard::new();
    let joiner = Heard::new();
    let first = scheduler
        .trigger(&id, SyncTrigger::Manual, Some(starter.sink()))
        .await
        .unwrap();
    // Well inside the adapter's dwell, so the second caller joins a run that is
    // genuinely still going rather than one that is already over.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let second = scheduler
        .trigger(&id, SyncTrigger::Manual, Some(joiner.sink()))
        .await
        .unwrap();
    assert_eq!(first, second, "one run, two watchers");

    let by_starter = await_ending(&starter).await;
    let by_joiner = await_ending(&joiner).await;
    scheduler.shutdown().await;

    assert_eq!(by_starter.phase, SyncPhase::Finished);
    assert_eq!(by_joiner.phase, SyncPhase::Finished);
    assert_eq!((by_starter.run_id, by_joiner.run_id), (first, first));
    assert_eq!(by_starter.items, by_joiner.items, "one run, one count");
    // Progress, not only the ending: the joiner attached while the adapter was
    // still fetching, and a set of sinks that only fanned out the terminal
    // message would leave its progress bar at zero until the run was over.
    assert!(
        joiner.all().len() > 1,
        "the joiner heard only its ending: {:?}",
        joiner.all()
    );
    assert_eq!(
        starter.all().first().map(|p| p.phase),
        Some(SyncPhase::Started),
        "the caller that started the run still hears it start"
    );
    assert_eq!(
        run_log::list(&pool, Some(&id), 10).await.unwrap().len(),
        1,
        "one run, one row"
    );
    retire(&pool, &ids).await;
}

/// **The invariant ADR-0005 says a later reader will simplify away: a caller
/// that attaches to a run which has already finished is served its ending, at
/// once, from the record.**
///
/// This is the interleaving the first-run wizard actually loses. `add_source`
/// wakes the scheduler, the scheduler's run of a brand-new source is the whole
/// first sync, and on a fast source it can be over before the wizard's own
/// trigger reaches the engine. The wizard then used to start a *second* run
/// over an already-mirrored corpus and report what that one wrote, which was
/// nothing.
///
/// So the second trigger starts nothing: it is handed the run that already
/// happened, together with a terminal message synthesised from that run's log
/// row. The assertion is taken with nothing awaited after the trigger returns,
/// because "eventually" is what a caller cannot tell apart from a hang.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_first_run_trigger_after_the_run_ended_is_served_the_ending_from_the_record() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed_quiet(&pool, 1).await;
    let id = ids[0].clone();
    let (deps, _) = deps(sched_pool, Duration::from_millis(300)).await;
    let scheduler = Scheduler::start(deps).await.unwrap();

    // The scheduler's own first run of a brand-new source: `FirstRun` and no
    // channel, which is exactly what the ticker does with a source `add_source`
    // just woke it for. **No sink here**, because a `FirstRun` caller carrying
    // one is served the source's first sync and spends the claim on it -- and
    // then the wizard below would be a *second* asker rather than the first,
    // which is not the interleaving under test.
    let scheduled = scheduler
        .trigger(&id, SyncTrigger::FirstRun, None)
        .await
        .unwrap();
    // Observed under `Manual` -- *Sync now* joining a run in flight -- only so
    // the test can know that run is *completely* over: `settle` writes
    // `finished_at` before the run tells anyone, so waiting on the log row
    // would leave the run's sinks still open and test the in-flight path by
    // accident. `Manual` asks for work rather than for the first sync, so it
    // does not spend the claim either.
    let observer = Heard::new();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        scheduler
            .trigger(&id, SyncTrigger::Manual, Some(observer.sink()))
            .await
            .unwrap(),
        scheduled,
        "the observer joined the wake's run rather than starting one"
    );
    await_ending(&observer).await;

    let wizard = Heard::new();
    let handed = scheduler
        .trigger(&id, SyncTrigger::FirstRun, Some(wizard.sink()))
        .await
        .unwrap();
    // No await in between: immediately, or not at all.
    let served = wizard.all();
    scheduler.shutdown().await;

    assert_eq!(
        handed, scheduled,
        "the wizard is handed the run that already happened, not a second one"
    );
    let rows = run_log::list(&pool, Some(&id), 10).await.unwrap();
    assert_eq!(rows.len(), 1, "adding a source produces one run: {rows:?}");
    let row = &rows[0];
    assert_eq!(row.trigger, SyncTrigger::FirstRun);

    assert_eq!(
        served.len(),
        1,
        "a caller that arrives after the end hears the ending and nothing else: {served:?}"
    );
    let ending = &served[0];
    // Faithful to the record, field by field -- the row is where a later reader
    // gets its answer, so it is where this one comes from.
    assert_eq!(ending.phase, SyncPhase::Finished);
    assert_eq!(ending.run_id, scheduled);
    assert_eq!(ending.source_id, id);
    assert_eq!(i64::try_from(ending.items).unwrap(), row.upserted);
    assert_eq!(ending.message, row.error);
    assert!(
        ending.elapsed_ms > 0,
        "the run took time and the ending says so: {ending:?}"
    );
    retire(&pool, &ids).await;
}

/// **A failure reads the same whether it was watched live or joined
/// afterwards** -- and *Retry* is still a retry.
///
/// The wizard's failure panel, with its *Retry* and *Skip for now*, is driven
/// entirely by the phase and the message on this channel. A caller that joined
/// after the run failed therefore has to be given the failure phase and the
/// error text the run stored, or it lands on a success panel over a sync that
/// did not happen.
///
/// The second half of the test is why the claim is spent when it is taken:
/// *Retry* is the same command asking a second time, and a *Retry* that replayed
/// the failure it was retrying would be a button that does nothing.
///
/// **The interleaving is the production one, deliberately.** The run is started
/// by the wake -- `FirstRun` with no channel of its own -- and the live watcher
/// joins it under `Manual`, which is *Sync now* pressed while the first sync is
/// going. Neither spends the claim on the source's first sync, so the wizard
/// arriving afterwards is genuinely the first caller owed that answer. An
/// earlier draft had the live watcher itself trigger `FirstRun`, which meant
/// three callers where production has two -- and the third one silently spent
/// the claim, so the *Retry* assertion below passed without the claim ever
/// being spent by the caller that pressed the button. See
/// [`the_wizards_retry_after_watching_its_own_run_fail_starts_a_run`], which is
/// the case that draft could not see.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failure_reads_the_same_live_or_joined_and_retry_still_runs() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed_quiet(&pool, 1).await;
    let id = ids[0].clone();
    let connector = knobas_db::test_util::test_connector().await;
    let scheduler = Scheduler::start(SchedulerDeps {
        pool: sched_pool,
        connections: Arc::new(TestConnections(connector)),
        registry: Arc::new(SlowRegistry {
            inside: Arc::new(AtomicUsize::new(0)),
            peak: Arc::new(AtomicUsize::new(0)),
            // Long enough that the live watcher joins a run that is genuinely
            // still going; the fault is raised after the dwell.
            dwell: Duration::from_millis(400),
            fault: Some(|| SourceError::Unreachable("simulated".into())),
        }),
        secrets: Arc::new(MemoryStore::new()),
        events: Arc::new(Silent),
    })
    .await
    .unwrap();

    // The wake's run: `add_source` pokes the scheduler, and the ticker triggers
    // with no channel of its own.
    let failed = scheduler
        .trigger(&id, SyncTrigger::FirstRun, None)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let live = Heard::new();
    assert_eq!(
        scheduler
            .trigger(&id, SyncTrigger::Manual, Some(live.sink()))
            .await
            .unwrap(),
        failed,
        "the live watcher joined the run that was going, rather than starting one"
    );
    let watched = await_ending(&live).await;

    let late = Heard::new();
    let handed = scheduler
        .trigger(&id, SyncTrigger::FirstRun, Some(late.sink()))
        .await
        .unwrap();
    let joined = late.all();
    assert_eq!(handed, failed, "the same run, not a second attempt at it");
    assert_eq!(joined.len(), 1, "the ending, immediately: {joined:?}");
    let joined = &joined[0];

    assert_eq!(joined.phase, SyncPhase::Failed);
    assert!(
        joined
            .message
            .as_deref()
            .unwrap_or_default()
            .contains("simulated"),
        "the stored error text is what the panel renders: {joined:?}"
    );
    // Field by field against what the live watcher was told. `elapsed_ms` is
    // the one that cannot match: the run measures it from a monotonic clock and
    // the record from two timestamps, so the claim there is only that neither
    // reports the run as instantaneous.
    assert_eq!(
        (
            joined.phase,
            joined.run_id,
            joined.source_id.as_str(),
            joined.items,
            joined.message.as_deref()
        ),
        (
            watched.phase,
            watched.run_id,
            watched.source_id.as_str(),
            watched.items,
            watched.message.as_deref()
        ),
        "a caller cannot tell watching live from joining late"
    );
    assert!(joined.elapsed_ms > 0 && watched.elapsed_ms > 0);

    // *Retry*: the claim was spent by the caller that took it, so this one gets
    // work rather than yesterday's news.
    let retry = Heard::new();
    let retried = scheduler
        .trigger(&id, SyncTrigger::FirstRun, Some(retry.sink()))
        .await
        .unwrap();
    assert_ne!(
        retried, failed,
        "Retry must start a run, not replay the failure it is retrying"
    );
    assert_eq!(await_ending(&retry).await.phase, SyncPhase::Failed);
    scheduler.shutdown().await;
    retire(&pool, &ids).await;
}

/// **The wizard's *Retry* starts a run even when the wizard watched the run it
/// is retrying.**
///
/// The claim on a source's first sync is spent by whoever is served it, and
/// there are two ways to be served: enrolled in the run while it is going, or
/// handed its recorded ending afterwards. Only the second is visible from a
/// test that puts a third caller between the failure and the *Retry* --
/// [`a_failure_reads_the_same_live_or_joined_and_retry_still_runs`] used to,
/// and the missing half was the ordinary case: the wizard triggers, watches its
/// own run fail, and presses the button. If enrolment does not spend the claim,
/// that press is served the failure it is retrying, starts nothing, and *Retry*
/// needs pressing twice to do anything.
///
/// Two callers, which is what production has: this run has no late joiner.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_wizards_retry_after_watching_its_own_run_fail_starts_a_run() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed_quiet(&pool, 1).await;
    let id = ids[0].clone();
    let connector = knobas_db::test_util::test_connector().await;
    let scheduler = Scheduler::start(SchedulerDeps {
        pool: sched_pool,
        connections: Arc::new(TestConnections(connector)),
        registry: Arc::new(SlowRegistry {
            inside: Arc::new(AtomicUsize::new(0)),
            peak: Arc::new(AtomicUsize::new(0)),
            dwell: Duration::from_millis(20),
            fault: Some(|| SourceError::Unreachable("simulated".into())),
        }),
        secrets: Arc::new(MemoryStore::new()),
        events: Arc::new(Silent),
    })
    .await
    .unwrap();

    let wizard = Heard::new();
    let failed = scheduler
        .trigger(&id, SyncTrigger::FirstRun, Some(wizard.sink()))
        .await
        .unwrap();
    assert_eq!(await_ending(&wizard).await.phase, SyncPhase::Failed);

    // *Retry*: the same command asking a second time, with nobody else having
    // touched the source in between.
    let retry = Heard::new();
    let retried = scheduler
        .trigger(&id, SyncTrigger::FirstRun, Some(retry.sink()))
        .await
        .unwrap();
    assert_eq!(await_ending(&retry).await.phase, SyncPhase::Failed);
    scheduler.shutdown().await;

    assert_ne!(
        retried, failed,
        "Retry was handed the run it is retrying: the button does nothing"
    );
    assert_eq!(
        run_log::list(&pool, Some(&id), 10).await.unwrap().len(),
        2,
        "a retry is a run, and the log should say so"
    );
    retire(&pool, &ids).await;
}

/// **The wizard that joined a run already in flight still gets a run when it
/// retries.**
///
/// The same claim, spent on the other path: here the wake started the run and
/// the wizard was enrolled in it mid-flight, which is the interleaving
/// `add_source`'s wake produces on an ordinary machine. The press afterwards
/// must still reach the scheduler.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_wizard_that_joined_the_wakes_run_still_gets_a_run_when_it_retries() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed_quiet(&pool, 1).await;
    let id = ids[0].clone();
    let connector = knobas_db::test_util::test_connector().await;
    let scheduler = Scheduler::start(SchedulerDeps {
        pool: sched_pool,
        connections: Arc::new(TestConnections(connector)),
        registry: Arc::new(SlowRegistry {
            inside: Arc::new(AtomicUsize::new(0)),
            peak: Arc::new(AtomicUsize::new(0)),
            dwell: Duration::from_millis(300),
            fault: Some(|| SourceError::Unreachable("simulated".into())),
        }),
        secrets: Arc::new(MemoryStore::new()),
        events: Arc::new(Silent),
    })
    .await
    .unwrap();

    let woken = scheduler
        .trigger(&id, SyncTrigger::FirstRun, None)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let wizard = Heard::new();
    let joined = scheduler
        .trigger(&id, SyncTrigger::FirstRun, Some(wizard.sink()))
        .await
        .unwrap();
    assert_eq!(joined, woken, "the wizard joined the run the wake started");
    assert_eq!(await_ending(&wizard).await.phase, SyncPhase::Failed);

    let retry = Heard::new();
    let retried = scheduler
        .trigger(&id, SyncTrigger::FirstRun, Some(retry.sink()))
        .await
        .unwrap();
    assert_eq!(await_ending(&retry).await.phase, SyncPhase::Failed);
    scheduler.shutdown().await;

    assert_ne!(
        retried, woken,
        "the wizard was enrolled in that run, so it is not owed it a second time"
    );
    retire(&pool, &ids).await;
}

/// **Adding a source produces exactly one run, whichever path reaches the
/// scheduler first.**
///
/// `add_source` wakes the scheduler so a brand-new source is not idle until its
/// first tick, and the wizard triggers a sync of its own because it draws the
/// progress bar: two paths for one intention. Both orders end with one run in
/// the log, spelled `first_run`, and the wizard hearing that run's ending.
///
/// The third interleaving -- the wake's run finishing before the wizard's
/// trigger arrives -- is
/// [`a_first_run_trigger_after_the_run_ended_is_served_the_ending_from_the_record`],
/// which is the one that used to produce a second run over an already-mirrored
/// corpus.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn adding_a_source_produces_exactly_one_run_on_every_interleaving() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed_quiet(&pool, 2).await;
    let (wizard_first, wake_first) = (ids[0].clone(), ids[1].clone());
    let (deps, _) = deps(sched_pool, Duration::from_millis(400)).await;
    let scheduler = Scheduler::start(deps).await.unwrap();

    // The wizard's trigger arrives first; the wake's finds the run it started.
    let a = Heard::new();
    let a_wizard = scheduler
        .trigger(&wizard_first, SyncTrigger::FirstRun, Some(a.sink()))
        .await
        .unwrap();
    let a_wake = scheduler
        .trigger(&wizard_first, SyncTrigger::FirstRun, None)
        .await
        .unwrap();

    // The wake's trigger arrives first and its run is still going when the
    // wizard arrives.
    let b_wake = scheduler
        .trigger(&wake_first, SyncTrigger::FirstRun, None)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let b = Heard::new();
    let b_wizard = scheduler
        .trigger(&wake_first, SyncTrigger::FirstRun, Some(b.sink()))
        .await
        .unwrap();

    assert_eq!(a_wizard, a_wake, "the wake joined the wizard's run");
    assert_eq!(b_wizard, b_wake, "the wizard joined the wake's run");
    assert_eq!(await_ending(&a).await.phase, SyncPhase::Finished);
    assert_eq!(await_ending(&b).await.phase, SyncPhase::Finished);
    scheduler.shutdown().await;

    for id in &ids {
        let rows = run_log::list(&pool, Some(id), 10).await.unwrap();
        assert_eq!(rows.len(), 1, "{id}: one addition, one run: {rows:?}");
        assert_eq!(
            rows[0].trigger,
            SyncTrigger::FirstRun,
            "{id}: a first sync is logged as one whoever started it"
        );
    }
    retire(&pool, &ids).await;
}

/// **A sink that misbehaves costs the run nothing and the sink behind it
/// nothing -- through a real run, end to end.**
///
/// This test was
/// `a_watcher_that_has_gone_away_does_not_fail_the_run`, and under that name it
/// pinned almost nothing: `ProgressSink::report` returns `()`, so there is no
/// route by which *any* sink could fail a run, and a sink that merely stays
/// silent is indistinguishable from one that worked. The property it really
/// had -- because the bad sink is enrolled **first** -- is fan-out completeness
/// past a sink that does not cooperate, so that is what it now says, and its
/// bad sink now **panics** rather than staying quiet. A panic is the one thing
/// a sink can do that genuinely reaches the run: before #118 it stranded every
/// sink enrolled after it, and from `Closing::drop` during an unwind it aborted
/// the process (`crates/knobas-sync/tests/progress.rs` pins both directly on
/// `Watchers`).
///
/// So there are two claims here, and a live sink enrolled *behind* the broken
/// one is what makes both falsifiable: the run still commits its work and logs
/// `ok`, and the second watcher still receives that run's ending.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_sink_that_panics_fails_neither_the_run_nor_the_sink_behind_it() {
    struct Boom;
    impl ProgressSink for Boom {
        fn report(&self, _progress: SyncProgress) {
            // A caller with a bug in its `report`. Out of contract -- and
            // "the caller broke its contract" is no reason for a *different*
            // caller to lose its ending, or for the process to abort.
            panic!("a sink that panics on purpose");
        }
    }

    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed_quiet(&pool, 1).await;
    let id = ids[0].clone();
    let (deps, _) = deps(sched_pool, Duration::from_millis(300)).await;
    let scheduler = Scheduler::start(deps).await.unwrap();

    let run = scheduler
        .trigger(
            &id,
            SyncTrigger::Manual,
            Some(Arc::new(Boom) as Arc<dyn ProgressSink>),
        )
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    let live = Heard::new();
    scheduler
        .trigger(&id, SyncTrigger::Manual, Some(live.sink()))
        .await
        .unwrap();

    let ending = await_ending(&live).await;
    scheduler.shutdown().await;
    assert_eq!(ending.phase, SyncPhase::Finished);
    assert_eq!(ending.run_id, run);

    let row = &run_log::list(&pool, Some(&id), 1).await.unwrap()[0];
    assert_eq!(
        row.outcome,
        Some(knobas_sync::run_log::SyncOutcome::Ok),
        "a broken listener is not a sync error"
    );
    assert!(row.upserted > 0, "and the run still did its work");
    retire(&pool, &ids).await;
}

/// **A run whose task dies still tells its watchers, and still releases its
/// source.**
///
/// The invariant has a second door: a caller can be handed a run id for a task
/// that then never reaches the line that reports an ending -- a panicking
/// adapter, or the abort `shutdown` falls back on when a run does not stop
/// inside the grace period. Before ADR-0005 that left the caller waiting for
/// ever *and* left the source claimed for the life of the process, so it could
/// never sync again. A guard closes the run's watchers however the task ends.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_run_whose_task_panics_still_gives_its_watchers_an_ending() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed_quiet(&pool, 1).await;
    let id = ids[0].clone();
    let connector = knobas_db::test_util::test_connector().await;
    let scheduler = Scheduler::start(SchedulerDeps {
        pool: sched_pool,
        connections: Arc::new(TestConnections(connector)),
        registry: Arc::new(SlowRegistry {
            inside: Arc::new(AtomicUsize::new(0)),
            peak: Arc::new(AtomicUsize::new(0)),
            dwell: Duration::from_millis(10),
            // Never returns: the adapter blows up where a bug in one would.
            fault: Some(|| panic!("the adapter blew up")),
        }),
        secrets: Arc::new(MemoryStore::new()),
        events: Arc::new(Silent),
    })
    .await
    .unwrap();

    let watcher = Heard::new();
    let run = scheduler
        .trigger(&id, SyncTrigger::Manual, Some(watcher.sink()))
        .await
        .unwrap();
    let ending = await_ending(&watcher).await;
    assert_eq!(ending.phase, SyncPhase::Failed);
    assert_eq!(ending.run_id, run);
    assert!(
        ending.message.is_some(),
        "a failed phase with nothing to say is not something a person can act on"
    );

    // And the source is free again: the next trigger gets a run of its own
    // rather than the id of a run that will never speak.
    let again = Heard::new();
    let second = scheduler
        .trigger(&id, SyncTrigger::Manual, Some(again.sink()))
        .await
        .unwrap();
    assert_ne!(second, run, "the dead run still owned the source");
    await_ending(&again).await;
    scheduler.shutdown().await;

    // Neither task reached `settle`, so both rows are still open --
    // `reconcile_abandoned` is what closes rows like these at the next start.
    // Closed here so the tests that assert the reconciler has nothing to do are
    // not reading this one's residue.
    sqlx::query(
        "update knobas.sync_run set finished_at = now(), outcome = 'error'
          where source_id = $1 and finished_at is null",
    )
    .bind(&id)
    .execute(&pool)
    .await
    .unwrap();
    retire(&pool, &ids).await;
}

/// **The synthesised ending is delivered like every other message: a sink that
/// panics on it does not unwind into the caller that triggered.**
///
/// The served-from-record branch of `Inner::trigger` is the one delivery that
/// does not go through [`Watchers`] -- it hands a lone caller its ending
/// directly, in that caller's own stack and under the scheduler's `runs` lock.
/// So it was also the one place #118's containment did not reach, and the
/// consequence is sharper than a lost message: the panic came out of `trigger`
/// itself, so `sync_now` returned an error to a frontend that had asked about a
/// run which had in fact finished perfectly well. That it is ADR-0005's own
/// path is what makes it worth pinning rather than leaving to the contract
/// `ProgressSink` already states.
///
/// The broken sink still hears nothing -- that is the trade `deliver` makes
/// everywhere -- but it is the only caller that pays for its own bug.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_panicking_sink_served_from_the_record_does_not_unwind_into_the_caller() {
    struct Boom;
    impl ProgressSink for Boom {
        fn report(&self, _progress: SyncProgress) {
            panic!("a sink that panics on purpose");
        }
    }

    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed_quiet(&pool, 1).await;
    let id = ids[0].clone();
    let (deps, _) = deps(sched_pool, Duration::from_millis(300)).await;
    let scheduler = Scheduler::start(deps).await.unwrap();

    // The same setup as
    // `a_first_run_trigger_after_the_run_ended_is_served_the_ending_from_the_record`,
    // and for the same reasons: no sink on the first trigger, so the `FirstRun`
    // claim is still unspent, and `Manual` to know the run is completely over.
    let scheduled = scheduler
        .trigger(&id, SyncTrigger::FirstRun, None)
        .await
        .unwrap();
    let observer = Heard::new();
    tokio::time::sleep(Duration::from_millis(50)).await;
    scheduler
        .trigger(&id, SyncTrigger::Manual, Some(observer.sink()))
        .await
        .unwrap();
    await_ending(&observer).await;

    // The wizard arrives after the end with a sink that blows up on the
    // message it is served. Without containment this `await` never returns a
    // value -- it unwinds this task, and the test dies here rather than
    // failing an assertion.
    let handed = scheduler
        .trigger(
            &id,
            SyncTrigger::FirstRun,
            Some(Arc::new(Boom) as Arc<dyn ProgressSink>),
        )
        .await
        .unwrap();
    assert_eq!(
        handed, scheduled,
        "the wizard is still handed the run that already happened"
    );

    // And the scheduler is still usable by everybody else, which is the part a
    // panic escaping under the `runs` lock would have put in doubt.
    let next = scheduler
        .trigger(&id, SyncTrigger::Manual, None)
        .await
        .unwrap();
    assert_ne!(next, scheduled, "the next trigger gets a run of its own");
    scheduler.shutdown().await;
    retire(&pool, &ids).await;
}

// -- #119: the entry's life ends with its source's -----------------------------

/// Add a source back under an id that has just been deleted -- the user action
/// #119 is about, and the one `sync_run`'s deliberate lack of a foreign key
/// makes possible: the deleted source's runs are still in the log, under the
/// same `source_id` the new source now has.
///
/// Disabled from the start, for the reason [`seed_quiet`] disables: no other
/// test's ticker may adopt it.
async fn re_add(pool: &PgPool, id: &str) {
    config::insert(
        pool,
        &InsertConfig {
            id: id.to_owned(),
            adapter_kind: "slow".into(),
            display_name: "Slow, the second".into(),
            base_url: String::new(),
            auth_kind: AuthKind::None,
            config: serde_json::json!({}),
            sync_interval_secs: 60,
            // **Enabled**, and held off the schedule by [`retire`] instead
            // (issue #202). The callers assert on the re-added source's
            // *mirror*, and since migration `0012` a disabled source's items
            // are not in `sync.live_item` at all -- so inserting it disabled
            // would make those assertions test visibility rather than the
            // purge they are about.
            enabled: true,
        },
    )
    .await
    .unwrap();
    retire(pool, std::slice::from_ref(&id.to_owned())).await;
}

/// **A source that is deleted and added again under the same id is not served
/// the deleted source's ending.**
///
/// `Inner::runs` holds an entry that deliberately outlives its run -- that is
/// the mechanism serving a late caller its ending (ADR-0005). Nothing used to
/// end that entry's life when the *source* went away, and `sync_run` rows
/// outlive a source by design (no foreign key: "deleting a source must not
/// rewrite its history"), so the first-run wizard for the *new* source could be
/// handed the *old* source's run and the ending recorded for it -- knobas'
/// first sentence about a brand-new source, describing a run that belongs to
/// something the user deleted. The same defect ADR-0005 exists to remove,
/// wearing a different hat.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_source_re_added_under_a_deleted_ones_id_is_not_served_the_deleted_ones_ending() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed_quiet(&pool, 1).await;
    let id = ids[0].clone();
    let (deps, _) = deps(sched_pool, Duration::from_millis(200)).await;
    let scheduler = Scheduler::start(deps).await.unwrap();

    // The first source's own first sync, run to completion. `Manual` with a
    // sink is how the test knows it is *completely* over rather than merely
    // logged -- `settle` writes `finished_at` before the run tells anybody --
    // and it asks for work rather than for the first sync, so it leaves the
    // `FirstRun` claim unspent, which is the state that makes the entry
    // serveable to a later wizard.
    let deleted_sources_run = scheduler
        .trigger(&id, SyncTrigger::FirstRun, None)
        .await
        .unwrap();
    let observer = Heard::new();
    tokio::time::sleep(Duration::from_millis(50)).await;
    scheduler
        .trigger(&id, SyncTrigger::Manual, Some(observer.sink()))
        .await
        .unwrap();
    await_ending(&observer).await;

    // What `delete_source` does, and what it now tells the scheduler.
    config::delete(&pool, &id, false).await.unwrap();
    scheduler.forget_source(&id, Purge::Keep).await;
    re_add(&pool, &id).await;

    // The new source's wizard.
    let wizard = Heard::new();
    let handed = scheduler
        .trigger(&id, SyncTrigger::FirstRun, Some(wizard.sink()))
        .await
        .unwrap();
    let ending = await_ending(&wizard).await;
    scheduler.shutdown().await;

    assert_ne!(
        handed, deleted_sources_run,
        "the wizard for the new source was handed the deleted source's run"
    );
    assert_eq!(
        ending.run_id, handed,
        "and the ending it heard is the new run's"
    );
    let rows = run_log::list(&pool, Some(&id), 10).await.unwrap();
    assert_eq!(
        rows.len(),
        2,
        "the deleted source's run stays in the log, and the new source has one of its own: {rows:?}"
    );
    retire(&pool, &ids).await;
}

// -- #127: the purge outlives the run that was in flight -----------------------

/// Rows in the mirror for one source, and rows of it a reader can actually
/// reach.
///
/// Two numbers and not one, because they answer different questions and the
/// defect #127 is about shows up in the second. `sync.item` is the mirror;
/// `sync.live_item` is the mirror minus what `knobas.entity` says is
/// tombstoned, and it is what every reader -- the launcher, the board,
/// `knobas-search`'s corpus -- is built on. An item can be gone from search
/// while its row stays, and an entity can be resurrected into search while
/// nobody deleted a row at all.
async fn mirrored(pool: &PgPool, id: &str) -> (i64, i64) {
    let (items,): (i64,) = sqlx::query_as("select count(*) from sync.item where source_id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap();
    let (live,): (i64,) =
        sqlx::query_as("select count(*) from sync.live_item where source_id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap();
    (items, live)
}

/// Wait until a run is *inside* the adapter -- past the one existence check a
/// run makes -- and fail loudly if none ever gets there.
///
/// The interleaving this test is about is only real from here on. Before it,
/// `run_locked`'s `select cursor from knobas.source_config` has not happened
/// yet, a delete would be seen, and the run would refuse and write nothing:
/// green, and about nothing.
async fn await_inside(inside: &Arc<AtomicUsize>) {
    for _ in 0..200 {
        if inside.load(Ordering::SeqCst) > 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("no run reached the adapter in two seconds; there is nothing to interleave with");
}

/// **Deleting a source with `purge_items` while a sync of it is in flight
/// leaves nothing of it in the mirror and nothing of it in search** (#127).
///
/// The run checks that its source exists once, at the top, before any network
/// traffic; everything after that is a window in which a delete commits
/// unnoticed and the run then commits over it. What comes back is not an orphan
/// row: `ENTITY_UPSERT` writes `deleted_at = excluded.deleted_at`, and a live
/// incoming item carries none, so the purge's tombstones are *cleared* and the
/// entities the user deleted return to `sync.live_item` -- for a source with no
/// configuration row, which nothing will ever sync or tombstone again. The
/// symptom is permanent searchable items, not a stray row, which is why this
/// asserts on both numbers.
///
/// The interleaving is arranged, not hoped for: the delete happens while a run
/// is demonstrably inside the adapter, and the run's own ending is asserted to
/// be a `Finished` that upserted its item. Without that second assertion a run
/// that refused at the top -- writing nothing, purging nothing -- would leave
/// an empty mirror and pass this test over a scheduler that does nothing at
/// all.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_purge_survives_the_run_that_was_in_flight_when_the_source_was_deleted() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed_quiet(&pool, 1).await;
    let id = ids[0].clone();
    let (deps, inside) = deps_watching_the_adapter(sched_pool, Duration::from_millis(400)).await;
    let scheduler = Scheduler::start(deps).await.unwrap();

    // A mirror to purge, first: one run to completion, so the source has an
    // item and a live entity before anybody deletes anything.
    let first = Heard::new();
    scheduler
        .trigger(&id, SyncTrigger::Manual, Some(first.sink()))
        .await
        .unwrap();
    await_ending(&first).await;
    assert_eq!(
        mirrored(&pool, &id).await,
        (1, 1),
        "the source is mirrored and searchable before the delete, or this test purges nothing"
    );

    // The second run, deleted out from under it mid-flight -- what
    // `delete_source(purge_items: true)` does, in the order it does it.
    let watcher = Heard::new();
    let in_flight = scheduler
        .trigger(&id, SyncTrigger::Manual, Some(watcher.sink()))
        .await
        .unwrap();
    await_inside(&inside).await;
    config::delete(&pool, &id, true).await.unwrap();
    assert!(
        inside.load(Ordering::SeqCst) > 0,
        "the delete has to commit while the run is still fetching. If the run \
         had already committed, the delete's own purge would tidy up after it \
         and this test would be green over a scheduler that purges nothing"
    );
    scheduler.forget_source(&id, Purge::Items).await;

    // ADR-0005 through the new path: the caller enrolled before the deletion is
    // still told how its run ended. Nothing here cancels anything.
    let ending = await_ending(&watcher).await;
    assert_eq!(ending.run_id, in_flight, "a run id came without its ending");
    assert_eq!(
        (ending.phase, ending.items),
        (SyncPhase::Finished, 1),
        "the deleted source's run has to have committed an item *after* the \
         purge, or there is no defect here to fix: {ending:?}"
    );

    scheduler.shutdown().await;

    assert_eq!(
        mirrored(&pool, &id).await,
        (0, 0),
        "the run that was in flight when the source was deleted wrote its \
         mirror back, and its entity upsert cleared the purge's tombstones: \
         the items the user deleted are searchable again (#127)"
    );
    // Tombstoned, never dropped: links, notes and activity rows point at these.
    let (tombstoned,): (bool,) =
        sqlx::query_as("select deleted_at is not null from knobas.entity where id = $1")
            .bind(format!("{id}:S-1"))
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        tombstoned,
        "the purged source's entity is tombstoned, not deleted"
    );
    retire(&pool, &ids).await;
}

/// **A source added back under the id wins: the purge armed for the source
/// that is gone does not fire over the new one's mirror** (#127).
///
/// The intent is in memory and keyed by id, so between arming it and applying
/// it the id can change hands -- `add_source` under a deleted source's id is a
/// real user action, and it is the one #119 exists for. A purge that still
/// fired then would delete the *new* source's first sync, which is worse than
/// the defect it is there to fix and would arrive with no error anywhere.
///
/// What this deliberately does **not** assert away is the residual: the old
/// run's late commit merges into the re-added source's mirror. That is the
/// accepted cost of letting the newest instruction about an id win, and it is
/// documented where the intent is cleared.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn adding_a_source_back_under_the_id_voids_the_purge_armed_for_the_old_one() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed_quiet(&pool, 1).await;
    let id = ids[0].clone();
    let (deps, inside) = deps_watching_the_adapter(sched_pool, Duration::from_millis(400)).await;
    let scheduler = Scheduler::start(deps).await.unwrap();

    let watcher = Heard::new();
    scheduler
        .trigger(&id, SyncTrigger::Manual, Some(watcher.sink()))
        .await
        .unwrap();
    await_inside(&inside).await;

    // Delete with the purge, then think better of it -- all while the run is
    // still fetching.
    config::delete(&pool, &id, true).await.unwrap();
    assert!(
        inside.load(Ordering::SeqCst) > 0,
        "the delete has to commit while the run is still fetching. If the run \
         had already committed, the delete's own purge would tidy up after it \
         and this test would be green over a scheduler that purges nothing"
    );
    scheduler.forget_source(&id, Purge::Items).await;
    re_add(&pool, &id).await;
    scheduler.source_added(&id).await;

    let ending = await_ending(&watcher).await;
    assert_eq!(
        (ending.phase, ending.items),
        (SyncPhase::Finished, 1),
        "the run has to have committed after the delete, or the purge it \
         arms has nothing to fire over: {ending:?}"
    );
    scheduler.shutdown().await;

    assert_eq!(
        mirrored(&pool, &id).await,
        (1, 1),
        "the purge was armed for the source the user deleted; firing it after \
         they added one back under the same id takes the new source's mirror"
    );
    retire(&pool, &ids).await;
}

// -- #154: an add_source that commits inside delete_source's own body ---------

/// The cursor stored for a source, or `None`.
///
/// Read *with* [`mirrored`] and never instead of it, because the state #154 is
/// about is the pair. `PURGE_ITEMS` takes `sync.item` rows and tombstones
/// entities; it never touches `knobas.source_config`, where the cursor lives.
/// So a purge that fires over a source the user has just added leaves that
/// source's cursor advanced past a corpus that is gone: the next incremental
/// run asks for what changed since, is told nothing did, and the hole stays
/// until somebody orders a backfill by hand. That is what makes this window
/// worse than the #127 defect the machinery was built to close -- #127 left
/// *extra* rows, this leaves an invisible absence.
async fn cursor_of(pool: &PgPool, id: &str) -> Option<String> {
    let (cursor,): (Option<String>,) =
        sqlx::query_as("select cursor from knobas.source_config where id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap();
    cursor
}

/// **A source added back under the id before the forget even happens is not
/// purged either** (#154) -- the arm-it branch.
///
/// The door `source_added` closes is delete → forget → add. This is the same
/// race through the opposite door, and it fits *inside `delete_source`'s own
/// body*: the delete purges and commits, deletes the keychain item, and only
/// then calls `forget_source`. An `add_source` for the same id that commits in
/// that window has already run `source_added` by the time the forget arrives,
/// so there is nothing left to clear the intent the forget is about to arm --
/// and the run still in flight applies it when it settles, over a source the
/// user has just created.
///
/// Same interleaving as
/// [`adding_a_source_back_under_the_id_voids_the_purge_armed_for_the_old_one`]
/// with the two calls in the order `delete_source` actually produces, which is
/// the whole of the defect: `source_added` before `forget_source`, not after.
///
/// The interleaving is arranged rather than hoped for. The re-add is a real
/// commit landing while a run is demonstrably inside the adapter
/// ([`await_inside`]), and the run's ending is asserted to be a `Finished` that
/// upserted its item -- without that, a run that refused at the top would leave
/// an empty mirror and pass this over a scheduler that does nothing at all.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_source_added_inside_the_delete_does_not_have_the_forgets_purge_armed_over_it() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed_quiet(&pool, 1).await;
    let id = ids[0].clone();
    let (deps, inside) = deps_watching_the_adapter(sched_pool, Duration::from_millis(400)).await;
    let scheduler = Scheduler::start(deps).await.unwrap();

    let watcher = Heard::new();
    scheduler
        .trigger(&id, SyncTrigger::Manual, Some(watcher.sink()))
        .await
        .unwrap();
    await_inside(&inside).await;

    // `delete_source`, one statement at a time -- and the add landing between
    // its second step and its third.
    config::delete(&pool, &id, true).await.unwrap();
    assert!(
        inside.load(Ordering::SeqCst) > 0,
        "the delete has to commit while the run is still fetching, or there is \
         no in-flight run for the forget to arm a purge against"
    );
    re_add(&pool, &id).await;
    scheduler.source_added(&id).await;
    scheduler.forget_source(&id, Purge::Items).await;

    let ending = await_ending(&watcher).await;
    assert_eq!(
        (ending.phase, ending.items),
        (SyncPhase::Finished, 1),
        "the run has to have committed after the delete, or the purge it \
         arms has nothing to fire over: {ending:?}"
    );
    scheduler.shutdown().await;

    assert_eq!(
        mirrored(&pool, &id).await,
        (1, 1),
        "a source exists under this id again; the purge armed for the one the \
         user deleted took the mirror out from under it (#154)"
    );
    assert!(
        cursor_of(&pool, &id).await.is_some(),
        "the run advanced the cursor of whatever row holds the id, which is \
         the new source's -- the assertion above is what stops that cursor \
         standing over an emptied mirror"
    );
    retire(&pool, &ids).await;
}

/// **The same window on the run-already-over branch** (#154): a forget that
/// applies its purge on the spot does not apply it to a source that exists.
///
/// The other half of `forget_source`'s decision. When the last run of the id is
/// over, the forget purges immediately rather than arming -- #127's gap branch,
/// which is right for a deleted source and wrong for the one the user added
/// back a moment ago. Here the new source has a mirror of its very own: it was
/// added, and it synced, before the forget for the *old* source arrived.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_source_added_inside_the_delete_is_not_purged_by_the_forget_itself() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed_quiet(&pool, 1).await;
    let id = ids[0].clone();
    let (deps, _) = deps(sched_pool, Duration::from_millis(10)).await;
    let scheduler = Scheduler::start(deps).await.unwrap();

    // The deleted source's own run, over and settled, so the forget below takes
    // the branch that purges on the spot.
    let first = Heard::new();
    let deleted_sources_run = scheduler
        .trigger(&id, SyncTrigger::Manual, Some(first.sink()))
        .await
        .unwrap();
    await_ending(&first).await;

    config::delete(&pool, &id, true).await.unwrap();
    assert_eq!(
        mirrored(&pool, &id).await,
        (0, 0),
        "the delete's own purge is what leaves the mirror empty for the new \
         source to fill"
    );

    // Inside `delete_source`'s body: a source added back under the id, and its
    // first sync, both before the forget arrives.
    re_add(&pool, &id).await;
    scheduler.source_added(&id).await;
    let newcomer = Heard::new();
    // `Manual`, not `FirstRun`: the scheduler still holds the deleted source's
    // closed entry -- nothing has forgotten it yet -- and a `FirstRun` trigger
    // would be served *that* run's recorded ending (ADR-0005) instead of
    // syncing, which is #119's defect and not this one. Asking for work gets
    // work.
    let newcomers_run = scheduler
        .trigger(&id, SyncTrigger::Manual, Some(newcomer.sink()))
        .await
        .unwrap();
    let ending = await_ending(&newcomer).await;
    assert_ne!(
        newcomers_run, deleted_sources_run,
        "the new source has to have run at all, or this test purges nothing"
    );
    assert_eq!(
        (ending.phase, ending.items),
        (SyncPhase::Finished, 1),
        "the new source has to have mirrored something, or this test purges \
         nothing: {ending:?}"
    );
    assert_eq!(
        mirrored(&pool, &id).await,
        (1, 1),
        "and its items have to be in the mirror before the forget arrives"
    );

    scheduler.forget_source(&id, Purge::Items).await;
    scheduler.shutdown().await;

    assert_eq!(
        mirrored(&pool, &id).await,
        (1, 1),
        "the forget belonged to the source the user deleted; applying its \
         purge over the source they added back takes that source's first \
         sync (#154)"
    );
    retire(&pool, &ids).await;
}

/// **The window does not close when the run does** (#127): a run whose commit
/// landed between `delete_source`'s purge and the forget is purged by the forget
/// itself.
///
/// `delete_source` purges, deletes the keychain item, and only then tells the
/// scheduler. A run finishing inside that gap has committed its mirror back
/// with nothing left in flight for a later purge to ride on, so the arm-it
/// branch never fires and the rows would stay for ever. The forget applies the
/// purge itself for exactly that case, which is what makes the guarantee
/// *closed* rather than merely narrower than it was.
///
/// The late commit is written by hand here rather than raced for: it is one
/// statement wide and the point is what the forget does with what it finds, not
/// whether this machine can be made to lose that race on cue.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_run_that_committed_in_the_gap_before_the_forget_is_purged_by_it() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed_quiet(&pool, 1).await;
    let id = ids[0].clone();
    let (deps, _) = deps(sched_pool, Duration::from_millis(10)).await;
    let scheduler = Scheduler::start(deps).await.unwrap();

    // A run, over and settled, so the scheduler holds a closed entry for the
    // id -- the state `delete_source` finds for any source that has ever
    // synced in this process.
    let first = Heard::new();
    scheduler
        .trigger(&id, SyncTrigger::Manual, Some(first.sink()))
        .await
        .unwrap();
    await_ending(&first).await;

    config::delete(&pool, &id, true).await.unwrap();
    assert_eq!(
        mirrored(&pool, &id).await,
        (0, 0),
        "the delete's own purge is what the late commit below undoes"
    );

    // The late commit, in the gap: the mirror back, and -- the part that makes
    // this permanent -- `deleted_at` cleared, so the entity is live in
    // `sync.live_item` again.
    let entity = format!("{id}:S-1");
    sqlx::query("update knobas.entity set deleted_at = null where id = $1")
        .bind(&entity)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, payload)
         values ($1, $2, 'ticket', 'slow', '{}'::jsonb)",
    )
    .bind(&entity)
    .bind(&id)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        mirrored(&pool, &id).await,
        (1, 1),
        "the resurrection this test is about did not happen"
    );

    scheduler.forget_source(&id, Purge::Items).await;
    scheduler.shutdown().await;

    assert_eq!(
        mirrored(&pool, &id).await,
        (0, 0),
        "a commit that landed after the delete's purge and before the forget \
         is nobody else's to clean up"
    );
    retire(&pool, &ids).await;
}

/// **"Remove source, keep items" survives the in-flight run too** (#127).
///
/// The other half of the intent, and the half a foreign key could never have
/// had: keeping the items is a real choice in the sources view (interfaces §3,
/// Delete), so the purge must be something `delete_source` asks for rather than
/// something the scheduler does whenever a source goes away. A forget that
/// purged regardless would delete a mirror the user explicitly kept, and it
/// would do it only sometimes -- when a sync happened to be running.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn deleting_a_source_mid_run_without_purging_keeps_its_items() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed_quiet(&pool, 1).await;
    let id = ids[0].clone();
    let (deps, inside) = deps_watching_the_adapter(sched_pool, Duration::from_millis(400)).await;
    let scheduler = Scheduler::start(deps).await.unwrap();

    let watcher = Heard::new();
    scheduler
        .trigger(&id, SyncTrigger::Manual, Some(watcher.sink()))
        .await
        .unwrap();
    await_inside(&inside).await;
    config::delete(&pool, &id, false).await.unwrap();
    assert!(
        inside.load(Ordering::SeqCst) > 0,
        "the delete has to commit while the run is still fetching. If the run \
         had already committed, the delete's own purge would tidy up after it \
         and this test would be green over a scheduler that purges nothing"
    );
    scheduler.forget_source(&id, Purge::Keep).await;

    let ending = await_ending(&watcher).await;
    assert_eq!((ending.phase, ending.items), (SyncPhase::Finished, 1));
    scheduler.shutdown().await;

    assert_eq!(
        mirrored(&pool, &id).await,
        (1, 1),
        "the user asked to keep the items; a run being in flight is not a \
         reason to throw them away"
    );
    retire(&pool, &ids).await;
}

/// **Forgetting a source does not take an ending away from anybody already
/// watching its run.**
///
/// The other half of #119, and the half that keeps ADR-0005 intact through the
/// new operation. A source can be deleted while a run of it is still going, so
/// forgetting has to be safe *mid-flight*: the run holds its own handle on its
/// watchers, so a caller enrolled before the deletion is still told how that
/// run ended. What forgetting removes is only the scheduler's claim on the
/// *source id* -- so the next trigger, which belongs to whatever was added
/// under that id afterwards, gets a run of its own rather than joining a run
/// that is not about it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn forgetting_a_source_mid_run_still_gives_that_runs_watchers_their_ending() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed_quiet(&pool, 1).await;
    let id = ids[0].clone();
    let (deps, _) = deps(sched_pool, Duration::from_millis(400)).await;
    let scheduler = Scheduler::start(deps).await.unwrap();

    let watcher = Heard::new();
    let in_flight = scheduler
        .trigger(&id, SyncTrigger::FirstRun, Some(watcher.sink()))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Deleted mid-run, and added again straight away -- the impatient version
    // of the same user action.
    config::delete(&pool, &id, false).await.unwrap();
    scheduler.forget_source(&id, Purge::Keep).await;
    re_add(&pool, &id).await;

    let wizard = Heard::new();
    let handed = scheduler
        .trigger(&id, SyncTrigger::FirstRun, Some(wizard.sink()))
        .await
        .unwrap();

    // ADR-0005 for the caller that was already there: the run it was handed is
    // one it can observe, deletion or no deletion.
    let ending = await_ending(&watcher).await;
    assert_eq!(ending.run_id, in_flight);
    assert_ne!(
        handed, in_flight,
        "the new source's wizard joined a run belonging to the source that was deleted"
    );
    assert_eq!(await_ending(&wizard).await.run_id, handed);
    scheduler.shutdown().await;
    retire(&pool, &ids).await;
}
