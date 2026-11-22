//! Reads of the working tree for the Files tab on *Working tree* (ux29): a JAR the tab draws is opened from the
//! checkout as it is on disk (`archive::open` at `~working-tree`), not from `HEAD`'s blob and never from the source.

use std::path::Path;
use std::sync::Arc;

use knobas_app::checkout::set_override;
use knobas_app::sources::Registry;
use knobas_app::sources::live::LiveSources;
use knobas_secrets::{MemoryStore, SecretStore};
use knobas_sync::scheduler::AdapterRegistry;
use sqlx::PgPool;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=knobas test",
            "-c",
            "user.email=test@knobas.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

/// A repository the mirror holds, its clone this directory.
async fn repo(pool: &PgPool, clone: &Path) -> String {
    let id = format!("gitea-wr:tidewater/jars-{}", std::process::id());
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'repo',$2)")
        .bind(&id)
        .bind("tidewater/jars")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, web_url)
         values ($1,'gitea-wr','repo',$2,'','{}'::jsonb,null)",
    )
    .bind(&id)
    .bind("tidewater/jars")
    .execute(pool)
    .await
    .unwrap();
    set_override(pool, &id, Some(&clone.to_string_lossy()))
        .await
        .unwrap();
    id
}

/// An archive with no entries: its end-of-central-directory record alone.
const EMPTY_ZIP: [u8; 22] = [
    0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

#[tokio::test]
async fn a_jar_on_the_working_tree_is_opened_as_it_is_on_disk() {
    let db = knobas_db::test_util::scratch_database("working_reads").await;
    let pool = db
        .pool(4)
        .await
        .expect("a pool onto this test's own database");
    let clone = tempfile::tempdir().unwrap();
    git(clone.path(), &["init", "-q", "-b", "main"]);
    std::fs::write(clone.path().join("app.jar"), b"not an archive at HEAD").unwrap();
    git(clone.path(), &["add", "-A"]);
    git(clone.path(), &["commit", "-q", "-m", "first"]);
    let head = git(clone.path(), &["rev-parse", "HEAD"]);
    // Rebuilt, not committed.
    std::fs::write(clone.path().join("app.jar"), EMPTY_ZIP).unwrap();
    let id = repo(&pool, clone.path()).await;
    let secrets: Arc<dyn SecretStore> = Arc::new(MemoryStore::new());
    let registry: Arc<dyn AdapterRegistry> = Arc::new(Registry::default());
    let live = LiveSources::default();
    let open = |commit: String, path: &'static str| {
        let (pool, secrets, registry, live, id) = (
            pool.clone(),
            secrets.clone(),
            registry.clone(),
            live.clone(),
            id.clone(),
        );
        async move {
            knobas_app::archive::open(&pool, &secrets, &registry, &live, &id, path, Some(&commit))
                .await
        }
    };

    let on_disk = open("~working-tree".to_owned(), "app.jar")
        .await
        .expect("the rebuilt archive opens");
    assert_eq!(on_disk.names, "");
    let at_head = open(head, "app.jar").await.unwrap_err();
    assert!(
        at_head.message.contains("not a zip archive"),
        "{}",
        at_head.message
    );
    let gone = open("~working-tree".to_owned(), "nope.jar")
        .await
        .unwrap_err();
    assert_eq!(gone.code, knobas_app::IpcErrorCode::NotFound, "{gone:?}");

    // The index's archive (Local changes' staged half, 2026-11-22): what is staged, not the working tree's file.
    let in_index = open("~index".to_owned(), "app.jar").await.unwrap_err();
    assert!(
        in_index.message.contains("not a zip archive"),
        "{}",
        in_index.message
    );
    git(clone.path(), &["add", "app.jar"]);
    std::fs::write(clone.path().join("app.jar"), b"changed again, not staged").unwrap();
    let staged = open("~index".to_owned(), "app.jar")
        .await
        .expect("the staged archive opens");
    assert_eq!(staged.names, "");
    let none = open("~index".to_owned(), "nope.jar").await.unwrap_err();
    assert_eq!(none.code, knobas_app::IpcErrorCode::NotFound, "{none:?}");
    assert!(none.message.contains("the index"), "{}", none.message);
}
