//! How a bring-up reaches its own server (ux28, ADR-0053): over TCP on the
//! loopback and over the Unix-domain socket in the data folder, the first of
//! the two that connects. A firewall rule or security software that blocks
//! the app's loopback TCP connect (os error 10013 on Windows) is stood in for
//! by `seam::refuse_tcp_connects`, which refuses this data folder's TCP
//! routes with that error.

use std::path::{Path, PathBuf};

use knobas_db::embedded::seam;
use knobas_db::report::{StartReport, WSAEACCES};
use knobas_db::{DbConfig, EmbeddedDb};

/// A root short enough for a socket path: the Mac's `$TMPDIR` is too deep
/// (103 bytes), `/tmp` is not.
fn short_root() -> tempfile::TempDir {
    let base = if cfg!(unix) {
        PathBuf::from("/tmp")
    } else {
        std::env::temp_dir()
    };
    tempfile::Builder::new()
        .prefix("kl")
        .tempdir_in(base)
        .unwrap()
}

fn config(root: &Path) -> DbConfig {
    DbConfig {
        root_dir: root.to_path_buf(),
        existing_url: None,
    }
}

/// Undoes the seam for a data folder when the test ends, however it ends.
struct Refusing(PathBuf);

impl Refusing {
    fn tcp(data_dir: PathBuf) -> Refusing {
        seam::refuse_tcp_connects(&data_dir, WSAEACCES);
        Refusing(data_dir)
    }
}

impl Drop for Refusing {
    fn drop(&mut self) {
        seam::allow_tcp_connects(&self.0);
    }
}

async fn select_one(db: &EmbeddedDb) -> i32 {
    let (one,): (i32,) = sqlx::query_as("select 1")
        .fetch_one(db.pool())
        .await
        .unwrap();
    one
}

/// ux27's failure, with the socket to fall back on: TCP refused for
/// permissions, and the start ends on the socket. Every connection after it
/// takes the socket too: the pool, a connection of its own, and `pg_dump`,
/// which reads `PGHOST` as the socket's folder.
#[tokio::test]
async fn a_start_whose_tcp_is_refused_ends_on_the_socket_and_a_backup_runs_over_it() {
    let root = short_root();
    let _refusing = Refusing::tcp(root.path().join("data"));
    let mut report = StartReport::default();
    let db = EmbeddedDb::start_reporting(config(root.path()), &mut report)
        .await
        .unwrap_or_else(|error| panic!("{error}\n{report:#?}"));

    assert_eq!(db.connector().link(), "socket", "{report:#?}");
    assert_eq!(select_one(&db).await, 1);
    db.connector()
        .connect()
        .await
        .expect("a connection of its own");
    let last = report.attempts.last().unwrap();
    assert!(
        last.route.starts_with("unix ") && last.error.is_none(),
        "{report:#?}"
    );
    if knobas_db::embedded::preferred_link() == knobas_db::embedded::Link::Tcp {
        let tcp = &report.attempts[0];
        assert!(
            tcp.route.starts_with("tcp 127.0.0.1:") && tcp.denied,
            "{report:#?}"
        );
    }
    assert!(!report.blocked(), "the start went on: {report:#?}");

    // A backup dumps the schema, so it is there first.
    knobas_db::migrate::run(db.pool()).await.unwrap();
    let archive = root.path().join("backup.dump");
    let bytes = knobas_db::backup::dump(&db.connector(), &archive)
        .await
        .expect("pg_dump over the socket");
    assert!(bytes > 0);
    db.stop().await.unwrap();
}

/// Both routes refused: the start fails as `Unreachable`, naming both, and
/// the report holds each with its OS error.
#[tokio::test]
async fn a_start_whose_every_route_is_refused_names_them_all() {
    let root = short_root();
    let data_dir = root.path().join("data");
    let _refusing = Refusing::tcp(data_dir.clone());
    seam::refuse_socket_connects(&data_dir, WSAEACCES);
    let mut report = StartReport::default();
    let Err(error) = EmbeddedDb::start_reporting(config(root.path()), &mut report).await else {
        panic!("every route was refused");
    };
    let message = error.to_string();
    assert!(message.contains("tcp 127.0.0.1:"), "{message}");
    assert!(message.contains("unix "), "{message}");
    assert_eq!(report.failed_routes().len(), 2, "{report:#?}");
    assert!(report.blocked());
    assert!(
        report.attempts.iter().all(|a| a.os_code == Some(WSAEACCES)),
        "{report:#?}"
    );
    assert!(
        !data_dir.join("postmaster.pid").exists(),
        "the failed start stopped the server it started"
    );
}

