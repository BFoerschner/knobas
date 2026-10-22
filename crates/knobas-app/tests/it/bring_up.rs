//! **Bring-up and shutdown, run** (#1632, spec #1619 ruling B4): the app's
//! own `bring_up` and `shutdown` on a `tauri::test` mock app over a scratch
//! database on the shared test server, in place of the `include_str!` scans
//! of `lib.rs` that pinned their order by byte offset.
//!
//! `bring_up` is `spawn_bring_up`'s body and `shutdown` is the exit handler's,
//! so what these assert is what a launch, a Retry and a Cmd-Q do: the order
//! is witnessed by what the next reader sees, never by where a word sits in
//! the source.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use knobas_app::sources::{Registry, SecretStoreKind, SourcesState, install, installed};
use knobas_app::{AppState, DbState, Lifecycle, Profile};
use knobas_source::{
    AuthMethod, ConnectionInfo, Cursor, Sink, Source, SourceDescriptor, SourceError, WriteOp,
    WriteReceipt,
};
use knobas_sync::scheduler::{AdapterRegistry, RunConnections, SchedulerDeps, SyncEvents};
use sqlx::PgPool;
use tauri::Manager;
use tauri::test::MockRuntime;

use crate::external_adapter_install::{install_fixture, scratch};

/// A mock app managing what `setup` manages before it spawns bring-up, over
/// `registry`.
fn mock_app(profile: &Profile, registry: &Registry) -> tauri::App<MockRuntime> {
    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("a mock app");
    app.manage(profile.clone());
    // Never the keychain: a test that reached it would read or write the
    // developer's real credentials.
    app.manage(SecretStoreKind::Memory);
    app.manage(Lifecycle::with_registry(Arc::new(registry.clone())));
    app.manage(knobas_app::capture::CaptureState::new());
    app
}

/// The app's bring-up of `app` onto `connector`'s database, to its end;
/// answers the state it ended in.
fn bring_up(
    app: &tauri::App<MockRuntime>,
    profile: &Profile,
    connector: &knobas_db::embedded::Connector,
    registry: &Registry,
) -> DbState {
    let handle = app.handle().clone();
    // `block_in_place` first: `block_on` from inside the test's runtime panics.
    tokio::task::block_in_place(|| {
        tauri::async_runtime::block_on(knobas_app::bring_up(
            &handle,
            profile,
            Some(connector.url()),
            registry.clone(),
        ));
    });
    app.state::<Lifecycle>().get()
}

/// The app's exit handler on `app`.
fn shutdown(app: &tauri::App<MockRuntime>) {
    tokio::task::block_in_place(|| knobas_app::shutdown(app.handle()));
}

/// LIFE-1: Retry after a failed bring-up runs bring-up again over the
/// process's registry, which already holds every adapter the first attempt
/// loaded. Loading with `add` filed each one under refused ("an adapter of
/// that kind is already installed"), and Settings said so for the rest of the
/// session.
///
/// Two mock apps over one registry, because that is what a retry has: the
/// first attempt's sources state was never managed (a retry follows a
/// failure, and `sources::start` manages last), while the registry is the
/// process's and outlives it.
#[tokio::test(flavor = "multi_thread")]
async fn a_second_bring_up_over_one_registry_refuses_no_installed_adapter() {
    let (pool, connector) = scratch("bring-up-twice").await;
    let data = tempfile::tempdir().unwrap();
    let profile = Profile::new(false, data.path());
    install_fixture(
        &pool,
        &Registry::default(),
        &install::store_dir(&profile.dir),
    )
    .await;

    let registry = Registry::default();
    for attempt in ["the first", "the retried"] {
        let app = mock_app(&profile, &registry);
        assert_eq!(
            bring_up(&app, &profile, &connector, &registry),
            DbState::Ready,
            "{attempt} bring-up"
        );
        shutdown(&app);
    }

    let shown: Vec<(String, Option<String>)> = installed::list(&pool, &registry)
        .await
        .unwrap()
        .into_iter()
        .map(|status| (status.adapter_kind, status.refused))
        .collect();
    assert_eq!(
        shown,
        [("wasm-fixture".to_owned(), None)],
        "Settings lists the installed adapter as refused after a retry"
    );
    assert!(
        crate::external_adapter_install::kinds(&registry).contains(&"wasm-fixture".to_owned()),
        "the retried bring-up left the adapter loaded"
    );
}

