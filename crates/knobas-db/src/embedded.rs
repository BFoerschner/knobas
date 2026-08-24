//! Lifecycle for the embedded PostgreSQL server knobas ships with.
//!
//! State -- the data directory and the password file -- lives under a
//! caller-supplied `root_dir`. The binaries do not: they are installed once
//! per machine and version into the shared `~/.theseus/postgresql/<version>`
//! (see the private `installation_dir` helper), so a `root_dir` holds only the user's data.
//!
//! The server always speaks TCP on `127.0.0.1`: a Unix socket path under
//! `~/Library/Application Support/...` would blow past macOS' 103-byte
//! `sun_path` limit.

use std::fs::{File, TryLockError};
use std::io;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use postgresql_embedded::{PostgreSQL, Settings, VersionReq};
use sqlx::postgres::PgPoolOptions;
use sqlx::{Connection, PgConnection, PgPool};

/// The exact PostgreSQL version knobas runs against. Pinned rather than
/// ranged: PG 18 changed generated-column defaults, and the schema depends on
/// `GENERATED ALWAYS AS (...) STORED` behaving the way 18.6 does.
pub const PG_VERSION_REQ: &str = "=18.6.0";

/// Name of the application database inside the server.
pub const DATABASE_NAME: &str = "knobas";

/// The database `initdb` always leaves behind, used to ask a server about
/// itself and to create [`DATABASE_NAME`] when it is missing. Connecting to
/// [`DATABASE_NAME`] cannot do either job -- on a server that has not got it
/// yet, the connection is what fails.
const MAINTENANCE_DATABASE: &str = "postgres";

/// Loopback host. Never a Unix socket -- see the module docs.
const HOST: &str = "127.0.0.1";

/// Pool size. The desktop app is a single user with a handful of concurrent
/// queries; a larger pool only buys idle backends.
const MAX_CONNECTIONS: u32 = 5;

/// Upper bound for `initdb` / `pg_ctl` invocations. The crate default is 5s,
/// which a cold `initdb` on a slow disk can exceed.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(60);

/// How long to wait for a TCP handshake when deciding whether a recorded
/// `postmaster.pid` still has a live server behind it.
const LIVENESS_TIMEOUT: Duration = Duration::from_millis(500);

/// Where the database lives and how to reach it.
#[derive(Clone, Debug)]
pub struct DbConfig {
    /// Directory owning this instance's state: the `data/` directory and the
    /// `.pgpass` password file. The PostgreSQL binaries are *not* stored here
    /// -- they are shared per machine, so this only needs room for the user's
    /// data, and deleting it resets the database without forcing a
    /// re-download. Ignored when `existing_url` is set.
    pub root_dir: PathBuf,
    /// Connect to an already-running PostgreSQL instead of managing one.
    pub existing_url: Option<String>,
}

/// Everything that can go wrong bringing the database up.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("embedded postgres: {0}")]
    Embedded(#[from] postgresql_embedded::Error),

    #[error("postgres connection: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("schema migration: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),

    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    /// A live PostgreSQL already serves this data directory and we could not
    /// join it either.
    ///
    /// The usual cause is a second knobas window on the same profile. Note
    /// what this error does *not* do: it never stops that server. Anything
    /// that shuts a stranger's postmaster down risks taking a running
    /// instance's database with it.
    #[error("another knobas instance is using the database at {data_dir} ({reason})")]
    AlreadyRunning { data_dir: PathBuf, reason: String },
}

impl DbError {
    fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        DbError::Io {
            path: path.into(),
            source,
        }
    }
}

/// A running database and a pool connected to it.
///
/// When `DbConfig::existing_url` was set, or when an already-running server was
/// adopted, no server is owned; the handle is then just the pool and `stop`
/// only closes it.
pub struct EmbeddedDb {
    pool: PgPool,
    /// Connection URL of the running server, for callers that need a pool of
    /// their own rather than the shared one.
    ///
    /// Only [`test_util`](crate::test_util) ever asks for it, so the field
    /// exists only when that feature does. Keeping it unconditionally would
    /// mean an application build carries a string holding the superuser
    /// password that nothing in that build can read -- and, because the field
    /// is private and its one reader is feature-gated, a `dead_code` warning
    /// that only appears when the crate is compiled the way a consumer
    /// compiles it.
    #[cfg(feature = "test-util")]
    url: String,
    /// `None` when connected to a server we do not manage.
    postgresql: Option<PostgreSQL>,
}

