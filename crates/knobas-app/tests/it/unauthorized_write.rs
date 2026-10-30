//! A write whose credential the source rejected waits for a person (#1628):
//! a new credential on its source, or *Flush now* from the queue panel. The
//! tick never sends it again on its own.
//!
//! Driven through the app's own seams -- `crud::set_secret`, which
//! `set_source_secret` calls, and `sources::write_queue::flush_now`, which
//! `flush_writes` calls -- against a trait-level fake that answers `401`
//! until it is told otherwise and counts every call. The tick is
//! `flush_all`, called by hand: the real scheduler's tick is held off, so
//! every send below is one the test asked for. Prior art:
//! `background_write.rs`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use knobas_app::sources::{AccountInput, SecretInput, SourcesState};
use knobas_core::write_queue::{self as store, WaitReason, WriteState};
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::{
    AuthMethod, ConnectionInfo, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError,
    WriteOp, WriteReceipt,
};
use knobas_sync::scheduler::{AdapterRegistry, RunConnections, SchedulerDeps, SyncEvents};
use serde_json::json;

/// The adapter kind every source of this file is built from.
const KIND: &str = "kuma";

/// What one source's fake does and has been asked.
#[derive(Default)]
struct Line {
    /// Answer `401` while set.
    refuses: Mutex<bool>,
    /// Every `write` call, whatever it answered.
    asked: Mutex<usize>,
}

struct Fake {
    line: Arc<Line>,
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
        Ok(cursor.unwrap_or_default())
    }

    async fn write(&self, _op: WriteOp) -> Result<WriteReceipt, SourceError> {
        *self.line.asked.lock().unwrap() += 1;
        if *self.line.refuses.lock().unwrap() {
            return Err(SourceError::Unauthorized { status: Some(401) });
        }
        Ok(WriteReceipt::none())
    }

    async fn reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError> {
        Err(SourceError::protocol(format!("no workflow for {entity:?}")))
    }
}

fn descriptor() -> SourceDescriptor {
    SourceDescriptor {
        id: KIND.to_owned(),
        adapter_kind: KIND.to_owned(),
        name: "Uptime Kuma".to_owned(),
        capabilities: vec![knobas_source::Capability::Write],
        adapter_version: "0".to_owned(),
        auth_methods: vec![AuthMethod::Pat],
        // The account is Kuma's second credential: the one its writes use.
        accepts_account: true,
        min_sync_interval_secs: 60,
        roles: Vec::new(),
        source_actions: Vec::new(),
        clone_from: Vec::new(),
        url_forms: Vec::new(),
        write_ops: vec!["comment".to_owned()],
        entity_kinds: vec![KindInfo {
            id: "monitor".to_owned(),
            label: "Monitor".to_owned(),
            plural: "Monitors".to_owned(),
            monogram: "MO".to_owned(),
            full_sync_exhaustive: true,
            refreshes: true,
        }],
        config_schema: json!({"type": "object", "properties": {}}),
        payload_paths: Vec::new(),
    }
}

struct Registry {
    lines: HashMap<String, Arc<Line>>,
}

