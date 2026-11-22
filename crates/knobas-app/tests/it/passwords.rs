//! The Passwords page's commands (ADR-0056) on a scratch database, over a
//! stand-in password source and a clipboard in memory: what each command
//! answers, what it keeps (the hold, the clipboard, the recent entries, the
//! code keys, the sources found down) and what a direct write leaves in the
//! mirror, the keychain and the activity log.
//!
//! The stand-in answers every secret from its name and counts what it is
//! asked, because what is under test is the app's handling of the answers.
//! The Pleasant adapter's own reads are `knobas-source-pleasant`'s tests.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use knobas_app::IpcErrorCode;
use knobas_app::commands::passwords::{
    self as cmd, AttachmentAnswer, CodeAnswer, ReadAnswer, WriteAnswer,
};
use knobas_app::passwords::PasswordsState;
use knobas_app::passwords::clipboard::ClipboardPort;
use knobas_app::sources::SourcesState;
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::passwords::{
    self, Approval, AttachmentMeta, Attachments, EntryHead, EntryRecord, FolderRecord,
    OtpAlgorithm, OtpInfo, OtpKey, PathStep, SecretAnswer, SecretAsk, SecretRef, SecretText, Write,
    Written,
};
use knobas_source::view::FileBytes;
use knobas_source::{
    AuthMethod, ConnectionInfo, Cursor, Sink, Source, SourceDescriptor, SourceError, SyncItem,
    WriteOp, WriteReceipt,
};
use knobas_sync::scheduler::{AdapterRegistry, RunConnections, SchedulerDeps, SyncEvents};
use serde_json::json;
use sqlx::PgPool;
use tauri::Manager as _;
use tauri::test::MockRuntime;

const SOURCE: &str = "pw";

/// What the stand-in was asked, and what it answers next.
#[derive(Default)]
struct Script {
    /// Every secret read, in order.
    reads: Mutex<Vec<SecretAsk>>,
    /// Every read and write fails as unreachable while set.
    down: AtomicBool,
    /// What the next direct write answers.
    written: Mutex<Option<Written>>,
    /// How many direct writes were sent.
    writes: AtomicUsize,
}

impl Script {
    fn reads(&self) -> usize {
        self.reads.lock().unwrap().len()
    }
}

fn key_of(entity: &str) -> &str {
    entity.split_once(':').map_or(entity, |(_, key)| key)
}

/// A password source that answers `<which>-<key>` for every secret, asks a
/// usage comment for the entry `asks`, and reads the attachments of `e1`.
struct Vault {
    id: String,
    script: Arc<Script>,
}

#[async_trait]
impl Source for Vault {
    async fn view(
        &self,
        _entity: &str,
        _section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, SourceError> {
        Ok(None)
    }

    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            ..knobas_source_pleasant::descriptor_template()
        }
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
        Err(SourceError::protocol(
            "a password source takes direct writes",
        ))
    }

    async fn reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError> {
        Err(SourceError::protocol(format!("no workflow for {entity:?}")))
    }

    async fn read_secret(&self, ask: &SecretAsk) -> Result<SecretAnswer, SourceError> {
        self.script.reads.lock().unwrap().push(ask.clone());
        if self.script.down.load(Ordering::SeqCst) {
            return Err(SourceError::Unreachable("the vault did not answer".into()));
        }
        let key = key_of(&ask.entity);
        if key == "asks" && ask.comment.is_none() {
            return Ok(SecretAnswer::CommentRequired {
                scope: vec!["Payments".into()],
            });
        }
        let value = match &ask.which {
            SecretRef::Password => format!("password-{key}"),
            SecretRef::Field { name } => format!("{name}-{key}"),
            SecretRef::Earlier { id } => format!("earlier-{id}-{key}"),
        };
        Ok(SecretAnswer::Value(SecretText::new(value)))
    }

    async fn passwords_write(&self, _write: Write) -> Result<Written, SourceError> {
        self.script.writes.fetch_add(1, Ordering::SeqCst);
        if self.script.down.load(Ordering::SeqCst) {
            return Err(SourceError::Unreachable("the vault did not answer".into()));
        }
        Ok(self
            .script
            .written
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Written {
                item: None,
                otp_key: None,
            }))
    }

    async fn attachment_bytes(
        &self,
        entity: &str,
        url: &str,
        _cap: u64,
    ) -> Result<Option<FileBytes>, SourceError> {
        Ok(match (key_of(entity), url) {
            ("e1", "a1") => Some(FileBytes::Read(b"first file".to_vec())),
            ("e1", "a2") => Some(FileBytes::Read(b"hello".to_vec())),
            _ => None,
        })
    }
}

