//! The write queue's flush loop, driven against a fake adapter (issue #42).
//!
//! The fake is told to succeed, to fail as unreachable, or to refuse -- the
//! same idiom `scheduler_run.rs` uses to drive the sync side. What is asserted
//! here is what a caller can observe: a write is kept when the source cannot
//! take it, a changed target produces a held write, and one entity's order is
//! preserved while another entity's queue keeps moving.
//!
//! No test here calls `flush_all`, by hand or through the real scheduler:
//! it flushes every source in the file's database, and its failed builds of
//! the other tests' sources would re-reason their rows `unauthorized` under
//! the tests asserting a wait reason (#609). The tests that need it live in
//! `write_queue_flush_all.rs`, among them story 16's "a held write never
//! flushes on its own". `flush_all_isolation.rs` fails if one comes back
//! (#620).

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
    /// The write lands, and then the answer never arrives: what a once-sent
    /// POST that timed out on the way back looks like from knobas (#1125).
    /// The fake records the write, then fails with
    /// [`SourceError::Unconfirmed`].
    LandsThenTimesOut,
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
    /// The version of a file the source holds, where a test says it holds
    /// one (#822): a `commit_file` made against any other comes back
    /// [`SourceError::Moved`] with this version and its text, as Gitea's
    /// `sha` check answers a stale base.
    file_at: Arc<Mutex<Option<String>>>,
    /// The pool the withdrawal above goes through. The fake reaches the store
    /// directly rather than the flusher, which would need the `SchedulerDeps`
    /// that holds this fake.
    pool: sqlx::PgPool,
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
        fake_descriptor(&self.id)
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
        let held = self.file_at.lock().unwrap().clone();
        if let (Some(theirs), WriteOp::CommitFile { base_sha, .. }) = (held, &op)
            && theirs != *base_sha
        {
            return Err(SourceError::Moved {
                current: format!("theirs at {theirs}\n"),
                base: theirs,
            });
        }
        let answer = *self.answer.lock().unwrap();
        let dwell = *self.dwell.lock().unwrap();
        if !dwell.is_zero() {
            tokio::time::sleep(dwell).await;
        }
        if answer == Answer::LandsThenTimesOut {
            self.written.lock().unwrap().push(op);
            return Err(SourceError::Unconfirmed {
                status: None,
                message: "operation timed out".to_owned(),
            });
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
    file_at: Arc<Mutex<Option<String>>>,
    pool: sqlx::PgPool,
}

impl AdapterRegistry for FakeRegistry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        vec![fake_descriptor("mock")]
    }

    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        Ok(Box::new(Fake {
            id: instance.id,
            answer: Arc::clone(&self.answer),
            written: Arc::clone(&self.written),
            dwell: Arc::clone(&self.dwell),
            receipt: Arc::clone(&self.receipt),
            interrupt: Arc::clone(&self.interrupt),
            file_at: Arc::clone(&self.file_at),
            pool: self.pool.clone(),
        }))
    }
}

