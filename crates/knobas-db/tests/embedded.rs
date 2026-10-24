// A crate root resolves `mod` beside itself, so the path is spelled out.
#[path = "embedded/elsewhere.rs"]
mod elsewhere;
#[path = "embedded/locale.rs"]
mod locale;
#[path = "embedded/slow_answer.rs"]
mod slow_answer;

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

/// #1637: a data directory with `postgresql.conf` and no `PG_VERSION` is a
/// half-written one, and the launch initialises it again instead of failing.
///
/// `postgresql_embedded` reads `postgresql.conf` alone as "initialised", so it
/// skipped `initdb` here and `pg_ctl start` failed with *is not a database
/// cluster directory*, on this launch and every one after it. `initdb` writes
/// `PG_VERSION` before `postgresql.conf` and its own cleanup after a failure
/// removes `PG_VERSION` first, so this shape is an `initdb` cleanup that was
/// itself cut short (the evidence is under #1637 in
/// `docs/decisions/2026-10-unattended-rulings.md`). The fixture is the
/// directory such a cut left: its subdirectories, emptied, and the
/// configuration files.
#[tokio::test]
async fn a_data_directory_an_interrupted_initdb_left_is_initialised_again() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    for sub in ["base/1", "global", "pg_wal/archive_status", "pg_xact"] {
        std::fs::create_dir_all(data.join(sub)).unwrap();
    }
    std::fs::write(data.join("postgresql.conf"), "# written by initdb\n").unwrap();
    std::fs::write(data.join("pg_hba.conf"), "# written by initdb\n").unwrap();
    let cfg = DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    };

    let db = EmbeddedDb::start(cfg.clone())
        .await
        .expect("a half-written data directory is initialised again");
    let one: (i32,) = sqlx::query_as("select 1")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(one.0, 1);
    assert!(data.join("PG_VERSION").is_file(), "initdb ran again");
    db.stop().await.unwrap();

    let again = EmbeddedDb::start(cfg)
        .await
        .expect("and the next launch starts it as any other");
    again.stop().await.unwrap();
}

/// #1985: a data directory with neither `PG_VERSION` nor `postgresql.conf`,
/// but not empty, is what an `initdb` killed before it wrote `PG_VERSION`
/// leaves, or one killed during its own cleanup once that removed
/// `postgresql.conf`. The launch initialises it again.
///
/// Before, it read as "not initialised", `initdb` refused a directory that
/// was not empty, and every launch failed. `initdb` makes its subdirectories,
/// `base/1` among them, before `PG_VERSION`. Its cleanup removes the top
/// level's files in the order the directory lists them, which on APFS is
/// `PG_VERSION`, then `postgresql.conf`, and the directories after every
/// file (PostgreSQL's `rmtree` defers them, #2007): 40 of 40 runs that
/// sent `SIGTERM` and then `SIGKILL` once `postgresql.conf` was gone left the
/// second and third fixture (evidence under #1985 in
/// `docs/decisions/2026-10-unattended-rulings.md`).
#[tokio::test]
async fn a_data_directory_an_initdb_killed_before_pg_version_left_is_initialised_again() {
    for subdirs in [
        &["base", "global"][..],
        &["base/1", "global", "pg_tblspc", "pg_xact"],
        &["base/1", "base/4", "base/5", "global", "pg_tblspc"],
    ] {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        for sub in subdirs {
            std::fs::create_dir_all(data.join(sub)).unwrap();
        }
        let cfg = DbConfig {
            root_dir: dir.path().to_path_buf(),
            existing_url: None,
        };

        let db = EmbeddedDb::start(cfg.clone())
            .await
            .unwrap_or_else(|error| panic!("{subdirs:?}: the launch failed: {error}"));
        let one: (i32,) = sqlx::query_as("select 1")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(one.0, 1, "{subdirs:?}");
        assert!(data.join("PG_VERSION").is_file(), "{subdirs:?}: initdb ran");
        db.stop().await.unwrap();

        let again = EmbeddedDb::start(cfg)
            .await
            .unwrap_or_else(|error| panic!("{subdirs:?}: the next launch failed: {error}"));
        again.stop().await.unwrap();
    }
}

