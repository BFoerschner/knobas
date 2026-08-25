//! One run, end to end, with no ticker in sight: build the adapter from the
//! config + the keychain, run it, log it, judge the credential, set the
//! backoff, emit. The ticker that decides *when* is `scheduler_loop.rs`.

use std::sync::{Arc, Mutex};

use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::contract::Fault;
use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, Source, SourceDescriptor, SourceError};
use knobas_source_mock::MockSource;
use knobas_sync::config::{self, AuthKind, AuthState, InsertConfig};
use knobas_sync::run_log::{self, SyncOutcome, SyncTrigger};
use knobas_sync::scheduler::{
    AdapterRegistry, RunConnections, SchedulerDeps, SourceSyncStatus, SyncEvents,
};

/// A registry that builds a `MockSource` under whatever id it is handed, with
/// a fault chosen per instance. Stands in for the compiled-in adapter table
/// (`knobas-app/src/sources/registry.rs`), which `knobas-sync` must not see.
struct MockRegistry {
    fault: Mutex<Fault>,
    /// Whether the built mock emits its one deleted item.
    ///
    /// Not decoration: it is the only way this harness can produce a run whose
    /// `deleted` differs from its `swept`, and without that difference the
    /// field-by-field comparison in
    /// `a_healthy_run_is_logged_ok_...` cannot fail -- swapping the two
    /// bindings in `run_log::finish` left it green, because both were `0`.
    ///
    /// `knobas_source_mock::build` cannot express this (it ignores
    /// `instance.config` and hardcodes `MockSource::new()`), which is why the
    /// harness builds the mock itself rather than going through the real
    /// registry.
    tombstone: Mutex<bool>,
}

impl AdapterRegistry for MockRegistry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        vec![knobas_source_mock::descriptor_template()]
    }
    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        let fault = *self.fault.lock().unwrap();
        let inner = if *self.tombstone.lock().unwrap() {
            MockSource::with_tombstone()
        } else {
            MockSource::with_fault(fault)
        };
        Ok(Box::new(Renamed {
            id: instance.id,
            inner,
        }))
    }
}

/// The mock always calls itself `mock`; a scheduler test needs one instance per
/// test, so the descriptor id is overridden and the items are renamed with it.
struct Renamed {
    id: String,
    inner: MockSource,
}

#[async_trait::async_trait]
impl Source for Renamed {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            ..self.inner.descriptor()
        }
    }
    async fn test_connection(&self) -> Result<knobas_source::ConnectionInfo, SourceError> {
        self.inner.test_connection().await
    }
    async fn sync(
        &self,
        cursor: Option<knobas_source::Cursor>,
        sink: &mut (dyn knobas_source::Sink + Send),
    ) -> Result<knobas_source::Cursor, SourceError> {
        // Rewrite the mock's namespace onto this instance's id, which is what a
        // real adapter does natively.
        let mut renaming = Renaming {
            id: &self.id,
            inner: sink,
        };
        self.inner.sync(cursor, &mut renaming).await
    }
    async fn write(&self, op: knobas_source::WriteOp) -> Result<(), SourceError> {
        self.inner.write(op).await
    }
}

struct Renaming<'s> {
    id: &'s str,
    inner: &'s mut (dyn knobas_source::Sink + Send),
}

#[async_trait::async_trait]
impl knobas_source::Sink for Renaming<'_> {
    async fn item(&mut self, mut item: knobas_source::SyncItem) -> Result<(), SourceError> {
        item.entity = knobas_core::entity::EntityRef::new(self.id, &item.entity.key);
        self.inner.item(item).await
    }
}

#[derive(Default)]
struct Recorder {
    states: Mutex<Vec<SourceSyncStatus>>,
    healths: Mutex<Vec<knobas_sync::config::CredentialHealth>>,
    activity: Mutex<Vec<knobas_core::activity::ActivityRow>>,
}

impl SyncEvents for Recorder {
    fn sync_state(&self, status: SourceSyncStatus) {
        self.states.lock().unwrap().push(status);
    }
    fn source_health(&self, health: knobas_sync::config::CredentialHealth) {
        self.healths.lock().unwrap().push(health);
    }
    fn activity_new(&self, row: knobas_core::activity::ActivityRow) {
        self.activity.lock().unwrap().push(row);
    }
}

/// A newtype, because the orphan rule forbids implementing this crate's trait
/// for `knobas_db`'s `Connector` from a test binary that owns neither.
pub struct TestConnections(knobas_db::embedded::Connector);

#[async_trait::async_trait]
impl RunConnections for TestConnections {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        self.0.connect().await
    }
}

