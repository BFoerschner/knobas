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
    /// knobas' own rate limit had no room for the write's request within
    /// its budget, so nothing was sent (#1729). The fake makes a real
    /// `knobas_http` send that the limiter holds, as an adapter's would be.
    HeldByQuota,
    /// The server read the write's request whole and closed the connection
    /// without answering (#1821): a keep-alive timeout firing as the write
    /// went out, or a server that fell over on it. The fake records the
    /// write, since it may have landed, and makes a real once-sent
    /// `knobas_http` POST to a server that does exactly that.
    HungUpOn,
    /// The source said no to the operation in a fault knobas raised itself:
    /// a status-less `Protocol`, the shape [`Answer::HeldByQuota`] fails in
    /// too, with no quota behind it.
    RefuseWithoutStatus,
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
    /// A new credential is saved for the source while a write it is about to
    /// refuse is in flight (#1628): the 401 that comes back is about the
    /// credential that was replaced.
    NewCredential,
}

/// How long a wait for a state that correct code reaches in milliseconds may
/// take before the test fails instead of hanging: past any loaded gate, and
/// reached only by a mutant (#1926).
const HANG_GUARD: std::time::Duration = std::time::Duration::from_secs(30);

/// A write parked here stays on the wire until the test opens the door (#1926).
/// A dwell said "the write is still out" only while the machine was fast
/// enough; on a loaded gate it landed before the test had acted, and correct
/// code went red. A door says it however slow the machine is.
///
/// A semaphore with no permits: [`open`](Door::open) closes it, which lets
/// every wait through at once, the ones parked and the ones to come.
struct Door(tokio::sync::Semaphore);

impl Door {
    fn shut() -> Arc<Door> {
        Arc::new(Door(tokio::sync::Semaphore::new(0)))
    }

    fn open(&self) {
        self.0.close();
    }

    async fn pass(&self) {
        // `Err` is the closed semaphore: the door is open.
        if let Ok(permit) = self.0.acquire().await {
            permit.forget();
        }
    }
}

/// A door for the writes to some targets only, and the order those writes
/// went on the wire (#1797, #1926): a lock-order test holds the flush task's
/// writes on the wire and lets the submit's own through.
#[derive(Default)]
struct Doors {
    /// The door a write to this entity id parks at.
    at: std::collections::HashMap<String, Arc<Door>>,
    /// The target of each write that reached its door, in order.
    reached: Vec<String>,
}

/// Look every 5 ms until `reached` answers, and fail with `what` after
/// [`HANG_GUARD`]: a state decides when this returns, not a clock.
async fn until<T>(what: &str, mut reached: impl FnMut() -> Option<T>) -> T {
    let deadline = std::time::Instant::now() + HANG_GUARD;
    loop {
        if let Some(found) = reached() {
            return found;
        }
        assert!(std::time::Instant::now() < deadline, "{what}");
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
}

/// `work`, failed with `what` if it has not returned after [`HANG_GUARD`]: a
/// wait that correct code ends at once and a mutant never does.
async fn guarded<T>(what: &str, work: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(HANG_GUARD, work)
        .await
        .unwrap_or_else(|_| panic!("{what}"))
}

/// A `Source` that records every write it is handed and answers to order.
struct Fake {
    id: String,
    answer: Arc<Mutex<Answer>>,
    /// Every `write` call, whatever it answered: what a cadence is counted in.
    asked: Arc<Mutex<usize>>,
    written: Arc<Mutex<Vec<WriteOp>>>,
    /// How long the fake takes to answer. Zero everywhere but the refused
    /// credential test, where it is time a re-send would spend. A test that
    /// needs a write held on the wire uses a door instead ([`Doors`], #1926).
    dwell: Arc<Mutex<std::time::Duration>>,
    /// What the source says it made. `None` is `WriteReceipt::none()`, which
    /// is what an op that creates nothing addressable answers; a test that
    /// needs an id sets one.
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
    /// A door for some targets only, and which of their writes reached it;
    /// see [`Doors`].
    doors: Arc<Mutex<Doors>>,
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
        *self.asked.lock().unwrap() += 1;
        let answer = *self.answer.lock().unwrap();
        let door = {
            let mut doors = self.doors.lock().unwrap();
            let door = doors.at.get(op.target()).cloned();
            if door.is_some() {
                doors.reached.push(op.target().to_owned());
            }
            door
        };
        if let Some(door) = door {
            door.pass().await;
        }
        let dwell = *self.dwell.lock().unwrap();
        if !dwell.is_zero() {
            tokio::time::sleep(dwell).await;
        }
        let renewed = {
            let mut interrupt = self.interrupt.lock().unwrap();
            let renewed = matches!(*interrupt, Some(Interrupt::NewCredential));
            if renewed {
                *interrupt = None;
            }
            renewed
        };
        if renewed {
            flusher::release_credential_waits(&self.pool, &self.id)
                .await
                .unwrap();
        }
        if answer == Answer::HungUpOn {
            self.written.lock().unwrap().push(op);
            return Err(hung_up_on().await);
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
                Some(Interrupt::NewCredential) | None => {}
            }
            self.written.lock().unwrap().push(op);
            let receipt = self.receipt.lock().unwrap().clone();
            return Ok(match receipt {
                Some(id) => knobas_source::WriteReceipt::id(id),
                None => knobas_source::WriteReceipt::none(),
            });
        }
        Err(match answer {
            Answer::HeldByQuota => held_by_the_local_quota().await,
            Answer::RefuseWithoutStatus => {
                SourceError::protocol("this fake cannot spell a comment for this ticket")
            }
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

/// What an adapter's write answers when `knobas_http`'s limiter holds its
/// request past the send budget (#1729).
///
/// The quota is one a second with a burst of one, and the burst is taken
/// before the send, so the send cannot pass the limiter within its 50 ms
/// budget. The origin is never reached. The fake wraps the message, as an
/// adapter may: what the queue reads is the mark, not the words.
async fn held_by_the_local_quota() -> SourceError {
    let (url, per_second, burst) = ("http://127.0.0.1:9", 1, 1);
    let client = knobas_http::HttpClient::new(knobas_http::HttpConfig {
        base_url: url.to_owned(),
        requests_per_second: per_second,
        burst,
        ..knobas_http::HttpConfig::default()
    })
    .unwrap()
    .with_send_budget(std::time::Duration::from_millis(50));
    knobas_http::InstanceLimiter::for_url(url, per_second, burst)
        .unwrap()
        .until_ready_within(std::time::Duration::from_secs(5))
        .await
        .unwrap();
    let error = client
        .send(client.request(knobas_http::Method::POST, "/comment"))
        .await
        .expect_err("the limiter holds the send past its budget");
    match error {
        SourceError::Protocol {
            status: None,
            message,
        } => SourceError::protocol(format!("posting the comment: {message}")),
        other => panic!("the limiter's failure is a status-less Protocol: {other:?}"),
    }
}

/// What an adapter's once-sent write answers when the server reads its
/// request whole and closes the connection without a byte of answer (#1821).
///
/// The server hangs up on every request, on a thread of its own with
/// blocking sockets: the request is read to the end of its body first, so
/// the close is a clean end of stream where the client waits for a status
/// line, which hyper reports as an incomplete message. `knobas-http`'s own
/// `hung_up` tests have the pooled-connection version of this server.
async fn hung_up_on() -> SourceError {
    use std::io::Read;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for socket in listener.incoming() {
            let Ok(mut socket) = socket else { return };
            let mut read = Vec::new();
            let mut buffer = [0u8; 4096];
            let whole = |read: &[u8]| {
                let end = read.windows(4).position(|w| w == b"\r\n\r\n")? + 4;
                let head = String::from_utf8_lossy(&read[..end]).to_ascii_lowercase();
                let length = head
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .and_then(|value| value.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                (read.len() >= end + length).then_some(())
            };
            while whole(&read).is_none() {
                match socket.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => read.extend_from_slice(&buffer[..n]),
                }
            }
            let _ = socket.shutdown(std::net::Shutdown::Both);
        }
    });
    let client = knobas_http::HttpClient::new(knobas_http::HttpConfig {
        base_url: url,
        ..knobas_http::HttpConfig::default()
    })
    .unwrap();
    let error = client
        .send(
            client
                .request(knobas_http::Method::POST, "/rest/api/2/issue")
                .json(&serde_json::json!({ "fields": { "summary": "once" } }))
                .no_retry(),
        )
        .await
        .expect_err("the server hangs up on every request");
    match error {
        error @ SourceError::Unconfirmed { status: None, .. } => error,
        other => panic!("a once-sent write the server hung up on is Unconfirmed: {other:?}"),
    }
}

struct FakeRegistry {
    answer: Arc<Mutex<Answer>>,
    asked: Arc<Mutex<usize>>,
    written: Arc<Mutex<Vec<WriteOp>>>,
    dwell: Arc<Mutex<std::time::Duration>>,
    receipt: Arc<Mutex<Option<String>>>,
    interrupt: Arc<Mutex<Option<Interrupt>>>,
    file_at: Arc<Mutex<Option<String>>>,
    doors: Arc<Mutex<Doors>>,
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
            asked: Arc::clone(&self.asked),
            written: Arc::clone(&self.written),
            dwell: Arc::clone(&self.dwell),
            receipt: Arc::clone(&self.receipt),
            interrupt: Arc::clone(&self.interrupt),
            file_at: Arc::clone(&self.file_at),
            doors: Arc::clone(&self.doors),
            pool: self.pool.clone(),
        }))
    }
}

