//! **Bring-up and shutdown, run** (#1632, spec #1619 ruling B4): the app's
//! own `bring_up` and `shutdown` on a `tauri::test` mock app over a scratch
//! database on the shared test server, in place of the `include_str!` scans
//! of `lib.rs` that pinned their order by byte offset.
//!
//! `bring_up` is `spawn_bring_up`'s body and `shutdown` is the exit handler's,
//! so what these assert is what a launch, a Retry and a Cmd-Q do: the order
//! is witnessed by what the next reader sees, never by where a word sits in
//! the source.

use std::sync::atomic::{AtomicBool, Ordering};
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

/// LIFE-1 (spec #1619): Retry after a failed bring-up runs bring-up again over the
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

/// LIFE-1's other half (#1632): a retried bring-up that refuses an adapter
/// the first attempt loaded drops the loaded one. Its artifact file went
/// between the two attempts, so the retry records a refusal, and the
/// registry must not go on building sources of the adapter Settings calls
/// refused.
#[tokio::test(flavor = "multi_thread")]
async fn a_retried_bring_up_that_refuses_a_loaded_adapter_unloads_it() {
    let (pool, connector) = scratch("bring-up-refuses-loaded").await;
    let data = tempfile::tempdir().unwrap();
    let profile = Profile::new(false, data.path());
    let store = install::store_dir(&profile.dir);
    let installed = install_fixture(&pool, &Registry::default(), &store).await;

    let registry = Registry::default();
    let app = mock_app(&profile, &registry);
    assert_eq!(
        bring_up(&app, &profile, &connector, &registry),
        DbState::Ready
    );
    shutdown(&app);
    assert!(
        crate::external_adapter_install::kinds(&registry).contains(&"wasm-fixture".to_owned()),
        "the first bring-up loads the adapter"
    );

    std::fs::remove_file(install::artifact_path(&store, &installed.artifact_sha256)).unwrap();
    let app = mock_app(&profile, &registry);
    assert_eq!(
        bring_up(&app, &profile, &connector, &registry),
        DbState::Ready
    );
    shutdown(&app);

    assert!(
        registry.refusal("wasm-fixture").is_some(),
        "the retry refuses the adapter whose artifact is gone"
    );
    assert!(
        !crate::external_adapter_install::kinds(&registry).contains(&"wasm-fixture".to_owned()),
        "the adapter is refused and still loaded"
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

/// D8 (#1703, spec #1619): `migrating` is the schema and nothing else. Once
/// `migrate::run` returns, the steps after it -- the two sweeps, the
/// declarations, the engines, the shortcut -- are `starting`, each naming
/// itself, so a slow disk's boot screen says what it is waiting on rather
/// than *Bringing the schema up to date* for seconds after it is.
///
/// Read off `db:state` as the boot screen hears it: every state announced, in
/// order.
#[tokio::test(flavor = "multi_thread")]
async fn bring_up_reports_starting_with_the_step_once_the_schema_is_up() {
    use tauri::Listener as _;

    let (_pool, connector) = scratch("bring-up-states").await;
    let data = tempfile::tempdir().unwrap();
    let profile = Profile::new(false, data.path());
    let registry = Registry::default();
    let app = mock_app(&profile, &registry);
    let heard: Arc<Mutex<Vec<serde_json::Value>>> = Arc::default();
    let seen = Arc::clone(&heard);
    app.listen(knobas_app::events::DB_STATE, move |event| {
        seen.lock()
            .unwrap()
            .push(serde_json::from_str(event.payload()).unwrap());
    });

    assert_eq!(
        bring_up(&app, &profile, &connector, &registry),
        DbState::Ready
    );
    shutdown(&app);

    let heard = heard.lock().unwrap().clone();
    let migrating = heard
        .iter()
        .position(|state| state["state"] == "migrating")
        .expect("bring-up announced the migration");
    let after: Vec<String> = heard[migrating + 1..]
        .iter()
        .map(|state| match state["detail"].as_str() {
            Some(detail) => format!("{}: {detail}", state["state"].as_str().unwrap()),
            None => state["state"].as_str().unwrap().to_owned(),
        })
        .collect();
    assert_eq!(
        after,
        [
            "starting: closing any timer left running",
            "starting: interrupting start-work steps a quit left running",
            "starting: storing the adapters' declarations",
            "starting: starting the sync engine",
            "starting: starting the nightly backup",
            "starting: registering the capture shortcut",
            "ready",
        ],
        "after the migration: {heard:?}"
    );
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
            ..Default::default()
        },
        quit: knobas_sync::scheduler::Quit::default(),
    })
    .await
    .expect("a scheduler");

    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("a mock app");
    let lifecycle = Lifecycle::with_registry(Arc::clone(&registry));
    lifecycle.hold_server(db);
    lifecycle.install(AppState::over_pool(app_pool.clone()));
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