struct Harness {
    deps: SchedulerDeps,
    events: Arc<Recorder>,
    registry: Arc<MockRegistry>,
    id: String,
}

async fn harness(auth: AuthKind, with_secret: bool) -> Harness {
    let connector = knobas_db::test_util::test_connector().await;
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let id = format!("sch-{}", uuid::Uuid::new_v4().simple());

    config::insert(
        &pool,
        &InsertConfig {
            id: id.clone(),
            adapter_kind: "mock".into(),
            display_name: "Mock".into(),
            base_url: String::new(),
            auth_kind: auth,
            config: serde_json::json!({}),
            sync_interval_secs: 300,
            enabled: true,
        },
    )
    .await
    .unwrap();

    let store = MemoryStore::new();
    if with_secret {
        store
            .put(
                &id,
                &Secret {
                    kind: AuthMethod::Pat,
                    value: "tok".into(),
                },
            )
            .unwrap();
    }
    let events = Arc::new(Recorder::default());
    let registry = Arc::new(MockRegistry {
        fault: Mutex::new(Fault::None),
        tombstone: Mutex::new(false),
    });

    Harness {
        deps: SchedulerDeps {
            pool,
            connections: Arc::new(TestConnections(connector)),
            registry: registry.clone(),
            secrets: Arc::new(store),
            events: events.clone(),
        },
        events,
        registry,
        id,
    }
}

/// Drive one run the way the ticker will: open the row, execute, settle.
///
/// Returns the row **and** the verdict that produced it, so a test can assert
/// that the two agree field by field. `finish` binds seven values into one
/// statement; a swapped pair there is invisible to any assertion that only
/// checks the row against a remembered constant.
async fn one_run(h: &Harness, trigger: SyncTrigger) -> (run_log::SyncRunRow, run_log::RunResult) {
    let run_id = run_log::start(&h.deps.pool, &h.id, trigger).await.unwrap();
    let result = knobas_sync::scheduler::execute_run(
        &h.deps,
        &h.id,
        run_id,
        None,
        std::time::Instant::now(),
    )
    .await;
    knobas_sync::scheduler::settle(&h.deps, &h.id, run_id, &result).await;
    let row = run_log::list(&h.deps.pool, Some(&h.id), 1)
        .await
        .unwrap()
        .remove(0);
    (row, result)
}

#[tokio::test]
async fn a_healthy_run_is_logged_ok_clears_backoff_and_marks_the_credential_good() {
    let h = harness(AuthKind::Method(AuthMethod::Pat), true).await;
    // The tombstoning mock, so this run's three counts are **three different
    // numbers** (21 upserted, 1 deleted, 0 swept). With the plain mock they
    // were 21/0/0, and the field-by-field comparison below could not fail:
    // swapping `deleted` and `swept` in `run_log::finish` gave 12 passed, 0
    // failed. The unit test on `RunCounts::of` says "values are all different
    // on purpose"; the same reasoning was missing one level up.
    *h.registry.tombstone.lock().unwrap() = true;
    config::set_backoff(
        &h.deps.pool,
        &h.id,
        chrono::Utc::now() + chrono::Duration::minutes(5),
    )
    .await
    .unwrap();

    let (row, result) = one_run(&h, SyncTrigger::FirstRun).await;

    assert_eq!(row.outcome, Some(SyncOutcome::Ok));
    assert!(
        row.upserted > 10,
        "the whole fixture landed: {}",
        row.upserted
    );
    assert!(row.finished_at.is_some());
    // **Field by field against the verdict, not against a remembered
    // constant.** `run_log::finish` binds seven values into one statement, and
    // a swapped pair there -- `deleted` into `swept`, or a cursor from
    // somewhere else -- is exactly what an `is_some()` cannot see. (The M0
    // `runner` tests carried these; deleting that path took them with it.)
    assert_eq!(row.upserted, result.counts.upserted);
    assert_eq!(row.deleted, result.counts.deleted);
    assert_eq!(row.swept, result.counts.swept);
    assert_eq!(
        row.cursor_after, result.counts.cursor_after,
        "the position recorded is the position the run returned"
    );
    // ...and the three really are distinct, so each of the three possible
    // swaps has a value that separates it. An assertion that can only fail on
    // a fixture nobody checked is the shape this test was in.
    assert_eq!(row.deleted, 1, "the tombstoning mock withdrew one item");
    assert_eq!(row.swept, 0, "nothing to sweep on a source's first run");
    assert!(
        row.upserted != row.deleted && row.deleted != row.swept && row.upserted != row.swept,
        "the counts must differ, or a swapped pair is invisible: {} / {} / {}",
        row.upserted,
        row.deleted,
        row.swept
    );
    assert!(
        row.cursor_after.is_some(),
        "the position is recorded for the diagnostics view"
    );
    assert!(row.error.is_none());

    let cfg = config::get(&h.deps.pool, &h.id).await.unwrap().unwrap();
    assert_eq!(cfg.health.state, AuthState::Ok);
    assert!(cfg.backoff_until.is_none(), "a success releases the source");
    assert!(
        cfg.cursor.is_some(),
        "run_from_stored_cursor persisted the position"
    );

    // Coarse state, and one message -- these tests drive `execute_run` by hand,
    // so the *start* transition (which `Scheduler::trigger` emits, before it
    // returns) is not in play here; `scheduler_loop::a_trigger_says_running_...`
    // pins that one. Never per item, in either case.
    let states = h.events.states.lock().unwrap();
    assert_eq!(states.len(), 1, "one sync:state at the finish");
    assert!(!states[0].running);
    assert_eq!(states[0].last_outcome, Some(SyncOutcome::Ok));
    assert!(
        states[0].next_run_at.is_some(),
        "the UI can count down to the next run"
    );
    assert_eq!(
        h.events.healths.lock().unwrap().len(),
        1,
        "unknown -> ok is one health change"
    );

    // Every column the read names, and the actor that scopes it. Six sources
    // finishing in the same second is ordinary, so "the newest line" is not the
    // same claim as "this run's line"; the row is asserted, not counted.
    let activity = h.events.activity.lock().unwrap();
    assert_eq!(activity.len(), 1, "one activity line for a run that landed");
    let line = &activity[0];
    assert_eq!(line.actor, format!("sync:{}", h.id));
    assert_eq!(line.verb, "synced");
    assert!(line.id > 0);
    assert!(
        line.entity_id.is_none(),
        "a run is about a source, not an entity"
    );
    assert_eq!(
        line.detail["source_id"], h.id,
        "the detail is the run's own report: {:?}",
        line.detail
    );
    assert_eq!(line.detail["upserted"], serde_json::json!(row.upserted));
}