/// The fake's descriptor. It declares one payload path, a pull request's
/// state in Gitea's shape (#666), which is what a queued close or reopen holds
/// on (#706), and the write ops and source action the flush holds a queued
/// write to (#1950).
fn fake_descriptor(id: &str) -> SourceDescriptor {
    use knobas_core::payload::{KindPaths, PayloadPath, StatePath, StateValue};
    use knobas_core::tone::Tone;
    SourceDescriptor {
        id: id.to_owned(),
        adapter_kind: "mock".to_owned(),
        name: "Fake".to_owned(),
        capabilities: vec![knobas_source::Capability::Write],
        adapter_version: "0".to_owned(),
        auth_methods: vec![AuthMethod::Pat],
        accepts_account: false,
        min_sync_interval_secs: 60,
        roles: Vec::new(),
        source_actions: vec!["add_label".to_owned()],
        clone_from: Vec::new(),
        url_forms: Vec::new(),
        code_pages: None,
        // Every op these tests queue, and no other: the flush refuses one the
        // source does not declare before handing it over (#1950), and
        // `an_op_the_source_does_not_declare_is_refused_without_reaching_it`
        // queues a transition and an undeclared source action to see it.
        write_ops: [
            "comment",
            "commit_file",
            "create_monitor",
            "create_page",
            "create_pull_request",
            "create_ticket",
            "edit_pull_request",
            "source_action",
            "update_page",
        ]
        .map(str::to_owned)
        .to_vec(),
        entity_kinds: vec![KindInfo {
            id: "ticket".to_owned(),
            label: "Ticket".to_owned(),
            plural: "Tickets".to_owned(),
            monogram: "TI".to_owned(),
            full_sync_exhaustive: true,
            refreshes: true,
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

/// What a locked keychain answers a read with; `None` while it is open.
type LockedWith = Arc<Mutex<Option<fn() -> knobas_secrets::SecretError>>>;

/// A door on the next read of a locked keychain (#1944): the read says it has
/// begun, then waits to be let go before it fails. What a prompt waiting on
/// a person looks like to the flush.
struct KeychainDoor {
    reached: tokio::sync::oneshot::Sender<()>,
    open: std::sync::mpsc::Receiver<()>,
}

/// The door [`Lockable`] holds its next locked read at, if any.
type KeychainDoors = Arc<Mutex<Option<KeychainDoor>>>;

/// A keychain that can be locked (#1741). What a locked macOS keychain
/// answers a read with, `errSecInteractionNotAllowed`, arrives as
/// [`SecretError::Backend`](knobas_secrets::SecretError::Backend); a store
/// that refuses access as
/// [`Unavailable`](knobas_secrets::SecretError::Unavailable). Open, it is the
/// [`MemoryStore`] it wraps.
struct Lockable {
    inner: MemoryStore,
    locked_with: LockedWith,
    door: KeychainDoors,
}

/// The read a locked macOS keychain fails with.
fn interaction_not_allowed() -> knobas_secrets::SecretError {
    knobas_secrets::SecretError::Backend("User interaction is not allowed.".to_owned())
}

impl SecretStore for Lockable {
    fn get(
        &self,
        account: &knobas_secrets::KeychainAccount,
    ) -> Result<Option<Secret>, knobas_secrets::SecretError> {
        let locked = *self.locked_with.lock().unwrap();
        if let Some(error) = locked {
            // On the store's own blocking thread, as a keychain prompt is.
            let door = self.door.lock().unwrap().take();
            if let Some(door) = door {
                let _ = door.reached.send(());
                let _ = door.open.recv();
            }
            return Err(error());
        }
        self.inner.get(account)
    }
    fn put(
        &self,
        account: &knobas_secrets::KeychainAccount,
        secret: &Secret,
    ) -> Result<(), knobas_secrets::SecretError> {
        self.inner.put(account, secret)
    }
    fn delete(
        &self,
        account: &knobas_secrets::KeychainAccount,
    ) -> Result<(), knobas_secrets::SecretError> {
        self.inner.delete(account)
    }
}

struct Harness {
    deps: SchedulerDeps,
    locked_with: LockedWith,
    keychain_door: KeychainDoors,
    events: Arc<Recorder>,
    answer: Arc<Mutex<Answer>>,
    asked: Arc<Mutex<usize>>,
    written: Arc<Mutex<Vec<WriteOp>>>,
    dwell: Arc<Mutex<std::time::Duration>>,
    receipt: Arc<Mutex<Option<String>>>,
    interrupt: Arc<Mutex<Option<Interrupt>>>,
    file_at: Arc<Mutex<Option<String>>>,
    doors: Arc<Mutex<Doors>>,
    source: String,
}

impl Harness {
    fn pool(&self) -> &sqlx::PgPool {
        &self.deps.pool
    }

    fn answer(&self, answer: Answer) {
        *self.answer.lock().unwrap() = answer;
    }

    /// Lock the keychain the flushes read the credential from: every read
    /// fails with what `error` makes, until [`Self::unlock_keychain`].
    fn lock_keychain(&self, error: fn() -> knobas_secrets::SecretError) {
        *self.locked_with.lock().unwrap() = Some(error);
    }

    fn unlock_keychain(&self) {
        *self.locked_with.lock().unwrap() = None;
    }

    /// Hold the next read of the locked keychain at a door: the first
    /// receiver hears the read begin, and a send on the second lets it fail.
    fn hold_the_keychain_read(
        &self,
    ) -> (
        tokio::sync::oneshot::Receiver<()>,
        std::sync::mpsc::Sender<()>,
    ) {
        let (reached, heard) = tokio::sync::oneshot::channel();
        let (open, opened) = std::sync::mpsc::channel();
        *self.keychain_door.lock().unwrap() = Some(KeychainDoor {
            reached,
            open: opened,
        });
        (heard, open)
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
                // A monitor by its name, which is what attaches it (#453).
                WriteOp::CreateMonitor { name, .. } => name.clone(),
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

    /// Shut a door on every write to `entity` (see [`Doors`]), and hand it
    /// back for the test to open.
    fn door(&self, entity: &EntityRef) -> Arc<Door> {
        let door = Door::shut();
        self.doors
            .lock()
            .unwrap()
            .at
            .insert(entity.to_string(), Arc::clone(&door));
        door
    }

    /// Wait for the `nth` write (from 1) to reach its door, and answer its
    /// target.
    async fn at_a_door(&self, nth: usize) -> String {
        until(&format!("write {nth} never reached its door"), || {
            self.doors.lock().unwrap().reached.get(nth - 1).cloned()
        })
        .await
    }

    /// Wait until `flushes` flushes are queued for the source's lock behind
    /// the write on the wire (#1926): from then on the lock serves them before
    /// any flush that asks after them, the flush task's next write included.
    async fn queued_for_the_source(&self, flushes: usize) {
        until(
            &format!("{flushes} flushes never queued for the source"),
            || (flusher::waiting_on_source(&self.source) >= flushes).then_some(()),
        )
        .await;
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

    /// Queue a monitor create on this harness's source, as the asset pane's
    /// dialog does (#453).
    async fn create_monitor(&self, name: &str) -> store::QueuedWrite {
        flusher::submit(
            &self.deps,
            &self.source,
            WriteOp::CreateMonitor {
                entity: knobas_source::monitor_target(&self.source),
                name: name.to_owned(),
                url: "https://payments.tidewater.test/health".to_owned(),
                kind: knobas_source::MonitorKind::default(),
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

    /// How many times the fake has been asked to write, whatever it answered.
    fn asked(&self) -> usize {
        *self.asked.lock().unwrap()
    }

    /// The fake clock the ladder is read against (#1628): this source's last
    /// attempts moved `secs` into the past. `attempted_at` is the database's
    /// `now()`, so it is the past that moves rather than the present.
    async fn rewind(&self, secs: i64) {
        sqlx::query(
            "update knobas.write_queue
                set attempted_at = attempted_at - make_interval(secs => $2)
              where source_id = $1 and attempted_at is not null",
        )
        .bind(&self.source)
        .bind(secs as f64)
        .execute(self.pool())
        .await
        .unwrap();
    }

    /// The tick's flush, once every rung of the ladder has passed: what "the
    /// source came back and the queue drained on its own" looks like now that
    /// an unreachable write is not tried again every tick.
    async fn flush_later(&self) {
        self.rewind(86_400).await;
        flusher::flush_source(&self.deps, &self.source)
            .await
            .unwrap();
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
    let asked = Arc::new(Mutex::new(0));
    let written = Arc::new(Mutex::new(Vec::new()));
    let dwell = Arc::new(Mutex::new(std::time::Duration::ZERO));
    let receipt = Arc::new(Mutex::new(None));
    let interrupt = Arc::new(Mutex::new(None));
    let file_at = Arc::new(Mutex::new(None));
    let doors = Arc::new(Mutex::new(Doors::default()));
    let events = Arc::new(Recorder::default());
    let locked_with: LockedWith = Arc::new(Mutex::new(None));
    let keychain_door: KeychainDoors = Arc::new(Mutex::new(None));
    Harness {
        deps: SchedulerDeps {
            pool: pool.clone(),
            connections: Arc::new(TestConnections(connector)),
            registry: Arc::new(FakeRegistry {
                answer: Arc::clone(&answer),
                asked: Arc::clone(&asked),
                written: Arc::clone(&written),
                dwell: Arc::clone(&dwell),
                receipt: Arc::clone(&receipt),
                interrupt: Arc::clone(&interrupt),
                file_at: Arc::clone(&file_at),
                doors: Arc::clone(&doors),
                pool,
            }),
            secrets: Arc::new(Lockable {
                inner: secrets,
                locked_with: Arc::clone(&locked_with),
                door: Arc::clone(&keychain_door),
            }),
            events: Arc::clone(&events) as Arc<dyn SyncEvents>,
            timing: knobas_sync::scheduler::SchedulerTiming::default(),
            quit: knobas_sync::scheduler::Quit::default(),
        },
        locked_with,
        keychain_door,
        events,
        answer,
        asked,
        written,
        dwell,
        receipt,
        interrupt,
        file_at,
        doors,
        source,
    }
}

/// How long a [`Dwelling`] build waits for the test to let it go before it
/// finishes anyway: a build held on the only worker then ends the test
/// instead of hanging it.
const HOLD: std::time::Duration = std::time::Duration::from_secs(20);

/// A registry whose builds wait until the test lets them go, the way an
/// external adapter's does while its component's `descriptor()` dwells, and
/// that says when one has started and whether one has finished.
struct Dwelling {
    inner: Arc<dyn AdapterRegistry>,
    entered: std::sync::mpsc::Sender<()>,
    /// Its sender dropped lets every build go.
    release: Mutex<std::sync::mpsc::Receiver<()>>,
    finished: Arc<std::sync::atomic::AtomicBool>,
}

impl AdapterRegistry for Dwelling {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        self.inner.descriptors()
    }

    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        let _ = self.entered.send(());
        let _ = self.release.lock().unwrap().recv_timeout(HOLD);
        self.finished
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.inner.build(instance)
    }
}

/// A flush builds its adapter off the runtime (#1705): on one worker, a
/// build held there would hold every other command until it finished. The
/// second command here is sent once the build has started, and answers while
/// it is still going.
///
/// No wall-clock bound: the build waits for the test, so a green command
/// answers before it finishes however slow it is, and a build held on the
/// worker has finished (after [`HOLD`]) before the command can start.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn a_flush_whose_adapter_is_slow_to_build_holds_no_worker() {
    let h = harness().await;
    let ticket = h.mirror("PAY-1", "a payout fails").await;
    let (entered, built) = std::sync::mpsc::channel();
    let (release, held) = std::sync::mpsc::channel::<()>();
    let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let deps = Arc::new(SchedulerDeps {
        pool: h.deps.pool.clone(),
        connections: Arc::clone(&h.deps.connections),
        registry: Arc::new(Dwelling {
            inner: Arc::clone(&h.deps.registry),
            entered,
            release: Mutex::new(held),
            finished: Arc::clone(&finished),
        }),
        secrets: Arc::clone(&h.deps.secrets),
        events: Arc::clone(&h.deps.events),
        timing: h.deps.timing,
        quit: knobas_sync::scheduler::Quit::default(),
    });
    let write = flusher::queue(
        &deps,
        &h.source,
        WriteOp::Comment {
            entity: ticket.to_string(),
            body: "on it".to_owned(),
        },
    )
    .await
    .unwrap();
    let flushing = tokio::spawn({
        let deps = Arc::clone(&deps);
        async move { flusher::flush_entity_of(&deps, write.id).await }
    });
    // The test's own thread, not the worker, waits here.
    built
        .recv_timeout(std::time::Duration::from_secs(60))
        .expect("the flush builds its adapter");

    let pool = deps.pool.clone();
    let read = tokio::spawn(async move { store::get(&pool, write.id).await })
        .await
        .unwrap();
    assert!(read.is_ok(), "{:?}", read.err());
    assert!(
        !finished.load(std::sync::atomic::Ordering::SeqCst),
        "the command answered only once the build had finished"
    );
    drop(release);
    flushing.await.unwrap().unwrap();
    assert_eq!(h.delivered(), vec!["on it".to_owned()]);
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

/// A comment the source named is stamped with that id as it settles (#2074),
/// the way a created page's is (#1949): Confluence answers the comment's id,
/// and Docs matches its queued comment by it once the page read holds it.
#[tokio::test]
async fn a_sent_comment_keeps_the_id_its_source_gave_it() {
    let h = harness().await;
    let page = h.mirror_page("360548", "the runbook", 3).await;

    h.receipt(Some("360600"));
    let write = h.comment(&page, "X").await;

    assert_eq!(h.reload(write.id).await.state, WriteState::Sent);
    let stamped: Option<String> =
        sqlx::query_scalar("select remote_id from knobas.write_queue where id = $1")
            .bind(write.id)
            .fetch_one(h.pool())
            .await
            .unwrap();
    assert_eq!(stamped.as_deref(), Some("360600"));
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
    h.flush_later().await;
    assert_eq!(
        h.reload(write.id).await.wait_reason,
        Some(WaitReason::Unauthorized)
    );

    // And it waits for a new credential rather than a clock (#1628).
    h.answer(Answer::Accept);
    h.flush_later().await;
    assert!(
        h.delivered().is_empty(),
        "a rejected credential is not re-sent"
    );
    flusher::release_credential_waits(h.pool(), &h.source)
        .await
        .unwrap();
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

/// Battery clause 5, held by the flush itself (#1950): an op the built
/// source does not declare is refused by name and never handed to it, and so
/// is a source action whose id it does not declare. This fake performs every
/// op it is handed, as a guest that declares fewer than it implements does,
/// so the refusal can only be the queue's.
#[tokio::test]
async fn an_op_the_source_does_not_declare_is_refused_without_reaching_it() {
    let h = harness().await;
    let ticket = h.mirror("PAY-1950", "a payout fails").await;
    let pull = mirror_pull(&h, "o/r#1950", "Bound the retry jitter", "open").await;

    h.answer(Answer::Accept);
    let transition = flusher::submit(
        &h.deps,
        &h.source,
        WriteOp::Transition {
            entity: ticket.to_string(),
            status: "Done".to_owned(),
        },
    )
    .await
    .unwrap();
    let action = flusher::submit(
        &h.deps,
        &h.source,
        WriteOp::SourceAction {
            entity: pull.to_string(),
            id: "close_forever".to_owned(),
            fields: std::collections::BTreeMap::new(),
        },
    )
    .await
    .unwrap();

    let transition = h.reload(transition.id).await;
    assert_eq!(transition.state, WriteState::Refused);
    assert_eq!(
        transition.detail.as_deref(),
        Some(
            format!(
                "protocol: source {:?} does not declare the write \"transition\", so it was not \
                 sent",
                h.source
            )
            .as_str()
        )
    );
    let action = h.reload(action.id).await;
    assert_eq!(action.state, WriteState::Refused);
    assert_eq!(
        action.detail.as_deref(),
        Some(
            format!(
                "protocol: source {:?} does not declare the source action \"close_forever\", so \
                 it was not sent",
                h.source
            )
            .as_str()
        )
    );
    assert_eq!(h.asked(), 0, "nothing reached the source");

    // The control: an op it declares, on the same source, goes. On another
    // ticket, since a refusal stays open and stops its own entity's queue.
    let other = h.mirror("PAY-1951", "a payout fails").await;
    h.comment(&other, "on it").await;
    assert_eq!(h.delivered(), vec!["on it".to_owned()]);
}

/// #1950, the ordering: an undeclared op on a target that has also moved is
/// refused, not held. No change to the target makes it declared, and a held
/// row's *Send again* would only be refused, so the reader is not asked to
/// resolve it. A transition holds on the whole target, so a move it does not
/// survive is one that would hold a declared op.
#[tokio::test]
async fn an_undeclared_op_on_a_moved_target_is_refused_rather_than_held() {
    let h = harness().await;
    let ticket = h.mirror("PAY-1952", "a payout fails").await;
    let queued = flusher::queue(
        &h.deps,
        &h.source,
        WriteOp::Transition {
            entity: ticket.to_string(),
            status: "Done".to_owned(),
        },
    )
    .await
    .unwrap();

    // Somebody edits the ticket before the flush.
    h.mirror("PAY-1952", "a payout fails twice").await;
    h.answer(Answer::Accept);
    flusher::flush_source(&h.deps, &h.source).await.unwrap();

    let row = h.reload(queued.id).await;
    assert_eq!(row.state, WriteState::Refused);
    assert_eq!(
        row.detail.as_deref(),
        Some(
            format!(
                "protocol: source {:?} does not declare the write \"transition\", so it was not \
                 sent",
                h.source
            )
            .as_str()
        )
    );
    assert_eq!(row.held_snapshot, None, "nothing was held");
    assert!(!h.verbs().contains(&"held".to_owned()));
    assert_eq!(h.asked(), 0, "nothing reached the source");
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
    h.flush_later().await;

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
    h.flush_later().await;

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
    h.flush_later().await;
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
    h.flush_later().await;
    assert_eq!(h.delivered(), ["add_label: 7"], "a refusal is not retried");
}

/// A source action is as conservative as knobas can be (#1194), on the two
/// counts a send can leave in doubt: one that reached the source and got no
/// answer is held for the reader rather than sent again, and one that landed
/// after the reader withdrew it is disclosed, since what it made is the
/// adapter's to know.
#[tokio::test]
async fn a_source_action_left_in_doubt_is_held_or_disclosed() {
    let h = harness().await;
    let pull = mirror_pull(&h, "o/r#9", "Bound the retry jitter", "open").await;
    // A second target, since the held write stops the first one's queue.
    let other = mirror_pull(&h, "o/r#10", "Bound the retry jitter", "open").await;

    h.answer(Answer::LandsThenTimesOut);
    let unanswered = add_label(&h, &pull, "1").await;
    h.answer(Answer::Accept);
    h.flush_later().await;
    assert_eq!(h.delivered(), ["add_label: 1"], "not sent a second time");
    assert_eq!(h.reload(unanswered.id).await.state, WriteState::Held);

    h.answer(Answer::Unreachable);
    let withdrawn = add_label(&h, &other, "2").await;
    h.answer(Answer::Accept);
    h.interrupt(Interrupt::Withdraw(withdrawn.id));
    h.flush_later().await;
    assert_eq!(h.delivered(), ["add_label: 1", "add_label: 2"]);
    assert_eq!(h.reload(withdrawn.id).await.state, WriteState::Discarded);
    let line = h.line("unclaimed");
    assert_eq!(line.detail["op"], "source_action");
    assert_eq!(line.detail["write_id"], withdrawn.id);
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
    h.flush_later().await;

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
    h.flush_later().await;

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
    h.flush_later().await;

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
    h.flush_later().await;
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
    h.flush_later().await;
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
/// The first flusher's write is held on the wire at a door until the second
/// is queued for the source's lock behind it (#1926), so the second has read
/// the write while it was out: the window it would fall into is open by
/// construction rather than for a dwell.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_flushes_of_one_source_deliver_a_write_once() {
    let h = harness().await;
    let ticket = h.mirror("PAY-15", "a payout fails").await;

    h.answer(Answer::Unreachable);
    let write = h.comment(&ticket, "exactly once").await;
    assert_eq!(h.reload(write.id).await.state, WriteState::Pending);

    h.answer(Answer::Accept);
    h.rewind(86_400).await;
    let out = h.door(&ticket);
    let lands = async {
        h.at_a_door(1).await;
        h.queued_for_the_source(1).await;
        out.open();
    };
    let (a, b, ()) = tokio::join!(
        flusher::flush_source(&h.deps, &h.source),
        flusher::flush_source(&h.deps, &h.source),
        lands,
    );
    a.unwrap();
    b.unwrap();

    assert_eq!(
        h.delivered(),
        vec!["exactly once".to_owned()],
        "the second flusher must not repost what the first was already sending"
    );
    assert_eq!(h.reload(write.id).await.state, WriteState::Sent);
    assert_eq!(
        flusher::waiting_on_source(&h.source),
        0,
        "a flusher that got the lock is no longer counted as waiting for it"
    );
}

/// #1629: the source's lock is held per write, so a second flusher takes its
/// turn at a write the first has just attempted, and re-reads it first. A
/// *Flush now* that read the write before the tick's attempt failed does not
/// send it a second time: the attempt it asked for has just been made.
///
/// Arranged rather than raced: the press starts while the tick's send is held
/// on the wire at a door, which opens once the press is queued for the
/// source's lock (#1926), so it read the row before the attempt was recorded.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_press_beside_a_failing_send_does_not_send_it_again() {
    let h = harness().await;
    let ticket = h.mirror("PAY-1629", "a payout fails").await;

    h.answer(Answer::Unreachable);
    let write = h.comment(&ticket, "once per turn").await;
    assert_eq!(*h.asked.lock().unwrap(), 1);
    h.rewind(86_400).await;
    let out = h.door(&ticket);

    let press = async {
        h.at_a_door(1).await;
        let lands = async {
            h.queued_for_the_source(1).await;
            out.open();
        };
        let (press, ()) = tokio::join!(flusher::flush_now(&h.deps, Some(&h.source)), lands);
        press
    };
    let (tick, press) = tokio::join!(flusher::flush_source(&h.deps, &h.source), press);
    tick.unwrap();
    press.unwrap();

    assert_eq!(
        *h.asked.lock().unwrap(),
        2,
        "the press sent the write the tick had just tried"
    );
    let row = h.reload(write.id).await;
    assert_eq!(row.attempts, 2);
    assert_eq!(row.wait_reason, Some(WaitReason::Unreachable));
}

/// #1797: a submit tries its own write and returns, and leaves the source's
/// other writes to the flush task. It waits for the send on the wire, since
/// the source takes one write at a time, and not for the writes the task
/// still owes after it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_submit_does_not_wait_for_the_flush_tasks_other_writes() {
    beside_the_flush_task(Mine::Submitted).await;
}

/// #1797: as a submit, a write queued and then sent on its own, which is
/// what `submit_write` and a worklog do (`flush_entity_of`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_queued_write_sent_on_its_own_does_not_wait_for_the_flush_tasks_other_writes() {
    beside_the_flush_task(Mine::Queued).await;
}

/// #1797: as a submit, *Send mine anyway*.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn send_mine_anyway_does_not_wait_for_the_flush_tasks_other_writes() {
    beside_the_flush_task(Mine::Released).await;
}

/// #1797: as a submit, *Edit and send*.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_amend_does_not_wait_for_the_flush_tasks_other_writes() {
    beside_the_flush_task(Mine::Amended).await;
}

/// How [`beside_the_flush_task`] sends the write a person just made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mine {
    /// `submit`.
    Submitted,
    /// `queue`, then `flush_entity_of`.
    Queued,
    /// `apply_anyway` on a write held before the task's pass.
    Released,
    /// `amend` on a write held before the task's pass.
    Amended,
}

/// The flush task's pass owes two writes on other entities, each held on the
/// wire at a door; `mine` is sent while the first is out, and its own write
/// has no door. Asserts that `mine` returns while the task's second write is
/// still held, and that every write was delivered once.
///
/// Arranged, not timed (#1926): the first write's door opens only once the
/// flush of `mine` is queued for the source's lock, so the lock serves it
/// before the task's next write. The second's door opens only after `mine`
/// returned, so a flush of `mine` that waited for that write never returns,
/// and the hang guard fails it.
async fn beside_the_flush_task(mine: Mine) {
    let h = harness().await;
    let first = h.mirror("PAY-1797-A", "a payout fails").await;
    let second = h.mirror("PAY-1797-B", "a refund fails").await;
    let pull = mirror_pull(&h, "o/r#1797", "Bound the retry jitter", "open").await;

    let comment = |entity: &EntityRef, body: &str| WriteOp::Comment {
        entity: entity.to_string(),
        body: body.to_owned(),
    };
    // The write the user is shown as held, which the two that release one
    // start from: its pull request was closed between the queue and the send.
    let held = async {
        let write = flusher::queue(&h.deps, &h.source, comment(&pull, "mine"))
            .await
            .unwrap();
        mirror_pull(&h, "o/r#1797", "Bound the retry jitter", "closed").await;
        flusher::flush_source(&h.deps, &h.source).await.unwrap();
        assert_eq!(h.reload(write.id).await.state, WriteState::Held);
        write.id
    };
    let held = match mine {
        Mine::Released | Mine::Amended => Some(held.await),
        Mine::Submitted | Mine::Queued => None,
    };
    for entity in [&first, &second] {
        let owed = format!("owed on {}", entity.key);
        flusher::queue(&h.deps, &h.source, comment(entity, &owed))
            .await
            .unwrap();
    }
    let (first_out, second_out) = (h.door(&first), h.door(&second));

    let send = async {
        match mine {
            Mine::Submitted => {
                h.comment(&pull, "mine").await;
            }
            Mine::Queued => {
                let write = flusher::queue(&h.deps, &h.source, comment(&pull, "mine"))
                    .await
                    .unwrap();
                flusher::flush_entity_of(&h.deps, write.id).await.unwrap();
            }
            Mine::Released => {
                let id = held.expect("held above");
                flusher::apply_anyway(&h.deps, id).await.unwrap().unwrap();
            }
            Mine::Amended => {
                let id = held.expect("held above");
                flusher::amend(&h.deps, id, comment(&pull, "mine"))
                    .await
                    .unwrap()
                    .unwrap();
            }
        }
    };
    let beside = async {
        assert_eq!(h.at_a_door(1).await, first.to_string());
        let lands = async {
            h.queued_for_the_source(1).await;
            first_out.open();
        };
        let ((), ()) = guarded(
            &format!("{mine:?} waited for the flush task's second send"),
            async { tokio::join!(send, lands) },
        )
        .await;
        second_out.open();
    };
    let (task, ()) = tokio::join!(flusher::flush_source(&h.deps, &h.source), beside);
    task.unwrap();

    let mut delivered = h.delivered();
    delivered.sort();
    assert_eq!(
        delivered,
        vec![
            "mine".to_owned(),
            "owed on PAY-1797-A".to_owned(),
            "owed on PAY-1797-B".to_owned(),
        ],
        "every write is delivered once"
    );
}

/// #1807: a submit on an entity whose earlier write the flush task is
/// sending tries its own write once that one lands, and does not wait for
/// the slow write the task owes on another entity after it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_submit_behind_a_head_the_flush_task_is_sending_tries_its_own_write() {
    behind_a_head_on_the_wire(Fate::Lands).await;
}

/// #1807: as above, with the head withdrawn while it is on the wire, which
/// closes it as surely as its landing does: the entity's next write is the
/// head now.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_submit_behind_a_head_withdrawn_on_the_wire_tries_its_own_write() {
    behind_a_head_on_the_wire(Fate::Withdrawn).await;
}

/// What becomes of the head in [`behind_a_head_on_the_wire`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fate {
    /// The flush task's send settles it.
    Lands,
    /// The reader withdraws it at the end of its send, once the submit is
    /// waiting for the source.
    Withdrawn,
}