/// #2007: a data directory that keeps `PG_VERSION` but lost another file
/// `initdb` writes is what a kill during `initdb`'s own cleanup leaves on a
/// filesystem that lists `PG_VERSION` after that file. The launch
/// initialises it again.
///
/// Before, `base/5` read as a database, so the directory was marked and
/// failed every launch: `initdb` refused it as not empty, or, with
/// `postgresql.conf` still there, the server could not start. The cleanup
/// removes the top level's files in `readdir` order before any directory.
/// Every ext4 tried, which lists by name hash, and tmpfs left these
/// fixtures, six Linux filesystems of seven (evidence under #2007 in
/// `docs/decisions/2026-10-unattended-rulings.md`).
#[tokio::test]
async fn a_data_directory_an_initdb_cleanup_cut_short_after_pg_version_left_is_initialised_again() {
    for files in [
        &[
            "PG_VERSION",
            "pg_hba.conf",
            "pg_ident.conf",
            "postgresql.auto.conf",
        ][..],
        &["PG_VERSION", "postgresql.auto.conf", "postgresql.conf"],
        &["PG_VERSION"],
    ] {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        cut_cleanup(&data, files);
        let cfg = DbConfig {
            root_dir: dir.path().to_path_buf(),
            existing_url: None,
        };

        let db = EmbeddedDb::start(cfg.clone())
            .await
            .unwrap_or_else(|error| panic!("{files:?}: the launch failed: {error}"));
        let one: (i32,) = sqlx::query_as("select 1")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(one.0, 1, "{files:?}");
        assert!(data.join("pg_hba.conf").is_file(), "{files:?}: initdb ran");
        db.stop().await.unwrap();

        let again = EmbeddedDb::start(cfg)
            .await
            .unwrap_or_else(|error| panic!("{files:?}: the next launch failed: {error}"));
        again.stop().await.unwrap();
    }
}

/// #2007: the same directory with `postmaster.opts`, which every server
/// start writes and `initdb` never does, is one a server ran on. It is kept,
/// left unmarked, and the launch fails naming it.
#[tokio::test]
async fn a_data_directory_a_server_ran_on_that_lost_an_initdb_file_is_kept_unmarked() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    cut_cleanup(
        &data,
        &["PG_VERSION", "pg_hba.conf", "postgresql.auto.conf"],
    );
    std::fs::write(data.join("postmaster.opts"), "postgres -D data\n").unwrap();

    let refused = EmbeddedDb::start(DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    })
    .await;

    let error = match refused {
        Ok(db) => {
            db.stop().await.unwrap();
            panic!("a directory with no pg_control started");
        }
        Err(error) => error.to_string(),
    };
    assert!(
        error.contains(data.to_str().unwrap()),
        "the error names the directory: {error}"
    );
    assert!(data.join("PG_VERSION").is_file(), "kept");
    assert!(data.join("base/5").is_dir(), "kept");
    assert!(
        !data.join("knobas-initdb-finished").exists(),
        "not marked finished"
    );
}

/// What `initdb`'s cleanup leaves at `data` when a kill cuts it short after
/// it removed some of the top level's files: `files`, and every directory a
/// finished `initdb` makes, `base/1`, `4` and `5` among them.
fn cut_cleanup(data: &std::path::Path, files: &[&str]) {
    for sub in [
        "base/1",
        "base/4",
        "base/5",
        "global",
        "pg_tblspc",
        "pg_wal",
        "pg_xact",
    ] {
        std::fs::create_dir_all(data.join(sub)).unwrap();
    }
    for file in files {
        std::fs::write(data.join(file), "# written by initdb\n").unwrap();
    }
}

/// #1819: a data directory that holds a database is never removed, whatever
/// else it lacks. Each one here fails to start, and every file in it is still
/// there afterwards.
///
/// A database is a directory under `base/` other than the templates `1` and
/// `4`, or anything under `pg_tblspc/`. A killed `initdb` leaves no other
/// directory under `base/` but `5`, the `postgres` database it makes last.
/// Each fixture here lacks `global/pg_control` and has no marker, so only the
/// database keeps it. The second has no `postgresql.conf`, so `initdb` runs
/// and refuses a directory that is not empty. Its `base/5` counts as a
/// database because a server ran there (`postmaster.opts`). Without one, it
/// is what a cut-short cleanup leaves (#2007).
#[tokio::test]
async fn a_data_directory_that_holds_a_database_is_never_removed() {
    for (with_conf, database) in [
        (true, "base/16384"),
        (false, "base/5"),
        (true, "pg_tblspc/16385"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(data.join("base/1")).unwrap();
        std::fs::create_dir_all(data.join("pg_tblspc")).unwrap();
        std::fs::create_dir_all(data.join(database)).unwrap();
        std::fs::write(data.join("PG_VERSION"), "18\n").unwrap();
        std::fs::write(data.join("keep-me"), "a reader's file\n").unwrap();
        if with_conf {
            std::fs::write(data.join("postgresql.conf"), "# written by initdb\n").unwrap();
        } else {
            std::fs::write(data.join("postmaster.opts"), "postgres -D data\n").unwrap();
        }

        let refused = EmbeddedDb::start(DbConfig {
            root_dir: dir.path().to_path_buf(),
            existing_url: None,
        })
        .await;
        if let Ok(db) = refused {
            db.stop().await.unwrap();
            panic!("a directory with no pg_control started ({database})");
        }
        assert!(
            data.join("PG_VERSION").is_file() && data.join("keep-me").is_file(),
            "the directory is kept ({database})"
        );
        assert_eq!(
            data.join("postgresql.conf").is_file(),
            with_conf,
            "the directory is kept ({database})"
        );
        assert!(
            data.join(database).is_dir(),
            "the directory is kept ({database})"
        );
    }
}

/// #1642: the superuser password file is readable by its owner alone, and so
/// is the data directory, on a fresh profile and on one an older build left
/// wider. The file is the whole key to the mirror: the server listens on
/// loopback with password auth, and any local account could connect with it.
#[cfg(unix)]
#[tokio::test]
async fn the_password_file_and_the_data_directory_are_the_owners_alone() {
    use std::os::unix::fs::PermissionsExt;
    let mode =
        |path: &std::path::Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    let dir = tempfile::tempdir().unwrap();
    let password_file = dir.path().join(".pgpass");
    let data = dir.path().join("data");
    let cfg = DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    };

    let db = EmbeddedDb::start(cfg.clone()).await.unwrap();
    db.stop().await.unwrap();
    assert_eq!(mode(&password_file), 0o600, "a fresh password file");
    assert_eq!(mode(&data) & 0o077, 0, "a fresh data directory");

    // What a build before #1642 left: `File::create`'s 0644, and a data
    // directory PostgreSQL itself accepts (group access, 0750).
    std::fs::set_permissions(&password_file, std::fs::Permissions::from_mode(0o644)).unwrap();
    std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o750)).unwrap();
    let db = EmbeddedDb::start(cfg).await.unwrap();
    db.stop().await.unwrap();
    assert_eq!(
        mode(&password_file),
        0o600,
        "an older build's password file"
    );
    assert_eq!(mode(&data), 0o700, "an older build's data directory");
}

