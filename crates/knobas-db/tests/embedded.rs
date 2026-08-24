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

/// A `postmaster.pid` naming a **live** process whose port is silent is
/// reported, not cleared -- and the same directory starts normally once that
/// process is gone.
///
/// The two halves are one story. A failed TCP handshake alone says nothing
/// about ownership: a postmaster still starting up, or wedged before it opened
/// its socket, fails it while holding the data directory, and clearing the lock
/// there puts a second server on the same files. What makes the lock clearable
/// is the *process* being gone, which is the second half.
///
/// The recorded PID must belong to a process that is alive and is not the
/// postmaster's own ancestry: `CreateLockFile` unlinks the file itself for a
/// dead PID, and exempts its own PID, its parent and its grandparent (the test
/// binary is the grandparent). Either shortcut lets `start()` succeed on the
/// first attempt and none of this code is reached at all. A spawned child
/// satisfies both conditions, so PostgreSQL genuinely refuses and
/// `inspect_lock` has to do the work.
#[tokio::test]
async fn a_lock_naming_a_live_process_is_reported_until_that_process_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    };

    let db = EmbeddedDb::start(cfg.clone()).await.unwrap();
    db.stop().await.unwrap();

    // The lock file survives, names a live process, and records a port nothing
    // listens on.
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
    let pid_file = data_dir.join("postmaster.pid");
    std::fs::write(
        &pid_file,
        format!(
            "{live_pid}\n{}\n1700000000\n{dead_port}\n",
            data_dir.display()
        ),
    )
    .unwrap();

    let refused = EmbeddedDb::start(cfg.clone()).await;

    // Asserted before the child is reaped, because both facts are about the
    // window in which it was alive.
    let error = match refused {
        Err(error) => error,
        Ok(_) => {
            let _ = sleeper.kill();
            let _ = sleeper.wait();
            panic!("a lock whose process is still running must not be taken over");
        }
    };
    assert!(
        matches!(error, knobas_db::DbError::AlreadyRunning { .. }),
        "{error:?}"
    );
    assert!(
        pid_file.exists(),
        "the lock must survive: clearing it is what lets a second server in"
    );

    let _ = sleeper.kill();
    let _ = sleeper.wait();

    // The process is gone now, so the same lock is stale and the start goes
    // through.
    let db = EmbeddedDb::start(cfg)
        .await
        .expect("a lock whose process is gone must not block the start");
    let one: (i32,) = sqlx::query_as("select 1")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(one.0, 1);
    db.stop().await.unwrap();
}

/// Two first launches on one profile at the same time: exactly one `initdb`
/// happens, and both callers end up on one working server.
///
/// Nothing on disk lets the second process see the first coming -- `setup()`
/// decides whether to initialise by looking at an empty directory, and there is
/// no `postmaster.pid` to adopt yet -- so without the bring-up lock the two
/// interleave inside one data directory. This is the shape of a double-click on
/// the app icon, or `just dev` started beside a packaged build.
///
/// Both halves of the loser's story are covered, and neither depends on how
/// warm the machine is: `start_managed` yields before `setup()`, so the two
/// launches really are both in flight here whether or not the binaries had to
/// be downloaded, and the credential half -- settings built before the winner
/// wrote `.pgpass` -- is pinned deterministically by
/// `a_launch_that_started_cold_authenticates_with_the_recorded_password`.
#[tokio::test]
async fn two_concurrent_first_launches_serialise_instead_of_racing() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    };

    let (first, second) = tokio::join!(
        EmbeddedDb::start(cfg.clone()),
        EmbeddedDb::start(cfg.clone())
    );
    let first = first.expect("the first launch must bring the database up");
    let second = second.expect("the second launch must join it, not corrupt it");

    // One server, not two: a table created through one handle is visible
    // through the other.
    sqlx::query("create table if not exists race_probe (n int)")
        .execute(second.pool())
        .await
        .unwrap();
    let (probes,): (i64,) =
        sqlx::query_as("select count(*) from information_schema.tables where table_name = $1")
            .bind("race_probe")
            .fetch_one(first.pool())
            .await
            .unwrap();
    assert_eq!(probes, 1, "the two launches ended up on different servers");

    // Whichever adopted owns nothing, so stopping it leaves the server up; the
    // owner's stop is what actually shuts it down.
    second.stop().await.unwrap();
    first.stop().await.unwrap();
}