/// The flush task's pass sends a head write on one entity and then a slow
/// write on another, each held on the wire at a door; a submit on the first
/// entity starts while the head is out. Asserts that the submit returns with
/// its own write sent while the other entity's is still held.
///
/// Arranged, not timed (#1926): the head's door opens only once the submit is
/// queued for the source's lock, so the lock serves it before the task's next
/// write; the slow write's door opens only after the submit returned, so a
/// submit that waited for it never returns, and the hang guard fails it.
async fn behind_a_head_on_the_wire(fate: Fate) {
    let h = harness().await;
    let mine = h.mirror("PAY-1807-A", "a payout fails").await;
    let other = h.mirror("PAY-1807-B", "a refund fails").await;
    let comment = |entity: &EntityRef, body: &str| WriteOp::Comment {
        entity: entity.to_string(),
        body: body.to_owned(),
    };

    // Queued in this order, so the task's pass sends the head on `mine`
    // first and then the write on `other`.
    let head = flusher::queue(&h.deps, &h.source, comment(&mine, "the head"))
        .await
        .unwrap();
    let slow = flusher::queue(&h.deps, &h.source, comment(&other, "the slow one"))
        .await
        .unwrap();
    let (head_out, slow_out) = (h.door(&mine), h.door(&other));
    if fate == Fate::Withdrawn {
        h.interrupt(Interrupt::Withdraw(head.id));
    }

    let submit = async {
        let target = h.at_a_door(1).await;
        assert_eq!(target, mine.to_string(), "the head on `mine` goes first");
        let lands = async {
            h.queued_for_the_source(1).await;
            // The head goes, and the door stays open for the submitted write
            // after it on the same entity.
            head_out.open();
        };
        let (submitted, ()) = guarded("the submit waited for the other entity's write", async {
            tokio::join!(h.comment(&mine, "mine"), lands)
        })
        .await;
        let submitted = h.reload(submitted.id).await.state;
        slow_out.open();
        submitted
    };
    let (task, submitted) = tokio::join!(flusher::flush_source(&h.deps, &h.source), submit);
    task.unwrap();

    assert_eq!(
        submitted,
        WriteState::Sent,
        "the submit returned without trying its own write"
    );
    let settled = match fate {
        Fate::Lands => WriteState::Sent,
        Fate::Withdrawn => WriteState::Discarded,
    };
    assert_eq!(h.reload(head.id).await.state, settled);
    assert_eq!(h.reload(slow.id).await.state, WriteState::Sent);
    let delivered = h.delivered();
    assert_eq!(
        delivered.iter().filter(|body| *body == "mine").count(),
        1,
        "every write is delivered once: {delivered:?}"
    );
    let at = |body: &str| delivered.iter().position(|b| b == body);
    assert!(
        at("the head") < at("mine"),
        "the entity's writes go in queue order: {delivered:?}"
    );
    assert_eq!(delivered.len(), 3, "{delivered:?}");
}