/// The server listens on `127.0.0.1` and nowhere else on its port (#1775).
/// PostgreSQL's default, `localhost`, adds `::1`, and a server that came up
/// there alone, because a stranger held `127.0.0.1:<port>`, was alive while
/// the stranger answered our clients. On one address that start fails
/// instead, which is what lets a stranger's answer clear our lock.
#[tokio::test]
async fn a_started_server_listens_on_the_ipv4_loopback_alone() {
    let dir = tempfile::tempdir().unwrap();
    let db = EmbeddedDb::start(DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    })
    .await
    .unwrap();
    let port = running_port(dir.path());
    let answers = |ip: std::net::IpAddr| {
        std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::new(ip, port),
            std::time::Duration::from_millis(500),
        )
        .is_ok()
    };
    let on_v4 = answers(std::net::Ipv4Addr::LOCALHOST.into());
    let on_v6 = answers(std::net::Ipv6Addr::LOCALHOST.into());
    db.stop().await.unwrap();
    assert!(on_v4, "the server answers on 127.0.0.1");
    assert!(!on_v6, "the server must not listen on ::1 as well");
}

/// #1779: a postmaster that dies under its handle is seen gone by the
/// handle's watch, and the handle starts it again on the port its pools and
/// connector dial, while it goes on holding the profile: a second launch in
/// between is refused, as it is beside any live owner (ruling B2).
#[tokio::test]
async fn a_server_that_dies_under_its_handle_is_seen_gone_and_restarts_on_its_port() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    };
    let mut db = EmbeddedDb::start(cfg.clone()).await.unwrap();
    let port = running_port(dir.path());
    let watch = db.watch().expect("a server this process owns is watched");
    assert!(!watch.is_gone(), "a running server is not gone");

    knobas_db::test_util::kill_server(dir.path());
    assert!(watch.is_gone(), "a killed server is gone");
    match EmbeddedDb::start(cfg).await {
        Err(knobas_db::DbError::AlreadyRunning { .. }) => {}
        Err(other) => panic!("a second launch beside the owner is refused, not {other}"),
        Ok(_) => panic!("a second launch was let in while the owner's server was down"),
    }

    db.restart().await.expect("the dead server starts again");
    assert_eq!(running_port(dir.path()), port, "on the port it had");
    let one: (i32,) = sqlx::query_as("select 1")
        .fetch_one(db.pool())
        .await
        .expect("the handle's pool reaches the restarted server");
    assert_eq!(one.0, 1);
    assert!(
        !db.watch().expect("still owned").is_gone(),
        "the restarted server is watched as running"
    );
    db.stop().await.unwrap();
}

/// #1896: the app's server writes durably, and goes on doing so after the
/// handle starts it again.
///
/// `postgresql_embedded` passes `-F` (`fsync=off`) to every server it starts;
/// a crash or a power loss on a server running that way can leave the data
/// directory corrupt. `EmbeddedDb::start` is the desktop app's path.
#[tokio::test]
async fn the_apps_server_runs_with_fsync_on_after_a_restart_too() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = EmbeddedDb::start(DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    })
    .await
    .unwrap();
    let started = durability(db.pool()).await;

    knobas_db::test_util::kill_server(dir.path());
    db.restart().await.expect("the dead server starts again");
    let restarted = durability(db.pool()).await;
    db.stop().await.unwrap();

    assert_eq!(started, DURABLE_SETTINGS, "the started server");
    assert_eq!(restarted, DURABLE_SETTINGS, "the restarted server");
}