impl EmbeddedDb {
    /// Bring the database up and return a connected pool.
    ///
    /// For a managed server this installs PostgreSQL into the shared,
    /// version-keyed `~/.theseus/postgresql/<version>` (a no-op once present,
    /// and shared with every other `root_dir` on the machine), runs `initdb` into `root_dir/data` the
    /// first time, starts the server on an ephemeral loopback port and creates
    /// the `knobas` database if it does not exist yet.
    ///
    /// A server that is already up and serving this very data directory -- one
    /// a signal-killed run left behind, or a second instance's -- is *adopted*
    /// rather than fought over: see [`adopt`].
    ///
    /// A `postmaster.pid` whose port turns out to be answered by some *other*
    /// server -- or, once the process it names is gone, by something that is
    /// not a PostgreSQL at all -- is proof that no postmaster holds this data
    /// directory: one that did would have recorded its own port. That lock is
    /// removed and the start is retried, rather than reported as a conflict
    /// that does not exist.
    ///
    /// The first `setup()` and `start()` for a `root_dir` run under an
    /// exclusive file lock in it, so two processes launching against one
    /// profile at the same time `initdb` it one after the other instead of on
    /// top of each other.
    ///
    /// # Errors
    ///
    /// Returns [`DbError`] if the install, `initdb`, start, database creation
    /// or the initial connection fails, or [`DbError::AlreadyRunning`] if a
    /// live server holds the data directory and cannot be joined.
    pub async fn start(cfg: DbConfig) -> Result<EmbeddedDb, DbError> {
        if let Some(url) = cfg.existing_url.as_deref() {
            tracing::info!("connecting to externally managed postgres");
            return Ok(EmbeddedDb {
                pool: connect(url).await?,
                #[cfg(feature = "test-util")]
                url: url.to_string(),
                postgresql: None,
            });
        }

        let settings = build_settings(&cfg.root_dir)?;
        let data_dir = settings.data_dir.clone();

        match start_managed(&cfg.root_dir, settings.clone()).await? {
            Started::Ready(db) => Ok(*db),
            Started::StaleLock { port, serving } => {
                // The lock file sent us to a stranger, so it is not describing
                // any live postmaster of ours. Ephemeral ports are recycled:
                // after a crash and a reboot, the port a dead server recorded
                // can belong to anything. Clear the lock and start normally --
                // the alternative, `AlreadyRunning`, would be both false and
                // unrecoverable, leaving the user to find and delete
                // `postmaster.pid` by hand before knobas would launch again.
                tracing::warn!(
                    port,
                    serving,
                    "postmaster.pid points at a server that is not ours: clearing the stale lock"
                );
                clear_lock(&data_dir)?;
                match start_managed(&cfg.root_dir, settings).await? {
                    Started::Ready(db) => Ok(*db),
                    // Only reachable if something recreated the lock file in
                    // between; at that point it is genuinely ambiguous.
                    Started::StaleLock { port, serving } => Err(DbError::AlreadyRunning {
                        data_dir,
                        reason: format!(
                            "postmaster.pid keeps naming port {port}, which is served by {serving}"
                        ),
                    }),
                }
            }
        }
    }

    /// The connection pool for the `knobas` database.
    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// The URL this handle is connected to.
    ///
    /// Crate-internal on purpose: it carries the superuser password, and the
    /// only reason to need it is opening a second pool -- a `PgPool` may not be
    /// shared across tokio runtimes, so anything outliving the runtime that
    /// built [`pool`](Self::pool) needs its own. `test_util` is the one caller,
    /// which is why this compiles only under that feature -- along with the
    /// field it reads.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub(crate) fn url(&self) -> &str {
        &self.url
    }

    /// Close the pool and shut the server down.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Embedded`] if `pg_ctl stop` fails.
    pub async fn stop(self) -> Result<(), DbError> {
        self.pool.close().await;
        if let Some(postgresql) = &self.postgresql {
            postgresql.stop().await?;
        }
        Ok(())
    }
}

/// What one attempt at bringing the managed server up produced.
enum Started {
    /// A pool on the right server -- one this attempt started, or one it
    /// adopted after confirming which data directory it serves.
    ///
    /// Boxed only to keep the two variants a similar size: an `EmbeddedDb` is
    /// two orders of magnitude larger than the other one's `(u16, String)`, and
    /// this enum is constructed at most twice per process.
    Ready(Box<EmbeddedDb>),
    /// `postmaster.pid` is provably stale: the port it records is answered by
    /// `serving` -- a PostgreSQL serving some other data directory, or (once
    /// the process the lock names is gone) something that does not speak
    /// PostgreSQL at all. Either way no postmaster of ours holds this
    /// directory, so clearing the lock and attempting the start again is the
    /// recovery.
    StaleLock { port: u16, serving: String },
}

