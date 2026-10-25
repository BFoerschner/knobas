//! A source saved before its adapter could discover a field id gets the id at
//! its next sync, and a typed id is never replaced (#889).

use std::sync::{Arc, Mutex};

use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::instance::SourceInstance;
use knobas_source::{
    AuthMethod, ConnectionInfo, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError,
};
use knobas_sync::config::{self, AuthKind, InsertConfig};
use knobas_sync::run_log::{self, SyncOutcome, SyncTrigger};
use knobas_sync::scheduler::{AdapterRegistry, RunConnections, RunMode, SchedulerDeps, SyncEvents};

/// What the fake far end answers *Test connection* with.
#[derive(Clone)]
enum Discovery {
    Fields(Vec<(&'static str, &'static str)>),
    Refused,
    /// A far end that never answers.
    Hangs,
}

/// Every config a sync-time adapter was built with, in order.
type Built = Arc<Mutex<Vec<serde_json::Value>>>;

/// Every position a sync was handed, in order.
type Handed = Arc<Mutex<Vec<Option<Cursor>>>>;

/// Whether the far end fails the sync itself.
type Fails = Arc<std::sync::atomic::AtomicBool>;

/// How many times something was asked: the far end's *Test connection*, or
/// the keychain.
type Asked = Arc<std::sync::atomic::AtomicUsize>;

fn count(asked: &Asked) -> usize {
    asked.load(std::sync::atomic::Ordering::SeqCst)
}

/// A keychain that counts its reads.
struct CountingStore {
    inner: MemoryStore,
    gets: Asked,
}

impl SecretStore for CountingStore {
    fn get(
        &self,
        account: &knobas_secrets::KeychainAccount,
    ) -> Result<Option<Secret>, knobas_secrets::SecretError> {
        self.gets.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.get(account)
    }
    fn put(
        &self,
        account: &knobas_secrets::KeychainAccount,
        secret: &Secret,
    ) -> Result<(), knobas_secrets::SecretError> {
        self.inner.put(account, secret)
    }
    fn delete(
        &self,
        account: &knobas_secrets::KeychainAccount,
    ) -> Result<(), knobas_secrets::SecretError> {
        self.inner.delete(account)
    }
}

struct Fake {
    id: String,
    discovery: Discovery,
    handed: Handed,
    fails: Fails,
    asked: Asked,
}

#[async_trait::async_trait]
impl Source for Fake {
    async fn view(
        &self,
        _entity: &str,
        _section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, SourceError> {
        Ok(None)
    }

    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_kind: "fieldy".to_owned(),
            name: "Fieldy".to_owned(),
            capabilities: Vec::new(),
            adapter_version: "0".to_owned(),
            auth_methods: vec![AuthMethod::Pat],
            accepts_account: false,
            min_sync_interval_secs: 60,
            roles: Vec::new(),
            source_actions: Vec::new(),
            clone_from: Vec::new(),
            url_forms: Vec::new(),
            write_ops: Vec::new(),
            entity_kinds: vec![KindInfo {
                id: "ticket".to_owned(),
                label: "Ticket".to_owned(),
                plural: "Tickets".to_owned(),
                monogram: "TI".to_owned(),
                full_sync_exhaustive: true,
                refreshes: true,
            }],
            config_schema: serde_json::json!({"type": "object", "properties": {
                "epic_link_field": {"type": ["string", "null"]},
                "sprint_field": {"type": ["string", "null"]},
            }}),
            payload_paths: Vec::new(),
        }
    }

    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        self.asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        match &self.discovery {
            Discovery::Refused => Err(SourceError::Unauthorized { status: Some(401) }),
            Discovery::Hangs => std::future::pending().await,
            Discovery::Fields(found) => Ok(ConnectionInfo {
                account: None,
                server_version: None,
                secret_expires_at: None,
                detail: None,
                discovered: found
                    .iter()
                    .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                    .collect(),
            }),
        }
    }

    async fn sync(
        &self,
        cursor: Option<Cursor>,
        _sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        self.handed.lock().unwrap().push(cursor.clone());
        if self.fails.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(SourceError::protocol("the far end failed the sync"));
        }
        Ok(cursor.unwrap_or_else(|| "from-the-top".to_owned()))
    }

    async fn write(
        &self,
        _op: knobas_source::WriteOp,
    ) -> Result<knobas_source::WriteReceipt, SourceError> {
        Ok(knobas_source::WriteReceipt::none())
    }

    async fn reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError> {
        Err(SourceError::protocol(format!("no workflow for {entity:?}")))
    }
}

