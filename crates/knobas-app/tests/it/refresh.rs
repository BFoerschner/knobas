//! A **refresh** (#2095, ADR-0031): one item read from its source and written
//! into the mirror under the source's run lock, the first thing that follows a
//! landed write, before the run that `after_landing` already started (#2094).
//!
//! Two sources. A trait-level fake whose sync the test holds at the door and
//! whose read of one item answers as the test says (the item, gone, a failure,
//! not offered), so "a run in flight when the write lands" is a state the test
//! sets up rather than a race it hopes to win. And the mock, through the real
//! registry, for what the mock answers.
//!
//! Every fixture keeps the scheduler's own tick away, as `after_landing.rs`
//! does: a day's interval, a run that just finished, and a tick an hour off.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use knobas_app::sources::SourcesState;
use knobas_app::sources::{refresh, write_queue};
use knobas_core::entity::EntityRef;
use knobas_core::write_queue::WriteState;
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::{
    AuthMethod, Capability, ConnectionInfo, Cursor, KindInfo, RefreshAnswer, Sink, Source,
    SourceDescriptor, SourceError, SyncItem, WriteOp, WriteReceipt,
};
use knobas_sync::refresh::{RefreshError, RefreshOutcome};
use knobas_sync::scheduler::{
    AdapterRegistry, RefreshEnd, RefreshEnded, RunConnections, SchedulerDeps, SchedulerTiming,
    SyncEvents,
};
use serde_json::json;
use tokio::sync::Semaphore;

const SOURCE: &str = "refreshing";
const TICKET: &str = "refreshing:PAY-1";
const BUILD: &str = "refreshing:BUILD-1";
const FILED: &str = "refreshing:PAY-9";
const PATIENCE: Duration = Duration::from_secs(20);

/// How the fake answers a read of one item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reads {
    /// What it holds now; *gone* for an id it does not hold.
    Answers,
    Gone,
    Fails,
    /// Declares no `Capability::Refresh`, and answers `NotOffered`.
    NotOffered,
}

/// The fake's remote system, shared by every adapter the registry builds.
struct Remote {
    /// PAY-1's title as the source holds it. A comment that lands sets it.
    title: Mutex<String>,
    reads: Mutex<Reads>,
    /// Reads of one item the fake answered.
    refreshes: AtomicUsize,
    /// Whether a `CreateTicket` has filed PAY-9.
    filed: Mutex<bool>,
    /// One permit per sync the test lets through.
    door: Semaphore,
    syncs_entered: AtomicUsize,
}

fn ticket(key: &str, title: &str) -> SyncItem {
    SyncItem {
        entity: EntityRef::new(SOURCE, key),
        kind: "ticket".to_owned(),
        title: title.to_owned(),
        body_text: title.to_owned(),
        author: None,
        updated_at: None,
        payload: json!({ "key": key }),
        web_url: None,
        deleted: false,
    }
}

fn build() -> SyncItem {
    SyncItem {
        entity: EntityRef::new(SOURCE, "BUILD-1"),
        kind: "build".to_owned(),
        title: "Build #1 failed".to_owned(),
        body_text: String::new(),
        author: None,
        updated_at: None,
        payload: json!({ "status": "FAILURE" }),
        web_url: None,
        deleted: false,
    }
}

struct Fake(Arc<Remote>);

