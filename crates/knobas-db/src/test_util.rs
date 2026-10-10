//! One database per calling source file, shared by every test in that file --
//! on a server of the binary's own, or on the one server a `just test` starts
//! for the whole run.
//!
//! Enabled by the `test-util` feature (not `cfg(test)` -- that is not set for
//! a crate's own `tests/` directory, nor for downstream crates).
//!
//! Every caller in one file gets the *same* database, so tests must isolate
//! themselves with unique keys. Truncating shared tables would break tests
//! running concurrently in the same file. A caller in another file -- in the
//! same binary or not -- gets another database: the key is the file
//! `#[track_caller]` reports, so a file merged into a crate's one test binary
//! (ADR-0017) keeps the isolation it had as a binary of its own.
//!
//! # Two ways to a server
//!
//! The *server* is decided once per process; each file's database on it is a
//! `create database` and a migration, named after this run's nonce.
//!
//! With [`GATE_URL_VAR`] (`KNOBAS_TEST_DB_URL`) unset -- `cargo test -p` by
//! hand -- the binary runs `initdb` and starts a postmaster in its scratch
//! root. That is the zero-config path, and it is what everything below the
//! layout section describes.
//!
//! With it set, the binary goes to the server the URL names instead, in
//! place of a whole bring-up. `just test` sets it, from the server the
//! `knobas-test-server` binary ([`serve_until_closed`]) starts once per run;
//! sixty-odd binaries each starting a postmaster of their own was most of the
//! gate's wall-clock and, when two gates overlapped, more SysV shared-memory
//! segments than macOS hands out (`shmget: No space left on device`). The
//! server is reached through `existing_url`, so it is never owned here and
//! nothing a binary does can stop it; the whole server goes when the recipe
//! closes the entry point's stdin, databases and all.
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

use std::collections::HashMap;
use std::fs::{File, TryLockError};
use std::io::{Read, Write};
use std::panic::Location;
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sqlx::{Connection, PgPool};

use crate::{DbConfig, EmbeddedDb};

/// Prefix for this crate's scratch directories under the system temp dir.
const DIR_PREFIX: &str = "knobas-test-";

/// The environment variable naming a server every test binary of a run
/// shares, which `just test` exports from the server it starts. Unset, each
/// binary starts a server of its own.
pub const GATE_URL_VAR: &str = "KNOBAS_TEST_DB_URL";

/// Suffix turning a scratch directory path into its ownership lock path.
const LOCK_SUFFIX: &str = ".lock";

/// Name of the per-run marker inside a scratch directory.
const NONCE_FILE: &str = ".run";

/// How long a claimant waits for a lock somebody else holds before giving up.
/// Contention is a reaper's sweep, which is a handful of `stat`s and at most
/// one `pg_ctl stop`, or -- for a pid recycled within seconds of a gate's
/// exit -- the `rm` still unlinking that gate's root under its lock (see
/// [`remove_root`]: about 10 s for 4 GB); anything past this is not contention
/// but a bug.
const CLAIM_TIMEOUT: Duration = Duration::from_secs(30);

/// Longest pause between attempts while waiting for a contended lock.
const CLAIM_BACKOFF_CAP: Duration = Duration::from_millis(100);

/// A pool onto the calling source file's shared database, starting the server
/// on first use.
///
/// The returned pool belongs to the calling runtime and should be dropped with
/// it -- see the module docs for why it is not shared.
///
/// A plain function returning the future rather than an `async fn`, because
/// `#[track_caller]` does nothing on an `async fn` and the caller's file is
/// what picks the database.
///
/// # Panics
///
/// Panics if the database cannot be started, or if the pool cannot connect --
/// there is no useful way for a test to continue without one.
#[track_caller]
pub fn test_pool() -> impl Future<Output = PgPool> + Send + 'static {
    let file = Location::caller().file();
    async move {
        database_of_file(file)
            .await
            .pool(TEST_POOL_SIZE)
            .await
            .expect("connect a test pool to the shared embedded postgres")
    }
}

/// How to reach the calling source file's shared database, for a test that
/// needs connections rather than a pool -- the scheduler's runs hold one each
/// (interfaces §10.6(c)). Within one file it is [`test_pool`]'s database.
///
/// # Panics
///
/// Panics if the database cannot be started.
#[track_caller]
pub fn test_connector() -> impl Future<Output = crate::embedded::Connector> + Send + 'static {
    let file = Location::caller().file();
    database_of_file(file)
}