/// A `postmaster.pid` whose port is answered by a server serving *someone
/// else's* data directory is a stale lock, not a conflict -- and the identity
/// check that establishes this is what keeps adoption safe.
///
/// The scenario is a knobas profile copied elsewhere (data directory *and*
/// `.pgpass`, so the superuser password is shared) and both copies used: the
/// original's lock file records a port its own server no longer holds, and the
/// copy's server answers there. Without the `show data_directory` check, this
/// run would silently attach to the copy's database -- same password, same
/// database name, entirely different content. With it, the mismatch proves
/// nothing holds *our* directory, so the lock is cleared and the start proceeds
/// normally.
///
/// The assertion on `show data_directory` is what pins the check: delete the
/// `same_dir` guard in `adopt` and this test attaches to the stranger and fails
/// here.
#[tokio::test]
async fn a_lock_naming_a_strangers_port_is_cleared_instead_of_reported_as_a_conflict() {
    let ours = tempfile::tempdir().unwrap();
    let theirs = tempfile::tempdir().unwrap();
    let cfg = DbConfig {
        root_dir: ours.path().to_path_buf(),
        existing_url: None,
    };

    // Our data directory, initialised and then left with nothing running on it.
    EmbeddedDb::start(cfg.clone())
        .await
        .unwrap()
        .stop()
        .await
        .unwrap();

    // The copy: the same superuser password, its own data directory, live.
    std::fs::copy(ours.path().join(".pgpass"), theirs.path().join(".pgpass")).unwrap();
    let stranger = EmbeddedDb::start(DbConfig {
        root_dir: theirs.path().to_path_buf(),
        existing_url: None,
    })
    .await
    .unwrap();
    let stranger_port = running_port(theirs.path());

    // A lock file in *our* data directory naming a live process and the
    // stranger's port -- what a hard kill plus a recycled ephemeral port
    // leaves behind. The PID has to be alive and unrelated to the postmaster's
    // ancestry, or PostgreSQL removes the file itself and never refuses.
    let mut sleeper = std::process::Command::new("sleep")
        .arg("120")
        .spawn()
        .unwrap();
    let data_dir = ours.path().join("data");
    std::fs::write(
        data_dir.join("postmaster.pid"),
        format!(
            "{}\n{}\n1700000000\n{stranger_port}\n",
            sleeper.id(),
            data_dir.display()
        ),
    )
    .unwrap();

    let result = EmbeddedDb::start(cfg).await;

    // Reap the child before asserting, so a failure does not leak it.
    let _ = sleeper.kill();
    let _ = sleeper.wait();

    let db = result.expect("a lock naming a stranger's port is stale, not a conflict");
    let (serving,): (String,) = sqlx::query_as("show data_directory")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(
        std::fs::canonicalize(&serving).unwrap(),
        std::fs::canonicalize(&data_dir).unwrap(),
        "the start attached to the server the stale lock pointed at, not to its own"
    );

    // And the stranger was neither stopped nor otherwise disturbed.
    let one: (i32,) = sqlx::query_as("select 1")
        .fetch_one(stranger.pool())
        .await
        .expect("clearing our own stale lock must not touch someone else's server");
    assert_eq!(one.0, 1);

    db.stop().await.unwrap();
    stranger.stop().await.unwrap();
}