#[async_trait]
impl Source for Fake {
    fn descriptor(&self) -> SourceDescriptor {
        descriptor(*self.0.reads.lock().unwrap())
    }

    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        Ok(ConnectionInfo::default())
    }

    /// A full sync emits PAY-1 as the source held it **when the run began**
    /// (a run fetches inside its lock, before it writes), then BUILD-1; an
    /// incremental one emits nothing. Held at the door after the fetch.
    async fn sync(
        &self,
        cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        let fetched = self.0.title.lock().unwrap().clone();
        self.0.syncs_entered.fetch_add(1, Ordering::SeqCst);
        self.0
            .door
            .acquire()
            .await
            .expect("the door is never closed")
            .forget();
        if cursor.is_none() {
            sink.item(ticket("PAY-1", &fetched)).await?;
            sink.item(build()).await?;
        }
        Ok("1".to_owned())
    }

    async fn write(&self, op: WriteOp) -> Result<WriteReceipt, SourceError> {
        match op {
            WriteOp::CreateTicket { .. } => {
                *self.0.filed.lock().unwrap() = true;
                Ok(WriteReceipt::id("PAY-9"))
            }
            _ => {
                *self.0.title.lock().unwrap() = "after".to_owned();
                Ok(WriteReceipt::none())
            }
        }
    }

    async fn refresh(&self, entity: &str) -> Result<RefreshAnswer, SourceError> {
        self.0.refreshes.fetch_add(1, Ordering::SeqCst);
        match *self.0.reads.lock().unwrap() {
            Reads::NotOffered => Ok(RefreshAnswer::NotOffered),
            Reads::Fails => Err(SourceError::Unreachable("no route".to_owned())),
            Reads::Gone => Ok(RefreshAnswer::Gone),
            Reads::Answers => Ok(match entity {
                TICKET => RefreshAnswer::Item(ticket("PAY-1", &self.0.title.lock().unwrap())),
                BUILD => RefreshAnswer::Item(build()),
                FILED if *self.0.filed.lock().unwrap() => {
                    RefreshAnswer::Item(ticket("PAY-9", "Build #1 failed"))
                }
                _ => RefreshAnswer::Gone,
            }),
        }
    }

    async fn reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError> {
        Err(SourceError::protocol(format!("no workflow for {entity:?}")))
    }

    async fn view(
        &self,
        _entity: &str,
        _section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, SourceError> {
        Ok(None)
    }
}

fn kind(id: &str) -> KindInfo {
    KindInfo {
        id: id.to_owned(),
        label: id.to_owned(),
        plural: format!("{id}s"),
        monogram: "T".to_owned(),
        full_sync_exhaustive: true,
    }
}

fn descriptor(reads: Reads) -> SourceDescriptor {
    SourceDescriptor {
        id: SOURCE.to_owned(),
        adapter_kind: SOURCE.to_owned(),
        name: "Refreshing".to_owned(),
        capabilities: if reads == Reads::NotOffered {
            vec![Capability::Write]
        } else {
            vec![Capability::Write, Capability::Refresh]
        },
        adapter_version: "0".to_owned(),
        auth_methods: vec![AuthMethod::Pat],
        accepts_account: false,
        min_sync_interval_secs: 60,
        roles: Vec::new(),
        source_actions: Vec::new(),
        clone_from: Vec::new(),
        url_forms: Vec::new(),
        write_ops: vec!["comment".to_owned(), "create_ticket".to_owned()],
        entity_kinds: vec![kind("ticket"), kind("build")],
        config_schema: json!({"type": "object", "properties": {}}),
        payload_paths: Vec::new(),
    }
}

struct Registry(Arc<Remote>);

impl AdapterRegistry for Registry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        vec![descriptor(*self.0.reads.lock().unwrap())]
    }

    fn build(
        &self,
        _instance: knobas_source::instance::SourceInstance,
    ) -> Result<Box<dyn Source>, SourceError> {
        Ok(Box::new(Fake(Arc::clone(&self.0))))
    }
}

struct Connections(knobas_db::embedded::Connector);

#[async_trait]
impl RunConnections for Connections {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        self.0.connect().await
    }
}

/// The events the scheduler hands on, the refreshes' kept.
#[derive(Clone, Default)]
struct Heard(Arc<Mutex<Vec<RefreshEnded>>>);

impl SyncEvents for Heard {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
    fn refresh_ended(&self, ended: &RefreshEnded) {
        self.0.lock().unwrap().push(ended.clone());
    }
}

const HELD_OFF: SchedulerTiming = SchedulerTiming {
    startup_delay: Duration::from_secs(3600),
    tick: Duration::from_secs(3600),
    shutdown_grace: knobas_sync::scheduler::SHUTDOWN_GRACE,
};

struct Fixture {
    state: SourcesState,
    remote: Arc<Remote>,
    heard: Heard,
}

impl Fixture {
    fn reads(&self, reads: Reads) {
        *self.remote.reads.lock().unwrap() = reads;
    }

    fn open_the_door(&self) {
        self.remote.door.add_permits(1_000);
    }

