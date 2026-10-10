//! The write queue's flush loop, driven against a fake adapter (issue #42).
//!
//! The fake is told to succeed, to fail as unreachable, or to refuse -- the
//! same idiom `scheduler_run.rs` uses to drive the sync side. What is asserted
//! here is what a caller can observe: a write is kept when the source cannot
//! take it, a changed target produces a held write, a held write never flushes
//! on its own, and one entity's order is preserved while another entity's
//! queue keeps moving.

use std::sync::{Arc, Mutex};

use knobas_core::entity::EntityRef;
use knobas_core::write_queue::{self as store, WaitReason, WriteState};
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
    /// The source said no to the operation itself -- a fault that will not
    /// pass, and must therefore never be retried.
    Refuse,
}

/// What settles a queue row while the write it belongs to is still in flight.
///
/// The window is one HTTP round trip wide and only these can open it, so the
/// fake opens it from inside its own `write` rather than racing a timer.
#[derive(Clone, Copy, Debug)]
enum Interrupt {
    /// The user withdrew it -- the only one a person can actually cause today.
    Withdraw(i64),
    /// The row left `pending` some other way. Nothing produces this
    /// concurrently: the flush loop is the only writer of `held` and it holds
    /// the source lock. It is here so the disclosure below is pinned to the
    /// row's own state rather than to an empty settle.
    Hold(i64),
}

/// A `Source` that records every write it is handed and answers to order.
struct Fake {
    id: String,
    answer: Arc<Mutex<Answer>>,
    written: Arc<Mutex<Vec<WriteOp>>>,
    /// How long the fake takes to answer. Zero everywhere but the concurrency
    /// test, which needs the window between reading the queue and settling it
    /// to be wide enough that a second flusher would fall into it.
    dwell: Arc<Mutex<std::time::Duration>>,
    /// What the source says it made. `None` is `WriteReceipt::none()`, which
    /// is what every op but `create_page` and `log_work` really answers; a
    /// test that needs an id sets one.
    receipt: Arc<Mutex<Option<String>>>,
    /// What settles the row from *inside* the write, once.
    ///
    /// [`Interrupt::Withdraw`] is the race `knobas_core::write_queue::discard`
    /// names in its own doc comment -- the flush loop's per-source lock does
    /// not hold a discard back -- with the timing taken out of it: it lands
    /// while the call is outstanding by construction rather than by luck, so
    /// the settle that follows finds no open row every time this test runs.
    interrupt: Arc<Mutex<Option<Interrupt>>>,
    /// The pool the withdrawal above goes through. The fake reaches the store
    /// directly rather than the flusher, which would need the `SchedulerDeps`
    /// that holds this fake.
    pool: sqlx::PgPool,
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
            accepts_account: false,
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

