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
    SchedulerTiming, SourceSyncStatus, SyncEvents,
};
use sqlx::PgPool;

/// Where a held adapter waits until the test lets it through (#1920).
///
/// This replaces a dwell that stood in for "the run is still going when the
/// test acts". A dwell says that only while the machine is fast enough; on
/// a loaded gate the run ended first, and the test went red on correct code.
/// A door says it until the test opens the door, however slow the machine
/// is.
///
/// A semaphore with no permits. [`admit`](Door::admit) lets a counted number
/// of waits through, and [`open`](Door::open) closes the semaphore, which makes
/// every wait return at once, the ones already parked and the ones to come.
struct Door(tokio::sync::Semaphore);

impl Door {
    fn shut() -> Arc<Door> {
        Arc::new(Door(tokio::sync::Semaphore::new(0)))
    }

    /// Let the next `waits` through, and keep the door shut behind them.
    fn admit(&self, waits: usize) {
        self.0.add_permits(waits);
    }

    /// Let everything through from now on.
    fn open(&self) {
        self.0.close();
    }

    async fn pass(&self) {
        // `Err` is the closed semaphore: the door is open.
        if let Ok(permit) = self.0.acquire().await {
            permit.forget();
        }
    }
}

/// An adapter that waits in `sync`, at its door if it has one and then for
/// its dwell, and records the high-water mark of how many of its instances
/// were inside `sync` at once.
struct Slow {
    id: String,
    inside: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    /// Passed after `inside` counts this run, so a test that sees the count
    /// knows the run is parked here.
    door: Option<Arc<Door>>,
    dwell: Duration,
    /// A function, not a value: `SourceError` is not `Clone` (frozen SPI).
    fault: Option<fn() -> SourceError>,
}

#[async_trait::async_trait]
impl Source for Slow {
    /// Draws nothing: this fake has no source view (#661).
    async fn view(
        &self,
        _entity: &str,
        _section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, knobas_source::SourceError> {
        Ok(None)
    }

    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_kind: "slow".into(),
            name: "Slow".into(),
            capabilities: Vec::new(),
            adapter_version: "0.1.0".into(),
            auth_methods: Vec::new(),
            accepts_account: false,
            min_sync_interval_secs: 60,
            roles: Vec::new(),
            source_actions: Vec::new(),
            clone_from: Vec::new(),
            url_forms: Vec::new(),
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
        if let Some(door) = &self.door {
            door.pass().await;
        }
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
    async fn write(
        &self,
        _op: knobas_source::WriteOp,
    ) -> Result<knobas_source::WriteReceipt, SourceError> {
        Err(SourceError::protocol("read-only"))
    }

    /// No workflow: this fake declares no `transition` write op, which is the
    /// contract battery's rule for when the read is refused (#498).
    async fn reachable_transitions(
        &self,
        entity: &str,
    ) -> Result<Vec<String>, knobas_source::SourceError> {
        Err(knobas_source::SourceError::protocol(format!(
            "this fake has no workflow, so there are no reachable transitions for {entity:?}"
        )))
    }
}

struct SlowRegistry {
    inside: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    door: Option<Arc<Door>>,
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
            door: self.door.clone(),
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

/// The clock every scheduler in this binary ticks on (#412).
///
/// Milliseconds where production has seconds, so a test that waits for the
/// ticker waits tens of milliseconds rather than the 2 s startup delay and 5 s
/// tick a user gets. Real time, not tokio's paused clock: with every task
/// blocked on database I/O tokio auto-advances a paused clock, which fires
/// pool-acquire timeouts these tests are not about. Two ticks of this are the
/// window [`the_ticker_runs_a_due_source_by_itself`] holds its run open for
/// (100 ms), at a door rather than for a dwell (#1920).
///
/// Tests that still seed *on* the schedule live with the ticker joining their
/// runs 50 ms in: each of them asserts something a join does not change (a
/// peak, a row count, one id handed to two callers, a cancelled row). A test
/// that asserts *who started* the run, or what its own sink heard first,
/// seeds with [`seed_quiet`] instead.
const FAST: SchedulerTiming = SchedulerTiming {
    startup_delay: Duration::from_millis(50),
    tick: Duration::from_millis(50),
    shutdown_grace: knobas_sync::scheduler::SHUTDOWN_GRACE,
};

/// A quit's grace far past [`HANG_GUARD`]: a run or a write the quit waited
/// out to the grace's end holds it for an hour, so a quit that returns inside
/// the guard waited out nothing (#1920).
const PATIENT_GRACE: Duration = Duration::from_secs(3600);

/// How long a test waits for something that, on a correct scheduler, happens
/// in milliseconds: a deadline against a hang, not a measurement.
const HANG_GUARD: Duration = Duration::from_secs(30);

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

/// Every `sync:state` a scheduler emitted, in order; the other events are
/// dropped.
#[derive(Default)]
struct Recorder(std::sync::Mutex<Vec<SourceSyncStatus>>);

impl SyncEvents for Recorder {
    fn sync_state(&self, s: SourceSyncStatus) {
        self.0.lock().unwrap().push(s);
    }
    fn source_health(&self, _h: knobas_sync::config::CredentialHealth) {}
    fn activity_new(&self, _r: knobas_core::activity::ActivityRow) {}
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
/// enabled, mirrored, searchable. The two tests that assert on `backoff_until`
/// read it before they retire their source or after their own run has set it,
/// which is what keeps it free to borrow.
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
    let (deps, peak, _inside) = slow_deps(pool, dwell, None).await;
    (deps, peak)
}

/// What a test holds of [`held_deps`]' adapter.
struct Held {
    /// Every run waits here, inside `sync`, until the test lets it through.
    door: Arc<Door>,
    /// How many runs are in the adapter's `sync` right now.
    ///
    /// What a test needs it for is an order it cannot otherwise see. `trigger`
    /// returns as soon as the run's log row exists, but the run has not yet
    /// read `knobas.source_config` -- and that read is the one existence check
    /// a run makes, so a test that deletes the source before it happens gets a
    /// run that refuses and writes nothing, which is not the interleaving it
    /// meant to arrange. `Slow` bumps this counter on entry to `sync`, which is
    /// *after* that read, so waiting for it puts the test in the window it is
    /// about, and the door keeps the window open until the test is done in it.
    inside: Arc<AtomicUsize>,
    /// The most runs that were in the adapter at once.
    peak: Arc<AtomicUsize>,
}

/// Deps whose every run parks at a shut door inside the adapter: the run is
/// demonstrably still going for as long as the test needs it to be (#1920).
///
/// The purge tests below hold their run there until the delete, the forget
/// and any re-add are done. A 400 ms dwell used to hold that window, and on a
/// loaded gate the run committed before the delete did.
async fn held_deps(pool: PgPool) -> (SchedulerDeps, Held) {
    let door = Door::shut();
    let (deps, peak, inside) = slow_deps(pool, Duration::ZERO, Some(Arc::clone(&door))).await;
    (deps, Held { door, inside, peak })
}

async fn slow_deps(
    pool: PgPool,
    dwell: Duration,
    door: Option<Arc<Door>>,
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
                door,
                fault: None,
            }),
            secrets: Arc::new(MemoryStore::new()),
            events: Arc::new(Silent),
            timing: FAST,
            quit: knobas_sync::scheduler::Quit::default(),
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
    let (deps, held) = held_deps(sched_pool).await;