/// #1807 beside #1628: a head that closed while the flush task waited hands
/// its turn on only to a successor the tick owes. One waiting out a rung of
/// the ladder is not sent early because the write ahead of it went.
///
/// Arranged: the task's pass reads the head on `mine`, then sends a slow
/// write on `other`, held on the wire at a door until the reader has
/// withdrawn the head and the write after it, now the entity's head, is
/// recorded as waiting `unreachable` on its first rung.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_head_withdrawn_while_the_task_waited_hands_on_only_to_an_owed_write() {
    let h = harness().await;
    let mine = h.mirror("PAY-1812-A", "a payout fails").await;
    let other = h.mirror("PAY-1812-B", "a refund fails").await;
    let comment = |entity: &EntityRef, body: &str| WriteOp::Comment {
        entity: entity.to_string(),
        body: body.to_owned(),
    };

    let slow = flusher::queue(&h.deps, &h.source, comment(&other, "the slow one"))
        .await
        .unwrap();
    let head = flusher::queue(&h.deps, &h.source, comment(&mine, "the head"))
        .await
        .unwrap();
    let next = flusher::queue(&h.deps, &h.source, comment(&mine, "on a rung"))
        .await
        .unwrap();
    let slow_out = h.door(&other);

    let meanwhile = async {
        let target = h.at_a_door(1).await;
        assert_eq!(target, other.to_string(), "the slow write goes first");
        flusher::discard(&h.deps, head.id).await.unwrap().unwrap();
        store::wait(&h.deps.pool, next.id, WaitReason::Unreachable, None)
            .await
            .unwrap()
            .unwrap();
        // Both have committed while the slow write was held (#1926).
        slow_out.open();
    };
    let (task, ()) = tokio::join!(flusher::flush_source(&h.deps, &h.source), meanwhile);
    task.unwrap();

    assert_eq!(h.reload(slow.id).await.state, WriteState::Sent);
    let row = h.reload(next.id).await;
    assert_eq!(row.state, WriteState::Pending, "sent before its rung");
    assert_eq!(row.attempts, 1);
    assert_eq!(h.delivered(), vec!["the slow one".to_owned()]);
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
    h.flush_later().await;

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

