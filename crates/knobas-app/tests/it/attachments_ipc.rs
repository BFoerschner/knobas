//! A ticket's attachment bytes and download (spec #2396, #2405) through their
//! commands, over the embedded PostgreSQL with a fake Jira: what is under test
//! is the app's half -- the file looked up in the mirror's list (never an
//! address from the shell), the 5 MiB cap, the 403 that reads *not
//! permitted*, and the numbered file a download leaves. The adapter's half is
//! `knobas-source-jira`'s (`tests/it/mockd.rs`).

use std::sync::{Arc, Mutex};

use knobas_app::IpcErrorCode;
use knobas_app::commands::entity::{
    AttachmentBytes, attachment_bytes_inner, download_attachment_inner,
};
use knobas_source::run::DownloadSink;
use knobas_source::view::{ATTACHMENT_CAP, FileBytes};
use knobas_source::{Source, SourceError};

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// What the fake Jira was asked: each read's address and cap.
type Asked = Arc<Mutex<Vec<(String, u64)>>>;

/// How the fake Jira answers every read.
#[derive(Clone, Copy)]
enum Answer {
    Bytes,
    Forbidden,
    Unreachable,
}

struct Fake {
    asked: Asked,
    answer: Answer,
}

impl knobas_sync::scheduler::AdapterRegistry for Fake {
    fn descriptors(&self) -> Vec<knobas_source::SourceDescriptor> {
        knobas_app::sources::Registry::for_process().descriptors()
    }
    fn build(
        &self,
        instance: knobas_source::instance::SourceInstance,
    ) -> Result<Box<dyn Source>, SourceError> {
        Ok(Box::new(Answers {
            id: instance.id,
            asked: self.asked.clone(),
            answer: self.answer,
        }))
    }
}

/// A Jira source of its own namespace whose attachments are the bytes
/// `PNGBYTES`, or the answer's failure.
struct Answers {
    id: String,
    asked: Asked,
    answer: Answer,
}

const BYTES: &[u8] = b"\x89PNGBYTES";

impl Answers {
    fn answer(&self, url: &str, cap: u64) -> Result<(), SourceError> {
        self.asked.lock().unwrap().push((url.to_owned(), cap));
        match self.answer {
            Answer::Bytes => Ok(()),
            Answer::Forbidden => Err(SourceError::Unauthorized { status: Some(403) }),
            Answer::Unreachable => Err(SourceError::Unreachable("no route to host".to_owned())),
        }
    }
}

#[async_trait::async_trait]
impl Source for Answers {
    fn descriptor(&self) -> knobas_source::SourceDescriptor {
        use knobas_sync::scheduler::AdapterRegistry as _;
        let mut d = knobas_app::sources::Registry::for_process()
            .descriptors()
            .into_iter()
            .find(|d| d.adapter_kind == "jira")
            .expect("Jira is compiled in");
        d.id = self.id.clone();
        d
    }
    async fn test_connection(&self) -> Result<knobas_source::ConnectionInfo, SourceError> {
        Ok(knobas_source::ConnectionInfo::default())
    }
    async fn sync(
        &self,
        cursor: Option<knobas_source::Cursor>,
        _sink: &mut (dyn knobas_source::Sink + Send),
    ) -> Result<knobas_source::Cursor, SourceError> {
        Ok(cursor.unwrap_or_default())
    }
    async fn write(
        &self,
        _op: knobas_source::WriteOp,
    ) -> Result<knobas_source::WriteReceipt, SourceError> {
        Err(SourceError::protocol("read-only"))
    }
    async fn reachable_transitions(&self, _entity: &str) -> Result<Vec<String>, SourceError> {
        Err(SourceError::protocol("no workflow"))
    }
    async fn view(
        &self,
        _entity: &str,
        _section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, SourceError> {
        Ok(None)
    }
    async fn attachment_bytes(
        &self,
        _entity: &str,
        url: &str,
        cap: u64,
    ) -> Result<Option<FileBytes>, SourceError> {
        self.answer(url, cap)?;
        Ok(Some(FileBytes::Read(BYTES.to_vec())))
    }
    async fn download_attachment(
        &self,
        _entity: &str,
        url: &str,
        name: &str,
        sink: &mut dyn DownloadSink,
    ) -> Result<bool, SourceError> {
        self.answer(url, u64::MAX)?;
        sink.start(name).await?;
        sink.write(BYTES).await?;
        Ok(true)
    }
}

/// A Jira source nothing else in this database uses, holding one ticket with
/// a 96 KB screenshot (`50001`) and a 6 MiB one (`50009`) in its list.
async fn ticket(pool: &sqlx::PgPool) -> (String, String) {
    let source = format!("jr{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
    crate::support::source_config(pool, &source, "jira", "Tidewater Jira", &serde_json::json!({}))
        .await;
    let id = format!("{source}:PAY-231");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', $1)")
        .bind(&id)
        .execute(pool)
        .await
        .unwrap();
    let file = |id: &str, name: &str, size: u64| {
        serde_json::json!({
            "id": id, "filename": name, "mimeType": "image/png", "size": size,
            "author": { "name": "mara", "displayName": "Mara Lindqvist" },
            "created": "2026-08-19T17:02:00.000+0200",
            "content": format!("https://jira.example/secure/attachment/{id}/{name}"),
        })
    };
    let payload = serde_json::json!({ "key": "PAY-231", "fields": {
        "summary": "Retry failed SEPA payouts",
        "attachment": [
            file("50001", "retry-timeline.png", 98_304),
            file("50009", "full-trace.png", 6 * 1024 * 1024),
        ],
    } });
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                item_updated_at)
         values ($1, $2, 'ticket', $1, '', $3, now())",
    )
    .bind(&id)
    .bind(&source)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
    (source, id)
}