/// A profile folder too deep for a socket path starts on TCP alone, as every
/// start did before, and the report says why the socket was not offered.
#[tokio::test]
async fn a_root_too_deep_for_a_socket_starts_on_tcp_and_says_why() {
    let root = short_root();
    let deep = root.path().join("d".repeat(60)).join("e".repeat(60));
    let mut report = StartReport::default();
    let db = EmbeddedDb::start_reporting(config(&deep), &mut report)
        .await
        .unwrap();
    assert_eq!(db.connector().link(), "tcp");
    let skipped = &report.attempts[0];
    assert!(
        skipped.skipped && skipped.route.starts_with("unix "),
        "{report:#?}"
    );
    assert!(
        skipped
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("bytes, past the"),
        "{report:#?}"
    );
    assert_eq!(select_one(&db).await, 1);
    db.stop().await.unwrap();
}

/// The socket never makes a start worse than TCP alone: a server that could
/// not make its socket file is started once more without one.
#[tokio::test]
async fn a_server_that_cannot_make_its_socket_starts_again_on_tcp() {
    let root = short_root();
    let data_dir = root.path().join("data");
    seam::break_socket(&data_dir);
    let _undo = Refusing(data_dir.clone());
    let mut report = StartReport::default();
    let db = EmbeddedDb::start_reporting(config(root.path()), &mut report)
        .await
        .unwrap_or_else(|error| panic!("{error}\n{report:#?}"));
    assert_eq!(db.connector().link(), "tcp");
    assert!(
        report.attempts.iter().any(|a| a.skipped
            && a.error
                .as_deref()
                .unwrap_or_default()
                .contains("could not make it")),
        "{report:#?}"
    );
    assert_eq!(select_one(&db).await, 1);
    db.stop().await.unwrap();
}

/// An orphan a killed knobas left running is adopted over its socket while
/// the loopback is refused, and the adopter owns its stop.
#[tokio::test]
async fn an_orphan_is_adopted_over_its_socket_while_tcp_is_refused() {
    let root = short_root();
    EmbeddedDb::start(config(root.path()))
        .await
        .unwrap()
        .abandon();
    let data_dir = root.path().join("data");
    let _refusing = Refusing::tcp(data_dir.clone());
    let mut report = StartReport::default();
    let db = EmbeddedDb::start_reporting(config(root.path()), &mut report)
        .await
        .unwrap_or_else(|error| panic!("{error}\n{report:#?}"));
    assert!(report.adopted, "{report:#?}");
    assert_eq!(db.connector().link(), "socket");
    assert!(
        matches!(report.found, Some(knobas_db::report::Found::Lock { ref socket_dir, .. }) if socket_dir.as_deref() == Some(".")),
        "{report:#?}"
    );
    assert_eq!(select_one(&db).await, 1);
    db.stop().await.unwrap();
    assert!(
        !data_dir.join("postmaster.pid").exists(),
        "the adopter stopped it"
    );
}