/// #278: a timer that outlived the last process is closed **before** the
/// state reads `Ready`, because `Ready` is when the shell makes its first
/// `current_timer` read and would draw a clock that ran all night.
///
/// The timer table is read inside the `db:state` listener, synchronously, at
/// the moment `Ready` is announced -- not after bring-up returns, when a
/// sweep that ran after the announcement would look the same.
#[tokio::test(flavor = "multi_thread")]
async fn bring_up_closes_a_stranded_timer_before_it_announces_ready() {
    use tauri::Listener as _;

    let (pool, connector) = scratch("bring-up-stranded").await;
    knobas_app::time::start(
        &pool,
        knobas_app::time::TimerTarget::Label {
            label: "left running overnight".to_owned(),
        },
        None,
    )
    .await
    .expect("the timer starts");

    let data = tempfile::tempdir().unwrap();
    let profile = Profile::new(false, data.path());
    let registry = Registry::default();
    let app = mock_app(&profile, &registry);

    // One entry per `Ready` announced: the running timers at that moment.
    let at_ready: Arc<Mutex<Vec<i64>>> = Arc::default();
    let seen = Arc::clone(&at_ready);
    let reader = connector.clone();
    app.listen(knobas_app::events::DB_STATE, move |event| {
        let state: serde_json::Value = serde_json::from_str(event.payload()).unwrap();
        if state["state"] != "ready" {
            return;
        }
        // A thread and a runtime of its own: the listener runs on bring-up's
        // task and must answer before bring-up goes on.
        let reader = reader.clone();
        let timers = std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async move {
                    let mut connection = reader.connect().await.unwrap();
                    sqlx::query_scalar::<_, i64>("select count(*) from knobas.timer")
                        .fetch_one(&mut connection)
                        .await
                        .unwrap()
                })
        })
        .join()
        .unwrap();
        seen.lock().unwrap().push(timers);
    });

    assert_eq!(
        bring_up(&app, &profile, &connector, &registry),
        DbState::Ready
    );
    shutdown(&app);

    assert_eq!(
        *at_ready.lock().unwrap(),
        [0],
        "`Ready` is announced once, with the stranded timer already closed"
    );
    let closed: i64 = sqlx::query_scalar(
        "select count(*) from knobas.block
          where label = 'left running overnight' and ended_by_relaunch",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(closed, 1, "the stranded timer became a closed block");
}

/// #1485: bring-up builds the suggestion pass and hands it to the scheduler
/// it starts, which requests it after every run that ends OK. A mock source
/// synced through the scheduler bring-up started proposes links, with nobody
/// else asking for a pass.
#[tokio::test(flavor = "multi_thread")]
async fn bring_up_hands_its_suggestion_pass_to_the_scheduler() {
    let (pool, connector) = scratch("bring-up-suggestions").await;
    let input: knobas_app::sources::NewSource = serde_json::from_value(serde_json::json!({
        "id": "mock",
        "adapter_kind": "mock",
        "display_name": "Tidewater",
        "base_url": "https://mock.example.invalid",
        "auth_kind": "Pat",
        "config": {},
        "secret": { "value": "a token the mock never sees" },
        "sync_interval_secs": 900,
        "enabled": true,
    }))
    .unwrap();
    let elsewhere: Arc<dyn knobas_secrets::SecretStore> =
        Arc::new(knobas_secrets::MemoryStore::new());
    knobas_app::sources::crud::add(&pool, &elsewhere, &Registry::default(), input)
        .await
        .expect("the mock source is added");

    let data = tempfile::tempdir().unwrap();
    let profile = Profile::new(false, data.path());
    let registry = Registry::default();
    let app = mock_app(&profile, &registry);
    assert_eq!(
        bring_up(&app, &profile, &connector, &registry),
        DbState::Ready
    );

    let sources = app.state::<SourcesState>();
    // The memory store bring-up made is empty; the credential goes into it.
    knobas_secrets::spawn::put(
        &sources.secrets,
        &knobas_secrets::KeychainAccount::source("mock"),
        knobas_secrets::Secret::just(AuthMethod::Pat, "a token the mock never sees"),
    )
    .await
    .unwrap();
    sources
        .scheduler
        .trigger("mock", knobas_sync::SyncTrigger::Manual, None)
        .await
        .expect("the run starts");

    let proposed = async {
        loop {
            let count: i64 = sqlx::query_scalar("select count(*) from knobas.proposed_link")
                .fetch_one(&pool)
                .await
                .unwrap();
            if count > 0 {
                return count;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    };
    let proposed = tokio::time::timeout(Duration::from_secs(60), proposed).await;
    shutdown(&app);
    assert!(
        proposed.is_ok(),
        "the Tidewater corpus matches the rules and no pass proposed anything: \
         the scheduler bring-up started holds no pass"
    );
}

// -- shutdown ---------------------------------------------------------------

const PARKED: &str = "parked";

/// Whether the application pool was closed at the moment the scheduler
/// stopped the run, once per stopped run.
type Stops = Arc<Mutex<Vec<bool>>>;

/// A registry of one adapter whose sync parks until the scheduler drops it,
/// recording the application pool's state as it goes.
struct Recording {
    entered: Arc<tokio::sync::Notify>,
    stops: Stops,
    app_pool: PgPool,
}

impl AdapterRegistry for Recording {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        vec![descriptor()]
    }

    fn build(
        &self,
        _instance: knobas_source::instance::SourceInstance,
    ) -> Result<Box<dyn Source>, SourceError> {
        Ok(Box::new(Parked {
            entered: Arc::clone(&self.entered),
            stops: Arc::clone(&self.stops),
            app_pool: self.app_pool.clone(),
        }))
    }
}

struct Parked {
    entered: Arc<tokio::sync::Notify>,
    stops: Stops,
    app_pool: PgPool,
}

/// Dropped with the run's future: when the scheduler stops the run.
struct OnStop {
    stops: Stops,
    app_pool: PgPool,
}

impl Drop for OnStop {
    fn drop(&mut self) {
        self.stops.lock().unwrap().push(self.app_pool.is_closed());
    }
}

#[async_trait]
impl Source for Parked {
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
        _cursor: Option<Cursor>,
        _sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        let _on_stop = OnStop {
            stops: Arc::clone(&self.stops),
            app_pool: self.app_pool.clone(),
        };
        self.entered.notify_one();
        std::future::pending().await
    }

    async fn write(&self, _op: WriteOp) -> Result<WriteReceipt, SourceError> {
        Err(SourceError::protocol("this adapter writes nothing"))
    }

    async fn reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError> {
        Err(SourceError::protocol(format!("no workflow for {entity:?}")))
    }
}