// -- the server between start and install (#1634) ---------------------------

/// The port the server serving `root_dir` recorded in `postmaster.pid` (line 4).
fn port_of(root_dir: &std::path::Path) -> u16 {
    let pid_file = std::fs::read_to_string(root_dir.join("data").join("postmaster.pid"))
        .expect("a running server's postmaster.pid");
    pid_file.lines().nth(3).unwrap().trim().parse().unwrap()
}

fn port_answers(port: u16) -> bool {
    std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_millis(500),
    )
    .is_ok()
}

/// `profile`'s own server, started, migrated and left as `then` says.
async fn provisioned(
    profile: &Profile,
    then: impl AsyncFnOnce(&knobas_db::EmbeddedDb),
) -> knobas_db::EmbeddedDb {
    let db = knobas_db::EmbeddedDb::start(profile.db_config(None))
        .await
        .expect("the profile's server starts");
    knobas_db::migrate::run(db.pool())
        .await
        .expect("the schema is applied");
    then(&db).await;
    db
}

/// LIFE-5 (spec #1619): Cmd-Q while bring-up is migrating stops the server.
/// Until the shared state is installed the server lived in the bring-up task
/// alone, so the exit handler found nothing to stop and the postmaster
/// outlived the quit, against `docs/development.md`'s *Shutting down*.
///
/// The migration waits on a signal: the migrations table, locked by a
/// connection of the test's. The server is the profile's own, orphaned the
/// way a `kill -9` leaves it, so bring-up adopts it and takes ownership: the
/// one way a test holds a connection on it before bring-up's migration runs.
#[tokio::test(flavor = "multi_thread")]
async fn a_quit_while_migrating_stops_the_server() {
    let data = tempfile::tempdir().unwrap();
    let profile = Profile::new(false, data.path());
    let root = profile.db_config(None).root_dir;
    let orphan = provisioned(&profile, async |_| {}).await;
    let connector = orphan.connector();
    orphan.abandon();
    let port = port_of(&root);

    let mut holder = connector
        .connect()
        .await
        .expect("a connection of the test's");
    sqlx::query("begin").execute(&mut holder).await.unwrap();
    sqlx::query("lock table _sqlx_migrations in access exclusive mode")
        .execute(&mut holder)
        .await
        .expect("the migrations table is locked");

    let registry = Registry::default();
    let app = mock_app(&profile, &registry);
    let bringing = tokio::spawn({
        let handle = app.handle().clone();
        let profile = profile.clone();
        async move { knobas_app::bring_up(&handle, &profile, None, registry).await }
    });
    tokio::time::timeout(Duration::from_secs(60), async {
        while app.state::<Lifecycle>().get() != DbState::Migrating {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("bring-up reaches the migration");
    // The migration is parked on the lock: the server lists its request for
    // the migrations table, not granted (#1927). Before, a 500 ms margin stood
    // for this, and a migration slower to reach the lock was quit before it
    // got there.
    crate::support::await_lock_waiter(&mut holder, "_sqlx_migrations").await;

    shutdown(&app);
    let still_up = port_answers(port);

    // Whatever happened: the lock goes, bring-up ends, and a second quit
    // stops a server a red run left behind.
    drop(holder);
    tokio::time::timeout(Duration::from_secs(60), bringing)
        .await
        .expect("bring-up ends")
        .unwrap();
    shutdown(&app);

    assert!(
        !still_up,
        "the server answered on its port after a quit during the migration"
    );
    assert!(
        matches!(app.state::<Lifecycle>().get(), DbState::Failed { .. }),
        "a bring-up whose server was stopped under it ends failed, not ready"
    );
}

/// LIFE-6 (spec #1619): a bring-up whose migration fails stops the server
/// with the asynchronous stop, not by dropping it. Dropping a server this
/// process started runs `pg_ctl stop` synchronously in `PostgreSQL`'s `Drop`,
/// on the runtime's thread, until the postmaster has exited.
///
/// The stop is held open until the runtime's one thread does something
/// (#1809), so the witness is a fact about the thread, not a bound on a
/// scheduler gap that a loaded machine stretches. The server archives WAL
/// through a command of the test's, and a shutdown hands the archiver the
/// segment it closes and waits for it: the command marks itself waiting and
/// returns only once `go` exists. Only a task on the runtime creates `go`,
/// while bring-up still says *migrating*. Awaited, the stop leaves the thread
/// free and the task releases it at once, however slow the machine. Dropped,
/// the stop holds the thread until the command gives up on its own
/// ([`ARCHIVE_GIVES_UP`]); bring-up then says *failed* and the task never
/// released anything.
///
/// The migration fails because an applied migration's checksum no longer
/// matches its file.
#[tokio::test]
async fn a_failed_migration_stops_the_server_without_holding_the_runtime() {
    let data = tempfile::tempdir().unwrap();
    let profile = Profile::new(false, data.path());
    let root = profile.db_config(None).root_dir;
    let gate = tempfile::tempdir().unwrap();
    let archive_command = archive_gate(gate.path());
    let stopped = provisioned(&profile, async |db| {
        sqlx::query(
            "update _sqlx_migrations set checksum = '\\x00'::bytea
              where version = (select min(version) from _sqlx_migrations)",
        )
        .execute(db.pool())
        .await
        .expect("an applied migration no longer matches its file");
        // Read at the next start: bring-up's.
        for statement in [
            "alter system set archive_mode = on".to_owned(),
            format!("alter system set archive_command = '{archive_command}'"),
        ] {
            sqlx::raw_sql(sqlx::AssertSqlSafe(statement))
                .execute(db.pool())
                .await
                .expect("the archiver is configured for the next start");
        }
    })
    .await;
    stopped.stop().await.expect("the provisioning server stops");

    let registry = Registry::default();
    let app = mock_app(&profile, &registry);
    let released = Arc::new(AtomicBool::new(false));
    let releaser = tokio::spawn({
        let released = Arc::clone(&released);
        let handle = app.handle().clone();
        let gate = gate.path().to_path_buf();
        async move {
            loop {
                if handle.state::<Lifecycle>().get() == DbState::Migrating
                    && gate.join("waiting").exists()
                    && !released.swap(true, Ordering::SeqCst)
                {
                    std::fs::write(gate.join("go"), b"").expect("the stop is released");
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }
    });

    knobas_app::bring_up(app.handle(), &profile, None, registry).await;
    releaser.abort();

    assert!(
        matches!(app.state::<Lifecycle>().get(), DbState::Failed { .. }),
        "the mismatched migration fails bring-up"
    );
    assert!(
        !root.join("data").join("postmaster.pid").exists(),
        "the failed bring-up stopped its server"
    );
    assert!(
        gate.path().join("waiting").exists(),
        "the stop never waited on the archive command, so nothing here was witnessed"
    );
    assert!(
        released.load(Ordering::SeqCst),
        "the runtime's one thread did not run while the server's stop waited on it"
    );
}

/// How long the archive command waits for `go` before it returns anyway, so
/// a stop that holds the runtime's thread ends and the test can say so. Far
/// past anything an awaited stop needs: there the release comes as soon as
/// the thread runs once.
const ARCHIVE_GIVES_UP: Duration = Duration::from_secs(10);

/// An `archive_command` that marks `gate/waiting`, then returns once
/// `gate/go` exists, or after [`ARCHIVE_GIVES_UP`]. The archiver runs it
/// through `/bin/sh`.
fn archive_gate(gate: &std::path::Path) -> String {
    let script = gate.join("archive.sh");
    let polls = ARCHIVE_GIVES_UP.as_millis() / 10;
    std::fs::write(
        &script,
        format!(
            "gate=\"{gate}\"\n\
             : > \"$gate/waiting\"\n\
             i=0\n\
             while [ ! -e \"$gate/go\" ] && [ \"$i\" -lt {polls} ]; do sleep 0.01; i=$((i + 1)); done\n\
             exit 0\n",
            gate = gate.display()
        ),
    )
    .unwrap();
    format!("/bin/sh {} %p", script.display())
}

// -- a server that dies after ready (#1779) ---------------------------------

/// Runs the app's exit handler when dropped, so a failed assertion still
/// stops the server the mock app holds. A second run is a no-op.
struct QuitsOnDrop<'a>(&'a tauri::App<MockRuntime>);

impl Drop for QuitsOnDrop<'_> {
    fn drop(&mut self) {
        shutdown(self.0);
    }
}

/// How long the window may take to notice: the watch looks every few seconds.
const NOTICED_WITHIN: Duration = Duration::from_secs(30);

/// Wait until `app`'s lifecycle satisfies `done`, or panic naming `what`.
async fn until(
    app: &tauri::App<MockRuntime>,
    what: &str,
    done: impl Fn(&DbState) -> bool,
) -> DbState {
    tokio::time::timeout(NOTICED_WITHIN, async {
        loop {
            let state = app.state::<Lifecycle>().get();
            if done(&state) {
                return state;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "{what}: still {:?} after {NOTICED_WITHIN:?}",
            app.state::<Lifecycle>().get()
        )
    })
}

/// #1779: the window's own postmaster dies after bring-up reached `ready`.
/// The window used to stay `ready` over a port that no longer answered, and
/// *Retry* is only offered from `failed`, so a quit was the only way out.
/// Now the window notices and moves to `failed`, a second launch is still
/// refused because the window still owns the profile, and *Retry* starts
/// the server again on the same data directory and port, where the pool the
/// commands use reaches it.
#[tokio::test(flavor = "multi_thread")]
async fn a_server_that_dies_after_ready_fails_the_window_and_retry_restarts_it() {
    let data = tempfile::tempdir().unwrap();
    let profile = Profile::new(false, data.path());
    let root = profile.db_config(None).root_dir;
    let registry = Registry::default();
    let app = mock_app(&profile, &registry);
    let _quits = QuitsOnDrop(&app);
    knobas_app::bring_up(app.handle(), &profile, None, registry).await;
    assert_eq!(app.state::<Lifecycle>().get(), DbState::Ready);
    let port = port_of(&root);

    knobas_db::test_util::kill_server(&root);
    let failed = until(&app, "the window notices its server died", |state| {
        matches!(state, DbState::Failed { .. })
    })
    .await;
    let DbState::Failed { message } = failed else {
        unreachable!()
    };
    assert!(
        message.contains("stopped"),
        "the message says the server stopped: {message}"
    );
    let refused = app
        .state::<Lifecycle>()
        .pool()
        .expect_err("no pool while the server is down");
    assert_eq!(
        refused.message, message,
        "a command is told the server stopped, not that it never started"
    );

    match knobas_db::EmbeddedDb::start(profile.db_config(None)).await {
        Err(knobas_db::DbError::AlreadyRunning { reason, .. }) => assert!(
            reason.contains(&format!("knobas process {} ", std::process::id())),
            "the refusal names the failed window: {reason}"
        ),
        Err(other) => panic!("a second launch is refused while the window is failed, not {other}"),
        Ok(_) => panic!("a second launch was let in beside the failed window"),
    }

    knobas_app::commands::app::retry_database(app.handle().clone()).expect("retry");
    until(&app, "retry brings the server back", |state| {
        *state == DbState::Ready
    })
    .await;
    assert_eq!(port_of(&root), port, "on the same port");
    let pool = app
        .state::<Lifecycle>()
        .pool()
        .expect("the commands' pool is back");
    let (serving,): (String,) = sqlx::query_as("show data_directory")
        .fetch_one(&pool)
        .await
        .expect("the commands' pool reaches the restarted server");
    assert_eq!(
        std::fs::canonicalize(serving).unwrap(),
        std::fs::canonicalize(root.join("data")).unwrap(),
        "on the same data directory"
    );

    shutdown(&app);
    assert!(!port_answers(port), "the quit stops the restarted server");
}

/// #1779: a quit while *Retry* is restarting the server. The restart takes
/// the server out of the lifecycle for as long as it runs, so a quit that did
/// not wait for it would find nothing to stop, and the postmaster the restart
/// then started would outlive the window (the defect #1634 fixed for
/// bring-up). The exit handler waits on the restart's gate and stops what it
/// started.
///
/// The test holds the profile's bring-up lock, which every start takes, so
/// the restart stalls inside its gate until a thread lets it go. The hold
/// outlasts the scheduler's shutdown, which comes first in the quit and waits
/// up to its grace plus a pool close: a shorter hold let the restart finish
/// during that wait, and the quit then found the server held again whether
/// or not it waited on the gate.
#[tokio::test(flavor = "multi_thread")]
async fn a_quit_during_a_restart_stops_the_server_the_restart_started() {
    let data = tempfile::tempdir().unwrap();
    let profile = Profile::new(false, data.path());
    let root = profile.db_config(None).root_dir;
    let registry = Registry::default();
    let app = mock_app(&profile, &registry);
    let _quits = QuitsOnDrop(&app);
    knobas_app::bring_up(app.handle(), &profile, None, registry).await;
    assert_eq!(app.state::<Lifecycle>().get(), DbState::Ready);
    let port = port_of(&root);

    knobas_db::test_util::kill_server(&root);
    until(&app, "the window notices its server died", |state| {
        matches!(state, DbState::Failed { .. })
    })
    .await;

    let bring_up_lock = std::fs::File::create(root.join(".bring-up.lock")).unwrap();
    bring_up_lock.lock().expect("the bring-up lock is free");
    knobas_app::commands::app::retry_database(app.handle().clone()).expect("retry");
    until(&app, "the restart begins", |state| {
        matches!(state, DbState::Starting { detail: Some(_) })
    })
    .await;
    let release = std::thread::spawn(move || {
        std::thread::sleep(knobas_sync::scheduler::SHUTDOWN_GRACE + Duration::from_secs(5));
        drop(bring_up_lock);
    });

    shutdown(&app);
    release.join().unwrap();
    // Settled first: a quit that did not wait returned before the restart
    // started the server it then leaves running.
    until(&app, "the restart settles", |state| {
        !matches!(state, DbState::Starting { .. })
    })
    .await;
    assert!(
        !port_answers(port),
        "the quit waits for the restart and stops the server it started"
    );
    assert!(
        !root.join("data").join("postmaster.pid").exists(),
        "no postmaster is left on the data directory"
    );
}

// -- a trigger racing the quit (#1633) --------------------------------------

/// Events that hold the first `running` announcement -- the one a trigger
/// makes after it has opened the run's row and before it spawns the run --
/// until the test lets it go.
struct HoldsTheStart {
    entered: std::sync::mpsc::SyncSender<()>,
    release: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
}

impl SyncEvents for HoldsTheStart {
    fn sync_state(&self, status: knobas_sync::SourceSyncStatus) {
        if !status.running {
            return;
        }
        let Some(release) = self.release.lock().unwrap().take() else {
            return;
        };
        let _ = self.entered.send(());
        let _ = release.recv_timeout(Duration::from_secs(30));
    }
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
}

/// SYNC-5 (#1633, spec #1619): *Sync now* is being processed when Cmd-Q
/// arrives. The trigger opened the run's row before the quit began and
/// reaches the spawn after it: it answers `ShuttingDown` and ends the row,
/// with the database still open for it. Before, it spawned a run the quit had
/// already stopped waiting for, which settled on a closed pool and left the
/// row open until the next start reconciled it.
#[tokio::test(flavor = "multi_thread")]
async fn a_trigger_that_reaches_the_spawn_after_the_quit_ends_its_row() {
    let connector = knobas_db::test_util::scratch_database("trigger-racing-quit").await;
    let app_pool = connector.pool(2).await.unwrap();
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
    let registry: Arc<dyn AdapterRegistry> = Arc::new(Recording {
        entered: Arc::new(tokio::sync::Notify::new()),
        stops: Stops::default(),
        app_pool: app_pool.clone(),
    });
    let (entered, held) = std::sync::mpsc::sync_channel(1);
    let (release, released) = std::sync::mpsc::channel();
    let scheduler = knobas_sync::scheduler::Scheduler::start(SchedulerDeps {
        pool: connector.pool(2).await.unwrap(),
        connections: Arc::new(Connections(connector.clone())),
        registry: Arc::clone(&registry),
        secrets: Arc::clone(&secrets),
        events: Arc::new(HoldsTheStart {
            entered,
            release: Mutex::new(Some(released)),
        }),
        timing: knobas_sync::scheduler::SchedulerTiming {
            startup_delay: Duration::from_secs(3600),
            tick: Duration::from_secs(3600),
            // A quit that waits at the door waits for the trigger, not for
            // its grace to run out under a loaded machine.
            shutdown_grace: Duration::from_secs(3600),
        },
        quit: knobas_sync::scheduler::Quit::default(),
    })
    .await
    .expect("a scheduler");
    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("a mock app");
    app.manage(SourcesState {
        pool: app_pool,
        scheduler: scheduler.clone(),
        secrets,
        registry,
    });

    let triggered = tokio::spawn({
        let scheduler = scheduler.clone();
        async move {
            scheduler
                .trigger(PARKED, knobas_sync::SyncTrigger::Manual, None)
                .await
        }
    });
    tokio::task::block_in_place(|| held.recv_timeout(Duration::from_secs(30)))
        .expect("the trigger opens its row and announces it");

    let quitting = std::thread::spawn({
        let handle = app.handle().clone();
        move || knobas_app::shutdown(&handle)
    });
    tokio::time::timeout(Duration::from_secs(30), async {
        while !scheduler.is_shutting_down() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the quit begins");
    // The trigger is let go once the quit waits at the scheduler's door for
    // it, or once a quit that does not wait is over (#1927). Before, a 300 ms
    // margin stood for the second, and a quit that did not wait but took
    // longer than that to close the pool passed.
    tokio::time::timeout(Duration::from_secs(30), async {
        while !quitting.is_finished() && !quit_waits_at_the_door(&scheduler) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the quit neither waits at the door nor ends");
    release.send(()).unwrap();

    let answered = tokio::time::timeout(Duration::from_secs(30), triggered)
        .await
        .expect("the trigger answers")
        .unwrap();
    quitting.join().expect("the quit finishes");

    let mut reader = connector.connect().await.unwrap();
    let open: i64 = sqlx::query_scalar(
        "select count(*) from knobas.sync_run where source_id = $1 and finished_at is null",
    )
    .bind(PARKED)
    .fetch_one(&mut reader)
    .await
    .unwrap();
    assert_eq!(open, 0, "the trigger left its run row open");
    assert!(
        matches!(
            answered,
            Err(knobas_sync::scheduler::TriggerError::ShuttingDown)
        ),
        "a trigger that reached the spawn after the quit answered {answered:?}"
    );
}

/// Whether the quit is queued at the scheduler's door, behind a trigger that
/// holds it.
///
/// A trigger takes the door for reading before anything else, and the quit
/// takes it for writing. Tokio's `RwLock` is fair: a reader asking while a
/// writer is queued waits behind it. So a probe trigger that does not answer
/// on its first poll is the quit waiting at the door; with nobody queued it
/// answers at once, `ShuttingDown`, having opened nothing. Unconstrained, so
/// tokio's cooperative budget cannot make the poll pending for its own
/// reasons. The probe is dropped either way, and a dropped wait leaves the
/// lock's queue.
fn quit_waits_at_the_door(scheduler: &knobas_sync::scheduler::Scheduler) -> bool {
    use std::future::Future as _;

    let probe = tokio::task::unconstrained(scheduler.trigger(
        PARKED,
        knobas_sync::SyncTrigger::Manual,
        None,
    ));
    let mut probe = std::pin::pin!(probe);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    match probe.as_mut().poll(&mut cx) {
        std::task::Poll::Pending => true,
        std::task::Poll::Ready(answer) => {
            assert!(
                matches!(
                    answer,
                    Err(knobas_sync::scheduler::TriggerError::ShuttingDown)
                ),
                "a trigger during the quit answered {answer:?}"
            );
            false
        }
    }
}

/// LIFE-4 and CORE-3 (spec #1619, rulings B2 and B6): a second `--demo` launch
/// gets past the single-instance guard, which leaves the demo profile out, and
/// used to borrow the first one's server: its scheduler start interrupted the
/// first window's runs, and the first window's quit stopped the server under
/// it. Bring-up now ends in `Failed`, and the boot screen's message names the
/// profile and the process to quit, before anything touched the database.
#[tokio::test(flavor = "multi_thread")]
async fn a_second_demo_launch_on_a_live_owner_is_refused_naming_the_profile_and_the_owner() {
    let data = tempfile::tempdir().unwrap();
    let profile = Profile::new(true, data.path());
    let owner = knobas_db::EmbeddedDb::start(profile.db_config(None))
        .await
        .expect("the first window's server");
    let port = port_of(&profile.db_config(None).root_dir);

    let registry = Registry::default();
    let app = mock_app(&profile, &registry);
    knobas_app::bring_up(app.handle(), &profile, None, registry).await;

    let message = match app.state::<Lifecycle>().get() {
        DbState::Failed { message } => message,
        other => panic!("a second launch on a live owner must fail, not reach {other:?}"),
    };
    assert!(
        message.starts_with("demo profile: "),
        "the message names the profile: {message}"
    );
    // The first window is this test's own handle, so its pid is this
    // process's.
    assert!(
        message.contains(&format!("knobas process {} ", std::process::id())),
        "the message names the window to quit: {message}"
    );
    assert!(
        port_answers(port),
        "the refused launch left the first window's server up"
    );

    shutdown(&app);
    assert!(
        port_answers(port),
        "the refused window's quit stops nothing"
    );
    owner.stop().await.unwrap();
}