/// The other half, and the one the copy must not overclaim: a source whose
/// `create_ticket` answers `WriteReceipt::none()` (an adapter whose receipt is
/// empty; Jira's named nothing until #1754).
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
    h.flush_later().await;

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
    h.flush_later().await;

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
    h.flush_later().await;

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
    h.flush_later().await;

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

/// **#1627.** A Kuma create whose ack was lost after the emit answers
/// `Unconfirmed`, and the queue holds it: a second `add` is a second
/// `payments-api` monitor. It is not left pending for the next tick.
#[tokio::test]
async fn a_monitor_create_whose_ack_was_lost_is_held_not_pending() {
    let h = harness().await;

    h.answer(Answer::LandsThenTimesOut);
    let write = h.create_monitor("payments-api").await;
    let held = h.reload(write.id).await;
    assert_eq!(held.state, WriteState::Held, "{held:?}");
    assert_eq!(held.wait_reason, None, "{held:?}");

    h.answer(Answer::Accept);
    for _ in 0..3 {
        flusher::flush_source(&h.deps, &h.source).await.unwrap();
    }
    assert_eq!(
        h.delivered(),
        vec!["payments-api".to_owned()],
        "a create that may have landed is not sent again on its own"
    );
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
    h.flush_later().await;
    assert_eq!(h.reload(write.id).await.state, WriteState::Sent);
}