fn descriptor() -> SourceDescriptor {
    SourceDescriptor {
        id: PARKED.to_owned(),
        adapter_kind: PARKED.to_owned(),
        name: "Parked".to_owned(),
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
        entity_kinds: Vec::new(),
        config_schema: serde_json::json!({"type": "object", "properties": {}}),
        payload_paths: Vec::new(),
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

/// Cmd-Q with a sync run in flight: the exit handler stops the scheduler, and
/// with it the run, **while the database is still open**, then stops the
/// database. The other order closes the pool under runs that hold
/// transactions, which is the stall the M0 carry-over describes. Both exit
/// events reach the handler, so the second call must do nothing.
///
/// The scheduler is the real one over a recording adapter: the run's future
/// is dropped when the scheduler stops it, and the drop records whether the
/// application pool -- the one `shutdown_database` closes -- was closed yet.
#[tokio::test(flavor = "multi_thread")]
async fn shutdown_stops_the_scheduler_before_the_database_and_a_second_does_nothing() {
    let connector = knobas_db::test_util::scratch_database("shutdown-order").await;
    let root = tempfile::tempdir().unwrap();
    let db = knobas_db::EmbeddedDb::start(knobas_db::DbConfig {
        root_dir: root.path().to_path_buf(),
        existing_url: Some(connector.url()),
    })
    .await
    .expect("the scratch database");
    let app_pool = db.pool().clone();

    knobas_sync::config::insert(
        &app_pool,
        &knobas_sync::config::InsertConfig {
            id: PARKED.to_owned(),
            adapter_kind: PARKED.to_owned(),
            display_name: "Parked".to_owned(),
            base_url: "https://parked.example".to_owned(),
            auth_kind: knobas_sync::config::AuthKind::Method(AuthMethod::Pat),
            config: serde_json::json!({}),
            sync_interval_secs: 86_400,
            enabled: true,
        },
    )
    .await
    .expect("the source row");
    let secrets: Arc<dyn knobas_secrets::SecretStore> =
        Arc::new(knobas_secrets::MemoryStore::new());
    knobas_secrets::spawn::put(
        &secrets,
        &knobas_secrets::KeychainAccount::source(PARKED),
        knobas_secrets::Secret::just(AuthMethod::Pat, "a token"),
    )
    .await
    .unwrap();

    let entered = Arc::new(tokio::sync::Notify::new());
    let stops: Stops = Arc::default();
    let registry: Arc<dyn AdapterRegistry> = Arc::new(Recording {
        entered: Arc::clone(&entered),
        stops: Arc::clone(&stops),
        app_pool: app_pool.clone(),
    });
    let scheduler = knobas_sync::scheduler::Scheduler::start(SchedulerDeps {
        pool: connector.pool(2).await.unwrap(),
        connections: Arc::new(Connections(connector)),
        registry: Arc::clone(&registry),
        secrets: Arc::clone(&secrets),
        events: Arc::new(Quiet),
        // No tick of its own: the one run is the test's.
        timing: knobas_sync::scheduler::SchedulerTiming {
            startup_delay: Duration::from_secs(3600),
            tick: Duration::from_secs(3600),
        },
    })
    .await
    .expect("a scheduler");

    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("a mock app");
    let lifecycle = Lifecycle::with_registry(Arc::clone(&registry));
    lifecycle.install(AppState::over_db(db));
    app.manage(lifecycle);
    app.manage(SourcesState {
        pool: app_pool.clone(),
        scheduler,
        secrets,
        registry,
    });

    app.state::<SourcesState>()
        .scheduler
        .trigger(PARKED, knobas_sync::SyncTrigger::Manual, None)
        .await
        .expect("the run starts");
    tokio::time::timeout(Duration::from_secs(30), entered.notified())
        .await
        .expect("the run reaches the adapter");

    shutdown(&app);
    assert_eq!(
        *stops.lock().unwrap(),
        [false],
        "the run was stopped once, with the database still open"
    );
    assert!(app_pool.is_closed(), "shutdown closed the database's pool");

    shutdown(&app);
    assert_eq!(
        *stops.lock().unwrap(),
        [false],
        "a second shutdown stopped something again"
    );
}
