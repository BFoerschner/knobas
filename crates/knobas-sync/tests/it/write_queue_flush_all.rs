//! The write queue's tests that flush every source at once (issue #42).
//!
//! `flush_all`, whether the real scheduler's tick calls it or a test does by
//! hand, reaches *every* configured source in the database, not only the
//! calling test's. Each test's secret store holds only its own sources'
//! credentials, so its builds of the other tests' sources fail, and a failed
//! build re-reasons each of their due writes `unauthorized`.
//!
//! In its own file, deliberately: each test file gets a database of its own
//! (ADR-0017). Inside `write_queue.rs` these tests shared a database with the
//! tests that assert a write's `wait_reason`, and an `Unreachable` read back
//! as `Unauthorized` when a `flush_all` landed between the write and the
//! reload (#609). Nothing here asserts a wait reason.
//!
//! And each test here has a scratch database of its own besides (#1628).
//! Since a write refused its credential waits for a new one rather than for
//! the next tick, a sibling's `flush_all` that failed to build this test's
//! source would leave this test's write waiting for good, where it used to
//! re-reason it for one tick.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use knobas_core::entity::EntityRef;
use knobas_core::write_queue::{self as store, WriteState};
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
}

/// A `Source` that records every write it accepts and answers to order. The
/// trimmed sibling of `write_queue.rs`'s fake: nothing here needs a refusal,
/// a receipt, a dwell or an interrupt.
struct Fake {
    id: String,
    line: Line,
}