/// **#1821.** A create the server hung up on before answering is held, not
/// refused: the server read it whole, so the ticket may stand at the source.
/// Before, the transport's status-less `Protocol` read as the source refusing
/// the write, and a ticket that landed showed as one that never would.
#[tokio::test]
async fn a_create_the_server_hung_up_on_is_held_not_refused() {
    let h = harness().await;
    let project = EntityRef::new(&h.source, "PAY");

    h.answer(Answer::HungUpOn);
    let write = h.create_ticket(&project, "SEPA retries double-book").await;

    let held = h.reload(write.id).await;
    assert_eq!(held.state, WriteState::Held, "{held:?}");
    assert_eq!(held.wait_reason, None, "{held:?}");
    let unconfirmed = &held.held_snapshot.as_ref().expect("a held side")["unconfirmed"];
    assert_eq!(unconfirmed["source"], "Fake", "{unconfirmed}");

    h.answer(Answer::Accept);
    for _ in 0..3 {
        flusher::flush_source(&h.deps, &h.source).await.unwrap();
    }
    assert_eq!(
        h.delivered(),
        vec!["SEPA retries double-book".to_owned()],
        "a create that may have landed is not sent again on its own"
    );
}

/// A write whose second arrival is refused (Gitea's `409` for a second pull
/// request from one head) waits on the ladder after the same hang-up and goes
/// on its own (#1821), as it does after a timeout.
#[tokio::test]
async fn a_resend_harmless_write_the_server_hung_up_on_waits_on_the_ladder() {
    let h = harness().await;
    let repo = EntityRef::new(&h.source, "tidewater/payout-service");

    h.answer(Answer::HungUpOn);
    let write = h.create_pull_request(&repo, "knobas-1821").await;
    let waiting = h.reload(write.id).await;
    assert_eq!(waiting.state, WriteState::Pending, "{waiting:?}");
    assert_eq!(waiting.wait_reason, Some(WaitReason::Unreachable));

    h.answer(Answer::Accept);
    h.flush_later().await;
    assert_eq!(h.reload(write.id).await.state, WriteState::Sent);
}