/// #1896: an orphan this build left is adopted, and the adopting handle's
/// restart (a `PostgreSQL` of its own over the settings it adopted with)
/// starts it durable as well.
#[tokio::test]
async fn an_adopted_server_restarts_with_fsync_on() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    };
    EmbeddedDb::start(cfg.clone()).await.unwrap().abandon();
    let mut adopted = EmbeddedDb::start(cfg).await.expect("the orphan is adopted");

    knobas_db::test_util::kill_server(dir.path());
    adopted
        .restart()
        .await
        .expect("the adopted server starts again");
    let restarted = durability(adopted.pool()).await;
    adopted.stop().await.unwrap();

    assert_eq!(restarted, DURABLE_SETTINGS);
}

/// #1896: an orphan a build before the fix left runs with `fsync` off, and
/// the launch that adopts it starts it again with `fsync` on, data intact.
/// The gate's test server stands in for that build: it is the one start that
/// still passes `fsync=off`, on the same data directory layout.
#[tokio::test]
async fn an_orphan_running_with_fsync_off_is_adopted_with_fsync_on() {
    let dir = tempfile::tempdir().unwrap();
    let orphan = EmbeddedDb::start_with_max_connections(dir.path().to_path_buf(), 20)
        .await
        .unwrap();
    sqlx::query("create table kept as select 1896 as issue")
        .execute(orphan.pool())
        .await
        .unwrap();
    assert_eq!(
        durability(orphan.pool()).await[0],
        "fsync=off",
        "the orphan"
    );
    orphan.abandon();

    let adopted = EmbeddedDb::start(DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    })
    .await
    .expect("the orphan is adopted");
    let settings = durability(adopted.pool()).await;
    let kept: i32 = sqlx::query_scalar("select issue from kept")
        .fetch_one(adopted.pool())
        .await
        .unwrap();
    adopted.stop().await.unwrap();

    assert_eq!(settings, DURABLE_SETTINGS);
    assert_eq!(kept, 1896);
    assert!(
        !dir.path().join("data/postmaster.pid").exists(),
        "the quit stops the server the adoption started"
    );
}

/// #1896: the test server `just test` starts keeps `fsync` off, on purpose:
/// its data directory is thrown away at the end of the run, and nothing on it
/// has to outlive a crash. The rest stay at their defaults.
#[tokio::test]
async fn the_gates_test_server_runs_with_fsync_off() {
    let dir = tempfile::tempdir().unwrap();
    let db = EmbeddedDb::start_with_max_connections(dir.path().to_path_buf(), 20)
        .await
        .unwrap();
    let settings = durability(db.pool()).await;
    db.stop().await.unwrap();

    assert_eq!(
        settings,
        ["fsync=off", "synchronous_commit=on", "full_page_writes=on"]
    );
}

/// `fsync`, `synchronous_commit` and `full_page_writes` at PostgreSQL's
/// defaults: the values a crash cannot corrupt a database under.
const DURABLE_SETTINGS: [&str; 3] = ["fsync=on", "synchronous_commit=on", "full_page_writes=on"];

/// What the server behind `pool` runs those three settings at, each as
/// `name=value`.
async fn durability(pool: &sqlx::PgPool) -> Vec<String> {
    let mut read = Vec::new();
    for name in ["fsync", "synchronous_commit", "full_page_writes"] {
        let value: String = sqlx::query_scalar("select current_setting($1)")
            .bind(name)
            .fetch_one(pool)
            .await
            .unwrap();
        read.push(format!("{name}={value}"));
    }
    read
}

/// #1779: stopping a handle whose server already died is not a failure.
/// The quit of a window whose postmaster died has nothing left to stop, and
/// used to log `pg_ctl`'s complaint as an error.
#[tokio::test]
async fn stopping_a_server_that_already_died_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let db = EmbeddedDb::start(DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    })
    .await
    .unwrap();
    knobas_db::test_util::kill_server(dir.path());
    db.stop()
        .await
        .expect("a server that is already gone stops cleanly");
}

