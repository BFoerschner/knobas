// A crate root resolves `mod` beside itself, so the path is spelled out.
#[path = "embedded/elsewhere.rs"]
mod elsewhere;

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

/// A start that fails *after* clearing a stale lock must not shut down a
/// server that appeared in the meantime.
///
/// `PostgreSQL::drop` runs `pg_ctl stop -m fast` whenever `postmaster.pid`
/// merely exists, and it reads the pid out of that file: after this handle
/// cleared the stale lock, any lock file standing there belongs to somebody
/// else. Dropping the failed handle therefore signals *their* postmaster --
/// the database of a sibling that won the race by a hair, killed by the
/// process that lost it.
///
/// The window is two statements wide and lives under the bring-up lock, so
/// nothing outside can be in it; the `test-util` seam is what lets this test
/// stand where the sibling would. Everything else here is real: a real second
/// server, a real forged lock file naming it, and a real failed retry.
#[tokio::test]
async fn a_failed_retry_never_stops_the_server_that_appeared_in_the_window() {
    let ours = tempfile::tempdir().unwrap();
    let cfg = DbConfig {
        root_dir: ours.path().to_path_buf(),
        existing_url: None,
    };
    let data_dir = ours.path().join("data");

    // Our profile, initialised and then made unstartable: an unrecognised
    // parameter is a FATAL during config parsing, before PostgreSQL touches
    // the lock file, so both the first attempt and the retry fail the same way
    // and neither writes a `postmaster.pid` of its own.
    EmbeddedDb::start(cfg.clone())
        .await
        .unwrap()
        .stop()
        .await
        .unwrap();
    let conf = data_dir.join("postgresql.conf");
    let mut text = std::fs::read_to_string(&conf).unwrap();
    text.push_str("\nknobas_cannot_start = on\n");
    std::fs::write(&conf, text).unwrap();

    // The sibling: a real server on a profile of its own, which this run has
    // no business touching.
    let theirs = tempfile::tempdir().unwrap();
    let sibling = EmbeddedDb::start(DbConfig {
        root_dir: theirs.path().to_path_buf(),
        existing_url: None,
    })
    .await
    .unwrap();
    let sibling_pid: u32 = {
        let pid_file =
            std::fs::read_to_string(theirs.path().join("data").join("postmaster.pid")).unwrap();
        pid_file.lines().next().unwrap().trim().parse().unwrap()
    };
    let sibling_port = running_port(theirs.path());

    // A stale lock of our own: dead process, silent port, so `inspect_lock`
    // clears it and the start is retried.
    let mut reaped = std::process::Command::new("true").spawn().unwrap();
    let dead_pid = reaped.id();
    reaped.wait().unwrap();
    let silent_port = {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.local_addr().unwrap().port()
    };
    std::fs::write(
        data_dir.join("postmaster.pid"),
        format!(
            "{dead_pid}\n{}\n1700000000\n{silent_port}\n",
            data_dir.display()
        ),
    )
    .unwrap();

    // In the window: exactly what a sibling's own bring-up leaves behind --
    // a fresh lock file, naming its live postmaster, in the directory we just
    // cleared.
    let forged = data_dir.clone();
    knobas_db::embedded::seam::after_clearing_a_stale_lock(move || {
        std::fs::write(
            forged.join("postmaster.pid"),
            format!(
                "{sibling_pid}\n{}\n1700000000\n{sibling_port}\n",
                forged.display()
            ),
        )
        .unwrap();
    });

    let refused = EmbeddedDb::start(cfg).await;
    knobas_db::embedded::seam::forget_hooks();

    assert!(
        refused.is_err(),
        "the retry cannot succeed against a config PostgreSQL refuses"
    );

    // The whole point: the sibling is still serving.
    let one: (i32,) = sqlx::query_as("select 1")
        .fetch_one(sibling.pool())
        .await
        .expect("a failed retry must not stop a server this process never started");
    assert_eq!(one.0, 1);

    sibling.stop().await.unwrap();
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
///
/// It also pins the ruling that a directory mismatch clears the lock
/// regardless of whether the recorded pid is alive -- the pid written below is
/// a live process on purpose, and the start must still recover. A postmaster
/// serves one data directory for its whole life, so a server answering on our
/// recorded port for *someone else's* directory proves no postmaster of ours
/// is there; the argument is spelled out at that branch in `embedded.rs`.
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

/// The port `postmaster.pid` records for a data directory.
fn port_of(root_dir: &std::path::Path) -> u16 {
    running_port(root_dir)
}

fn port_answers(port: u16) -> bool {
    std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        std::time::Duration::from_millis(500),
    )
    .is_ok()
}

