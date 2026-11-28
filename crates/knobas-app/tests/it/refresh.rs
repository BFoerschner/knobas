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
use knobas_sync::refresh::{Lock, RefreshError, RefreshOutcome};
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
/// A run a `TriggerBuild` puts on the queue, whose state the test moves.
const RUN: &str = "refreshing:BUILD-2";
/// The pipeline it runs.
const PIPELINE: &str = "refreshing:CONF-1";
/// A parent its source reads with what stands under it (ADR-0047).
const REPO: &str = "refreshing:REPO-1";
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
/// A run of BUILD-2's chain at the source: its state (`None` once the source
/// no longer has it) and the keys it waits on in turn.
type ChainRun = (Option<&'static str>, Vec<String>);

struct Remote {
    /// PAY-1's title as the source holds it. A comment that lands sets it.
    title: Mutex<String>,
    reads: Mutex<Reads>,
    /// Reads of one item the fake answered.
    refreshes: AtomicUsize,
    /// Each read of one item, by entity, as the fake was asked it.
    read_at: Mutex<Vec<(String, std::time::Instant)>>,
    /// Whether a `CreateTicket` has filed PAY-9.
    filed: Mutex<bool>,
    /// One permit per sync the test lets through.
    door: Semaphore,
    syncs_entered: AtomicUsize,
    /// The positions each walk under a parent was handed, in order.
    walks: Mutex<Vec<Option<String>>>,
    /// Whether a walk under REPO-1 finds something (PAY-1 and REPO-1 as the
    /// source holds them, and a position moved past the one handed in), or
    /// nothing, as on a source unchanged since the run.
    walk_finds: Mutex<bool>,
    /// The keys a walk under REPO-1 answers as deleted: an item under the
    /// parent the source dropped, or the parent itself.
    walk_deleted: Mutex<Vec<&'static str>>,
    /// Whether the walk leaves the position where it was though it wrote
    /// rows, as a TeamCity build configuration's walk does (#2564).
    walk_keeps_position: Mutex<bool>,
    /// BUILD-2 as the source holds it: `None` until a trigger queues it, then
    /// `queued`, `running` or `finished`.
    run_state: Mutex<Option<&'static str>>,
    /// The keys of the runs BUILD-2 waits on (#2594): its snapshot
    /// dependencies, queued by the same trigger.
    waits: Mutex<Vec<String>>,
    /// Those runs as the source holds them, by key: each one's state (`None`
    /// once the source no longer has it) and the keys it waits on in turn.
    chain: Mutex<std::collections::BTreeMap<String, ChainRun>>,
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

/// The run `key` in `state` (BUILD-2 is the one a trigger queues): a
/// finished one carries its `finishDate`, which the build kind declares as
/// its `finished` instant, waiting on `waits` (#2594): written beside the
/// record as `waitsOn`, as the TeamCity adapter writes a build's snapshot
/// dependencies, and left out where it waits on nothing.
fn chain_run(key: &str, state: &str, waits: &[String]) -> SyncItem {
    let mut payload = json!({ "state": state });
    if state == "finished" {
        payload["finishDate"] = json!("20261125T100000+0000");
    }
    if !waits.is_empty() {
        payload["waitsOn"] = json!(waits);
    }
    SyncItem {
        entity: EntityRef::new(SOURCE, key),
        kind: "build".to_owned(),
        title: "Build #2".to_owned(),
        body_text: String::new(),
        author: None,
        updated_at: None,
        payload,
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
        payload: json!({ "status": "FAILURE", "finishDate": "20261124T100000+0000" }),
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
            WriteOp::TriggerBuild { .. } => {
                *self.0.run_state.lock().unwrap() = Some("queued");
                Ok(WriteReceipt::id("BUILD-2"))
            }
            _ => {
                *self.0.title.lock().unwrap() = "after".to_owned();
                Ok(WriteReceipt::none())
            }
        }
    }

    async fn refresh(&self, entity: &str) -> Result<RefreshAnswer, SourceError> {
        self.0.refreshes.fetch_add(1, Ordering::SeqCst);
        self.0
            .read_at
            .lock()
            .unwrap()
            .push((entity.to_owned(), std::time::Instant::now()));
        match *self.0.reads.lock().unwrap() {
            Reads::NotOffered => Ok(RefreshAnswer::NotOffered),
            Reads::Fails => Err(SourceError::Unreachable("no route".to_owned())),
            Reads::Gone => Ok(RefreshAnswer::Gone),
            Reads::Answers => Ok(match entity {
                TICKET => RefreshAnswer::Item(ticket("PAY-1", &self.0.title.lock().unwrap())),
                BUILD => RefreshAnswer::Item(build()),
                RUN => match *self.0.run_state.lock().unwrap() {
                    Some(state) => RefreshAnswer::Item(chain_run(
                        "BUILD-2",
                        state,
                        &self.0.waits.lock().unwrap(),
                    )),
                    None => RefreshAnswer::Gone,
                },
                other if other.starts_with("refreshing:BUILD-") => {
                    let key = other.trim_start_matches("refreshing:");
                    match self.0.chain.lock().unwrap().get(key) {
                        Some((Some(state), waits)) => {
                            RefreshAnswer::Item(chain_run(key, state, waits))
                        }
                        _ => RefreshAnswer::Gone,
                    }
                }
                REPO => RefreshAnswer::NotOffered,
                FILED if *self.0.filed.lock().unwrap() => {
                    RefreshAnswer::Item(ticket("PAY-9", "Build #1 failed"))
                }
                _ => RefreshAnswer::Gone,
            }),
        }
    }

    async fn refresh_under(
        &self,
        entity: &str,
        cursor: Option<&str>,
    ) -> Result<knobas_source::UnderAnswer, SourceError> {
        if entity != REPO {
            return Ok(knobas_source::UnderAnswer::NotOffered);
        }
        self.0.walks.lock().unwrap().push(cursor.map(str::to_owned));
        if !*self.0.walk_finds.lock().unwrap() {
            return Ok(knobas_source::UnderAnswer::Walked {
                items: Vec::new(),
                cursor: None,
            });
        }
        let mut repo = ticket("REPO-1", "payout-service");
        repo.kind = "repo".to_owned();
        let deleted = self.0.walk_deleted.lock().unwrap().clone();
        repo.deleted = deleted.contains(&"REPO-1");
        let mut items = vec![repo, ticket("PAY-1", &self.0.title.lock().unwrap())];
        for key in deleted.into_iter().filter(|key| *key != "REPO-1") {
            items.push(SyncItem {
                deleted: true,
                ..ticket(key, "dropped")
            });
        }
        let keeps = *self.0.walk_keeps_position.lock().unwrap();
        Ok(knobas_source::UnderAnswer::Walked {
            items,
            cursor: (!keeps).then(|| format!("{} walked", cursor.unwrap_or("none"))),
        })
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
        refreshes: true,
        title_stays_in_mirror: false,
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
        web_sign_in: None,
        web_links: None,
        code_pages: None,
        by_hand_by_default: false,
        passwords: None,
        write_ops: vec![
            "comment".to_owned(),
            "create_ticket".to_owned(),
            "trigger_build".to_owned(),
        ],
        entity_kinds: vec![
            kind("ticket"),
            kind("build"),
            kind("repo"),
            kind("build_config"),
        ],
        config_schema: json!({"type": "object", "properties": {}}),
        // A build is a run, finished once its `finishDate` is there: what
        // the follow of a started run reads (the TeamCity run fix).
        payload_paths: vec![knobas_source::KindPaths {
            kind: "build".to_owned(),
            frame: Some(knobas_source::Frame::Run),
            finished: vec![knobas_source::PayloadPath::of(["finishDate"])],
            // The runs a build waits on, by key (#2594).
            waits_on: vec![knobas_source::ListPath {
                at: knobas_source::PayloadPath::of(["waitsOn"]),
                entry: knobas_source::PayloadPath::default(),
            }],
            ..knobas_source::KindPaths::default()
        }],
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
            sync_by_hand: false,
        },
    )
    .await
    .expect("the source row is written");
    not_due(&pool, SOURCE).await;

    let remote = Arc::new(Remote {
        title: Mutex::new("before".to_owned()),
        reads: Mutex::new(Reads::Answers),
        refreshes: AtomicUsize::new(0),
        read_at: Mutex::new(Vec::new()),
        filed: Mutex::new(false),
        door: Semaphore::new(0),
        syncs_entered: AtomicUsize::new(0),
        walks: Mutex::new(Vec::new()),
        walk_finds: Mutex::new(true),
        walk_keeps_position: Mutex::new(false),
        walk_deleted: Mutex::new(Vec::new()),
        run_state: Mutex::new(None),
        waits: Mutex::new(Vec::new()),
        chain: Mutex::new(std::collections::BTreeMap::new()),
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
            live: Default::default(),
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

/// A run of `trigger` of its own, not the one [`mirrored`] just finished:
/// that run's row closes a moment before its watchers do, and a trigger in
/// between joins it.
async fn a_run_of_its_own(f: &Fixture, trigger: knobas_sync::SyncTrigger) {
    let last: i64 = sqlx::query_scalar("select max(id) from knobas.sync_run where source_id = $1")
        .bind(SOURCE)
        .fetch_one(&f.state.pool)
        .await
        .unwrap();
    tokio::time::timeout(PATIENCE, async {
        while f
            .state
            .scheduler
            .trigger(SOURCE, trigger, None)
            .await
            .expect("a run starts")
            == last
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("a run of its own starts");
}

/// **A scheduled run stands aside for a landed write's read-back** (ADR-0031
/// as amended 2026-11-24, the pr-state fix). The source holds a position, so
/// the run is incremental; it is held at the door, the lock in its hands. The
/// write lands: its read-back must not wait for the run, which is stopped,
/// rolled back and logged as standing aside, and the run after the write
/// starts again from the same position.
#[tokio::test(flavor = "multi_thread")]
async fn a_landed_writes_read_back_stops_a_scheduled_run_in_its_way() {
    let f = fixture("refresh_stands_aside").await;
    mirrored(&f).await;
    let entered = f.remote.syncs_entered.load(Ordering::SeqCst);
    a_run_of_its_own(&f, knobas_sync::SyncTrigger::Schedule).await;
    until("the scheduled run holds the lock", || async {
        f.remote.syncs_entered.load(Ordering::SeqCst) > entered
    })
    .await;

    let id = write_queue::submit_in_background(&f.state, comment())
        .await
        .expect("queued")
        .id;
    assert_eq!(settled(&f.state, id).await, WriteState::Sent);
    until("the read-back goes first, the door still shut", || async {
        live_title(&f.state.pool, TICKET).await.as_deref() == Some("after")
    })
    .await;
    assert_eq!(f.remote.refreshes.load(Ordering::SeqCst), 1);
    let aside: Option<String> = sqlx::query_scalar(
        "select error from knobas.sync_run where source_id = $1 and trigger = 'schedule'
            and finished_at is not null and error like 'stood aside%'",
    )
    .bind(SOURCE)
    .fetch_optional(&f.state.pool)
    .await
    .unwrap();
    assert_eq!(
        aside,
        Some(format!("{}{id}", knobas_sync::run_log::STOOD_ASIDE_PREFIX)),
        "the stopped run's row says which write it stood aside for"
    );
    assert_eq!(
        knobas_sync::run_log::failures_since_last_ok(&f.state.pool, SOURCE)
            .await
            .unwrap(),
        0,
        "standing aside is no failure of the source"
    );

    f.open_the_door();
    run_after(&f.state.pool, SOURCE, id).await;
    f.state.scheduler.shutdown().await;
}

/// A run a person pressed is not stopped: the read-back waits for it, as
/// before (Fable's ruling 1).
#[tokio::test(flavor = "multi_thread")]
async fn a_landed_writes_read_back_waits_for_a_run_a_person_pressed() {
    let f = fixture("refresh_waits_for_a_person").await;
    mirrored(&f).await;
    let entered = f.remote.syncs_entered.load(Ordering::SeqCst);
    a_run_of_its_own(&f, knobas_sync::SyncTrigger::Manual).await;
    until("the pressed run holds the lock", || async {
        f.remote.syncs_entered.load(Ordering::SeqCst) > entered
    })
    .await;
    let id = write_queue::submit_in_background(&f.state, comment())
        .await
        .expect("queued")
        .id;
    assert_eq!(settled(&f.state, id).await, WriteState::Sent);
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        f.remote.refreshes.load(Ordering::SeqCst),
        0,
        "the read-back waits for the pressed run"
    );
    f.open_the_door();
    run_after(&f.state.pool, SOURCE, id).await;
    assert_eq!(f.remote.refreshes.load(Ordering::SeqCst), 1);
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

    let outcome = refresh::refresh(&f.state.scheduler, TICKET, None, Lock::Wait).await;
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
    let refreshed = |_: ()| refresh::refresh(&f.state.scheduler, TICKET, None, Lock::Wait);

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

    let outcome = refresh::refresh(&f.state.scheduler, TICKET, None, Lock::Wait).await;
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

    // Sent the way *File ticket* sends it, its link owed on the row: the
    // caller waits for the landed write's follow-up rather than starting a
    // run of its own, so the refresh is the first thing under the run lock
    // (#2115).
    let id = write_queue::submit_linked_in_background(&f.state, filed(), BUILD.to_owned())
        .await
        .expect("queued")
        .id;
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

/// *File ticket* from the failed build: what the fake files as PAY-9.
fn filed() -> serde_json::Value {
    json!({ "CreateTicket": {
        "entity": BUILD,
        "title": "Build #1 failed",
        "body": "filed from the run",
        "ticket_type": "Bug",
    } })
}

/// **A send whose caller waits for the mirror starts no run of its own**
/// (#2115). The landed write's follow-up is the refresh, then one run that
/// began after the write; the caller waits for that and then looks. A caller
/// that started its own run beside it was a second run, and one the refresh
/// could find in flight and wait behind.
#[tokio::test(flavor = "multi_thread")]
async fn a_linked_send_waits_for_the_landed_writes_follow_up_and_starts_no_run_of_its_own() {
    let f = fixture("refresh_linked_one_run").await;
    mirrored(&f).await;
    f.open_the_door();
    let id = write_queue::submit_linked_in_background(&f.state, filed(), BUILD.to_owned())
        .await
        .expect("queued")
        .id;
    assert_eq!(settled(&f.state, id).await, WriteState::Sent);
    run_after(&f.state.pool, SOURCE, id).await;
    until("the send and everything after it are over", || async {
        f.state.scheduler.background().running() == 0
    })
    .await;
    let after: i64 = sqlx::query_scalar(
        "select count(*) from knobas.sync_run r, knobas.write_queue w
          where w.id = $2 and r.source_id = $1 and r.started_at > w.settled_at",
    )
    .bind(SOURCE)
    .bind(id)
    .fetch_one(&f.state.pool)
    .await
    .unwrap();
    assert_eq!(after, 1, "one run after the write: the follow-up's");
    assert_eq!(
        f.heard()
            .into_iter()
            .map(|e| (e.entity_id, e.write_id))
            .collect::<Vec<_>>(),
        [(FILED.to_owned(), Some(id))],
        "the filed ticket was refreshed"
    );
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
                sync_by_hand: false,
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
            live: Default::default(),
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
    sqlx::query("update knobas.source_config set cursor = 'tidewater-v8' where id = 'mockread'")
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
    sqlx::query("update knobas.source_config set cursor = 'tidewater-v8' where id = 'mocknoread'")
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

/// **The reader's *Refresh* does not wait behind a run** (the query audit of
/// PR #2479, 2026-11-12). A run holds its source's lock for the whole run,
/// network included, and the button said *Refreshing…* all that while. With
/// the lock held, the reader's refresh answers *a run is in flight* at once:
/// nothing is read from the source, nothing written, nobody told. Once the
/// run commits, the same press refreshes the item.
#[tokio::test(flavor = "multi_thread")]
async fn the_readers_refresh_says_a_run_is_in_flight_instead_of_waiting() {
    let f = fixture("refresh_run_in_flight").await;
    let mut run = f.state.pool.begin().await.unwrap();
    sqlx::query("select pg_advisory_xact_lock(hashtext($1::text))")
        .bind(SOURCE)
        .execute(&mut *run)
        .await
        .unwrap();

    let asked = std::time::Instant::now();
    let answer = refresh::of_reader(&f.state.scheduler, TICKET).await;
    assert!(
        matches!(answer, Ok(refresh::EntityRefresh::RunInFlight)),
        "{answer:?}"
    );
    assert!(
        asked.elapsed() < std::time::Duration::from_secs(5),
        "answered without waiting for the run: {:?}",
        asked.elapsed()
    );
    assert_eq!(f.remote.refreshes.load(Ordering::SeqCst), 0, "nothing read");
    assert_eq!(
        live_title(&f.state.pool, TICKET).await,
        None,
        "nothing written"
    );
    assert!(
        f.heard().is_empty(),
        "a refresh held off is no refresh that ended"
    );

    run.rollback().await.unwrap();
    let again = refresh::of_reader(&f.state.scheduler, TICKET).await;
    assert!(
        matches!(again, Ok(refresh::EntityRefresh::Refreshed)),
        "{again:?}"
    );
    assert_eq!(
        live_title(&f.state.pool, TICKET).await.as_deref(),
        Some("before")
    );
    f.state.scheduler.shutdown().await;
}

/// The stored position, as a run or a walk left it.
async fn stored_cursor(pool: &sqlx::PgPool) -> Option<String> {
    sqlx::query_scalar("select cursor from knobas.source_config where id = $1")
        .bind(SOURCE)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// ADR-0047: a parent `refresh` does not read is walked with what stands
/// under it, from the stored position read inside the lock, and what the walk
/// found is written with the parent's position moved, in one transaction. No
/// run, no activity line, and the reader is told it refreshed.
#[tokio::test(flavor = "multi_thread")]
async fn a_refresh_under_a_parent_writes_what_it_walked_and_moves_the_position_with_it() {
    let f = fixture("refresh_under_walks").await;
    sqlx::query("update knobas.source_config set cursor = 'c-7' where id = $1")
        .bind(SOURCE)
        .execute(&f.state.pool)
        .await
        .unwrap();
    *f.remote.title.lock().unwrap() = "pushed".to_owned();

    let answer = refresh::of_reader(&f.state.scheduler, REPO).await;
    assert!(
        matches!(answer, Ok(refresh::EntityRefresh::Refreshed)),
        "{answer:?}"
    );
    assert_eq!(*f.remote.walks.lock().unwrap(), [Some("c-7".to_owned())]);
    assert_eq!(
        live_title(&f.state.pool, REPO).await.as_deref(),
        Some("payout-service")
    );
    assert_eq!(
        live_title(&f.state.pool, TICKET).await.as_deref(),
        Some("pushed"),
        "what stands under the parent is written too"
    );
    assert_eq!(
        stored_cursor(&f.state.pool).await.as_deref(),
        Some("c-7 walked"),
        "the parent's position moves with its rows"
    );
    assert_eq!(finished_runs(&f.state.pool, SOURCE).await, 1, "no run");
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
            entity_id: REPO.to_owned(),
            write_id: None,
            ended: RefreshEnd::Refreshed,
        }]
    );
    f.state.scheduler.shutdown().await;
}

/// A walk that wrote rows and answered no position (a TeamCity build
/// configuration's newest builds, #2564, ADR-0047 as amended 2026-11-27)
/// writes them and leaves the stored position byte for byte: the next run
/// reads from where it stood.
#[tokio::test(flavor = "multi_thread")]
async fn a_walk_that_answers_no_position_writes_its_rows_and_leaves_the_position() {
    let f = fixture("refresh_under_keeps").await;
    sqlx::query("update knobas.source_config set cursor = 'c-7' where id = $1")
        .bind(SOURCE)
        .execute(&f.state.pool)
        .await
        .unwrap();
    *f.remote.title.lock().unwrap() = "started at the source".to_owned();
    *f.remote.walk_keeps_position.lock().unwrap() = true;

    let answer = refresh::of_reader(&f.state.scheduler, REPO).await;
    assert!(
        matches!(answer, Ok(refresh::EntityRefresh::Refreshed)),
        "{answer:?}"
    );
    assert_eq!(
        live_title(&f.state.pool, TICKET).await.as_deref(),
        Some("started at the source"),
        "what the walk found is written"
    );
    assert_eq!(stored_cursor(&f.state.pool).await.as_deref(), Some("c-7"));
    assert_eq!(finished_runs(&f.state.pool, SOURCE).await, 1, "no run");
    f.state.scheduler.shutdown().await;
}

/// A walk says the parent is gone only when it answers the parent itself
/// deleted: an item under it the source dropped is written as gone, and the
/// parent still refreshed (the final suite's mutants: the test turned around,
/// or widened to any deleted item, went unseen).
#[tokio::test(flavor = "multi_thread")]
async fn a_walk_is_gone_only_when_it_answers_the_parent_deleted() {
    let f = fixture("refresh_under_gone").await;
    *f.remote.walk_deleted.lock().unwrap() = vec!["PAY-2"];
    let answer = refresh::of_reader(&f.state.scheduler, REPO).await;
    assert!(
        matches!(answer, Ok(refresh::EntityRefresh::Refreshed)),
        "a dropped child is not the parent gone: {answer:?}"
    );
    *f.remote.walk_deleted.lock().unwrap() = vec!["REPO-1"];
    let answer = refresh::of_reader(&f.state.scheduler, REPO).await;
    assert!(
        matches!(answer, Ok(refresh::EntityRefresh::Gone)),
        "the parent answered deleted: {answer:?}"
    );
    f.state.scheduler.shutdown().await;
}

/// A walk that found nothing leaves the position byte for byte, as an idle
/// run does, and still says it refreshed. It tells the shell nothing (the
/// pr-state fix, 2026-11-24): the repository page walks its repository each
/// time it is shown, and a walk with no news must not drop every answer the
/// window holds.
#[tokio::test(flavor = "multi_thread")]
async fn a_walk_that_found_nothing_leaves_the_position() {
    let f = fixture("refresh_under_idle").await;
    sqlx::query("update knobas.source_config set cursor = 'c-7' where id = $1")
        .bind(SOURCE)
        .execute(&f.state.pool)
        .await
        .unwrap();
    *f.remote.walk_finds.lock().unwrap() = false;

    let answer = refresh::of_reader(&f.state.scheduler, REPO).await;
    assert!(
        matches!(answer, Ok(refresh::EntityRefresh::Refreshed)),
        "{answer:?}"
    );
    assert_eq!(stored_cursor(&f.state.pool).await.as_deref(), Some("c-7"));
    assert_eq!(
        live_title(&f.state.pool, REPO).await,
        None,
        "nothing written"
    );
    assert!(
        f.heard().is_empty(),
        "no item:refreshed for a walk with no news"
    );
    f.state.scheduler.shutdown().await;
}

/// Never beside a run (Fable's ruling 4): the walk is not asked while a run
/// of the source holds its lock, and the reader is told the run is in flight.
#[tokio::test(flavor = "multi_thread")]
async fn a_walk_is_not_asked_while_a_run_holds_the_source() {
    let f = fixture("refresh_under_in_flight").await;
    let mut run = f.state.pool.begin().await.unwrap();
    sqlx::query("select pg_advisory_xact_lock(hashtext($1::text))")
        .bind(SOURCE)
        .execute(&mut *run)
        .await
        .unwrap();

    let answer = refresh::of_reader(&f.state.scheduler, REPO).await;
    assert!(
        matches!(answer, Ok(refresh::EntityRefresh::RunInFlight)),
        "{answer:?}"
    );
    assert!(f.remote.walks.lock().unwrap().is_empty(), "nothing walked");
    run.rollback().await.unwrap();
    f.state.scheduler.shutdown().await;
}

/// The refreshes heard for `entity` with no write behind them.
fn heard_after(f: &Fixture, entity: &str) -> Vec<RefreshEnd> {
    f.heard()
        .into_iter()
        .filter(|ended| ended.entity_id == entity && ended.write_id.is_none())
        .map(|ended| ended.ended)
        .collect()
}

/// The state the mirror holds BUILD-2 in.
async fn mirrored_state(pool: &sqlx::PgPool) -> Option<String> {
    sqlx::query_scalar("select payload->>'state' from sync.live_item where entity_id = $1")
        .bind(RUN)
        .fetch_optional(pool)
        .await
        .unwrap()
}

/// **A run started in knobas moves on in the mirror as it moves at the
/// source** (the TeamCity run fix, 2026-11-25): queued at the read-back, then
/// running and finished within a read of each, with no run of the source;
/// then nothing reads it again.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_started_here_is_read_on_until_it_finishes() {
    let f = fixture("refresh_follow_run").await;
    mirrored(&f).await;
    f.open_the_door();
    let id = write_queue::submit_in_background(
        &f.state,
        json!({ "TriggerBuild": { "entity": PIPELINE } }),
    )
    .await
    .expect("queued")
    .id;
    assert_eq!(settled(&f.state, id).await, WriteState::Sent);
    until("the read-back holds the queued run", || async {
        mirrored_state(&f.state.pool).await.as_deref() == Some("queued")
    })
    .await;
    until("the run is followed", || async {
        knobas_app::sources::follow_run::followed(&f.state.scheduler, SOURCE) == [RUN]
    })
    .await;

    // The agent takes it: the next read, 2 s on, says running.
    *f.remote.run_state.lock().unwrap() = Some("running");
    let moved = std::time::Instant::now();
    until("the mirror says running", || async {
        mirrored_state(&f.state.pool).await.as_deref() == Some("running")
    })
    .await;
    assert!(
        moved.elapsed() < Duration::from_secs(4),
        "{:?}",
        moved.elapsed()
    );
    // The row and the event are two effects of one read: under load the event lands after the row (#2538).
    until("the run's refresh is heard", || async {
        !heard_after(&f, RUN).is_empty()
    })
    .await;
    assert_eq!(heard_after(&f, RUN), [RefreshEnd::Refreshed]);

    *f.remote.run_state.lock().unwrap() = Some("finished");
    until("the mirror says finished", || async {
        mirrored_state(&f.state.pool).await.as_deref() == Some("finished")
    })
    .await;
    until("the follow ends", || async {
        knobas_app::sources::follow_run::followed(&f.state.scheduler, SOURCE).is_empty()
    })
    .await;
    let reads = f.remote.refreshes.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(
        f.remote.refreshes.load(Ordering::SeqCst),
        reads,
        "a finished run is read no more"
    );
    f.state.scheduler.shutdown().await;
}

/// **A followed run that has not moved tells nothing**: each read finds it
/// as the mirror holds it (`Unchanged`), so the window keeps its answers and
/// no page reads again while a build waits for an agent.
#[tokio::test(flavor = "multi_thread")]
async fn a_followed_run_that_has_not_moved_tells_nothing() {
    let f = fixture("refresh_follow_quiet").await;
    mirrored(&f).await;
    f.open_the_door();
    let id = write_queue::submit_in_background(
        &f.state,
        json!({ "TriggerBuild": { "entity": PIPELINE } }),
    )
    .await
    .expect("queued")
    .id;
    assert_eq!(settled(&f.state, id).await, WriteState::Sent);
    until("the run is followed", || async {
        knobas_app::sources::follow_run::followed(&f.state.scheduler, SOURCE) == [RUN]
    })
    .await;
    let reads = f.remote.refreshes.load(Ordering::SeqCst);
    until("two more reads of the queued run", || async {
        f.remote.refreshes.load(Ordering::SeqCst) >= reads + 2
    })
    .await;
    assert_eq!(heard_after(&f, RUN), [], "an unchanged read tells nothing");
    assert_eq!(
        knobas_app::sources::follow_run::followed(&f.state.scheduler, SOURCE),
        [RUN],
        "still followed while it waits"
    );

    // Gone at the source (deleted from the queue): the follow ends.
    *f.remote.run_state.lock().unwrap() = None;
    until("the follow ends", || async {
        knobas_app::sources::follow_run::followed(&f.state.scheduler, SOURCE).is_empty()
    })
    .await;
    f.state.scheduler.shutdown().await;
}

/// **A one-item refresh that finds the item as the mirror holds it is
/// `Unchanged`** and tells nothing; one that finds it moved is `Refreshed`.
#[tokio::test(flavor = "multi_thread")]
async fn a_refresh_of_an_unmoved_item_is_unchanged_and_tells_nothing() {
    let f = fixture("refresh_unchanged").await;
    let refreshed = |_: ()| refresh::refresh(&f.state.scheduler, TICKET, None, Lock::Wait);
    assert!(matches!(refreshed(()).await, Ok(RefreshOutcome::Refreshed)));
    assert!(matches!(refreshed(()).await, Ok(RefreshOutcome::Unchanged)));
    *f.remote.title.lock().unwrap() = "after".to_owned();
    assert!(matches!(refreshed(()).await, Ok(RefreshOutcome::Refreshed)));
    assert_eq!(
        f.heard().len(),
        2,
        "the unchanged read told nothing: {:?}",
        f.heard()
    );
    f.state.scheduler.shutdown().await;
}

/// The state the mirror holds the run `entity` in.
async fn state_of(pool: &sqlx::PgPool, entity: &str) -> Option<String> {
    sqlx::query_scalar("select payload->>'state' from sync.live_item where entity_id = $1")
        .bind(entity)
        .fetch_optional(pool)
        .await
        .unwrap()
}

/// Move the run `key` of BUILD-2's chain to `state` at the source.
fn chain_moves(f: &Fixture, key: &str, state: &'static str) {
    f.remote
        .chain
        .lock()
        .unwrap()
        .get_mut(key)
        .expect("a run of the chain")
        .0 = Some(state);
}

fn followed(f: &Fixture) -> Vec<String> {
    knobas_app::sources::follow_run::followed(&f.state.scheduler, SOURCE)
}

/// Trigger BUILD-2 and wait until it is sent.
async fn trigger_the_run(f: &Fixture) {
    let id = write_queue::submit_in_background(
        &f.state,
        json!({ "TriggerBuild": { "entity": PIPELINE } }),
    )
    .await
    .expect("queued")
    .id;
    assert_eq!(settled(&f.state, id).await, WriteState::Sent);
}

/// **A run started here brings the runs it waits on** (#2594): BUILD-2's
/// trigger queued BUILD-3, which waits on BUILD-4. Each is read by id from
/// the started run's own record and moves on in the mirror within a read of
/// moving at the source, queued to running to finished, with no run of the
/// source reading it (its incremental runs emit nothing); each follow ends
/// with its run, and the started run's goes on until it finishes.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_started_here_brings_the_runs_it_waits_on_to_their_end() {
    let f = fixture("refresh_follow_chain").await;
    mirrored(&f).await;
    f.open_the_door();
    *f.remote.waits.lock().unwrap() = vec!["BUILD-3".to_owned()];
    f.remote.chain.lock().unwrap().extend([
        (
            "BUILD-3".to_owned(),
            (Some("queued"), vec!["BUILD-4".to_owned()]),
        ),
        ("BUILD-4".to_owned(), (Some("queued"), Vec::new())),
    ]);
    trigger_the_run(&f).await;
    let (dep, deeper) = ("refreshing:BUILD-3", "refreshing:BUILD-4");
    until("the run and its chain are followed", || async {
        followed(&f) == [RUN, dep, deeper]
    })
    .await;
    until(
        "the mirror holds both queued, each read at once",
        || async {
            state_of(&f.state.pool, dep).await.as_deref() == Some("queued")
                && state_of(&f.state.pool, deeper).await.as_deref() == Some("queued")
        },
    )
    .await;

    // The first step runs and finishes: read within a read of each.
    chain_moves(&f, "BUILD-4", "running");
    let moved = std::time::Instant::now();
    until("the mirror says BUILD-4 runs", || async {
        state_of(&f.state.pool, deeper).await.as_deref() == Some("running")
    })
    .await;
    assert!(
        moved.elapsed() < Duration::from_secs(4),
        "{:?}",
        moved.elapsed()
    );
    chain_moves(&f, "BUILD-4", "finished");
    until("BUILD-4's follow ends with it", || async {
        followed(&f) == [RUN, dep]
    })
    .await;
    assert_eq!(
        state_of(&f.state.pool, deeper).await.as_deref(),
        Some("finished")
    );

    chain_moves(&f, "BUILD-3", "running");
    until("the mirror says BUILD-3 runs", || async {
        state_of(&f.state.pool, dep).await.as_deref() == Some("running")
    })
    .await;
    chain_moves(&f, "BUILD-3", "finished");
    until("BUILD-3's follow ends with it", || async {
        followed(&f) == [RUN]
    })
    .await;
    assert_eq!(
        state_of(&f.state.pool, dep).await.as_deref(),
        Some("finished")
    );

    // A finished run of the chain is not brought again by the next reads.
    let reads = f.remote.refreshes.load(Ordering::SeqCst);
    until("two more reads of the started run", || async {
        f.remote.refreshes.load(Ordering::SeqCst) >= reads + 2
    })
    .await;
    assert_eq!(followed(&f), [RUN]);

    *f.remote.run_state.lock().unwrap() = Some("finished");
    until("the follow ends", || async { followed(&f).is_empty() }).await;
    f.state.scheduler.shutdown().await;
}

/// **The follow's bounds hold for a chain** (#2594): the started run waits
/// on more runs than [`PER_SOURCE`] leaves room for. At most that many are
/// followed at once, the started run is never stopped by its own chain, and
/// each one left out is brought once a slot frees, until every one has been
/// read to its end.
#[tokio::test(flavor = "multi_thread")]
async fn a_chain_longer_than_the_cap_is_followed_a_slot_at_a_time_behind_the_started_run() {
    use knobas_app::sources::follow_run::PER_SOURCE;
    let f = fixture("refresh_follow_long_chain").await;
    mirrored(&f).await;
    f.open_the_door();
    let keys: Vec<String> = (11..11 + PER_SOURCE + 2)
        .map(|n| format!("BUILD-{n}"))
        .collect();
    *f.remote.waits.lock().unwrap() = keys.clone();
    f.remote.chain.lock().unwrap().extend(
        keys.iter()
            .map(|key| (key.clone(), (Some("queued"), Vec::new()))),
    );
    trigger_the_run(&f).await;
    until("the cap is full", || async {
        followed(&f).len() == PER_SOURCE
    })
    .await;
    assert_eq!(
        followed(&f)[0],
        RUN,
        "the started run first: {:?}",
        followed(&f)
    );

    for key in &keys {
        let entity = format!("{SOURCE}:{key}");
        until(&format!("{key} is followed"), || async {
            followed(&f).contains(&entity)
        })
        .await;
        let now = followed(&f);
        assert!(now.len() <= PER_SOURCE, "{now:?}");
        assert_eq!(now[0], RUN, "the started run goes on: {now:?}");
        chain_moves(&f, key, "finished");
        until(&format!("{key} is read finished"), || async {
            state_of(&f.state.pool, &entity).await.as_deref() == Some("finished")
        })
        .await;
    }
    until("only the started run is left", || async {
        followed(&f) == [RUN]
    })
    .await;
    for key in &keys {
        assert_eq!(
            state_of(&f.state.pool, &format!("{SOURCE}:{key}"))
                .await
                .as_deref(),
            Some("finished"),
            "{key}"
        );
    }

    *f.remote.run_state.lock().unwrap() = Some("finished");
    until("the follow ends", || async { followed(&f).is_empty() }).await;
    f.state.scheduler.shutdown().await;
}

/// When the fake was asked to read `entity`, first first.
fn reads_of(f: &Fixture, entity: &str) -> Vec<std::time::Instant> {
    f.remote
        .read_at
        .lock()
        .unwrap()
        .iter()
        .filter(|(read, _)| read == entity)
        .map(|(_, at)| *at)
        .collect()
}

/// **A run of a chain is read at once, on the started run's clock** (#2594,
/// ruling 3): BUILD-2 names BUILD-3 and BUILD-5 only once it has been
/// followed past its 30 s as a person's read. Each is read at once after the
/// read that named it, not a cadence later, though the two reads come
/// together and the source's lock lets one through at a time; from then on
/// each is read at the started run's 5 s, not the 2 s of a follow whose
/// clock began there. The gaps are timed from the fake's own reads, and load
/// only lengthens them.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_of_a_chain_is_read_at_once_and_keeps_the_started_runs_clock() {
    let f = fixture("refresh_follow_chain_clock").await;
    mirrored(&f).await;
    f.open_the_door();
    trigger_the_run(&f).await;
    until("the run is followed", || async { followed(&f) == [RUN] }).await;
    tokio::time::sleep(Duration::from_secs(31)).await;

    let keys = ["BUILD-3", "BUILD-5"];
    f.remote
        .chain
        .lock()
        .unwrap()
        .extend(keys.map(|key| (key.to_owned(), (Some("queued"), Vec::new()))));
    *f.remote.waits.lock().unwrap() = keys.map(str::to_owned).to_vec();
    let named = std::time::Instant::now();
    for key in keys {
        let dep = format!("{SOURCE}:{key}");
        for n in 1..=3 {
            until(&format!("{key}'s read {n}"), || async {
                reads_of(&f, &dep).len() >= n
            })
            .await;
        }
    }
    let naming = reads_of(&f, RUN)
        .into_iter()
        .find(|at| *at >= named)
        .expect("the read that named the chain");
    for key in keys {
        let reads = reads_of(&f, &format!("{SOURCE}:{key}"));
        let first = reads[0].duration_since(naming);
        assert!(
            first < Duration::from_millis(2_500),
            "{key} is read at once, not 5 s on: {first:?}"
        );
        for pair in reads.windows(2).take(2) {
            let gap = pair[1].duration_since(pair[0]);
            assert!(
                gap >= Duration::from_millis(4_500),
                "{key} is read on the started run's 5 s, not a clock of its own: {gap:?}"
            );
        }
    }

    for key in keys {
        chain_moves(&f, key, "finished");
    }
    *f.remote.run_state.lock().unwrap() = Some("finished");
    until("the follows end", || async { followed(&f).is_empty() }).await;
    f.state.scheduler.shutdown().await;
}

/// **A run of a chain read as gone is not brought again** (#2594): BUILD-2
/// waits on BUILD-6, which the source answers gone, as a TeamCity source
/// scoped to a project answers for a dependency in another. BUILD-6 is never
/// mirrored, so never held as finished; it is read once, not again on every
/// read of BUILD-2.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_of_a_chain_read_as_gone_is_not_brought_again() {
    let f = fixture("refresh_follow_chain_gone").await;
    mirrored(&f).await;
    f.open_the_door();
    *f.remote.waits.lock().unwrap() = vec!["BUILD-6".to_owned()];
    f.remote
        .chain
        .lock()
        .unwrap()
        .insert("BUILD-6".to_owned(), (None, Vec::new()));
    trigger_the_run(&f).await;
    let gone = format!("{SOURCE}:BUILD-6");
    until("BUILD-6 is read", || async {
        !reads_of(&f, &gone).is_empty()
    })
    .await;
    let reads = reads_of(&f, RUN).len();
    until("three more reads of the started run", || async {
        reads_of(&f, RUN).len() >= reads + 3
    })
    .await;
    assert_eq!(reads_of(&f, &gone).len(), 1, "read once");
    assert_eq!(followed(&f), [RUN]);

    *f.remote.run_state.lock().unwrap() = Some("finished");
    until("the follow ends", || async { followed(&f).is_empty() }).await;
    f.state.scheduler.shutdown().await;
}
