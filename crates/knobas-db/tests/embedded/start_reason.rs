//! A server start that fails names the data directory and the reason the
//! server gave (#2025).
//!
//! `pg_ctl start` prints only *waiting for server to start.... stopped
//! waiting*; the server writes why it stopped to `<data_dir>/start.log`.
//! Each test removes `pg_hba.conf`, which the server refuses to start without
//! (*could not load pg_hba.conf*) while `pg_ctl` has nothing to say about it,
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

    let error = match EmbeddedDb::start(config(dir.path())).await {
        Ok(db) => {
            db.stop().await.unwrap();
            panic!("a cluster with no pg_hba.conf started");
        }
        Err(error) => error.to_string(),
    };
    assert_names_the_directory_and_the_reason(&error, &data_dir);
}