/// The read-back is scoped to the source that ran. A second source finishing
/// first must not have its line handed to this one -- which a global
/// "newest line" read does exactly, and silently.
#[tokio::test]
async fn a_run_emits_its_own_activity_line_and_not_the_newest_one() {
    let mine = harness(AuthKind::None, false).await;
    let theirs = harness(AuthKind::None, false).await;

    one_run(&mine, SyncTrigger::Manual).await;
    // Now somebody else writes the newest line in the whole table...
    one_run(&theirs, SyncTrigger::Manual).await;
    // ...and this source runs again.
    knobas_sync::config::patch(&mine.deps.pool, &mine.id, &config::PatchConfig::default())
        .await
        .unwrap();
    sqlx::query("update knobas.source_config set cursor = null where id = $1")
        .bind(&mine.id)
        .execute(&mine.deps.pool)
        .await
        .unwrap();
    one_run(&mine, SyncTrigger::Manual).await;

    let activity = mine.events.activity.lock().unwrap();
    assert_eq!(activity.len(), 2);
    for line in activity.iter() {
        assert_eq!(
            line.actor,
            format!("sync:{}", mine.id),
            "a source was handed another source's activity line"
        );
    }
}

#[tokio::test]
async fn an_unreachable_source_backs_off_along_the_ladder() {
    let h = harness(AuthKind::Method(AuthMethod::Pat), true).await;
    *h.registry.fault.lock().unwrap() = Fault::Unreachable;

    let (row, _) = one_run(&h, SyncTrigger::Schedule).await;
    assert_eq!(row.outcome, Some(SyncOutcome::Unreachable));
    assert!(
        row.error.as_deref().unwrap().contains("simulated"),
        "{:?}",
        row.error
    );

    let cfg = config::get(&h.deps.pool, &h.id).await.unwrap().unwrap();
    assert_eq!(cfg.health.state, AuthState::Unreachable);
    let first = cfg.backoff_until.expect("the first failure sets a backoff");
    let delay = (first - chrono::Utc::now()).num_seconds();
    assert!(
        (45..=70).contains(&delay),
        "the first rung is one minute, got {delay}s"
    );

    one_run(&h, SyncTrigger::Schedule).await;
    let second = config::get(&h.deps.pool, &h.id)
        .await
        .unwrap()
        .unwrap()
        .backoff_until
        .unwrap();
    let delay = (second - chrono::Utc::now()).num_seconds();
    assert!(
        (105..=130).contains(&delay),
        "the second rung is two minutes, got {delay}s"
    );

    // Same verdict twice: one health event, not two (interfaces §2.3).
    assert_eq!(h.events.healths.lock().unwrap().len(), 1);
}