/// #1789: the after-reboot shape with a silent port. Our lock file names a
/// pid that has been recycled to some live, unrelated process of ours, and a
/// port nothing listens on. Nobody holds the ownership lock, so no knobas has
/// the profile open, and the start clears the lock and comes up on its own
/// data directory, where it used to be refused on every launch until the
/// reader deleted `postmaster.pid` (ruling B5, reversed by #1789).
///
/// The recorded pid must belong to a process that is alive and is not the
/// postmaster's own ancestry: `CreateLockFile` unlinks the file itself for a
/// dead pid, and exempts its own pid, its parent and its grandparent (the test
/// binary is the grandparent). Either shortcut lets `start()` succeed on the
/// first attempt and `inspect_lock` is never reached. A spawned `sleep`
/// satisfies both, so PostgreSQL genuinely refuses the first start and the
/// lock really reaches `inspect_lock`.
#[tokio::test]
async fn a_lock_naming_a_live_unrelated_process_and_a_silent_port_is_cleared() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    };
    EmbeddedDb::start(cfg.clone())
        .await
        .unwrap()
        .stop()
        .await
        .unwrap();

    let mut sleeper = Sleeper::spawn();
    let silent = SilentPort::bind();
    let silent_port = silent.port();
    let data_dir = dir.path().join("data");
    let pid_file = data_dir.join("postmaster.pid");
    let lock = format!(
        "{}\n{}\n1700000000\n{silent_port}\n",
        sleeper.pid(),
        data_dir.display()
    );
    std::fs::write(&pid_file, &lock).unwrap();
    assert!(
        !port_answers(silent_port),
        "the recorded port must be silent"
    );

    let db = match EmbeddedDb::start(cfg).await {
        Ok(db) => db,
        Err(error) => panic!(
            "no knobas owns the profile and nothing answers on the port, whatever the pid does: \
             {error}"
        ),
    };
    let (serving,): (String,) = sqlx::query_as("show data_directory")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(
        std::fs::canonicalize(&serving).unwrap(),
        std::fs::canonicalize(&data_dir).unwrap(),
        "the start came up on its own data directory"
    );
    assert!(db.owns_server());
    assert_ne!(
        std::fs::read_to_string(&pid_file).unwrap(),
        lock,
        "the stale lock was replaced by the new server's own"
    );
    assert!(
        sleeper.is_alive(),
        "clearing the lock leaves the recorded process alone"
    );

    db.stop().await.unwrap();
}

/// Two first launches on one profile at the same time: exactly one `initdb`
/// happens, one launch brings the server up, and the other is refused.
///
/// Nothing on disk lets the second process see the first coming -- `setup()`
/// decides whether to initialise by looking at an empty directory, and there is
/// no `postmaster.pid` to adopt yet -- so without the bring-up lock the two
/// interleave inside one data directory. This is the shape of a double-click on
/// the app icon, or `just dev` started beside a packaged build. The loser waits
/// for the bring-up lock, finds the winner holding the ownership lock, and is
/// refused before it touches the directory (ruling B2): a second window on a
/// live owner is never let in.
///
/// Both launches really are in flight whether or not the binaries had to be
/// downloaded: `start_managed` yields before `setup()`.
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
    let (winner, refused) = match (first, second) {
        (Ok(winner), Err(refused)) | (Err(refused), Ok(winner)) => (winner, refused),
        (Ok(one), Ok(two)) => {
            one.stop().await.unwrap();
            two.stop().await.unwrap();
            panic!("both launches came up on one profile");
        }
        (Err(one), Err(two)) => panic!("neither launch came up: {one}; {two}"),
    };
    assert!(
        matches!(refused, knobas_db::DbError::AlreadyRunning { .. }),
        "the loser is refused as a second window, not broken by the race: {refused}"
    );

    let one: (i32,) = sqlx::query_as("select 1")
        .fetch_one(winner.pool())
        .await
        .unwrap();
    assert_eq!(one.0, 1);
    winner.stop().await.unwrap();
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
    let silent = SilentPort::bind();
    let silent_port = silent.port();
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

/// A second embedded server on a profile of its own, standing in for a
/// PostgreSQL that is not ours: its superuser password is its own invention,
/// so ours is refused there with `28P01`.
///
/// A guard, so the server stops on every path, a failed assertion included:
/// dropping the handle runs `postgresql_embedded`'s own `pg_ctl stop -m fast`.
/// The happy path calls [`Squatter::stop`] instead, which reports a failure.
struct Squatter {
    db: Option<EmbeddedDb>,
    _root: tempfile::TempDir,
    port: u16,
}

impl Squatter {
    async fn start() -> Squatter {
        let root = tempfile::tempdir().unwrap();
        let db = EmbeddedDb::start(DbConfig {
            root_dir: root.path().to_path_buf(),
            existing_url: None,
        })
        .await
        .expect("the squatter's own server starts");
        let port = running_port(root.path());
        Squatter {
            db: Some(db),
            _root: root,
            port,
        }
    }

    fn pool(&self) -> &sqlx::PgPool {
        self.db.as_ref().expect("the squatter is running").pool()
    }

    async fn stop(mut self) {
        if let Some(db) = self.db.take() {
            db.stop().await.expect("stop the squatter");
        }
    }
}

impl Drop for Squatter {
    fn drop(&mut self) {
        // Explicit rather than left to field order: the handle goes before
        // the root it lives in is removed.
        drop(self.db.take());
    }
}