/// #1628, SYNC-1: a write whose credential was refused is not sent again by
/// the flushes after it, however long ago the refusal was. Re-sending a dead
/// credential on a timer is how an instance with a login-failure lockout
/// locks the account the user is about to re-enter.
///
/// The fake dwells, as a real source does, so each flush that sends spends
/// time the count would show.
#[tokio::test]
async fn a_write_whose_credential_was_rejected_is_not_sent_again_by_the_next_flushes() {
    let h = harness().await;
    let ticket = h.mirror("PAY-40", "a payout fails").await;
    *h.dwell.lock().unwrap() = std::time::Duration::from_millis(20);

    h.answer(Answer::Unauthorized);
    let write = h.comment(&ticket, "after the password expired").await;
    assert_eq!(
        h.reload(write.id).await.wait_reason,
        Some(WaitReason::Unauthorized)
    );
    assert_eq!(h.asked(), 1, "submit tries the write once");

    h.answer(Answer::Accept);
    for _ in 0..2 {
        // Not a matter of the ladder: a day later it is still not owed.
        h.rewind(86_400).await;
        flusher::flush_source(&h.deps, &h.source).await.unwrap();
    }
    assert_eq!(h.asked(), 1, "no flush re-sends a rejected credential");
    assert_eq!(h.reload(write.id).await.state, WriteState::Pending);
}

/// #1628: a credential change on the source releases its unauthorized writes, and
/// the next flush sends them. A change on another source releases nothing.
#[tokio::test]
async fn a_new_credential_releases_the_wait_on_its_own_source_only() {
    let h = harness().await;
    let other = harness().await;
    let ticket = h.mirror("PAY-41", "a payout fails").await;
    let elsewhere = other.mirror("PAY-42", "a payout fails").await;

    h.answer(Answer::Unauthorized);
    other.answer(Answer::Unauthorized);
    let write = h.comment(&ticket, "re-entered").await;
    let theirs = other.comment(&elsewhere, "still refused").await;

    let released = flusher::release_credential_waits(h.pool(), &h.source)
        .await
        .unwrap();
    assert_eq!(
        released.iter().map(|w| w.id).collect::<Vec<_>>(),
        vec![write.id]
    );
    assert_eq!(h.reload(write.id).await.wait_reason, None);

    h.answer(Answer::Accept);
    other.answer(Answer::Accept);
    flusher::flush_source(&h.deps, &h.source).await.unwrap();
    flusher::flush_source(&other.deps, &other.source)
        .await
        .unwrap();

    assert_eq!(h.delivered(), vec!["re-entered".to_owned()]);
    assert_eq!(h.reload(write.id).await.state, WriteState::Sent);
    assert_eq!(other.asked(), 1, "the other source's write still waits");
    assert_eq!(
        other.reload(theirs.id).await.wait_reason,
        Some(WaitReason::Unauthorized)
    );
}

/// #1628: a refusal that comes back after a new credential was saved is
/// about the credential that was replaced, so it does not re-reason the
/// write `unauthorized` -- that would leave it waiting for a change that has
/// already happened. The next flush tries it with the new one.
#[tokio::test]
async fn a_refusal_in_flight_when_the_credential_changes_does_not_keep_the_write_waiting() {
    let h = harness().await;
    let ticket = h.mirror("PAY-43", "a payout fails").await;

    h.answer(Answer::Unauthorized);
    *h.interrupt.lock().unwrap() = Some(Interrupt::NewCredential);
    let write = h.comment(&ticket, "saved mid-flight").await;
    assert_eq!(h.asked(), 1);
    assert_eq!(
        h.reload(write.id).await.wait_reason,
        None,
        "the stale refusal is not recorded"
    );

    h.answer(Answer::Accept);
    flusher::flush_source(&h.deps, &h.source).await.unwrap();
    assert_eq!(h.delivered(), vec!["saved mid-flight".to_owned()]);
}

/// #1628, story 14: an unreachable write climbs the sync side's ladder
/// (P7: one minute, then two) rather than going every tick. The clock is
/// the row's own `attempted_at`, moved back by hand.
#[tokio::test]
async fn an_unreachable_write_is_tried_again_on_the_sync_ladder() {
    let h = harness().await;
    let ticket = h.mirror("PAY-44", "a payout fails").await;

    h.answer(Answer::Unreachable);
    let write = h.comment(&ticket, "typed offline").await;
    assert_eq!(h.asked(), 1);

    flusher::flush_source(&h.deps, &h.source).await.unwrap();
    h.rewind(50).await;
    flusher::flush_source(&h.deps, &h.source).await.unwrap();
    assert_eq!(h.asked(), 1, "not before the first rung, one minute");

    h.rewind(10).await;
    flusher::flush_source(&h.deps, &h.source).await.unwrap();
    assert_eq!(h.asked(), 2, "on the first rung");
    assert_eq!(h.reload(write.id).await.attempts, 2);

    h.rewind(60).await;
    flusher::flush_source(&h.deps, &h.source).await.unwrap();
    assert_eq!(h.asked(), 2, "the second rung is two minutes");

    h.rewind(60).await;
    h.answer(Answer::Accept);
    flusher::flush_source(&h.deps, &h.source).await.unwrap();
    assert_eq!(h.asked(), 3);
    assert_eq!(h.reload(write.id).await.state, WriteState::Sent);
}

/// #1741: a keychain that would not open is not a refused credential. A
/// write flushed while it is locked waits `unreachable` and climbs the same
/// ladder the sync does, rather than waiting for a person, and goes once the
/// keychain is open and the rung has passed. The source is never asked
/// while the credential cannot be read.
#[tokio::test]
async fn a_write_flushed_while_the_keychain_is_locked_climbs_the_ladder() {
    let h = harness().await;
    let ticket = h.mirror("PAY-46", "a payout fails").await;

    h.lock_keychain(interaction_not_allowed);
    let write = h
        .comment(&ticket, "typed while the keychain was locked")
        .await;
    let waiting = h.reload(write.id).await;
    assert_eq!(waiting.state, WriteState::Pending);
    assert_eq!(waiting.wait_reason, Some(WaitReason::Unreachable));
    // The panel reads the detail for `keychain:`, and prints it: once.
    assert_eq!(
        waiting.detail.as_deref(),
        Some("keychain: User interaction is not allowed.")
    );
    assert_eq!(h.asked(), 0, "the source is not asked without a credential");

    h.unlock_keychain();
    flusher::flush_source(&h.deps, &h.source).await.unwrap();
    assert_eq!(h.asked(), 0, "not before the first rung, one minute");

    h.rewind(60).await;
    flusher::flush_source(&h.deps, &h.source).await.unwrap();
    assert_eq!(h.reload(write.id).await.state, WriteState::Sent);
    assert_eq!(
        h.delivered(),
        vec!["typed while the keychain was locked".to_owned()]
    );
}

/// #1741: a store that refuses access is a keychain fault too, and its detail
/// starts `keychain:` although its own message does not.
#[tokio::test]
async fn a_write_flushed_while_the_store_refuses_access_waits_on_the_keychain() {
    let h = harness().await;
    let ticket = h.mirror("PAY-47", "a payout fails").await;

    h.lock_keychain(|| knobas_secrets::SecretError::Unavailable);
    let write = h.comment(&ticket, "typed while the store refused").await;
    let waiting = h.reload(write.id).await;
    assert_eq!(waiting.wait_reason, Some(WaitReason::Unreachable));
    assert_eq!(
        waiting.detail.as_deref(),
        Some("keychain: the credential store refused access")
    );
    assert_eq!(h.asked(), 0);
}