/// The database of the source file `file`, created and migrated on its first
/// call and the same one on every call after.
///
/// Keyed by the file rather than the process: a test file merged into a
/// crate's one test binary (ADR-0017) is then exactly as isolated as it was as
/// a binary of its own, without its call sites changing.
async fn database_of_file(file: &'static str) -> crate::embedded::Connector {
    static FILES: OnceLock<Mutex<HashMap<&'static str, Arc<FileDatabase>>>> = OnceLock::new();

    let entry = {
        let mut files = FILES
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // Taken before `entry` borrows the map; used only if `file` is new.
        let ordinal = files.len();
        Arc::clone(files.entry(file).or_insert_with(|| {
            Arc::new(FileDatabase {
                ordinal,
                connector: tokio::sync::OnceCell::new(),
            })
        }))
    };

    entry
        .connector
        .get_or_init(|| async {
            // An ordinal rather than the file's name: PostgreSQL silently
            // truncates a name past 63 bytes, and two long paths could then
            // meet on one database.
            let name = format!(
                "knobas_test_{}_{}",
                identifier_slug(run_nonce()),
                entry.ordinal
            );
            create_migrated_database(&shared_server().await.connector, &name).await
        })
        .await
        .clone()
}

/// One source file's database, created on first use.
struct FileDatabase {
    /// The order this file first asked in, which names its database.
    ordinal: usize,
    connector: tokio::sync::OnceCell<crate::embedded::Connector>,
}

/// This binary's server, decided once per process.
async fn shared_server() -> &'static Server {
    static SERVER: tokio::sync::OnceCell<Server> = tokio::sync::OnceCell::const_new();

    SERVER
        .get_or_init(|| async {
            match std::env::var(GATE_URL_VAR) {
                Ok(url) => Server::of_the_gate(&url).await,
                Err(_) => Server::of_its_own().await,
            }
        })
        .await
}

/// The server this binary's databases live on.
struct Server {
    /// Reaches the server; which database it names does not matter, since
    /// every use goes through [`create_migrated_database`], which moves it.
    connector: crate::embedded::Connector,
    /// The server this binary started, kept here so its `Drop` -- which would
    /// stop the server -- never runs; statics are never dropped. `None` when
    /// the server is the gate's, which is nobody's to keep alive from here.
    _own_server: Option<EmbeddedDb>,
}

impl Server {
    /// The zero-config path: a server of this binary's own in its scratch
    /// root.
    async fn of_its_own() -> Server {
        let root_dir = scratch_root();
        claim(&root_dir);
        reap_abandoned(&root_dir);
        let db = EmbeddedDb::start(DbConfig {
            root_dir,
            existing_url: None,
        })
        .await
        .expect("start embedded postgres for tests");
        Server {
            connector: db.connector(),
            _own_server: Some(db),
        }
    }

    /// The gate path: the server `url` names is somebody else's -- reached
    /// through `existing_url`, so it is never owned and never stopped from
    /// here.
    ///
    /// No claim and no sweep: this binary has no scratch root, and the gate's
    /// server did the sweeping when it started.
    async fn of_the_gate(url: &str) -> Server {
        let gate = EmbeddedDb::start(DbConfig {
            // Ignored on the `existing_url` branch; there is no root to name.
            root_dir: PathBuf::new(),
            existing_url: Some(url.to_owned()),
        })
        .await
        .unwrap_or_else(|error| panic!("connect to the gate's server at {GATE_URL_VAR}: {error}"));
        let connector = gate.connector();
        // The handle's own pool is the only thing `stop` closes for a server
        // it does not own, and nothing here uses that pool.
        gate.stop()
            .await
            .expect("close the handle on the gate's server");
        Server {
            connector,
            _own_server: None,
        }
    }
}

/// A database of its own on this binary's shared server, migrated and empty.
///
/// The file's shared database cannot be used by a test that *replaces* its
/// contents: every other test in the file is in it, and nothing here
/// truncates. A restore is exactly such a test -- so it gets its own database
/// instead, which costs one `create database` rather than a second postmaster.
///
/// The name carries `label` plus a per-call nonce, so two calls (and two runs
/// against a server that outlived one) never collide. Nothing drops it: the
/// whole scratch directory goes when the run's lock is released, and a
/// `drop database` in a test that failed would run only when it did not need
/// to.
///
/// # Panics
///
/// Panics if the database cannot be created, connected to, or migrated --
/// there is no useful way for a test to continue without one.
pub async fn scratch_database(label: &str) -> crate::embedded::Connector {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    let name = format!(
        "knobas_scratch_{}_{}_{}",
        identifier_slug(label),
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );

    create_migrated_database(&shared_server().await.connector, &name).await
}

