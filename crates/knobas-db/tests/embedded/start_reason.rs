//! A server start that fails names the data directory and the reason the
//! server gave (#2025).
//!
//! `pg_ctl start` prints only *waiting for server to start.... stopped
//! waiting*; the server writes why it stopped to `<data_dir>/start.log`.
//! Each test removes `pg_hba.conf`, which the server refuses to start without
//! (*could not load …/pg_hba.conf*) while `pg_ctl` has nothing to say about it,
//! and drives one of the paths that start a server.

use knobas_db::{DbConfig, EmbeddedDb};

fn config(root_dir: &std::path::Path) -> DbConfig {
    DbConfig {
        root_dir: root_dir.to_path_buf(),
        existing_url: None,
    }
}

/// The error names the data directory and the server's own reason, which
/// names `pg_hba.conf` (*could not load <data_dir>/pg_hba.conf* on PostgreSQL
/// 18). `pg_ctl`'s output names neither.
#[track_caller]
fn assert_names_the_directory_and_the_reason(error: &str, data_dir: &std::path::Path) {
    assert!(
        error.contains(data_dir.to_str().unwrap()),
        "the error names the data directory: {error}"
    );
    let without_the_directory = error.replace(data_dir.to_str().unwrap(), "");
    assert!(
        without_the_directory.contains("pg_hba.conf"),
        "the error gives the server's reason: {error}"
    );
}

/// The error a start on `root_dir` failed with, stopping a server that came
/// up anyway.
async fn launch_error(root_dir: &std::path::Path) -> String {
    match EmbeddedDb::start(config(root_dir)).await {
        Ok(db) => {
            db.stop().await.unwrap();
            panic!("a cluster with no pg_hba.conf started");
        }
        Err(error) => error.to_string(),
    }
}

/// The acceptance case: a stopped cluster that lost `pg_hba.conf`, launched.
#[tokio::test]
async fn a_launch_on_a_cluster_that_lost_pg_hba_conf_names_the_directory_and_the_reason() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    EmbeddedDb::start(config(dir.path()))
        .await
        .unwrap()
        .stop()
        .await
        .unwrap();
    std::fs::remove_file(data_dir.join("pg_hba.conf")).unwrap();

    let error = launch_error(dir.path()).await;
    assert_names_the_directory_and_the_reason(&error, &data_dir);
}

/// The handle's own restart after its server died (`restart`, the started
/// handle's arm).
#[tokio::test]
async fn a_restart_on_a_cluster_that_lost_pg_hba_conf_names_the_directory_and_the_reason() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let mut db = EmbeddedDb::start(config(dir.path())).await.unwrap();
    knobas_db::test_util::kill_server(dir.path());
    std::fs::remove_file(data_dir.join("pg_hba.conf")).unwrap();

    let error = db
        .restart()
        .await
        .expect_err("a cluster with no pg_hba.conf started again")
        .to_string();
    db.stop().await.unwrap();
    assert_names_the_directory_and_the_reason(&error, &data_dir);
}

/// An adopted handle's restart, which starts a `PostgreSQL` of its own
/// (`restart`, the adopted arm).
#[tokio::test]
async fn an_adopted_servers_restart_on_a_cluster_that_lost_pg_hba_conf_names_the_directory_and_the_reason() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    EmbeddedDb::start(config(dir.path())).await.unwrap().abandon();
    let mut adopted = EmbeddedDb::start(config(dir.path()))
        .await
        .expect("the orphan is adopted");
    knobas_db::test_util::kill_server(dir.path());
    std::fs::remove_file(data_dir.join("pg_hba.conf")).unwrap();

    let error = adopted
        .restart()
        .await
        .expect_err("a cluster with no pg_hba.conf started again")
        .to_string();
    adopted.stop().await.unwrap();
    assert_names_the_directory_and_the_reason(&error, &data_dir);
}

/// The adoption of an orphan running with `fsync` off, which stops it and
/// starts it again durable (`restart_durable`). A running server does not
/// read `pg_hba.conf` again, so the orphan serves until that start.
#[tokio::test]
async fn adopting_an_orphan_that_lost_pg_hba_conf_names_the_directory_and_the_reason() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    EmbeddedDb::start_with_max_connections(dir.path().to_path_buf(), 20)
        .await
        .unwrap()
        .abandon();
    std::fs::remove_file(data_dir.join("pg_hba.conf")).unwrap();

    let error = launch_error(dir.path()).await;
    assert_names_the_directory_and_the_reason(&error, &data_dir);
    assert!(
        !data_dir.join("postmaster.pid").exists(),
        "the orphan was stopped before the start that failed"
    );
}

/// The retry after a stale lock was cleared (`start_managed`, the
/// `Lock::Cleared` arm). The lock names a live process that is not a
/// postmaster and a port nothing answers on, so the first start is refused
/// over the lock (as in `a_lock_naming_a_live_unrelated_process_and_a_silent_port_is_cleared`)
/// and the retry over `pg_hba.conf`. The reason is the retry's: the first
/// start's `FATAL` about the lock is in the log before it.
#[tokio::test]
async fn the_retry_after_a_stale_lock_names_the_directory_and_the_retrys_reason() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    EmbeddedDb::start(config(dir.path()))
        .await
        .unwrap()
        .stop()
        .await
        .unwrap();
    let sleeper = super::Sleeper::spawn();
    let silent = super::SilentPort::bind();
    std::fs::write(
        data_dir.join("postmaster.pid"),
        format!(
            "{}\n{}\n1700000000\n{}\n",
            sleeper.pid(),
            data_dir.display(),
            silent.port()
        ),
    )
    .unwrap();
    std::fs::remove_file(data_dir.join("pg_hba.conf")).unwrap();

    let error = launch_error(dir.path()).await;
    assert_names_the_directory_and_the_reason(&error, &data_dir);
    assert!(
        !error.contains("postmaster.pid"),
        "the reason is the retry's, not the refused first start's: {error}"
    );
}
