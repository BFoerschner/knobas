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

/// Pins that `EmbeddedDb::start` recovers a data directory whose
/// `postmaster.pid` PostgreSQL itself refuses to clear.
///
/// The recorded PID must belong to a process that is **alive** and is not the
/// postmaster's own ancestry: `CreateLockFile` unlinks the file itself for a
/// dead PID, and exempts its own PID, its parent and its grandparent (the test
/// binary is the grandparent). Either shortcut lets `start()` succeed on the
/// first attempt and the recovery path is never entered -- which is what made
/// the previous version of this test vacuous. A spawned child satisfies both
/// conditions, so PostgreSQL genuinely refuses and `clear_stale_lock` has to
/// do the work.
#[tokio::test]
async fn recovers_from_a_stale_postmaster_pid() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    };

    let db = EmbeddedDb::start(cfg.clone()).await.unwrap();
    db.stop().await.unwrap();

    // Imitate a hard-killed server: the lock file survives and names a live
    // process, but nothing listens on the port it recorded.
    let mut sleeper = std::process::Command::new("sleep")
        .arg("120")
        .spawn()
        .unwrap();
    let live_pid = sleeper.id();
    let dead_port = {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.local_addr().unwrap().port()
    };
    let data_dir = dir.path().join("data");
    std::fs::write(
        data_dir.join("postmaster.pid"),
        format!(
            "{live_pid}\n{}\n1700000000\n{dead_port}\n",
            data_dir.display()
        ),
    )
    .unwrap();

    let result = EmbeddedDb::start(cfg).await;

    // Reap the child before asserting, so a failure does not leak it.
    let _ = sleeper.kill();
    let _ = sleeper.wait();

    let db = result.expect("start should recover from the stale lock");
    let one: (i32,) = sqlx::query_as("select 1")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(one.0, 1);
    db.stop().await.unwrap();
}

/// `existing_url` must connect to a server we do not own -- and `stop()` must
/// leave that server running. Getting this wrong shuts down a user's own
/// production PostgreSQL.
#[tokio::test]
async fn existing_url_connects_without_taking_ownership() {
    let dir = tempfile::tempdir().unwrap();
    let owned = EmbeddedDb::start(DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    })
    .await
    .unwrap();

    let borrowed = EmbeddedDb::start(DbConfig {
        // Ignored on this branch; a path that does not exist proves it.
        root_dir: dir.path().join("never-touched"),
        existing_url: Some(running_url(dir.path())),
    })
    .await
    .unwrap();

    let one: (i32,) = sqlx::query_as("select 1")
        .fetch_one(borrowed.pool())
        .await
        .unwrap();
    assert_eq!(one.0, 1);

    borrowed.stop().await.unwrap();
    assert!(
        !dir.path().join("never-touched").exists(),
        "existing_url must not touch root_dir"
    );

    // The borrowed handle owned nothing, so the server is still up.
    let one: (i32,) = sqlx::query_as("select 1")
        .fetch_one(owned.pool())
        .await
        .expect("stopping a borrowed handle must not stop the server");
    assert_eq!(one.0, 1);

    owned.stop().await.unwrap();
}

/// Reconstruct the URL of a running managed instance from what it wrote to
/// disk: the port from `postmaster.pid` line 4, the password from `.pgpass`.
fn running_url(root_dir: &std::path::Path) -> String {
    let pid_file = std::fs::read_to_string(root_dir.join("data").join("postmaster.pid")).unwrap();
    let port: u16 = pid_file.lines().nth(3).unwrap().trim().parse().unwrap();
    let password = std::fs::read_to_string(root_dir.join(".pgpass")).unwrap();
    format!(
        "postgresql://postgres:{}@127.0.0.1:{port}/knobas",
        password.trim_end()
    )
}

/// `test_util` is what downstream crates get; exercise it the same way they
/// will -- through the `test-util` feature, from a `tests/` binary.
#[tokio::test]
async fn test_pool_is_shared_and_usable() {
    let first = knobas_db::test_util::test_pool().await;
    let one: (i32,) = sqlx::query_as("select 1").fetch_one(&first).await.unwrap();
    assert_eq!(one.0, 1);

    // One embedded instance per test binary: the second call gets a pool of
    // its own (pools are per-runtime) but must not start another server.
    let second = knobas_db::test_util::test_pool().await;
    assert_eq!(
        server_identity(&first).await,
        server_identity(&second).await
    );
}

/// What server a pool is talking to: its loopback port plus the cluster's
/// `system_identifier`, which `initdb` stamps into a fresh data directory.
async fn server_identity(pool: &sqlx::PgPool) -> (i32, String) {
    sqlx::query_as(
        "select inet_server_port(), (select system_identifier::text from pg_control_system())",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

/// A pool from `test_pool` must keep working after an earlier runtime that
/// used the shared database is gone. Sharing one `static` pool across tokio
/// runtimes silently leaks the pool's semaphore permits -- a connection
/// released while its runtime shuts down never runs the task that returns it
/// -- and the next test to ask for a connection dies with `PoolTimedOut`.
#[test]
fn test_pool_survives_an_earlier_runtimes_death() {
    for _ in 0..3 {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let pool = knobas_db::test_util::test_pool().await;
            // A migration plus a query: the shape that starved the pool. The
            // migrator's advisory-lock connection is released last, right as
            // the runtime winds down.
            knobas_db::migrate::run(&pool).await.unwrap();
            let one: (i32,) = sqlx::query_as("select 1").fetch_one(&pool).await.unwrap();
            assert_eq!(one.0, 1);
        });
        drop(rt);
    }
}
