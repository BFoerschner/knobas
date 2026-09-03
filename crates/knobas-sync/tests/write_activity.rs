//! The write queue's narration under repetition (issue #42, story 20).
//!
//! Story 20 promises one activity line per *change*. The scheduler retries a
//! waiting write every tick, so a source that stays down has to read as one
//! line of patience -- not as a line per attempt, which would bury the day's
//! record under hundreds of identical entries an hour while a laptop is
//! offline. A *different* fault is news and is announced again; the row
//! itself still records every attempt.
//!
//! In its own binary, deliberately: each test binary gets its own embedded
//! database, and this is the one place in the queue's suite that counts
//! activity lines across several flushes. Inside `write_queue.rs` it would
//! share a database with `the_scheduler_drains_the_queue_on_its_own`, whose
//! real scheduler calls `flush_all` and reaches *every* configured source --
//! its failed builds of the other tests' sources re-reason their rows
//! (`unauthorized`, no stored credential in *its* secret store), and a count
//! of "waiting" lines taken in that weather is a count of somebody else's.

use std::sync::{Arc, Mutex};

use knobas_core::entity::EntityRef;
use knobas_core::write_queue::{self as store, WaitReason};
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::instance::SourceInstance;
use knobas_source::{
    AuthMethod, ConnectionInfo, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError,
    WriteOp,
};
use knobas_sync::config::{self, AuthKind, InsertConfig};
use knobas_sync::scheduler::{AdapterRegistry, RunConnections, SchedulerDeps, SyncEvents};
use knobas_sync::write_queue as flusher;

/// What the fake source does when it is asked to write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Answer {
    /// The write lands.
    Accept,
    /// The server did not answer -- a fault that will pass.
    Unreachable,
    /// The credential was refused -- a fault that will pass once a human acts.
    Unauthorized,
}

/// A `Source` that answers to order. The trimmed sibling of `write_queue.rs`'s
/// fake: this file's assertions are about the narration, not the deliveries.
struct Fake {
    id: String,
    answer: Arc<Mutex<Answer>>,
}

#[async_trait::async_trait]
impl Source for Fake {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_kind: "mock".to_owned(),
            name: "Fake".to_owned(),
            capabilities: Vec::new(),
            adapter_version: "0".to_owned(),
            auth_methods: vec![AuthMethod::Pat],
            write_ops: vec!["comment".to_owned()],
            entity_kinds: vec![KindInfo {
                id: "ticket".to_owned(),
                label: "Ticket".to_owned(),
                plural: "Tickets".to_owned(),
                monogram: "TI".to_owned(),
                full_sync_exhaustive: true,
            }],
            config_schema: serde_json::json!({"type": "object", "properties": {}}),
            // Nothing declared: this stand-in has no payload shapes to
            // read, so every path-driven read misses on it (#277).
            payload_paths: Vec::new(),
        }
    }

    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        Ok(ConnectionInfo {
            account: None,
            server_version: None,
            secret_expires_at: None,
            detail: None,
            discovered: std::collections::BTreeMap::new(),
        })
    }

    async fn sync(
        &self,
        cursor: Option<Cursor>,
        _sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        Ok(cursor.unwrap_or_default())
    }

    async fn write(&self, _op: WriteOp) -> Result<knobas_source::WriteReceipt, SourceError> {
        Err(match *self.answer.lock().unwrap() {
            Answer::Accept => return Ok(knobas_source::WriteReceipt::none()),
            Answer::Unreachable => SourceError::Unreachable("connection timed out".to_owned()),
            Answer::Unauthorized => SourceError::Unauthorized { status: Some(401) },
        })
    }
}

struct FakeRegistry {
    answer: Arc<Mutex<Answer>>,
}

impl AdapterRegistry for FakeRegistry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        Vec::new()
    }

    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        Ok(Box::new(Fake {
            id: instance.id,
            answer: Arc::clone(&self.answer),
        }))
    }
}

/// A newtype, because the orphan rule forbids implementing this crate's trait
/// for `knobas_db`'s `Connector` from a test binary that owns neither.
struct TestConnections(knobas_db::embedded::Connector);

#[async_trait::async_trait]
impl RunConnections for TestConnections {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        self.0.connect().await
    }
}

#[derive(Default)]
struct Recorder {
    activity: Mutex<Vec<knobas_core::activity::ActivityRow>>,
}

impl SyncEvents for Recorder {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, row: knobas_core::activity::ActivityRow) {
        self.activity.lock().unwrap().push(row);
    }
}