/// `text` reduced to what may sit inside a database name without quoting:
/// ASCII alphanumerics, everything else an underscore.
fn identifier_slug(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// `create database` on `server`'s server, then the schema, then a connector
/// onto it.
///
/// `name` is built by the two callers from an alphanumeric slug, a pid and a
/// counter or a nonce, so there is no caller-supplied text in it -- `create
/// database` takes no parameter.
///
/// # Panics
///
/// Panics if the database cannot be created, connected to, or migrated.
async fn create_migrated_database(
    server: &crate::embedded::Connector,
    name: &str,
) -> crate::embedded::Connector {
    let mut admin = server
        .with_database(crate::embedded::MAINTENANCE_DATABASE)
        .connect()
        .await
        .expect("connect to the maintenance database");
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("create database {name}")))
        .execute(&mut admin)
        .await
        .expect("create a scratch database");
    let _ = admin.close().await;

    let created = server.with_database(name);
    let pool = created
        .pool(2)
        .await
        .expect("connect to the scratch database");
    crate::migrate::run(&pool)
        .await
        .expect("migrate the scratch database");
    pool.close().await;
    created
}

/// Connections a test pool gets. The same five the application pool has, so a
/// test that exhausts one is exhausting what the app would.
const TEST_POOL_SIZE: u32 = 5;

/// `max_connections` for the server `just test` starts.
///
/// Formula: runner parallelism × per-binary peak, plus the scheduler suite's
/// one connection per run outside its pool, plus PostgreSQL's reserved
/// superuser slots.
///
/// * Runner parallelism: **12**, the core count of the 12-core machine the
///   gate runs on. The recipe is serial today; #415 will run binaries through
///   `xargs -P N` and pick `N`, and this is sized so that choice needs no
///   change here up to the core count.
/// * Per-binary peak: **29** client backends, measured (2026-09-05) by
///   sampling `pg_stat_activity` every 0.25 s through a serial `just test`,
///   where at most one binary is on the server at a time. The ceiling is
///   higher -- every `#[tokio::test]` opens its own pool of [`TEST_POOL_SIZE`]
///   and libtest runs up to one test per core, so 60 -- but the tests hold
///   one or two connections each, not five. Budgeted as 30.
/// * The scheduler suite: one connection per run on top of its pool, budgeted
///   at one per test thread, **12**.
/// * `superuser_reserved_connections`: **3**.
///
/// 12 × 30 + 12 + 3 = 375, rounded up to **400**. Each unused slot costs a
/// few kilobytes of shared memory and nothing else; running out costs a
/// `FATAL: sorry, too many clients already` in whichever binary asked last.
const GATE_MAX_CONNECTIONS: u32 = 400;

/// This process's scratch root: `$TMPDIR/knobas-test-<pid>`.
fn scratch_root() -> PathBuf {
    std::env::temp_dir().join(format!("{DIR_PREFIX}{}", std::process::id()))
}

/// What the `knobas-test-server` binary runs: one server in this process's
/// scratch root, under the same claim-and-reap scheme a test binary's own
/// server lives under, for as long as `input` stays open.
///
/// The protocol is the smallest one a shell can drive: the maintenance
/// database's URL on `output`, one line, then nothing; end-of-file on `input`
/// (or `SIGINT`/`SIGTERM`) is the signal to stop. The server is then
/// [discarded](EmbeddedDb::discard) rather than stopped the way a clean quit
/// of the app stops one -- its data directory is about to go -- and the
/// scratch root is removed by a child process that outlives this one (see
/// [`remove_root`]), so the recipe gets its exit status as soon as the
/// postmaster is gone and a gate that ends normally leaves nothing for the
/// next run's reaper. A gate killed with `SIGKILL` does leave the postmaster
/// behind, and that is what the reaper is for.
///
/// `input` is read on a plain thread rather than `spawn_blocking`: a runtime
/// shutting down waits for its blocking tasks, and a thread parked in `read`
/// on a pipe nobody will close (a signal arrived first) would hold the exit.
///
/// # Errors
///
/// [`DbError`] if the server cannot be started or stopped, or the URL cannot
/// be written.
pub async fn serve_until_closed(
    mut input: impl Read + Send + 'static,
    mut output: impl Write,
) -> Result<(), crate::DbError> {
    let root_dir = scratch_root();
    let lock = claim(&root_dir);
    reap_abandoned(&root_dir);
    let db = EmbeddedDb::start_with_max_connections(root_dir.clone(), GATE_MAX_CONNECTIONS).await?;

    let url = db
        .connector()
        .with_database(crate::embedded::MAINTENANCE_DATABASE)
        .url();
    let announce = writeln!(output, "{url}").and_then(|()| output.flush());
    if let Err(source) = announce {
        // Nobody is listening, so nobody will close the pipe: stop now
        // rather than serve for ever.
        db.discard().await?;
        remove_root(&root_dir, lock);
        return Err(crate::DbError::Io {
            path: PathBuf::from("<stdout>"),
            source,
        });
    }

    let (closed_tx, closed_rx) = tokio::sync::oneshot::channel::<()>();
    std::thread::spawn(move || {
        let _ = std::io::copy(&mut input, &mut std::io::sink());
        let _ = closed_tx.send(());
    });
    tokio::select! {
        _ = closed_rx => {}
        _ = tokio::signal::ctrl_c() => {}
        () = terminated() => {}
    }

    db.discard().await?;
    remove_root(&root_dir, lock);
    Ok(())
}

