//! A ticket's reads asked ahead of a press go in the background lane (ADR-0054): `reachable_transitions` and
//! `ticket_fields` with `behind: true` are the board's reads while the pointer rests on a card, and stand aside for a
//! person's own reads of the same server; without it they are a person's. Through Tauri's own IPC pipeline, over a
//! source that records the lane each read was sent on (`knobas_http::current_lane`), and that answers fields, so the
//! command's answer is the adapter's (the final suite's mutants: the `behind` branches and the fields' answer each
//! survived the lib's tests).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use knobas_app::sources::{Registry, SourcesState};
use knobas_secrets::MemoryStore;
use knobas_source::instance::SourceInstance;
use knobas_source::{Source, SourceDescriptor, SourceError};
use knobas_sync::scheduler::{
    AdapterRegistry, RunConnections, Scheduler, SchedulerDeps, SyncEvents,
};
use serde_json::json;
use tauri::Manager;
use tauri::ipc::CallbackFn;
use tauri::test::MockRuntime;

#[cfg(any(windows, target_os = "android"))]
const LOCAL_ORIGIN: &str = "http://tauri.localhost";
#[cfg(not(any(windows, target_os = "android")))]
const LOCAL_ORIGIN: &str = "tauri://localhost";

const SRC: &str = "lanes";

/// The lane of every read, in order, by what was read.
type Lanes = Arc<Mutex<Vec<(&'static str, knobas_http::Lane)>>>;

struct Recording(Lanes);

impl AdapterRegistry for Recording {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        Registry::for_process().descriptors()
    }
    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        Ok(Box::new(Fake {
            instance,
            lanes: self.0.clone(),
        }))
    }
}

struct Fake {
    instance: SourceInstance,
    lanes: Lanes,
}