/// One attempt at starting (or joining) the server for `settings`.
///
/// Factored out of [`EmbeddedDb::start`] because the stale-lock recovery has to
/// run the whole sequence again -- `setup`, `start`, database creation --
/// against a fresh `PostgreSQL` handle.
///
/// The whole attempt runs under [`bring_up_lock`], so two processes launching
/// against one `root_dir` for the first time cannot `initdb` it at the same
/// time. The guard is a local, so it is released whichever way this returns --
/// including into [`EmbeddedDb::start`]'s stale-lock retry, which takes it
/// again.
async fn start_managed(root_dir: &Path, settings: Settings) -> Result<Started, DbError> {
    let _guard = bring_up_lock(root_dir).await?;

    let data_dir = settings.data_dir.clone();
    // `PostgreSQL::new` consumes the settings, and the adoption path needs
    // them after the handle is gone.
    let adoption = settings.clone();
    let mut postgresql = PostgreSQL::new(settings);

    postgresql.setup().await?;

    if let Err(error) = postgresql.start().await {
        // `PostgreSQL::drop` runs `pg_ctl stop -m fast` whenever
        // `postmaster.pid` merely *exists* -- it has no idea whether this
        // handle is the one that started that server. So every arm below that
        // leaves a lock file standing forgets the handle rather than dropping
        // it: it owns no OS resource, only paths and strings, so the leak is a
        // few hundred bytes for the rest of the process -- a trade worth
        // making against shutting down a database somebody else is using.
        match inspect_lock(&data_dir)? {
            // A hard-killed process (crash, `kill -9`, laptop shutdown)
            // leaves `postmaster.pid` behind and PostgreSQL then refuses to
            // start. The lock is gone now, so the start is worth retrying.
            Lock::Cleared => {
                if let Err(retry) = postgresql.start().await {
                    // The lock file this handle would stop on is no longer
                    // the one it cleared: a sibling that lost the same race
                    // may have started its server in between, and `pg_ctl
                    // stop` against *that* pid file kills a database this
                    // process never started.
                    std::mem::forget(postgresql);
                    return Err(retry.into());
                }
            }
            // No lock at all: the start failed for some other reason, and
            // that reason is the one worth reporting.
            Lock::Absent => return Err(error.into()),
            Lock::Live { port, pid } => {
                std::mem::forget(postgresql);
                return adopt(adoption, port, pid).await;
            }
            Lock::LiveProcess { pid } => {
                std::mem::forget(postgresql);
                return Err(DbError::AlreadyRunning {
                    data_dir,
                    reason: format!(
                        "postmaster.pid records process {pid}, which is still running, but \
                         nothing answers on the port it recorded"
                    ),
                });
            }
        }
    }

    if !postgresql.database_exists(DATABASE_NAME).await? {
        postgresql.create_database(DATABASE_NAME).await?;
    }

    let url = postgresql.settings().url(DATABASE_NAME);
    let pool = connect(&url).await?;

    tracing::info!(port = postgresql.settings().port, "embedded postgres ready");

    Ok(Started::Ready(Box::new(EmbeddedDb {
        pool,
        #[cfg(feature = "test-util")]
        url,
        postgresql: Some(postgresql),
    })))
}

