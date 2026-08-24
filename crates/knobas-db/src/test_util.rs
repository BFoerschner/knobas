//! One embedded PostgreSQL per test binary, shared by every test in it.
//!
//! Enabled by the `test-util` feature (not `cfg(test)` -- that is not set for
//! a crate's own `tests/` directory, nor for downstream crates).
//!
//! Every caller gets the *same* database, so tests must isolate themselves
//! with unique keys. Truncating shared tables would break tests running
//! concurrently in the same binary.
//!
//! # Why the pool is not shared too
//!
//! The *server* is process-wide; the pool is not. A `PgPool` belongs to the
//! tokio runtime that used it, and `#[tokio::test]` builds a fresh runtime per
//! test. Handing a `static` pool to a second runtime starves it: a connection
//! released while its runtime is shutting down never gets to run the task that
//! returns its permit to the pool's semaphore, so after a test or two every
//! `acquire` fails with `PoolTimedOut`. Each call therefore opens its own pool
//! against the shared server, and the caller drops it with its runtime.
//!
//! # Why there is a reaper in here
//!
//! The instance lives in a `static`, and Rust never drops statics -- so
//! `EmbeddedDb`'s shutdown never runs and the server outlives the test
//! binary. Left alone, every `cargo test` run would abandon another postgres
//! on the developer's machine. Instead each run takes an OS-level lock
//! (released by the kernel when the process dies, however it dies) and sweeps
//! the scratch directories of runs whose lock is free.
//!
//! # Layout
//!
//! ```text
//! $TMPDIR/knobas-test-<pid>/       scratch: data/ and .pgpass
//! $TMPDIR/knobas-test-<pid>.lock   ownership lock, a *sibling* of it
//! ```
//!
//! The lock deliberately sits outside the directory the reaper deletes. Were
//! it inside, a reaper could unlink it out from under a claimant that had
//! created but not yet locked it; the claimant would then hold a lock on an
//! unlinked inode while its recreated directory looked ownerless, and the next
//! reaper would stop a live server.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use sqlx::PgPool;

use crate::{DbConfig, EmbeddedDb};

/// Prefix for this crate's scratch directories under the system temp dir.
const DIR_PREFIX: &str = "knobas-test-";

/// Suffix turning a scratch directory path into its ownership lock path.
const LOCK_SUFFIX: &str = ".lock";

/// Binary name of `pg_ctl` on this platform.
const PG_CTL: &str = if cfg!(windows) {
    "pg_ctl.exe"
} else {
    "pg_ctl"
};

/// A pool onto this test binary's shared database, starting the server on
/// first use.
///
/// The returned pool belongs to the calling runtime and should be dropped with
/// it -- see the module docs for why it is not shared.
///
/// # Panics
///
/// Panics if the database cannot be started, or if the pool cannot connect --
/// there is no useful way for a test to continue without one.
pub async fn test_pool() -> PgPool {
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

    crate::embedded::connect(db.url())
        .await
        .expect("connect a test pool to the shared embedded postgres")
}

/// The ownership lock path for a scratch directory: a sibling, never a child.
fn lock_path(root_dir: &Path) -> PathBuf {
    let mut path = root_dir.as_os_str().to_os_string();
    path.push(LOCK_SUFFIX);
    PathBuf::from(path)
}

/// Take the ownership lock for our own scratch directory and hold it for the
/// lifetime of the process.
///
/// The file handle is parked in a `static` on purpose: it is never closed, so
/// the lock is only ever released by the kernel reaping the process --
/// including on panic, `SIGKILL`, or a test-harness timeout.
///
/// Taken *before* the scratch directory exists, so no reaper can ever observe
/// that directory without an owner.
fn claim(root_dir: &Path) {
    static HELD: OnceLock<File> = OnceLock::new();

    let path = lock_path(root_dir);
    let file = loop {
        let file = File::create(&path).expect("create owner lock");
        file.try_lock().expect("claim owner lock");
        if still_current(&file, &path) {
            break file;
        }
        // A reaper unlinked the file between `create` and `try_lock`; the lock
        // we hold protects an inode nobody can see. Start over.
    };
    let _ = HELD.set(file);
}

/// Whether `file` is still the file living at `path`, rather than an inode
/// somebody unlinked while we were locking it.
#[cfg(unix)]
fn still_current(file: &File, path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;

    let (Ok(held), Ok(named)) = (file.metadata(), std::fs::metadata(path)) else {
        return false;
    };
    held.dev() == named.dev() && held.ino() == named.ino()
}

#[cfg(not(unix))]
fn still_current(_file: &File, path: &Path) -> bool {
    path.exists()
}

/// Stop and delete the scratch directories of test binaries that are gone.
///
/// Best-effort: a directory whose lock we cannot take belongs to a binary
/// still running (cargo runs test binaries in parallel) and is left strictly
/// alone.
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
        let _ = std::fs::remove_file(lock_path(&path));
    }

    sweep_orphan_locks(parent, own_root);
}

/// Delete lock files left with no scratch directory beside them -- a run that
/// died between `claim` and the first `EmbeddedDb::start`. They are empty, but
/// without this they would accumulate in the temp directory forever.
fn sweep_orphan_locks(parent: &Path, own_root: &Path) {
    let own_lock = lock_path(own_root);
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path == own_lock || !is_orphan_lock(&path) {
            continue;
        }
        // Only if nobody holds it: a live owner claims its lock before it
        // creates its directory, so "no directory yet" is a legitimate state.
        if File::open(&path).is_ok_and(|file| file.try_lock().is_ok()) {
            let _ = std::fs::remove_file(&path);
        }
    }
}

fn is_orphan_lock(path: &Path) -> bool {
    let is_lock = path.is_file()
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(DIR_PREFIX) && name.ends_with(LOCK_SUFFIX));

    is_lock && !scratch_dir_of(path).is_some_and(|dir| dir.exists())
}

/// The scratch directory a lock file belongs to: its path minus the suffix.
fn scratch_dir_of(lock: &Path) -> Option<PathBuf> {
    let name = lock.file_name()?.to_str()?;
    Some(lock.with_file_name(name.strip_suffix(LOCK_SUFFIX)?))
}

fn is_scratch_dir(path: &Path) -> bool {
    path.is_dir()
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(DIR_PREFIX))
}

/// A directory is abandoned when its sibling lock is free, or absent.
///
/// Treating a missing lock as abandonment is only sound because `claim` takes
/// the lock *before* the directory is created and verifies it still holds the
/// file it locked. A live owner therefore always has a lock beside its
/// directory, and no reaper can unlink it -- `is_abandoned` returns false
/// while it is held. A directory with no lock has no live owner.
fn is_abandoned(root: &Path) -> bool {
    let Ok(file) = File::open(lock_path(root)) else {
        return true;
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