struct Vaults(Arc<Script>);

impl AdapterRegistry for Vaults {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        knobas_app::sources::Registry::for_process().descriptors()
    }

    fn build(
        &self,
        instance: knobas_source::instance::SourceInstance,
    ) -> Result<Box<dyn Source>, SourceError> {
        Ok(Box::new(Vault {
            id: instance.id,
            script: Arc::clone(&self.0),
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

struct Quiet;

impl SyncEvents for Quiet {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
}

/// A clipboard in memory that "another app" can write to.
#[derive(Clone, Default)]
struct Board(Arc<Mutex<Option<String>>>);

impl Board {
    fn holds(&self) -> Option<String> {
        self.0.lock().unwrap().clone()
    }
}

impl ClipboardPort for Board {
    fn set_secret(&mut self, text: &str) -> Result<(), String> {
        *self.0.lock().unwrap() = Some(text.to_owned());
        Ok(())
    }
    fn text(&mut self) -> Result<Option<String>, String> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn clear(&mut self) -> Result<(), String> {
        *self.0.lock().unwrap() = None;
        Ok(())
    }
}

/// RFC 6238's SHA-1 seed, and another.
const SEED: &[u8] = b"12345678901234567890";
const OTHER_SEED: &[u8] = b"abcdefghijabcdefghij";

fn otp_key(seed: &[u8]) -> OtpKey {
    OtpKey {
        secret: SecretText::new(data_encoding::BASE32_NOPAD.encode(seed)),
        digits: 8,
        period: 30,
        algorithm: OtpAlgorithm::Sha1,
    }
}

fn otp_info(format: &str) -> Option<OtpInfo> {
    Some(OtpInfo {
        format: format.into(),
        digits: 8,
        period: 30,
        algorithm: OtpAlgorithm::Sha1,
    })
}

/// The path of an entry in `Tidewater / Payments`.
fn payments_path() -> Vec<PathStep> {
    vec![
        PathStep {
            id: "pw:root".into(),
            name: "Tidewater".into(),
        },
        PathStep {
            id: "pw:pay".into(),
            name: "Payments".into(),
        },
    ]
}

fn entry(key: &str, title: &str, record: EntryRecord) -> SyncItem {
    passwords::entry_item(
        EntryHead {
            source: SOURCE.into(),
            key: key.into(),
            title: title.into(),
            updated_at: None,
        },
        &record,
    )
}

/// An entry in Payments the account may read.
fn in_payments() -> EntryRecord {
    EntryRecord {
        folder: Some("pw:pay".into()),
        path: payments_path(),
        username: Some("mara".into()),
        can_read: true,
        can_modify: true,
        ..EntryRecord::default()
    }
}

/// The mirror the tests start from: `Tidewater / Payments` and its entries.
fn seed() -> Vec<SyncItem> {
    vec![
        passwords::folder_item(
            SOURCE,
            "root",
            &FolderRecord {
                name: "Tidewater".into(),
                ..FolderRecord::default()
            },
        ),
        passwords::folder_item(
            SOURCE,
            "pay",
            &FolderRecord {
                name: "Payments".into(),
                parent: Some("pw:root".into()),
                ..FolderRecord::default()
            },
        ),
        entry(
            "e1",
            "Stripe",
            EntryRecord {
                otp: otp_info("keepass"),
                attachments: Attachments::Listed(vec![
                    AttachmentMeta {
                        id: "a1".into(),
                        name: "first.txt".into(),
                        size: Some(10),
                    },
                    AttachmentMeta {
                        id: "a2".into(),
                        name: "hello.txt".into(),
                        size: Some(5),
                    },
                ]),
                ..in_payments()
            },
        ),
        entry("e2", "Adyen", in_payments()),
        entry(
            "e3",
            "PayPal",
            EntryRecord {
                otp: otp_info("keepass"),
                ..in_payments()
            },
        ),
        entry(
            "odd",
            "Klarna",
            EntryRecord {
                otp: otp_info(passwords::OTP_UNREADABLE),
                ..in_payments()
            },
        ),
        entry(
            "locked",
            "Mollie",
            EntryRecord {
                otp: otp_info("keepass"),
                approval: Approval::Required { approver: None },
                ..in_payments()
            },
        ),
        entry("asks", "Braintree", in_payments()),
    ]
}

/// When the seeded run of the source ended.
const SYNCED_AT: &str = "2026-11-20T10:00:00Z";

struct Fixture {
    app: tauri::App<MockRuntime>,
    pool: PgPool,
    secrets: Arc<dyn SecretStore>,
    script: Arc<Script>,
    board: Board,
}

impl Fixture {
    async fn new(name: &str) -> Self {
        let connector = knobas_db::test_util::scratch_database(name).await;
        let pool = connector.pool(4).await.expect("a pool");
        let secrets: Arc<dyn SecretStore> = Arc::new(MemoryStore::new());
        secrets
            .put(
                &knobas_secrets::KeychainAccount::source(SOURCE),
                &Secret::just(AuthMethod::UserPassword, "a-password"),
            )
            .expect("the credential is stored");
        knobas_sync::config::insert(
            &pool,
            &knobas_sync::config::InsertConfig {
                id: SOURCE.to_owned(),
                adapter_kind: "pleasant".to_owned(),
                display_name: "Team passwords".to_owned(),
                base_url: "https://vault.example".to_owned(),
                auth_kind: knobas_sync::config::AuthKind::Method(AuthMethod::UserPassword),
                config: json!({ "username": "mara" }),
                sync_interval_secs: 86_400,
                enabled: true,
                sync_by_hand: true,
            },
        )
        .await
        .expect("the source row is written");
        let script = Arc::new(Script::default());
        let mut conn = connector.connect().await.expect("a connection");
        let vault = Vault {
            id: SOURCE.into(),
            script: Arc::clone(&script),
        };
        for item in seed() {
            knobas_sync::refresh::write_direct(&mut conn, &vault, item)
                .await
                .expect("the mirror is seeded");
        }
        drop(conn);
        sqlx::query(
            "insert into knobas.sync_run (source_id, trigger, started_at, finished_at, outcome)
             values ($1, 'manual', $2::timestamptz, $2::timestamptz, 'ok')",
        )
        .bind(SOURCE)
        .bind(SYNCED_AT)
        .execute(&pool)
        .await
        .expect("the run is recorded");
        knobas_secrets::otp::replace(
            secrets.as_ref(),
            SOURCE,
            &[("pw:e1".to_owned(), otp_key(SEED))].into_iter().collect(),
        )
        .expect("the code keys are kept");

        let registry: Arc<dyn AdapterRegistry> = Arc::new(Vaults(Arc::clone(&script)));
        let scheduler = knobas_sync::scheduler::Scheduler::start(SchedulerDeps {
            pool: pool.clone(),
            connections: Arc::new(Connections(connector)),
            registry: Arc::clone(&registry),
            secrets: secrets.clone(),
            events: Arc::new(Quiet),
            timing: knobas_sync::scheduler::SchedulerTiming {
                startup_delay: Duration::from_secs(3600),
                tick: Duration::from_secs(3600),
                shutdown_grace: knobas_sync::scheduler::SHUTDOWN_GRACE,
            },
            quit: knobas_sync::scheduler::Quit::default(),
        })
        .await
        .expect("a scheduler");
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("a mock app");
        app.manage(SourcesState {
            live: Default::default(),
            pool: pool.clone(),
            scheduler,
            secrets: secrets.clone(),
            registry,
        });
        let board = Board::default();
        app.manage(PasswordsState::new(
            Box::new(board.clone()),
            Arc::new(|_| {}),
        ));
        Self {
            app,
            pool,
            secrets,
            script,
            board,
        }
    }

    fn handle(&self) -> tauri::AppHandle<MockRuntime> {
        self.app.handle().clone()
    }

    fn pw(&self) -> tauri::State<'_, PasswordsState> {
        self.app.state::<PasswordsState>()
    }

    async fn show(&self, entity: &str, which: SecretRef) -> ReadAnswer {
        cmd::password_show(self.handle(), entity.into(), which, None)
            .await
            .expect("the show answers")
    }

    async fn copy(&self, entity: &str, which: SecretRef) -> ReadAnswer {
        cmd::password_copy(self.handle(), entity.into(), which, None)
            .await
            .expect("the copy answers")
    }

    async fn code(&self, entity: &str) -> CodeAnswer {
        cmd::otp_code(self.handle(), entity.into())
            .await
            .expect("the code answers")
    }

    async fn held(&self) -> knobas_app::passwords::reads::Held {
        cmd::passwords_held(self.handle())
            .await
            .expect("the page's read answers")
    }

    async fn write(&self, write: Write, answer: Written) -> WriteAnswer {
        *self.script.written.lock().unwrap() = Some(answer);
        cmd::passwords_write(self.handle(), write)
            .await
            .expect("the write answers")
    }

    /// The `path` the mirror holds for `entity`.
    async fn mirrored_path(&self, entity: &str) -> serde_json::Value {
        sqlx::query_scalar("select payload->'path' from sync.item where entity_id = $1")
            .bind(entity)
            .fetch_one(&self.pool)
            .await
            .expect("the entry is mirrored")
    }
}

fn pin() -> SecretRef {
    SecretRef::Field { name: "PIN".into() }
}

/// `at` as an instant, which must be RFC 3339.
fn instant(at: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(at)
        .unwrap_or_else(|e| panic!("{at:?} is not RFC 3339: {e}"))
        .to_utc()
}

/// `at` is `secs` from now, give or take two seconds.
fn about_from_now(at: &str, secs: i64) {
    let off = (instant(at) - chrono::Utc::now()).num_seconds() - secs;
    assert!(off.abs() <= 2, "{at} is not {secs} s from now");
}

/// The code `seed` makes now, or a moment ago: `answer`'s.
fn is_code_of(answer: &CodeAnswer, seed: &[u8], before: &str) {
    let CodeAnswer::Code {
        code,
        digits,
        period,
        left_ms,
    } = answer
    else {
        panic!("no code: {answer:?}");
    };
    let now = knobas_app::passwords::otp::code_now(&otp_key(seed))
        .unwrap()
        .code;
    assert!(
        code == &now || code == before,
        "{code} is not the code of {seed:?}"
    );
    assert_eq!((*digits, *period), (8, 30));
    assert!((1..=30_000).contains(left_ms), "{left_ms}");
}

fn code_now(seed: &[u8]) -> String {
    knobas_app::passwords::otp::code_now(&otp_key(seed))
        .unwrap()
        .code
}

#[tokio::test]
async fn the_pages_read_names_the_source_its_folders_and_its_entries() {
    let fx = Fixture::new("pw_held").await;
    let held = fx.held().await;
    let [source] = &held.sources[..] else {
        panic!("one source: {:?}", held.sources);
    };
    assert_eq!(source.id, SOURCE);
    assert_eq!(source.name, "Team passwords");
    assert_eq!(source.adapter_name, "Pleasant Password Server");
    assert_eq!(
        Some(source.offers.clone()),
        knobas_source_pleasant::descriptor_template().passwords,
        "the offers are its kind's"
    );
    assert_eq!(source.account.as_deref(), Some("mara"));
    assert!(source.by_hand);
    assert_eq!(source.synced_at.as_deref(), Some(SYNCED_AT));
    assert!(!source.never_synced);
    assert_eq!(source.down_at, None);
    assert_eq!(source.skipped_root.as_deref(), Some("pw:root"));
    assert_eq!(held.folders.len(), 1);
    assert_eq!(held.folders[0].id, "pw:pay");
    assert_eq!(held.folders[0].parent, None);
    assert_eq!(held.entries.len(), 6);
    let e1 = held.entries.iter().find(|r| r.0 == "pw:e1").unwrap();
    assert_eq!(e1.3, "Stripe");
    assert_eq!(e1.2.as_deref(), Some("pw:pay"));
    assert_eq!(e1.7, knobas_app::passwords::reads::HAS_CODE);
    assert!(held.recent.is_empty());

    let entry = cmd::passwords_entry(fx.handle(), "pw:e1".into())
        .await
        .unwrap()
        .expect("the entry");
    assert_eq!(entry.title, "Stripe");
    assert_eq!(entry.folder.as_deref(), Some("pw:pay"));
    assert_eq!(entry.username.as_deref(), Some("mara"));
    assert!(
        cmd::passwords_entry(fx.handle(), "pw:nothing".into())
            .await
            .unwrap()
            .is_none()
    );

    let facts = cmd::password_source_facts(fx.handle(), SOURCE.into())
        .await
        .unwrap();
    assert_eq!((facts.entries, facts.folders), (6, 2));
    assert_eq!(
        facts.formats,
        [
            ("keepass".to_owned(), 3),
            (passwords::OTP_UNREADABLE.to_owned(), 1)
        ]
    );
}

#[tokio::test]
async fn a_show_reads_the_source_once_and_answers_the_value_with_its_times() {
    let fx = Fixture::new("pw_show").await;
    let ReadAnswer::Shown {
        value,
        read_at,
        until,
    } = fx.show("pw:e1", SecretRef::Password).await
    else {
        panic!("not shown");
    };
    assert_eq!(value.expose(), "password-e1");
    about_from_now(&read_at, 0);
    about_from_now(&until, 30);
    assert_eq!(fx.script.reads(), 1);
    {
        let reads = fx.script.reads.lock().unwrap();
        assert_eq!(reads[0].entity, "pw:e1");
        assert_eq!(reads[0].folder.as_deref(), Some("pw:pay"));
    }

    // Shown again from the hold: the source is not asked.
    let ReadAnswer::Shown { value, until, .. } = fx.show("pw:e1", SecretRef::Password).await else {
        panic!("not shown again");
    };
    assert_eq!(value.expose(), "password-e1");
    about_from_now(&until, 30);
    assert_eq!(fx.script.reads(), 1, "the hold answered");

    // The recent entries: newest first, each once, at most four.
    for entity in ["pw:e2", "pw:e1", "pw:e3", "pw:odd", "pw:asks"] {
        let _ = fx.show(entity, SecretRef::Field { name: "x".into() }).await;
    }
    assert_eq!(
        fx.held().await.recent,
        ["pw:odd", "pw:e3", "pw:e1", "pw:e2"]
    );

    // An entity that is no entry asks nothing.
    assert!(matches!(
        fx.show("pw:nothing", SecretRef::Password).await,
        ReadAnswer::NotOffered
    ));
    // A source that asks for a usage comment says where.
    let before = fx.script.reads();
    let ReadAnswer::CommentRequired { scope } = fx.show("pw:asks", SecretRef::Password).await
    else {
        panic!("no comment asked");
    };
    assert_eq!(scope, ["Payments"]);
    assert_eq!(fx.script.reads(), before + 1);
}

#[tokio::test]
async fn hide_takes_one_secret_an_entrys_password_or_all() {
    let fx = Fixture::new("pw_hide").await;
    fx.show("pw:e1", SecretRef::Password).await;
    fx.show("pw:e1", pin()).await;
    fx.show("pw:e2", SecretRef::Password).await;
    assert_eq!(fx.pw().hold.len(), 3);

    cmd::password_hide(fx.handle(), Some("pw:e1".into()), Some(pin())).unwrap();
    assert_eq!(fx.pw().hold.len(), 2, "only the PIN was hidden");
    assert!(fx.pw().hold.get("pw:e1", &pin()).is_none());
    assert!(fx.pw().hold.get("pw:e1", &SecretRef::Password).is_some());

    cmd::password_hide(fx.handle(), Some("pw:e2".into()), None).unwrap();
    assert_eq!(fx.pw().hold.len(), 1, "only e2's password was hidden");
    assert!(fx.pw().hold.get("pw:e1", &SecretRef::Password).is_some());

    cmd::password_hide(fx.handle(), None, None).unwrap();
    assert!(fx.pw().hold.is_empty(), "the page was left");
    let reads = fx.script.reads();
    fx.show("pw:e1", SecretRef::Password).await;
    assert_eq!(
        fx.script.reads(),
        reads + 1,
        "a hidden secret is read again"
    );
}

#[tokio::test]
async fn a_copy_goes_to_the_clipboard_named_and_clear_and_forget_take_it_back() {
    let fx = Fixture::new("pw_copy").await;
    let ReadAnswer::Copied { read_at, until } = fx.copy("pw:e1", pin()).await else {
        panic!("not copied");
    };
    about_from_now(&read_at, 0);
    about_from_now(&until, 30);
    assert_eq!(fx.board.holds().as_deref(), Some("PIN-e1"));
    let state = cmd::clipboard_state(fx.handle()).unwrap();
    assert_eq!(state.what.as_deref(), Some("PIN"));
    assert_eq!(state.entity_id.as_deref(), Some("pw:e1"));
    assert_eq!(state.until, Some(until));
    assert_eq!(fx.held().await.recent, ["pw:e1"], "a copy is a use");

    fx.copy("pw:e2", SecretRef::Password).await;
    assert_eq!(fx.board.holds().as_deref(), Some("password-e2"));
    assert_eq!(
        cmd::clipboard_state(fx.handle()).unwrap().what.as_deref(),
        Some("password")
    );
    fx.copy("pw:e2", SecretRef::Earlier { id: "h1".into() })
        .await;
    assert_eq!(
        cmd::clipboard_state(fx.handle()).unwrap().what.as_deref(),
        Some("earlier password")
    );

    // *Clear now* takes back what is still knobas's, and answers so.
    assert!(cmd::clipboard_clear(fx.handle()).unwrap());
    assert_eq!(fx.board.holds(), None);
    assert!(!cmd::clipboard_clear(fx.handle()).unwrap(), "nothing left");

    // A copy in the window drops the deadline and leaves the clipboard.
    fx.copy("pw:e1", SecretRef::Password).await;
    cmd::clipboard_forget(fx.handle()).unwrap();
    assert_eq!(cmd::clipboard_state(fx.handle()).unwrap().what, None);
    assert_eq!(fx.board.holds().as_deref(), Some("password-e1"));

    // A shown secret is copied from the hold: the source is not asked.
    fx.show("pw:e3", SecretRef::Password).await;
    let reads = fx.script.reads();
    fx.copy("pw:e3", SecretRef::Password).await;
    assert_eq!(fx.script.reads(), reads);
    assert_eq!(fx.board.holds().as_deref(), Some("password-e3"));

    // A secret typed in a dialog.
    let copied =
        cmd::clipboard_copy_secret(fx.handle(), "typed".into(), "new password".into()).unwrap();
    about_from_now(&copied.until, 30);
    assert_eq!(fx.board.holds().as_deref(), Some("typed"));

    // The quit hides what is shown and takes back what is still knobas's.
    assert!(!fx.pw().hold.is_empty());
    fx.pw().quit();
    assert!(fx.pw().hold.is_empty());
    assert_eq!(fx.board.holds(), None);
}

#[tokio::test]
async fn a_source_that_does_not_answer_is_down_until_a_call_succeeds() {
    let fx = Fixture::new("pw_down").await;
    fx.script.down.store(true, Ordering::SeqCst);
    let ReadAnswer::Unreachable { message } = fx.show("pw:e1", SecretRef::Password).await else {
        panic!("not unreachable");
    };
    assert!(message.contains("did not answer"), "{message}");
    let down_at = fx.held().await.sources[0]
        .down_at
        .clone()
        .expect("found down");
    about_from_now(&down_at, 0);

    fx.script.down.store(false, Ordering::SeqCst);
    assert!(matches!(
        fx.show("pw:e1", SecretRef::Password).await,
        ReadAnswer::Shown { .. }
    ));
    assert_eq!(fx.held().await.sources[0].down_at, None, "up again");
}

#[tokio::test]
async fn a_code_is_made_from_the_kept_key_read_once_per_run() {
    let fx = Fixture::new("pw_code").await;
    let before = code_now(SEED);
    is_code_of(&fx.code("pw:e1").await, SEED, &before);
    assert!(matches!(fx.code("pw:e2").await, CodeAnswer::NotOffered));
    assert!(matches!(fx.code("pw:e3").await, CodeAnswer::NeedsSync));
    assert!(matches!(fx.code("pw:odd").await, CodeAnswer::Unreadable));
    assert!(matches!(fx.code("pw:locked").await, CodeAnswer::Hidden));
    assert!(matches!(
        fx.code("pw:nothing").await,
        CodeAnswer::NotOffered
    ));
    assert_eq!(fx.script.reads(), 0, "a code reads nothing from the source");

    // The keychain changes under a running session: the keys read after
    // the last run still hold.
    knobas_secrets::otp::replace(
        fx.secrets.as_ref(),
        SOURCE,
        &[("pw:e1".to_owned(), otp_key(OTHER_SEED))]
            .into_iter()
            .collect(),
    )
    .unwrap();
    let before = code_now(SEED);
    is_code_of(&fx.code("pw:e1").await, SEED, &before);

    // A newer run reads them again.
    sqlx::query(
        "insert into knobas.sync_run (source_id, trigger, started_at, finished_at, outcome)
         values ($1, 'manual', now(), now(), 'ok')",
    )
    .bind(SOURCE)
    .execute(&fx.pool)
    .await
    .unwrap();
    let before = code_now(OTHER_SEED);
    is_code_of(&fx.code("pw:e1").await, OTHER_SEED, &before);

    // A copied code goes to the clipboard as one.
    let answer = cmd::otp_copy(fx.handle(), "pw:e1".into()).await.unwrap();
    let CodeAnswer::Code { code, .. } = &answer else {
        panic!("no code: {answer:?}");
    };
    assert_eq!(fx.board.holds().as_ref(), Some(code));
    assert_eq!(
        cmd::clipboard_state(fx.handle()).unwrap().what.as_deref(),
        Some("one-time code")
    );
}

#[tokio::test]
async fn a_written_entry_reaches_the_mirror_keeping_the_path_it_had() {
    let fx = Fixture::new("pw_write_path").await;
    // The answer of one listing carries no ancestors: the path is kept.
    let answer = fx
        .write(
            Write::EditEntry {
                entity: "pw:e2".into(),
                title: Some("Adyen live".into()),
                username: None,
                address: None,
                notes: None,
                expires: None,
                folder: None,
                otp_key: None,
                comment: None,
                listed_in: None,
            },
            Written {
                item: Some(entry(
                    "e2",
                    "Adyen live",
                    EntryRecord {
                        path: Vec::new(),
                        ..in_payments()
                    },
                )),
                otp_key: None,
            },
        )
        .await;
    assert!(
        matches!(&answer, WriteAnswer::Written { entity_id } if entity_id == "pw:e2"),
        "{answer:?}"
    );
    let mirrored = cmd::passwords_entry(fx.handle(), "pw:e2".into())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(mirrored.title, "Adyen live", "the mirror has the write");
    assert_eq!(
        fx.mirrored_path("pw:e2").await,
        serde_json::to_value(payments_path()).unwrap()
    );

    // An answer that carries a path is taken as it is.
    let moved = vec![PathStep {
        id: "pw:pay".into(),
        name: "Payments (EU)".into(),
    }];
    fx.write(
        Write::EditEntry {
            entity: "pw:e1".into(),
            title: None,
            username: None,
            address: None,
            notes: Some(Some("rotated".into())),
            expires: None,
            folder: None,
            otp_key: None,
            comment: None,
            listed_in: None,
        },
        Written {
            item: Some(entry(
                "e1",
                "Stripe",
                EntryRecord {
                    path: moved.clone(),
                    ..in_payments()
                },
            )),
            otp_key: None,
        },
    )
    .await;
    assert_eq!(
        fx.mirrored_path("pw:e1").await,
        serde_json::to_value(&moved).unwrap()
    );

    let lines: Vec<(String, Option<String>)> =
        sqlx::query_as("select verb, entity_id from knobas.activity order by id")
            .fetch_all(&fx.pool)
            .await
            .unwrap();
    assert_eq!(
        lines,
        [
            ("Edited an entry".to_owned(), Some("pw:e2".to_owned())),
            ("Edited an entry".to_owned(), Some("pw:e1".to_owned())),
        ]
    );
}

#[tokio::test]
async fn a_written_code_key_is_kept_and_read_at_the_next_code() {
    let fx = Fixture::new("pw_write_key").await;
    // The keys are read, and kept for the session.
    let before = code_now(SEED);
    is_code_of(&fx.code("pw:e1").await, SEED, &before);
    assert!(matches!(fx.code("pw:e3").await, CodeAnswer::NeedsSync));

    let answer = fx
        .write(
            Write::SetPassword {
                entity: "pw:e3".into(),
                password: SecretText::new("new"),
                comment: None,
                listed_in: None,
            },
            Written {
                item: None,
                otp_key: Some(otp_key(OTHER_SEED)),
            },
        )
        .await;
    assert!(
        matches!(&answer, WriteAnswer::Written { entity_id } if entity_id == "pw:e3"),
        "{answer:?}"
    );
    let before = code_now(OTHER_SEED);
    is_code_of(&fx.code("pw:e3").await, OTHER_SEED, &before);
    let before = code_now(SEED);
    is_code_of(&fx.code("pw:e1").await, SEED, &before);

    let answer = fx
        .write(
            Write::CreateEntry {
                source: SOURCE.into(),
                folder: Some("pw:pay".into()),
                title: "Wise".into(),
                username: None,
                address: None,
                notes: None,
                expires: None,
                password: SecretText::new("p"),
                otp_key: None,
                comment: None,
            },
            Written {
                item: Some(entry("new", "Wise", in_payments())),
                otp_key: None,
            },
        )
        .await;
    assert!(
        matches!(&answer, WriteAnswer::Written { entity_id } if entity_id == "pw:new"),
        "{answer:?}"
    );
    let lines: Vec<(String, Option<String>)> =
        sqlx::query_as("select verb, entity_id from knobas.activity order by id")
            .fetch_all(&fx.pool)
            .await
            .unwrap();
    assert_eq!(
        lines,
        [
            ("Changed a password".to_owned(), Some("pw:e3".to_owned())),
            ("Added an entry".to_owned(), Some("pw:new".to_owned())),
        ]
    );

    // A source that does not answer keeps nothing, and is down.
    fx.script.down.store(true, Ordering::SeqCst);
    let answer = fx
        .write(
            Write::RequestAccess {
                entity: "pw:locked".into(),
                comment: "on call".into(),
            },
            Written {
                item: None,
                otp_key: None,
            },
        )
        .await;
    assert!(
        matches!(answer, WriteAnswer::Unreachable { .. }),
        "{answer:?}"
    );
    assert!(fx.held().await.sources[0].down_at.is_some());
}

#[tokio::test]
async fn an_attachment_is_read_live_by_its_id() {
    let fx = Fixture::new("pw_attachment").await;
    let answer = cmd::password_attachment(fx.handle(), "pw:e1".into(), "a2".into())
        .await
        .unwrap();
    let AttachmentAnswer::Read {
        name,
        size,
        data,
        text,
    } = answer
    else {
        panic!("not read: {answer:?}");
    };
    assert_eq!(name, "hello.txt");
    assert_eq!(size, 5);
    assert_eq!(data, "aGVsbG8=");
    assert_eq!(text.as_deref(), Some("hello"));
    assert_eq!(fx.held().await.recent, ["pw:e1"]);

    for (entity, attachment) in [("pw:e1", "a9"), ("pw:e2", "a1"), ("pw:nothing", "a1")] {
        let answer = cmd::password_attachment(fx.handle(), entity.into(), attachment.into())
            .await
            .unwrap();
        assert!(
            matches!(answer, AttachmentAnswer::NotOffered),
            "{entity} {attachment}: {answer:?}"
        );
        // Saving one the entry does not list writes nothing.
        let saved =
            cmd::password_attachment_save(fx.handle(), entity.into(), attachment.into()).await;
        assert!(
            matches!(&saved, Err(e) if e.code == IpcErrorCode::NotFound),
            "{entity} {attachment}: {saved:?}"
        );
    }
}

/// *Open web interface* (ux30): the address is the source's typed base URL
/// and its adapter's declared template, for an entry, a folder or the front
/// page, and nothing for an id that is no entry or folder of the source.
#[tokio::test]
async fn the_web_interface_opens_an_entry_a_folder_or_the_front_page() {
    use knobas_app::passwords::web::WebAt;
    let fx = Fixture::new("pw_web_link").await;
    let held = fx.held().await;
    assert_eq!(
        held.sources[0].web,
        knobas_app::passwords::web::WebPages {
            entry: true,
            folder: true
        },
        "Pleasant declares both pages"
    );
    let link = |item: Option<&str>| {
        let item = item.map(str::to_owned);
        let handle = fx.handle();
        async move {
            cmd::password_web_link(handle, SOURCE.into(), item)
                .await
                .expect("the link answers")
        }
    };
    let entry = link(Some("pw:e1")).await.expect("an entry's link");
    assert_eq!(entry.at, WebAt::Entry);
    assert_eq!(
        entry.url,
        "https://vault.example/SignIn/Direct?ReturnUrl=%2FWebClient%2FMain%3FitemId%3De1"
    );
    let folder = link(Some("pw:pay")).await.expect("a folder's link");
    assert_eq!(folder.at, WebAt::Folder);
    assert!(folder.url.ends_with("itemId%3Dpay"), "{}", folder.url);
    let home = link(None).await.expect("the front page");
    assert_eq!(
        (home.at, home.url.as_str()),
        (
            WebAt::Home,
            "https://vault.example/SignIn/Direct?ReturnUrl=%2FWebClient%2FMain"
        )
    );
    // Another source's id, or one that is gone, opens the front page.
    let other = link(Some("jira:PAY-1")).await.expect("the front page");
    assert_eq!(other.at, WebAt::Home);
    // A source that is no password source has no web interface here.
    assert_eq!(
        cmd::password_web_link(fx.handle(), "nothing".into(), None)
            .await
            .unwrap(),
        None
    );
}