/// CORE-2 (spec #1619, ruling B5): our lock file names a process that is gone
/// and a port another PostgreSQL now holds, one that refuses our password.
/// That is a stale lock: a postmaster of ours accepts our password, so a
/// refusal beside a dead pid is the same evidence the wire-level arm takes.
///
/// knobas only reads the lock when a start has failed, and PostgreSQL's own
/// `CreateLockFile` removes a lock whose process is gone before it can fail
/// on it. So the lock reaches `adopt` when the start fails for some other
/// reason, here a parameter PostgreSQL refuses before it touches the lock.
/// That start used to end in `AlreadyRunning`, "another knobas instance",
/// which is false and hides the real error. Now the lock is cleared and the
/// real error is what comes back. Repaired, the next start is a clean one.
#[tokio::test]
async fn a_lock_naming_a_foreign_postgres_and_a_dead_process_is_cleared() {
    let ours = tempfile::tempdir().unwrap();
    let cfg = DbConfig {
        root_dir: ours.path().to_path_buf(),
        existing_url: None,
    };
    EmbeddedDb::start(cfg.clone())
        .await
        .unwrap()
        .stop()
        .await
        .unwrap();
    let data_dir = ours.path().join("data");
    let conf = data_dir.join("postgresql.conf");
    let good_conf = std::fs::read_to_string(&conf).unwrap();
    std::fs::write(&conf, format!("{good_conf}\nknobas_cannot_start = on\n")).unwrap();

    let squatter = Squatter::start().await;
    let pid_file = data_dir.join("postmaster.pid");
    let forge = || {
        std::fs::write(
            &pid_file,
            format!(
                "{}\n{}\n1700000000\n{}\n",
                dead_pid(),
                data_dir.display(),
                squatter.port
            ),
        )
        .unwrap();
    };

    forge();
    let error = match EmbeddedDb::start(cfg.clone()).await {
        Ok(_) => panic!("a parameter PostgreSQL refuses cannot start"),
        Err(error) => error,
    };
    assert!(
        !matches!(error, knobas_db::DbError::AlreadyRunning { .. }),
        "a dead process and a server that refuses our password is no knobas instance: {error}"
    );
    assert!(!pid_file.exists(), "the stale lock is cleared");

    std::fs::write(&conf, good_conf).unwrap();
    forge();
    let db = EmbeddedDb::start(cfg)
        .await
        .expect("the repaired profile starts past the same lock");
    let (serving,): (String,) = sqlx::query_as("show data_directory")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(
        std::fs::canonicalize(&serving).unwrap(),
        std::fs::canonicalize(&data_dir).unwrap(),
        "the start came up on its own data directory"
    );
    assert!(db.owns_server());

    let one: (i32,) = sqlx::query_as("select 1")
        .fetch_one(squatter.pool())
        .await
        .expect("clearing our own stale lock leaves the squatter serving");
    assert_eq!(one.0, 1);

    db.stop().await.unwrap();
    squatter.stop().await;
}

/// #1775: the after-reboot shape. Our lock file names a pid that has been
/// recycled to some live, unrelated process of ours, and a port another
/// PostgreSQL now holds, one that refuses our password. Nobody holds the
/// ownership lock, so no knobas has the profile open, and a postmaster of
/// ours bound to that port would have answered there itself. The start clears
/// the lock and comes up on its own data directory, where it used to be
/// refused on every launch until the reader deleted `postmaster.pid`.
///
/// The live pid is a spawned `sleep`, so PostgreSQL's own `CreateLockFile`
/// refuses the first start (it removes a lock whose process is gone itself)
/// and the lock really reaches `adopt`.
#[tokio::test]
async fn a_lock_naming_a_foreign_postgres_and_a_live_unrelated_process_is_cleared() {
    let ours = tempfile::tempdir().unwrap();
    let cfg = DbConfig {
        root_dir: ours.path().to_path_buf(),
        existing_url: None,
    };
    EmbeddedDb::start(cfg.clone())
        .await
        .unwrap()
        .stop()
        .await
        .unwrap();
    let data_dir = ours.path().join("data");
    let squatter = Squatter::start().await;
    let sleeper = Sleeper::spawn();
    let pid_file = data_dir.join("postmaster.pid");
    std::fs::write(
        &pid_file,
        format!(
            "{}\n{}\n1700000000\n{}\n",
            sleeper.pid(),
            data_dir.display(),
            squatter.port
        ),
    )
    .unwrap();

    let db = match EmbeddedDb::start(cfg).await {
        Ok(db) => db,
        Err(error) => panic!("a stranger on the port is not ours, whatever the pid does: {error}"),
    };
    let (serving,): (String,) = sqlx::query_as("show data_directory")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(
        std::fs::canonicalize(&serving).unwrap(),
        std::fs::canonicalize(&data_dir).unwrap(),
        "the start came up on its own data directory"
    );
    assert!(db.owns_server());
    let one: (i32,) = sqlx::query_as("select 1")
        .fetch_one(squatter.pool())
        .await
        .expect("clearing our own stale lock leaves the squatter serving");
    assert_eq!(one.0, 1);

    db.stop().await.unwrap();
    squatter.stop().await;
}

