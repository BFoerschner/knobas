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
//! $TMPDIR/knobas-test-<pid>/           scratch: data/ and .pgpass
//! $TMPDIR/knobas-test-<pid>/.run       this run's nonce, written at claim
//! $TMPDIR/knobas-test-<pid>.lock       ownership lock, a *sibling* of it
//! ```
//!
//! The lock deliberately sits outside the directory the reaper deletes. Were
//! it inside, a reaper could unlink it out from under a claimant that had
//! created but not yet locked it; the claimant would then hold a lock on an
//! unlinked inode while its recreated directory looked ownerless, and the next
//! reaper would stop a live server.
//!
//! The nonce sits inside it, and is what makes "this directory is mine" a
//! matter of evidence rather than of its name: pids are recycled, so
//! `knobas-test-4242` may be the leftovers of a run that died an hour ago,
//! server and all. See [`take_over`].

use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sqlx::PgPool;

use crate::{DbConfig, EmbeddedDb};

/// Prefix for this crate's scratch directories under the system temp dir.
const DIR_PREFIX: &str = "knobas-test-";

/// Suffix turning a scratch directory path into its ownership lock path.
const LOCK_SUFFIX: &str = ".lock";

/// Name of the per-run marker inside a scratch directory.
const NONCE_FILE: &str = ".run";

/// How long a claimant waits for a lock somebody else holds before giving up.
/// Contention is a reaper's sweep, which is a handful of `stat`s and at most
/// one `pg_ctl stop`; anything past this is not contention but a bug.
const CLAIM_TIMEOUT: Duration = Duration::from_secs(30);

/// Longest pause between attempts while waiting for a contended lock.
const CLAIM_BACKOFF_CAP: Duration = Duration::from_millis(100);

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
    test_connector()
        .await
        .pool(TEST_POOL_SIZE)
        .await
        .expect("connect a test pool to the shared embedded postgres")
}

/// How to reach this test binary's shared database, for a test that needs
/// connections rather than a pool -- the scheduler's runs hold one each
/// (interfaces §10.6(c)).
///
/// # Panics
///
/// Panics if the database cannot be started.
pub async fn test_connector() -> crate::embedded::Connector {
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
    db.connector()
}

/// Connections a test pool gets. The same five the application pool has, so a
/// test that exhausts one is exhausting what the app would.
const TEST_POOL_SIZE: u32 = 5;

/// The ownership lock path for a scratch directory: a sibling, never a child.
fn lock_path(root_dir: &Path) -> PathBuf {
    let mut path = root_dir.as_os_str().to_os_string();
    path.push(LOCK_SUFFIX);
    PathBuf::from(path)
}

/// This run's nonce: what distinguishes *our* scratch directory from one a
/// previous process with the same pid left at the same path.
///
/// Pid plus the nanosecond it was first asked for. No randomness is needed for
/// the property that matters: a recycled pid necessarily belongs to a process
/// that started later, so its nonce differs whatever the clock does.
fn run_nonce() -> &'static str {
    static NONCE: OnceLock<String> = OnceLock::new();
    NONCE.get_or_init(|| {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        format!("{}-{nanos}", std::process::id())
    })
}

/// Whether `root` is a scratch directory *this* run stamped.
fn is_ours(root: &Path) -> bool {
    std::fs::read_to_string(root.join(NONCE_FILE)).is_ok_and(|nonce| nonce == run_nonce())
}

/// Take the ownership lock for our own scratch directory, hold it for the
/// lifetime of the process, and make the directory this run's.
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
        lock_with_backoff(&file, &path);
        if still_current(&file, &path) {
            break file;
        }
        // A reaper unlinked the file between `create` and `try_lock`; the lock
        // we hold protects an inode nobody can see. Start over.
    };
    let _ = HELD.set(file);
    take_over(root_dir);
}

/// Take the OS lock on `file`, waiting for whoever holds it.
///
/// Contention here is legitimate and short: a reaper -- in another test binary
/// cargo is running in parallel, or in this one -- opens and locks every
/// candidate lock file it sweeps, and a directory being claimed right now is a
/// candidate until its lock is taken. Treating the first `WouldBlock` as fatal,
/// which is what this used to do, turns somebody else's routine sweep into a
/// failed test run.
///
/// # Panics
///
/// If the lock cannot be taken within [`CLAIM_TIMEOUT`], or if locking fails
/// for any reason other than contention. Both mean no database for this
/// binary, which no test can proceed without.
fn lock_with_backoff(file: &File, path: &Path) {
    let deadline = Instant::now() + CLAIM_TIMEOUT;
    let mut pause = Duration::from_millis(1);
    loop {
        match file.try_lock() {
            Ok(()) => return,
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(source)) => {
                panic!("claim owner lock {}: {source}", path.display())
            }
        }
        assert!(
            Instant::now() < deadline,
            "owner lock {} is still held after {CLAIM_TIMEOUT:?}",
            path.display()
        );
        std::thread::sleep(pause);
        pause = (pause * 2).min(CLAIM_BACKOFF_CAP);
    }
}

