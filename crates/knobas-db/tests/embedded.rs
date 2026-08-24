use knobas_db::{DbConfig, EmbeddedDb};

#[tokio::test]
async fn starts_answers_and_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    };

    let db = EmbeddedDb::start(cfg.clone()).await.unwrap();
    let one: (i32,) = sqlx::query_as("select 1")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(one.0, 1);
    db.stop().await.unwrap();

    // Second start on the same root_dir must reuse the data dir, not re-initdb.
    let db = EmbeddedDb::start(cfg).await.unwrap();
    let one: (i32,) = sqlx::query_as("select 1")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(one.0, 1);
    db.stop().await.unwrap();
}

#[tokio::test]
async fn recovers_from_a_stale_postmaster_pid() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    };

    let db = EmbeddedDb::start(cfg.clone()).await.unwrap();
    db.stop().await.unwrap();

    // Imitate a hard-killed server: the lock file survives, nothing listens.
    // The recorded port is one we bound and released, so it is free.
    let dead_port = {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.local_addr().unwrap().port()
    };
    let data_dir = dir.path().join("data");
    let pid_file = data_dir.join("postmaster.pid");
    std::fs::write(
        &pid_file,
        format!("999999\n{}\n1700000000\n{dead_port}\n", data_dir.display()),
    )
    .unwrap();

    let db = EmbeddedDb::start(cfg).await.unwrap();
    let one: (i32,) = sqlx::query_as("select 1")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(one.0, 1);
    db.stop().await.unwrap();
}

/// `test_util` is what downstream crates get; exercise it the same way they
/// will -- through the `test-util` feature, from a `tests/` binary.
#[tokio::test]
async fn test_pool_is_shared_and_usable() {
    let first = knobas_db::test_util::test_pool().await;
    let one: (i32,) = sqlx::query_as("select 1").fetch_one(first).await.unwrap();
    assert_eq!(one.0, 1);

    // One embedded instance per test binary: the second call must not start
    // another server.
    let second = knobas_db::test_util::test_pool().await;
    assert!(std::ptr::eq(first, second));
}