/// Connect to a server that is already serving our data directory, instead of
/// failing because it exists.
///
/// A knobas killed by a signal -- Ctrl-C under `just dev`, a `kill`, a crashed
/// debugger -- never runs its shutdown, so its PostgreSQL outlives it. The next
/// launch then finds a healthy server holding the data directory. Refusing to
/// start would strand the user until they hunt down a stray process; stopping
/// it would risk killing a *live* sibling's database, since nothing on disk
/// distinguishes the two. Adopting it does neither: the orphan becomes this
/// run's warm start, skipping `initdb` and the start entirely.
///
/// The adopted server is deliberately **not** owned -- `postgresql` stays
/// `None`, so [`EmbeddedDb::stop`] closes the pool and leaves the server up.
/// Two consequences worth knowing:
///
/// * A server adopted this way keeps running after knobas quits, and is
///   adopted again next launch. It is only ever stopped by the run that
///   actually started it.
/// * If the server belongs to a second live instance, both instances now share
///   it -- which PostgreSQL is entirely happy with -- but the instance that
///   started it will stop it on quit, and the adopter's pool dies with it.
///   Single-user desktop, M0: acceptable. Real multi-instance arbitration is
///   M1's.
///
/// # Errors
///
/// [`DbError::AlreadyRunning`] if the server answered as a PostgreSQL and then
/// could not be queried, or if the [`DATABASE_NAME`] database cannot be created
/// on it. Two outcomes are *not* errors, and both come back as
/// [`Started::StaleLock`] so the caller clears the lock and starts normally:
///
/// * a PostgreSQL that turns out to serve a **different** data directory, and
/// * a listener on the recorded port that does not speak PostgreSQL at all,
///   provided the process the lock file names is gone.
///
/// The second is the recycled-ephemeral-port case: some unrelated program now
/// owns the port a dead postmaster recorded. Reporting that as "another knobas
/// instance" is both false and unrecoverable -- the user has no way to make the
/// stranger release the port, and knobas would refuse to launch until they
/// found and deleted `postmaster.pid` by hand. The pid check is what keeps the
/// recovery honest: while the recorded process is still alive, the lock may
/// still be owned, and a non-PostgreSQL answer on its port is not enough to
/// condemn it.
async fn adopt(mut settings: Settings, port: u16, pid: Option<u32>) -> Result<Started, DbError> {
    let data_dir = settings.data_dir.clone();
    // The recorded port, not the 0 that `build_settings` asks a fresh start to
    // pick: this server chose its port long ago.
    settings.port = port;
    let url = settings.url(DATABASE_NAME);

    let unreachable = |reason: String| DbError::AlreadyRunning {
        data_dir: data_dir.clone(),
        reason,
    };

    // Through the maintenance database, not `knobas`: the identity check has to
    // happen before we trust the server, and creating `knobas` when it is
    // missing needs a connection that does not depend on it existing.
    let admin = PgConnection::connect(&settings.url(MAINTENANCE_DATABASE)).await;
    let mut admin = match admin {
        Ok(admin) => admin,
        Err(source) if is_wire_level(&source) && !recorded_pid_alive(pid) => {
            tracing::warn!(
                port,
                %source,
                "the recorded port is held by something that does not speak postgres, and the \
                 recorded process is gone: the lock is stale"
            );
            return Ok(Started::StaleLock {
                port,
                serving: format!("something that does not speak postgres ({source})"),
            });
        }
        Err(source) => {
            return Err(unreachable(format!(
                "cannot connect on port {port}: {source}"
            )));
        }
    };

    // Ask the server which directory it serves rather than trusting line 2 of
    // the lock file: a data directory that was copied elsewhere carries a
    // `postmaster.pid` naming a port some unrelated server may now hold, and
    // connecting to that would silently read the wrong database.
    let serving: (String,) = sqlx::query_as("show data_directory")
        .fetch_one(&mut admin)
        .await
        .map_err(|source| unreachable(format!("cannot query it: {source}")))?;
    if !same_dir(Path::new(&serving.0), &data_dir) {
        let _ = admin.close().await;
        return Ok(Started::StaleLock {
            port,
            serving: serving.0,
        });
    }

    // The same step the managed path performs, for the same reason: a first run
    // that died between `initdb` and `create database` leaves a server with no
    // `knobas` in it, and adopting it must finish the job rather than report an
    // unreachable database. `create database` has no `if not exists`, hence the
    // lookup, and `AssertSqlSafe` for the statement: a database name cannot be
    // a bind parameter, and the audit sqlx is asking for is that `DATABASE_NAME`
    // is a compile-time constant of this crate -- no input reaches the string.
    let existing: Option<(i32,)> = sqlx::query_as("select 1 from pg_database where datname = $1")
        .bind(DATABASE_NAME)
        .fetch_optional(&mut admin)
        .await
        .map_err(|source| unreachable(format!("cannot query it: {source}")))?;
    if existing.is_none() {
        tracing::warn!(
            port,
            "the adopted server has no {DATABASE_NAME} database: creating it"
        );
        sqlx::query(sqlx::AssertSqlSafe(format!(
            r#"create database "{DATABASE_NAME}""#
        )))
        .execute(&mut admin)
        .await
        .map_err(|source| {
            unreachable(format!(
                "cannot create the {DATABASE_NAME} database: {source}"
            ))
        })?;
    }
    let _ = admin.close().await;

    let pool = connect(&url).await.map_err(|source| {
        unreachable(format!(
            "cannot connect to {DATABASE_NAME} on port {port}: {source}"
        ))
    })?;

    tracing::warn!(
        port,
        "adopting the postgres already serving this data directory"
    );
    Ok(Started::Ready(Box::new(EmbeddedDb {
        pool,
        #[cfg(feature = "test-util")]
        url,
        postgresql: None,
    })))
}