    let scheduler = Scheduler::start(deps).await.unwrap();
    for id in &ids {
        scheduler
            .trigger(id, SyncTrigger::Manual, None)
            .await
            .unwrap();
    }
    // The cap's worth of runs parked at the door (#1920). Before, a 600 ms
    // dwell had to outlast eight triggers, and on a loaded machine the first
    // runs finished before the last began, so the peak fell short of a cap
    // that was holding.
    await_inside_count(&held.inside, SYNC_CONCURRENCY).await;
    // The other three found the cap full and wait for a permit (#1927): from
    // here none of them can reach the adapter before the door opens, so the
    // peak read now is the peak of the whole wave. Before, a 300 ms margin
    // stood for this, and a run past the cap that arrived later than the
    // look went unseen.
    await_waiting_for_a_permit(&scheduler, 3).await;
    let peak = held.peak.load(Ordering::SeqCst);
    held.door.open();
    // Every triggered run has finished: a condition, not a span (#307).
    for id in &ids {
        await_source_idle(&pool, id).await;
    }
    scheduler.shutdown().await;

    // **Equality, not `<=`.** Three sources more than the cap are triggered at
    // once and none can leave the adapter, so the semaphore is what decides
    // the peak and the peak is what it decides. A `<=` here would read the very constant
    // it is checking: raising `SYNC_CONCURRENCY` to 30 raises the bound *and*
    // the peak, and the test stays green while the cap is gone. (Observed --
    // that mutation survived the first draft of this assertion.)
    assert_eq!(
        peak,
        SYNC_CONCURRENCY,
        "{} sources triggered at once must saturate the cap of {SYNC_CONCURRENCY} \
         and never exceed it",
        ids.len()
    );
    assert_eq!(
        held.peak.load(Ordering::SeqCst),
        SYNC_CONCURRENCY,
        "and the three let in after the door opened never ran beside five"
    );
    for id in &ids {
        let row = &run_log::list(&pool, Some(id), 1).await.unwrap()[0];
        assert!(row.finished_at.is_some(), "{id} never finished");
    }
    retire(&pool, &ids).await;
}

