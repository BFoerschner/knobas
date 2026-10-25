//! What follows a write that lands (#2094): a run of its source that **began
//! after the write reached `sent`**, whichever path sent it -- a submit, the
//! scheduler's flush tick, *Flush now*, *Apply anyway*, an amendment.
//!
//! The source is a trait-level fake that answers each write as the test says
//! (lands, moved, refused, unreachable) and whose sync the test can hold at
//! the door, so "a run already in flight when the write lands" is a state the
//! test sets up rather than a race it hopes to win.
//!
//! Every fixture guards against the scheduler's own tick starting the run, as
//! the story-15 test does: the source syncs once a day and has a run that
//! finished a moment ago, so it is not due. Only the tick test lets the tick
//! go, and it is the flush it wants from it, not a run.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use knobas_app::sources::SourcesState;
use knobas_app::sources::write_queue;
use knobas_core::write_queue::WriteState;
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::{
    AuthMethod, ConnectionInfo, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError,
    WriteOp, WriteReceipt,
};
use knobas_sync::scheduler::{
    AdapterRegistry, RunConnections, SchedulerDeps, SchedulerTiming, SyncEvents,
};
use serde_json::json;
use tokio::sync::Semaphore;

const SOURCE: &str = "landing";
const TICKET: &str = "landing:PAY-1";
const PATIENCE: Duration = Duration::from_secs(20);

/// How the fake answers the next write.
#[derive(Clone, Copy, Debug)]
enum Answer {
    Lands,
    Moved,
    Refused,
    Unreachable,
}

struct Fake {
    answer: Arc<Mutex<Answer>>,
    /// One permit per sync the test lets through.
    door: Arc<Semaphore>,
    syncs_entered: Arc<AtomicUsize>,
}

#[async_trait]
impl Source for Fake {
    async fn view(
        &self,
        _entity: &str,
        _section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, SourceError> {
        Ok(None)
    }

    fn descriptor(&self) -> SourceDescriptor {
        descriptor()
    }

    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        Ok(ConnectionInfo::default())
    }

    async fn sync(
        &self,
        cursor: Option<Cursor>,
        _sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        self.syncs_entered.fetch_add(1, Ordering::SeqCst);
        self.door
            .acquire()
            .await
            .expect("the door is never closed")
            .forget();
        Ok(cursor.unwrap_or_default())
    }

    async fn write(&self, _op: WriteOp) -> Result<WriteReceipt, SourceError> {
        let answer = *self.answer.lock().unwrap();
        match answer {
            Answer::Lands => Ok(WriteReceipt::none()),
            Answer::Moved => Err(SourceError::Moved {
                base: "1".to_owned(),
                current: "2".to_owned(),
            }),
            Answer::Refused => Err(SourceError::protocol("the source said no")),
            Answer::Unreachable => Err(SourceError::Unreachable("no route".to_owned())),
        }
    }

    async fn reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError> {
        Err(SourceError::protocol(format!("no workflow for {entity:?}")))
    }
}

fn descriptor() -> SourceDescriptor {
    SourceDescriptor {
        id: SOURCE.to_owned(),
        adapter_kind: SOURCE.to_owned(),
        name: "Landing".to_owned(),
        capabilities: vec![knobas_source::Capability::Write],
        adapter_version: "0".to_owned(),
        auth_methods: vec![AuthMethod::Pat],
        accepts_account: false,
        min_sync_interval_secs: 60,
        roles: Vec::new(),
        source_actions: Vec::new(),
        clone_from: Vec::new(),
        url_forms: Vec::new(),
        write_ops: vec!["comment".to_owned()],
        entity_kinds: vec![KindInfo {
            id: "ticket".to_owned(),
            label: "Ticket".to_owned(),
            plural: "Tickets".to_owned(),
            monogram: "T".to_owned(),
            full_sync_exhaustive: true,
            refreshes: true,
        }],
        config_schema: json!({"type": "object", "properties": {}}),
        payload_paths: Vec::new(),
    }
}

struct Registry {
    answer: Arc<Mutex<Answer>>,
    door: Arc<Semaphore>,
    syncs_entered: Arc<AtomicUsize>,
}

impl AdapterRegistry for Registry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        vec![descriptor()]
    }

    fn build(
        &self,
        _instance: knobas_source::instance::SourceInstance,
    ) -> Result<Box<dyn Source>, SourceError> {
        Ok(Box::new(Fake {
            answer: Arc::clone(&self.answer),
            door: Arc::clone(&self.door),
            syncs_entered: Arc::clone(&self.syncs_entered),
        }))
    }
}