/// Whether two paths name the same directory.
///
/// Canonicalised first: macOS hands out `/var/folders/...` paths that
/// PostgreSQL reports back as `/private/var/folders/...`, and a raw string
/// comparison would call an adopted server a stranger every time. Falls back to
/// the literal comparison when a path cannot be resolved.
fn same_dir(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

pub(crate) async fn connect(url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(MAX_CONNECTIONS)
        .connect(url)
        .await
}

/// `Settings::new()` with its litter cleaned up.
///
/// It eagerly materialises two throwaway temp directories for the `data_dir`
/// and `password_file` defaults, which every caller here overrides. Reclaim
/// them -- `remove_dir` only succeeds while they are still empty, which is
/// exactly the safety check we want.
fn fresh_settings() -> Settings {
    let settings = Settings::new();

    let _ = std::fs::remove_dir(&settings.data_dir);
    if let Some(dir) = settings.password_file.parent() {
        let _ = std::fs::remove_dir(dir);
    }

    settings
}

/// Where the PostgreSQL binaries live: the crate's shared, version-keyed
/// default (`~/.theseus/postgresql/<version>`), deliberately *not* under any
/// `root_dir`.
///
/// The binaries are immutable and version-pinned, so one copy per machine is
/// enough; putting them under each `root_dir` would re-download ~13 MB for
/// every fresh data directory and make the test suite need the network on
/// every run. The archive extractor takes its own cross-process lock and
/// renames into place atomically, so concurrent bootstraps are safe -- but it
/// skips extraction entirely if the target directory already exists, so
/// nothing here may pre-create it.
///
/// `build_settings` does not call this -- it leaves `installation_dir` at the
/// very default this returns. The readers are all test-side: `test_util`'s
/// reaper, which needs the `pg_ctl` under it, and this module's own unit tests
/// asserting where the default points. Hence the cfg: outside `test-util` the
/// function has no caller at all.
///
/// The cfg needs no `test` arm for those unit tests. This crate dev-depends on
/// itself with `test-util` on, so building any of its own test targets unifies
/// the feature onto the library -- `cfg(test)` here always implies
/// `feature = "test-util"`. That unification is the same one `just check`'s
/// `clippy-libs` pass exists to see past.
#[cfg(feature = "test-util")]
pub(crate) fn installation_dir() -> PathBuf {
    fresh_settings().installation_dir
}

fn build_settings(root_dir: &Path) -> Result<Settings, DbError> {
    std::fs::create_dir_all(root_dir).map_err(|source| DbError::io(root_dir, source))?;

    let mut settings = fresh_settings();

    settings.version =
        VersionReq::parse(PG_VERSION_REQ).expect("PG_VERSION_REQ is a valid semver requirement");
    // `installation_dir` is left at its shared default on purpose -- see
    // `installation_dir()`. Only state lives under `root_dir`.
    settings.data_dir = root_dir.join("data");
    settings.password_file = root_dir.join(".pgpass");
    settings.host = HOST.to_string();
    // 0 makes `start()` pick a free loopback port and record it in `settings`.
    settings.port = 0;
    settings.temporary = false;
    // Explicit: `None` keeps `Settings::url` on the TCP form.
    settings.socket_dir = None;
    settings.timeout = Some(COMMAND_TIMEOUT);

    // `Settings::new()` invents a fresh random password every call, but
    // `initdb` burned the first one into the data directory. Reuse what was
    // recorded, otherwise every restart authenticates with the wrong password.
    if let Some(password) = read_password(&settings.password_file)? {
        settings.password = password;
    }

    Ok(settings)
}

fn read_password(password_file: &Path) -> Result<Option<String>, DbError> {
    match std::fs::read_to_string(password_file) {
        Ok(contents) => {
            let password = contents.trim_end_matches(['\r', '\n']);
            Ok((!password.is_empty()).then(|| password.to_string()))
        }
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(DbError::io(password_file, source)),
    }
}

/// What `postmaster.pid` says about a data directory a start just failed on.
#[derive(Debug, PartialEq, Eq)]
enum Lock {
    /// There is no lock file, so it is not what refused the start.
    Absent,
    /// The lock belonged to a server that is gone, and has been removed: the
    /// start is worth retrying.
    Cleared,
    /// Something is answering on the port the lock records. Whether it is
    /// *our* server is [`adopt`]'s to establish; `pid` is line 1 of the lock
    /// file, which that decision needs too.
    Live { port: u16, pid: Option<u32> },
    /// Nothing answers on the recorded port, but the process the lock records
    /// is still alive. Not clearable: see [`inspect_lock`].
    LiveProcess { pid: u32 },
}

/// Classify `data_dir`'s `postmaster.pid`, clearing it when it is stale.
///
/// Two independent pieces of evidence, because either one alone lies:
///
/// * The **port** (line 4): a TCP handshake proves *something* is listening,
///   but a recycled ephemeral port can put a stranger there, so an answer is
///   not proof the lock is live -- which is why [`adopt`] asks the server what
///   it serves before trusting it.
/// * The **pid** (line 1): a live process is proof the lock may still be
///   owned. A postmaster that is still starting up, one wedged before it
///   opened its socket, or one listening on an interface this probe does not
///   reach all fail the 500 ms handshake while owning the data directory --
///   and clearing the lock under a live postmaster is how two servers end up
///   writing one data directory. So a dead port is only clearable once the
///   recorded process is gone too.
fn inspect_lock(data_dir: &Path) -> Result<Lock, DbError> {
    let pid_file = data_dir.join("postmaster.pid");

    let contents = match std::fs::read_to_string(&pid_file) {
        Ok(contents) => contents,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(Lock::Absent),
        Err(source) => return Err(DbError::io(pid_file, source)),
    };

    // Line 1 is the postmaster's pid, line 4 the port it last listened on.
    let recorded_pid = contents
        .lines()
        .next()
        .and_then(|line| line.trim().parse::<u32>().ok())
        .filter(|pid| *pid != 0);
    let recorded_port = contents
        .lines()
        .nth(3)
        .and_then(|line| line.trim().parse::<u16>().ok())
        .filter(|port| *port != 0);

    if let Some(port) = recorded_port
        && port_answers(port)
    {
        tracing::warn!(
            port,
            "postmaster.pid names a port something is answering on"
        );
        return Ok(Lock::Live {
            port,
            pid: recorded_pid,
        });
    }

    if let Some(pid) = recorded_pid
        && process_alive(pid)
    {
        tracing::warn!(
            pid,
            "postmaster.pid names a live process, though its port is silent: keeping the lock"
        );
        return Ok(Lock::LiveProcess { pid });
    }

    tracing::warn!(pid_file = %pid_file.display(), "removing stale postmaster.pid");
    clear_lock(data_dir)?;
    Ok(Lock::Cleared)
}

/// Whether a process with `pid` exists right now.
///
/// `ps -p` rather than `kill(pid, 0)`: this crate has no `libc` dependency and
/// nothing here is worth one. `ps` exits 0 when it printed a matching process
/// and 1 when it did not, on both macOS and Linux.
///
/// A pid is *not* proof of identity -- pids are recycled, so the process may
/// be anything at all. It is only ever used here as evidence that the lock
/// might still be owned, never as evidence that it is.
#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    std::process::Command::new("ps")
        .arg("-p")
        .arg(pid.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// No `ps` to ask, so nothing is known: the pid contributes no evidence and
/// the port decides alone, exactly as it did before the check existed.
#[cfg(not(unix))]
fn process_alive(_pid: u32) -> bool {
    false
}

/// Whether the pid `postmaster.pid` recorded belongs to a process that is
/// still running. An unparseable or absent pid is no evidence of life.
fn recorded_pid_alive(pid: Option<u32>) -> bool {
    pid.is_some_and(process_alive)
}

/// Whether a connection failure happened *below* the PostgreSQL protocol.
///
/// [`sqlx::Error::Database`] means a PostgreSQL answered and refused us (a
/// wrong password, a missing database): there is a postmaster on that port,
/// whatever else is wrong. `Io`, `Tls` and `Protocol` mean the bytes never
/// added up to a PostgreSQL conversation at all -- the shape of an unrelated
/// listener sitting on a recycled ephemeral port.
fn is_wire_level(error: &sqlx::Error) -> bool {
    matches!(
        error,
        sqlx::Error::Io(_) | sqlx::Error::Tls(_) | sqlx::Error::Protocol(_)
    )
}

/// Name of the file whose OS lock serialises bring-up within one `root_dir`.
const BRING_UP_LOCK: &str = ".bring-up.lock";

/// How long to wait for another process to finish bringing this `root_dir` up.
/// A cold first run is an `initdb` plus a server start; the wait has to cover
/// both, and expiring is a hard failure rather than a race to run in parallel.
const BRING_UP_TIMEOUT: Duration = Duration::from_secs(180);

/// How often to retry the lock while another process holds it.
const BRING_UP_POLL: Duration = Duration::from_millis(50);

/// Take the exclusive bring-up lock for `root_dir`, waiting for whoever holds
/// it.
///
/// `initdb` is not safe to run twice against one data directory, and neither
/// process can see the other coming: `PostgreSQL::setup` decides whether to
/// initialise by looking at the directory, so two first launches on one profile
/// (a double-click, a `just dev` beside a packaged build) both find it empty
/// and both proceed. The loser then finds a half-written data directory rather
/// than a server it could adopt -- `postmaster.pid` does not exist yet, so
/// none of the recovery paths above apply.
///
/// The lock is an OS lock on a file *beside* the data directory, so the kernel
/// releases it however the process dies, and it is only ever held for the
/// duration of one bring-up.
///
/// Returned rather than dropped here: the guard must outlive the work it
/// protects, and holding the returned `File` is what does that.
async fn bring_up_lock(root_dir: &Path) -> Result<File, DbError> {
    let path = root_dir.join(BRING_UP_LOCK);
    let file = File::create(&path).map_err(|source| DbError::io(&path, source))?;

    let deadline = std::time::Instant::now() + BRING_UP_TIMEOUT;
    let mut waited = false;
    loop {
        match file.try_lock() {
            Ok(()) => {
                if waited {
                    tracing::info!("the other process finished bringing the database up");
                }
                return Ok(file);
            }
            Err(TryLockError::WouldBlock) if std::time::Instant::now() < deadline => {
                if !waited {
                    waited = true;
                    tracing::info!(
                        lock = %path.display(),
                        "another process is bringing this database up: waiting"
                    );
                }
                tokio::time::sleep(BRING_UP_POLL).await;
            }
            Err(TryLockError::WouldBlock) => {
                return Err(DbError::io(
                    &path,
                    io::Error::new(
                        io::ErrorKind::WouldBlock,
                        format!(
                            "another process has held the bring-up lock for over {} seconds",
                            BRING_UP_TIMEOUT.as_secs()
                        ),
                    ),
                ));
            }
            Err(TryLockError::Error(source)) => return Err(DbError::io(&path, source)),
        }
    }
}

/// Remove `data_dir`'s `postmaster.pid`, if it is still there.
///
/// Missing is success: the only callers are recoveries from a lock that has
/// already been proven not to describe a live server, and losing a race to
/// remove it is the outcome they wanted.
fn clear_lock(data_dir: &Path) -> Result<(), DbError> {
    let pid_file = data_dir.join("postmaster.pid");
    match std::fs::remove_file(&pid_file) {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(DbError::io(pid_file, source)),
    }
}

fn port_answers(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&addr, LIVENESS_TIMEOUT).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, contents: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn build_settings_pins_version_and_uses_loopback_tcp() {
        let dir = tempfile::tempdir().unwrap();
        let settings = build_settings(dir.path()).unwrap();

        assert_eq!(settings.version.to_string(), "=18.6.0");
        assert_eq!(settings.host, HOST);
        assert_eq!(settings.port, 0);
        assert!(!settings.temporary);
        assert!(settings.socket_dir.is_none());
        assert_eq!(settings.data_dir, dir.path().join("data"));
        assert_eq!(settings.password_file, dir.path().join(".pgpass"));
        // No `?host=` query -- a Unix-socket URL would carry one.
        assert!(settings.url(DATABASE_NAME).contains("@127.0.0.1:0/knobas"));
    }

    #[test]
    fn build_settings_keeps_the_binaries_out_of_the_root_dir() {
        let dir = tempfile::tempdir().unwrap();
        let settings = build_settings(dir.path()).unwrap();

        // Shared across every `root_dir` so the archive is fetched once per
        // machine, not once per data directory.
        assert_eq!(settings.installation_dir, installation_dir());
        assert!(!settings.installation_dir.starts_with(dir.path()));
        assert!(settings.installation_dir.ends_with("postgresql"));
    }

    #[test]
    fn build_settings_does_not_create_the_installation_dir() {
        // The extractor skips extraction when the target already exists, so
        // nothing may bring it into being ahead of `setup()`.
        let install = installation_dir();
        let existed = install.exists();
        let dir = tempfile::tempdir().unwrap();

        build_settings(dir.path()).unwrap();

        assert_eq!(install.exists(), existed);
    }

    #[test]
    fn build_settings_reuses_the_recorded_password() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), ".pgpass", "s3cret\n");

        let settings = build_settings(dir.path()).unwrap();

        assert_eq!(settings.password, "s3cret");
    }

    #[test]
    fn build_settings_ignores_an_empty_password_file() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), ".pgpass", "");

        let settings = build_settings(dir.path()).unwrap();

        assert!(!settings.password.is_empty());
    }

    /// A pid that is certainly not running: a child that has already been
    /// reaped. Nothing else about a number is provably dead -- pids recycle.
    fn dead_pid() -> u32 {
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        pid
    }

    /// A port nothing is listening on: bind it, read it back, drop the listener.
    fn free_port() -> u16 {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.local_addr().unwrap().port()
    }

    /// A `postmaster.pid` as PostgreSQL writes it: pid, data directory, start
    /// time, port.
    fn pid_file(dir: &Path, pid: u32, port: u16) -> PathBuf {
        write(
            dir,
            "postmaster.pid",
            &format!("{pid}\n{}\n1700000000\n{port}\n", dir.display()),
        )
    }

    #[test]
    fn inspect_lock_reports_nothing_to_do_without_a_pid_file() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(inspect_lock(dir.path()).unwrap(), Lock::Absent);
    }

    #[test]
    fn inspect_lock_removes_a_pid_file_with_no_process_and_no_listener() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = pid_file(dir.path(), dead_pid(), free_port());

        assert_eq!(inspect_lock(dir.path()).unwrap(), Lock::Cleared);
        assert!(!pid_file.exists());
    }

    /// A silent port is *not* on its own proof that the lock is free: a
    /// postmaster still starting up, or wedged before it opened its socket,
    /// fails the handshake while owning the data directory. Clearing the lock
    /// there is how a second server ends up writing the same files.
    #[test]
    fn inspect_lock_keeps_a_pid_file_whose_process_is_still_alive() {
        let dir = tempfile::tempdir().unwrap();
        // Our own pid: alive for certain, for as long as this test runs.
        let alive = std::process::id();
        let pid_file = pid_file(dir.path(), alive, free_port());

        assert_eq!(
            inspect_lock(dir.path()).unwrap(),
            Lock::LiveProcess { pid: alive }
        );
        assert!(
            pid_file.exists(),
            "a lock whose process is still running must not be cleared"
        );
    }

    #[test]
    fn inspect_lock_keeps_a_pid_file_a_live_listener_owns() {
        let dir = tempfile::tempdir().unwrap();
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let pid = dead_pid();
        let pid_file = pid_file(dir.path(), pid, port);

        assert_eq!(
            inspect_lock(dir.path()).unwrap(),
            Lock::Live {
                port,
                pid: Some(pid)
            }
        );
        assert!(pid_file.exists());
    }

    #[test]
    fn process_liveness_tells_a_running_process_from_a_reaped_one() {
        // Both directions, because a `process_alive` stuck at either answer
        // would satisfy one of the `inspect_lock` cases above on its own.
        assert!(process_alive(std::process::id()));
        assert!(!process_alive(dead_pid()));
    }

    /// A listener that accepts connections and immediately drops them: an
    /// unrelated program on a recycled ephemeral port, as far as a client can
    /// tell. Returns the port; the accept loop lives until the process exits.
    fn squatter() -> u16 {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                drop(stream);
            }
        });
        port
    }

    fn adoption_settings(dir: &Path) -> Settings {
        let mut settings = build_settings(dir).unwrap();
        // `adopt` never starts anything, so the data directory need not exist;
        // what matters is that it is *this* root's.
        settings.data_dir = dir.join("data");
        settings
    }

    /// The recycled-port lockout: a dead postmaster's port now belongs to some
    /// unrelated program. That is not another knobas instance, and reporting it
    /// as one leaves the user with an app that refuses to launch until they
    /// delete `postmaster.pid` by hand.
    #[tokio::test]
    async fn adopt_treats_a_non_postgres_listener_as_a_stale_lock() {
        let dir = tempfile::tempdir().unwrap();
        let port = squatter();

        match adopt(adoption_settings(dir.path()), port, Some(dead_pid())).await {
            Ok(Started::StaleLock { port: p, .. }) => assert_eq!(p, port),
            Ok(Started::Ready(_)) => panic!("nothing on that port could have been adopted"),
            Err(error) => panic!("a stranger on the port is a stale lock, not {error:?}"),
        }
    }

    /// The same stranger, while the recorded process is still alive: the lock
    /// may still be owned -- a postmaster that has not opened its socket yet
    /// looks exactly like this -- so it is reported, not cleared.
    #[tokio::test]
    async fn adopt_refuses_while_the_recorded_process_is_alive() {
        let dir = tempfile::tempdir().unwrap();
        let port = squatter();

        // `Started` carries an `EmbeddedDb`, which is not `Debug`, so the
        // success arm is matched rather than unwrapped.
        match adopt(
            adoption_settings(dir.path()),
            port,
            Some(std::process::id()),
        )
        .await
        {
            Err(DbError::AlreadyRunning { .. }) => {}
            Err(other) => panic!("expected AlreadyRunning, got {other:?}"),
            Ok(_) => panic!("a live recorded process must not have its lock cleared"),
        }
    }

    /// A failure the *PostgreSQL protocol* produced -- a wrong password, say --
    /// means there is a postmaster on that port. Clearing the lock under it
    /// would start a second server on the same data directory.
    #[test]
    fn only_sub_protocol_failures_count_as_a_stranger_on_the_port() {
        assert!(is_wire_level(&sqlx::Error::Io(io::Error::from(
            io::ErrorKind::ConnectionReset
        ))));
        assert!(is_wire_level(&sqlx::Error::Protocol("garbage".into())));
        assert!(!is_wire_level(&sqlx::Error::RowNotFound));
        assert!(!is_wire_level(&sqlx::Error::PoolClosed));
    }

    /// Two bring-ups of one `root_dir` cannot overlap -- which is what keeps
    /// two first launches from running `initdb` on top of each other.
    #[tokio::test]
    async fn the_bring_up_lock_is_exclusive_and_released_with_its_guard() {
        let dir = tempfile::tempdir().unwrap();
        let held = bring_up_lock(dir.path()).await.unwrap();

        // A second holder from *this* process would prove nothing about
        // another one on some platforms, so the contention is probed the way
        // the loser sees it: the same file, an independent handle.
        let contender = File::open(dir.path().join(BRING_UP_LOCK)).unwrap();
        assert!(
            matches!(contender.try_lock(), Err(TryLockError::WouldBlock)),
            "the bring-up lock must exclude a second process"
        );

        drop(held);
        assert!(
            bring_up_lock(dir.path()).await.is_ok(),
            "dropping the guard must release the lock"
        );
    }
}
