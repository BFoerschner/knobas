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