struct Registry {
    discovery: Discovery,
    built: Built,
    handed: Handed,
    fails: Fails,
    asked: Asked,
}

impl AdapterRegistry for Registry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        vec![
            Fake {
                id: "fieldy".to_owned(),
                discovery: self.discovery.clone(),
                handed: Arc::clone(&self.handed),
                fails: Arc::clone(&self.fails),
                asked: Arc::clone(&self.asked),
            }
            .descriptor(),
        ]
    }

    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        self.built.lock().unwrap().push(instance.config.clone());
        Ok(Box::new(Fake {
            id: instance.id,
            discovery: self.discovery.clone(),
            handed: Arc::clone(&self.handed),
            fails: Arc::clone(&self.fails),
            asked: Arc::clone(&self.asked),
        }))
    }
}

struct TestConnections(knobas_db::embedded::Connector);

#[async_trait::async_trait]
impl RunConnections for TestConnections {
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

struct Harness {
    deps: SchedulerDeps,
    built: Built,
    handed: Handed,
    fails: Fails,
    /// *Test connection* calls, across every adapter the registry built.
    asked: Asked,
    /// Keychain reads.
    gets: Asked,
    id: String,
}

async fn harness(stored: serde_json::Value, discovery: Discovery) -> Harness {
    let connector = knobas_db::test_util::test_connector().await;
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let id = format!("disc-{}", uuid::Uuid::new_v4().simple());
    config::insert(
        &pool,
        &InsertConfig {
            id: id.clone(),
            adapter_kind: "fieldy".into(),
            display_name: "Fieldy".into(),
            base_url: String::new(),
            auth_kind: AuthKind::Method(AuthMethod::Pat),
            config: stored,
            sync_interval_secs: 300,
            enabled: true,
        },
    )
    .await
    .unwrap();
    let store = MemoryStore::new();
    store
        .put(
            &knobas_secrets::KeychainAccount::source(&id),
            &Secret::just(AuthMethod::Pat, "tok"),
        )
        .unwrap();
    let built: Built = Arc::default();
    let handed: Handed = Arc::default();
    let fails: Fails = Arc::default();
    let asked: Asked = Arc::default();
    let gets: Asked = Arc::default();
    Harness {
        deps: SchedulerDeps {
            pool,
            connections: Arc::new(TestConnections(connector)),
            registry: Arc::new(Registry {
                discovery,
                built: Arc::clone(&built),
                handed: Arc::clone(&handed),
                fails: Arc::clone(&fails),
                asked: Arc::clone(&asked),
            }),
            secrets: Arc::new(CountingStore {
                inner: store,
                gets: Arc::clone(&gets),
            }),
            events: Arc::new(Quiet),
            timing: knobas_sync::scheduler::SchedulerTiming::default(),
            quit: knobas_sync::scheduler::Quit::default(),
        },
        built,
        handed,
        fails,
        asked,
        gets,
        id,
    }
}

/// Give the source a stored position, as every source saved before #811 has.
async fn with_position(h: &Harness, cursor: &str) {
    sqlx::query("update knobas.source_config set cursor = $1 where id = $2")
        .bind(cursor)
        .bind(&h.id)
        .execute(&h.deps.pool)
        .await
        .unwrap();
}

async fn last_trigger(h: &Harness) -> String {
    sqlx::query_scalar(
        "select trigger from knobas.sync_run where source_id = $1 order by id desc limit 1",
    )
    .bind(&h.id)
    .fetch_one(&h.deps.pool)
    .await
    .unwrap()
}

async fn sync_once(h: &Harness) -> SyncOutcome {
    let run_id = run_log::start(&h.deps.pool, &h.id, SyncTrigger::Manual)
        .await
        .unwrap();
    let result = knobas_sync::scheduler::execute_run(
        &h.deps,
        &h.id,
        run_id,
        RunMode::Incremental,
        None,
        std::time::Instant::now(),
    )
    .await;
    knobas_sync::scheduler::settle(&h.deps, &h.id, run_id, &result).await;
    result.outcome
}

async fn stored(h: &Harness) -> serde_json::Value {
    config::get(&h.deps.pool, &h.id)
        .await
        .unwrap()
        .unwrap()
        .config
}

#[tokio::test]
async fn a_source_saved_without_the_field_ids_gets_them_at_its_next_sync() {
    let h = harness(
        serde_json::json!({ "projects": ["PAY"], "sprint_field": null }),
        Discovery::Fields(vec![
            ("epic_link_field", "customfield_10101"),
            ("sprint_field", "customfield_10104"),
        ]),
    )
    .await;
    assert_eq!(sync_once(&h).await, SyncOutcome::Ok);
    let config = stored(&h).await;
    assert_eq!(config["epic_link_field"], "customfield_10101");
    assert_eq!(config["sprint_field"], "customfield_10104");
    assert_eq!(
        config["projects"],
        serde_json::json!(["PAY"]),
        "the rest is kept"
    );
    let last = h.built.lock().unwrap().last().cloned().unwrap();
    assert_eq!(
        last["sprint_field"], "customfield_10104",
        "the adapter that syncs is built from the recorded id"
    );
}

#[tokio::test]
async fn a_typed_id_is_never_replaced_and_the_empty_one_is_still_filled() {
    let h = harness(
        serde_json::json!({ "sprint_field": "customfield_77777" }),
        Discovery::Fields(vec![
            ("epic_link_field", "customfield_10101"),
            ("sprint_field", "customfield_10104"),
        ]),
    )
    .await;
    assert_eq!(sync_once(&h).await, SyncOutcome::Ok);
    let config = stored(&h).await;
    assert_eq!(config["sprint_field"], "customfield_77777");
    assert_eq!(config["epic_link_field"], "customfield_10101");
}

#[tokio::test]
async fn fill_missing_does_not_overwrite_an_id_saved_after_the_read() {
    let h = harness(
        serde_json::json!({ "sprint_field": "customfield_77777" }),
        Discovery::Fields(Vec::new()),
    )
    .await;
    let found = std::collections::BTreeMap::from([
        ("sprint_field".to_owned(), "customfield_10104".to_owned()),
        ("epic_link_field".to_owned(), "customfield_10101".to_owned()),
    ]);
    let row = config::fill_missing(&h.deps.pool, &h.id, &found)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.config["sprint_field"], "customfield_77777");
    assert_eq!(row.config["epic_link_field"], "customfield_10101");
}