struct Connections(knobas_db::embedded::Connector);

#[async_trait]
impl RunConnections for Connections {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        self.0.connect().await
    }
}

struct Quiet;

impl SyncEvents for Quiet {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
}

struct Fixture {
    state: SourcesState,
    answer: Arc<Mutex<Answer>>,
    door: Arc<Semaphore>,
    syncs_entered: Arc<AtomicUsize>,
}

impl Fixture {
    fn answer(&self, answer: Answer) {
        *self.answer.lock().unwrap() = answer;
    }
}

/// The tick held off for the whole test: it flushes the queue too, and a send
/// the tick made would pass a test with no send of the path it names.
const HELD_OFF: SchedulerTiming = SchedulerTiming {
    startup_delay: Duration::from_secs(3600),
    tick: Duration::from_secs(3600),
    shutdown_grace: knobas_sync::scheduler::SHUTDOWN_GRACE,
};

/// A scheduler started the way the app starts it
/// (`knobas_app::sources::start_scheduler`), over a scratch database holding
/// one source that is not due: a day's interval and a run that just finished.
/// The door is open unless `door_shut`.
async fn fixture(name: &str, timing: SchedulerTiming, door_shut: bool) -> Fixture {
    let connector = knobas_db::test_util::scratch_database(name).await;
    let pool = connector.pool(4).await.expect("a pool");

    let secrets = Arc::new(MemoryStore::new());
    secrets
        .put(
            &knobas_secrets::KeychainAccount::source(SOURCE),
            &Secret::just(AuthMethod::Pat, "a-token"),
        )
        .expect("the credential is stored");
    knobas_sync::config::insert(
        &pool,
        &knobas_sync::config::InsertConfig {
            id: SOURCE.to_owned(),
            adapter_kind: SOURCE.to_owned(),
            display_name: "Landing".to_owned(),
            base_url: "https://landing.example".to_owned(),
            auth_kind: knobas_sync::config::AuthKind::Method(AuthMethod::Pat),
            config: json!({}),
            sync_interval_secs: 86_400,
            enabled: true,
        },
    )
    .await
    .expect("the source row is written");
    // Without it the tick syncs the source at once: one with no run behind it
    // is due. With it, only a landed write can start a run here.
    sqlx::query(
        "insert into knobas.sync_run (source_id, trigger, started_at, finished_at, outcome)
         values ($1, 'schedule', now(), now(), 'ok')",
    )
    .bind(SOURCE)
    .execute(&pool)
    .await
    .unwrap();

    let answer = Arc::new(Mutex::new(Answer::Lands));
    let door = Arc::new(Semaphore::new(if door_shut { 0 } else { 1_000 }));
    let syncs_entered = Arc::new(AtomicUsize::new(0));
    let registry: Arc<dyn AdapterRegistry> = Arc::new(Registry {
        answer: Arc::clone(&answer),
        door: Arc::clone(&door),
        syncs_entered: Arc::clone(&syncs_entered),
    });
    let scheduler = knobas_app::sources::start_scheduler(
        SchedulerDeps {
            pool: pool.clone(),
            connections: Arc::new(Connections(connector)),
            registry: Arc::clone(&registry),
            secrets: secrets.clone(),
            events: Arc::new(Quiet),
            timing,
            quit: knobas_sync::scheduler::Quit::default(),
        },
        knobas_sync::background::Background::new(),
    )
    .await
    .expect("a scheduler");

    Fixture {
        state: SourcesState {
            pool,
            scheduler,
            secrets,
            registry,
        },
        answer,
        door,
        syncs_entered,
    }
}

fn comment(body: &str) -> serde_json::Value {
    json!({ "Comment": { "entity": TICKET, "body": body } })
}

fn comment_op(body: &str) -> WriteOp {
    serde_json::from_value(comment(body)).expect("a comment op")
}