/// Six due sources whose runs block: five run, and the sixth starts only when
/// one of them finishes (#1189). The literal five, not `SYNC_CONCURRENCY`, so
/// a change of the constant has to change this too.
///
/// Also the restated pool invariant: the five held runs take no connection from
/// the scheduler's bookkeeping pool, so all of it is still free to acquire.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn six_blocked_runs_five_go_and_the_sixth_waits() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed(&pool, 6).await;
    let bookkeeping = sched_pool.clone();
    let (deps, parked) = held_deps(sched_pool).await;
    let inside = &parked.inside;
    assert_eq!(SYNC_CONCURRENCY, 5);

    let scheduler = Scheduler::start(deps).await.unwrap();
    for id in &ids {
        scheduler
            .trigger(id, SyncTrigger::Manual, None)
            .await
            .unwrap();
    }
    // Blocked at the door until the end of the test (#1920), not for a 1.5 s
    // dwell that had to outlast the waits below on a loaded machine.
    await_inside_count(inside, 5).await;
    // The sixth found the cap full and waits for a permit (#1927), so it
    // cannot reach the adapter while the five are held: the count below is
    // final until the door opens, not a look taken before a late sixth.
    await_waiting_for_a_permit(&scheduler, 1).await;
    assert_eq!(
        inside.load(Ordering::SeqCst),
        5,
        "the sixth waits while five are blocked"
    );

    let mut held = Vec::new();
    for _ in 0..knobas_sync::scheduler::SYNC_POOL_SIZE {
        held.push(
            tokio::time::timeout(Duration::from_secs(10), bookkeeping.acquire())
                .await
                .expect("the runs hold no bookkeeping connection")
                .unwrap(),
        );
    }
    drop(held);
    parked.door.open();

    for id in &ids {
        await_source_idle(&pool, id).await;
    }
    scheduler.shutdown().await;
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
    let (deps, held) = held_deps(sched_pool).await;

    let scheduler = Scheduler::start(deps).await.unwrap();
    let first = scheduler
        .trigger(&id, SyncTrigger::Manual, None)
        .await
        .unwrap();
    // Still going, held at the door (#1920), rather than 100 ms into a 600 ms
    // dwell that a loaded machine could see out before the second press.
    await_inside(&held.inside).await;
    let second = scheduler
        .trigger(&id, SyncTrigger::Manual, None)
        .await
        .unwrap();
    held.door.open();
    assert_eq!(
        first, second,
        "the caller is handed the run that is already going"
    );

    await_source_idle(&pool, &id).await;
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
    // The run the first tick starts is held at the door, so it is still open
    // when the later ticks look. `config::due` lists a source with an open
    // run -- it only knows about finished ones -- so each later tick sees the
    // same source due again, and the only thing between it and a second row
    // is the scheduler's own "already has an open run" guard. A run that
    // finished first would leave the source not due, and the guard never
    // asked: a 400 ms dwell used to stand in for the door, and on a loaded
    // machine it ran out first (#1920).
    let (deps, held) = held_deps(sched_pool).await;

    let scheduler = Scheduler::start(deps).await.unwrap();
    // A never-run source is due immediately: the first tick's run.
    await_inside(&held.inside).await;
    // Two more ticks while it is held. A **window, not a wait**: "the ticker
    // looked twice" is not a condition anything outside the scheduler can
    // observe, so the spec keeps it as two injected ticks (#412). On a loaded
    // machine the ticker may look fewer times inside it, which weakens the
    // test and cannot turn it red; the guard mutant fails with a row per look.
    tokio::time::sleep(2 * FAST.tick).await;
    held.door.open();
    // Then the run finishing is a condition (#307), and it has to have
    // finished for the interval to run from anywhere.
    await_source_idle(&pool, &id).await;
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
    // Held at the door for good: the run never finishes on its own, so only
    // the cancellation can end it.
    let (mut deps, held) = held_deps(sched_pool).await;
    // A grace far past the wait on `shutdown` below (#1920). A run stopped by
    // the abort at the grace's end would hold the quit for an hour, so a quit
    // that returns at all is one whose run **noticed** the cancellation at
    // its next await point. The old witness was a 2 s bound against the 3 s
    // grace, which a loaded gate overran on correct code.
    deps.timing.shutdown_grace = PATIENT_GRACE;
    // A stored `unreachable` the run will not get to re-witness, and the rung
    // the ladder stands on before it: neither is the cancelled run's to move
    // (#1629, spec #1618 ruling A8).
    config::set_health(
        &pool,
        &id,
        config::AuthState::Unreachable,
        Some("connection refused"),
        None,
    )
    .await
    .unwrap();
    let rung_before = run_log::failures_since_last_ok(&pool, &id).await.unwrap();

    let scheduler = Scheduler::start(deps).await.unwrap();
    scheduler
        .trigger(&id, SyncTrigger::Manual, None)
        .await
        .unwrap();
    // The run is inside its transaction, holding the connection the stall was
    // about -- a condition rather than a 200 ms guess (#307).
    await_inside(&held.inside).await;

    // The grace above is an hour, so a run that is waited out rather than
    // stopped fails here.
    tokio::time::timeout(HANG_GUARD, scheduler.shutdown())
        .await
        .expect(
            "the quit waited out its grace: a cancelled run stops at its next \
             await, it is not waited out",
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

    // The quit was the reader's act, not the source's fault (#1629, spec
    // #1618 ruling A8): the source is not backed off for it, the ladder's rung
    // does not move, and the health the run never re-witnessed stays. A large
    // first sync quit twice would otherwise be held off 1 and then 2 minutes.
    let source = config::get(&pool, &id).await.unwrap().unwrap();
    assert_eq!(
        source.backoff_until, None,
        "a cancelled run backed the source off"
    );
    assert_eq!(
        run_log::failures_since_last_ok(&pool, &id).await.unwrap(),
        rung_before,
        "a cancelled run counted as a failure"
    );
    assert_eq!(
        source.health.state,
        config::AuthState::Unreachable,
        "a cancelled run says nothing about the far end"
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
    let (deps, held) = held_deps(sched_pool).await;
    let inside = &held.inside;

    let scheduler = Scheduler::start(deps).await.unwrap();
    scheduler
        .trigger(&id, SyncTrigger::Manual, None)
        .await
        .unwrap();
    // Both waits here are conditions, not clock (#307). A fixed sleep before
    // the shutdown decided by guess whether the run had reached
    // `run_locked`'s `pg_advisory_xact_lock` yet, and on a loaded machine it
    // sometimes had not -- a run cancelled *before* it took the lock leaves
    // the lock free for a reason this test is not about, so the sleep was
    // buying a green that says nothing. `await_inside` puts the run past that
    // statement and inside the adapter, where it demonstrably holds the lock.
    await_inside(inside).await;
    let mut asking = probe(&pool).await;
    assert!(
        !lock_is_free(&mut asking, &id).await,
        "the run must be holding the source's lock when the cancellation \
         arrives, or what follows is about nothing"
    );
    drop(asking);

    scheduler.shutdown().await;

    // And the release is waited for rather than assumed. `shutdown` returns
    // when the run's *task* is done; the lock goes when the server notices the
    // dropped connection, which is a beat later and not one this process can
    // observe. Polling to a deadline covers that beat without paying for it on
    // a quiet machine -- and a lock that is never released still fails, five
    // seconds later, which is what the mutation proof turns on.
    assert!(
        await_lock_free(&pool, &id).await,
        "the cancelled run's connection is still holding the source's lock"
    );
    retire(&pool, &ids).await;
}

/// One pooled connection to ask [`lock_is_free`] down, bounded.
///
/// The wait is on `acquire`, not on the question: `pg_try_advisory_lock`
/// cannot block, but drawing the connection to ask it on can, and a test that
/// hangs on a saturated pool reports nothing at all. The old single-shot form
/// of this assertion had a timeout around the whole query for that reason;
/// this is where it went.
async fn probe(pool: &PgPool) -> sqlx::pool::PoolConnection<sqlx::Postgres> {
    tokio::time::timeout(Duration::from_secs(30), pool.acquire())
        .await
        .expect("a connection to ask about the lock on")
        .unwrap()
}

/// Is `source_id`'s advisory lock free **right now**?
///
/// `pg_try_advisory_lock` is the *non-blocking* form: it answers at once
/// rather than queueing behind whoever holds it, so this cannot hang and a
/// caller can ask it in either direction -- "nobody holds it yet" is as much
/// an answer as "somebody still does".
///
/// Takes the connection rather than the pool because the lock it may acquire
/// is a **session** lock: it belongs to the session that took it, so the
/// unlock has to go down the same connection. Issued on whatever the pool
/// handed out next, the unlock would release nothing and leave the test's own
/// lock sitting on a pooled connection for whoever draws it after.
async fn lock_is_free(conn: &mut sqlx::PgConnection, source_id: &str) -> bool {
    let (free,): (bool,) = sqlx::query_as("select pg_try_advisory_lock(hashtext($1::text))")
        .bind(source_id)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    if free {
        sqlx::query("select pg_advisory_unlock(hashtext($1::text))")
            .bind(source_id)
            .execute(&mut *conn)
            .await
            .unwrap();
    }
    free
}

/// Wait, to a five-second deadline, for `source_id`'s advisory lock to come
/// free -- one connection for the whole poll, for the reason
/// [`lock_is_free`] gives.
///
/// Returns rather than panicking like its siblings above, and that is the
/// difference between a harness that could not get going and a claim that is
/// false: a lock still held when the deadline runs out is the *subject* of
/// the test that calls this, so the message belongs at that call site.
async fn await_lock_free(pool: &PgPool, source_id: &str) -> bool {
    let mut conn = probe(pool).await;
    for _ in 0..250 {
        if lock_is_free(&mut conn, source_id).await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
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
    for run_id in &started {
        await_finish(&pool, *run_id).await;
    }
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
    // Off the schedule: the `running` this reads must be *this* trigger's. A
    // ticker that started the run first would emit one that satisfies every
    // assertion below and witnesses nothing about the caller's path.
    let ids = seed_quiet(&pool, 1).await;
    let id = ids[0].clone();

    let events = Arc::new(Recorder::default());
    let connector = knobas_db::test_util::test_connector().await;
    let scheduler = Scheduler::start(SchedulerDeps {
        pool: sched_pool,
        connections: Arc::new(TestConnections(connector)),
        registry: Arc::new(SlowRegistry {
            inside: Arc::new(AtomicUsize::new(0)),
            peak: Arc::new(AtomicUsize::new(0)),
            dwell: Duration::from_secs(5),
            door: None,
            fault: None,
        }),
        secrets: Arc::new(MemoryStore::new()),
        events: Arc::clone(&events) as Arc<dyn SyncEvents>,
        timing: FAST,
        quit: knobas_sync::scheduler::Quit::default(),
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

    let events = Arc::new(Recorder::default());
    let connector = knobas_db::test_util::test_connector().await;
    let scheduler = Scheduler::start(SchedulerDeps {
        pool: sched_pool,
        connections: Arc::new(TestConnections(connector)),
        registry: Arc::new(SlowRegistry {
            inside: Arc::new(AtomicUsize::new(0)),
            peak: Arc::new(AtomicUsize::new(0)),
            dwell: Duration::from_secs(5),
            door: None,
            fault: None,
        }),
        secrets: Arc::new(MemoryStore::new()),
        events: Arc::clone(&events) as Arc<dyn SyncEvents>,
        timing: FAST,
        quit: knobas_sync::scheduler::Quit::default(),
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
/// holds by construction: the TypeScript union is generated from `SyncPhase`
/// (`app/src/lib/ipc/generated/sources.ts`, #1327). This is the stronger
/// half, and it was missing: `Started` was declared on both sides and
/// sent by nobody, `Writing` was sent from the scheduler *after* the run had
/// finished (and on the failure path, where nothing was written), and the
/// terminal message carried a hardcoded `elapsed_ms: 0`. A phase the UI can
/// never observe invites a frontend to wait for a state that never arrives.
///
/// Driven through `Scheduler::trigger`, because the terminal message is the
/// ticker's: a test that drove only `execute_run` could not see
/// `Finished`/`Failed` at all, which is how the zero survived. The expected set
/// is the union the generator emits for `SyncPhase`, asked of `ts_rs` in this
/// process rather than listed here -- a list this test owns is a list this test
/// can quietly shrink -- and rather than read out of the generated file, which
/// is the same text one `just ipc-mirror` later (#1332).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_declared_phase_is_actually_emitted() {
    let _serial = serially().await;
    let declared: Vec<String> = <SyncPhase as ts_rs::TS>::inline(&ts_rs::Config::new())
        .split('|')
        .map(|member| member.trim().trim_matches('"').to_owned())
        .collect();
    assert!(
        declared.len() > 1 && declared.iter().all(|m| !m.is_empty() && !m.contains(' ')),
        "SyncPhase's TypeScript is no longer a string union: {declared:?}"
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
    // Off the schedule: this sink has to hear the run *start*, and a ticker
    // that started the run first would hand it a join instead.
    let ids = seed_quiet(&pool, 1).await;
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
            door: None,
            fault,
        }),
        secrets: Arc::new(MemoryStore::new()),
        events: Arc::new(Silent),
        timing: FAST,
        quit: knobas_sync::scheduler::Quit::default(),
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
    // Off the schedule: the assertion is on the spelling of *this* caller's
    // trigger, and a ticker that got there first would have logged `first_run`.
    let ids = seed_quiet(&pool, 2).await;
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

/// Wait until a source has run at least once and none of its runs is open.
///
/// The wait the scheduler tests used to spell as "long enough for every run
/// to have finished" (#307, #412): by source rather than by run id, because
/// the runs the ticker starts on its own are the ones a caller never got an
/// id for. Five seconds is the deadline every other wait in this file
/// allows, for the same reason -- a loaded machine, not a measurement.
async fn await_source_idle(pool: &PgPool, source_id: &str) {
    for _ in 0..200 {
        let runs = run_log::list(pool, Some(source_id), 10).await.unwrap();
        if !runs.is_empty() && runs.iter().all(|r| r.finished_at.is_some()) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("{source_id} still has a run open after five seconds");
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
    // Off the schedule: the starter must be the one that started the run, or
    // "still hears it start" is about the ticker's run and not its own.
    let ids = seed_quiet(&pool, 1).await;
    let id = ids[0].clone();
    let (deps, held) = held_deps(sched_pool).await;
    let scheduler = Scheduler::start(deps).await.unwrap();

    let starter = Heard::new();
    let joiner = Heard::new();
    let first = scheduler
        .trigger(&id, SyncTrigger::Manual, Some(starter.sink()))
        .await
        .unwrap();
    // Held at the door, so the second caller joins a run that is genuinely
    // still going rather than one that is already over (#1920).
    await_inside(&held.inside).await;
    let second = scheduler
        .trigger(&id, SyncTrigger::Manual, Some(joiner.sink()))
        .await
        .unwrap();
    held.door.open();
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

// -- #358: a sync after a write waits for a run that could have seen it -----

/// **A sync after a write is not satisfied by a run that was already going.**
///
/// The dedupe the test above is about -- a second caller handed the run in
/// flight -- is right for *Sync now* and wrong for the caller
/// `sync_after_write` serves:
/// one that has just written something and is about to look for it. A run that
/// began before the write cannot carry it, however faithfully its ending is
/// delivered. That is the start-work link step's flake (#358): the branch
/// write's own refresh was still going when the flow, having since opened a
/// pull request, asked for a sync and looked.
///
/// The run this ends up waiting on must therefore have **started after the
/// stale one finished**, which is the only reading under which it could have
/// seen a write made before the call.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_sync_after_a_write_waits_for_a_run_that_began_after_it_asked() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed_quiet(&pool, 1).await;
    let id = ids[0].clone();
    let (deps, held) = held_deps(sched_pool).await;
    let scheduler = Scheduler::start(deps).await.unwrap();

    // The run `write_queue::flush` leaves behind after every write that lands.
    // It is going before the caller below writes the thing it will look for,
    // and it stays going, held at the door, until that caller has joined it
    // (#1927). Before, a 600 ms dwell and a 100 ms margin stood for that, and
    // a load that saw the dwell out first left `sync_after_write` starting
    // the one run itself: every assertion below still passed, and the join
    // this test is about never happened.
    let stale = scheduler
        .trigger(&id, SyncTrigger::Manual, None)
        .await
        .unwrap();
    await_inside_count(&held.inside, 1).await;
    let asked = tokio::spawn({
        let scheduler = scheduler.clone();
        let id = id.clone();
        async move { scheduler.sync_after_write(&id).await }
    });
    // Its sink is enrolled in the stale run: the trigger above enrolled none.
    await_watching(&scheduler, &id, 1).await;
    held.door.open();

    let waited_on = tokio::time::timeout(Duration::from_secs(30), asked)
        .await
        .expect("the sync after the write answers once its run ends")
        .unwrap()
        .unwrap();
    scheduler.shutdown().await;

    assert_ne!(
        waited_on, stale,
        "it settled for the run that was already going, whose ending says \
         nothing about a write made after it started"
    );
    let stale_row = run_log::get(&pool, stale).await.unwrap().expect("the run");
    let waited_row = run_log::get(&pool, waited_on)
        .await
        .unwrap()
        .expect("the run");
    assert!(
        waited_row.started_at >= stale_row.finished_at.expect("the stale run ended"),
        "the run it waited on overlapped the stale one, so it is not one \
         that could have seen a write made after the call: {stale_row:?} \
         then {waited_row:?}"
    );
    // **Two, and not more.** The bound is the whole of story 11's side of this:
    // a step that gives up after one effective sync stays distinguishable from
    // one that spins, and an implementation that looped until it liked what it
    // saw would pass every assertion above. Asserted as a count rather than as
    // `finished_at.is_some()` on the run it named, which would witness nothing
    // -- `shutdown` settles a cancelled run too, so that field is set whether
    // this waited or not.
    let runs = run_log::list(&pool, Some(&id), 10).await.unwrap();
    assert_eq!(
        runs.len(),
        2,
        "one run waited out and one asked for is the whole budget: {runs:?}"
    );
    retire(&pool, &ids).await;
}

/// **…and it costs one run when there was nothing to wait out.**
///
/// The pair to the test above, and the reason the fix is not "always sync
/// twice": the ordinary write-then-read is one caller, one run. A call that
/// waited twice unconditionally would double the latency of every start-work
/// link step and every protocol publish (#289) to buy a case that had not
/// happened.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_sync_after_a_write_with_nothing_in_flight_starts_one_run_and_no_more() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed_quiet(&pool, 1).await;
    let id = ids[0].clone();
    let (deps, _) = deps(sched_pool, Duration::from_millis(100)).await;
    let scheduler = Scheduler::start(deps).await.unwrap();

    let waited_on = scheduler.sync_after_write(&id).await.unwrap();
    scheduler.shutdown().await;

    let runs = run_log::list(&pool, Some(&id), 10).await.unwrap();
    assert_eq!(
        runs.len(),
        1,
        "one caller, one sync -- it started a second run nobody asked \
         for: {runs:?}"
    );
    assert_eq!(runs[0].id, waited_on, "and it is the one it named");
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
    let (deps, held) = held_deps(sched_pool).await;
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
    // does not spend the claim either. The run is held at the door while the
    // observer joins (#1920): a 300 ms dwell used to, and a loaded machine
    // could see it out first and have the observer start a run of its own.
    let observer = Heard::new();
    await_inside(&held.inside).await;
    assert_eq!(
        scheduler
            .trigger(&id, SyncTrigger::Manual, Some(observer.sink()))
            .await
            .unwrap(),
        scheduled,
        "the observer joined the wake's run rather than starting one"
    );
    held.door.open();
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
    let inside = Arc::new(AtomicUsize::new(0));
    let door = Door::shut();
    let scheduler = Scheduler::start(SchedulerDeps {
        pool: sched_pool,
        connections: Arc::new(TestConnections(connector)),
        registry: Arc::new(SlowRegistry {
            inside: Arc::clone(&inside),
            peak: Arc::new(AtomicUsize::new(0)),
            // Held until the live watcher has joined, so it joins a run that is
            // genuinely still going (#1920); the fault is raised after it.
            dwell: Duration::ZERO,
            door: Some(Arc::clone(&door)),
            fault: Some(|| SourceError::Unreachable("simulated".into())),
        }),
        secrets: Arc::new(MemoryStore::new()),
        events: Arc::new(Silent),
        timing: FAST,
        quit: knobas_sync::scheduler::Quit::default(),
    })
    .await
    .unwrap();

    // The wake's run: `add_source` pokes the scheduler, and the ticker triggers
    // with no channel of its own.
    let failed = scheduler
        .trigger(&id, SyncTrigger::FirstRun, None)
        .await
        .unwrap();
    await_inside(&inside).await;
    let live = Heard::new();
    assert_eq!(
        scheduler
            .trigger(&id, SyncTrigger::Manual, Some(live.sink()))
            .await
            .unwrap(),
        failed,
        "the live watcher joined the run that was going, rather than starting one"
    );
    door.open();
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
            door: None,
            fault: Some(|| SourceError::Unreachable("simulated".into())),
        }),
        secrets: Arc::new(MemoryStore::new()),
        events: Arc::new(Silent),
        timing: FAST,
        quit: knobas_sync::scheduler::Quit::default(),
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
    let inside = Arc::new(AtomicUsize::new(0));
    let door = Door::shut();
    let scheduler = Scheduler::start(SchedulerDeps {
        pool: sched_pool,
        connections: Arc::new(TestConnections(connector)),
        registry: Arc::new(SlowRegistry {
            inside: Arc::clone(&inside),
            peak: Arc::new(AtomicUsize::new(0)),
            // Held until the wizard has joined (#1920).
            dwell: Duration::ZERO,
            door: Some(Arc::clone(&door)),
            fault: Some(|| SourceError::Unreachable("simulated".into())),
        }),
        secrets: Arc::new(MemoryStore::new()),
        events: Arc::new(Silent),
        timing: FAST,
        quit: knobas_sync::scheduler::Quit::default(),
    })
    .await
    .unwrap();

    let woken = scheduler
        .trigger(&id, SyncTrigger::FirstRun, None)
        .await
        .unwrap();
    await_inside(&inside).await;
    let wizard = Heard::new();
    let joined = scheduler
        .trigger(&id, SyncTrigger::FirstRun, Some(wizard.sink()))
        .await
        .unwrap();
    door.open();
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
    // Both runs held at the door until both second callers have arrived, so
    // each one finds a run still going (#1920). A 400 ms dwell used to, and a
    // loaded machine could see it out before the second trigger.
    let (deps, held) = held_deps(sched_pool).await;
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
    let b = Heard::new();
    let b_wizard = scheduler
        .trigger(&wake_first, SyncTrigger::FirstRun, Some(b.sink()))
        .await
        .unwrap();
    held.door.open();

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
/// the process (`crates/knobas-sync/tests/it/progress.rs` pins both directly on
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
    // Held at the door until the live sink has joined behind the broken one
    // (#1920): a 300 ms dwell could run out first on a loaded machine, and the
    // live sink then started a run of its own.
    let (deps, held) = held_deps(sched_pool).await;
    let scheduler = Scheduler::start(deps).await.unwrap();

    let run = scheduler
        .trigger(
            &id,
            SyncTrigger::Manual,
            Some(Arc::new(Boom) as Arc<dyn ProgressSink>),
        )
        .await
        .unwrap();
    let live = Heard::new();
    scheduler
        .trigger(&id, SyncTrigger::Manual, Some(live.sink()))
        .await
        .unwrap();
    held.door.open();

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
            door: None,
            fault: Some(|| panic!("the adapter blew up")),
        }),
        secrets: Arc::new(MemoryStore::new()),
        events: Arc::new(Silent),
        timing: FAST,
        quit: knobas_sync::scheduler::Quit::default(),
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

    // Neither task reached its own `settle`; the guard recorded both rows
    // (#1638), so this test leaves no residue for the ones that assert the
    // reconciler has nothing to do. The test below pins the record itself.
    retire(&pool, &ids).await;
}

/// **A run whose adapter panics is recorded, and its source stops running**
/// (#1638, SYNC-4).
///
/// The guard above closes the watchers, which keeps ADR-0005 for the caller.
/// Before #1638 it did only that: the run's row stayed open and no `sync:state`
/// ending went out, so `status_for` -- which takes the open run per source --
/// said `running: true` with no next time for the rest of the session, however
/// many later runs finished. The guard now records the ending too: the row is
/// finished, one ending is emitted, and the start after this one finds nothing
/// of this run's to reconcile.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_run_whose_adapter_panics_is_recorded_and_stops_running() {
    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let ids = seed_quiet(&pool, 1).await;
    let id = ids[0].clone();

    let events = Arc::new(Recorder::default());
    let connector = knobas_db::test_util::test_connector().await;
    let scheduler = Scheduler::start(SchedulerDeps {
        pool: sched_pool,
        connections: Arc::new(TestConnections(connector)),
        registry: Arc::new(SlowRegistry {
            inside: Arc::new(AtomicUsize::new(0)),
            peak: Arc::new(AtomicUsize::new(0)),
            dwell: Duration::from_millis(10),
            door: None,
            fault: Some(|| panic!("the adapter blew up")),
        }),
        secrets: Arc::new(MemoryStore::new()),
        events: Arc::clone(&events) as Arc<dyn SyncEvents>,
        timing: FAST,
        quit: knobas_sync::scheduler::Quit::default(),
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

    // Read the moment the caller hears the ending, with no wait in between:
    // the row is written before the watchers are told, as on the ordinary
    // path, so a caller that reacts to the ending reads a finished run.
    let status = knobas_sync::scheduler::status_for(&pool, &id)
        .await
        .unwrap()
        .expect("the source exists");
    assert!(
        !status.running,
        "the panicked run still reads as running: {status:?}"
    );
    assert_eq!(status.run_id, Some(run), "{status:?}");
    assert!(
        status.next_run_at.is_some(),
        "a source whose run ended has a next time: {status:?}"
    );
    // An error on the ladder like any other: the first rung, in place of the
    // hour `seed_quiet` parked the source behind. Not the cancelled run's
    // untouched backoff -- a panic is the adapter's fault, not the reader's.
    let until = status.backoff_until.expect("a failed run backs off");
    let rung = chrono::Duration::from_std(config::backoff_after(1)).unwrap();
    let now = chrono::Utc::now();
    assert!(
        until > now && until <= now + rung,
        "the panicked run backs off one rung: {status:?}"
    );
    let row = run_log::list(&pool, Some(&id), 10)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.id == run)
        .expect("the run's row");
    assert!(row.finished_at.is_some(), "the row is still open: {row:?}");
    assert_eq!(row.outcome, Some(run_log::SyncOutcome::Error));
    assert_eq!(
        row.error.as_deref(),
        ending.message.as_deref(),
        "the row and the ending say the same thing"
    );

    scheduler.shutdown().await;

    // One ending, not one from the guard and another from somewhere else.
    let endings = events
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|s| s.run_id == Some(run) && !s.running)
        .count();
    assert_eq!(endings, 1, "the panicked run's sync:state endings");
    // What `reconcile_abandoned` would close at the next start: nothing of
    // this source's. Asked of the source rather than by running the
    // reconciler, which is whole-database and would close other binaries'
    // live runs.
    let open: i64 = sqlx::query_scalar(
        "select count(*) from knobas.sync_run where source_id = $1 and finished_at is null",
    )
    .bind(&id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(open, 0, "a run of this source is left for the next start");
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
    let (deps, held) = held_deps(sched_pool).await;
    let scheduler = Scheduler::start(deps).await.unwrap();

    // The same setup as
    // `a_first_run_trigger_after_the_run_ended_is_served_the_ending_from_the_record`,
    // and for the same reasons: no sink on the first trigger, so the `FirstRun`
    // claim is still unspent, and `Manual` to know the run is completely over.
    // Held at the door while the observer joins (#1920).
    let scheduled = scheduler
        .trigger(&id, SyncTrigger::FirstRun, None)
        .await
        .unwrap();
    let observer = Heard::new();
    assert_eq!(
        scheduler
            .trigger(&id, SyncTrigger::Manual, Some(observer.sink()))
            .await
            .unwrap(),
        scheduled,
        "the observer joined the wake's run rather than starting one"
    );
    held.door.open();
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
    let (deps, held) = held_deps(sched_pool).await;
    let scheduler = Scheduler::start(deps).await.unwrap();

    // The first source's own first sync, run to completion. `Manual` with a
    // sink is how the test knows it is *completely* over rather than merely
    // logged -- `settle` writes `finished_at` before the run tells anybody --
    // and it asks for work rather than for the first sync, so it leaves the
    // `FirstRun` claim unspent, which is the state that makes the entry
    // serveable to a later wizard.
    // Held at the door while the observer joins (#1920).
    let deleted_sources_run = scheduler
        .trigger(&id, SyncTrigger::FirstRun, None)
        .await
        .unwrap();
    let observer = Heard::new();
    assert_eq!(
        scheduler
            .trigger(&id, SyncTrigger::Manual, Some(observer.sink()))
            .await
            .unwrap(),
        deleted_sources_run,
        "the observer joined the run rather than starting one"
    );
    held.door.open();
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
async fn await_inside(inside: &AtomicUsize) {
    await_inside_count(inside, 1).await;
}

/// Wait until at least `runs` runs are inside the adapter at once.
/// Wait until `runs` runs found the concurrency cap full and wait for a
/// permit (#1927): the positive sign that the cap is holding them, where an
/// adapter count alone cannot tell a held run from one not yet arrived.
async fn await_waiting_for_a_permit(scheduler: &Scheduler, runs: usize) {
    // A hang guard, as in `await_inside_count`.
    for _ in 0..3000 {
        if scheduler.waiting_for_a_permit() >= runs {
            assert_eq!(
                scheduler.waiting_for_a_permit(),
                runs,
                "more runs wait for a permit than were triggered past the cap"
            );
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!(
        "{} of {runs} runs waited for a permit in thirty seconds: the cap let them in, \
         or they never asked",
        scheduler.waiting_for_a_permit()
    );
}

/// Wait until `sinks` callers are enrolled in `source`'s run in flight: the
/// positive sign that a caller has joined the run, which nothing it returns
/// shows while the run is still going (#1927).
async fn await_watching(scheduler: &Scheduler, source: &str, sinks: usize) {
    // A hang guard, as in `await_inside_count`.
    for _ in 0..3000 {
        if scheduler.watching(source).await >= sinks {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!(
        "{} of {sinks} callers joined {source}'s run in thirty seconds",
        scheduler.watching(source).await
    );
}

async fn await_inside_count(inside: &AtomicUsize, runs: usize) {
    // A deadline on a machine that may be running six other test binaries,
    // not a measurement of how long reaching the adapter takes. Thirty
    // seconds: a held run never leaves, so the margin costs nothing on the
    // passing side.
    for _ in 0..3000 {
        if inside.load(Ordering::SeqCst) >= runs {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!(
        "{} of {runs} runs reached the adapter in thirty seconds; there is nothing to \
         interleave with",
        inside.load(Ordering::SeqCst)
    );
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
    let (deps, held) = held_deps(sched_pool).await;
    let inside = &held.inside;
    let scheduler = Scheduler::start(deps).await.unwrap();

    // A mirror to purge, first: one run to completion, so the source has an
    // item and a live entity before anybody deletes anything.
    held.door.admit(1);
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
    await_inside(inside).await;
    config::delete(&pool, &id, true).await.unwrap();
    assert!(
        inside.load(Ordering::SeqCst) > 0,
        "the delete has to commit while the run is still fetching. If the run \
         had already committed, the delete's own purge would tidy up after it \
         and this test would be green over a scheduler that purges nothing"
    );
    scheduler.forget_source(&id, Purge::Items).await;
    held.door.open();

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
    let (deps, held) = held_deps(sched_pool).await;
    let inside = &held.inside;
    let scheduler = Scheduler::start(deps).await.unwrap();

    let watcher = Heard::new();
    scheduler
        .trigger(&id, SyncTrigger::Manual, Some(watcher.sink()))
        .await
        .unwrap();
    await_inside(inside).await;

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
    held.door.open();

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
    let (deps, held) = held_deps(sched_pool).await;
    let inside = &held.inside;
    let scheduler = Scheduler::start(deps).await.unwrap();

    let watcher = Heard::new();
    scheduler
        .trigger(&id, SyncTrigger::Manual, Some(watcher.sink()))
        .await
        .unwrap();
    await_inside(inside).await;

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
    held.door.open();

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
    let (deps, held) = held_deps(sched_pool).await;
    let inside = &held.inside;
    let scheduler = Scheduler::start(deps).await.unwrap();

    let watcher = Heard::new();
    scheduler
        .trigger(&id, SyncTrigger::Manual, Some(watcher.sink()))
        .await
        .unwrap();
    await_inside(inside).await;
    config::delete(&pool, &id, false).await.unwrap();
    assert!(
        inside.load(Ordering::SeqCst) > 0,
        "the delete has to commit while the run is still fetching. If the run \
         had already committed, the delete's own purge would tidy up after it \
         and this test would be green over a scheduler that purges nothing"
    );
    scheduler.forget_source(&id, Purge::Keep).await;
    held.door.open();

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
    // Held at the door until the new source's wizard has its own run, so the
    // deletion lands mid-run (#1920) rather than 50 ms into a 400 ms dwell.
    let (deps, held) = held_deps(sched_pool).await;
    let scheduler = Scheduler::start(deps).await.unwrap();

    let watcher = Heard::new();
    let in_flight = scheduler
        .trigger(&id, SyncTrigger::FirstRun, Some(watcher.sink()))
        .await
        .unwrap();
    await_inside(&held.inside).await;

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
    held.door.open();

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

// ---------------------------------------------------------------------------
// The write queue's flush beside the ticker (#1629, spec #1618 ruling A7).
// ---------------------------------------------------------------------------

/// A [`Slow`] whose writes land, each once it is let through its door: a
/// source that takes its time over a write, as an unreachable one spends the
/// send budget on each, and that takes it for as long as the test needs.
///
/// `started` counts the calls to `write`, `finished` the ones that returned,
/// of the one source a test counts; a write in flight is the difference.
/// Only that one: the flush reaches every source in this binary's database,
/// and a source an earlier test left owing would otherwise count here.
struct Writer {
    slow: Slow,
    door: Arc<Door>,
    counted: bool,
    started: Arc<AtomicUsize>,
    finished: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl Source for Writer {
    async fn view(
        &self,
        entity: &str,
        section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, SourceError> {
        self.slow.view(entity, section).await
    }
    fn descriptor(&self) -> SourceDescriptor {
        self.slow.descriptor()
    }
    async fn test_connection(&self) -> Result<knobas_source::ConnectionInfo, SourceError> {
        self.slow.test_connection().await
    }
    async fn sync(
        &self,
        cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        self.slow.sync(cursor, sink).await
    }
    async fn write(
        &self,
        _op: knobas_source::WriteOp,
    ) -> Result<knobas_source::WriteReceipt, SourceError> {
        if self.counted {
            self.started.fetch_add(1, Ordering::SeqCst);
        }
        self.door.pass().await;
        if self.counted {
            self.finished.fetch_add(1, Ordering::SeqCst);
        }
        Ok(knobas_source::WriteReceipt::none())
    }
    async fn reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError> {
        self.slow.reachable_transitions(entity).await
    }
}

struct WriterRegistry {
    door: Arc<Door>,
    counted: String,
    started: Arc<AtomicUsize>,
    finished: Arc<AtomicUsize>,
}

impl AdapterRegistry for WriterRegistry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        Vec::new()
    }
    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        Ok(Box::new(Writer {
            counted: instance.id == self.counted,
            slow: Slow {
                id: instance.id,
                inside: Arc::new(AtomicUsize::new(0)),
                peak: Arc::new(AtomicUsize::new(0)),
                dwell: Duration::ZERO,
                door: None,
                fault: None,
            },
            door: Arc::clone(&self.door),
            started: Arc::clone(&self.started),
            finished: Arc::clone(&self.finished),
        }))
    }
}

/// Deps whose every source syncs at once and writes once `door` lets it, and
/// the two write counters of `counted`.
async fn writer_deps(
    pool: PgPool,
    door: &Arc<Door>,
    counted: &str,
) -> (SchedulerDeps, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let connector = knobas_db::test_util::test_connector().await;
    let started = Arc::new(AtomicUsize::new(0));
    let finished = Arc::new(AtomicUsize::new(0));
    let deps = SchedulerDeps {
        pool,
        connections: Arc::new(TestConnections(connector)),
        registry: Arc::new(WriterRegistry {
            door: Arc::clone(door),
            counted: counted.to_owned(),
            started: Arc::clone(&started),
            finished: Arc::clone(&finished),
        }),
        secrets: Arc::new(MemoryStore::new()),
        events: Arc::new(Silent),
        timing: FAST,
        quit: knobas_sync::scheduler::Quit::default(),
    };
    (deps, started, finished)
}

/// Queue a comment on a mirrored entity of `source_id`, without the attempt
/// `submit` makes: the flush the ticker wakes is the only thing sending it.
async fn owe_a_comment(
    deps: &SchedulerDeps,
    source_id: &str,
    key: &str,
) -> knobas_core::write_queue::QueuedWrite {
    let entity = knobas_core::entity::EntityRef::new(source_id, key);
    sqlx::query(
        "insert into knobas.entity (id, kind, title) values ($1,'ticket','a ticket')
         on conflict (id) do nothing",
    )
    .bind(entity.to_string())
    .execute(&deps.pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,$2,'ticket','a ticket','','{}'::jsonb)
         on conflict (entity_id) do nothing",
    )
    .bind(entity.to_string())
    .bind(source_id)
    .execute(&deps.pool)
    .await
    .unwrap();
    knobas_sync::write_queue::queue(
        deps,
        source_id,
        knobas_source::WriteOp::Comment {
            entity: entity.to_string(),
            body: format!("on {key}"),
        },
    )
    .await
    .unwrap()
}

/// Drop what `ids` still owe, so the next test's flush does not send it: the
/// write queue's half of [`retire`].
async fn drop_owed(pool: &PgPool, ids: &[String]) {
    sqlx::query("delete from knobas.write_queue where source_id = any($1)")
        .bind(ids)
        .execute(pool)
        .await
        .unwrap();
}

/// Wait for `counter` to reach `n`, against the same five-second deadline
/// [`await_inside`] allows.
async fn await_count(counter: &Arc<AtomicUsize>, n: usize) {
    for _ in 0..500 {
        if counter.load(Ordering::SeqCst) >= n {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the source was asked for {n} writes in five seconds, not {counter:?}");
}

/// SYNC-2 (#1629): a write that takes ten seconds does not hold off a
/// source that falls due one second in. The flush runs on its own task, so
/// the ticker goes on looking while a write is on the wire.
///
/// Red with the flush inline in the tick: the ticker sits in the write until
/// it returns, and the due source's run starts after it. The write here is
/// held at its door until the run has started (#1920), so "after it" is
/// never: the inline flush fails at the hang guard below rather than at a
/// clock reading.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_slow_write_does_not_hold_off_a_source_that_falls_due() {
    /// The ticket's: a source due one second in.
    const DUE_IN: Duration = Duration::from_secs(1);

    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    // Off the sync schedule: only its write is owed, and the flush does not
    // read the backoff.
    let writing = seed_quiet(&pool, 1).await;
    let due_soon = seed(&pool, 1).await;
    let (due_at,): (chrono::DateTime<chrono::Utc>,) = sqlx::query_as(
        "update knobas.source_config set backoff_until = now() + make_interval(secs => $2)
          where id = $1 returning backoff_until",
    )
    .bind(&due_soon[0])
    .bind(DUE_IN.as_secs_f64())
    .fetch_one(&pool)
    .await
    .unwrap();
    let door = Door::shut();
    let (deps, started, finished) = writer_deps(sched_pool, &door, &writing[0]).await;

    let scheduler = Scheduler::start(deps).await.unwrap();
    owe_a_comment(scheduler.deps(), &writing[0], "W-1").await;
    await_count(&started, 1).await;

    // The write stays on the wire until the run has started.
    let run = tokio::time::timeout(HANG_GUARD, async {
        loop {
            if let Some(run) = run_log::list(&pool, Some(&due_soon[0]), 1)
                .await
                .unwrap()
                .pop()
            {
                break run;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the due source's run did not start while the write was on the wire");
    assert_eq!(
        finished.load(Ordering::SeqCst),
        0,
        "the write is still on the wire"
    );
    assert!(
        run.started_at >= due_at,
        "the run is the one its falling due started: {run:?}"
    );
    door.open();

    scheduler.shutdown().await;
    drop_owed(&pool, &writing).await;
    retire(&pool, &[writing, due_soon].concat()).await;
}

/// SYNC-2 (#1629): a quit during a flush stops it between writes. The write
/// on the wire is let finish inside the grace, so its `sent` lands; the two
/// behind it are not started, and stay owed for the next launch.
///
/// Red with the flush inline in the tick: nothing looks at the cancel inside
/// it, and the next two writes go out once the first returns.
///
/// Ordered, not timed (#1920): the first write is held at its door until the
/// quit has begun, and only then let finish. The old version raced a 750 ms
/// write against the quit and bounded the quit by the grace, and on a loaded
/// machine either could lose.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_quit_during_a_flush_stops_it_between_writes() {
    use knobas_core::write_queue::{self as store, WriteState};

    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let writing = seed_quiet(&pool, 1).await;
    let door = Door::shut();
    let (mut deps, started, finished) = writer_deps(sched_pool, &door, &writing[0]).await;
    // A grace far past the wait on the quit below: the write on the wire is
    // let finish whenever this test opens its door, and an abort would hold
    // the quit for an hour rather than land early.
    deps.timing.shutdown_grace = PATIENT_GRACE;

    let scheduler = Scheduler::start(deps).await.unwrap();
    let mut owed = Vec::new();
    for key in ["Q-1", "Q-2", "Q-3"] {
        owed.push(owe_a_comment(scheduler.deps(), &writing[0], key).await);
    }
    await_count(&started, 1).await;

    let quitting = tokio::spawn({
        let scheduler = scheduler.clone();
        async move { scheduler.shutdown().await }
    });
    tokio::time::timeout(HANG_GUARD, async {
        while !scheduler.is_shutting_down() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the quit begins");
    // The quit has begun with the first write on the wire. Let it, and
    // anything after it, through.
    door.open();
    tokio::time::timeout(HANG_GUARD, quitting)
        .await
        .expect("the quit waited for something other than the write on the wire")
        .unwrap();

    assert_eq!(
        started.load(Ordering::SeqCst),
        1,
        "a write was started after the quit began"
    );
    assert_eq!(
        finished.load(Ordering::SeqCst),
        1,
        "the write on the wire was aborted rather than let finish"
    );
    let mut states = Vec::new();
    for write in &owed {
        states.push(store::get(&pool, write.id).await.unwrap().unwrap().state);
    }
    assert_eq!(
        states.iter().filter(|s| **s == WriteState::Sent).count(),
        1,
        "the write the source took is marked sent, and no other: {states:?}"
    );
    assert_eq!(
        states.iter().filter(|s| **s == WriteState::Pending).count(),
        2,
        "the writes behind the quit stay owed: {states:?}"
    );
    drop_owed(&pool, &writing).await;
    retire(&pool, &writing).await;
}

/// #1932: a write the flush task still has at its source when the quit's
/// grace runs out is aborted and held as one the source may have, naming the
/// source to check, so the next launch does not send it again blind. The
/// writes behind it were never started and stay owed.
///
/// Red before #1932: the abort left the row pending, and the next launch's
/// flush sent the comment again. The door stays shut until the quit is over,
/// so the grace's length decides nothing here.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_write_on_the_wire_when_the_grace_ends_is_held_not_sent_again() {
    use knobas_core::write_queue::{self as store, WriteState};

    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let writing = seed_quiet(&pool, 1).await;
    let door = Door::shut();
    let (mut deps, started, finished) = writer_deps(sched_pool, &door, &writing[0]).await;
    deps.timing.shutdown_grace = Duration::from_millis(100);

    let scheduler = Scheduler::start(deps).await.unwrap();
    let mut owed = Vec::new();
    for key in ["C-1", "C-2"] {
        owed.push(owe_a_comment(scheduler.deps(), &writing[0], key).await);
    }
    await_count(&started, 1).await;
    tokio::time::timeout(HANG_GUARD, scheduler.shutdown())
        .await
        .expect("the quit ends at its grace");
    door.open();

    assert_eq!(
        finished.load(Ordering::SeqCst),
        0,
        "the write was let finish"
    );
    let mut rows = Vec::new();
    for write in &owed {
        rows.push(store::get(&pool, write.id).await.unwrap().unwrap());
    }
    let cut: Vec<_> = rows
        .iter()
        .filter(|r| r.state == WriteState::Held)
        .collect();
    assert_eq!(
        cut.len(),
        1,
        "the write the grace cut is held, and no other: {:?}",
        rows.iter().map(|r| r.state).collect::<Vec<_>>()
    );
    assert_eq!(
        cut[0].held_snapshot.as_ref().unwrap()["unconfirmed"]["source"],
        serde_json::json!("Slow"),
        "held as one the source may have, naming the source to check"
    );
    assert_eq!(
        rows.iter()
            .filter(|r| r.state == WriteState::Pending)
            .count(),
        1,
        "the write behind it stays owed"
    );
    drop_owed(&pool, &writing).await;
    retire(&pool, &writing).await;
}

/// #1924: a submit's flush stops between writes once the quit has begun,
/// as the flush task does. Its first write, on the wire at the quit, lands;
/// the entity's next head, which the flush would take once that one lands,
/// is not started.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_quit_stops_a_submits_flush_between_writes() {
    a_quit_stops_a_flush_from_outside_the_task(Caller::Submit).await;
}

/// #1924: as a submit's, *Flush now*'s: it ignores what a write waits on,
/// not the quit.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_quit_stops_flush_now_between_writes() {
    a_quit_stops_a_flush_from_outside_the_task(Caller::FlushNow).await;
}

/// Which flush [`a_quit_stops_a_flush_from_outside_the_task`] sends with.
#[derive(Clone, Copy, Debug)]
enum Caller {
    /// `flush_entity_of`, on two writes to one entity: a submit, a worklog,
    /// a background send, and through `flush_entity` *Send mine anyway* and
    /// *Edit and send*.
    Submit,
    /// `flush_now` on the source, two writes on two entities.
    FlushNow,
}

/// Two writes owed, sent by `caller` rather than the flush task, which the
/// held-off tick never wakes. The flush runs in the scheduler's background
/// set, so the quit waits for it as it waits for a background send; the first
/// write is held at its door until the quit has begun, and only then let
/// through. A flush that started the second write after the quit sends it
/// through the open door, and the count says so.
///
/// Then a submit made once the quit has begun, on a third entity: its flush
/// returns without sending, and its write stays owed for the next start. It
/// does not wait for the source's lock, which the first flush holds until
/// the door opens.
async fn a_quit_stops_a_flush_from_outside_the_task(caller: Caller) {
    use knobas_core::write_queue::{self as store, WriteState};
    use knobas_sync::write_queue as flusher;

    let _serial = serially().await;
    let (pool, sched_pool) = pools().await;
    let writing = seed_quiet(&pool, 1).await;
    let door = Door::shut();
    let (mut deps, started, finished) = writer_deps(sched_pool, &door, &writing[0]).await;
    deps.timing = SchedulerTiming {
        startup_delay: PATIENT_GRACE,
        tick: PATIENT_GRACE,
        shutdown_grace: PATIENT_GRACE,
    };

    let scheduler = Scheduler::start(deps).await.unwrap();
    let keys = match caller {
        Caller::Submit => ["E-1", "E-1"],
        Caller::FlushNow => ["N-1", "N-2"],
    };
    let mut owed = Vec::new();
    for key in keys {
        owed.push(owe_a_comment(scheduler.deps(), &writing[0], key).await);
    }
    let spawned = scheduler.background().spawn({
        let scheduler = scheduler.clone();
        let source = writing[0].clone();
        let first = owed[0].id;
        async move {
            match caller {
                Caller::Submit => flusher::flush_entity_of(scheduler.deps(), first).await,
                Caller::FlushNow => flusher::flush_now(scheduler.deps(), Some(&source)).await,
            }
            .expect("the flush reads and writes the queue");
        }
    });
    assert!(spawned, "the flush joins the background set");
    await_count(&started, 1).await;

    let quitting = tokio::spawn({
        let scheduler = scheduler.clone();
        async move { scheduler.shutdown().await }
    });
    tokio::time::timeout(HANG_GUARD, async {
        while !scheduler.is_shutting_down() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the quit begins");

    let late = owe_a_comment(scheduler.deps(), &writing[0], "L-1").await;
    tokio::time::timeout(
        HANG_GUARD,
        flusher::flush_entity_of(scheduler.deps(), late.id),
    )
    .await
    .expect("a submit made during the quit waited for the source's lock")
    .expect("the flush reads the queue");

    door.open();
    tokio::time::timeout(HANG_GUARD, quitting)
        .await
        .expect("the quit waited for something other than the write on the wire")
        .unwrap();

    assert_eq!(
        started.load(Ordering::SeqCst),
        1,
        "{caller:?} started a write after the quit began"
    );
    assert_eq!(
        finished.load(Ordering::SeqCst),
        1,
        "the write on the wire was aborted rather than let finish"
    );
    let mut states = Vec::new();
    for write in owed.iter().chain([&late]) {
        states.push(store::get(&pool, write.id).await.unwrap().unwrap().state);
    }
    assert_eq!(
        states,
        [WriteState::Sent, WriteState::Pending, WriteState::Pending],
        "the write the source took is marked sent, and the ones behind the quit stay owed"
    );
    drop_owed(&pool, &writing).await;
    retire(&pool, &writing).await;
}