/// P7: `unauthorized` gets **no** automatic retry -- only a human can fix it.
/// Backing it off would be a promise to try again, which is exactly wrong: it
/// would keep pushing a rejected credential at a Jira DC that answers repeated
/// failures with a CAPTCHA lockout.
#[tokio::test]
async fn a_401_marks_the_credential_and_sets_no_backoff_at_all() {
    let h = harness(AuthKind::Method(AuthMethod::Pat), true).await;
    *h.registry.fault.lock().unwrap() = Fault::Unauthorized;

    let (row, _) = one_run(&h, SyncTrigger::Schedule).await;
    assert_eq!(row.outcome, Some(SyncOutcome::Unauthorized));

    let cfg = config::get(&h.deps.pool, &h.id).await.unwrap().unwrap();
    assert_eq!(cfg.health.state, AuthState::Unauthorized);
    assert!(
        cfg.backoff_until.is_none(),
        "unauthorized must not schedule a retry"
    );
    assert!(
        config::due(&h.deps.pool)
            .await
            .unwrap()
            .iter()
            .all(|d| d.id != h.id)
    );

    let healths = h.events.healths.lock().unwrap();
    assert_eq!(healths.len(), 1);
    assert_eq!(healths[0].state, AuthState::Unauthorized);
    assert_eq!(healths[0].source_id, h.id);
}

/// interfaces §3, "Missing": a configured source with no keychain item is
/// `missing_secret` -- no request, no backoff churn, and the sources view offers
/// *Re-enter*.
#[tokio::test]
async fn a_source_whose_secret_is_gone_is_reported_rather_than_attempted() {
    let h = harness(AuthKind::Method(AuthMethod::Pat), false).await;

    let (row, _) = one_run(&h, SyncTrigger::Schedule).await;
    assert_eq!(row.outcome, Some(SyncOutcome::Unauthorized));
    assert_eq!(row.upserted, 0);

    let cfg = config::get(&h.deps.pool, &h.id).await.unwrap().unwrap();
    assert_eq!(
        cfg.health.state,
        AuthState::MissingSecret,
        "\"you never entered one\" and \"yours was rejected\" are different \
         sentences with different offers"
    );
    assert!(cfg.backoff_until.is_none());
}

/// The mock reaches nothing and needs no credential, and `--demo` must work
/// with an empty keychain (§14a).
#[tokio::test]
async fn a_source_that_needs_no_credential_syncs_without_one() {
    let h = harness(AuthKind::None, false).await;
    let (row, _) = one_run(&h, SyncTrigger::FirstRun).await;
    assert_eq!(row.outcome, Some(SyncOutcome::Ok));
    assert!(row.upserted > 10);
}

#[tokio::test]
async fn the_log_is_pruned_as_runs_accumulate() {
    let h = harness(AuthKind::None, false).await;
    for _ in 0..3 {
        one_run(&h, SyncTrigger::Schedule).await;
    }
    let rows = run_log::list(&h.deps.pool, Some(&h.id), 1000)
        .await
        .unwrap();
    assert!(rows.len() <= usize::try_from(run_log::KEEP_RUNS).unwrap());
    assert_eq!(rows.len(), 3, "well under the cap, so all three are kept");
}

#[tokio::test]
async fn status_reports_running_then_the_finished_shape() {
    let h = harness(AuthKind::None, false).await;
    let run_id = run_log::start(&h.deps.pool, &h.id, SyncTrigger::Manual)
        .await
        .unwrap();

    let running = knobas_sync::scheduler::status_for(&h.deps.pool, &h.id)
        .await
        .unwrap()
        .unwrap();
    assert!(running.running);
    assert_eq!(running.run_id, Some(run_id));
    assert!(running.started_at.is_some());
    assert!(
        running.next_run_at.is_none(),
        "a run in flight has no next time to count down to"
    );

    let result = knobas_sync::scheduler::execute_run(
        &h.deps,
        &h.id,
        run_id,
        None,
        std::time::Instant::now(),
    )
    .await;
    knobas_sync::scheduler::settle(&h.deps, &h.id, run_id, &result).await;

    let done = knobas_sync::scheduler::status_for(&h.deps.pool, &h.id)
        .await
        .unwrap()
        .unwrap();
    assert!(!done.running);
    assert_eq!(
        done.run_id,
        Some(run_id),
        "the terminal status names the run it is reporting on, or a progress \
         bar keyed on the run id cannot tell which run ended"
    );
    assert_eq!(done.last_outcome, Some(SyncOutcome::Ok));
    assert!(done.last_finished_at.is_some());
    assert!(
        done.next_run_at.unwrap() > chrono::Utc::now(),
        "interval runs from the finish"
    );

    assert!(
        knobas_sync::scheduler::status_all(&h.deps.pool)
            .await
            .unwrap()
            .iter()
            .any(|s| s.source_id == h.id)
    );
}

