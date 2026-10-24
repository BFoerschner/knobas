//! A write pressed while the keychain is locked waits it out, as the sync
//! does (#1944). It is queued, not refused at the button, it climbs the
//! ladder as `unreachable` with a `keychain:` detail (#1741), a credential
//! saved after the keychain is unlocked sends it at once, and a quit while
//! its flush is reading the keychain sends nothing and leaves it for the
//! next start.
//!
//! Driven through the app's own seams: `sources::write_queue::submit` and
//! `submit_in_background` (what `submit_write` calls), `crud::set_secret`
//! (`set_source_secret`) and `Scheduler::shutdown` (the quit), over a
//! keychain fake behind the `SecretStore` seam that can be locked, and whose
//! read can be held at a door the way a keychain prompt holds it. The real
//! keychain is never touched. The tick is `flush_all`, called by hand.
//! Prior art: `unauthorized_write.rs`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use knobas_app::sources::{AccountInput, SecretInput, SourcesState};
use knobas_core::write_queue::{self as store, WaitReason, WriteState};
use knobas_secrets::{KeychainAccount, MemoryStore, Secret, SecretError, SecretStore};
use knobas_source::{
    AuthMethod, ConnectionInfo, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError,
    WriteOp, WriteReceipt,
};
use knobas_sync::scheduler::{AdapterRegistry, RunConnections, SchedulerDeps, SyncEvents};
use serde_json::json;

/// The adapter kind every source of this file is built from.
const KIND: &str = "kuma";

/// How long a test waits for something it knows will happen.
const PATIENCE: Duration = Duration::from_secs(60);

/// What the fake sources have been asked to write, by every instance.
type Asked = Arc<Mutex<Vec<WriteOp>>>;

/// A Kuma's shape: its kind offers nothing to write, and an instance with
/// an account beside its key offers pause and resume (#452).
fn descriptor(account: bool) -> SourceDescriptor {
    SourceDescriptor {
        id: KIND.to_owned(),
        adapter_kind: KIND.to_owned(),
        name: "Uptime Kuma".to_owned(),
        capabilities: if account {
            vec![knobas_source::Capability::Write]
        } else {
            Vec::new()
        },
        adapter_version: "0".to_owned(),
        auth_methods: vec![AuthMethod::Pat],
        accepts_account: true,
        min_sync_interval_secs: 60,
        roles: vec![knobas_source::role::RoleClaim::new(
            knobas_source::role::MONITORING,
            [],
        )],
        source_actions: Vec::new(),
        clone_from: Vec::new(),
        url_forms: Vec::new(),
        write_ops: if account {
            vec!["pause_monitor".to_owned(), "resume_monitor".to_owned()]
        } else {
            Vec::new()
        },
        entity_kinds: vec![KindInfo {
            id: "monitor".to_owned(),
            label: "Monitor".to_owned(),
            plural: "Monitors".to_owned(),
            monogram: "MO".to_owned(),
            full_sync_exhaustive: true,
        }],
        config_schema: json!({"type": "object", "properties": {}}),
        payload_paths: Vec::new(),
    }
}

struct Fake {
    account: bool,
    asked: Asked,
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
        descriptor(self.account)
    }

    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        Ok(ConnectionInfo::default())
    }

    async fn sync(
        &self,
        cursor: Option<Cursor>,
        _sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        Ok(cursor.unwrap_or_default())
    }

    async fn write(&self, op: WriteOp) -> Result<WriteReceipt, SourceError> {
        self.asked.lock().unwrap().push(op);
        Ok(WriteReceipt::none())
    }

    async fn reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError> {
        Err(SourceError::protocol(format!("no workflow for {entity:?}")))
    }
}

struct Registry {
    asked: Asked,
}

impl AdapterRegistry for Registry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        vec![descriptor(false)]
    }

    fn build(
        &self,
        instance: knobas_source::instance::SourceInstance,
    ) -> Result<Box<dyn Source>, SourceError> {
        Ok(Box::new(Fake {
            account: instance.account.is_some(),
            asked: Arc::clone(&self.asked),
        }))
    }
}

/// A door on a read of the locked keychain: the read says it has begun, then
/// waits to be let go before it fails.
struct Door {
    /// How many locked reads go past before the one held here.
    passing: usize,
    reached: std::sync::mpsc::Sender<()>,
    open: std::sync::mpsc::Receiver<()>,
}

/// The keychain, lockable. Locked, a read fails as a locked macOS keychain
/// fails one, `errSecInteractionNotAllowed` arriving as `Backend`.
#[derive(Clone, Default)]
struct Keychain {
    inner: Arc<MemoryStore>,
    locked: Arc<Mutex<bool>>,
    door: Arc<Mutex<Option<Door>>>,
}