/// The fake's descriptor. It declares one payload path, a pull request's
/// state in Gitea's shape (#666), which is what a queued close or reopen holds
/// on (#706); nothing else here reads a declaration.
fn fake_descriptor(id: &str) -> SourceDescriptor {
    use knobas_core::payload::{KindPaths, PayloadPath, StatePath, StateValue};
    use knobas_core::tone::Tone;
    SourceDescriptor {
        id: id.to_owned(),
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
        write_ops: vec!["comment".to_owned()],
        entity_kinds: vec![KindInfo {
            id: "ticket".to_owned(),
            label: "Ticket".to_owned(),
            plural: "Tickets".to_owned(),
            monogram: "TI".to_owned(),
            full_sync_exhaustive: true,
        }],
        config_schema: serde_json::json!({"type": "object", "properties": {}}),
        payload_paths: vec![KindPaths {
            kind: "pr".to_owned(),
            state: vec![StatePath {
                at: PayloadPath::of(["state"]),
                values: vec![
                    StateValue::of("open", Tone::Info),
                    StateValue::of("closed", Tone::Muted),
                ],
            }],
            ..KindPaths::default()
        }],
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
    file_at: Arc<Mutex<Option<String>>>,
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
                // was opened from rather than by its title -- sync's `unclaimed`
                // argues why that difference matters (#353).
                WriteOp::CreatePullRequest { head, .. } => head.clone(),
                // A pull request edit by the title it gives, else the state
                // it sets (#706).
                WriteOp::EditPullRequest { title, state, .. } => title
                    .clone()
                    .or_else(|| state.map(|s| s.as_str().to_owned()))
                    .unwrap_or_default(),
                // A file commit by the version it was sent against and what it
                // says, so a re-based send reads differently from the first
                // (#822).
                WriteOp::CommitFile {
                    base_sha, content, ..
                } => format!("{base_sha}: {content}"),
                // A source action by its id and the label it carries (#1194):
                // what the adapter is handed back is what was queued.
                WriteOp::SourceAction { id, fields, .. } => {
                    format!("{id}: {}", fields.get("label").map_or("", String::as_str))
                }
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

    /// Say which version of a file the source holds now (#822).
    fn file_at(&self, version: &str) {
        *self.file_at.lock().unwrap() = Some(version.to_owned());
    }

    /// Queue a commit of `content` to `docs/retry.md` in `repo`, made against
    /// the file at `base_sha`.
    async fn commit_file(
        &self,
        repo: &EntityRef,
        base_sha: &str,
        content: &str,
    ) -> store::QueuedWrite {
        flusher::submit(
            &self.deps,
            &self.source,
            WriteOp::CommitFile {
                repo: repo.to_string(),
                branch: "main".to_owned(),
                new_branch: None,
                path: "docs/retry.md".to_owned(),
                content: content.to_owned(),
                message: "PAY-231: say how often".to_owned(),
                base_sha: base_sha.to_owned(),
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
    let file_at = Arc::new(Mutex::new(None));
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
                file_at: Arc::clone(&file_at),
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
        file_at,
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
///
/// Since the deputy's ruling on #752 a comment adds rather than overwrites,
/// so what it holds on is the state its source declares (#666): a pull
/// request closed while the comment was queued holds it, one that was only
/// replied to does not -- and on a ticket, whose kind declares no state, a
/// reply holds nothing either.
#[tokio::test]
async fn a_target_that_changed_holds_the_write_and_one_that_did_not_does_not() {
    let h = harness().await;
    let replied_ticket = h.mirror("PAY-5", "a payout fails").await;
    let replied_pull = mirror_pull(&h, "o/r#5", "Bound the retry jitter", "open").await;
    let closed_pull = mirror_pull(&h, "o/r#6", "Bound the retry jitter", "open").await;

    h.answer(Answer::Unreachable);
    let on_ticket = h.comment(&replied_ticket, "mine").await;
    let on_pull = h.comment(&replied_pull, "also mine").await;
    let conflicted = h.comment(&closed_pull, "too late").await;
    assert_eq!(conflicted.target_snapshot["text"], "open");

    // Somebody replies to the ticket and to one pull request, and closes the
    // other. Contract §4.1 puts a reply's text into `body_text`.
    h.mirror("PAY-5", "a payout fails\n\njonas: already on it")
        .await;
    reply_on_pull(&h, &replied_pull, "jonas: already on it").await;
    mirror_pull(&h, "o/r#6", "Bound the retry jitter", "closed").await;

    h.answer(Answer::Accept);
    flusher::flush_source(&h.deps, &h.source).await.unwrap();

    assert_eq!(
        h.delivered(),
        vec!["mine".to_owned(), "also mine".to_owned()],
        "a reply is not what a comment holds on"
    );
    assert_eq!(h.reload(on_ticket.id).await.state, WriteState::Sent);
    assert_eq!(h.reload(on_pull.id).await.state, WriteState::Sent);

    let held = h.reload(conflicted.id).await;
    assert_eq!(held.state, WriteState::Held);
    // Story 12: both versions, side by side.
    assert_eq!(held.target_snapshot["text"], "open");
    assert_eq!(held.held_snapshot.as_ref().unwrap()["text"], "closed");
    assert!(h.verbs().contains(&"held".to_owned()));
}

/// The deputy's ruling on #752, the case it was made for: two comments typed
/// back to back on one pull request are both sent, in order, with no hold --
/// though the first one's re-sync moves the item's text (and its payload)
/// between the second being queued and being sent.
#[tokio::test]
async fn two_comments_back_to_back_on_one_pull_request_both_go_in_order() {
    let h = harness().await;
    let pull = mirror_pull(&h, "o/r#9", "Bound the retry jitter", "open").await;

    h.answer(Answer::Accept);
    let first = flusher::queue(
        &h.deps,
        &h.source,
        WriteOp::Comment {
            entity: pull.to_string(),
            body: "first".to_owned(),
        },
    )
    .await
    .unwrap();
    let second = flusher::queue(
        &h.deps,
        &h.source,
        WriteOp::Comment {
            entity: pull.to_string(),
            body: "second".to_owned(),
        },
    )
    .await
    .unwrap();

    // The first comment's re-sync lands before the second is sent.
    reply_on_pull(&h, &pull, "first").await;

    flusher::flush_source(&h.deps, &h.source).await.unwrap();

    assert_eq!(
        h.delivered(),
        vec!["first".to_owned(), "second".to_owned()],
        "both comments go, in the order they were typed"
    );
    assert_eq!(h.reload(first.id).await.state, WriteState::Sent);
    assert_eq!(h.reload(second.id).await.state, WriteState::Sent);
    assert!(!h.verbs().contains(&"held".to_owned()), "nothing was held");
}

/// A reply on a mirrored pull request, as a sync writes it: the text into
/// `body_text` and a payload that moved (a new `updated_at`), the state left
/// as it was.
async fn reply_on_pull(h: &Harness, pull: &EntityRef, reply: &str) {
    sqlx::query(
        "update sync.item
            set body_text = body_text || E'\n\n' || $2,
                payload = payload || jsonb_build_object('updated_at', now()::text)
          where entity_id = $1",
    )
    .bind(pull.to_string())
    .bind(reply)
    .execute(h.pool())
    .await
    .unwrap();
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

/// A pull request in the mirror with `title` and Gitea's `state`, the record
/// an `edit_pull_request` holds against (#706).
async fn mirror_pull(h: &Harness, key: &str, title: &str, state: &str) -> EntityRef {
    let entity = EntityRef::new(&h.source, key);
    sqlx::query(
        "insert into knobas.entity (id, kind, title) values ($1,'pr',$2)
         on conflict (id) do nothing",
    )
    .bind(entity.to_string())
    .bind(title)
    .execute(h.pool())
    .await
    .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,$2,'pr',$3,$3,$4)
         on conflict (entity_id) do update
            set title = excluded.title, body_text = excluded.body_text,
                payload = excluded.payload",
    )
    .bind(entity.to_string())
    .bind(&h.source)
    .bind(title)
    .bind(serde_json::json!({ "title": title, "state": state }))
    .execute(h.pool())
    .await
    .unwrap();
    entity
}

/// Queue `add_label` with `label` on `pull`, as the shell's payload table
/// builds it from a declared dialog (#1194).
async fn add_label(h: &Harness, pull: &EntityRef, label: &str) -> store::QueuedWrite {
    flusher::submit(
        &h.deps,
        &h.source,
        WriteOp::SourceAction {
            entity: pull.to_string(),
            id: "add_label".to_owned(),
            fields: [("label".to_owned(), label.to_owned())].into(),
        },
    )
    .await
    .unwrap()
}

/// A source action (#1194, ADR-0029) is a write like any other, though the
/// queue cannot read it: held when its target moved, sent when it did not --
/// handed back to the adapter with its id and fields as queued -- and refused,
/// never retried, when the adapter refuses it.
#[tokio::test]
async fn a_source_action_is_held_sent_and_refused_as_a_typed_write_is() {
    let h = harness().await;
    let still = mirror_pull(&h, "o/r#7", "Bound the retry jitter", "open").await;
    let moved = mirror_pull(&h, "o/r#8", "Bound the retry jitter", "open").await;

    h.answer(Answer::Unreachable);
    let sent = add_label(&h, &still, "7").await;
    let held = add_label(&h, &moved, "8").await;
    // Somebody retitles one: knobas cannot know what the action overwrites,
    // so any change to its target holds it.
    mirror_pull(&h, "o/r#8", "Bound the retry jitter, retitled", "open").await;

    h.answer(Answer::Accept);
    flusher::flush_source(&h.deps, &h.source).await.unwrap();
    assert_eq!(h.delivered(), ["add_label: 7"]);
    assert_eq!(h.reload(sent.id).await.state, WriteState::Sent);
    let held = h.reload(held.id).await;
    assert_eq!(held.state, WriteState::Held);
    assert_eq!(held.op, "source_action");
    assert_eq!(held.payload["SourceAction"]["fields"]["label"], "8");

    h.answer(Answer::Refuse);
    let refused = add_label(&h, &still, "9").await;
    let refused = h.reload(refused.id).await;
    assert_eq!(refused.state, WriteState::Refused);
    assert_eq!(
        refused.detail.as_deref(),
        Some("protocol: this issue type does not accept comments")
    );
    h.answer(Answer::Accept);
    flusher::flush_source(&h.deps, &h.source).await.unwrap();
    assert_eq!(h.delivered(), ["add_label: 7"], "a refusal is not retried");
}

/// The deputy's ruling on #706 through the queue itself, not only through
/// `project`: a title edit holds on a retitle and goes over a close. The
/// payload has to reach the projection at the queue *and* at the flush --
/// were either side to project without it, the two snapshots would differ in
/// shape and the closed pull request's edit would hold too.
#[tokio::test]
async fn a_title_edit_holds_on_a_retitle_and_not_on_a_close() {
    let h = harness().await;
    let closed = mirror_pull(&h, "o/r#1", "Bound the retry jitter", "open").await;
    let retitled = mirror_pull(&h, "o/r#2", "Bound the retry jitter", "open").await;

    h.answer(Answer::Unreachable);
    let edit = |entity: &EntityRef, title: &str| WriteOp::EditPullRequest {
        entity: entity.to_string(),
        title: Some(title.to_owned()),
        body: None,
        state: None,
    };
    let control = flusher::submit(&h.deps, &h.source, edit(&closed, "mine"))
        .await
        .unwrap();
    let conflicted = flusher::submit(&h.deps, &h.source, edit(&retitled, "also mine"))
        .await
        .unwrap();

    // Somebody closes one and retitles the other, and a sync mirrors both.
    mirror_pull(&h, "o/r#1", "Bound the retry jitter", "closed").await;
    mirror_pull(&h, "o/r#2", "Bound the jitter", "open").await;

    h.answer(Answer::Accept);
    flusher::flush_source(&h.deps, &h.source).await.unwrap();

    assert_eq!(
        h.delivered(),
        vec!["mine".to_owned()],
        "a close is not what a title edit overwrites"
    );
    assert_eq!(h.reload(control.id).await.state, WriteState::Sent);
    let held = h.reload(conflicted.id).await;
    assert_eq!(held.state, WriteState::Held);
    // The two titles, side by side.
    assert_eq!(held.target_snapshot["text"], "Bound the retry jitter");
    assert_eq!(
        held.held_snapshot.as_ref().unwrap()["text"],
        "Bound the jitter"
    );
}

/// The ruling's own example through the queue (#706): a queued *Close* goes
/// over a retitle, and holds on somebody else closing the pull request first.
/// Both are read from the state the source **declares** for the kind (#666),
/// the registry's template descriptor, looked up at the queue and again at
/// the flush.
#[tokio::test]
async fn a_close_holds_on_the_declared_state_and_not_on_a_retitle() {
    let h = harness().await;
    let retitled = mirror_pull(&h, "o/r#3", "Bound the retry jitter", "open").await;
    let closed = mirror_pull(&h, "o/r#4", "Bound the retry jitter", "open").await;

    h.answer(Answer::Unreachable);
    let close = |entity: &EntityRef| WriteOp::EditPullRequest {
        entity: entity.to_string(),
        title: None,
        body: None,
        state: Some(knobas_source::PullRequestState::Closed),
    };
    let control = flusher::submit(&h.deps, &h.source, close(&retitled))
        .await
        .unwrap();
    let conflicted = flusher::submit(&h.deps, &h.source, close(&closed))
        .await
        .unwrap();
    assert_eq!(control.target_snapshot["text"], "open");

    mirror_pull(&h, "o/r#3", "Bound the jitter", "open").await;
    mirror_pull(&h, "o/r#4", "Bound the retry jitter", "closed").await;

    h.answer(Answer::Accept);
    flusher::flush_source(&h.deps, &h.source).await.unwrap();

    assert_eq!(
        h.delivered(),
        vec!["closed".to_owned()],
        "only one close went"
    );
    assert_eq!(
        h.reload(control.id).await.state,
        WriteState::Sent,
        "a retitle is not what a close overwrites"
    );
    let held = h.reload(conflicted.id).await;
    assert_eq!(held.state, WriteState::Held);
    assert_eq!(held.held_snapshot.as_ref().unwrap()["text"], "closed");
}

/// Story 22, at the flush loop rather than at the store: a stalled write
/// blocks its own successors and nothing else. The second comment on one
/// ticket may not overtake the first, and a second ticket's write must not
/// wait behind either.
#[tokio::test]
async fn one_entitys_writes_keep_their_order_without_blocking_another() {
    let h = harness().await;
    let blocked = mirror_pull(&h, "o/r#7", "Bound the retry jitter", "open").await;
    let sibling = h.mirror("PAY-8", "another").await;

    h.answer(Answer::Unreachable);
    let first = h.comment(&blocked, "first").await;
    let second = h.comment(&blocked, "second").await;
    h.comment(&sibling, "unrelated").await;

    // The first write is now held: its target moved while the source was down
    // (somebody closed the pull request; a comment holds on its declared
    // state, #752). Its successor must not overtake it.
    mirror_pull(&h, "o/r#7", "Bound the retry jitter", "closed").await;
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
    let pull = mirror_pull(&h, "o/r#12", "Bound the retry jitter", "open").await;

    h.answer(Answer::Unreachable);
    let write = h.comment(&pull, "on it").await;
    mirror_pull(&h, "o/r#12", "Bound the retry jitter", "closed").await;
    h.answer(Answer::Accept);
    flusher::flush_source(&h.deps, &h.source).await.unwrap();
    assert_eq!(h.reload(write.id).await.state, WriteState::Held);

    flusher::amend(
        &h.deps,
        write.id,
        WriteOp::Comment {
            entity: pull.to_string(),
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

/// Story 93 of spec #808 (issue #822): a commit that lost a race -- the file
/// changed at the source since it was read -- is a **held** write carrying
/// both versions, never a refusal and never an overwrite. The reader's is the
/// payload, theirs the held snapshot, with the version the source holds now.
/// Nothing sends it until the reader acts; *Apply anyway* sends it again
/// against **their** version, which the source then takes; and a file that
/// moved again after the reader looked holds it again.
#[tokio::test]
async fn a_commit_over_a_file_that_moved_is_held_with_both_versions() {
    let h = harness().await;
    let repo = EntityRef::new(&h.source, "tidewater/payout-service");
    h.file_at("b2");

    let write = h.commit_file(&repo, "b1", "mine\n").await;
    let held = h.reload(write.id).await;
    assert_eq!(held.state, WriteState::Held, "{held:?}");
    let theirs = held.held_snapshot.clone().expect("the held side");
    assert_eq!(theirs["text"], "theirs at b2\n");
    assert_eq!(theirs["base"], "b2");
    assert_eq!(held.payload["CommitFile"]["content"], "mine\n");
    assert_eq!(held.payload["CommitFile"]["base_sha"], "b1");
    assert!(h.verbs().contains(&"held".to_owned()));
    assert!(!h.verbs().contains(&"refused".to_owned()));

    for _ in 0..2 {
        flusher::flush_source(&h.deps, &h.source).await.unwrap();
    }
    assert_eq!(h.reload(write.id).await.state, WriteState::Held);
    assert!(
        h.delivered().is_empty(),
        "a held write waits for the reader"
    );

    // Somebody commits again before the reader decides: what they consent to
    // overwrite is what they saw, so the send against b2 is held on b3.
    h.file_at("b3");
    flusher::apply_anyway(&h.deps, write.id)
        .await
        .unwrap()
        .unwrap();
    let again = h.reload(write.id).await;
    assert_eq!(again.state, WriteState::Held);
    assert_eq!(again.payload["CommitFile"]["base_sha"], "b2");
    assert_eq!(again.held_snapshot.as_ref().unwrap()["base"], "b3");

    flusher::apply_anyway(&h.deps, write.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(h.reload(write.id).await.state, WriteState::Sent);
    assert_eq!(h.delivered(), vec!["b3: mine\n".to_owned()]);
}

/// A commit's target is its repository, and the mirror holds no file: a
/// repository record that moved while the commit was queued is not what the
/// commit overwrites, so it goes (#822). The file's own version, which the
/// source compares, is what holds it (the test above).
#[tokio::test]
async fn a_commit_into_a_repository_whose_record_moved_is_not_held() {
    let h = harness().await;
    let repo = h.mirror("tidewater/payout-service", "payout-service").await;
    h.file_at("b1");

    h.answer(Answer::Unreachable);
    let write = h.commit_file(&repo, "b1", "mine\n").await;
    h.mirror("tidewater/payout-service", "payout-service\n\nretitled")
        .await;

    h.answer(Answer::Accept);
    flusher::flush_source(&h.deps, &h.source).await.unwrap();
    assert_eq!(h.reload(write.id).await.state, WriteState::Sent);
    assert_eq!(h.delivered(), vec!["b1: mine\n".to_owned()]);
}

/// A row no `WriteOp` decodes is refused at its first flush, even over a
/// target that moved: the hold compares the shape the decoded op states, so
/// the decode comes first, and a row that can never be sent is not offered
/// for review as a hold (#1307).
#[tokio::test]
async fn an_undecodable_write_is_refused_rather_than_held() {
    let h = harness().await;
    let ticket = h.mirror("PAY-1307", "a payout fails").await;
    let write = store::queue(
        h.pool(),
        &h.source,
        &ticket,
        "comment",
        serde_json::json!({ "NoSuchOp": { "entity": ticket.to_string() } }),
        serde_json::json!({ "op": "comment", "live": false }),
    )
    .await
    .unwrap();

    h.answer(Answer::Accept);
    flusher::flush_source(&h.deps, &h.source).await.unwrap();
    let refused = h.reload(write.id).await;
    assert_eq!(refused.state, WriteState::Refused);
    assert!(
        refused
            .detail
            .as_deref()
            .is_some_and(|d| d.starts_with("undecodable write")),
        "{:?}",
        refused.detail
    );
    assert!(h.delivered().is_empty());
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
/// is what made this op look like `create_ticket`. `unclaimed`'s doc is
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

/// Issue #1125: a create that reached the source and then timed out is **not
/// sent again by the next tick**. The source made the ticket; nothing said
/// so. The queue holds the write, says it may have been sent and names the
/// source to check, and only *Send again* sends it a second time.
///
/// The control is the same create answered `Unreachable` (the connection
/// never opened): that one waits and goes on its own, which
/// `an_undeliverable_write_is_kept_and_flushes_when_the_source_returns` pins.
#[tokio::test]
async fn a_create_that_landed_and_timed_out_is_held_and_not_sent_again() {
    let h = harness().await;
    let project = EntityRef::new(&h.source, "PAY");

    h.answer(Answer::LandsThenTimesOut);
    let write = h.create_ticket(&project, "SEPA retries double-book").await;
    assert_eq!(h.delivered(), vec!["SEPA retries double-book".to_owned()]);

    let held = h.reload(write.id).await;
    assert_eq!(held.state, WriteState::Held, "{held:?}");
    let unconfirmed = &held.held_snapshot.as_ref().expect("a held side")["unconfirmed"];
    assert_eq!(unconfirmed["source"], "Fake", "{unconfirmed}");
    assert_eq!(
        unconfirmed["detail"], "unconfirmed: operation timed out",
        "{unconfirmed}"
    );
    assert_eq!(h.line("held").detail["state"], "held");

    // The source is back and would take anything: the queue still sends
    // nothing, on any number of ticks.
    h.answer(Answer::Accept);
    for _ in 0..3 {
        flusher::flush_source(&h.deps, &h.source).await.unwrap();
    }
    assert_eq!(
        h.delivered(),
        vec!["SEPA retries double-book".to_owned()],
        "a write that may have landed must not be sent again on its own"
    );
    assert_eq!(h.reload(write.id).await.state, WriteState::Held);

    // The person checked Jira, found no ticket, and sends it again.
    flusher::apply_anyway(&h.deps, write.id)
        .await
        .unwrap()
        .expect("held");
    assert_eq!(h.delivered().len(), 2, "{:?}", h.delivered());
    assert_eq!(h.reload(write.id).await.state, WriteState::Sent);
}

/// A comment is held the same way (#1125): two of the same comment on a
/// ticket is the duplicate a person sees.
#[tokio::test]
async fn a_comment_that_landed_and_timed_out_is_held() {
    let h = harness().await;
    let ticket = h.mirror("PAY-1125", "a payout fails").await;

    h.answer(Answer::LandsThenTimesOut);
    let write = h.comment(&ticket, "on it").await;
    h.answer(Answer::Accept);
    flusher::flush_source(&h.deps, &h.source).await.unwrap();

    assert_eq!(h.delivered(), vec!["on it".to_owned()]);
    assert_eq!(h.reload(write.id).await.state, WriteState::Held);
}

/// A write the source refuses on repeat is not held (#1125): Gitea answers a
/// second pull request from the same head with `409`, so sending it again is
/// harmless and the write waits and goes on its own, as any `Unreachable`.
#[tokio::test]
async fn a_write_the_source_refuses_on_repeat_waits_rather_than_holds() {
    let h = harness().await;
    let repo = EntityRef::new(&h.source, "tidewater/payout-service");

    h.answer(Answer::LandsThenTimesOut);
    let write = h.create_pull_request(&repo, "knobas-1125").await;
    let waiting = h.reload(write.id).await;
    assert_eq!(waiting.state, WriteState::Pending, "{waiting:?}");
    assert_eq!(waiting.wait_reason, Some(WaitReason::Unreachable));

    h.answer(Answer::Accept);
    flusher::flush_source(&h.deps, &h.source).await.unwrap();
    assert_eq!(h.reload(write.id).await.state, WriteState::Sent);
}