/// The orphan's TCP refused for permissions, and no socket to reach it by: the
/// lock is not cleared as a stranger's (it may well be a live server of ours),
/// and the start says it could not connect.
#[tokio::test]
async fn a_refused_tcp_answer_is_no_stranger_and_keeps_the_lock() {
    let root = short_root();
    let deep = root.path().join("d".repeat(60)).join("e".repeat(60));
    EmbeddedDb::start(config(&deep)).await.unwrap().abandon();
    let data_dir = deep.join("data");
    let pid_file = std::fs::read_to_string(data_dir.join("postmaster.pid")).unwrap();
    {
        let _refusing = Refusing::tcp(data_dir.clone());
        let mut report = StartReport::default();
        let outcome = EmbeddedDb::start_reporting(config(&deep), &mut report).await;
        assert!(outcome.is_err(), "{report:#?}");
        assert!(report.blocked(), "{report:#?}");
        assert_eq!(
            std::fs::read_to_string(data_dir.join("postmaster.pid")).unwrap(),
            pid_file,
            "the lock stays"
        );
    }
    // With the loopback back, the orphan is adopted and stopped.
    EmbeddedDb::start(config(&deep))
        .await
        .unwrap()
        .stop()
        .await
        .unwrap();
}

/// A data folder too deep for a socket, with a short folder to fall back to
/// (what Windows gives it under `%LOCALAPPDATA%\knobas`, here a root of the
/// test's own through `seam::fall_back_to`). Undoes the seam when dropped.
struct Deep {
    _root: tempfile::TempDir,
    _short: tempfile::TempDir,
    profile: PathBuf,
    data_dir: PathBuf,
    short_dir: PathBuf,
}

impl Deep {
    fn new(fallback: Option<&Path>) -> Deep {
        let root = short_root();
        let short = match fallback {
            Some(base) => tempfile::Builder::new()
                .prefix("ks")
                .tempdir_in(base)
                .unwrap(),
            None => short_root(),
        };
        let profile = root.path().join("d".repeat(60)).join("e".repeat(60));
        let data_dir = profile.join("data");
        seam::fall_back_to(&data_dir, short.path());
        let short_dir = short.path().join(knobas_db::socket::key(&data_dir));
        Deep {
            _root: root,
            _short: short,
            profile,
            data_dir,
            short_dir,
        }
    }
}

impl Drop for Deep {
    fn drop(&mut self) {
        seam::no_fallback(&self.data_dir);
        seam::allow_tcp_connects(&self.data_dir);
    }
}

/// The Windows pass on ux28: a profile 114 bytes deep started on TCP alone.
/// With the short folder it ends on a socket there while TCP is refused, the
/// report says why the data folder was not tried, and `pg_dump` takes the
/// same folder.
#[tokio::test]
async fn a_root_too_deep_for_a_socket_listens_in_the_short_folder() {
    let deep = Deep::new(None);
    seam::refuse_tcp_connects(&deep.data_dir, WSAEACCES);
    let mut report = StartReport::default();
    let db = EmbeddedDb::start_reporting(config(&deep.profile), &mut report)
        .await
        .unwrap_or_else(|error| panic!("{error}\n{report:#?}"));
    assert_eq!(db.connector().link(), "socket", "{report:#?}");
    let skipped = &report.attempts[0];
    assert!(
        skipped.skipped
            && skipped.route == format!("unix {}", deep.data_dir.display())
            && skipped
                .error
                .as_deref()
                .unwrap_or_default()
                .contains("bytes, past the"),
        "{report:#?}"
    );
    let connected = report.attempts.iter().find(|a| a.error.is_none()).unwrap();
    assert!(
        connected
            .route
            .starts_with(&format!("unix {} (.s.PGSQL.", deep.short_dir.display())),
        "{report:#?}"
    );
    assert_eq!(select_one(&db).await, 1);
    knobas_db::migrate::run(db.pool()).await.unwrap();
    let archive = deep.profile.join("backup.dump");
    knobas_db::backup::dump(&db.connector(), &archive)
        .await
        .expect("pg_dump over the short folder's socket");
    db.stop().await.unwrap();
    assert!(
        !deep.short_dir.exists(),
        "a clean stop leaves no short folder"
    );
}