#[async_trait]
impl Source for Fake {
    fn descriptor(&self) -> SourceDescriptor {
        let mut d = Registry::for_process()
            .descriptors()
            .into_iter()
            .find(|d| d.adapter_kind == "mock")
            .expect("the mock is compiled in");
        d.id = self.instance.id.clone();
        d
    }
    async fn test_connection(&self) -> Result<knobas_source::ConnectionInfo, SourceError> {
        Ok(knobas_source::ConnectionInfo::default())
    }
    async fn sync(
        &self,
        cursor: Option<knobas_source::Cursor>,
        _sink: &mut (dyn knobas_source::Sink + Send),
    ) -> Result<knobas_source::Cursor, SourceError> {
        Ok(cursor.unwrap_or_default())
    }
    async fn write(
        &self,
        _op: knobas_source::WriteOp,
    ) -> Result<knobas_source::WriteReceipt, SourceError> {
        Err(SourceError::protocol("read-only"))
    }
    async fn view(
        &self,
        _entity: &str,
        _section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, SourceError> {
        Ok(None)
    }
    async fn reachable_transitions(&self, _entity: &str) -> Result<Vec<String>, SourceError> {
        self.lanes
            .lock()
            .unwrap()
            .push(("transitions", knobas_http::current_lane()));
        Ok(vec!["In Progress".to_owned()])
    }
    async fn ticket_fields(
        &self,
        _entity: &str,
    ) -> Result<Option<knobas_source::ticket::TicketFields>, SourceError> {
        self.lanes
            .lock()
            .unwrap()
            .push(("fields", knobas_http::current_lane()));
        Ok(Some(
            serde_json::from_value(json!({ "fields": [
                { "id": "title", "label": "Title", "kind": { "type": "text" }, "value": "Fender panels cracked" }
            ] }))
            .expect("one text field"),
        ))
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

async fn state(lanes: &Lanes) -> SourcesState {
    let connector = knobas_db::test_util::scratch_database("behind_reads_ipc").await;
    let pool = connector
        .pool(4)
        .await
        .expect("a pool onto the scratch database");
    knobas_sync::config::insert(
        &pool,
        &knobas_sync::config::InsertConfig {
            id: SRC.to_owned(),
            adapter_kind: "mock".to_owned(),
            display_name: "Lanes".to_owned(),
            base_url: String::new(),
            auth_kind: knobas_sync::config::AuthKind::None,
            config: json!({}),
            sync_interval_secs: 86_400,
            enabled: true,
            sync_by_hand: false,
        },
    )
    .await
    .expect("the source row is written");
    let registry: Arc<dyn AdapterRegistry> = Arc::new(Recording(lanes.clone()));
    let secrets: Arc<dyn knobas_secrets::SecretStore> = Arc::new(MemoryStore::new());
    let scheduler = Scheduler::start(SchedulerDeps {
        pool: pool.clone(),
        connections: Arc::new(Connections(connector)),
        registry: registry.clone(),
        secrets: secrets.clone(),
        events: Arc::new(Quiet),
        timing: knobas_sync::scheduler::SchedulerTiming::default(),
        quit: knobas_sync::scheduler::Quit::default(),
    })
    .await
    .expect("a scheduler over the scratch database");
    SourcesState {
        live: Default::default(),
        pool,
        scheduler,
        secrets,
        registry,
    }
}

/// Each `(command, body)` invoked in order on a mock app managing `state`; their answers.
fn invoke(
    state: SourcesState,
    calls: Vec<(&'static str, serde_json::Value)>,
) -> Vec<serde_json::Value> {
    let app = tauri::test::mock_builder()
        .invoke_handler(tauri::generate_handler![
            knobas_app::commands::entity::reachable_transitions,
            knobas_app::commands::entity::ticket_fields
        ])
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app");
    app.manage(state);
    let webview = tauri::WebviewWindowBuilder::<MockRuntime, _>::new(
        &app,
        "main",
        tauri::WebviewUrl::default(),
    )
    .build()
    .expect("mock webview");
    calls
        .into_iter()
        .map(|(cmd, body)| {
            tauri::test::get_ipc_response(
                &webview,
                tauri::webview::InvokeRequest {
                    cmd: cmd.to_owned(),
                    callback: CallbackFn(0),
                    error: CallbackFn(1),
                    url: LOCAL_ORIGIN.parse().expect("url"),
                    body: body.into(),
                    headers: HashMap::new().into_iter().collect(),
                    invoke_key: tauri::test::INVOKE_KEY.to_string(),
                },
            )
            .unwrap_or_else(|error| panic!("{cmd}: {error:?}"))
            .deserialize::<serde_json::Value>()
            .expect("json")
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_read_ahead_of_a_press_goes_behind_and_a_persons_goes_first() {
    let lanes: Lanes = Arc::default();
    let state = state(&lanes).await;
    let entity = format!("{SRC}:BERTH-1");
    let answers = tokio::task::spawn_blocking(move || {
        invoke(
            state,
            vec![
                (
                    "reachable_transitions",
                    json!({ "entityId": entity, "behind": true }),
                ),
                ("reachable_transitions", json!({ "entityId": entity })),
                (
                    "ticket_fields",
                    json!({ "entityId": entity, "behind": true }),
                ),
                (
                    "ticket_fields",
                    json!({ "entityId": entity, "behind": false }),
                ),
            ],
        )
    })
    .await
    .unwrap();
    assert_eq!(answers[0], json!(["In Progress"]));
    assert_eq!(answers[1], json!(["In Progress"]));
    assert_eq!(answers[2]["fields"][0]["id"], "title", "{}", answers[2]);
    assert_eq!(answers[3]["fields"][0]["value"], "Fender panels cracked");
    use knobas_http::Lane::{Background, Interactive};
    assert_eq!(
        *lanes.lock().unwrap(),
        [
            ("transitions", Background),
            ("transitions", Interactive),
            ("fields", Background),
            ("fields", Interactive),
        ]
    );
}