    async fn write(&self, op: WriteOp) -> Result<knobas_source::WriteReceipt, SourceError> {
        let answer = *self.answer.lock().unwrap();
        let dwell = *self.dwell.lock().unwrap();
        if !dwell.is_zero() {
            tokio::time::sleep(dwell).await;
        }
        if answer == Answer::Accept {
            let interrupt = self.interrupt.lock().unwrap().take();
            match interrupt {
                Some(Interrupt::Withdraw(id)) => {
                    store::discard(&self.pool, id).await.unwrap();
                }
                Some(Interrupt::Hold(id)) => {
                    store::hold(&self.pool, id, serde_json::json!({"moved": true}))
                        .await
                        .unwrap();
                }
                None => {}
            }
            self.written.lock().unwrap().push(op);
            let receipt = self.receipt.lock().unwrap().clone();
            return Ok(match receipt {
                Some(id) => knobas_source::WriteReceipt::id(id),
                None => knobas_source::WriteReceipt::none(),
            });
        }
        Err(match answer {
            Answer::Unreachable => SourceError::Unreachable("connection timed out".to_owned()),
            Answer::Unauthorized => SourceError::Unauthorized { status: Some(401) },
            _ => SourceError::Protocol {
                status: Some(400),
                message: "this issue type does not accept comments".to_owned(),
            },
        })
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

struct FakeRegistry {
    answer: Arc<Mutex<Answer>>,
    written: Arc<Mutex<Vec<WriteOp>>>,
    dwell: Arc<Mutex<std::time::Duration>>,
    receipt: Arc<Mutex<Option<String>>>,
    interrupt: Arc<Mutex<Option<Interrupt>>>,
    pool: sqlx::PgPool,
}

impl AdapterRegistry for FakeRegistry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        Vec::new()
    }

    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        Ok(Box::new(Fake {
            id: instance.id,
            answer: Arc::clone(&self.answer),
            written: Arc::clone(&self.written),
            dwell: Arc::clone(&self.dwell),
            receipt: Arc::clone(&self.receipt),
            interrupt: Arc::clone(&self.interrupt),
            pool: self.pool.clone(),
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
    written: Arc<Mutex<Vec<WriteOp>>>,
    dwell: Arc<Mutex<std::time::Duration>>,
    receipt: Arc<Mutex<Option<String>>>,
    interrupt: Arc<Mutex<Option<Interrupt>>>,
    source: String,
}

impl Harness {
    fn pool(&self) -> &sqlx::PgPool {
        &self.deps.pool
    }

    fn answer(&self, answer: Answer) {
        *self.answer.lock().unwrap() = answer;
    }

    /// What the fake actually accepted, in order, each write read back by the
    /// thing a person would look for it at the source by.
    fn delivered(&self) -> Vec<String> {
        self.written
            .lock()
            .unwrap()
            .iter()
            .map(|op| match op {
                WriteOp::Comment { body, .. } => body.clone(),
                // The body a page edit carried, which is the thing worth
                // reading back: it is the *whole* re-assembled page, and a
                // queue that sent a fragment would show it here (#286).
                WriteOp::UpdatePage { body, .. } => body.clone(),
                // A create is read back by its title: that is what a person
                // would look for at the source to find what knobas made.
                WriteOp::CreateTicket { title, .. } | WriteOp::CreatePage { title, .. } => {
                    title.clone()
                }
                // Except this one, which is looked for by the head branch it
                // was opened from rather than by its title -- `UNCLAIMED_OPS`
                // argues why that difference matters (#353).
                WriteOp::CreatePullRequest { head, .. } => head.clone(),
                other => panic!("this harness queues comments, edits and creates, got {other:?}"),
            })
            .collect()
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

    /// Mirror an entity of this harness's source, or move it if it is there.
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

    /// The activity line for one verb. Panics if there is not exactly one:
    /// story 20 is one line per state change, and two would be as wrong as
    /// none.
    fn line(&self, verb: &str) -> knobas_core::activity::ActivityRow {
        // Collected out of the lock before anything is asserted: the failure
        // message reads `verbs()`, which takes the same mutex.
        let found: Vec<_> = self
            .events
            .activity
            .lock()
            .unwrap()
            .iter()
            .filter(|row| row.verb == verb)
            .cloned()
            .collect();
        assert_eq!(
            found.len(),
            1,
            "expected exactly one {verb:?} line, got {:?}",
            self.verbs()
        );
        found.into_iter().next().unwrap()
    }

    /// What the fake answers with next: `None` is `WriteReceipt::none()`.
    fn receipt(&self, remote_id: Option<&str>) {
        *self.receipt.lock().unwrap() = remote_id.map(str::to_owned);
    }

    /// Settle this row from inside the next write the fake accepts.
    fn interrupt(&self, interrupt: Interrupt) {
        *self.interrupt.lock().unwrap() = Some(interrupt);
    }

    /// Queue a ticket under a project container -- the op ADR-0012 singles out
    /// as not naturally idempotent, and whose adapter names nothing back.
    async fn create_ticket(&self, project: &EntityRef, title: &str) -> store::QueuedWrite {
        flusher::submit(
            &self.deps,
            &self.source,
            WriteOp::CreateTicket {
                entity: project.to_string(),
                title: title.to_owned(),
                body: "why".to_owned(),
                ticket_type: "Task".to_owned(),
            },
        )
        .await
        .unwrap()
    }

    /// Queue a page under a parent -- the create whose adapter *does* name what
    /// it made (`WriteReceipt::id`), which is the other half of #336.
    async fn create_page(&self, parent: &EntityRef, title: &str) -> store::QueuedWrite {
        flusher::submit(
            &self.deps,
            &self.source,
            WriteOp::CreatePage {
                parent: parent.to_string(),
                space: "TIDE".to_owned(),
                title: title.to_owned(),
                body: "<p>body</p>".to_owned(),
            },
        )
        .await
        .unwrap()
    }

    /// Queue a pull request in a repository, from `head` into `main` -- the
    /// create whose *number* the source assigns and never says, and which is
    /// nonetheless addressable by what the row carries (#353).
    async fn create_pull_request(&self, repo: &EntityRef, head: &str) -> store::QueuedWrite {
        flusher::submit(
            &self.deps,
            &self.source,
            WriteOp::CreatePullRequest {
                entity: repo.to_string(),
                title: "Retry the SEPA batch".to_owned(),
                body: "why".to_owned(),
                head: head.to_owned(),
                base: "main".to_owned(),
            },
        )
        .await
        .unwrap()
    }

    /// A Confluence page in the mirror, at `version`, with `body` as its
    /// storage format -- the payload shape `update_page` holds against.
    ///
    /// `body_text` is the stripped body, the way the adapter builds it, so the
    /// two halves of the record move together the way a real edit moves them.
    async fn mirror_page(&self, id: &str, body: &str, version: i64) -> EntityRef {
        let entity = EntityRef::new(&self.source, id);
        sqlx::query(
            "insert into knobas.entity (id, kind, title) values ($1,'page','a page')
             on conflict (id) do nothing",
        )
        .bind(entity.to_string())
        .execute(self.pool())
        .await
        .unwrap();
        sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
             values ($1,$2,'page','a page',$3,$4)
             on conflict (entity_id) do update
                set body_text = excluded.body_text, payload = excluded.payload",
        )
        .bind(entity.to_string())
        .bind(&self.source)
        .bind(body)
        .bind(serde_json::json!({
            "id": id,
            "body": { "storage": { "value": format!("<h2>H</h2><p>{body}</p>") } },
            "version": { "number": version },
        }))
        .execute(self.pool())
        .await
        .unwrap();
        entity
    }

    /// Queue a whole-body page edit made against `base_version`.
    async fn edit_page(
        &self,
        entity: &EntityRef,
        base_version: i64,
        body: &str,
    ) -> store::QueuedWrite {
        flusher::submit(
            &self.deps,
            &self.source,
            WriteOp::UpdatePage {
                entity: entity.to_string(),
                base_version,
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
/// source id unique to this test -- the database is shared by the file.
async fn harness() -> Harness {
    let connector = knobas_db::test_util::test_connector().await;
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();

    let source = format!("wq{}", uuid::Uuid::new_v4().simple());
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
            &knobas_secrets::KeychainAccount::source(&source),
            &Secret::just(AuthMethod::Pat, "tok"),
        )
        .unwrap();

    let answer = Arc::new(Mutex::new(Answer::Accept));
    let written = Arc::new(Mutex::new(Vec::new()));
    let dwell = Arc::new(Mutex::new(std::time::Duration::ZERO));
    let receipt = Arc::new(Mutex::new(None));
    let interrupt = Arc::new(Mutex::new(None));
    let events = Arc::new(Recorder::default());
    Harness {
        deps: SchedulerDeps {
            pool: pool.clone(),
            connections: Arc::new(TestConnections(connector)),
            registry: Arc::new(FakeRegistry {
                answer: Arc::clone(&answer),
                written: Arc::clone(&written),
                dwell: Arc::clone(&dwell),
                receipt: Arc::clone(&receipt),
                interrupt: Arc::clone(&interrupt),
                pool,
            }),
            secrets: Arc::new(secrets),
            events: Arc::clone(&events) as Arc<dyn SyncEvents>,
            timing: knobas_sync::scheduler::SchedulerTiming::default(),
        },
        events,
        answer,
        written,
        dwell,
        receipt,
        interrupt,
        source,
    }
}

/// The happy path, and story 20's first two lines: a write that can go, goes,
/// and both the queueing and the delivery are in the activity stream.
#[tokio::test]
async fn a_write_a_source_can_take_goes_at_once() {
    let h = harness().await;
    let ticket = h.mirror("PAY-1", "a payout fails").await;

    let write = h.comment(&ticket, "on it").await;

    assert_eq!(h.delivered(), vec!["on it".to_owned()]);
    assert_eq!(h.reload(write.id).await.state, WriteState::Sent);
    assert_eq!(h.verbs(), vec!["queued".to_owned(), "sent".to_owned()]);
}

/// Stories 1, 2, 5 and 9: an edit made while the source cannot take it is
/// kept, says which fault it is waiting on, and goes on its own once the
/// source can take it -- no ceremony from the user.
#[tokio::test]
async fn an_undeliverable_write_is_kept_and_flushes_when_the_source_returns() {
    let h = harness().await;
    let ticket = h.mirror("PAY-2", "a payout fails").await;

    h.answer(Answer::Unreachable);
    let write = h.comment(&ticket, "typed on a train").await;
    let waiting = h.reload(write.id).await;
    assert_eq!(waiting.state, WriteState::Pending, "the edit is not lost");
    assert_eq!(waiting.wait_reason, Some(WaitReason::Unreachable));
    assert!(h.delivered().is_empty());

    // A rejected credential is a different sentence from a silent server.
    h.answer(Answer::Unauthorized);
    flusher::flush_source(&h.deps, &h.source).await.unwrap();
    assert_eq!(
        h.reload(write.id).await.wait_reason,
        Some(WaitReason::Unauthorized)
    );

    h.answer(Answer::Accept);
    flusher::flush_source(&h.deps, &h.source).await.unwrap();
    assert_eq!(h.delivered(), vec!["typed on a train".to_owned()]);
    assert_eq!(h.reload(write.id).await.state, WriteState::Sent);
}

/// Story 19: a source that refuses the operation is not retried, and what it
/// said is kept. The proof it is not retried is that a later flush against a
/// source that would now accept anything still delivers nothing.
#[tokio::test]
async fn a_refused_write_is_reported_and_never_retried() {
    let h = harness().await;
    let ticket = h.mirror("PAY-3", "a payout fails").await;

    h.answer(Answer::Refuse);
    let write = h.comment(&ticket, "no room for this").await;
    let refused = h.reload(write.id).await;
    assert_eq!(refused.state, WriteState::Refused);
    assert_eq!(
        refused.detail.as_deref(),
        Some("protocol: this issue type does not accept comments")
    );

    h.answer(Answer::Accept);
    for _ in 0..3 {
        flusher::flush_source(&h.deps, &h.source).await.unwrap();
    }
    assert!(
        h.delivered().is_empty(),
        "a refusal is a decision, not a blip"
    );
    assert!(h.verbs().contains(&"refused".to_owned()));
}

/// Story 11, and the property the whole feature rests on: a target that moved
/// on produces a held write rather than a silent overwrite. The control is the
/// same flush against a target that did *not* move.
#[tokio::test]
async fn a_target_that_changed_holds_the_write_and_one_that_did_not_does_not() {
    let h = harness().await;
    let untouched = h.mirror("PAY-4", "a payout fails").await;
    let replied_to = h.mirror("PAY-5", "a payout fails").await;

    h.answer(Answer::Unreachable);
    let control = h.comment(&untouched, "mine").await;
    let conflicted = h.comment(&replied_to, "mine").await;

    // Somebody replies to the thread being answered. Contract §4.1 puts a
    // comment's text into `body_text`, which is what the queued snapshot took.
    h.mirror("PAY-5", "a payout fails\n\njonas: already on it")
        .await;

    h.answer(Answer::Accept);
    flusher::flush_source(&h.deps, &h.source).await.unwrap();

    assert_eq!(
        h.delivered(),
        vec!["mine".to_owned()],
        "only the untouched target's write was sent"
    );
    assert_eq!(h.reload(control.id).await.state, WriteState::Sent);

    let held = h.reload(conflicted.id).await;
    assert_eq!(held.state, WriteState::Held);
    // Story 12: both versions, side by side.
    assert_eq!(held.target_snapshot["text"], "a payout fails");
    assert_eq!(
        held.held_snapshot.as_ref().unwrap()["text"],
        "a payout fails\n\njonas: already on it"
    );
    assert!(h.verbs().contains(&"held".to_owned()));
}

/// M3.2's version hold (#286): a page whose version moved past the one the
/// edit was made against is held, with both versions in the record.
///
/// No separate version check does this. `update_page` takes the whole-record
/// projection, which carries the mirrored payload verbatim, and a Confluence
/// page keeps `version.number` in that payload -- so the ordinary "snapshot at
/// queue time, snapshot at flush time" comparison *is* the version comparison.
/// `base_version` is the same number by construction: the detail reads it off
/// the mirror the reader is looking at.
///
/// The control is the page that did not move. Without it this test would pass
/// against a queue that held every `update_page`, which would be a feature
/// nobody could use.
#[tokio::test]
async fn a_page_whose_version_moved_past_the_edit_holds_it_with_both_versions() {
    let h = harness().await;
    let untouched = h.mirror_page("98307", "base 30 s.", 3).await;
    let overtaken = h.mirror_page("98311", "base 30 s.", 3).await;

    h.answer(Answer::Unreachable);
    let control = h
        .edit_page(&untouched, 3, "<h2>H</h2><p>base 45 s.</p>")
        .await;
    let conflicted = h
        .edit_page(&overtaken, 3, "<h2>H</h2><p>base 45 s.</p>")
        .await;

    // Somebody edits the page in Confluence and a sync mirrors it: version 4.
    h.mirror_page("98311", "base 60 s.", 4).await;

    h.answer(Answer::Accept);
    flusher::flush_source(&h.deps, &h.source).await.unwrap();

    assert_eq!(
        h.delivered(),
        vec!["<h2>H</h2><p>base 45 s.</p>".to_owned()],
        "only the page nobody touched was written"
    );
    assert_eq!(h.reload(control.id).await.state, WriteState::Sent);

    let held = h.reload(conflicted.id).await;
    assert_eq!(held.state, WriteState::Held);
    // The two versions, side by side -- and they are versions, not merely two
    // texts: the number the edit was made against is on one side and the one
    // the mirror now holds is on the other.
    assert_eq!(held.target_snapshot["payload"]["version"]["number"], 3);
    assert_eq!(
        held.held_snapshot.as_ref().unwrap()["payload"]["version"]["number"],
        4
    );
    assert_eq!(held.target_snapshot["text"], "base 30 s.");
    assert_eq!(held.held_snapshot.as_ref().unwrap()["text"], "base 60 s.");
    // The held reason is the target's, and it is not the disabled-source one:
    // the source is on, and #204 forbids collapsing the two explanations.
    assert!(
        held.source_enabled,
        "the source is on; this is a target hold"
    );
    assert!(h.verbs().contains(&"held".to_owned()));
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
    h.mirror("PAY-6", "a payout fails\n\nsomebody else").await;
    h.answer(Answer::Accept);
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

/// Story 22, at the flush loop rather than at the store: a stalled write
/// blocks its own successors and nothing else. The second comment on one
/// ticket may not overtake the first, and a second ticket's write must not
/// wait behind either.
#[tokio::test]
async fn one_entitys_writes_keep_their_order_without_blocking_another() {
    let h = harness().await;
    let blocked = h.mirror("PAY-7", "a payout fails").await;
    let sibling = h.mirror("PAY-8", "another").await;

    h.answer(Answer::Unreachable);
    let first = h.comment(&blocked, "first").await;
    let second = h.comment(&blocked, "second").await;
    h.comment(&sibling, "unrelated").await;

    // The first write is now held: its target moved while the source was down.
    // Its successor must not overtake it.
    h.mirror("PAY-7", "a payout fails\n\nsomebody else").await;
    h.answer(Answer::Accept);
    flusher::flush_source(&h.deps, &h.source).await.unwrap();

    assert_eq!(h.reload(first.id).await.state, WriteState::Held);
    assert_eq!(
        h.reload(second.id).await.state,
        WriteState::Pending,
        "a comment written second may not land first"
    );
    assert_eq!(
        h.delivered(),
        vec!["unrelated".to_owned()],
        "the sibling entity kept moving while the first ticket was stuck"
    );

    // Once the user releases the first, it goes -- and the second is then
    // judged on its own account. Its target moved too, so it holds rather than
    // riding out on its predecessor's decision: consent to overwrite is per
    // write, never inherited.
    flusher::apply_anyway(&h.deps, first.id).await.unwrap();
    assert_eq!(
        h.delivered(),
        vec!["unrelated".to_owned(), "first".to_owned()]
    );
    assert_eq!(h.reload(second.id).await.state, WriteState::Held);

    flusher::apply_anyway(&h.deps, second.id).await.unwrap();
    assert_eq!(
        h.delivered(),
        vec![
            "unrelated".to_owned(),
            "first".to_owned(),
            "second".to_owned()
        ],
        "and the order the user wrote them in is what landed"
    );
}

/// Story 21: one source being down does not stop another. `flush_all` is what
/// the schedule calls, so the isolation has to hold there and not only in a
/// per-source flush.
#[tokio::test]
async fn a_source_that_cannot_write_does_not_stop_another_source() {
    let down = harness().await;
    let up = harness().await;
    let stuck = down.mirror("PAY-9", "a payout fails").await;
    let fine = up.mirror("PAY-10", "a payout fails").await;

    down.answer(Answer::Unreachable);
    let held_back = down.comment(&stuck, "queued").await;
    let goes = up.comment(&fine, "delivered").await;

    flusher::flush_all(&down.deps).await;

    assert_eq!(down.reload(held_back.id).await.state, WriteState::Pending);
    assert_eq!(up.reload(goes.id).await.state, WriteState::Sent);
    assert_eq!(up.delivered(), vec!["delivered".to_owned()]);
}

/// Stories 7, 14 and 20: withdrawing a write is one action, it is terminal,
/// and it is in the activity stream.
#[tokio::test]
async fn a_withdrawn_write_is_terminal_and_recorded() {
    let h = harness().await;
    let ticket = h.mirror("PAY-11", "a payout fails").await;

    h.answer(Answer::Unreachable);
    let write = h.comment(&ticket, "never mind").await;
    assert!(flusher::discard(&h.deps, write.id).await.unwrap().is_some());
    assert!(
        flusher::discard(&h.deps, write.id).await.unwrap().is_none(),
        "withdrawing twice changes nothing, and says so"
    );

    h.answer(Answer::Accept);
    flusher::flush_source(&h.deps, &h.source).await.unwrap();
    assert!(h.delivered().is_empty());
    assert_eq!(h.reload(write.id).await.state, WriteState::Discarded);
    assert!(h.verbs().contains(&"discarded".to_owned()));
}

/// Story 15: the user merges the two intentions themselves, and what is sent
/// is the edited text.
#[tokio::test]
async fn an_edited_held_write_sends_what_the_user_wrote() {
    let h = harness().await;
    let ticket = h.mirror("PAY-12", "a payout fails").await;

    h.answer(Answer::Unreachable);
    let write = h.comment(&ticket, "on it").await;
    h.mirror("PAY-12", "a payout fails\n\njonas: on it").await;
    h.answer(Answer::Accept);
    flusher::flush_source(&h.deps, &h.source).await.unwrap();
    assert_eq!(h.reload(write.id).await.state, WriteState::Held);

    flusher::amend(
        &h.deps,
        write.id,
        WriteOp::Comment {
            entity: ticket.to_string(),
            body: "thanks jonas -- adding the trace".to_owned(),
        },
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(
        h.delivered(),
        vec!["thanks jonas -- adding the trace".to_owned()]
    );
    assert_eq!(h.reload(write.id).await.state, WriteState::Sent);
}

/// A source with no stored credential cannot take a write, and that is a
/// *waiting* fault rather than a lost edit -- entering the credential is what
/// releases it.
#[tokio::test]
async fn a_source_with_no_credential_keeps_the_write() {
    let h = harness().await;
    let ticket = h.mirror("PAY-13", "a payout fails").await;
    knobas_secrets::spawn::delete(
        &h.deps.secrets,
        &knobas_secrets::KeychainAccount::source(&h.source),
    )
    .await
    .unwrap();

    let write = h.comment(&ticket, "kept").await;
    let waiting = h.reload(write.id).await;
    assert_eq!(waiting.state, WriteState::Pending);
    assert_eq!(waiting.wait_reason, Some(WaitReason::Unauthorized));
    assert!(h.delivered().is_empty());
}

/// One probe value per `WriteOp` variant, shared by the two tests below that
/// each ask the enum a question of its own.
///
/// The list itself forces nothing -- a `vec!` does not go non-exhaustive when
/// an enum grows. The no-wildcard `match` in each test is the forcing
/// function, and the length assertion beside it is what catches a variant
/// given an arm there but never a probe here.
fn write_op_probes() -> Vec<WriteOp> {
    vec![
        WriteOp::Comment {
            entity: "jira:PAY-231".to_owned(),
            body: "probe".to_owned(),
        },
        WriteOp::Transition {
            entity: "jira:PAY-231".to_owned(),
            status: "In Progress".to_owned(),
        },
        WriteOp::CreateTicket {
            entity: "jira:PAY".to_owned(),
            title: "probe".to_owned(),
            body: "probe".to_owned(),
            ticket_type: "Task".to_owned(),
        },
        WriteOp::CreateBranch {
            entity: "gitea:tidewater/payout-service".to_owned(),
            name: "probe".to_owned(),
            from_ref: "main".to_owned(),
        },
        WriteOp::CreatePullRequest {
            entity: "gitea:tidewater/payout-service".to_owned(),
            title: "probe".to_owned(),
            body: "probe".to_owned(),
            head: "probe".to_owned(),
            base: "main".to_owned(),
        },
        WriteOp::Approve {
            entity: "gitea:tidewater/payout-service#142".to_owned(),
            body: String::new(),
        },
        WriteOp::TriggerBuild {
            entity: "teamcity:buildType:Payout_Build".to_owned(),
        },
        WriteOp::RerunBuild {
            entity: "teamcity:build:1187".to_owned(),
        },
        WriteOp::LogWork {
            entity: "jira:PAY-231".to_owned(),
            // Fixed, like `contract.rs`'s probe: a payload that moves between
            // runs is one whose failures cannot be compared.
            started: chrono::DateTime::from_timestamp(1_788_000_000, 0).expect("a fixed instant"),
            seconds: 2_700,
            comment: "SEPA retry".to_owned(),
        },
        WriteOp::CreatePage {
            parent: "confluence:98400".to_owned(),
            space: "ENG".to_owned(),
            title: "Standup 2026-09-03".to_owned(),
            body: "<p>nothing blocked</p>".to_owned(),
        },
        WriteOp::UpdatePage {
            entity: "confluence:98307".to_owned(),
            base_version: 3,
            body: "<h2>Backoff policy</h2><p>base 30 s.</p>".to_owned(),
        },
        WriteOp::PauseMonitor {
            entity: "kuma:8".to_owned(),
        },
        WriteOp::ResumeMonitor {
            entity: "kuma:8".to_owned(),
        },
        WriteOp::CreateMonitor {
            entity: knobas_source::monitor_target("kuma"),
            name: "gitea".to_owned(),
            url: "http://gitea:3000/api/healthz".to_owned(),
        },
    ]
}

/// The forcing function ADR-0006 relies on, applied to hold detection: a new
/// `WriteOp` variant that reaches the queue without a stated definition of
/// "changed" must fail a test rather than fall back quietly.
///
/// The match has no wildcard arm for the same reason `WriteOp::identifier`
/// has none, so a new variant stops this test compiling until it is given a
/// probe value -- and then this assertion until it is given a projection.
#[test]
fn every_write_op_has_a_stated_projection() {
    let probes = write_op_probes();
    for op in &probes {
        let identifier = match op {
            WriteOp::Comment { .. }
            | WriteOp::Transition { .. }
            | WriteOp::CreateTicket { .. }
            | WriteOp::CreateBranch { .. }
            | WriteOp::CreatePullRequest { .. }
            | WriteOp::Approve { .. }
            | WriteOp::TriggerBuild { .. }
            | WriteOp::RerunBuild { .. }
            | WriteOp::LogWork { .. }
            | WriteOp::CreatePage { .. }
            | WriteOp::UpdatePage { .. }
            | WriteOp::PauseMonitor { .. }
            | WriteOp::ResumeMonitor { .. }
            | WriteOp::CreateMonitor { .. } => op.identifier(),
        };
        assert!(
            store::PROJECTED_OPS.contains(&identifier),
            "{identifier:?} has no stated definition of a changed target: add one \
             to `knobas_core::write_queue::project` and list it in PROJECTED_OPS"
        );
    }
    assert_eq!(
        probes.len(),
        store::PROJECTED_OPS.len(),
        "PROJECTED_OPS lists an op `WriteOp` does not define"
    );
}

/// Story 9: recovery needs no ceremony. A write queued while the source was
/// down goes on the scheduler's own tick, with nobody asking for it.
///
/// The scheduler is the real one, started and shut down, because the claim is
/// about the loop being *wired* -- calling `flush_all` by hand would prove
/// only what the tests above already prove.
#[tokio::test]
async fn the_scheduler_drains_the_queue_on_its_own() {
    let h = harness().await;
    let ticket = h.mirror("PAY-14", "a payout fails").await;

    h.answer(Answer::Unreachable);
    let write = h.comment(&ticket, "sent by nobody").await;
    assert_eq!(h.reload(write.id).await.state, WriteState::Pending);

    // The source can take it now -- and nothing tells the queue so.
    h.answer(Answer::Accept);
    let scheduler = knobas_sync::scheduler::Scheduler::start(SchedulerDeps {
        pool: knobas_db::test_util::test_pool().await,
        connections: Arc::clone(&h.deps.connections),
        registry: Arc::clone(&h.deps.registry),
        secrets: Arc::clone(&h.deps.secrets),
        events: Arc::clone(&h.deps.events),
        timing: knobas_sync::scheduler::SchedulerTiming::default(),
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

/// Two flushers, one write, one delivery.
///
/// `submit` flushes at once and the scheduler's tick flushes every five
/// seconds, so the two overlapping is ordinary rather than exotic. Both would
/// read the same head write and both would call `Source::write`; the store's
/// state guard stops the second from *settling* the row, but nothing in the
/// store can un-post a comment. A queue that must not lose an edit must not
/// post it twice either.
///
/// The fake dwells for the length of its write, so the window a second flusher
/// would fall into is wide enough to observe rather than a matter of luck.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_flushes_of_one_source_deliver_a_write_once() {
    let h = harness().await;
    let ticket = h.mirror("PAY-15", "a payout fails").await;

    h.answer(Answer::Unreachable);
    let write = h.comment(&ticket, "exactly once").await;
    assert_eq!(h.reload(write.id).await.state, WriteState::Pending);

    h.answer(Answer::Accept);
    *h.dwell.lock().unwrap() = std::time::Duration::from_millis(300);
    let (a, b) = tokio::join!(
        flusher::flush_source(&h.deps, &h.source),
        flusher::flush_source(&h.deps, &h.source),
    );
    a.unwrap();
    b.unwrap();

    assert_eq!(
        h.delivered(),
        vec!["exactly once".to_owned()],
        "the second flusher must not repost what the first was already sending"
    );
    assert_eq!(h.reload(write.id).await.state, WriteState::Sent);
}

/// Issue #336: a create that landed after the user withdrew it is recorded,
/// and the record carries what the source called what it made.
///
/// This is the one moment knobas can know any of it. At discard time the queue
/// cannot tell a write in flight from one never tried -- `discard`'s own doc
/// comment says so, and `attempts` is bumped after the call, never before --
/// so nothing at that end could have said this truthfully. Here the receipt is
/// in hand and the settle has just come back empty, which together mean
/// exactly one thing: the write landed, and the row it belonged to is gone.
///
/// `create_page` is the half where the source names what it made, so the line
/// can point at the page: Confluence answers `WriteReceipt::id`.
#[tokio::test]
async fn a_page_created_after_the_user_withdrew_it_is_recorded_with_its_id() {
    let h = harness().await;
    let parent = h.mirror_page("HOME", "the space home", 4).await;

    h.answer(Answer::Unreachable);
    let write = h.create_page(&parent, "Payout runbook").await;
    assert_eq!(h.reload(write.id).await.state, WriteState::Pending);

    h.answer(Answer::Accept);
    h.receipt(Some("9007"));
    h.interrupt(Interrupt::Withdraw(write.id));
    flusher::flush_source(&h.deps, &h.source).await.unwrap();

    assert_eq!(
        h.delivered(),
        vec!["Payout runbook".to_owned()],
        "the withdrawal must not have stopped the write -- there is no residue \
         to record unless the page was really made"
    );
    assert_eq!(h.reload(write.id).await.state, WriteState::Discarded);

    let line = h.line("unclaimed");
    assert_eq!(line.detail["write_id"], write.id);
    assert_eq!(line.detail["op"], "create_page");
    assert_eq!(line.detail["state"], "discarded");
    assert_eq!(
        line.detail["remote_id"], "9007",
        "the page Confluence made is the one thing knobas can still point at"
    );
    assert_eq!(line.entity_id.as_deref(), Some(parent.to_string().as_str()));
}

/// The other half, and the one the copy must not overclaim: Jira's
/// `create_ticket` answers `WriteReceipt::none()` -- the adapter drops the key
/// on purpose, because a created ticket is found by reading the mirror back.
///
/// So the line still says a ticket was made and withdrawn, and says **nothing**
/// about which ticket, because knobas does not know. A `remote_id` invented
/// here would be knobas naming an artefact it never saw.
#[tokio::test]
async fn a_ticket_created_after_the_user_withdrew_it_is_recorded_without_one() {
    let h = harness().await;
    // A project container is not mirrored -- `project` reads `{"live": false}`
    // at both ends, which is equal, so the write is never held.
    let project = EntityRef::new(&h.source, "PAY");

    h.answer(Answer::Unreachable);
    let write = h.create_ticket(&project, "Reconcile the SEPA batch").await;
    assert_eq!(h.reload(write.id).await.state, WriteState::Pending);

    h.answer(Answer::Accept);
    h.receipt(None);
    h.interrupt(Interrupt::Withdraw(write.id));
    flusher::flush_source(&h.deps, &h.source).await.unwrap();

    assert_eq!(h.delivered(), vec!["Reconcile the SEPA batch".to_owned()]);
    assert_eq!(h.reload(write.id).await.state, WriteState::Discarded);

    let line = h.line("unclaimed");
    assert_eq!(line.detail["op"], "create_ticket");
    assert_eq!(
        line.detail.get("remote_id"),
        None,
        "the source named nothing, so the record names nothing"
    );
    assert_eq!(
        line.detail["write_id"], write.id,
        "with no id from the source, the withdrawn row is the only handle on \
         what was asked for"
    );
    assert_eq!(
        h.reload(write.id).await.payload["CreateTicket"]["title"],
        "Reconcile the SEPA batch",
        "and that row still carries it -- the line points, it does not copy"
    );
}

/// The disclosure is read off the row, not inferred from an empty settle.
///
/// A row that left `pending` some way other than a withdrawal must not be
/// announced as one. Nothing produces that concurrently today -- the flush
/// loop is the only writer of `held` and `refused`, and it holds the source
/// lock across the call -- so this drives the store directly and says so.
/// What it pins is the shape of the claim: the line says *withdrawn* because
/// the row says `discarded`, and the day another transition can land in that
/// window it will not be mislabelled.
#[tokio::test]
async fn a_row_that_settled_some_other_way_is_not_called_withdrawn() {
    let h = harness().await;
    let parent = h.mirror_page("HOME2", "the space home", 4).await;

    h.answer(Answer::Unreachable);
    let write = h.create_page(&parent, "Payout runbook").await;

    h.answer(Answer::Accept);
    h.receipt(Some("9008"));
    h.interrupt(Interrupt::Hold(write.id));
    flusher::flush_source(&h.deps, &h.source).await.unwrap();

    assert_eq!(h.reload(write.id).await.state, WriteState::Held);
    assert!(
        !h.verbs().contains(&"unclaimed".to_owned()),
        "the row was not withdrawn, so nothing may say it was: {:?}",
        h.verbs()
    );
}

/// The forcing function ADR-0006 relies on, applied to the residue a
/// withdrawal can leave: a new `WriteOp` variant must say whether withdrawing
/// it in flight can leave something at the source that knobas cannot name.
///
/// The match has no wildcard arm, so a new variant stops this test compiling
/// until somebody answers -- and the answer is written here, next to the op,
/// rather than inferred from a list that would grow by omission.
#[test]
fn every_write_op_says_whether_a_withdrawal_can_leave_one() {
    let probes = write_op_probes();
    for op in &probes {
        let leaves_one = match op {
            // Both make something new that lives only at the source until the
            // next sync, with nothing linking it to what asked for it.
            WriteOp::CreateTicket { .. }
            | WriteOp::CreatePage { .. }
            // A monitor made and then withdrawn from is a check running
            // against somebody's estate that no knobas row claims, and the
            // name it carries is a label rather than an address -- Uptime Kuma
            // holds any number of monitors called the same thing (issue #453).
            | WriteOp::CreateMonitor { .. } => true,
            // Changes to something the mirror already holds: the write simply
            // arrived, which is what the user asked for.
            WriteOp::Comment { .. }
            | WriteOp::Transition { .. }
            | WriteOp::Approve { .. }
            | WriteOp::UpdatePage { .. }
            // A build is not an artefact, and knobas never claimed one.
            | WriteOp::TriggerBuild { .. }
            | WriteOp::RerunBuild { .. }
            // The hour is at Jira and #328 owns that gap; the worklog's own
            // copy, not this line, is where it is answered.
            | WriteOp::LogWork { .. }
            // Opened against a ref the withdrawn row still carries, which
            // is the address the artefact is found by. #353 decided it and
            // `UNCLAIMED_OPS`'s doc argues it.
            | WriteOp::CreateBranch { .. }
            | WriteOp::CreatePullRequest { .. }
            // A monitor's own state, changed at the source: pausing a check
            // leaves a *paused check*, which is the thing the reader asked
            // for and which the next poll reads back. Nothing new exists that
            // knobas cannot name -- and withdrawing one that never went
            // leaves the monitor exactly as it was.
            | WriteOp::PauseMonitor { .. }
            | WriteOp::ResumeMonitor { .. } => false,
        };
        assert_eq!(
            flusher::UNCLAIMED_OPS.contains(&op.identifier()),
            leaves_one,
            "{:?} and UNCLAIMED_OPS disagree about what a withdrawal leaves",
            op.identifier()
        );
    }
    // `PROJECTED_OPS` is one entry per variant -- the test above is what makes
    // that true -- so it is the variant count, and using it here rather than a
    // literal means neither of these tests has a number to hand-bump.
    assert_eq!(
        probes.len(),
        store::PROJECTED_OPS.len(),
        "a `WriteOp` variant has no probe in `write_op_probes`"
    );
    // And the other direction, which iterating the probes cannot reach: an
    // entry in `UNCLAIMED_OPS` that names no variant at all would never be
    // compared against one, so it would sit there inert and unfalsifiable.
    for op in flusher::UNCLAIMED_OPS {
        assert!(
            probes.iter().any(|probe| probe.identifier() == *op),
            "UNCLAIMED_OPS names {op:?}, which is not a `WriteOp`"
        );
    }
}

/// The op is what the line is about, not the race. A `comment` that landed
/// after the user withdrew it left nothing unclaimed: the reply is on a ticket
/// the mirror names, and it is the write that was asked for.
#[tokio::test]
async fn a_comment_that_landed_after_a_withdrawal_leaves_nothing_unclaimed() {
    let h = harness().await;
    let ticket = h.mirror("PAY-337", "a payout fails").await;

    h.answer(Answer::Unreachable);
    let write = h.comment(&ticket, "on it").await;

    h.answer(Answer::Accept);
    h.interrupt(Interrupt::Withdraw(write.id));
    flusher::flush_source(&h.deps, &h.source).await.unwrap();

    assert_eq!(
        h.delivered(),
        vec!["on it".to_owned()],
        "the race really happened -- the comment went"
    );
    assert_eq!(h.reload(write.id).await.state, WriteState::Discarded);
    assert!(
        !h.verbs().contains(&"unclaimed".to_owned()),
        "nothing was left unclaimed, so nothing may say it was: {:?}",
        h.verbs()
    );
}

/// Issue #353's ruling, driven rather than read off the list: a pull request
/// opened after the user withdrew the write leaves **nothing unclaimed**, and
/// the row is why.
///
/// The number Gitea assigns is server-assigned exactly as a Jira key is, which
/// is what made this op look like `create_ticket`. `UNCLAIMED_OPS`'s doc is
/// where that is argued and not repeated here; what this drives is the claim
/// itself, against a real withdrawal landing inside a real write.
///
/// The last assertion is the load-bearing one: the ruling rests on the
/// discarded row still carrying the `head` the artefact is found by, so a
/// `discard` that stopped keeping the payload would take the reason away, and
/// this is where that would be noticed.
#[tokio::test]
async fn a_pull_request_opened_after_a_withdrawal_leaves_nothing_unclaimed() {
    let h = harness().await;
    // A repository container, unmirrored the way the project above is:
    // `project` reads `{"live": false}` at both ends, which is equal, so the
    // write is never held.
    let repo = EntityRef::new(&h.source, "tidewater/payout-service");

    h.answer(Answer::Unreachable);
    let write = h.create_pull_request(&repo, "knobas-sepa-retry").await;
    assert_eq!(h.reload(write.id).await.state, WriteState::Pending);

    h.answer(Answer::Accept);
    // What Gitea really answers for this op, on the success path as much as
    // here: `WriteReceipt::none()`. The number is nothing the withdrawal took
    // away, because knobas never had it.
    h.receipt(None);
    h.interrupt(Interrupt::Withdraw(write.id));
    flusher::flush_source(&h.deps, &h.source).await.unwrap();

    assert_eq!(
        h.delivered(),
        vec!["knobas-sepa-retry".to_owned()],
        "the race really happened -- the pull request was opened"
    );
    assert_eq!(h.reload(write.id).await.state, WriteState::Discarded);
    assert!(
        !h.verbs().contains(&"unclaimed".to_owned()),
        "a pull request is reclaimable from the head the row still carries, so \
         nothing may say it is unclaimed: {:?}",
        h.verbs()
    );
    assert_eq!(
        h.reload(write.id).await.payload["CreatePullRequest"]["head"],
        "knobas-sepa-retry",
        "and this is why: the discarded row still holds the head the pull \
         request was opened from, which is the address it is found by"
    );
}