struct Harness {
    deps: SchedulerDeps,
    events: Arc<Recorder>,
    answer: Arc<Mutex<Answer>>,
    source: String,
}

impl Harness {
    fn pool(&self) -> &sqlx::PgPool {
        &self.deps.pool
    }

    fn answer(&self, answer: Answer) {
        *self.answer.lock().unwrap() = answer;
    }

    fn verbs(&self) -> Vec<String> {
        self.events
            .activity
            .lock()
            .unwrap()
            .iter()
            .map(|row| row.verb.clone())
            .collect()
    }

    /// Mirror an entity of this harness's source.
    async fn mirror(&self, key: &str, body: &str) -> EntityRef {
        let entity = EntityRef::new(&self.source, key);
        sqlx::query(
            "insert into knobas.entity (id, kind, title) values ($1,'ticket','a ticket')
             on conflict (id) do nothing",
        )
        .bind(entity.to_string())
        .execute(self.pool())
        .await
        .unwrap();
        sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
             values ($1,$2,'ticket','a ticket',$3,'{}'::jsonb)
             on conflict (entity_id) do update set body_text = excluded.body_text",
        )
        .bind(entity.to_string())
        .bind(&self.source)
        .bind(body)
        .execute(self.pool())
        .await
        .unwrap();
        entity
    }

    async fn comment(&self, entity: &EntityRef, body: &str) -> store::QueuedWrite {
        flusher::submit(
            &self.deps,
            &self.source,
            WriteOp::Comment {
                entity: entity.to_string(),
                body: body.to_owned(),
            },
        )
        .await
        .unwrap()
    }

    async fn reload(&self, id: i64) -> store::QueuedWrite {
        store::get(self.pool(), id).await.unwrap().unwrap()
    }
}

/// A migrated database, a configured source whose adapter is the fake, and a
/// source id unique to this test -- the same shape as `write_queue.rs`'s
/// harness, minus what narration does not need.
async fn harness() -> Harness {
    let connector = knobas_db::test_util::test_connector().await;
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();

    let source = format!("wa{}", uuid::Uuid::new_v4().simple());
    config::insert(
        &pool,
        &InsertConfig {
            id: source.clone(),
            adapter_kind: "mock".to_owned(),
            display_name: "Fake".to_owned(),
            base_url: String::new(),
            auth_kind: AuthKind::Method(AuthMethod::Pat),
            config: serde_json::json!({}),
            sync_interval_secs: 300,
            enabled: true,
        },
    )
    .await
    .unwrap();
    let secrets = MemoryStore::new();
    secrets
        .put(
            &source,
            &Secret {
                kind: AuthMethod::Pat,
                value: "tok".to_owned(),
            },
        )
        .unwrap();

    let answer = Arc::new(Mutex::new(Answer::Accept));
    let events = Arc::new(Recorder::default());
    Harness {
        deps: SchedulerDeps {
            pool,
            connections: Arc::new(TestConnections(connector)),
            registry: Arc::new(FakeRegistry {
                answer: Arc::clone(&answer),
            }),
            secrets: Arc::new(secrets),
            events: Arc::clone(&events) as Arc<dyn SyncEvents>,
        },
        events,
        answer,
        source,
    }
}

/// One line of patience, however many attempts it covers -- and a changed
/// fault is news again.
#[tokio::test]
async fn an_unchanged_wait_is_one_activity_line_not_one_per_attempt() {
    let h = harness().await;
    let ticket = h.mirror("PAY-16", "a payout fails").await;

    h.answer(Answer::Unreachable);
    let write = h.comment(&ticket, "typed offline").await;
    for _ in 0..3 {
        flusher::flush_source(&h.deps, &h.source).await.unwrap();
    }
    assert_eq!(
        h.verbs(),
        vec!["queued".to_owned(), "waiting".to_owned()],
        "a repeat of the same wait is patience, not news"
    );
    assert!(
        h.reload(write.id).await.attempts >= 4,
        "the row still records every attempt"
    );

    // The fault changing is a change: the credential is now the problem.
    h.answer(Answer::Unauthorized);
    flusher::flush_source(&h.deps, &h.source).await.unwrap();
    assert_eq!(
        h.reload(write.id).await.wait_reason,
        Some(WaitReason::Unauthorized)
    );
    assert_eq!(
        h.verbs(),
        vec![
            "queued".to_owned(),
            "waiting".to_owned(),
            "waiting".to_owned()
        ]
    );
}