fn fake(answer: Answer) -> (Arc<dyn knobas_sync::scheduler::AdapterRegistry>, Asked) {
    let asked = Asked::default();
    (
        Arc::new(Fake {
            asked: asked.clone(),
            answer,
        }),
        asked,
    )
}

fn no_secrets() -> Arc<dyn knobas_secrets::SecretStore> {
    Arc::new(knobas_secrets::MemoryStore::new())
}

async fn bytes(
    pool: &sqlx::PgPool,
    registry: &Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    id: &str,
    attachment: &str,
) -> Result<AttachmentBytes, knobas_app::IpcError> {
    attachment_bytes_inner(
        pool,
        &no_secrets(),
        registry,
        &knobas_app::sources::live::LiveSources::default(),
        id,
        attachment,
    )
    .await
}

/// The listed file's bytes come back with its media type, read from the
/// address the mirror lists and capped at 5 MiB.
#[tokio::test]
async fn a_listed_attachments_bytes_come_back_with_its_media_type_under_the_cap() {
    let pool = pool().await;
    let (_, id) = ticket(&pool).await;
    let (registry, asked) = fake(Answer::Bytes);

    let read = bytes(&pool, &registry, &id, "50001").await.unwrap();
    assert_eq!(
        read,
        AttachmentBytes {
            mime: "image/png".to_owned(),
            data: "iVBOR0JZVEVT".to_owned(),
        }
    );
    assert_eq!(
        *asked.lock().unwrap(),
        [(
            "https://jira.example/secure/attachment/50001/retry-timeline.png".to_owned(),
            ATTACHMENT_CAP
        )]
    );
    assert_eq!(ATTACHMENT_CAP, 5 * 1024 * 1024);
}

/// A file the list says is over 5 MiB is refused with its size, and Jira is
/// never asked; an id the list does not hold is not found, and not asked
/// either.
#[tokio::test]
async fn an_attachment_over_the_cap_or_not_listed_is_refused_unasked() {
    let pool = pool().await;
    let (_, id) = ticket(&pool).await;
    let (registry, asked) = fake(Answer::Bytes);

    let large = bytes(&pool, &registry, &id, "50009").await.unwrap_err();
    assert_eq!(large.code, IpcErrorCode::Invalid);
    assert_eq!(
        large.message,
        "full-trace.png is 6 MiB; knobas previews an attachment up to 5 MiB"
    );
    let unlisted = bytes(&pool, &registry, &id, "70000").await.unwrap_err();
    assert_eq!(unlisted.code, IpcErrorCode::NotFound);
    assert!(asked.lock().unwrap().is_empty(), "{asked:?}");
}

/// A 403 names its status, so the page reads *not permitted*; a Jira that
/// cannot be reached is `unreachable`. Both carry the source.
#[tokio::test]
async fn a_403_names_its_status_and_an_unreachable_jira_is_unreachable() {
    let pool = pool().await;
    let (source, id) = ticket(&pool).await;

    let (registry, _) = fake(Answer::Forbidden);
    let refused = bytes(&pool, &registry, &id, "50001").await.unwrap_err();
    assert_eq!(refused.code, IpcErrorCode::Unauthorized);
    assert_eq!(
        refused.message,
        "Tidewater Jira does not permit this account to read retry-timeline.png (HTTP 403)"
    );
    assert_eq!(refused.source_id.as_deref(), Some(source.as_str()));

    let (registry, _) = fake(Answer::Unreachable);
    let down = bytes(&pool, &registry, &id, "50001").await.unwrap_err();
    assert_eq!(down.code, IpcErrorCode::Unreachable);
    assert_eq!(down.source_id.as_deref(), Some(source.as_str()));
}

/// *Download* saves the file into the folder under its listed name, and a
/// second download beside it is numbered; one that fails leaves nothing.
#[tokio::test]
async fn a_download_lands_under_its_name_and_the_next_one_is_numbered() {
    let pool = pool().await;
    let (_, id) = ticket(&pool).await;
    let dir = tempfile::tempdir().unwrap();
    let download = |registry: Arc<dyn knobas_sync::scheduler::AdapterRegistry>| {
        let (pool, id, dir) = (pool.clone(), id.clone(), dir.path().to_owned());
        async move {
            download_attachment_inner(
                &pool,
                &no_secrets(),
                &registry,
                &knobas_app::sources::live::LiveSources::default(),
                &id,
                "50001",
                &dir,
            )
            .await
        }
    };

    let first = download(fake(Answer::Bytes).0).await.unwrap();
    let second = download(fake(Answer::Bytes).0).await.unwrap();
    assert_eq!(first, dir.path().join("retry-timeline.png").display().to_string());
    assert_eq!(
        second,
        dir.path().join("retry-timeline (1).png").display().to_string()
    );
    assert_eq!(std::fs::read(&second).unwrap(), BYTES);

    let failed = download(fake(Answer::Unreachable).0).await.unwrap_err();
    assert_eq!(failed.code, IpcErrorCode::Unreachable);
    let mut names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, ["retry-timeline (1).png", "retry-timeline.png"]);
}