/// An orphan listening in the short folder is found there and adopted while
/// TCP is refused; a killed server's files in the folder are gone after the
/// adoption, the adopted server's own stay until its stop.
#[tokio::test]
async fn an_orphan_in_the_short_folder_is_adopted_and_old_files_are_swept() {
    let deep = Deep::new(None);
    EmbeddedDb::start(config(&deep.profile))
        .await
        .unwrap()
        .abandon();
    for stale in [".s.PGSQL.1", ".s.PGSQL.1.lock"] {
        std::fs::write(deep.short_dir.join(stale), "").unwrap();
    }
    seam::refuse_tcp_connects(&deep.data_dir, WSAEACCES);
    let mut report = StartReport::default();
    let db = EmbeddedDb::start_reporting(config(&deep.profile), &mut report)
        .await
        .unwrap_or_else(|error| panic!("{error}\n{report:#?}"));
    assert!(report.adopted, "{report:#?}");
    assert_eq!(db.connector().link(), "socket", "{report:#?}");
    assert_eq!(select_one(&db).await, 1);
    let swept = || {
        !deep.short_dir.join(".s.PGSQL.1").exists()
            && !deep.short_dir.join(".s.PGSQL.1.lock").exists()
    };
    for _ in 0..40 {
        if swept() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(swept(), "the killed server's files are gone");
    let own = std::fs::read_dir(&deep.short_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with(".s.PGSQL."))
        .count();
    assert_eq!(own, 2, "the adopted server's socket and lock stay");
    db.stop().await.unwrap();
}

/// Two deep profiles at once each reach their own server through their own
/// short folder.
#[tokio::test]
async fn two_deep_roots_at_once_each_reach_their_own_server() {
    let shared = short_root();
    let one = Deep::new(Some(shared.path()));
    let two = Deep::new(Some(shared.path()));
    seam::refuse_tcp_connects(&one.data_dir, WSAEACCES);
    seam::refuse_tcp_connects(&two.data_dir, WSAEACCES);
    let (a, b) = tokio::join!(
        EmbeddedDb::start(config(&one.profile)),
        EmbeddedDb::start(config(&two.profile))
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    sqlx::query("create table only_in_one (x int)")
        .execute(a.pool())
        .await
        .unwrap();
    let in_two: Option<(String,)> =
        sqlx::query_as("select tablename::text from pg_tables where tablename = 'only_in_one'")
            .fetch_optional(b.pool())
            .await
            .unwrap();
    assert_eq!(in_two, None, "two servers, not one");
    a.stop().await.unwrap();
    b.stop().await.unwrap();
}

/// A short folder the server's command line cannot carry (a space in it) is
/// not tried either: TCP alone, with both folders named as not tried.
#[tokio::test]
async fn a_short_folder_with_a_space_leaves_tcp_alone() {
    let base = short_root();
    let spaced = base.path().join("with space");
    std::fs::create_dir(&spaced).unwrap();
    let deep = Deep::new(Some(&spaced));
    let mut report = StartReport::default();
    let db = EmbeddedDb::start_reporting(config(&deep.profile), &mut report)
        .await
        .unwrap_or_else(|error| panic!("{error}\n{report:#?}"));
    assert_eq!(db.connector().link(), "tcp");
    let skipped: Vec<_> = report.attempts.iter().filter(|a| a.skipped).collect();
    assert_eq!(skipped.len(), 2, "{report:#?}");
    assert!(
        skipped[1]
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("cannot carry"),
        "{report:#?}"
    );
    assert!(!deep.short_dir.exists(), "nothing made");
    db.stop().await.unwrap();
}

/// The Windows pass on ux28: after a killed knobas, the next start ran a
/// second server against the orphan's lock, which can only fail, and on
/// Windows `pg_ctl start` took about 33 s to say so before the adoption. A
/// lock whose server answers is adopted at once: no start is tried, so the
/// server has said nothing.
#[tokio::test]
async fn an_answering_orphan_is_adopted_without_a_doomed_start() {
    let root = short_root();
    EmbeddedDb::start(config(root.path()))
        .await
        .unwrap()
        .abandon();
    let started = std::time::Instant::now();
    let mut report = StartReport::default();
    let db = EmbeddedDb::start_reporting(config(root.path()), &mut report)
        .await
        .unwrap_or_else(|error| panic!("{error}\n{report:#?}"));
    assert!(report.adopted, "{report:#?}");
    assert!(
        report.server_said.is_empty(),
        "no second server was started: {report:#?}"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(select_one(&db).await, 1);
    db.stop().await.unwrap();
}