/// A live process that is not a postmaster, killed and reaped on every path.
struct Sleeper(std::process::Child);

impl Sleeper {
    fn spawn() -> Sleeper {
        Sleeper(
            std::process::Command::new("sleep")
                .arg("120")
                .spawn()
                .unwrap(),
        )
    }

    fn pid(&self) -> u32 {
        self.0.id()
    }

    fn is_alive(&mut self) -> bool {
        self.0.try_wait().unwrap().is_none()
    }
}

impl Drop for Sleeper {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A pid that is certainly not running: a child that has already been reaped.
fn dead_pid() -> u32 {
    let mut reaped = std::process::Command::new("true").spawn().unwrap();
    let pid = reaped.id();
    reaped.wait().unwrap();
    pid
}

/// A second `start()` on a profile a **live** owner has open is refused, and
/// leaves the owner's server standing.
///
/// The refusal used to be an adoption: the second handle borrowed the server,
/// and the owner's quit stopped it under the borrower (CORE-3). Before that,
/// it was worse: `postgresql_embedded`'s `Drop` runs `pg_ctl stop -m fast`
/// whenever `postmaster.pid` merely exists, so the *failed* second handle
/// killed the first one's database on its way out. The assertion that the
/// first pool still answers afterwards is what pins that down.
#[tokio::test]
async fn a_second_start_on_a_live_owner_is_refused_and_never_stops_its_server() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    };

    let first = EmbeddedDb::start(cfg.clone()).await.unwrap();

    match EmbeddedDb::start(cfg).await {
        Err(knobas_db::DbError::AlreadyRunning { .. }) => {}
        Err(other) => panic!("expected AlreadyRunning, got {other}"),
        Ok(second) => {
            second.stop().await.unwrap();
            panic!("a second start on a live owner was let in");
        }
    }

    let one: (i32,) = sqlx::query_as("select 1")
        .fetch_one(first.pool())
        .await
        .expect("a refused second start must never stop the owner's server");
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

    // An orphan, as a signal-kill leaves one: a live owner would refuse the
    // second start before adoption is reached.
    EmbeddedDb::start(cfg.clone()).await.unwrap().abandon();

    // Take the database away from under the running server. `with (force)`
    // terminates the orphan's sessions, which is what makes the drop possible
    // at all while its leaked pool is connected.
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

    // Adopted from an orphan, so this handle's stop is the one that stops it.
    assert!(adopted.owns_server());
    adopted.stop().await.unwrap();
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

/// A port on `127.0.0.1` that nothing answers on for as long as this lives
/// (#1808): bound, and never listened on, so a connect to it is refused and
/// no other bind on the machine can be handed it. A port picked by binding a
/// listener and dropping it was free again at once, and the next bind to
/// port 0 -- a postmaster of another test or another gate -- could take it
/// before the test was done with it.
struct SilentPort {
    port: u16,
    _held: tokio::net::TcpSocket,
}

impl SilentPort {
    fn bind() -> SilentPort {
        let socket = tokio::net::TcpSocket::new_v4().unwrap();
        socket
            .bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
            .unwrap();
        SilentPort {
            port: socket.local_addr().unwrap().port(),
            _held: socket,
        }
    }

    fn port(&self) -> u16 {
        self.port
    }
}