#[tokio::test]
async fn a_refused_discovery_leaves_the_config_alone() {
    let h = harness(serde_json::json!({}), Discovery::Refused).await;
    let _ = sync_once(&h).await;
    assert_eq!(stored(&h).await, serde_json::json!({}));
}

/// A source saved before #811 has a position and tickets mirrored with
/// `sprints: []`. An incremental run from that position would re-fetch only
/// what changed upstream, so the run that records the id reads from the top,
/// and the log calls it a backfill.
#[tokio::test]
async fn the_run_that_records_an_id_on_a_source_with_a_position_is_a_backfill() {
    let h = harness(
        serde_json::json!({}),
        Discovery::Fields(vec![("sprint_field", "customfield_10104")]),
    )
    .await;
    with_position(&h, "pos-1").await;
    assert_eq!(sync_once(&h).await, SyncOutcome::Ok);
    assert_eq!(*h.handed.lock().unwrap(), [None], "read from the top");
    assert_eq!(last_trigger(&h).await, "backfill");

    // The next run has nothing to record and resumes from where that one
    // ended, as any incremental run does.
    assert_eq!(sync_once(&h).await, SyncOutcome::Ok);
    assert_eq!(
        *h.handed.lock().unwrap(),
        [None, Some("from-the-top".to_owned())]
    );
    assert_eq!(last_trigger(&h).await, "manual");
}