impl Keychain {
    fn lock(&self) {
        *self.locked.lock().unwrap() = true;
    }

    fn unlock(&self) {
        *self.locked.lock().unwrap() = false;
    }

    /// Hold the locked read after the next `passing`: the first receiver
    /// hears it begin, and a send on the second lets it fail.
    fn hold_a_read(
        &self,
        passing: usize,
    ) -> (std::sync::mpsc::Receiver<()>, std::sync::mpsc::Sender<()>) {
        let (reached, heard) = std::sync::mpsc::channel();
        let (open, opened) = std::sync::mpsc::channel();
        *self.door.lock().unwrap() = Some(Door {
            passing,
            reached,
            open: opened,
        });
        (heard, open)
    }
}

impl SecretStore for Keychain {
    fn get(&self, account: &KeychainAccount) -> Result<Option<Secret>, SecretError> {
        if *self.locked.lock().unwrap() {
            // On the store's own blocking thread, as a keychain prompt is.
            let door = {
                let mut door = self.door.lock().unwrap();
                match door.as_mut() {
                    Some(held) if held.passing > 0 => {
                        held.passing -= 1;
                        None
                    }
                    _ => door.take(),
                }
            };
            if let Some(door) = door {
                let _ = door.reached.send(());
                let _ = door.open.recv();
            }
            return Err(SecretError::Backend(
                "User interaction is not allowed.".to_owned(),
            ));
        }
        self.inner.get(account)
    }

    fn put(&self, account: &KeychainAccount, secret: &Secret) -> Result<(), SecretError> {
        self.inner.put(account, secret)
    }

    fn delete(&self, account: &KeychainAccount) -> Result<(), SecretError> {
        self.inner.delete(account)
    }
}

struct Connections(knobas_db::embedded::Connector);

#[async_trait]
impl RunConnections for Connections {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        self.0.connect().await
    }
}

struct Silent;

impl SyncEvents for Silent {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
}

struct Fixture {
    connector: knobas_db::embedded::Connector,
    /// This test's source: the flush gate is process-wide and keyed by
    /// source id, so each test names its own (#1752).
    source: String,
    state: SourcesState,
    keychain: Keychain,
    asked: Asked,
}

/// The app's sources state over `connector`, its tick held off for an hour
/// and its quit's grace an hour long: a quit that ends at all waited for
/// nothing it could cut.
async fn state_on(
    connector: &knobas_db::embedded::Connector,
    keychain: &Keychain,
    asked: &Asked,
) -> SourcesState {
    let pool = connector.pool(4).await.expect("a pool");
    let registry: Arc<dyn AdapterRegistry> = Arc::new(Registry {
        asked: Arc::clone(asked),
    });
    let secrets: Arc<dyn SecretStore> = Arc::new(keychain.clone());
    let scheduler = knobas_sync::scheduler::Scheduler::start(SchedulerDeps {
        pool: pool.clone(),
        connections: Arc::new(Connections(connector.clone())),
        registry: Arc::clone(&registry),
        secrets: Arc::clone(&secrets),
        events: Arc::new(Silent),
        timing: knobas_sync::scheduler::SchedulerTiming {
            startup_delay: Duration::from_secs(3600),
            tick: Duration::from_secs(3600),
            shutdown_grace: Duration::from_secs(3600),
        },
        quit: knobas_sync::scheduler::Quit::default(),
    })
    .await
    .expect("a scheduler");
    SourcesState {
        pool,
        scheduler,
        secrets,
        registry,
    }
}

/// A Kuma with its key and the account beside it, and one of its monitors
/// mirrored.
async fn fixture(name: &str) -> Fixture {
    let connector = knobas_db::test_util::scratch_database(name).await;
    let source = format!("kuma-{name}");
    let keychain = Keychain::default();
    keychain
        .put(
            &KeychainAccount::source(&source),
            &Secret {
                kind: AuthMethod::Pat,
                value: "an-api-key".to_owned(),
                account: Some(knobas_source::instance::Account {
                    username: "admin".to_owned(),
                    password: "a-password".to_owned(),
                }),
            },
        )
        .expect("the credential is stored");
    let asked = Asked::default();
    let state = state_on(&connector, &keychain, &asked).await;
    knobas_sync::config::insert(
        &state.pool,
        &knobas_sync::config::InsertConfig {
            id: source.clone(),
            adapter_kind: KIND.to_owned(),
            display_name: source.clone(),
            base_url: "https://kuma.example".to_owned(),
            auth_kind: knobas_sync::config::AuthKind::Method(AuthMethod::Pat),
            config: json!({}),
            sync_interval_secs: 86_400,
            enabled: true,
        },
    )
    .await
    .expect("the source row is written");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'monitor', 'api')")
        .bind(format!("{source}:7"))
        .execute(&state.pool)
        .await
        .expect("the monitor is mirrored");
    Fixture {
        connector,
        source,
        state,
        keychain,
        asked,
    }
}