    fn heard(&self) -> Vec<RefreshEnded> {
        self.heard.0.lock().unwrap().clone()
    }
}

/// A scheduler started the way the app starts it, over a scratch database
/// holding the fake source, not due, its door shut.
async fn fixture(name: &str) -> Fixture {
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
            display_name: "Refreshing".to_owned(),
            base_url: "https://refreshing.example".to_owned(),
            auth_kind: knobas_sync::config::AuthKind::Method(AuthMethod::Pat),
            config: json!({}),
            sync_interval_secs: 86_400,
            enabled: true,
        },
    )
    .await
    .expect("the source row is written");
    not_due(&pool, SOURCE).await;

    let remote = Arc::new(Remote {
        title: Mutex::new("before".to_owned()),
        reads: Mutex::new(Reads::Answers),
        refreshes: AtomicUsize::new(0),
        filed: Mutex::new(false),
        door: Semaphore::new(0),
        syncs_entered: AtomicUsize::new(0),
    });
    let registry: Arc<dyn AdapterRegistry> = Arc::new(Registry(Arc::clone(&remote)));
    let heard = Heard::default();
    let scheduler = knobas_app::sources::start_scheduler(
        SchedulerDeps {
            pool: pool.clone(),
            connections: Arc::new(Connections(connector)),
            registry: Arc::clone(&registry),
            secrets: secrets.clone(),
            events: Arc::new(heard.clone()),
            timing: HELD_OFF,
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
        remote,
        heard,
    }
}

/// A run of `source` that finished a moment ago, so the tick finds it not due.
async fn not_due(pool: &sqlx::PgPool, source: &str) {
    sqlx::query(
        "insert into knobas.sync_run (source_id, trigger, started_at, finished_at, outcome)
         values ($1, 'schedule', now(), now(), 'ok')",
    )
    .bind(source)
    .execute(pool)
    .await
    .unwrap();
}

