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
}

/// Every config a sync-time adapter was built with, in order.
type Built = Arc<Mutex<Vec<serde_json::Value>>>;

struct Fake {
    id: String,
    discovery: Discovery,
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
            write_ops: Vec::new(),
            entity_kinds: vec![KindInfo {
                id: "ticket".to_owned(),
                label: "Ticket".to_owned(),
                plural: "Tickets".to_owned(),
                monogram: "TI".to_owned(),
                full_sync_exhaustive: true,
            }],
            config_schema: serde_json::json!({"type": "object", "properties": {
                "epic_link_field": {"type": ["string", "null"]},
                "sprint_field": {"type": ["string", "null"]},
            }}),
            payload_paths: Vec::new(),
        }
    }

    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        match &self.discovery {
            Discovery::Refused => Err(SourceError::Unauthorized { status: Some(401) }),
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
        Ok(cursor.unwrap_or_default())
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
}

impl AdapterRegistry for Registry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        vec![
            Fake {
                id: "fieldy".to_owned(),
                discovery: self.discovery.clone(),
            }
            .descriptor(),
        ]
    }

    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        self.built.lock().unwrap().push(instance.config.clone());
        Ok(Box::new(Fake {
            id: instance.id,
            discovery: self.discovery.clone(),
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
    Harness {
        deps: SchedulerDeps {
            pool,
            connections: Arc::new(TestConnections(connector)),
            registry: Arc::new(Registry {
                discovery,
                built: Arc::clone(&built),
            }),
            secrets: Arc::new(store),
            events: Arc::new(Quiet),
            timing: knobas_sync::scheduler::SchedulerTiming::default(),
        },
        built,
        id,
    }
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