/// A recording run that fails records nothing, so the next run discovers the
/// id again and is the backfill instead: one failure never strands the
/// untouched tickets on the old position.
#[tokio::test]
async fn a_recording_run_that_fails_records_nothing_and_the_next_one_backfills() {
    let h = harness(
        serde_json::json!({}),
        Discovery::Fields(vec![("sprint_field", "customfield_10104")]),
    )
    .await;
    with_position(&h, "pos-1").await;
    h.fails.store(true, std::sync::atomic::Ordering::SeqCst);
    assert_ne!(sync_once(&h).await, SyncOutcome::Ok);
    assert_eq!(stored(&h).await, serde_json::json!({}), "nothing recorded");

    h.fails.store(false, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(sync_once(&h).await, SyncOutcome::Ok);
    assert_eq!(
        *h.handed.lock().unwrap(),
        [None, None],
        "both read from the top"
    );
    assert_eq!(last_trigger(&h).await, "backfill");
    assert_eq!(stored(&h).await["sprint_field"], "customfield_10104");
}

#[tokio::test]
async fn a_source_with_a_position_and_nothing_to_record_resumes_from_it() {
    let h = harness(serde_json::json!({}), Discovery::Fields(Vec::new())).await;
    with_position(&h, "pos-1").await;
    assert_eq!(sync_once(&h).await, SyncOutcome::Ok);
    assert_eq!(*h.handed.lock().unwrap(), [Some("pos-1".to_owned())]);
    assert_eq!(last_trigger(&h).await, "manual");
}

/// A far end that does not answer the discovery costs the run its budget, not
/// the run: the sync goes on without it.
#[tokio::test]
async fn a_discovery_that_never_answers_does_not_hold_the_sync() {
    let h = harness(serde_json::json!({}), Discovery::Hangs).await;
    let started = std::time::Instant::now();
    assert_eq!(sync_once(&h).await, SyncOutcome::Ok);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(30),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(h.handed.lock().unwrap().len(), 1, "the sync ran");
    assert_eq!(stored(&h).await, serde_json::json!({}));
}

/// A far end that answers and has nothing to find (a Jira without Jira
/// Software) is asked once per process, not on every run (#1702, ruling D7).
#[tokio::test]
async fn a_source_with_nothing_to_discover_is_asked_once() {
    let h = harness(serde_json::json!({}), Discovery::Fields(Vec::new())).await;
    assert_eq!(sync_once(&h).await, SyncOutcome::Ok);
    assert_eq!(sync_once(&h).await, SyncOutcome::Ok);
    assert_eq!(count(&h.asked), 1);
    assert_eq!(stored(&h).await, serde_json::json!({}));
}

/// A run that finds one id of two records it, and the next run asks nothing:
/// the key that answered nothing is remembered, and recording the other does
/// not make it look like a new source.
#[tokio::test]
async fn the_key_that_answered_nothing_is_not_asked_again_after_the_other_is_recorded() {
    let h = harness(
        serde_json::json!({}),
        Discovery::Fields(vec![("epic_link_field", "customfield_10101")]),
    )
    .await;
    assert_eq!(sync_once(&h).await, SyncOutcome::Ok);
    assert_eq!(stored(&h).await["epic_link_field"], "customfield_10101");
    assert_eq!(sync_once(&h).await, SyncOutcome::Ok);
    assert_eq!(count(&h.asked), 1);
}

/// A refusal is not an answer: the far end may let the next run in, so it is
/// asked again.
#[tokio::test]
async fn a_refused_discovery_is_asked_again_next_run() {
    let h = harness(serde_json::json!({}), Discovery::Refused).await;
    let _ = sync_once(&h).await;
    let _ = sync_once(&h).await;
    assert_eq!(count(&h.asked), 2);
}

/// What is remembered is about one far end: a source pointed somewhere else
/// is asked again.
#[tokio::test]
async fn a_source_moved_to_another_address_is_asked_again() {
    let h = harness(serde_json::json!({}), Discovery::Fields(Vec::new())).await;
    assert_eq!(sync_once(&h).await, SyncOutcome::Ok);
    sqlx::query(
        "update knobas.source_config set base_url = 'https://elsewhere.example' where id = $1",
    )
    .bind(&h.id)
    .execute(&h.deps.pool)
    .await
    .unwrap();
    assert_eq!(sync_once(&h).await, SyncOutcome::Ok);
    assert_eq!(count(&h.asked), 2);
}

/// The run that records an id builds its adapter twice, with the id and
/// without, and reads the keychain once for both (#1702).
#[tokio::test]
async fn a_run_that_finds_an_id_reads_the_keychain_once() {
    let h = harness(
        serde_json::json!({}),
        Discovery::Fields(vec![("sprint_field", "customfield_10104")]),
    )
    .await;
    assert_eq!(sync_once(&h).await, SyncOutcome::Ok);
    assert_eq!(count(&h.gets), 1);
    assert_eq!(h.built.lock().unwrap().len(), 2);
}
