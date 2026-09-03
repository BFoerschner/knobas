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

/// A `Source` that records every write it is handed and answers to order.
struct Fake {
    id: String,
    answer: Arc<Mutex<Answer>>,
    written: Arc<Mutex<Vec<WriteOp>>>,
    /// How long the fake takes to answer. Zero everywhere but the concurrency
    /// test, which needs the window between reading the queue and settling it
    /// to be wide enough that a second flusher would fall into it.
    dwell: Arc<Mutex<std::time::Duration>>,
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

    async fn write(&self, op: WriteOp) -> Result<knobas_source::WriteReceipt, SourceError> {
        let answer = *self.answer.lock().unwrap();
        let dwell = *self.dwell.lock().unwrap();
        if !dwell.is_zero() {
            tokio::time::sleep(dwell).await;
        }
        if answer == Answer::Accept {
            self.written.lock().unwrap().push(op);
            return Ok(knobas_source::WriteReceipt::none());
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
}

struct FakeRegistry {
    answer: Arc<Mutex<Answer>>,
    written: Arc<Mutex<Vec<WriteOp>>>,
    dwell: Arc<Mutex<std::time::Duration>>,
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
    source: String,
}

impl Harness {
    fn pool(&self) -> &sqlx::PgPool {
        &self.deps.pool
    }

    fn answer(&self, answer: Answer) {
        *self.answer.lock().unwrap() = answer;
    }

    /// The bodies of every comment the fake actually accepted, in order.
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
                other => panic!("this harness queues comments and page edits, got {other:?}"),
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
/// source id unique to this test -- the database is shared by the binary.
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
            &source,
            &Secret {
                kind: AuthMethod::Pat,
                value: "tok".to_owned(),
            },
        )
        .unwrap();

    let answer = Arc::new(Mutex::new(Answer::Accept));
    let written = Arc::new(Mutex::new(Vec::new()));
    let dwell = Arc::new(Mutex::new(std::time::Duration::ZERO));
    let events = Arc::new(Recorder::default());
    Harness {
        deps: SchedulerDeps {
            pool,
            connections: Arc::new(TestConnections(connector)),
            registry: Arc::new(FakeRegistry {
                answer: Arc::clone(&answer),
                written: Arc::clone(&written),
                dwell: Arc::clone(&dwell),
            }),
            secrets: Arc::new(secrets),
            events: Arc::clone(&events) as Arc<dyn SyncEvents>,
        },
        events,
        answer,
        written,
        dwell,
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
    assert!(held.source_enabled, "the source is on; this is a target hold");
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
    knobas_secrets::spawn::delete(&h.deps.secrets, &h.source)
        .await
        .unwrap();

    let write = h.comment(&ticket, "kept").await;
    let waiting = h.reload(write.id).await;
    assert_eq!(waiting.state, WriteState::Pending);
    assert_eq!(waiting.wait_reason, Some(WaitReason::Unauthorized));
    assert!(h.delivered().is_empty());
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
    let probes = [
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
    ];
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
            | WriteOp::UpdatePage { .. } => op.identifier(),
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