impl Fixture {
    fn pause(&self) -> serde_json::Value {
        json!({ "PauseMonitor": { "entity": format!("{}:7", self.source) } })
    }

    fn asked(&self) -> usize {
        self.asked.lock().unwrap().len()
    }

    async fn row(&self, pool: &sqlx::PgPool, id: i64) -> store::QueuedWrite {
        store::get(pool, id)
            .await
            .expect("the queue reads")
            .expect("the row is there")
    }
}

/// #1944: a pause pressed while the keychain is locked is queued where it
/// used to be refused with "does not offer". It waits the keychain out on
/// the ladder, and saving the credential once the keychain is open sends it
/// with the next tick, not a rung later.
#[tokio::test]
async fn a_pause_pressed_while_the_keychain_is_locked_goes_once_the_credential_is_saved() {
    let f = fixture("keychain_write_save").await;

    f.keychain.lock();
    let queued = knobas_app::sources::write_queue::submit(&f.state, f.pause())
        .await
        .expect("the pause is queued, not refused");
    let waiting = f.row(&f.state.pool, queued.id).await;
    assert_eq!(waiting.state, WriteState::Pending);
    assert_eq!(waiting.wait_reason, Some(WaitReason::Unreachable));
    assert_eq!(
        waiting.detail.as_deref(),
        Some("keychain: User interaction is not allowed.")
    );
    assert_eq!(f.asked(), 0, "the source is not asked without a credential");

    f.keychain.unlock();
    knobas_sync::write_queue::flush_all(f.state.scheduler.deps()).await;
    assert_eq!(f.asked(), 0, "the write waits out its rung");

    knobas_app::sources::crud::set_secret(
        &f.state.pool,
        &f.state.secrets,
        &f.state.registry,
        &f.source,
        SecretInput {
            value: None,
            account: Some(AccountInput {
                username: "admin".to_owned(),
                password: "a-password".to_owned(),
            }),
        },
    )
    .await
    .expect("the credential is saved");
    knobas_sync::write_queue::flush_all(f.state.scheduler.deps()).await;
    assert_eq!(f.asked(), 1);
    assert_eq!(
        f.row(&f.state.pool, queued.id).await.state,
        WriteState::Sent
    );
}

/// #1944 against #1933 and #1938: a pause queued while the keychain is
/// locked is sent in the background set, so the quit waits for its flush.
/// The quit begins while that flush is reading the keychain (held at the
/// door, as a prompt holds it). The flush then starts nothing (#1924): the
/// row is left untried, nothing reaches the source, and the next start,
/// keychain open, sends it once.
#[tokio::test(flavor = "multi_thread")]
async fn a_pause_queued_while_the_keychain_is_locked_sends_nothing_at_quit_and_goes_next_start() {
    let f = fixture("keychain_write_quit").await;

    f.keychain.lock();
    // The submit's own read, which finds the instance's ops unknown, goes
    // past; the background flush's is held.
    let (reading, let_it_fail) = f.keychain.hold_a_read(1);
    let queued = knobas_app::sources::write_queue::submit_in_background(&f.state, f.pause())
        .await
        .expect("the pause is queued, not refused");
    tokio::task::spawn_blocking(move || reading.recv_timeout(PATIENCE))
        .await
        .unwrap()
        .expect("the background flush reads the keychain");

    let quitting = tokio::spawn({
        let scheduler = f.state.scheduler.clone();
        async move { scheduler.shutdown().await }
    });
    tokio::time::timeout(PATIENCE, async {
        while !f.state.scheduler.is_shutting_down() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the quit begins");
    let_it_fail.send(()).unwrap();
    tokio::time::timeout(PATIENCE, quitting)
        .await
        .expect("the quit waited out its grace: the flush did not end at the keychain")
        .unwrap();

    let reader = f.connector.pool(1).await.unwrap();
    let left = f.row(&reader, queued.id).await;
    assert_eq!(left.state, WriteState::Pending);
    assert_eq!(left.wait_reason, None, "the quit recorded no wait");
    assert_eq!(left.attempts, 0);
    assert_eq!(f.asked(), 0, "nothing reached the source");

    f.keychain.unlock();
    let next = state_on(&f.connector, &f.keychain, &f.asked).await;
    knobas_sync::write_queue::flush_all(next.scheduler.deps()).await;
    assert_eq!(f.asked(), 1, "the next start sends it once");
    assert_eq!(f.row(&reader, queued.id).await.state, WriteState::Sent);
    next.scheduler.shutdown().await;
}