#[async_trait::async_trait]
impl Source for Fake {
    /// Draws nothing: this fake has no source view (#661).
    async fn view(
        &self,
        _entity: &str,
        _section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, knobas_source::SourceError> {
        Ok(None)
    }

    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_kind: "mock".to_owned(),
            name: "Fake".to_owned(),
            capabilities: Vec::new(),
            adapter_version: "0".to_owned(),
            auth_methods: vec![AuthMethod::Pat],
            accepts_account: false,
            min_sync_interval_secs: 60,
            roles: Vec::new(),
            source_actions: Vec::new(),
            clone_from: Vec::new(),
            url_forms: Vec::new(),
            write_ops: vec!["comment".to_owned()],
            entity_kinds: vec![KindInfo {
                id: "ticket".to_owned(),
                label: "Ticket".to_owned(),
                plural: "Tickets".to_owned(),
                monogram: "TI".to_owned(),
                full_sync_exhaustive: true,
                refreshes: true,
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

    async fn write(&self, op: WriteOp) -> Result<knobas_source::WriteReceipt, SourceError> {
        match *self.line.answer.lock().unwrap() {
            Answer::Accept => {
                self.line.written.lock().unwrap().push(op);
                Ok(knobas_source::WriteReceipt::none())
            }
            Answer::Unreachable => Err(SourceError::Unreachable("connection timed out".to_owned())),
        }
    }

    /// No workflow: this fake declares no `transition` write op, which is the
    /// contract battery's rule for when the read is refused (#498).
    async fn reachable_transitions(
        &self,
        entity: &str,
    ) -> Result<Vec<String>, knobas_source::SourceError> {
        Err(knobas_source::SourceError::protocol(format!(
            "this fake has no workflow, so there are no reachable transitions for {entity:?}"
        )))
    }
}

/// One source's end of the fake: what it answers, and what it received.
#[derive(Clone)]
struct Line {
    answer: Arc<Mutex<Answer>>,
    written: Arc<Mutex<Vec<WriteOp>>>,
}

/// Builds a fake per source id, each on its own [`Line`], so a write
/// delivered to one source cannot read back as delivered to another (#619).
struct FakeRegistry {
    lines: HashMap<String, Line>,
}

impl AdapterRegistry for FakeRegistry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        Vec::new()
    }

    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        // Not reached for a sibling test's source -- its build fails first,
        // on the credential this deps' secret store does not hold. Should
        // that ever change, the sibling's writes wait on this error rather
        // than land in a fake that is not theirs.
        let line = self.lines.get(&instance.id).ok_or_else(|| {
            SourceError::protocol(format!("no fake for source {:?}", instance.id))
        })?;
        Ok(Box::new(Fake {
            id: instance.id,
            line: line.clone(),
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

/// Events nobody reads: this file asserts on the row and the deliveries.
struct Silent;

impl SyncEvents for Silent {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
}

struct Harness {
    deps: Arc<SchedulerDeps>,
    line: Line,
    source: String,
}

impl Harness {
    fn pool(&self) -> &sqlx::PgPool {
        &self.deps.pool
    }

    fn answer(&self, answer: Answer) {
        *self.line.answer.lock().unwrap() = answer;
    }

    /// The bodies of the comments the fake accepted, in order.
    fn delivered(&self) -> Vec<String> {
        self.line
            .written
            .lock()
            .unwrap()
            .iter()
            .map(|op| match op {
                WriteOp::Comment { body, .. } => body.clone(),
                other => panic!("this harness queues comments, got {other:?}"),
            })
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

    /// Every attempt in this test's database a day into the past, so the
    /// backoff ladder an unreachable write climbs has passed (#1628).
    async fn later(&self) {
        sqlx::query(
            "update knobas.write_queue set attempted_at = attempted_at - interval '1 day'
              where attempted_at is not null",
        )
        .execute(self.pool())
        .await
        .unwrap();
    }
}

/// A migrated database, a configured source whose adapter is the fake, and a
/// source id unique to this test -- `write_queue.rs`'s harness, minus what
/// these tests do not need, and over [`sources`] so that one test can hold
/// two sources on one [`SchedulerDeps`].
async fn harness() -> Harness {
    let source = format!("ws{}", uuid::Uuid::new_v4().simple());
    sources(&[source]).await.pop().unwrap()
}

/// One harness per id, all on one [`SchedulerDeps`]: its secret store holds
/// every one of these sources' credentials and its registry builds each its
/// own fake, so one `flush_all` can deliver to any of them (#619).
async fn sources(ids: &[String]) -> Vec<Harness> {
    let connector = knobas_db::test_util::scratch_database("write_queue_flush_all").await;
    let pool = connector.pool(4).await.unwrap();

    let secrets = MemoryStore::new();
    let mut lines = HashMap::new();
    for id in ids {
        config::insert(
            &pool,
            &InsertConfig {
                id: id.clone(),
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
        secrets
            .put(
                &knobas_secrets::KeychainAccount::source(id),
                &Secret::just(AuthMethod::Pat, "tok"),
            )
            .unwrap();
        lines.insert(
            id.clone(),
            Line {
                answer: Arc::new(Mutex::new(Answer::Accept)),
                written: Arc::new(Mutex::new(Vec::new())),
            },
        );
    }

    let deps = Arc::new(SchedulerDeps {
        pool,
        connections: Arc::new(TestConnections(connector)),
        registry: Arc::new(FakeRegistry {
            lines: lines.clone(),
        }),
        secrets: Arc::new(secrets),
        events: Arc::new(Silent) as Arc<dyn SyncEvents>,
        timing: knobas_sync::scheduler::SchedulerTiming::default(),
        quit: knobas_sync::scheduler::Quit::default(),
    });
    ids.iter()
        .map(|id| Harness {
            deps: Arc::clone(&deps),
            line: lines[id].clone(),
            source: id.clone(),
        })
        .collect()
}

/// Story 9: recovery needs no ceremony. A write queued while the source was
/// down goes on the scheduler's own tick, with nobody asking for it.
///
/// The scheduler is the real one, started and shut down, because the claim is
/// about the loop being *wired* -- calling `flush_all` by hand would prove
/// only what the per-source flushes in `write_queue.rs` and the hand-called
/// `flush_all` below already prove.
#[tokio::test]
async fn the_scheduler_drains_the_queue_on_its_own() {
    let h = harness().await;
    let ticket = h.mirror("PAY-14", "a payout fails").await;

    h.answer(Answer::Unreachable);
    let write = h.comment(&ticket, "sent by nobody").await;
    assert_eq!(h.reload(write.id).await.state, WriteState::Pending);

    // The source can take it now -- and nothing tells the queue so. Its rung
    // of the ladder has passed.
    h.answer(Answer::Accept);
    h.later().await;
    let scheduler = knobas_sync::scheduler::Scheduler::start(SchedulerDeps {
        pool: h.deps.pool.clone(),
        connections: Arc::clone(&h.deps.connections),
        registry: Arc::clone(&h.deps.registry),
        secrets: Arc::clone(&h.deps.secrets),
        events: Arc::clone(&h.deps.events),
        timing: knobas_sync::scheduler::SchedulerTiming::default(),
        quit: knobas_sync::scheduler::Quit::default(),
    })
    .await
    .unwrap();

    let settled = tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            if h.reload(write.id).await.state == WriteState::Sent {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await;
    scheduler.shutdown().await;

    settled.expect("the scheduler's own tick must flush the queue");
    assert_eq!(h.delivered(), vec!["sent by nobody".to_owned()]);
}

/// Story 16, stated as the thing that can actually be tested: there is no
/// timeout to wait out. However many times the flush loop runs, and however
/// willing the source is, a held write does not move.
#[tokio::test]
async fn a_held_write_never_flushes_on_its_own() {
    let h = harness().await;
    let ticket = h.mirror("PAY-6", "a payout fails").await;

    h.answer(Answer::Unreachable);
    let write = h.comment(&ticket, "mine").await;
    // The ticket is withdrawn upstream: what holds a comment on a kind that
    // declares no state (#752; a reply no longer does).
    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(ticket.to_string())
        .execute(&h.deps.pool)
        .await
        .unwrap();
    h.answer(Answer::Accept);
    h.later().await;
    flusher::flush_source(&h.deps, &h.source).await.unwrap();
    assert_eq!(h.reload(write.id).await.state, WriteState::Held);

    for _ in 0..10 {
        flusher::flush_source(&h.deps, &h.source).await.unwrap();
        flusher::flush_all(&h.deps).await;
    }
    assert!(
        h.delivered().is_empty(),
        "nothing may decide on the user's behalf by timing out"
    );
    assert_eq!(h.reload(write.id).await.state, WriteState::Held);

    // And the user's choice is what moves it -- with the version they were
    // shown as the one they consented to overwrite.
    flusher::apply_anyway(&h.deps, write.id).await.unwrap();
    assert_eq!(h.delivered(), vec!["mine".to_owned()]);
    assert_eq!(h.reload(write.id).await.state, WriteState::Sent);
}

/// Story 21: one source being down does not stop another. `flush_all` is what
/// the schedule calls, so the isolation has to hold there and not only in a
/// per-source flush.
///
/// Both writes are still owed when `flush_all` starts -- `submit` delivers on
/// the spot, so `up`'s is queued while `up` is down too -- and both sources
/// are on one `SchedulerDeps` that can build either. Otherwise `up`'s write
/// is `Sent` before the flush, and the test passes whether or not the flush
/// ever reaches `up` (#619).
#[tokio::test]
async fn a_source_that_cannot_write_does_not_stop_another_source() {
    // `config::list` orders by id and `flush_all` flushes in that order, one
    // source after another, so `down` ("wsa...") must sort before `up`
    // ("wsb..."). The other way round, a `flush_all` that gave up after a
    // failing source would already have flushed `up`, and this test could
    // not tell it from the real one.
    let suffix = uuid::Uuid::new_v4().simple();
    let mut both = sources(&[format!("wsa{suffix}"), format!("wsb{suffix}")]).await;
    let up = both.pop().unwrap();
    let down = both.pop().unwrap();
    assert!(down.source < up.source);

    let stuck = down.mirror("PAY-9", "a payout fails").await;
    let fine = up.mirror("PAY-10", "a payout fails").await;

    down.answer(Answer::Unreachable);
    up.answer(Answer::Unreachable);
    let held_back = down.comment(&stuck, "queued").await;
    let goes = up.comment(&fine, "delivered").await;
    assert_eq!(up.reload(goes.id).await.state, WriteState::Pending);

    // `up` comes back; `down` stays down. Both are past their rung, so the
    // flush tries `down` before it reaches `up`.
    up.answer(Answer::Accept);
    up.later().await;
    flusher::flush_all(&down.deps).await;

    assert_eq!(down.reload(held_back.id).await.state, WriteState::Pending);
    assert_eq!(up.reload(goes.id).await.state, WriteState::Sent);
    assert_eq!(up.delivered(), vec!["delivered".to_owned()]);
}

/// #1628: the tick skips a source whose credential needs a human -- the test
/// `trigger_all` makes before a sync -- without building its adapter or
/// sending anything. *Flush now* is a person asking, and does not.
#[tokio::test]
async fn the_tick_skips_a_source_whose_credential_needs_a_human() {
    let h = harness().await;
    let ticket = h.mirror("PAY-47", "a payout fails").await;
    let op = WriteOp::Comment {
        entity: ticket.to_string(),
        body: "queued while refused".to_owned(),
    };
    // Queued without the attempt `submit` makes, so nothing about the row
    // says why it should not go: only the source's health does.
    let write = flusher::queue(&h.deps, &h.source, op).await.unwrap();
    config::set_health(
        h.pool(),
        &h.source,
        config::AuthState::Unauthorized,
        Some("401"),
        None,
    )
    .await
    .unwrap();

    flusher::flush_all(&h.deps).await;
    assert!(h.delivered().is_empty());
    assert_eq!(h.reload(write.id).await.attempted_at, None, "not tried");

    flusher::flush_now(&h.deps, None).await.unwrap();
    assert_eq!(h.delivered(), vec!["queued while refused".to_owned()]);
}