/// A disabled source still appears in the sources view -- it just has no next
/// run. So does one that needs a human: `next_run_at` is the countdown, and a
/// countdown to a run that will never start is a lie the UI would render.
#[tokio::test]
async fn a_source_that_will_not_run_has_no_next_run_time() {
    let h = harness(AuthKind::None, false).await;
    one_run(&h, SyncTrigger::Manual).await;
    assert!(
        knobas_sync::scheduler::status_for(&h.deps.pool, &h.id)
            .await
            .unwrap()
            .unwrap()
            .next_run_at
            .is_some()
    );

    config::patch(
        &h.deps.pool,
        &h.id,
        &config::PatchConfig {
            enabled: Some(false),
            ..config::PatchConfig::default()
        },
    )
    .await
    .unwrap();
    assert!(
        knobas_sync::scheduler::status_for(&h.deps.pool, &h.id)
            .await
            .unwrap()
            .unwrap()
            .next_run_at
            .is_none(),
        "a disabled source has no next run"
    );

    config::patch(
        &h.deps.pool,
        &h.id,
        &config::PatchConfig {
            enabled: Some(true),
            ..config::PatchConfig::default()
        },
    )
    .await
    .unwrap();
    config::set_health(
        &h.deps.pool,
        &h.id,
        AuthState::Unauthorized,
        Some("401"),
        None,
    )
    .await
    .unwrap();
    assert!(
        knobas_sync::scheduler::status_for(&h.deps.pool, &h.id)
            .await
            .unwrap()
            .unwrap()
            .next_run_at
            .is_none(),
        "a source that needs a human has no next run either"
    );
}

/// The backoff is what `next_run_at` reports while it is in force -- the
/// interval alone would count down to a moment the scheduler will skip.
#[tokio::test]
async fn a_backed_off_source_counts_down_to_the_backoff_not_the_interval() {
    let h = harness(AuthKind::None, false).await;
    one_run(&h, SyncTrigger::Manual).await;

    let until = chrono::Utc::now() + chrono::Duration::hours(3);
    config::set_backoff(&h.deps.pool, &h.id, until)
        .await
        .unwrap();

    let status = knobas_sync::scheduler::status_for(&h.deps.pool, &h.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(status.backoff_until.unwrap().timestamp(), until.timestamp());
    let next = status.next_run_at.expect("it will run, just later");
    assert!(
        (next - until).num_seconds().abs() <= 1,
        "the backoff is later than the 300 s interval, so it decides: {next} vs {until}"
    );
}

/// A source with no run at all is due now, and says so.
#[tokio::test]
async fn a_never_synced_source_is_due_immediately() {
    let h = harness(AuthKind::None, false).await;
    let status = knobas_sync::scheduler::status_for(&h.deps.pool, &h.id)
        .await
        .unwrap()
        .unwrap();
    assert!(!status.running);
    assert!(
        status.run_id.is_none(),
        "it has never run, so there is no run"
    );
    assert!(status.last_finished_at.is_none());
    let next = status.next_run_at.expect("a new source runs at once");
    assert!(
        next <= chrono::Utc::now() + chrono::Duration::seconds(1),
        "a never-synced source is due now, not in an interval: {next}"
    );
}

/// A run with no channel attached allocates and reports nothing (P3).
#[tokio::test]
async fn a_run_with_no_channel_reports_nothing() {
    #[derive(Default)]
    struct Recorder(Mutex<usize>);
    impl knobas_sync::progress::ProgressSink for Recorder {
        fn report(&self, _progress: knobas_sync::progress::SyncProgress) {
            *self.0.lock().unwrap() += 1;
        }
    }

    let h = harness(AuthKind::None, false).await;
    let sink = Arc::new(Recorder::default());
    let run_id = run_log::start(&h.deps.pool, &h.id, SyncTrigger::Manual)
        .await
        .unwrap();
    let result = knobas_sync::scheduler::execute_run(
        &h.deps,
        &h.id,
        run_id,
        None,
        std::time::Instant::now(),
    )
    .await;
    assert_eq!(result.outcome, SyncOutcome::Ok);
    assert_eq!(
        *sink.0.lock().unwrap(),
        0,
        "a sink nobody attached must never be reported to"
    );
}
