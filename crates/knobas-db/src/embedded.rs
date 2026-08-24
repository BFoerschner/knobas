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
    /// server is proof that no postmaster holds this data directory -- one that
    /// did would have recorded its own port. That lock is removed and the start
    /// is retried, rather than reported as a conflict that does not exist.
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

        match start_managed(settings.clone()).await? {
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
                match start_managed(settings).await? {
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
    /// `postmaster.pid` is provably stale: the port it records is answered, but
    /// by a server serving `serving` rather than our data directory. Clearing
    /// the lock and attempting the start again is the recovery.
    StaleLock { port: u16, serving: String },
}

/// One attempt at starting (or joining) the server for `settings`.
///
/// Factored out of [`EmbeddedDb::start`] because the stale-lock recovery has to
/// run the whole sequence again -- `setup`, `start`, database creation --
/// against a fresh `PostgreSQL` handle.
async fn start_managed(settings: Settings) -> Result<Started, DbError> {
    let data_dir = settings.data_dir.clone();
    // `PostgreSQL::new` consumes the settings, and the adoption path needs
    // them after the handle is gone.
    let adoption = settings.clone();
    let mut postgresql = PostgreSQL::new(settings);

    postgresql.setup().await?;

    if let Err(error) = postgresql.start().await {
        match inspect_lock(&data_dir)? {
            // A hard-killed process (crash, `kill -9`, laptop shutdown)
            // leaves `postmaster.pid` behind and PostgreSQL then refuses to
            // start. The lock is gone now, so the start is worth retrying.
            Lock::Cleared => postgresql.start().await?,
            // No lock at all: the start failed for some other reason, and
            // that reason is the one worth reporting.
            Lock::Absent => return Err(error.into()),
            Lock::Live { port } => {
                // `PostgreSQL::drop` runs `pg_ctl stop -m fast` whenever
                // `postmaster.pid` merely *exists* -- it has no idea
                // whether this handle is the one that started that server.
                // Dropping it here would therefore shut down the live
                // server that just refused us, taking the database out from
                // under whoever is using it. Forget it instead: it owns no
                // OS resource, only paths and strings, so the leak is a few
                // hundred bytes for the rest of the process -- a trade
                // worth making against killing a sibling's database.
                std::mem::forget(postgresql);
                return adopt(adoption, port).await;
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
/// [`DbError::AlreadyRunning`] if the server cannot be reached or queried, or
/// if the [`DATABASE_NAME`] database cannot be created on it. A server that
/// turns out to serve a *different* data directory is not an error at all: it
/// is returned as [`Started::StaleLock`], because the mismatch proves the lock
/// file we came from is stale.
async fn adopt(mut settings: Settings, port: u16) -> Result<Started, DbError> {
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
    let mut admin = PgConnection::connect(&settings.url(MAINTENANCE_DATABASE))
        .await
        .map_err(|source| unreachable(format!("cannot connect on port {port}: {source}")))?;

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
/// asserting where the default points. Hence the cfg: outside those two builds
/// the function has no caller at all.
#[cfg(any(test, feature = "test-util"))]
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
    /// A live server is answering on the port the lock records.
    Live { port: u16 },
}

/// Classify `data_dir`'s `postmaster.pid`, clearing it when it is stale.
fn inspect_lock(data_dir: &Path) -> Result<Lock, DbError> {
    let pid_file = data_dir.join("postmaster.pid");

    let contents = match std::fs::read_to_string(&pid_file) {
        Ok(contents) => contents,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(Lock::Absent),
        Err(source) => return Err(DbError::io(pid_file, source)),
    };

    // Line 4 of postmaster.pid is the port the server last listened on.
    let recorded_port = contents
        .lines()
        .nth(3)
        .and_then(|line| line.trim().parse::<u16>().ok())
        .filter(|port| *port != 0);

    if let Some(port) = recorded_port
        && port_answers(port)
    {
        tracing::warn!(port, "postmaster.pid is held by a live server");
        return Ok(Lock::Live { port });
    }

    tracing::warn!(pid_file = %pid_file.display(), "removing stale postmaster.pid");
    clear_lock(data_dir)?;
    Ok(Lock::Cleared)
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

    #[test]
    fn inspect_lock_reports_nothing_to_do_without_a_pid_file() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(inspect_lock(dir.path()).unwrap(), Lock::Absent);
    }

    #[test]
    fn inspect_lock_removes_a_pid_file_nobody_answers_for() {
        let dir = tempfile::tempdir().unwrap();
        // A free port: bind it, read it back, drop the listener.
        let port = {
            let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
            listener.local_addr().unwrap().port()
        };
        let pid_file = write(
            dir.path(),
            "postmaster.pid",
            &format!("4242\n{}\n1700000000\n{port}\n", dir.path().display()),
        );

        assert_eq!(inspect_lock(dir.path()).unwrap(), Lock::Cleared);
        assert!(!pid_file.exists());
    }

    #[test]
    fn inspect_lock_keeps_a_pid_file_a_live_listener_owns() {
        let dir = tempfile::tempdir().unwrap();
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let pid_file = write(
            dir.path(),
            "postmaster.pid",
            &format!("4242\n{}\n1700000000\n{port}\n", dir.path().display()),
        );

        assert_eq!(inspect_lock(dir.path()).unwrap(), Lock::Live { port });
        assert!(pid_file.exists());
    }
}