/// Remove a scratch root and the lock beside it -- in the background, where
/// a child process can do it.
///
/// After a full `just test` the gate server's root holds one database per
/// test source file plus every `scratch_database` -- at one per binary, when
/// #421 measured it, 355 databases, 4 GB, and about 10 s of unlinking, a fifth
/// of the whole run spent after the last test had reported. Nothing reads the
/// files again, so on Unix the unlinking is handed to an `rm -rf` and this
/// process exits at once.
///
/// The child keeps the root's ownership lock while it works: `lock` is the
/// held lock file, handed to it as its stdin, and a `flock` lock belongs to
/// the open file description, which a child inherits and keeps for as long as
/// it lives -- however the parent goes. That is what keeps the claim/reap
/// invariants standing over a root that is half gone. A reaper sweeping in
/// the meantime finds it owned and skips it; without the lock it would take
/// the root as abandoned and unlink the same tree itself, harmlessly, but
/// synchronously, so the next server's start would wait for it. A claimant
/// handed the same pid waits on the lock instead of taking over a directory
/// `rm` is still inside; its wait is bounded by [`CLAIM_TIMEOUT`] (30 s),
/// against an unlinking that took 10 s for 4 GB on an idle machine. Should
/// the child be killed, the lock dies with it and the next sweep finishes the
/// job, as for any abandoned root.
///
/// Where no child can be spawned the removal happens here, as it used to.
fn remove_root(root: &Path, lock: &File) {
    if remove_root_in_background(root, lock).is_err() {
        remove_root_in_place(root);
    }
}

/// Remove a scratch root and the lock beside it, here and now.
fn remove_root_in_place(root: &Path) {
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_file(lock_path(root));
}

/// `rm -rf <root> <root>.lock`, holding `lock` until both are gone.
///
/// The lock file is `rm`'s own stdin and its last argument: the tree goes
/// first, then the lock, and the descriptor closes with the process -- so the
/// lock is released only once there is nothing left to protect. The
/// ownership lock the reaper checks is this same file, so anything that
/// looks at the root while `rm` runs sees an owner.
#[cfg(unix)]
fn remove_root_in_background(root: &Path, lock: &File) -> std::io::Result<()> {
    let mut rm = Command::new("rm");
    rm.arg("-rf").arg(root).arg(lock_path(root));
    // The child is not waited for: it is meant to outlive this process.
    spawn_holding(rm, lock).map(drop)
}

/// No `rm`, and `CreateProcess` does not carry a `flock`: remove in place.
#[cfg(not(unix))]
fn remove_root_in_background(_root: &Path, _lock: &File) -> std::io::Result<()> {
    Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
}

