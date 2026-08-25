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
use knobas_sync::run_log::{self, SyncTrigger};
use knobas_sync::scheduler::{
    AdapterRegistry, RunConnections, SYNC_CONCURRENCY, Scheduler, SchedulerDeps, SourceSyncStatus,
    SyncEvents,
};
use sqlx::PgPool;

/// An adapter that sleeps in `sync`, and records the high-water mark of how
/// many of its instances were inside `sync` at once.
struct Slow {
    id: String,
    inside: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    dwell: Duration,
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
            }],
            full_sync_exhaustive: true,
            config_schema: serde_json::json!({ "type": "object", "properties": {} }),
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
        Err(SourceError::Protocol("read-only".into()))
    }
}

struct SlowRegistry {
    inside: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    dwell: Duration,
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

/// Disable every source this test made, so the next test's ticker does not
/// adopt them.
async fn retire(pool: &PgPool, ids: &[String]) {
    for id in ids {
        sqlx::query("update knobas.source_config set enabled = false where id = $1")
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
    }
}

async fn deps(pool: PgPool, dwell: Duration) -> (SchedulerDeps, Arc<AtomicUsize>) {
    let connector = knobas_db::test_util::test_connector().await;
    let peak = Arc::new(AtomicUsize::new(0));
    (
        SchedulerDeps {
            pool,
            connections: Arc::new(TestConnections(connector)),
            registry: Arc::new(SlowRegistry {
                inside: Arc::new(AtomicUsize::new(0)),
                peak: Arc::clone(&peak),
                dwell,
            }),
            secrets: Arc::new(MemoryStore::new()),
            events: Arc::new(Silent),
        },
        peak,
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
