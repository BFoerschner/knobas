//! One embedded PostgreSQL per test binary, shared by every test in it.
//!
//! Enabled by the `test-util` feature (not `cfg(test)` -- that is not set for
//! a crate's own `tests/` directory, nor for downstream crates).
//!
//! Every caller gets the *same* database, so tests must isolate themselves
//! with unique keys. Truncating shared tables would break tests running
//! concurrently in the same binary.
//!
//! # Why there is a reaper in here
//!
//! The instance lives in a `static`, and Rust never drops statics -- so
//! `EmbeddedDb`'s shutdown never runs and the server outlives the test
//! binary. Left alone, every `cargo test` run would abandon another postgres
//! on the developer's machine. Instead each run takes an OS-level lock on its
//! own directory (released by the kernel when the process dies, however it
//! dies) and sweeps the directories of runs whose lock is free.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use sqlx::PgPool;

use crate::{DbConfig, EmbeddedDb};

/// Prefix for this crate's scratch directories under the system temp dir.
const DIR_PREFIX: &str = "knobas-test-";

/// Name of the file whose advisory lock marks a directory as in use.
const OWNER_LOCK: &str = ".owner.lock";

/// Binary name of `pg_ctl` on this platform.
const PG_CTL: &str = if cfg!(windows) {
    "pg_ctl.exe"
} else {
    "pg_ctl"
};

/// The shared pool for this test binary, starting the server on first use.
///
/// # Panics
///
/// Panics if the database cannot be started -- there is no useful way for a
/// test to continue without one.
pub async fn test_pool() -> &'static PgPool {
    static DB: tokio::sync::OnceCell<EmbeddedDb> = tokio::sync::OnceCell::const_new();

    let db = DB
        .get_or_init(|| async {
            let root_dir = std::env::temp_dir().join(format!("{DIR_PREFIX}{}", std::process::id()));
            claim(&root_dir);
            reap_abandoned(&root_dir);
            EmbeddedDb::start(DbConfig {
                root_dir,
                existing_url: None,
            })
            .await
            .expect("start embedded postgres for tests")
        })
        .await;

    db.pool()
}

/// Take an advisory lock on our own directory and hold it for the lifetime of
/// the process. The file handle is parked in a `static` on purpose: it is
/// never closed, so the lock is only ever released by the kernel reaping the
/// process -- including on panic, `SIGKILL`, or a test-harness timeout.
fn claim(root_dir: &Path) {
    static HELD: OnceLock<File> = OnceLock::new();

    std::fs::create_dir_all(root_dir).expect("create test root dir");
    let file = File::create(root_dir.join(OWNER_LOCK)).expect("create owner lock");
    file.try_lock().expect("claim owner lock");
    let _ = HELD.set(file);
}

/// Stop and delete the scratch directories of test binaries that are gone.
///
/// Best-effort: a directory we cannot lock belongs to a binary still running
/// (cargo runs test binaries in parallel) and is left strictly alone.
fn reap_abandoned(own_root: &Path) {
    let Some(parent) = own_root.parent() else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path == own_root || !is_scratch_dir(&path) {
            continue;
        }
        if !is_abandoned(&path) {
            continue;
        }
        stop_server(&path);
        let _ = std::fs::remove_dir_all(&path);
    }
}

fn is_scratch_dir(path: &Path) -> bool {
    path.is_dir()
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(DIR_PREFIX))
}

/// A directory is abandoned when its owner lock can be taken.
///
/// A directory with no lock file at all is only claimed once it has a data
/// directory: otherwise we could delete a sibling run's directory in the
/// instant between its `create_dir_all` and its `File::create`.
fn is_abandoned(root: &Path) -> bool {
    let lock_path = root.join(OWNER_LOCK);
    let Ok(file) = File::open(&lock_path) else {
        return root.join("data").exists();
    };
    // Taking the lock succeeds only if nobody holds it; dropping `file` right
    // after releases it again, which is fine -- we are about to delete it.
    file.try_lock().is_ok()
}

/// Ask an abandoned server to shut down before its files are removed.
///
/// The binaries live in the shared installation directory, not under the
/// scratch directory being reaped.
fn stop_server(root: &Path) {
    if !root.join("data").join("postmaster.pid").exists() {
        return;
    }
    let Some(pg_ctl) = find_pg_ctl(&crate::embedded::installation_dir()) else {
        return;
    };
    let _ = std::process::Command::new(pg_ctl)
        .arg("stop")
        .arg("-D")
        .arg(root.join("data"))
        .args(["-m", "immediate", "-w"])
        .output();
}

/// `pg_ctl` sits at either `<install>/bin/pg_ctl` or, once the archive has
/// been unpacked into a version subdirectory, `<install>/<version>/bin/pg_ctl`.
fn find_pg_ctl(installation_dir: &Path) -> Option<PathBuf> {
    let direct = installation_dir.join("bin").join(PG_CTL);
    if direct.is_file() {
        return Some(direct);
    }
    std::fs::read_dir(installation_dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path().join("bin").join(PG_CTL))
        .find(|path| path.is_file())
}