/// Spawn `command` holding `lock` for as long as it runs.
///
/// The child's stdin is a duplicate of the lock file, so the open file
/// description -- and the `flock` on it -- stays open until the child exits,
/// whatever happens to this process's own handle. Its stdout and stderr go
/// nowhere: a child that outlives this process must not keep the pipes this
/// process was given open, or whoever reads them waits for the child too.
#[cfg(unix)]
fn spawn_holding(mut command: Command, lock: &File) -> std::io::Result<Child> {
    let held = lock.try_clone()?;
    command
        .stdin(Stdio::from(held))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

/// Resolves when `SIGTERM` arrives; never, where there is no such signal.
#[cfg(unix)]
async fn terminated() {
    use tokio::signal::unix::{SignalKind, signal};
    match signal(SignalKind::terminate()) {
        Ok(mut term) => {
            term.recv().await;
        }
        Err(_) => std::future::pending().await,
    }
}

#[cfg(not(unix))]
async fn terminated() {
    std::future::pending().await
}

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
/// including on panic, `SIGKILL`, or a test-harness timeout. It is returned
/// as well, for the one caller that hands the lock on rather than letting it
/// lapse: [`remove_root`].
///
/// Taken *before* the scratch directory exists, so no reaper can ever observe
/// that directory without an owner.
fn claim(root_dir: &Path) -> &'static File {
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
    let held = HELD.get_or_init(move || file);
    take_over(root_dir);
    held
}

/// Take the OS lock on `file`, waiting for whoever holds it.
///
/// Contention here is legitimate and short: a reaper -- in another test binary
/// cargo is running in parallel, or in this one -- opens and locks every
/// candidate lock file it sweeps, and a directory being claimed right now is a
/// candidate until its lock is taken. The one longer holder is the `rm` a gate
/// left unlinking its root ([`remove_root`]), met only by a claimant whose pid
/// is that gate's, recycled within seconds. Treating the first `WouldBlock` as
/// fatal, which is what this used to do, turns somebody else's routine sweep
/// into a failed test run.
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
        remove_root_in_place(&path);
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
/// scratch directory being reaped; `pg_ctl_stop_immediate` knows where.
fn stop_server(root: &Path) {
    let data_dir = root.join("data");
    if !data_dir.join("postmaster.pid").exists() {
        return;
    }
    if let Some(mut pg_ctl) = crate::embedded::pg_ctl_stop_immediate(&data_dir) {
        let _ = pg_ctl.output();
    }
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

    /// A child spawned holding a lock keeps it for exactly as long as it
    /// lives, with the parent's own handle already gone -- the property the
    /// background removal rests on. While the child is up the root is owned,
    /// so a reaper's claim fails; once it is gone the lock is free again.
    #[cfg(unix)]
    #[test]
    fn a_child_spawned_holding_a_lock_keeps_it_until_it_exits() {
        let dir = tempfile::tempdir().unwrap();
        let root = scratch(dir.path(), "handed-on", Some("4242-1"));
        let lock = hold_lock(&root);

        let mut sleeper = Command::new("sleep");
        sleeper.arg("30");
        let mut child = spawn_holding(sleeper, &lock).expect("spawn a child holding the lock");
        drop(lock);

        // Held for as long as the child lives -- asked repeatedly rather
        // than once, because a single failed attempt right after a `close`
        // proves nothing on macOS (see the reaper test below), while a lock
        // nobody holds is free within microseconds of the close: half a
        // second of refusals is the child's doing.
        let until = Instant::now() + Duration::from_millis(500);
        while Instant::now() < until {
            assert!(
                claim_abandoned(&root).is_none(),
                "the child holds the lock, so the root must not read as abandoned"
            );
            std::thread::sleep(Duration::from_millis(20));
        }

        child.kill().unwrap();
        child.wait().unwrap();
        // Free again once the child is gone; waited for rather than asserted
        // outright, for the same macOS release lag
        // `the_reaper_holds_the_lock_it_took_until_the_directory_is_gone`
        // absorbs.
        let contender = File::open(lock_path(&root)).unwrap();
        lock_with_backoff(&contender, &lock_path(&root));
    }

    /// Removing a root leaves neither the directory nor the lock beside it --
    /// eventually, since the removal runs in the background. The lock file
    /// matters: one left behind with no directory is exactly what
    /// `sweep_orphan_locks` exists to clean up, and a clean exit should not
    /// be making work for it.
    #[test]
    fn a_removed_root_takes_its_lock_with_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = scratch(dir.path(), "removed", Some("4242-1"));
        // A tree rather than an empty directory, so the removal has to recurse.
        std::fs::create_dir_all(root.join("data").join("base")).unwrap();
        let lock = hold_lock(&root);

        remove_root(&root, &lock);
        drop(lock);

        let deadline = Instant::now() + Duration::from_secs(10);
        while (root.exists() || lock_path(&root).exists()) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            !root.exists(),
            "the root is still there: {}",
            root.display()
        );
        assert!(
            !lock_path(&root).exists(),
            "the lock outlived its root: {}",
            lock_path(&root).display()
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