/// The M0 carry-over: an adopted server was owned by nobody, so it outlived
/// every later run too. A launch that finds an orphan must be able to take
/// responsibility for it, or `just dev` leaves a postmaster per crash behind
/// for ever.
#[tokio::test]
async fn a_launch_that_adopts_an_orphaned_server_takes_ownership_and_can_stop_it() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    };

    let first = EmbeddedDb::start(cfg.clone()).await.unwrap();
    assert!(first.owns_server(), "the process that started it owns it");
    let port = port_of(dir.path());

    // Exactly what a `kill -9` leaves behind: the handle is gone, its locks are
    // released, the server is still up.
    first.abandon();
    assert!(port_answers(port), "the orphan is still serving");

    let second = EmbeddedDb::start(cfg).await.unwrap();
    assert!(
        second.owns_server(),
        "adopting an ownerless server takes ownership of it"
    );
    let one: (i32,) = sqlx::query_as("select 1")
        .fetch_one(second.pool())
        .await
        .unwrap();
    assert_eq!(one.0, 1);

    second.stop().await.unwrap();
    assert!(!port_answers(port), "and a clean quit actually stops it");
}

/// The other half of the same rule: a server a *live* instance owns must never
/// be stopped by an adopter. Two knobas windows on one profile share the
/// database; the one that started it is the one that stops it.
#[tokio::test]
async fn an_adopter_does_not_take_ownership_from_a_live_owner() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    };

    let owner = EmbeddedDb::start(cfg.clone()).await.unwrap();
    let port = port_of(dir.path());
    let guest = EmbeddedDb::start(cfg).await.unwrap();

    assert!(owner.owns_server());
    assert!(
        !guest.owns_server(),
        "the owner is still alive; the guest only borrows"
    );

    guest.stop().await.unwrap();
    assert!(
        port_answers(port),
        "a guest's quit must not take the owner's database down"
    );

    owner.stop().await.unwrap();
    assert!(!port_answers(port));
}

/// A server knobas does not manage at all is never owned, whatever the locks
/// in `root_dir` say.
#[tokio::test]
async fn an_externally_managed_server_is_never_owned() {
    let dir = tempfile::tempdir().unwrap();
    let owned = EmbeddedDb::start(DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    })
    .await
    .unwrap();

    let borrowed = EmbeddedDb::start(DbConfig {
        root_dir: dir.path().join("never-touched"),
        existing_url: Some(running_url(dir.path())),
    })
    .await
    .unwrap();
    assert!(
        !borrowed.owns_server(),
        "KNOBAS_DB_URL points at somebody else's server"
    );
    borrowed.stop().await.unwrap();

    owned.stop().await.unwrap();
}