/// A second `start()` on a data directory a **live** server already holds must
/// join that server, and must leave it standing.
///
/// This is the shape of two real situations: a knobas killed by a signal, whose
/// PostgreSQL outlives it and is still there at the next launch, and a second
/// instance opened on the same profile. Both used to end the same way --
/// `postgresql_embedded`'s `Drop` runs `pg_ctl stop -m fast` whenever
/// `postmaster.pid` merely exists, so the *failed* second handle killed the
/// first one's database on its way out. The assertion that the first pool still
/// answers afterwards is what pins that down.
#[tokio::test]
async fn a_second_start_adopts_the_running_server_instead_of_killing_it() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    };

    let first = EmbeddedDb::start(cfg.clone()).await.unwrap();

    // Same root_dir, server already up: this is the path that used to fail.
    let second = EmbeddedDb::start(cfg)
        .await
        .expect("second start should adopt");
    let one: (i32,) = sqlx::query_as("select 1")
        .fetch_one(second.pool())
        .await
        .unwrap();
    assert_eq!(one.0, 1);

    // Both handles talk to the same server, so a write through one is visible
    // through the other -- adoption, not a second server on a second port.
    sqlx::query("create table if not exists adoption_probe (n int)")
        .execute(second.pool())
        .await
        .unwrap();
    let (probes,): (i64,) =
        sqlx::query_as("select count(*) from information_schema.tables where table_name = $1")
            .bind("adoption_probe")
            .fetch_one(first.pool())
            .await
            .unwrap();
    assert_eq!(probes, 1, "the two handles are on different servers");

    // Dropping the adopted handle must not take the server with it.
    second.stop().await.unwrap();
    let one: (i32,) = sqlx::query_as("select 1")
        .fetch_one(first.pool())
        .await
        .expect("adopting a running server must never stop it");
    assert_eq!(one.0, 1);

    first.stop().await.unwrap();
}

/// Adoption must finish the setup the managed path performs, not assume it
/// already happened: a server serving our data directory but with no `knobas`
/// database in it gets one.
///
/// That is the shape a run killed between `initdb` and `create database` leaves
/// behind. Without this step the adopted server is reported as unreachable --
/// the connection to a database that does not exist is what fails -- and knobas
/// never starts again on that profile.
#[tokio::test]
async fn adoption_creates_the_database_when_the_running_server_has_none() {
    use sqlx::Connection;

    let dir = tempfile::tempdir().unwrap();
    let cfg = DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    };

    let owner = EmbeddedDb::start(cfg.clone()).await.unwrap();

    // Take the database away from under the running server. `with (force)`
    // terminates the pool's sessions, which is what makes the drop possible at
    // all while `owner` is connected.
    let mut admin = sqlx::PgConnection::connect(&maintenance_url(dir.path()))
        .await
        .unwrap();
    sqlx::query("drop database knobas with (force)")
        .execute(&mut admin)
        .await
        .unwrap();
    admin.close().await.unwrap();

    let adopted = EmbeddedDb::start(cfg)
        .await
        .expect("adopting a server with no knobas database must create it");
    let (current,): (String,) = sqlx::query_as("select current_database()")
        .fetch_one(adopted.pool())
        .await
        .unwrap();
    assert_eq!(current, "knobas");

    // The adopted handle owns nothing; the original still owns the server.
    adopted.stop().await.unwrap();
    owner.stop().await.unwrap();
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

/// The port a running managed instance recorded in `postmaster.pid` line 4.
fn running_port(root_dir: &std::path::Path) -> u16 {
    let pid_file = std::fs::read_to_string(root_dir.join("data").join("postmaster.pid")).unwrap();
    pid_file.lines().nth(3).unwrap().trim().parse().unwrap()
}

/// Reconstruct the URL of a running managed instance from what it wrote to
/// disk: the port from `postmaster.pid` line 4, the password from `.pgpass`.
fn running_url(root_dir: &std::path::Path) -> String {
    database_url(root_dir, "knobas")
}

/// The same, against the `postgres` maintenance database -- the one that stays
/// connectable while `knobas` is being dropped or created.
fn maintenance_url(root_dir: &std::path::Path) -> String {
    database_url(root_dir, "postgres")
}

fn database_url(root_dir: &std::path::Path, database: &str) -> String {
    let port = running_port(root_dir);
    let password = std::fs::read_to_string(root_dir.join(".pgpass")).unwrap();
    format!(
        "postgresql://postgres:{}@127.0.0.1:{port}/{database}",
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