impl AdapterRegistry for Registry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        vec![descriptor()]
    }

    fn build(
        &self,
        instance: knobas_source::instance::SourceInstance,
    ) -> Result<Box<dyn Source>, SourceError> {
        Ok(Box::new(Fake {
            line: Arc::clone(&self.lines[&instance.id]),
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

struct Silent;

impl SyncEvents for Silent {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
}

struct Fixture {
    /// This test's source. The write queue's flush gate is process-wide and
    /// keyed by source id, so tests that share an id share a release count
    /// and one test's credential save reads as a stale refusal in another's
    /// flush (#1752): each fixture names its own.
    source: String,
    /// A second source of this test's, whose credential is not `source`'s.
    other: String,
    state: SourcesState,
    lines: HashMap<String, Arc<Line>>,
}

impl Fixture {
    fn asked(&self, source: &str) -> usize {
        *self.lines[source].asked.lock().unwrap()
    }

    fn refuses(&self, source: &str, refuses: bool) {
        *self.lines[source].refuses.lock().unwrap() = refuses;
    }

    /// The scheduler's tick, by hand.
    async fn tick(&self) {
        knobas_sync::write_queue::flush_all(self.state.scheduler.deps()).await;
    }

    /// A comment on a monitor of `source`, submitted while the source refuses
    /// the credential: it comes back waiting `unauthorized`.
    async fn unauthorized_comment(&self, source: &str, body: &str) -> i64 {
        let entity = format!("{source}:7");
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'monitor', 'api')")
            .bind(&entity)
            .execute(&self.state.pool)
            .await
            .expect("the monitor is mirrored");
        self.refuses(source, true);
        let queued = knobas_app::sources::write_queue::submit(
            &self.state,
            json!({ "Comment": { "entity": entity, "body": body } }),
        )
        .await
        .expect("the comment is queued");
        let row = self.row(queued.id).await;
        assert_eq!(row.wait_reason, Some(WaitReason::Unauthorized), "{row:?}");
        queued.id
    }

    async fn row(&self, id: i64) -> store::QueuedWrite {
        store::get(&self.state.pool, id)
            .await
            .expect("the queue reads")
            .expect("the row is there")
    }

    async fn save(&self, source: &str, secret: SecretInput) {
        knobas_app::sources::crud::set_secret(
            &self.state.pool,
            &self.state.secrets,
            &self.state.registry,
            &self.state.live,
            source,
            secret,
        )
        .await
        .expect("the credential is saved");
    }
}

async fn fixture(name: &str) -> Fixture {
    let connector = knobas_db::test_util::scratch_database(name).await;
    let pool = connector.pool(4).await.expect("a pool");

    let source = format!("kuma-{name}");
    let other = format!("kuma-{name}-two");
    let secrets = Arc::new(MemoryStore::new());
    let mut lines = HashMap::new();
    for id in [&source, &other] {
        secrets
            .put(
                &knobas_secrets::KeychainAccount::source(id),
                &Secret::just(AuthMethod::Pat, "an-api-key"),
            )
            .expect("the credential is stored");
        knobas_sync::config::insert(
            &pool,
            &knobas_sync::config::InsertConfig {
                id: id.clone(),
                adapter_kind: KIND.to_owned(),
                display_name: id.clone(),
                base_url: "https://kuma.example".to_owned(),
                auth_kind: knobas_sync::config::AuthKind::Method(AuthMethod::Pat),
                config: json!({}),
                sync_interval_secs: 86_400,
                enabled: true,
            },
        )
        .await
        .expect("the source row is written");
        lines.insert(id.clone(), Arc::new(Line::default()));
    }

    let registry: Arc<dyn AdapterRegistry> = Arc::new(Registry {
        lines: lines.clone(),
    });
    let scheduler = knobas_sync::scheduler::Scheduler::start(SchedulerDeps {
        pool: pool.clone(),
        connections: Arc::new(Connections(connector)),
        registry: Arc::clone(&registry),
        secrets: secrets.clone(),
        events: Arc::new(Silent),
        timing: knobas_sync::scheduler::SchedulerTiming {
            startup_delay: Duration::from_secs(3600),
            tick: Duration::from_secs(3600),
            ..Default::default()
        },
        quit: knobas_sync::scheduler::Quit::default(),
    })
    .await
    .expect("a scheduler");

    Fixture {
        source,
        other,
        state: SourcesState {
            live: Default::default(),
            pool,
            scheduler,
            secrets,
            registry,
        },
        lines,
    }
}

/// A new secret on the source releases its unauthorized write and the next tick
/// sends it; a new secret on another source releases nothing.
#[tokio::test]
async fn a_new_secret_releases_the_unauthorized_write_on_its_own_source() {
    let f = fixture("unauthorized_write_secret").await;
    let write = f
        .unauthorized_comment(&f.source, "after the key was rotated")
        .await;
    f.refuses(&f.source, false);

    f.tick().await;
    f.tick().await;
    assert_eq!(
        f.asked(&f.source),
        1,
        "the tick does not re-send a rejected credential"
    );

    f.save(&f.other, SecretInput::of("another-key")).await;
    f.tick().await;
    assert_eq!(
        f.asked(&f.source),
        1,
        "another source's credential is not this one's"
    );

    f.save(&f.source, SecretInput::of("a-new-key")).await;
    f.tick().await;
    assert_eq!(f.asked(&f.source), 2);
    assert_eq!(f.row(write).await.state, WriteState::Sent);
}

/// Kuma's case: the account is what its writes log in with, and saving it
/// alone (the API key kept) releases the write too.
#[tokio::test]
async fn a_new_account_releases_the_unauthorized_write() {
    let f = fixture("unauthorized_write_account").await;
    let write = f
        .unauthorized_comment(&f.source, "after the password changed")
        .await;
    f.refuses(&f.source, false);

    f.tick().await;
    assert_eq!(f.asked(&f.source), 1);

    f.save(
        &f.source,
        SecretInput {
            value: None,
            account: Some(AccountInput {
                username: "admin".to_owned(),
                password: "a-new-password".to_owned(),
            }),
        },
    )
    .await;
    f.tick().await;
    assert_eq!(f.asked(&f.source), 2);
    assert_eq!(f.row(write).await.state, WriteState::Sent);
}

/// *Flush now* sends an unauthorized write once per press, and the tick after a
/// press that was refused again still sends nothing.
#[tokio::test]
async fn flush_now_sends_an_unauthorized_write_once() {
    let f = fixture("unauthorized_write_flush_now").await;
    let write = f.unauthorized_comment(&f.source, "pressed").await;

    knobas_app::sources::write_queue::flush_now(&f.state, Some(f.source.clone()))
        .await
        .expect("the flush runs");
    assert_eq!(f.asked(&f.source), 2, "one send for the press");
    assert_eq!(
        f.row(write).await.wait_reason,
        Some(WaitReason::Unauthorized)
    );

    f.refuses(&f.source, false);
    f.tick().await;
    assert_eq!(f.asked(&f.source), 2, "a press is not a release");

    knobas_app::sources::write_queue::flush_now(&f.state, None)
        .await
        .expect("the flush runs");
    assert_eq!(f.asked(&f.source), 3);
    assert_eq!(f.row(write).await.state, WriteState::Sent);
}

/// A new address or configuration releases the write too: an adapter that
/// would not build waits under the same reason as a refused credential, and
/// the edit is the person acting on it. A rename is not.
#[tokio::test]
async fn an_edit_of_where_the_source_is_releases_the_unauthorized_write() {
    let f = fixture("unauthorized_write_edit").await;
    let write = f.unauthorized_comment(&f.source, "after the move").await;
    f.refuses(&f.source, false);

    let edit = |patch: knobas_app::sources::SourcePatch| {
        knobas_app::sources::crud::update(
            &f.state.pool,
            f.state.registry.as_ref(),
            &f.source,
            patch,
        )
    };
    edit(knobas_app::sources::SourcePatch {
        display_name: Some("Kuma, renamed".to_owned()),
        base_url: None,
        config: None,
        sync_interval_secs: None,
        enabled: None,
    })
    .await
    .expect("the rename is saved");
    f.tick().await;
    assert_eq!(f.asked(&f.source), 1, "a rename releases nothing");

    edit(knobas_app::sources::SourcePatch {
        display_name: None,
        base_url: Some("https://status.example".to_owned()),
        config: None,
        sync_interval_secs: None,
        enabled: None,
    })
    .await
    .expect("the address is saved");
    f.tick().await;
    assert_eq!(f.asked(&f.source), 2);
    assert_eq!(f.row(write).await.state, WriteState::Sent);
}