/// Carry-over: "quitting mid-sync stalls on `pool.close()` until the run's
/// transaction drains". The scheduler cancels its runs first (stream F, Task
/// 7), but the database handle must not be able to hang the exit on its own
/// either -- a stray query from anywhere else would do it.
#[tokio::test]
async fn stopping_does_not_wait_out_a_long_running_query() {
    let dir = tempfile::tempdir().unwrap();
    let db = EmbeddedDb::start(DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    })
    .await
    .unwrap();

    let pool = db.pool().clone();
    // Far longer than the close timeout, and holding a pooled connection.
    let hog = tokio::spawn(async move {
        let _ = sqlx::query("select pg_sleep(30)").execute(&pool).await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let started = std::time::Instant::now();
    db.stop().await.unwrap();
    let took = started.elapsed();
    assert!(
        took < std::time::Duration::from_secs(15),
        "stop took {took:?}; it must be bounded"
    );

    hog.abort();
}

/// The scheduler needs connections of its own: a network-bound run pins one for
/// the length of a remote call, and the UI's five must stay free (carry-over).
#[tokio::test]
async fn a_second_pool_can_be_opened_on_the_same_server() {
    let dir = tempfile::tempdir().unwrap();
    let db = EmbeddedDb::start(DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    })
    .await
    .unwrap();

    let second = db.pool_for(4).await.unwrap();
    knobas_db::migrate::run(db.pool()).await.unwrap();

    sqlx::query(
        "insert into knobas.entity (id, kind, title) values ('note:pool-test', 'note', 'x')",
    )
    .execute(&second)
    .await
    .unwrap();
    let (n,): (i64,) =
        sqlx::query_as("select count(*) from knobas.entity where id = 'note:pool-test'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(n, 1, "both pools see one database");

    // ...and it really is a *second* pool: closing it must leave the
    // application's untouched. A `pool_for` that handed back a clone of
    // `db.pool()` would satisfy everything above and fail here, which is the
    // whole point of the carry-over.
    second.close().await;
    assert!(second.is_closed());
    assert!(
        !db.pool().is_closed(),
        "the application pool is not the scheduler's"
    );
    let (still,): (i64,) = sqlx::query_as("select count(*) from knobas.entity")
        .fetch_one(db.pool())
        .await
        .expect("the application pool still answers after the scheduler's closed");
    assert!(still >= 1);

    db.stop().await.unwrap();
}

/// A connection that belongs to no pool at all -- what §10.6(c) makes the sync
/// run hold its advisory lock on.
#[tokio::test]
async fn a_connector_hands_out_connections_outside_every_pool() {
    use sqlx::Connection;

    let dir = tempfile::tempdir().unwrap();
    let db = EmbeddedDb::start(DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    })
    .await
    .unwrap();
    let connector = db.connector();

    // The pool is exhausted for the whole of this test...
    let pool = db.pool_for(1).await.unwrap();
    let held = pool.acquire().await.unwrap();

    // ...and the connector still answers, because it draws on nothing.
    let mut conn = connector.connect().await.unwrap();
    let (one,): (i32,) = sqlx::query_as("select 1")
        .fetch_one(&mut conn)
        .await
        .unwrap();
    assert_eq!(one, 1);
    conn.close().await.unwrap();

    drop(held);
    pool.close().await;
    db.stop().await.unwrap();
}

/// A `Connector` carries the superuser password, so `{:?}` must not.
#[tokio::test]
async fn a_connector_does_not_print_its_password() {
    let dir = tempfile::tempdir().unwrap();
    let db = EmbeddedDb::start(DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    })
    .await
    .unwrap();

    let password = std::fs::read_to_string(dir.path().join(".pgpass")).unwrap();
    let password = password.trim_end();
    assert!(!password.is_empty(), "initdb recorded a password");

    let printed = format!("{:?}", db.connector());
    assert!(
        !printed.contains(password),
        "the connector printed its password: {printed}"
    );
    assert!(printed.contains("<redacted>"), "{printed}");

    db.stop().await.unwrap();
}

/// Carry-over: a `KNOBAS_DB_URL` pointing at a dead port left the boot screen
/// on "Starting the local database" for a full 30 s -- `PgPoolOptions::connect`
/// retrying a refused handshake until the pool's acquire timeout expired.
#[tokio::test]
async fn an_existing_url_that_answers_nothing_fails_fast_rather_than_after_thirty_seconds() {
    // A port nothing listens on: bind it, read it back, drop the listener.
    let port = {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.local_addr().unwrap().port()
    };
    let dir = tempfile::tempdir().unwrap();

    let started = std::time::Instant::now();
    let outcome = EmbeddedDb::start(DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: Some(format!("postgresql://postgres:x@127.0.0.1:{port}/knobas")),
    })
    .await;
    let took = started.elapsed();

    assert!(outcome.is_err(), "nothing is listening on {port}");
    assert!(
        took < std::time::Duration::from_secs(15),
        "the failure took {took:?}; sqlx' 30 s pool-acquire wait is the bug this bounds"
    );
}