/// Wait until write `id` is no longer pending, and answer its state.
async fn settled(state: &SourcesState, id: i64) -> WriteState {
    tokio::time::timeout(PATIENCE, async {
        loop {
            let row = knobas_core::write_queue::get(&state.pool, id)
                .await
                .expect("the queue reads")
                .expect("the row is there");
            if row.state != WriteState::Pending {
                return row.state;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("write {id} never settled"))
}

/// Wait for a **finished** run of the source that started after write `id`
/// reached `sent`, and answer its id. A run that began before cannot hold the
/// write, however faithfully its ending is reported.
async fn run_after(state: &SourcesState, id: i64) -> i64 {
    tokio::time::timeout(PATIENCE, async {
        loop {
            let run: Option<i64> = sqlx::query_scalar(
                "select r.id from knobas.sync_run r, knobas.write_queue w
                  where w.id = $2 and w.state = 'sent'
                    and r.source_id = $1 and r.started_at > w.settled_at
                    and r.finished_at is not null
                  order by r.id limit 1",
            )
            .bind(SOURCE)
            .bind(id)
            .fetch_optional(&state.pool)
            .await
            .unwrap();
            if let Some(run) = run {
                return run;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("no run of {SOURCE} began after write {id} landed"))
}

/// Runs of the source other than the one the fixture seeded.
async fn runs(state: &SourcesState) -> i64 {
    sqlx::query_scalar("select count(*) from knobas.sync_run where source_id = $1")
        .bind(SOURCE)
        .fetch_one(&state.pool)
        .await
        .map(|n: i64| n - 1)
        .unwrap()
}

/// Queue a write without sending it: what a write that waited out a down
/// source looks like to the paths that send it later.
async fn queued(state: &SourcesState, body: &str) -> i64 {
    knobas_sync::write_queue::queue(state.scheduler.deps(), SOURCE, comment_op(body))
        .await
        .expect("the write is queued")
        .id
}

/// *Flush now*: the queue panel's send of a write that was waiting.
#[tokio::test(flavor = "multi_thread")]
async fn a_write_sent_by_flush_now_is_followed_by_a_run_that_began_after_it() {
    let f = fixture("after_landing_flush_now", HELD_OFF, false).await;
    let id = queued(&f.state, "flushed by hand").await;

    write_queue::flush_now(&f.state, Some(SOURCE.to_owned()))
        .await
        .expect("the flush answers");

    assert_eq!(settled(&f.state, id).await, WriteState::Sent);
    run_after(&f.state, id).await;
    f.state.scheduler.shutdown().await;
}

/// The scheduler's flush tick: a write that waited goes out on its own, and
/// the read-back follows it rather than waiting a scheduled interval more.
/// The tick runs here, every 50 ms, for its flush; the source is still not
/// due, so the run it is followed by is the landed write's.
#[tokio::test(flavor = "multi_thread")]
async fn a_write_the_flush_tick_sends_is_followed_by_a_run_that_began_after_it() {
    let ticking = SchedulerTiming {
        startup_delay: Duration::from_millis(10),
        tick: Duration::from_millis(50),
        ..HELD_OFF
    };
    let f = fixture("after_landing_tick", ticking, false).await;
    let id = queued(&f.state, "sent on the tick").await;

    assert_eq!(settled(&f.state, id).await, WriteState::Sent);
    run_after(&f.state, id).await;
    f.state.scheduler.shutdown().await;
}

/// *Apply anyway*: a write held because its target moved, released and sent.
#[tokio::test(flavor = "multi_thread")]
async fn a_held_write_sent_by_apply_anyway_is_followed_by_a_run_that_began_after_it() {
    let f = fixture("after_landing_apply_anyway", HELD_OFF, false).await;
    f.answer(Answer::Moved);
    let id = write_queue::submit(&f.state, comment("mine"))
        .await
        .expect("queued")
        .id;
    assert_eq!(settled(&f.state, id).await, WriteState::Held);

    f.answer(Answer::Lands);
    write_queue::apply_anyway(&f.state.scheduler, id)
        .await
        .expect("released and sent");

    assert_eq!(settled(&f.state, id).await, WriteState::Sent);
    run_after(&f.state, id).await;
    f.state.scheduler.shutdown().await;
}

/// *Edit and send*: a refused write, amended and sent.
#[tokio::test(flavor = "multi_thread")]
async fn an_amended_write_is_followed_by_a_run_that_began_after_it() {
    let f = fixture("after_landing_amend", HELD_OFF, false).await;
    f.answer(Answer::Refused);
    let id = write_queue::submit(&f.state, comment("too blunt"))
        .await
        .expect("queued")
        .id;
    assert_eq!(settled(&f.state, id).await, WriteState::Refused);

    f.answer(Answer::Lands);
    write_queue::amend(&f.state.scheduler, id, comment("kinder"))
        .await
        .expect("amended and sent");

    assert_eq!(settled(&f.state, id).await, WriteState::Sent);
    run_after(&f.state, id).await;
    f.state.scheduler.shutdown().await;
}

/// A submit, both ways the app makes one: waited for (start-work, the
/// standup protocol) and in the background (`submit_write`).
#[tokio::test(flavor = "multi_thread")]
async fn a_submitted_write_is_followed_by_a_run_that_began_after_it() {
    let f = fixture("after_landing_submit", HELD_OFF, false).await;

    let waited = write_queue::submit(&f.state, comment("waited for"))
        .await
        .expect("queued")
        .id;
    assert_eq!(settled(&f.state, waited).await, WriteState::Sent);
    run_after(&f.state, waited).await;

    let background = write_queue::submit_in_background(&f.state, comment("in the background"))
        .await
        .expect("queued")
        .id;
    assert_eq!(settled(&f.state, background).await, WriteState::Sent);
    run_after(&f.state, background).await;
    f.state.scheduler.shutdown().await;
}

/// **A run already in flight does not stand in for the one after the write.**
/// It began before the write and cannot hold it. The run is held at the door
/// until the landed write's follow-up has joined it, so the join is a fact;
/// then a second run must start, after the write.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_in_flight_when_a_write_lands_is_followed_by_one_that_began_after_it() {
    let f = fixture("after_landing_in_flight", HELD_OFF, true).await;
    let stale = f
        .state
        .scheduler
        .trigger(SOURCE, knobas_sync::SyncTrigger::Manual, None)
        .await
        .expect("a run starts");
    tokio::time::timeout(PATIENCE, async {
        while f.syncs_entered.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the stale run reaches the source");

    let id = write_queue::submit_in_background(&f.state, comment("during a run"))
        .await
        .expect("queued")
        .id;
    assert_eq!(settled(&f.state, id).await, WriteState::Sent);
    // The follow-up has enrolled in the stale run before the door opens.
    tokio::time::timeout(PATIENCE, async {
        while f.state.scheduler.watching(SOURCE).await == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the landed write's follow-up joins the run in flight");
    f.door.add_permits(1_000);

    let after = run_after(&f.state, id).await;
    assert_ne!(after, stale, "the run in flight began before the write");
    f.state.scheduler.shutdown().await;
}

/// **A write that does not land starts no run**: held (its target moved),
/// refused, or waiting on a source that did not answer. A landed write after
/// the first two is the control: it starts one run and waits it out, so a run
/// either had asked for would be counted beside it. The waiting write goes
/// last, since it holds back every write of its source, and a run it asked for
/// would be on the books within the half second the count waits.
#[tokio::test(flavor = "multi_thread")]
async fn a_write_held_refused_or_waiting_starts_no_run() {
    let f = fixture("after_landing_no_run", HELD_OFF, false).await;
    // Each on a ticket of its own: a held write holds back its ticket's
    // later writes in queue order.
    let on = |ticket: &str| json!({ "Comment": { "entity": ticket, "body": ticket } });
    for (answer, state, ticket) in [
        (Answer::Moved, WriteState::Held, "landing:PAY-1"),
        (Answer::Refused, WriteState::Refused, "landing:PAY-2"),
    ] {
        f.answer(answer);
        let id = write_queue::submit(&f.state, on(ticket))
            .await
            .expect("queued")
            .id;
        assert_eq!(settled(&f.state, id).await, state, "{answer:?}");
    }
    f.answer(Answer::Lands);
    let landed = write_queue::submit(&f.state, on("landing:PAY-3"))
        .await
        .expect("queued")
        .id;
    assert_eq!(settled(&f.state, landed).await, WriteState::Sent);
    run_after(&f.state, landed).await;
    assert_eq!(
        runs(&f.state).await,
        1,
        "only the landed write is followed by a run"
    );

    f.answer(Answer::Unreachable);
    let waiting = write_queue::submit(&f.state, on("landing:PAY-4"))
        .await
        .expect("queued")
        .id;
    let row = knobas_core::write_queue::get(&f.state.pool, waiting)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.state, WriteState::Pending, "it waits");
    assert!(row.wait_reason.is_some(), "on the source: {row:?}");
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(runs(&f.state).await, 1, "a waiting write starts no run");
    f.state.scheduler.shutdown().await;
}