/// Make `root_dir` this run's, discarding whatever a previous run left there,
/// and stamp it with [`run_nonce`].
///
/// Called with the ownership lock already held, which is what makes the
/// discard safe: the lock was free, so the process that wrote those files is
/// gone for certain.
///
/// Pids are recycled, and the scratch path is built from the pid alone, so
/// `$TMPDIR/knobas-test-4242` may be a *different* binary's leftovers -- one
/// that was killed with its server still running. That server is still alive
/// and still serving those files, so `EmbeddedDb::start` would happily adopt
/// it, and this run's tests would then share a database with whatever state
/// the dead run left in it, on a data directory no sweep will ever remove
/// (it is, after all, "ours"). The nonce is what tells the two apart: only a
/// directory this process stamped is this process's.
fn take_over(root_dir: &Path) {
    if root_dir.exists() && !is_ours(root_dir) {
        stop_server(root_dir);
        let _ = std::fs::remove_dir_all(root_dir);
    }
    let _ = std::fs::create_dir_all(root_dir);
    let _ = std::fs::write(root_dir.join(NONCE_FILE), run_nonce());
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
///
/// Our own directory is skipped on the nonce [`claim`] stamped into it, not on
/// its path: a directory that merely *sits at* our path may be a recycled
/// pid's leftovers, and [`take_over`] has already dealt with those.
fn reap_abandoned(own_root: &Path) {
    let Some(parent) = own_root.parent() else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !is_scratch_dir(&path) || is_ours(&path) {
            continue;
        }
        // Held across the whole removal, and dropped only afterwards.
        let Some(_lock) = claim_abandoned(&path) else {
            continue;
        };
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

/// Take the ownership lock of a directory that looks abandoned, so it can be
/// removed under it.
///
/// `Some(file)` means the lock is now **ours and held**; the caller must keep
/// the file alive until the directory and the lock itself are gone. Releasing
/// it any earlier reopens the window the lock exists to close: a fresh
/// claimant takes the lock, creates its directory and starts its server, and
/// this reaper -- already past its check -- then stops that server and deletes
/// its files.
///
/// A directory is abandoned when its sibling lock is free, or absent. Treating
/// a missing lock as abandonment is only sound because [`claim`] takes the lock
/// *before* the directory is created and verifies it still holds the file it
/// locked; the missing lock is created here so that even that case is done
/// under a held lock. A live owner therefore always has a lock beside its
/// directory, and no reaper can unlink it -- this returns `None` while it is
/// held.
fn claim_abandoned(root: &Path) -> Option<File> {
    let path = lock_path(root);
    let file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .ok()?;
    file.try_lock().ok()?;
    // Somebody unlinked the lock while we were taking it, so what we hold
    // guards an inode nobody else can reach. Leave the directory to the next
    // sweep rather than deleting it without a lock.
    still_current(&file, &path).then_some(file)
}

/// Ask an abandoned server to shut down before its files are removed.
///
/// The binaries live in the shared installation directory, not under the
/// scratch directory being reaped.
fn stop_server(root: &Path) {
    if !root.join("data").join("postmaster.pid").exists() {
        return;
    }
    let Some(pg_ctl) = crate::embedded::find_tool(&crate::embedded::installation_dir(), PG_CTL)
    else {
        return;
    };
    let _ = std::process::Command::new(pg_ctl)
        .arg("stop")
        .arg("-D")
        .arg(root.join("data"))
        .args(["-m", "immediate", "-w"])
        .output();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch directory under `parent`, stamped with `nonce` if given.
    ///
    /// The names only need the prefix: the sweep keys on that, and building
    /// them out of a real pid would make one test's fixtures another run's
    /// business.
    fn scratch(parent: &Path, name: &str, nonce: Option<&str>) -> PathBuf {
        let root = parent.join(format!("{DIR_PREFIX}{name}"));
        std::fs::create_dir_all(&root).unwrap();
        if let Some(nonce) = nonce {
            std::fs::write(root.join(NONCE_FILE), nonce).unwrap();
        }
        root
    }

    /// A locked ownership lock beside `root`, as a live owner has.
    fn hold_lock(root: &Path) -> File {
        let file = File::create(lock_path(root)).unwrap();
        file.try_lock().unwrap();
        file
    }

    #[test]
    fn the_nonce_identifies_this_run_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_ours(&scratch(dir.path(), "unstamped", None)));
        assert!(!is_ours(&scratch(dir.path(), "stranger", Some("4242-1"))));
        assert!(is_ours(&scratch(dir.path(), "ours", Some(run_nonce()))));
    }

    /// The reaper's two halves: a directory whose lock is free is swept, and
    /// one whose lock is held by a live owner is left strictly alone.
    #[test]
    fn a_sweep_removes_the_abandoned_and_spares_the_owned() {
        let dir = tempfile::tempdir().unwrap();
        let own_root = scratch(dir.path(), "own", Some(run_nonce()));

        let abandoned = scratch(dir.path(), "abandoned", Some("4242-1"));
        let unlocked_ever = scratch(dir.path(), "no-lock", None);
        let owned = scratch(dir.path(), "owned", Some("4243-1"));
        let held = hold_lock(&owned);

        reap_abandoned(&own_root);

        assert!(!abandoned.exists(), "a free lock means nobody is using it");
        assert!(
            !unlocked_ever.exists(),
            "a directory with no lock has no owner"
        );
        assert!(owned.exists(), "a held lock means a live test binary");
        assert!(own_root.exists(), "this run's own directory is not swept");
        assert!(
            !lock_path(&abandoned).exists(),
            "the reaped directory's lock goes with it"
        );
        drop(held);
    }

    /// The ownership lock is held *through* the removal, not merely consulted
    /// before it. A reaper that let go first would delete the files of a
    /// claimant that took the lock in between.
    #[test]
    fn the_reaper_holds_the_lock_it_took_until_the_directory_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        let root = scratch(dir.path(), "candidate", Some("4242-1"));
        let _lock_file = File::create(lock_path(&root)).unwrap();

        let held = claim_abandoned(&root).expect("a free lock is claimable");

        let contender = File::open(lock_path(&root)).unwrap();
        assert!(
            matches!(contender.try_lock(), Err(TryLockError::WouldBlock)),
            "the reaper must still hold the lock while it deletes"
        );
        drop(held);
        // ...and released once it is done. Waited for rather than asserted
        // outright: on macOS the release a `close` performs is not always
        // visible to another descriptor in this process on the very next
        // attempt, which is the same spurious `WouldBlock` `lock_with_backoff`
        // exists to absorb.
        lock_with_backoff(&contender, &lock_path(&root));
    }

    #[test]
    fn a_directory_a_live_owner_holds_is_not_claimable() {
        let dir = tempfile::tempdir().unwrap();
        let root = scratch(dir.path(), "live", Some("4242-1"));
        let held = hold_lock(&root);

        assert!(claim_abandoned(&root).is_none());
        drop(held);
    }

    /// Same path, different run: a pid that came back around must not inherit
    /// the previous holder's data directory (nor, through it, its server).
    #[test]
    fn a_claim_discards_what_a_recycled_pid_left_behind() {
        let dir = tempfile::tempdir().unwrap();
        let root = scratch(dir.path(), "recycled", Some("4242-1"));
        let leftover = root.join("data");
        std::fs::create_dir_all(&leftover).unwrap();

        take_over(&root);

        assert!(
            !leftover.exists(),
            "the previous run's data directory survived"
        );
        assert!(
            is_ours(&root),
            "the directory must be stamped as this run's"
        );
    }

    /// ...and a directory this run already stamped is left exactly as it is.
    #[test]
    fn a_claim_leaves_this_runs_own_directory_alone() {
        let dir = tempfile::tempdir().unwrap();
        let root = scratch(dir.path(), "ours", Some(run_nonce()));
        let data = root.join("data");
        std::fs::create_dir_all(&data).unwrap();

        take_over(&root);

        assert!(
            data.exists(),
            "take_over must not wipe its own live directory"
        );
    }

    /// Contention is not failure: a sweep holding the lock for a moment must
    /// make a claimant wait, not abort the test run.
    #[test]
    fn a_contended_lock_is_waited_out_rather_than_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("contended.lock");
        let sweeper = File::create(&path).unwrap();
        sweeper.try_lock().unwrap();

        let released = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            drop(sweeper);
        });

        let claimant = File::options().read(true).write(true).open(&path).unwrap();
        // Panics if the wait is not honoured -- which is what the old
        // `try_lock().expect(...)` did on the first attempt.
        lock_with_backoff(&claimant, &path);
        released.join().unwrap();
    }
}