/// #1944: a credential save is a person acting on what a keychain wait
/// waits for, so it releases that write as it releases a refused one, and
/// the next flush sends it without waiting out its rung. A write waiting on
/// a server that did not answer stays on its rung: the save changed nothing
/// about the server.
#[tokio::test]
async fn a_credential_save_releases_a_write_waiting_on_the_keychain() {
    let h = harness().await;
    let locked = h.mirror("PAY-48", "a payout fails").await;
    let silent = h.mirror("PAY-49", "a payout fails").await;

    h.lock_keychain(interaction_not_allowed);
    let keychain = h.comment(&locked, "typed while it was locked").await;
    h.unlock_keychain();
    h.answer(Answer::Unreachable);
    let server = h.comment(&silent, "typed while it was down").await;
    assert_eq!(h.asked(), 1, "only the second write reached the source");

    let released = flusher::release_credential_waits(h.pool(), &h.source)
        .await
        .unwrap();
    assert_eq!(
        released.iter().map(|w| w.id).collect::<Vec<_>>(),
        vec![keychain.id]
    );
    assert_eq!(h.reload(keychain.id).await.wait_reason, None);
    assert_eq!(
        h.reload(server.id).await.wait_reason,
        Some(WaitReason::Unreachable)
    );

    h.answer(Answer::Accept);
    flusher::flush_source(&h.deps, &h.source).await.unwrap();
    assert_eq!(h.reload(keychain.id).await.state, WriteState::Sent);
    assert_eq!(
        h.delivered(),
        vec!["typed while it was locked".to_owned()],
        "the server's write waits out its rung"
    );
}

/// #1944: a save made while a flush is reading the locked keychain is newer
/// than the fault that read meets, as a save is newer than a 401 the
/// replaced credential got (#1628). The write stays released, and the next
/// flush tries it at once rather than after a rung.
#[tokio::test]
async fn a_keychain_fault_met_across_a_credential_save_does_not_put_the_write_back_to_wait() {
    let h = harness().await;
    let ticket = h.mirror("PAY-50", "a payout fails").await;

    h.lock_keychain(interaction_not_allowed);
    let write = flusher::queue(
        &h.deps,
        &h.source,
        WriteOp::Comment {
            entity: ticket.to_string(),
            body: "saved across the read".to_owned(),
        },
    )
    .await
    .unwrap();
    let (reading, let_it_fail) = h.hold_the_keychain_read();
    let (flushed, ()) = tokio::join!(flusher::flush_entity_of(&h.deps, write.id), async {
        reading.await.expect("the flush reads the keychain");
        flusher::release_credential_waits(h.pool(), &h.source)
            .await
            .unwrap();
        let_it_fail.send(()).unwrap();
    });
    flushed.unwrap();
    assert_eq!(h.reload(write.id).await.wait_reason, None);

    h.unlock_keychain();
    flusher::flush_source(&h.deps, &h.source).await.unwrap();
    assert_eq!(h.reload(write.id).await.state, WriteState::Sent);
}

/// #1741: the panel tells a keychain wait from a silent server by the
/// detail's prefix, a string contract across the bridge. The two tests above
/// pin the detail the queue writes; this pins that the panel reads the same
/// prefix, read off the file the component ships. A rename on either side is
/// a red here, not a panel that says "The server did not answer." again.
#[test]
fn the_panel_reads_the_keychain_prefix_the_queue_writes() {
    const PANEL: &str = include_str!("../../../../app/src/lib/shell/WriteQueue.svelte");
    let reads = format!("startsWith(\"{}\")", store::KEYCHAIN_DETAIL_PREFIX);
    assert!(
        PANEL.contains(&reads),
        "WriteQueue.svelte does not read the detail for {reads}, so a write waiting on \
         the keychain reads as a server that did not answer"
    );
}

/// #1628: *Flush now* is the reader saying "try it": it sends a
/// write waiting on a rejected credential once per press, and leaves the wait in place when the source
/// refuses again, so the tick after it sends nothing.
#[tokio::test]
async fn flush_now_sends_an_unauthorized_write_once_and_the_tick_after_it_does_not() {
    let h = harness().await;
    let ticket = h.mirror("PAY-45", "a payout fails").await;

    h.answer(Answer::Unauthorized);
    let write = h.comment(&ticket, "pressed").await;
    assert_eq!(h.asked(), 1);

    flusher::flush_now(&h.deps, Some(&h.source)).await.unwrap();
    assert_eq!(h.asked(), 2, "one send per press");
    assert_eq!(
        h.reload(write.id).await.wait_reason,
        Some(WaitReason::Unauthorized)
    );

    flusher::flush_source(&h.deps, &h.source).await.unwrap();
    assert_eq!(
        h.asked(),
        2,
        "the tick does not take the press as a release"
    );

    h.answer(Answer::Accept);
    flusher::flush_now(&h.deps, Some(&h.source)).await.unwrap();
    assert_eq!(h.delivered(), vec!["pressed".to_owned()]);
}

/// #1628: *Flush now* does not wait out the ladder either -- "I know the
/// source is back" (story 10).
#[tokio::test]
async fn flush_now_does_not_wait_out_the_ladder() {
    let h = harness().await;
    let ticket = h.mirror("PAY-46", "a payout fails").await;

    h.answer(Answer::Unreachable);
    let write = h.comment(&ticket, "back now").await;
    h.answer(Answer::Accept);
    flusher::flush_now(&h.deps, Some(&h.source)).await.unwrap();
    assert_eq!(h.reload(write.id).await.state, WriteState::Sent);
}

/// #1729: a write the local rate limit held before it was sent waits on the
/// sync ladder, as an unreachable one does. Its error is a status-less
/// `Protocol`, the shape of a refusal, but nothing reached the source, so
/// nothing was refused.
#[tokio::test]
async fn a_write_the_local_quota_held_waits_on_the_ladder() {
    let h = harness().await;
    let ticket = h.mirror("PAY-47", "a payout fails").await;

    h.answer(Answer::HeldByQuota);
    let write = h.comment(&ticket, "held by knobas").await;
    let waiting = h.reload(write.id).await;
    assert_eq!(waiting.state, WriteState::Pending);
    assert_eq!(waiting.wait_reason, Some(WaitReason::Unreachable));
    let detail = waiting.detail.unwrap_or_default();
    assert!(
        detail.contains("local rate limit") && detail.contains("was not sent"),
        "the row says the quota held it: {detail}"
    );

    flusher::flush_source(&h.deps, &h.source).await.unwrap();
    assert_eq!(h.asked(), 1, "not before the first rung, one minute");

    h.rewind(60).await;
    h.answer(Answer::Accept);
    flusher::flush_source(&h.deps, &h.source).await.unwrap();
    assert_eq!(h.asked(), 2, "on the first rung");
    assert_eq!(h.reload(write.id).await.state, WriteState::Sent);
}

/// #1729's other half: a status-less `Protocol` with no quota behind it is
/// still the source, or the adapter, refusing this write.
#[tokio::test]
async fn a_status_less_refusal_with_no_quota_behind_it_is_refused() {
    let h = harness().await;
    let ticket = h.mirror("PAY-48", "a payout fails").await;

    h.answer(Answer::RefuseWithoutStatus);
    let write = h.comment(&ticket, "no spelling for this").await;
    let refused = h.reload(write.id).await;
    assert_eq!(refused.state, WriteState::Refused);
    assert_eq!(
        refused.detail.as_deref(),
        Some("protocol: this fake cannot spell a comment for this ticket")
    );
}