/// What the silent-port tests rest on (#1808): a held [`SilentPort`] answers
/// nothing, and nobody else can bind it, not even with `SO_REUSEADDR`, which
/// PostgreSQL sets on its own listeners.
#[test]
fn a_silent_port_answers_nothing_and_cannot_be_taken_while_held() {
    let silent = SilentPort::bind();
    assert!(!port_answers(silent.port()), "a connect is refused");

    let squatter = tokio::net::TcpSocket::new_v4().unwrap();
    squatter.set_reuseaddr(true).unwrap();
    assert!(
        squatter
            .bind(std::net::SocketAddr::from(([127, 0, 0, 1], silent.port())))
            .is_err(),
        "a second bind to the held port is refused"
    );
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

/// The other half of the same rule (ruling B2, spec #1619): a server a *live*
/// instance owns is never adopted. Two knobas windows on one profile used to
/// share it, and the owner's quit stopped the server under the guest, whose
/// every command then failed (CORE-3). The second launch is refused instead,
/// with a message naming the window to quit and the profile it has open, and
/// once that window has quit the next launch is an ordinary start.
#[tokio::test]
async fn a_second_launch_on_a_live_owner_is_refused_naming_the_owner() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    };

    let owner = EmbeddedDb::start(cfg.clone()).await.unwrap();
    let port = port_of(dir.path());
    assert!(owner.owns_server());

    let refused = match EmbeddedDb::start(cfg.clone()).await {
        Err(refused) => refused,
        Ok(guest) => {
            guest.stop().await.unwrap();
            panic!("a second launch on a live owner was let in");
        }
    };
    let message = refused.to_string();
    assert!(
        matches!(refused, knobas_db::DbError::AlreadyRunning { .. }),
        "{message}"
    );
    // The owner is this test's own handle, so its pid is this process's.
    assert!(
        message.contains(&format!("knobas process {} ", std::process::id())),
        "the message names the owning process: {message}"
    );
    assert!(
        message.contains(&dir.path().join("data").display().to_string()),
        "the message names the profile's data directory: {message}"
    );
    assert!(port_answers(port), "the refusal left the owner's server up");

    owner.stop().await.unwrap();
    assert!(!port_answers(port));

    let next = EmbeddedDb::start(cfg)
        .await
        .expect("once the owner has quit, the profile starts normally");
    assert!(next.owns_server());
    next.stop().await.unwrap();
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
    // The hog's statement is running on the server, so its connection is
    // checked out of the pool when the stop comes (#1927). Before, a 300 ms
    // margin stood for this, and a hog slower to start let the stop find an
    // idle pool.
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let running: i64 = sqlx::query_scalar(
                "select count(*) from pg_stat_activity
                  where query = 'select pg_sleep(30)' and state = 'active'",
            )
            .fetch_one(db.pool())
            .await
            .unwrap();
            if running > 0 {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the hog's statement never ran");

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
    // A port nothing answers on, held until the test ends.
    let silent = SilentPort::bind();
    let port = silent.port();
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
/// -- on a migrated database of its own, not the maintenance database the URL
/// names -- and two binaries on the same server get two databases.
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
    let (_, output) = run_a_child(test, url);
    stdout_of_a_passed_child(test, &output)
}

/// This binary re-run as a child on the one test named `test` -- its pid and
/// output, passed or not.
fn run_a_child(test: &str, url: Option<&str>) -> (u32, std::process::Output) {
    run_a_child_with(test, url, &[])
}

/// [`run_a_child`], with `env` set in the child's environment too.
fn run_a_child_with(
    test: &str,
    url: Option<&str>,
    env: &[(&str, &str)],
) -> (u32, std::process::Output) {
    let mut child = std::process::Command::new(std::env::current_exe().unwrap());
    child
        .args(["--exact", test, "--nocapture"])
        .envs(env.iter().copied())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    match url {
        Some(url) => child.env("KNOBAS_TEST_DB_URL", url),
        None => child.env_remove("KNOBAS_TEST_DB_URL"),
    };
    let child = child.spawn().expect("re-run this binary as a child");
    let pid = child.id();
    let output = child.wait_with_output().expect("wait for the child");
    (pid, output)
}

/// A child's stdout, once it has passed and has run at least one test.
fn stdout_of_a_passed_child(test: &str, output: &std::process::Output) -> String {
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
///
/// That server outlives the child -- `test_util` keeps it in a `static` -- so
/// it is adopted and stopped here, rather than left running beside the gate
/// until the next run's reaper finds it.
#[tokio::test]
async fn on_a_server_of_its_own_each_source_file_gets_a_database_of_its_own() {
    let (pid, output) = run_a_child("the_shared_database_follows_the_calling_source_file", None);
    stop_the_server_a_child_left(pid).await;

    let stdout = stdout_of_a_passed_child(
        "the_shared_database_follows_the_calling_source_file",
        &output,
    );
    let (here, there) = files_of(&stdout);
    assert_ne!(here, there);
}

/// Stop the server a finished child started in its scratch root
/// (`test_util`'s `$TMPDIR/knobas-test-<pid>`), and remove the root and the
/// lock beside it. The child is gone, so its lock is free and the server is
/// an orphan, which a start on the same root adopts and owns.
///
/// Under that lock, taken first and held to the end, as a reaper takes it
/// (#1802): with the child gone, the first test binary of any run to sweep
/// `$TMPDIR` finds the root abandoned, and a sweep that ran between this
/// adoption and its stop deleted the data directory under it -- `pg_ctl:
/// directory ... is not a database cluster directory`. Held here, the lock
/// makes every reaper pass the root by; taken after a reaper's, it finds
/// nothing left to stop.
async fn stop_the_server_a_child_left(pid: u32) {
    let root = std::env::temp_dir().join(format!("knobas-test-{pid}"));
    let lock_path = root.with_file_name(format!("knobas-test-{pid}.lock"));
    let lock = std::fs::File::create(&lock_path).expect("the child's scratch lock");
    lock.lock()
        .expect("wait out any reaper of the child's root");
    if root.join("data").join("postmaster.pid").exists() {
        let adopted = EmbeddedDb::start(DbConfig {
            root_dir: root.clone(),
            existing_url: None,
        })
        .await
        .expect("adopt the server the child left");
        assert!(
            adopted.owns_server(),
            "the child's server has another owner"
        );
        adopted
            .stop()
            .await
            .expect("stop the server the child left");
    }
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_file(&lock_path);
    drop(lock);
}