/// The test-server entry point `just test` runs: one server for a whole gate,
/// alive exactly as long as its stdin is open.
///
/// Spawned the way the recipe spawns it -- a pipe on each end -- so what is
/// observed is the contract the recipe relies on: the first line on stdout is
/// a usable URL for the maintenance database, and closing the pipe is what
/// stops the server. Prior art for the stop half is
/// `a_launch_that_adopts_an_orphaned_server_takes_ownership_and_can_stop_it`.
#[tokio::test]
async fn the_test_server_serves_until_its_stdin_closes_and_then_stops() {
    use std::io::BufRead;
    use std::process::{Command, Stdio};

    use sqlx::Connection;

    let mut child = Command::new(env!("CARGO_BIN_EXE_knobas-test-server"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn the test server");
    let stdout = child.stdout.take().unwrap();
    let mut url = String::new();
    std::io::BufReader::new(stdout)
        .read_line(&mut url)
        .expect("the test server's first line");
    let url = url.trim_end().to_owned();
    assert!(
        url.starts_with("postgresql://"),
        "the first line on stdout is the URL, not {url:?}"
    );

    let mut conn = sqlx::PgConnection::connect(&url)
        .await
        .expect("connect to the URL the server printed");
    let (port, database): (i32, String) =
        sqlx::query_as("select inet_server_port(), current_database()")
            .fetch_one(&mut conn)
            .await
            .unwrap();
    assert_eq!(
        database, "postgres",
        "the URL names the maintenance database"
    );
    let port = u16::try_from(port).unwrap();
    conn.close().await.unwrap();
    assert!(port_answers(port), "the server is up while stdin is open");

    // The recipe's shutdown: close the pipe, nothing else.
    drop(child.stdin.take());
    let status = child.wait().expect("wait for the test server");
    assert!(status.success(), "the test server exited with {status}");
    assert!(
        !port_answers(port),
        "closing stdin must stop the server, not just the process"
    );
}

/// The server's scratch root goes with it. It is removed by a child the
/// server leaves behind rather than before the server exits (#421: unlinking
/// a gate's 355 databases took 10 s of every `just test`, after the last test
/// had reported), so "gone" here means gone shortly after the exit status --
/// directory and lock both, so a clean run leaves the next run's reaper
/// nothing.
///
/// The root's name is the connector's convention, `$TMPDIR/knobas-test-<pid>`
/// with `knobas-test-<pid>.lock` beside it (`test_util`'s module docs; the
/// `test` recipe's comment names the prefix). The test spells it out rather
/// than importing it because the path is the contract a shell could check.
#[test]
fn the_test_server_takes_its_root_with_it() {
    use std::io::BufRead;
    use std::process::{Command, Stdio};

    let mut child = Command::new(env!("CARGO_BIN_EXE_knobas-test-server"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn the test server");
    let stdout = child.stdout.take().unwrap();
    let mut url = String::new();
    std::io::BufReader::new(stdout)
        .read_line(&mut url)
        .expect("the test server's first line");
    assert!(url.starts_with("postgresql://"), "not a URL: {url:?}");

    let root = std::env::temp_dir().join(format!("knobas-test-{}", child.id()));
    let lock = root.with_file_name(format!("knobas-test-{}.lock", child.id()));
    assert!(
        root.join("data").is_dir() && lock.is_file(),
        "while it serves, the server's root and lock are {} and {}",
        root.display(),
        lock.display()
    );

    drop(child.stdin.take());
    let status = child.wait().expect("wait for the test server");
    assert!(status.success(), "the test server exited with {status}");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while (root.exists() || lock.exists()) && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(
        !root.exists(),
        "the server's root is still there after it exited: {}",
        root.display()
    );
    assert!(
        !lock.exists(),
        "the server's lock is still there after it exited: {}",
        lock.display()
    );
}

/// Where this binary's shared connector landed, in a form the test below can
/// read out of a child process: `landed=<port> <system_identifier> <database>
/// <migrated>`.
///
/// It stands on its own too: whichever server the connector reached -- its
/// own or the one `KNOBAS_TEST_DB_URL` names -- it is never on the maintenance
/// database, which is where a URL naming `postgres` would leave a connector
/// that forgot to move off it.
#[tokio::test]
async fn the_shared_connector_never_lands_on_the_maintenance_database() {
    use sqlx::Connection;

    let mut conn = knobas_db::test_util::test_connector()
        .await
        .connect()
        .await
        .unwrap();
    let (port, identifier, database, migrated): (i32, String, String, bool) = sqlx::query_as(
        "select inet_server_port(),
                (select system_identifier::text from pg_control_system()),
                current_database(),
                to_regclass('knobas.entity') is not null",
    )
    .fetch_one(&mut conn)
    .await
    .unwrap();
    conn.close().await.unwrap();

    println!("landed={port} {identifier} {database} {migrated}");
    assert_ne!(database, "postgres");
}

/// Where a child process's shared connector ended up, as the test above
/// reports it.
struct Landing {
    port: i32,
    system_identifier: String,
    database: String,
    migrated: bool,
}

/// What a child process of this binary reports through the test above, with
/// `KNOBAS_TEST_DB_URL` set to `url`.
///
/// A child rather than `set_var` in this process: the shared server is decided
/// once per process, and the other tests here have already decided it.
fn landing_of_a_child_with(url: &str) -> Landing {
    let stdout = stdout_of_a_child(
        "the_shared_connector_never_lands_on_the_maintenance_database",
        Some(url),
    );
    let landed = stdout
        .lines()
        .find_map(|line| line.strip_prefix("landed="))
        .unwrap_or_else(|| panic!("no landed= line in:\n{stdout}"));
    let mut fields = landed.split(' ');
    Landing {
        port: fields.next().unwrap().parse().unwrap(),
        system_identifier: fields.next().unwrap().to_owned(),
        database: fields.next().unwrap().to_owned(),
        migrated: fields.next().unwrap().parse().unwrap(),
    }
}

/// With `KNOBAS_TEST_DB_URL` set, the shared connector reaches *that* server
/// -- on a migrated database of the binary's own, not the maintenance database
/// the URL names -- and two binaries on the same server get two databases.
///
/// The server is this test's own, so a child that ignored the variable and
/// started a server of its own (the mutant: delete the environment read) shows
/// up as a different `system_identifier` on a different port. Two children are
/// two processes, which is what "two test binaries" comes down to for the
/// database name: it is built from the pid and a nonce.
#[tokio::test]
async fn the_shared_connector_follows_knobas_test_db_url_onto_a_database_of_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let ours = EmbeddedDb::start(DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    })
    .await
    .unwrap();
    let url = maintenance_url(dir.path());

    let first = landing_of_a_child_with(&url);
    assert_eq!(
        (first.port, first.system_identifier.clone()),
        server_identity(ours.pool()).await,
        "the child's shared connector is not on this test's server"
    );
    assert_ne!(
        first.database, "postgres",
        "the maintenance database is not a test's"
    );
    assert_ne!(first.database, "knobas", "nor is the app's");
    assert!(first.migrated, "the binary's database must arrive migrated");

    let second = landing_of_a_child_with(&url);
    assert_ne!(
        first.database, second.database,
        "two binaries must not share a database"
    );

    ours.stop().await.unwrap();
}

/// This binary re-run as a child on the one test named `test`, with
/// `KNOBAS_TEST_DB_URL` set to `url` or, for `None`, removed -- its stdout,
/// once it has passed.
///
/// A child that ran no test at all passes too: libtest prints `running 0
/// tests` for an `--exact` name that matches nothing, and exits 0. Every
/// reader of the output would then fail on a missing line at best, so the
/// count is checked here, where the cause is known.
fn stdout_of_a_child(test: &str, url: Option<&str>) -> String {
    let mut child = std::process::Command::new(std::env::current_exe().unwrap());
    child.args(["--exact", test, "--nocapture"]);
    match url {
        Some(url) => child.env("KNOBAS_TEST_DB_URL", url),
        None => child.env_remove("KNOBAS_TEST_DB_URL"),
    };
    let output = child.output().expect("re-run this binary as a child");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "the child failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let ran = stdout
        .lines()
        .find_map(|line| line.strip_prefix("running "))
        .and_then(|rest| rest.split(' ').next())
        .and_then(|count| count.parse::<u32>().ok())
        .unwrap_or_else(|| panic!("no `running N tests` line in:\n{stdout}"));
    assert_ne!(ran, 0, "the child ran no test named {test}:\n{stdout}");
    stdout
}

/// The database `pool` is on.
async fn database_of(pool: &sqlx::PgPool) -> String {
    let (database,): (String,) = sqlx::query_as("select current_database()")
        .fetch_one(pool)
        .await
        .unwrap();
    database
}

/// The database `connector` reaches.
async fn database_of_connector(connector: &knobas_db::embedded::Connector) -> String {
    use sqlx::Connection;

    let mut conn = connector.connect().await.unwrap();
    let (database,): (String,) = sqlx::query_as("select current_database()")
        .fetch_one(&mut conn)
        .await
        .unwrap();
    conn.close().await.unwrap();
    database
}

/// The shared database belongs to the calling source file: every call from
/// one file -- `test_pool` or `test_connector` -- meets the same database, and
/// a call from `elsewhere.rs`, compiled into this same binary, meets another.
/// A test file merged into a crate's one test binary (ADR-0017) is then as
/// isolated as it was as a binary of its own.
///
/// Prints `files=<this file's> <elsewhere's>` for the parents below, which
/// re-run it on each of the two paths to a server.
#[tokio::test]
async fn the_shared_database_follows_the_calling_source_file() {
    let here = database_of(&knobas_db::test_util::test_pool().await).await;
    let here_again = database_of(&knobas_db::test_util::test_pool().await).await;
    let here_connector = database_of_connector(&knobas_db::test_util::test_connector().await).await;
    let there = elsewhere::database_of_test_pool().await;
    let there_connector = elsewhere::database_of_test_connector().await;

    println!("files={here} {there}");
    assert_eq!(here, here_again, "two calls from one file, two databases");
    assert_eq!(
        here, here_connector,
        "test_connector and test_pool disagree within one file"
    );
    assert_eq!(
        there, there_connector,
        "test_connector and test_pool disagree within elsewhere.rs"
    );
    assert_ne!(here, there, "two source files share one database");
}

/// The two databases a child's `files=` line names.
fn files_of(stdout: &str) -> (String, String) {
    let files = stdout
        .lines()
        .find_map(|line| line.strip_prefix("files="))
        .unwrap_or_else(|| panic!("no files= line in:\n{stdout}"));
    let (here, there) = files
        .split_once(' ')
        .unwrap_or_else(|| panic!("a files= line names two databases: {files}"));
    (here.to_owned(), there.to_owned())
}

/// On the gate path, per file: the child, pointed at this test's server,
/// passes the per-file test -- and both databases it names are on this
/// server, migrated, so the child really was on the gate path rather than a
/// server of its own.
#[tokio::test]
async fn on_the_gate_server_each_source_file_gets_a_database_of_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let ours = EmbeddedDb::start(DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    })
    .await
    .unwrap();

    let stdout = stdout_of_a_child(
        "the_shared_database_follows_the_calling_source_file",
        Some(&maintenance_url(dir.path())),
    );
    let (here, there) = files_of(&stdout);

    for database in [&here, &there] {
        let pool = sqlx::PgPool::connect(&database_url(dir.path(), database))
            .await
            .unwrap_or_else(|error| panic!("{database} is not on this test's server: {error}"));
        let (migrated,): (bool,) =
            sqlx::query_as("select to_regclass('knobas.entity') is not null")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(migrated, "{database} arrived unmigrated");
        pool.close().await;
    }

    ours.stop().await.unwrap();
}

/// On the zero-config path, per file: the child, with `KNOBAS_TEST_DB_URL`
/// removed, starts a server of its own and passes the per-file test there.
#[test]
fn on_a_server_of_its_own_each_source_file_gets_a_database_of_its_own() {
    let stdout = stdout_of_a_child("the_shared_database_follows_the_calling_source_file", None);
    let (here, there) = files_of(&stdout);
    assert_ne!(here, there);
}