/// Runs of `source` that have finished.
async fn finished_runs(pool: &sqlx::PgPool, source: &str) -> i64 {
    sqlx::query_scalar(
        "select count(*) from knobas.sync_run where source_id = $1 and finished_at is not null",
    )
    .bind(source)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Poll `check` until it holds, or fail naming `what`.
async fn until<F, Fut>(what: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    tokio::time::timeout(PATIENCE, async {
        while !check().await {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("never: {what}"));
}

/// One full sync, let through the door and waited out: the mirror holds
/// PAY-1 and BUILD-1, and the stored cursor is `1`.
async fn mirrored(f: &Fixture) {
    let before = finished_runs(&f.state.pool, SOURCE).await;
    f.remote.door.add_permits(1);
    f.state
        .scheduler
        .trigger(SOURCE, knobas_sync::SyncTrigger::Manual, None)
        .await
        .expect("a run starts");
    until("the first sync finishes", || async {
        finished_runs(&f.state.pool, SOURCE).await > before
    })
    .await;
}

async fn settled(state: &SourcesState, id: i64) -> WriteState {
    tokio::time::timeout(PATIENCE, async {
        loop {
            let row = knobas_core::write_queue::get(&state.pool, id)
                .await
                .unwrap()
                .unwrap();
            if row.state != WriteState::Pending {
                return row.state;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("write {id} never settled"))
}

/// The mirror's title for `entity`, read through `sync.live_item`: `None`
/// for an item it does not hold or holds tombstoned.
async fn live_title(pool: &sqlx::PgPool, entity: &str) -> Option<String> {
    sqlx::query_scalar("select title from sync.live_item where entity_id = $1")
        .bind(entity)
        .fetch_optional(pool)
        .await
        .unwrap()
}

/// Wait for a finished run of `source` that started after write `id` was
/// sent: the follow-up run of #2094.
async fn run_after(pool: &sqlx::PgPool, source: &str, id: i64) {
    until("a run that began after the write finishes", || async {
        sqlx::query_scalar::<_, i64>(
            "select r.id from knobas.sync_run r, knobas.write_queue w
              where w.id = $2 and w.state = 'sent'
                and r.source_id = $1 and r.started_at > w.settled_at
                and r.finished_at is not null
              limit 1",
        )
        .bind(source)
        .bind(id)
        .fetch_optional(pool)
        .await
        .unwrap()
        .is_some()
    })
    .await;
}

fn comment() -> serde_json::Value {
    json!({ "Comment": { "entity": TICKET, "body": "landed" } })
}

/// **A refresh waits behind a run in flight, and that run's older payload
/// does not overwrite it.** The run is held at the door having fetched PAY-1
/// as `before`; the comment lands and the source now holds `after`. The
/// refresh must not read until the run commits, and what it reads then is
/// what the mirror keeps: the run's commit cannot put `before` back.
#[tokio::test(flavor = "multi_thread")]
async fn a_refresh_waits_behind_a_run_in_flight_whose_older_payload_does_not_overwrite_it() {
    let f = fixture("refresh_behind_a_run").await;
    f.state
        .scheduler
        .trigger(SOURCE, knobas_sync::SyncTrigger::Manual, None)
        .await
        .expect("a run starts");
    until("the run in flight has fetched", || async {
        f.remote.syncs_entered.load(Ordering::SeqCst) == 1
    })
    .await;

    let id = write_queue::submit_in_background(&f.state, comment())
        .await
        .expect("queued")
        .id;
    assert_eq!(settled(&f.state, id).await, WriteState::Sent);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        f.remote.refreshes.load(Ordering::SeqCst),
        0,
        "the refresh reads only once the run in flight has committed"
    );

    f.open_the_door();
    run_after(&f.state.pool, SOURCE, id).await;
    assert_eq!(f.remote.refreshes.load(Ordering::SeqCst), 1);
    assert_eq!(
        live_title(&f.state.pool, TICKET).await.as_deref(),
        Some("after"),
        "the run that began before the write committed `before`; the refresh read after it"
    );
    assert_eq!(
        f.heard(),
        [RefreshEnded {
            entity_id: TICKET.to_owned(),
            write_id: Some(id),
            ended: RefreshEnd::Refreshed,
        }],
        "the shell is told which item, after which write"
    );
    f.state.scheduler.shutdown().await;
}

/// A refresh that fails is only a failed read: the write stays `sent`, the
/// run after it still starts, and the shell hears `failed`.
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_refresh_leaves_the_write_sent_and_the_run_still_starts() {
    let f = fixture("refresh_fails").await;
    f.open_the_door();
    f.reads(Reads::Fails);
    let id = write_queue::submit_in_background(&f.state, comment())
        .await
        .expect("queued")
        .id;
    assert_eq!(settled(&f.state, id).await, WriteState::Sent);
    run_after(&f.state.pool, SOURCE, id).await;
    let row = knobas_core::write_queue::get(&f.state.pool, id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.state, WriteState::Sent, "{row:?}");
    assert_eq!(row.detail, None, "the write row is not touched: {row:?}");
    assert_eq!(f.remote.refreshes.load(Ordering::SeqCst), 1);
    assert_eq!(
        f.heard(),
        [RefreshEnded {
            entity_id: TICKET.to_owned(),
            write_id: Some(id),
            ended: RefreshEnd::Failed,
        }]
    );
    f.state.scheduler.shutdown().await;
}

/// A refresh is no run: it writes no "synced" activity line and leaves the
/// stored cursor where it was, while the item is written.
#[tokio::test(flavor = "multi_thread")]
async fn a_refresh_writes_no_activity_line_and_leaves_the_cursor() {
    let f = fixture("refresh_no_run").await;
    sqlx::query("update knobas.source_config set cursor = 'c-7' where id = $1")
        .bind(SOURCE)
        .execute(&f.state.pool)
        .await
        .unwrap();

    let outcome = refresh::refresh(&f.state.scheduler, TICKET, None).await;
    assert!(
        matches!(outcome, Ok(RefreshOutcome::Refreshed)),
        "{outcome:?}"
    );
    assert_eq!(
        live_title(&f.state.pool, TICKET).await.as_deref(),
        Some("before")
    );

    let cursor: Option<String> =
        sqlx::query_scalar("select cursor from knobas.source_config where id = $1")
            .bind(SOURCE)
            .fetch_one(&f.state.pool)
            .await
            .unwrap();
    assert_eq!(cursor.as_deref(), Some("c-7"));
    let lines: i64 = sqlx::query_scalar(
        "select count(*) from knobas.activity where actor = $1 and verb = 'synced'",
    )
    .bind(format!("sync:{SOURCE}"))
    .fetch_one(&f.state.pool)
    .await
    .unwrap();
    assert_eq!(lines, 0, "a refresh is not a sync");
    assert_eq!(
        f.heard(),
        [RefreshEnded {
            entity_id: TICKET.to_owned(),
            write_id: None,
            ended: RefreshEnd::Refreshed,
        }]
    );
    f.state.scheduler.shutdown().await;
}

/// *Gone* tombstones the item, and an item tombstoned that the source answers
/// again is live again.
#[tokio::test(flavor = "multi_thread")]
async fn gone_tombstones_the_item_and_an_answer_brings_it_back() {
    let f = fixture("refresh_gone").await;
    let refreshed = |_: ()| refresh::refresh(&f.state.scheduler, TICKET, None);

    assert!(matches!(refreshed(()).await, Ok(RefreshOutcome::Refreshed)));
    assert!(live_title(&f.state.pool, TICKET).await.is_some());

    f.reads(Reads::Gone);
    assert!(matches!(refreshed(()).await, Ok(RefreshOutcome::Gone)));
    assert_eq!(live_title(&f.state.pool, TICKET).await, None, "tombstoned");
    let tombstoned: bool =
        sqlx::query_scalar("select deleted_at is not null from knobas.entity where id = $1")
            .bind(TICKET)
            .fetch_one(&f.state.pool)
            .await
            .unwrap();
    assert!(tombstoned);

    f.reads(Reads::Answers);
    assert!(matches!(refreshed(()).await, Ok(RefreshOutcome::Refreshed)));
    assert_eq!(
        live_title(&f.state.pool, TICKET).await.as_deref(),
        Some("before"),
        "answered again, live again"
    );
    let ends: Vec<RefreshEnd> = f.heard().into_iter().map(|e| e.ended).collect();
    assert_eq!(
        ends,
        [
            RefreshEnd::Refreshed,
            RefreshEnd::Gone,
            RefreshEnd::Refreshed
        ]
    );
    f.state.scheduler.shutdown().await;
}

/// A turned-off source's item is refused before anything is read, and
/// nothing is written or told.
#[tokio::test(flavor = "multi_thread")]
async fn a_turned_off_sources_item_is_refused_and_nothing_is_read() {
    let f = fixture("refresh_turned_off").await;
    sqlx::query("update knobas.source_config set enabled = false where id = $1")
        .bind(SOURCE)
        .execute(&f.state.pool)
        .await
        .unwrap();

    let outcome = refresh::refresh(&f.state.scheduler, TICKET, None).await;
    assert!(
        matches!(&outcome, Err(RefreshError::TurnedOff(id)) if id == SOURCE),
        "{outcome:?}"
    );
    assert_eq!(f.remote.refreshes.load(Ordering::SeqCst), 0, "nothing read");
    let held: i64 = sqlx::query_scalar("select count(*) from sync.item where entity_id = $1")
        .bind(TICKET)
        .fetch_one(&f.state.pool)
        .await
        .unwrap();
    assert_eq!(held, 0, "nothing written");
    assert!(f.heard().is_empty(), "a refusal is no refresh that ended");
    f.state.scheduler.shutdown().await;
}

/// **Links on arrival settle from a refresh.** A ticket filed from a failed
/// run is linked to it before the run that follows the write ends: that run
/// is held at the door, and the link is already drawn.
#[tokio::test(flavor = "multi_thread")]
async fn a_ticket_filed_from_a_run_is_linked_to_it_before_the_follow_up_run_ends() {
    let f = fixture("refresh_links").await;
    mirrored(&f).await;
    let runs_before = finished_runs(&f.state.pool, SOURCE).await;

    // Queued with its link owed and then sent by *Flush now*, so the one
    // thing after the send is `after_landing`: a submit that waits for a run
    // of its own (`submit_linked_in_background`) can start that run first,
    // and a refresh waits behind a run in flight.
    let filed = WriteOp::CreateTicket {
        entity: BUILD.to_owned(),
        title: "Build #1 failed".to_owned(),
        body: "filed from the run".to_owned(),
        ticket_type: "Bug".to_owned(),
    };
    let id = knobas_sync::write_queue::queue(f.state.scheduler.deps(), SOURCE, filed)
        .await
        .expect("queued")
        .id;
    write_queue::hold_arrival_link(&f.state.pool, id, BUILD)
        .await
        .expect("the link is owed");
    write_queue::flush_now(&f.state, Some(SOURCE.to_owned()))
        .await
        .expect("sent");
    assert_eq!(settled(&f.state, id).await, WriteState::Sent);
    // The door stays shut, so no run of the source can end while this waits:
    // the link can only come from the refresh.
    until("the filed ticket is linked to its run", || async {
        sqlx::query_scalar::<_, i64>(
            "select count(*) from knobas.link
              where (from_id = $1 and to_id = $2) or (from_id = $2 and to_id = $1)",
        )
        .bind(FILED)
        .bind(BUILD)
        .fetch_one(&f.state.pool)
        .await
        .unwrap()
            == 1
    })
    .await;
    // The link is written before `refresh_ended` is told, so a loaded machine
    // can see the link with the event still to come.
    until("the shell is told the refresh ended", || async {
        !f.heard().is_empty()
    })
    .await;
    assert_eq!(
        finished_runs(&f.state.pool, SOURCE).await,
        runs_before,
        "and the follow-up run has not ended"
    );
    assert_eq!(
        f.heard()
            .into_iter()
            .map(|e| (e.entity_id, e.write_id))
            .collect::<Vec<_>>(),
        [(FILED.to_owned(), Some(id))],
        "a create is refreshed by the id its receipt names"
    );
    f.open_the_door();
    run_after(&f.state.pool, SOURCE, id).await;
    f.state.scheduler.shutdown().await;
}

// -- the mock, through the real registry --------------------------------------

/// Two mock instances over one scratch database, neither due: `mockread`
/// reads one item and files tickets, `mocknoread` offers the run alone.
async fn mock_fixture(name: &str) -> (SourcesState, Heard) {
    scheduled(
        name,
        knobas_app::sources::Registry::for_process(),
        "mock",
        &[
            ("mockread", json!({ "files_tickets": true })),
            ("mocknoread", json!({ "refresh": false })),
        ],
    )
    .await
}

/// A scheduler started the way the app starts it over `registry`, with a
/// source of `kind` per `(id, config)` in `sources`, none of them due.
async fn scheduled(
    name: &str,
    registry: knobas_app::sources::Registry,
    kind: &str,
    sources: &[(&str, serde_json::Value)],
) -> (SourcesState, Heard) {
    let connector = knobas_db::test_util::scratch_database(name).await;
    let pool = connector.pool(4).await.expect("a pool");
    for (id, config) in sources {
        knobas_sync::config::insert(
            &pool,
            &knobas_sync::config::InsertConfig {
                id: (*id).to_owned(),
                adapter_kind: kind.to_owned(),
                display_name: (*id).to_owned(),
                base_url: String::new(),
                auth_kind: knobas_sync::config::AuthKind::None,
                config: config.clone(),
                sync_interval_secs: 86_400,
                enabled: true,
            },
        )
        .await
        .expect("the source row is written");
        not_due(&pool, id).await;
    }
    let registry: Arc<dyn AdapterRegistry> = Arc::new(registry);
    let secrets: Arc<dyn SecretStore> = Arc::new(MemoryStore::new());
    let heard = Heard::default();
    let scheduler = knobas_app::sources::start_scheduler(
        SchedulerDeps {
            pool: pool.clone(),
            connections: Arc::new(Connections(connector)),
            registry: Arc::clone(&registry),
            secrets: Arc::clone(&secrets),
            events: Arc::new(heard.clone()),
            timing: HELD_OFF,
            quit: knobas_sync::scheduler::Quit::default(),
        },
        knobas_sync::background::Background::new(),
    )
    .await
    .expect("a scheduler");
    (
        SourcesState {
            pool,
            scheduler,
            secrets,
            registry,
        },
        heard,
    )
}

/// When `entity` was last written into the mirror, and when the first run of
/// its source that began after write `id` was sent started: the refresh's row
/// is written before that run starts, a run's is not.
async fn written_and_run_started(
    pool: &sqlx::PgPool,
    entity: &str,
    id: i64,
) -> (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) {
    sqlx::query_as(
        "select i.synced_at, r.started_at
           from sync.item i, knobas.write_queue w, knobas.sync_run r
          where i.entity_id = $1 and w.id = $2
            and r.source_id = w.source_id and r.started_at > w.settled_at
          order by r.started_at limit 1",
    )
    .bind(entity)
    .bind(id)
    .fetch_one(pool)
    .await
    .expect("the item is mirrored and a run followed the write")
}

/// **Mock source: a landed write's item is in the mirror as the mock answers
/// it before any run of the source has started**, a comment's by its target
/// and a create's by the id its receipt names.
#[tokio::test(flavor = "multi_thread")]
async fn a_landed_writes_item_is_mirrored_from_the_mock_before_any_run_starts() {
    let (state, heard) = mock_fixture("refresh_mock").await;
    // At the mock's own position, so the run after each write is incremental
    // and writes nothing over what the refresh wrote.
    sqlx::query("update knobas.source_config set cursor = 'tidewater-v4' where id = 'mockread'")
        .execute(&state.pool)
        .await
        .unwrap();

    let commented = write_queue::submit_in_background(
        &state,
        json!({ "Comment": { "entity": "mockread:PAY-231", "body": "landed" } }),
    )
    .await
    .expect("queued")
    .id;
    assert_eq!(settled(&state, commented).await, WriteState::Sent);
    run_after(&state.pool, "mockread", commented).await;
    let (written, run_started) =
        written_and_run_started(&state.pool, "mockread:PAY-231", commented).await;
    assert!(
        written < run_started,
        "the refresh wrote it ({written}) before the run began ({run_started})"
    );
    assert_eq!(
        live_title(&state.pool, "mockread:PAY-231").await.as_deref(),
        Some("Retry failed SEPA payouts"),
        "as the mock holds it"
    );

    let filed = write_queue::submit_in_background(
        &state,
        json!({ "CreateTicket": {
            "entity": "mockread:PAY-231",
            "title": "Payout retries stall",
            "body": "filed",
            "ticket_type": "Bug",
        } }),
    )
    .await
    .expect("queued")
    .id;
    assert_eq!(settled(&state, filed).await, WriteState::Sent);
    run_after(&state.pool, "mockread", filed).await;
    let made: String =
        sqlx::query_scalar("select 'mockread:' || remote_id from knobas.write_queue where id = $1")
            .bind(filed)
            .fetch_one(&state.pool)
            .await
            .unwrap();
    let (written, run_started) = written_and_run_started(&state.pool, &made, filed).await;
    assert!(
        written < run_started,
        "{made}: {written} before {run_started}"
    );
    assert_eq!(
        live_title(&state.pool, &made).await.as_deref(),
        Some("Payout retries stall")
    );
    assert_eq!(
        heard
            .0
            .lock()
            .unwrap()
            .iter()
            .map(|e| (e.entity_id.clone(), e.write_id))
            .collect::<Vec<_>>(),
        [
            ("mockread:PAY-231".to_owned(), Some(commented)),
            (made.clone(), Some(filed)),
        ]
    );
    state.scheduler.shutdown().await;
}

/// **A mock that does not offer the read gets the run alone**, as after
/// #2094. At the mock's own position, so the run writes nothing: an item in
/// the mirror afterwards could only be a refresh's.
#[tokio::test(flavor = "multi_thread")]
async fn a_mock_that_does_not_offer_the_read_gets_the_run_alone() {
    let (state, heard) = mock_fixture("refresh_mock_unoffered").await;
    sqlx::query("update knobas.source_config set cursor = 'tidewater-v4' where id = 'mocknoread'")
        .execute(&state.pool)
        .await
        .unwrap();
    let id = write_queue::submit_in_background(
        &state,
        json!({ "Comment": { "entity": "mocknoread:PAY-231", "body": "landed" } }),
    )
    .await
    .expect("queued")
    .id;
    assert_eq!(settled(&state, id).await, WriteState::Sent);
    run_after(&state.pool, "mocknoread", id).await;
    let held: i64 = sqlx::query_scalar("select count(*) from sync.item where source_id = $1")
        .bind("mocknoread")
        .fetch_one(&state.pool)
        .await
        .unwrap();
    assert_eq!(held, 0, "nothing was refreshed");
    assert!(heard.0.lock().unwrap().is_empty(), "no refresh happened");
    state.scheduler.shutdown().await;
}

// -- the WASM fixture, an external adapter (#2101) ------------------------------

/// One source of the fixture component at `artifact`, `surveys`, at the
/// fixture's own position, so the run after a write writes nothing: an item
/// in the mirror afterwards could only be a refresh's.
async fn wasm_fixture(name: &str, artifact: &std::path::Path) -> (SourcesState, Heard) {
    let adapter = knobas_wasm_host::WasmAdapter::load(artifact).unwrap_or_else(|e| {
        panic!(
            "the fixture at {} does not load ({e}); `just wasm-fixture` builds it",
            artifact.display()
        )
    });
    let registry = knobas_app::sources::Registry::default();
    registry.add(adapter).expect("the fixture's kind is free");
    let fixture = scheduled(name, registry, "wasm-fixture", &[("surveys", json!({}))]).await;
    sqlx::query("update knobas.source_config set cursor = '5' where id = 'surveys'")
        .execute(&fixture.0.pool)
        .await
        .unwrap();
    fixture
}

/// The fixture's one write, on its first survey.
fn book_repair() -> serde_json::Value {
    json!({ "SourceAction": {
        "entity": "surveys:survey-1",
        "id": "book_repair",
        "fields": { "day": "Tuesday" },
    } })
}

async fn held(pool: &sqlx::PgPool) -> Vec<String> {
    sqlx::query_scalar("select entity_id from sync.item where source_id = 'surveys'")
        .fetch_all(pool)
        .await
        .unwrap()
}

/// **A 1.4.0 component's landed write is followed by its refresh**: the
/// survey is read across the boundary and mirrored before the run starts.
#[tokio::test(flavor = "multi_thread")]
async fn a_landed_write_on_an_external_source_is_refreshed_across_the_boundary() {
    let (state, heard) = wasm_fixture(
        "refresh_wasm_offered",
        &knobas_wasm_host::fixture_component(),
    )
    .await;
    let id = write_queue::submit_in_background(&state, book_repair())
        .await
        .expect("queued")
        .id;
    assert_eq!(settled(&state, id).await, WriteState::Sent);
    run_after(&state.pool, "surveys", id).await;
    let (written, run_started) = written_and_run_started(&state.pool, "surveys:survey-1", id).await;
    assert!(
        written < run_started,
        "the refresh wrote it ({written}) before the run began ({run_started})"
    );
    assert_eq!(
        live_title(&state.pool, "surveys:survey-1").await.as_deref(),
        Some("Quay crane 4 inspection")
    );
    assert_eq!(
        heard
            .0
            .lock()
            .unwrap()
            .iter()
            .map(|e| (e.entity_id.clone(), e.write_id, e.ended))
            .collect::<Vec<_>>(),
        [(
            "surveys:survey-1".to_owned(),
            Some(id),
            RefreshEnd::Refreshed
        )]
    );
    state.scheduler.shutdown().await;
}

/// **A component built against 1.3.0, the minor before `refresh`, gets the
/// run alone** after a landed write: nothing is read by id, and the run that
/// follows the write is all that happens.
#[tokio::test(flavor = "multi_thread")]
async fn a_landed_write_on_a_1_3_0_external_source_gets_the_run_alone() {
    let (state, heard) = wasm_fixture(
        "refresh_wasm_unoffered",
        &knobas_wasm_host::fixture_component_v1_3(),
    )
    .await;
    let id = write_queue::submit_in_background(&state, book_repair())
        .await
        .expect("queued")
        .id;
    assert_eq!(settled(&state, id).await, WriteState::Sent);
    run_after(&state.pool, "surveys", id).await;
    assert_eq!(
        held(&state.pool).await,
        Vec::<String>::new(),
        "nothing was refreshed"
    );
    assert!(heard.0.lock().unwrap().is_empty(), "no refresh happened");
    state.scheduler.shutdown().await;
}
