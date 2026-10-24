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

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{self, Read, Seek, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use postgresql_embedded::{PostgreSQL, Settings, VersionReq};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
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
pub(crate) const MAINTENANCE_DATABASE: &str = "postgres";

/// Loopback host. Never a Unix socket -- see the module docs.
const HOST: &str = "127.0.0.1";

/// Binary name of `pg_ctl` on this platform.
///
/// Read only below `test-util`, by the immediate-stop builder the two
/// disposable-server stops share; the shipped build reaches `pg_ctl` through
/// `postgresql_embedded` alone.
#[cfg(feature = "test-util")]
pub(crate) const PG_CTL: &str = if cfg!(windows) {
    "pg_ctl.exe"
} else {
    "pg_ctl"
};

/// The settings the app's server writes durably under, passed as `-c` every
/// time it starts (#1896): see [`build_settings`]. The gate's test server
/// overrides `fsync` (see `start_with_max_connections`).
const DURABLE_SETTINGS: [(&str, &str); 3] = [
    ("fsync", "on"),
    ("synchronous_commit", "on"),
    ("full_page_writes", "on"),
];

/// Pool size. The desktop app is a single user with a handful of concurrent
/// queries; a larger pool only buys idle backends.
const MAX_CONNECTIONS: u32 = 5;

/// The `statement_timeout` every connection of the app's pool carries (#745).
///
/// Defence in depth: a statement that holds a UI connection for longer than
/// this is cancelled by the server rather than starving every command behind
/// it, which is what the 2026-10-13 hang was (a detection pass held all five
/// for minutes). Thirty seconds is the pool's own acquire timeout -- a command
/// that waits longer than that for a connection has already failed -- and far
/// above any UI statement measured for #745: under five sources syncing at
/// machine load 25, the slowest UI command in `just ui-pool-probe` took well
/// under a second. The long statements knobas does run are elsewhere: sync
/// runs and the detection pass on connections of their own, a backup in
/// `pg_dump`, and migrations on the one-connection pool `migrate::run` opens
/// for itself without the limit. Two app-pool callers grow with the mirror and
/// lift the limit for themselves: the *Re-index* rebuild
/// (`knobas_sync::stats::reindex_fts`, on a connection taken out of the pool)
/// and a change to the adapters' declarations (`Searcher::declaring`, `set
/// local` in its transaction), 7.6 s at 100,000 items in the #745 review.
/// Every other app-pool caller measured there stays under a second at that
/// size: a source's purge 0.74 s for 33,000 items, `db_stats` 72 ms.
pub const UI_STATEMENT_TIMEOUT: Duration = Duration::from_secs(30);

/// Upper bound for `initdb` / `pg_ctl` invocations. The crate default is 5s,
/// which a cold `initdb` on a slow disk can exceed.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(60);

/// How long to wait for a TCP handshake when deciding whether a recorded
/// `postmaster.pid` still has a live server behind it.
const LIVENESS_TIMEOUT: Duration = Duration::from_millis(500);

/// Upper bound on the first connection of a *first contact*: `KNOBAS_DB_URL`'s
/// server, before anything has answered on it.
///
/// M0 let `PgPoolOptions::connect` decide, and its acquire timeout is 30 s: a
/// `KNOBAS_DB_URL` pointing at a dead port therefore left the boot screen on
/// "Starting the local database" for a full half-minute before saying anything
/// (M1 carry-over, observed by stream D). A refused handshake is instant and a
/// live server answers in milliseconds, so five seconds is generous for the
/// question actually being asked -- and it bounds only the probe, not the
/// pool's own acquire timeout, which still has to cover a busy database.
///
/// **Only for a first contact** (#1815): a pool on a server that has already
/// answered -- one this process started, adopted, or reached before -- waits
/// for its first connection under the pool's acquire timeout instead. On a
/// loaded machine a live local server took longer than five seconds to
/// answer, moments after it had answered, and this bound called it dead.
const FIRST_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// How long [`EmbeddedDb::stop`] waits for in-flight queries before stopping
/// the server regardless.
///
/// `PgPool::close` waits for every borrowed connection to come back, and one
/// parked on a remote system is how Cmd-Q during a sync turns into a
/// thirty-second hang (M0 carry-over). Past this, `pg_ctl stop -m fast`
/// terminates the backends -- which is what `-m fast` is for.
const POOL_CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

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

    /// The server on `data_dir` did not start (#2025). `reason` is what the
    /// server wrote to `start.log` during this start ([`start_reason`]), or
    /// `pg_ctl`'s own output when the log gave nothing. `pg_ctl` alone says
    /// *stopped waiting*, and the boot screen shows this one line.
    #[error("embedded postgres at {data_dir} did not start: {reason}")]
    ServerStart {
        data_dir: PathBuf,
        reason: String,
        #[source]
        source: postgresql_embedded::Error,
    },

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

    /// The data directory is in use, and this start leaves it alone.
    ///
    /// The usual cause is a second knobas window on the same profile: a live
    /// process holds the ownership lock, and `reason` names its pid (ruling
    /// B2, spec #1619). A second window is refused rather than let in beside
    /// the first, because the first one's quit stops the server under it.
    /// The other cause is a lock file whose recorded port answers as a
    /// PostgreSQL that cannot be reached as ours (one still starting up or
    /// shutting down, say), and `reason` says what it answered.
    ///
    /// Note what this error does *not* do: it never stops that server.
    /// Anything that shuts a stranger's postmaster down risks taking a
    /// running instance's database with it.
    #[error("the database at {data_dir} is in use: {reason}")]
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
/// When `DbConfig::existing_url` was set, no server is owned; the handle is
/// then just the pool and `stop` only closes it. Every managed handle owns its
/// server, adopted or started here: a live owner refuses a second one (B2).
pub struct EmbeddedDb {
    pool: PgPool,
    /// How to reach the running server, for callers that need connections of
    /// their own rather than the shared pool's.
    ///
    /// No longer test-only: [`pool_for`](Self::pool_for) and
    /// [`connector`](Self::connector) are application paths -- the sync
    /// scheduler runs on its own pool, and each of its runs holds a connection
    /// that belongs to no pool at all (interfaces §10.6(c)). It carries the
    /// superuser password, so it stays **private** and redacts in `Debug`;
    /// callers ask for a pool or a connection, never for the string.
    connector: Connector,
    /// `None` when connected to a server we do not manage.
    postgresql: Option<PostgreSQL>,
    /// The settings `pg_ctl` needs to stop a server this process *adopted*.
    ///
    /// `None` for a server started here (`postgresql` knows how to stop that
    /// one) and for an externally managed `existing_url`, which is never ours
    /// to stop.
    stop_settings: Option<Settings>,
    /// Held for this handle's life when this process is responsible for
    /// stopping the server. `None` only when `existing_url` points at a
    /// server knobas does not manage at all: a managed start either takes
    /// the lock or is refused.
    owner_lock: Option<OwnerLock>,
}

/// How to open a connection to a knobas database.
///
/// A wrapper rather than a bare [`PgConnectOptions`] for one reason: that type
/// derives `Debug` and holds the superuser password, so a single
/// `tracing::debug!(?options)` anywhere downstream would put it in a log file.
/// Spec §14 says no secret is ever logged, and the way to make that true is a
/// type that cannot print one.
#[derive(Clone)]
pub struct Connector {
    options: PgConnectOptions,
    /// The password, kept beside the options because [`PgConnectOptions`] has
    /// no getter for one and [`backup`](crate::backup) has to hand it to a
    /// *child process* -- `pg_dump` speaks libpq, not sqlx.
    ///
    /// It goes to that child in `PGPASSWORD`, never in an argument: `argv` is
    /// world-readable in `ps` and the environment of another user's process is
    /// not.
    password: Option<String>,
    /// Whatever must stay alive for as long as any clone of this connector
    /// does: a test database's lease (`test_util`), which hands the database
    /// on to another test only once nothing can reach it under this name.
    /// `None` for every connector the application makes.
    keep: Option<Arc<dyn std::any::Any + Send + Sync>>,
}

impl std::fmt::Debug for Connector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connector")
            .field("host", &self.options.get_host())
            .field("port", &self.options.get_port())
            .field("database", &self.options.get_database())
            .field("password", &"<redacted>")
            .finish()
    }
}

impl Connector {
    /// # Errors
    /// [`sqlx::Error`] if the URL cannot be parsed.
    fn parse(url: &str) -> Result<Connector, sqlx::Error> {
        Ok(Connector {
            options: url.parse()?,
            password: password_of(url),
            keep: None,
        })
    }

    /// The same server, a different database.
    ///
    /// What `create database` needs (the statement cannot run inside the
    /// database it creates, so it is issued from the maintenance database) and
    /// what a restore into a *fresh* database needs.
    #[must_use]
    pub fn with_database(&self, database: &str) -> Connector {
        Connector {
            options: self.options.clone().database(database),
            password: self.password.clone(),
            keep: self.keep.clone(),
        }
    }

    /// This connector, keeping `keep` alive until its last clone is gone.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub(crate) fn keeping(mut self, keep: Arc<dyn std::any::Any + Send + Sync>) -> Connector {
        self.keep = Some(keep);
        self
    }

    /// The database this connector opens, when the URL named one.
    #[must_use]
    pub fn database(&self) -> Option<&str> {
        self.options.get_database()
    }

    /// The connection URL, password and all.
    ///
    /// The one place a `Connector` gives the string back, and it exists only
    /// with `test-util` on: the test-server entry point has to hand its URL to the
    /// test binaries through the environment, which is a string. Nothing a
    /// shipped build compiles can call this, so the redaction the type exists
    /// for still holds there.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub fn url(&self) -> String {
        use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};

        let user = utf8_percent_encode(self.options.get_username(), NON_ALPHANUMERIC);
        let password = self
            .password
            .as_deref()
            .map(|password| format!(":{}", utf8_percent_encode(password, NON_ALPHANUMERIC)))
            .unwrap_or_default();
        let database = self.options.get_database().unwrap_or_default();
        format!(
            "postgresql://{user}{password}@{}:{}/{database}",
            self.options.get_host(),
            self.options.get_port()
        )
    }

    /// The libpq environment a child process needs to reach this server.
    ///
    /// `pg_dump` and `pg_restore` take their connection from the environment
    /// rather than from `argv` on purpose -- see [`Connector::password`].
    pub(crate) fn libpq_env(&self) -> Vec<(&'static str, String)> {
        let mut env = vec![
            ("PGHOST", self.options.get_host().to_owned()),
            ("PGPORT", self.options.get_port().to_string()),
            ("PGUSER", self.options.get_username().to_owned()),
        ];
        if let Some(database) = self.options.get_database() {
            env.push(("PGDATABASE", database.to_owned()));
        }
        if let Some(password) = &self.password {
            env.push(("PGPASSWORD", password.clone()));
        }
        env
    }

    /// One connection, belonging to no pool.
    ///
    /// This is what interfaces §10.6(c) requires of a sync run: the advisory
    /// lock and the transaction are held for as long as the remote system
    /// takes to answer, and holding them on a *pooled* connection means the
    /// rest of the app is competing for a connection the network has parked.
    /// A connection opened here is competing with nothing -- it costs one TCP
    /// handshake and one authentication per run, which against a paginated
    /// sync is not measurable.
    ///
    /// # Errors
    /// [`sqlx::Error`] if the connection fails.
    pub async fn connect(&self) -> Result<PgConnection, sqlx::Error> {
        PgConnection::connect_with(&self.options).await
    }

    /// A pool of `max_connections` on the same server, its first connection
    /// waited for under the pool's acquire timeout: this server has answered
    /// already, so [`FIRST_CONNECT_TIMEOUT`] is not the question (#1815).
    ///
    /// # Errors
    /// [`sqlx::Error`] if the first connection fails.
    pub async fn pool(&self, max_connections: u32) -> Result<PgPool, sqlx::Error> {
        PgPoolOptions::new()
            .max_connections(max_connections)
            .connect_with(self.options.clone())
            .await
    }

    /// The app's own pool: five connections, each carrying
    /// [`UI_STATEMENT_TIMEOUT`] from its first packet. Its first connection
    /// is waited for under the pool's acquire timeout, as [`pool`](Self::pool)'s
    /// is: the callers reach a server that has just answered them.
    ///
    /// # Errors
    /// [`sqlx::Error`] if the first connection fails.
    pub async fn app_pool(&self) -> Result<PgPool, sqlx::Error> {
        let pool = self.app_pool_unconnected();
        drop(pool.acquire().await?);
        Ok(pool)
    }

    /// [`app_pool`](Self::app_pool) for `KNOBAS_DB_URL`'s first contact,
    /// where nothing has answered yet: the first connection is bounded by
    /// [`FIRST_CONNECT_TIMEOUT`], so a dead URL fails the boot screen fast.
    async fn app_pool_on_first_contact(&self) -> Result<PgPool, sqlx::Error> {
        let pool = self.app_pool_unconnected();
        probe(&pool).await?;
        Ok(pool)
    }

    /// The app pool's settings, with no connection opened yet: the two
    /// callers above differ only in how long they wait for the first one.
    fn app_pool_unconnected(&self) -> PgPool {
        let options = self.options.clone().options([(
            "statement_timeout",
            format!("{}ms", UI_STATEMENT_TIMEOUT.as_millis()),
        )]);
        PgPoolOptions::new()
            .max_connections(MAX_CONNECTIONS)
            // sqlx's default, spelled out: the statement limit above is
            // argued from it.
            .acquire_timeout(Duration::from_secs(30))
            .connect_lazy_with(options)
    }
}

/// The password a connection URL carries, percent-decoded.
///
/// sqlx keeps it inside [`PgConnectOptions`] with no getter, and a child
/// process needs it, so it is read off the URL the same way sqlx reads it:
/// `url` for the structure, `percent-encoding` for the escapes. Both crates
/// are already in the tree -- `sqlx-core` depends on them -- so this costs
/// nothing to compile.
///
/// `None` for a URL with no password (a `trust`-authenticated server, or a
/// `KNOBAS_DB_URL` relying on the caller's own `.pgpass`); libpq then falls
/// back to its usual sources, which is the right behaviour rather than a
/// failure.
fn password_of(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let raw = parsed.password()?;
    Some(
        percent_encoding::percent_decode_str(raw)
            .decode_utf8()
            .ok()?
            .into_owned(),
    )
}

/// Take one connection out of `pool`, under [`FIRST_CONNECT_TIMEOUT`]: for
/// `KNOBAS_DB_URL`'s first contact alone (#1815).
///
/// `PgPoolOptions::connect` does this itself and bounds it by the pool's
/// acquire timeout, which is 30 s and has to stay that way for a *busy*
/// database. Bounding the first connection separately is what keeps a dead
/// `KNOBAS_DB_URL` from holding the boot screen for half a minute.
async fn probe(pool: &PgPool) -> Result<(), sqlx::Error> {
    match tokio::time::timeout(FIRST_CONNECT_TIMEOUT, pool.acquire()).await {
        Ok(Ok(conn)) => {
            drop(conn);
            Ok(())
        }
        Ok(Err(error)) => Err(error),
        Err(_elapsed) => Err(sqlx::Error::Io(io::Error::new(
            io::ErrorKind::TimedOut,
            format!(
                "no answer from the database within {}s",
                FIRST_CONNECT_TIMEOUT.as_secs()
            ),
        ))),
    }
}

impl EmbeddedDb {
    /// Bring the database up and return a connected pool.
    ///
    /// For a managed server this installs PostgreSQL into the shared,
    /// version-keyed `~/.theseus/postgresql/<version>` (a no-op once present,
    /// and shared with every other `root_dir` on the machine), runs `initdb` into `root_dir/data` the
    /// first time, starts the server on an ephemeral loopback port with
    /// `fsync` on (#1896) and creates the `knobas` database if it does not
    /// exist yet. An adopted orphan an older build left running with `fsync`
    /// off is stopped and started again with it on (#1896).
    ///
    /// A server that is already up and serving this very data directory, one
    /// a signal-killed run left behind, is *adopted* rather than fought over:
    /// see [`adopt`]. A second instance on a profile a live knobas already has
    /// open is refused instead, before it touches anything: see
    /// [`claim_ownership`].
    ///
    /// A `postmaster.pid` whose port turns out to be answered by some *other*
    /// server, by something that is not a PostgreSQL at all, or by one that
    /// refuses our password, is proof that no postmaster of ours holds this
    /// data directory: one that did would answer on the port it recorded (see
    /// [`adopt`], *A stranger on the port*). That lock is removed and the start is
    /// retried, rather than reported as a conflict that does not exist.
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
    /// live knobas has this profile open, or a server answering on the port
    /// the lock file records cannot be reached as ours.
    pub async fn start(cfg: DbConfig) -> Result<EmbeddedDb, DbError> {
        if let Some(url) = cfg.existing_url.as_deref() {
            tracing::info!("connecting to externally managed postgres");
            let connector = Connector::parse(url)?;
            return Ok(EmbeddedDb {
                pool: connector.app_pool_on_first_contact().await?,
                connector,
                postgresql: None,
                stop_settings: None,
                // Never ours to stop, whatever any lock file says.
                owner_lock: None,
            });
        }

        let root = cfg.root_dir.clone();
        let settings = off_runtime(&cfg.root_dir, move || build_settings(&root)).await??;
        launch(&cfg.root_dir, settings).await
    }

    /// [`start`](Self::start) for a managed server with a larger connection
    /// limit than the desktop app's server has, and with `fsync` off.
    ///
    /// For the test server `just test` runs: every test binary of a gate opens
    /// its pools on that one server, so PostgreSQL's default ceiling of 100 is
    /// what a parallel run would hit first. The knob is exposed here rather
    /// than through [`DbConfig`] because no shipped path wants it -- hence
    /// `test-util` only. `fsync` is off because the run deletes the data
    /// directory at its end; the app's server runs it on (#1896).
    ///
    /// # Errors
    ///
    /// As [`start`](Self::start).
    #[cfg(feature = "test-util")]
    pub async fn start_with_max_connections(
        root_dir: PathBuf,
        max_connections: u32,
    ) -> Result<EmbeddedDb, DbError> {
        let root = root_dir.clone();
        let mut settings = off_runtime(&root_dir, move || build_settings(&root)).await??;
        settings
            .configuration
            .insert("max_connections".to_owned(), max_connections.to_string());
        // Over the `fsync=on` `build_settings` passes (#1896): this server's
        // data directory is deleted when the run ends, so a crash costs it
        // nothing a sync to disk would have saved, and every test's `create
        // database` and `drop database` skip the flush.
        settings
            .configuration
            .insert("fsync".to_owned(), "off".to_owned());
        launch(&root_dir, settings).await
    }

    /// The connection pool for the `knobas` database.
    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// How to reach this server, for a caller that needs connections of its
    /// own.
    ///
    /// A [`Connector`] and not the URL: the string carries the superuser
    /// password, and a type that redacts in `Debug` is what keeps it out of a
    /// log line. Cheap to clone and independent of this handle's lifetime,
    /// which is what the scheduler needs -- it opens a connection per run for
    /// as long as the process lives.
    #[must_use]
    pub fn connector(&self) -> Connector {
        self.connector.clone()
    }

    /// A second pool on the same server, for a caller that must not share the
    /// application's connections.
    ///
    /// The sync scheduler is the reason this exists: its bookkeeping -- the
    /// run log, credential health, the status read -- happens while runs are
    /// parked on the network, and a scheduler that cannot record what it is
    /// doing is worse than a slow one. It is **not** what discharges the
    /// §10.6(c) carry-over on its own; the runs themselves hold connections
    /// from [`connector`](Self::connector), outside every pool.
    ///
    /// # Errors
    /// [`DbError::Sqlx`] if the connection fails.
    pub async fn pool_for(&self, max_connections: u32) -> Result<PgPool, DbError> {
        Ok(self.connector.pool(max_connections).await?)
    }

    /// Whether this process is responsible for stopping the server.
    ///
    /// False for an externally managed server (`existing_url`) only: a
    /// managed start that cannot take ownership is refused instead.
    #[must_use]
    pub fn owns_server(&self) -> bool {
        self.owner_lock.is_some()
    }

    /// A watch on this handle's server, for noticing that it died (#1779).
    ///
    /// `None` for a server this process does not own (`existing_url`), whose
    /// life is not knobas's to follow, and when there is no `postmaster.pid`
    /// naming a pid. A file left naming a dead postmaster is watched, and
    /// reads as gone at once.
    #[must_use]
    pub fn watch(&self) -> Option<ServerWatch> {
        self.owner_lock.as_ref()?;
        let data_dir = self.server_settings()?.data_dir.clone();
        let pid = recorded_pid(&data_dir)?;
        Some(ServerWatch { data_dir, pid })
    }

    /// Start this handle's server again after it died under the handle
    /// (#1779), on the port it had, so the pool, every pool
    /// [`pool_for`](Self::pool_for) opened and the
    /// [`connector`](Self::connector) reach it again without being rebuilt.
    ///
    /// The handle holds the ownership lock throughout, so the profile is
    /// never free for a second launch while its owner is down. The start runs
    /// under the bring-up lock, as every start does. A server that is still
    /// running is left alone, and one this process does not own is never
    /// started: for both, this only checks that the pool answers.
    ///
    /// # Errors
    ///
    /// [`DbError::ServerStart`] if the start fails, naming the data directory
    /// and the server's reason. A port something else took in the meantime
    /// is one cause: the server listens on `127.0.0.1` alone, and a stranger
    /// there ends its start. [`DbError::Sqlx`] if the server does not answer
    /// the pool afterwards.
    pub async fn restart(&mut self) -> Result<(), DbError> {
        if self.owner_lock.is_some()
            && let Some(settings) = self.server_settings().cloned()
        {
            // `build_settings` puts the data directory at `<root_dir>/data`.
            let Some(root_dir) = settings.data_dir.parent() else {
                return Err(DbError::io(
                    &settings.data_dir,
                    io::Error::other("a data directory with no root above it"),
                ));
            };
            let _guard = bring_up_lock(root_dir).await?;
            if !self.server_is_running() {
                tracing::warn!(port = settings.port, "restarting the embedded postgres");
                match self.postgresql.as_mut() {
                    Some(postgresql) => start_server(postgresql).await?,
                    // Adopted: a handle of our own over the settings `stop`
                    // would have used, which from now on is how it stops.
                    None => {
                        let mut postgresql = PostgreSQL::new(settings);
                        if let Err(error) = start_server(&mut postgresql).await {
                            // As in `start_managed`: a handle whose start
                            // failed is never dropped, since its `Drop`
                            // stops whatever `postmaster.pid` names.
                            std::mem::forget(postgresql);
                            return Err(error);
                        }
                        self.postgresql = Some(postgresql);
                        self.stop_settings = None;
                    }
                }
            }
        }
        // Under the pool's acquire timeout, not [`FIRST_CONNECT_TIMEOUT`]:
        // this handle's server has answered it before, and a start above ran
        // `pg_ctl start --wait`, which returns once it accepts connections
        // (#1815).
        drop(self.pool.acquire().await?);
        Ok(())
    }

    /// Whether `postmaster.pid` names a live postmaster for this handle's
    /// data directory, read afresh. False for `existing_url`.
    fn server_is_running(&self) -> bool {
        self.watch().is_some_and(|watch| !watch.is_gone())
    }

    /// The settings this handle's server runs on: the started handle's, or
    /// the adopted server's. `None` for `existing_url`.
    fn server_settings(&self) -> Option<&Settings> {
        self.postgresql
            .as_ref()
            .map(PostgreSQL::settings)
            .or(self.stop_settings.as_ref())
    }

    /// Drop this handle the way a signal does: no `pg_ctl stop`, no pool
    /// close, the ownership lock released. The server keeps running, owned
    /// by nobody -- which is exactly the state a `kill -9` leaves behind.
    ///
    /// Test-only, and the only way to reach the orphan case in-process: a
    /// `kill -9` is not something a `#[tokio::test]` can do to itself, and the
    /// property under test -- that the *next* launch adopts an orphan **and
    /// takes responsibility for it** -- is the one M0 could not deliver.
    ///
    /// The two halves are deliberately different. The lock is **dropped**,
    /// because the kernel is what releases an OS lock when a process dies and
    /// an orphan with a live lock holder would be indistinguishable from a
    /// live sibling's server. Dropping an [`OwnerLock`] unlocks it at once. A
    /// dead process cannot unlock: its lock goes when the last copy of the
    /// descriptor closes, a child's exec at the latest, and a later launch
    /// finds it free either way. The pool and the `PostgreSQL` handle are
    /// **forgotten**, because dropping either one is what would stop the
    /// server (`PostgreSQL::drop` runs `pg_ctl stop -m fast` whenever
    /// `postmaster.pid` exists). Leaking them costs a handful of sockets for
    /// the rest of a short-lived test binary.
    #[cfg(feature = "test-util")]
    pub fn abandon(self) {
        // `PostgreSQL::drop` runs `pg_ctl stop -m fast` whenever
        // `postmaster.pid` exists, which is exactly what this must not do. The
        // owner lock is closed first, because a signal-killed process releases
        // its locks and this is standing in for one.
        let EmbeddedDb {
            pool,
            connector,
            postgresql,
            stop_settings,
            owner_lock,
        } = self;
        drop(owner_lock);
        drop(connector);
        drop(stop_settings);
        std::mem::forget(pool);
        std::mem::forget(postgresql);
    }

    /// Close the pool and shut the server down, if this process owns it.
    ///
    /// The close is **bounded** by [`POOL_CLOSE_TIMEOUT`]: `PgPool::close`
    /// waits for in-flight queries, and waiting for one parked on a remote
    /// system is how Cmd-Q during a sync turns into a thirty-second hang (M0
    /// carry-over). Past the timeout the pool is abandoned and `pg_ctl stop -m
    /// fast` terminates the backends anyway.
    ///
    /// An **adopted** server is stopped too, provided this process holds the
    /// ownership lock. That is the M1 fix for the carry-over: M0 never stopped
    /// one, so a signal-killed run left a postmaster per profile running until
    /// the machine rebooted.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Embedded`] if `pg_ctl stop` fails.
    pub async fn stop(self) -> Result<(), DbError> {
        close_pool_bounded(&self.pool).await;
        if self.owner_lock.is_none() {
            tracing::info!("not this process's server to stop");
            return Ok(());
        }
        // A server that died under the handle (#1779) has nothing to stop,
        // and `pg_ctl stop` would only report that it is not running.
        if !self.server_is_running() {
            tracing::info!("the embedded postgres is not running: nothing to stop");
            return Ok(());
        }
        match &self.postgresql {
            // Started here: the handle knows how to stop it.
            Some(postgresql) => postgresql.stop().await?,
            // Adopted, and ownerless when we found it. Rebuild a handle over
            // the settings this instance is actually connected to and stop it
            // through the same `pg_ctl` path; the ownership lock is what makes
            // that safe.
            None => {
                if let Some(settings) = self.stop_settings.clone() {
                    PostgreSQL::new(settings).stop().await?;
                }
            }
        }
        Ok(())
    }

    /// Stop the server without the shutdown checkpoint, for a data directory
    /// that is about to be deleted.
    ///
    /// [`stop`](Self::stop) is `pg_ctl stop -m fast`, which checkpoints
    /// before the postmaster exits: every buffer is written and every file
    /// touched since the last checkpoint is fsynced, so that the data
    /// directory opens cleanly next time. The gate server `just test` runs
    /// has no next time -- its root is removed the moment it stops -- and by
    /// then it holds one database per test binary, several hundred `create
    /// database`s' worth of files for that checkpoint to sync: 0.84 s of
    /// fast stop against 0.11 s of immediate, measured on 355 databases
    /// (#421). This is `-m immediate`: the
    /// backends are told to quit and the postmaster exits, leaving the
    /// directory in the state a crash would.
    ///
    /// The SysV shared-memory segment the postmaster holds is released
    /// either way. It goes when the postmaster process exits, not with the
    /// checkpoint; only a `SIGKILL`ed postmaster leaves one behind, which is
    /// the case the scratch-root reaper exists for.
    ///
    /// `test-util` only, and only for a server this process owns: a shipped
    /// quit stays on [`stop`](Self::stop), because a data directory the user
    /// keeps must never be left to crash recovery on purpose. A server this
    /// process does not own is left alone, as `stop` leaves it.
    ///
    /// # Errors
    ///
    /// [`DbError::Io`] if `pg_ctl` cannot be found or reports a failure --
    /// `Io` rather than `Embedded`, because the command is run here and not
    /// through the crate's handle. On either error the handle still drops,
    /// and for a server started here that runs `PostgreSQL`'s own `Drop`, a
    /// fast stop if `postmaster.pid` is still there: a second attempt, which
    /// is the right fallback for a failed first one. An adopted server
    /// (`postgresql` is `None`) has no such fallback and stays up; the gate
    /// server never adopts, its root being fresh at every start.
    #[cfg(feature = "test-util")]
    pub async fn discard(self) -> Result<(), DbError> {
        close_pool_bounded(&self.pool).await;
        if self.owner_lock.is_none() {
            tracing::info!("not this process's server to stop");
            return Ok(());
        }
        // An owned server was either started here or adopted, so one of the
        // two holds its settings; the `else` is for the type, not for a case.
        let Some(data_dir) = self
            .server_settings()
            .map(|settings| settings.data_dir.clone())
        else {
            return Ok(());
        };
        // The lookup stats the installation and the stop forks: both under
        // `spawn_blocking`, which a dropped caller does not cut off.
        let stop_dir = data_dir.clone();
        let output = match off_runtime(&data_dir, move || {
            pg_ctl_stop_immediate(&stop_dir)
                .map(|mut pg_ctl| pg_ctl.output())
                // Resolving the installation touches the disk too.
                .ok_or_else(installation_dir)
        })
        .await?
        {
            Ok(output) => output,
            Err(installation) => {
                return Err(DbError::io(
                    installation,
                    io::Error::new(io::ErrorKind::NotFound, format!("no {PG_CTL} under it")),
                ));
            }
        };
        let output = output.map_err(|source| DbError::io(&data_dir, source))?;
        if !output.status.success() {
            return Err(DbError::io(
                &data_dir,
                io::Error::other(format!(
                    "pg_ctl stop -m immediate exited with {}: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr).trim()
                )),
            ));
        }
        // `-w` returned, so `postmaster.pid` is gone and the handle's own
        // `Drop` -- `pg_ctl stop -m fast` whenever that file exists -- has
        // nothing to do.
        Ok(())
    }
}

/// Close `pool`, but not for longer than [`POOL_CLOSE_TIMEOUT`].
///
/// `PgPool::close` waits for every borrowed connection to come back, and one
/// parked on a remote system is how Cmd-Q during a sync turns into a
/// thirty-second hang (M0 carry-over). Past the timeout the pool is left to
/// the server's stop, which terminates the backends anyway.
async fn close_pool_bounded(pool: &PgPool) {
    if tokio::time::timeout(POOL_CLOSE_TIMEOUT, pool.close())
        .await
        .is_err()
    {
        tracing::warn!(
            "connections were still busy after {}s: stopping the server anyway",
            POOL_CLOSE_TIMEOUT.as_secs()
        );
    }
}

/// `pg_ctl stop -m immediate -w` against `data_dir`, or `None` when the
/// binaries are not where [`installation_dir`] says.
///
/// One builder for the two places a disposable server is stopped: the
/// scratch-root reaper in `test_util`, which finds a postmaster a dead test
/// binary left behind, and [`EmbeddedDb::discard`]. Both are stopping a
/// server whose files are about to be removed, so neither wants the
/// checkpoint a fast stop pays for. `-w` waits until `postmaster.pid` is
/// gone, which is the moment the directory can be removed under it.
#[cfg(feature = "test-util")]
pub(crate) fn pg_ctl_stop_immediate(data_dir: &Path) -> Option<std::process::Command> {
    let pg_ctl = find_tool(&installation_dir(), PG_CTL)?;
    let mut command = std::process::Command::new(pg_ctl);
    command
        .arg("stop")
        .arg("-D")
        .arg(data_dir)
        .args(["-m", "immediate", "-w"]);
    Some(command)
}

/// Bring a managed server up from `settings`, recovering once from a lock
/// file that provably describes no postmaster of ours.
///
/// The managed half of [`EmbeddedDb::start`], split from it so the test
/// server's start shares the recovery rather than a copy of it.
async fn launch(root_dir: &Path, settings: Settings) -> Result<EmbeddedDb, DbError> {
    let data_dir = settings.data_dir.clone();

    match start_managed(root_dir, settings.clone()).await? {
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
            let locked = data_dir.clone();
            off_runtime(&data_dir, move || clear_lock(&locked)).await??;
            match start_managed(root_dir, settings).await? {
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

/// Name of the file whose OS lock marks "this process stops the server".
///
/// A *different* lock from [`BRING_UP_LOCK`], and held for a different length
/// of time: bring-up is held for one `initdb`+start and released; ownership is
/// held for the process's whole life. The kernel releases it however the
/// process dies, which is the property the whole design rests on -- a server
/// orphaned by a `kill -9` has no lock holder, so the next launch can adopt it
/// *and take responsibility for it*.
const OWNER_LOCK: &str = ".owner.lock";

/// The ownership lock, held by this process until this is dropped.
///
/// Dropping it **unlocks** the file before closing it (#1802). Closing alone
/// does not release an OS lock while another copy of the descriptor is
/// open, and every process spawned holds such a copy from its fork to its
/// exec: a release that closed the file while another thread was spawning
/// -- `pg_ctl`, `git`, anything -- left the lock held for that long, and a
/// launch in that instant was refused naming this process. The unlock
/// releases the lock itself, whoever still holds a copy of the descriptor.
/// A process that dies cannot unlock, and its lock goes with the last copy,
/// a child's exec at the latest.
struct OwnerLock(File);

impl Drop for OwnerLock {
    fn drop(&mut self) {
        // Nothing to report to: the close that follows releases it anyway,
        // as it did before #1802, only later.
        let _ = self.0.unlock();
    }
}

/// Who is responsible for stopping the server serving a `root_dir`.
enum Ownership {
    /// This process: the lock is held for as long as the [`OwnerLock`] is.
    Ours(OwnerLock),
    /// A live process holds the lock. `by` is the pid it wrote into the
    /// file, `None` when it has not written one yet or it cannot be read.
    Held { by: Option<u32> },
}

/// Claim responsibility for the server serving `root_dir`, if nobody else has.
///
/// Non-blocking on purpose: a live sibling holding this lock is the answer --
/// the start is refused -- not a delay. [`Ownership::Held`] means "somebody
/// else owns it", which is exactly when stopping the server would take a
/// running instance's database down with it.
///
/// The winner writes its pid into the file, so a refused second launch can
/// name the window to quit (ruling B2). The file is opened without truncating
/// it, since a loser that truncated it would erase the name it is about to
/// read. Writing the pid is best-effort: a lock whose holder is not named
/// still excludes every other process.
fn claim_ownership(root_dir: &Path) -> Result<Ownership, DbError> {
    let path = root_dir.join(OWNER_LOCK);
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|source| DbError::io(&path, source))?;
    match file.try_lock() {
        Ok(()) => {
            let named = file
                .set_len(0)
                .and_then(|()| file.rewind())
                .and_then(|()| writeln!(file, "{}", std::process::id()));
            if let Err(error) = named {
                tracing::warn!(
                    %error,
                    lock = %path.display(),
                    "could not write our pid into the ownership lock"
                );
            }
            Ok(Ownership::Ours(OwnerLock(file)))
        }
        Err(TryLockError::WouldBlock) => {
            let mut contents = String::new();
            let by = file
                .read_to_string(&mut contents)
                .ok()
                .and_then(|_| contents.trim().parse::<u32>().ok());
            tracing::info!(?by, "another knobas instance owns this server");
            Ok(Ownership::Held { by })
        }
        Err(TryLockError::Error(source)) => Err(DbError::io(&path, source)),
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
    /// `serving` -- a PostgreSQL serving some other data directory or
    /// refusing our password, or something that does not speak PostgreSQL at
    /// all. Either way no postmaster of ours holds this
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
///
/// The superuser password is read **here**, under that lock, and deliberately
/// not by [`build_settings`]: see [`adopt_recorded_password`].
///
/// **Ownership is claimed first**, before anything on disk is read or
/// changed. A live process holding the lock is a knobas that has this profile
/// open, and the start is refused naming its pid (ruling B2, spec #1619): no
/// second window borrows a server whose owner's quit would stop it under the
/// borrower. The kernel releases the lock however its holder dies, so a held
/// lock always means a live holder, whatever state its server is in.
///
/// Under both locks, a data directory an interrupted `initdb` left is
/// removed and a finished one an older build made is marked
/// ([`recover_or_mark`]), an uninitialised one is initialised by knobas's
/// own `initdb` and marked ([`initialise`]), and after `setup()`
/// the password file and the data directory are kept to their owner
/// ([`keep_to_owner`]).
async fn start_managed(root_dir: &Path, mut settings: Settings) -> Result<Started, DbError> {
    let _guard = bring_up_lock(root_dir).await?;
    let data_dir = settings.data_dir.clone();
    let root = root_dir.to_path_buf();
    let claimed = off_runtime(&root, {
        let root = root.clone();
        move || claim_ownership(&root)
    })
    .await??;
    let owner_lock = match claimed {
        Ownership::Ours(lock) => lock,
        Ownership::Held { by } => {
            let holder = by.map_or_else(
                || "another knobas process".to_owned(),
                |pid| format!("knobas process {pid}"),
            );
            return Err(DbError::AlreadyRunning {
                data_dir,
                reason: format!("{holder} has this profile open; quit that window first"),
            });
        }
    };
    // Each step reads or writes the data directory, so each runs under
    // `spawn_blocking`, in the order it ran before; the bring-up lock and the
    // ownership lock are held across them.
    settings = off_runtime(&data_dir, move || {
        adopt_recorded_password(&mut settings).map(|()| settings)
    })
    .await??;
    let recovering = data_dir.clone();
    off_runtime(&data_dir, move || recover_or_mark(&recovering)).await??;

    // `PostgreSQL::new` consumes the settings, and the adoption path needs
    // them after the handle is gone.
    let adoption = settings.clone();
    let mut postgresql = PostgreSQL::new(settings);

    // One yield before the work starts, so a *warm* install interleaves the
    // way a cold one does. On a machine without the binaries, `setup()`
    // downloads and unpacks PostgreSQL and suspends here for seconds; with
    // them already present it runs all the way to `initdb` without suspending
    // once, and a second launch on the same profile is then never polled until
    // the first has finished. That difference costs nothing in production and
    // decides everything in a test:
    // `two_concurrent_first_launches_serialise_instead_of_racing` only
    // exercises the contended path if both launches are really in flight, and
    // without this it passes for the wrong reason on a warm machine while CI
    // -- which re-downloads whenever this file changes -- runs the cold one.
    tokio::task::yield_now().await;

    initialise(root_dir, &adoption).await?;
    postgresql.setup().await?;
    let owned = adoption.clone();
    off_runtime(&data_dir, move || keep_to_owner(&owned)).await?;

    if let Err(error) = start_server(&mut postgresql).await {
        // `PostgreSQL::drop` runs `pg_ctl stop -m fast` whenever
        // `postmaster.pid` merely *exists* -- it has no idea whether this
        // handle is the one that started that server. So every arm below that
        // leaves a lock file standing forgets the handle rather than dropping
        // it: it owns no OS resource, only paths and strings, so the leak is a
        // few hundred bytes for the rest of the process -- a trade worth
        // making against shutting down a database somebody else is using.
        let inspecting = data_dir.clone();
        let lock = off_runtime(&data_dir, move || inspect_lock(&inspecting)).await??;
        match lock {
            // A hard-killed process (crash, `kill -9`, laptop shutdown)
            // leaves `postmaster.pid` behind and PostgreSQL then refuses to
            // start. The lock is gone now, so the start is worth retrying.
            Lock::Cleared => {
                #[cfg(feature = "test-util")]
                seam::stale_lock_cleared();
                if let Err(retry) = start_server(&mut postgresql).await {
                    // The lock file this handle would stop on is no longer
                    // the one it cleared: a sibling that lost the same race
                    // may have started its server in between, and `pg_ctl
                    // stop` against *that* pid file kills a database this
                    // process never started.
                    std::mem::forget(postgresql);
                    return Err(retry);
                }
            }
            // No lock at all: the start failed for some other reason, and
            // that reason is the one worth reporting.
            Lock::Absent => return Err(error),
            Lock::Live { port } => {
                std::mem::forget(postgresql);
                return adopt(adoption, port, owner_lock).await;
            }
        }
    }

    if !postgresql.database_exists(DATABASE_NAME).await? {
        postgresql.create_database(DATABASE_NAME).await?;
    }

    let url = postgresql.settings().url(DATABASE_NAME);
    let connector = Connector::parse(&url)?;
    let pool = connector.app_pool().await?;

    tracing::info!(port = postgresql.settings().port, "embedded postgres ready");

    Ok(Started::Ready(Box::new(EmbeddedDb {
        pool,
        connector,
        postgresql: Some(postgresql),
        stop_settings: None,
        owner_lock: Some(owner_lock),
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
/// **The adopter owns what it adopts.** M0 adopted without owning: `stop` left
/// the server up, so a server orphaned by a signal-kill was never stopped by
/// anybody -- "from then until the machine reboots there is one PostgreSQL
/// running per profile". M1 let the adopter take the ownership lock when it
/// was free and borrow when a live sibling held it. The borrower had no way
/// back when the owner quit and stopped the server under it (CORE-3), so a
/// live owner now refuses the second launch before this is reached
/// ([`start_managed`], ruling B2). What gets here is an **orphan**: the lock
/// was free, `owner_lock` is it, and the clean quit of this process stops the
/// server.
///
/// An orphan running with `fsync` off, which every build before #1896
/// started, is stopped and started again on `settings` ([`restart_durable`]),
/// so the app never goes on writing to it without syncing.
///
/// # A stranger on the port
///
/// An answer on the recorded port that is not a server of ours -- not
/// PostgreSQL, or a PostgreSQL that refuses our password -- clears the lock
/// **whatever the recorded pid is doing** (#1775, reversing that part of
/// ruling B5). The pid used to be the last witness: alive, the lock might
/// still be owned. Two facts retired it:
///
/// * **Ownership comes first.** `owner_lock` is held here, so no knobas has
///   this profile open: a live sibling was refused before this was reached.
///   What a live recorded pid can still be is a pid recycled after a reboot,
///   which no knobas window can ever clear, or an orphaned postmaster of
///   ours whose knobas died.
/// * **A postmaster of ours on that port answers there itself.** It listens
///   on `127.0.0.1` alone ([`build_settings`]), and binding is all or
///   nothing: with a stranger already on the port it exits with *could not
///   create any TCP/IP sockets*, and once it is bound no stranger can take
///   `127.0.0.1` from it, while a wildcard listener loses loopback
///   connections to it. So a stranger's answer means no postmaster of ours
///   is bound there.
///
/// What is left is a postmaster of ours that is not bound: one in the
/// instant between writing `postmaster.pid` and binding (it then exits, as
/// above), or an orphan an older build started on `localhost`, which a
/// stranger on `127.0.0.1` leaves on `::1` alone. Clearing the lock under
/// that one still puts no second server on the files: PostgreSQL's
/// shared-memory interlock refuses the new postmaster (*pre-existing shared
/// memory block is still in use*), and the orphan, finding its lock file
/// gone, shuts itself down within a minute, after which the next start is a
/// clean one. That is what deleting `postmaster.pid` by hand, the old
/// refusal's advice, did too. The evidence is recorded in
/// `docs/decisions/2026-10-unattended-rulings.md` under #1775.
///
/// # Errors
///
/// [`DbError::AlreadyRunning`] if the server could not be reached as ours, or
/// answered as a PostgreSQL and then could not be queried, or if the
/// [`DATABASE_NAME`] database cannot be created on it. Two outcomes are *not*
/// errors, and both come back as [`Started::StaleLock`] so the caller clears
/// the lock and starts normally:
///
/// * a PostgreSQL that turns out to serve a **different** data directory, and
/// * a listener on the recorded port that is not a server of ours -- it does
///   not speak PostgreSQL at all, or it refuses our password.
///
/// The second is the recycled-ephemeral-port case: some unrelated program, or
/// another PostgreSQL (a second profile's, a test's scratch server), now owns
/// the port a dead postmaster recorded. Reporting that as a conflict is both
/// false and unrecoverable -- the user has no way to make the stranger release
/// the port, and knobas would refuse to launch until they found and deleted
/// `postmaster.pid` by hand. A postmaster of ours accepts our password, so a
/// refusal is the same evidence a non-PostgreSQL answer is (ruling B5).
///
/// PostgreSQL removes a lock whose process is gone by itself when it starts,
/// so a dead pid gets here only when the start failed for another reason. The
/// recovery then hands that reason back instead of a conflict that does not
/// exist.
async fn adopt(
    mut settings: Settings,
    port: u16,
    owner_lock: OwnerLock,
) -> Result<Started, DbError> {
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
        Err(source) => {
            let stranger = if is_wire_level(&source) {
                Some("something that does not speak postgres")
            } else if refuses_our_credentials(&source) {
                Some("a postgres that refuses our password")
            } else {
                None
            };
            return match stranger {
                // Whatever the recorded pid is doing (#1775): see `adopt`'s
                // docs, *A stranger on the port*.
                Some(stranger) => {
                    tracing::warn!(
                        port,
                        %source,
                        "the recorded port is held by {stranger}, and no knobas owns this \
                         profile: the lock is stale"
                    );
                    Ok(Started::StaleLock {
                        port,
                        serving: format!("{stranger} ({source})"),
                    })
                }
                None => Err(unreachable(format!(
                    "cannot connect on port {port}: {source}"
                ))),
            };
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
    let (served_dir, ours) = (PathBuf::from(&serving.0), data_dir.clone());
    if off_runtime(&data_dir, move || provably_different(&served_dir, &ours)).await? {
        // A mismatch clears the lock **whatever the recorded pid is doing**,
        // as the stranger arms above do since #1775. Ruled first for this
        // arm, and here is the argument:
        //
        // A postmaster serves exactly one data directory for its whole life.
        // This one answered on the port our lock file names and reported some
        // *other* directory, so it is not the postmaster our lock describes --
        // and no postmaster of ours can be listening there either, since that
        // port is taken. Whatever the recorded pid is, it is not a live
        // postmaster holding this directory.
        //
        // Clearing endangers nothing: the foreign server does not hold our
        // directory, and nothing here ever stops it -- the lock file we remove
        // is our own, and it is provably stale.
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
        if let Err(source) = sqlx::query(sqlx::AssertSqlSafe(format!(
            r#"create database "{DATABASE_NAME}""#
        )))
        .execute(&mut admin)
        .await
        {
            // 42P04 = duplicate_database. `create database` has no `if not
            // exists`, so the lookup above is a check-then-act. No second
            // knobas runs it beside this one (both the bring-up and the
            // ownership lock are held here), but anything else on the server
            // can create the database in between, and the outcome wanted --
            // a database that exists -- is then exactly what happened.
            let raced = source
                .as_database_error()
                .and_then(sqlx::error::DatabaseError::code)
                .is_some_and(|code| code == "42P04");
            if !raced {
                return Err(unreachable(format!(
                    "cannot create the {DATABASE_NAME} database: {source}"
                )));
            }
            tracing::info!("the {DATABASE_NAME} database appeared before this adoption created it");
        }
    }
    let fsync: (String,) = sqlx::query_as("select current_setting('fsync')")
        .fetch_one(&mut admin)
        .await
        .map_err(|source| unreachable(format!("cannot query it: {source}")))?;
    let _ = admin.close().await;

    // An orphan a build before #1896 started runs with `fsync` off: that
    // build passed `-F` alone. A command-line setting outranks a reload, so
    // only a start of our own gives it the `fsync=on` this build passes.
    // Stopping it is safe for the reason `stop` stopping an adopted server
    // is: `owner_lock` is held, so no knobas has this profile open.
    let postgresql = if fsync.0 == "on" {
        None
    } else {
        restart_durable(&settings).await?
    };
    let stop_settings = postgresql.is_none().then_some(settings);

    let connector = Connector::parse(&url)?;
    let pool = connector.app_pool().await.map_err(|source| {
        unreachable(format!(
            "cannot connect to {DATABASE_NAME} on port {port}: {source}"
        ))
    })?;

    tracing::warn!(
        port,
        "adopting the orphaned postgres serving this data directory, and its stop"
    );
    Ok(Started::Ready(Box::new(EmbeddedDb {
        pool,
        connector,
        postgresql,
        // `settings.port` was set to the recorded port above, and
        // `adopt_recorded_password` gave it the password `initdb` burned in,
        // so this is a handle over the server we are actually connected to.
        stop_settings,
        owner_lock: Some(owner_lock),
    })))
}

/// Stop the orphan [`adopt`] found running with `fsync` off and start it
/// again on `settings`, which pass `fsync=on` (#1896), on the port it had.
///
/// `Some` handle when the server runs durably now, which then stops it as a
/// server started here does. `None` when the stop failed: the orphan is
/// adopted as it is, with `fsync` off until the next quit, since a launch
/// refused over a setting the previous build chose would cost the user more
/// than the setting does. A failed start after a clean stop is an error, as
/// any failed start is.
async fn restart_durable(settings: &Settings) -> Result<Option<PostgreSQL>, DbError> {
    tracing::warn!(
        port = settings.port,
        "the orphaned postgres runs with fsync off: starting it again with fsync on"
    );
    let orphan = PostgreSQL::new(settings.clone());
    if let Err(error) = orphan.stop().await {
        tracing::warn!(%error, "could not stop it: adopting it with fsync off");
        // Its `Drop` would run `pg_ctl stop` again, under a server the
        // caller goes on to connect to.
        std::mem::forget(orphan);
        return Ok(None);
    }
    drop(orphan);
    let mut postgresql = PostgreSQL::new(settings.clone());
    if let Err(error) = start_server(&mut postgresql).await {
        // As in `start_managed`: a handle whose start failed is never
        // dropped, since its `Drop` stops whatever `postmaster.pid` names.
        std::mem::forget(postgresql);
        return Err(error);
    }
    Ok(Some(postgresql))
}

/// Whether two paths are **provably different** directories.
///
/// Canonicalised first: macOS hands out `/var/folders/...` paths that
/// PostgreSQL reports back as `/private/var/folders/...`, so a raw string
/// comparison would call an adopted server a stranger every time.
///
/// Deliberately **not** the negation of "same directory". Its caller uses it to
/// decide whether to clear a lock file -- a destructive act -- and a path that
/// cannot be resolved is not evidence of anything. Both sides must canonicalise
/// for a mismatch to count; anything else keeps the lock.
fn provably_different(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left != right,
        _ => false,
    }
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
/// very default this returns. It used to be `#[cfg(feature = "test-util")]`,
/// because `test_util`'s reaper (which needs the `pg_ctl` under it) was the
/// only reader. [`backup`](crate::backup) made it a **shipping** path: the
/// `pg_dump` and `pg_restore` a backup runs are the ones that came out of the
/// same archive as the server, and this is where they are.
pub(crate) fn installation_dir() -> PathBuf {
    fresh_settings().installation_dir
}

/// One of the PostgreSQL binaries under `installation_dir`, if it is there.
///
/// Two shapes, because the archive is unpacked lazily: before the first
/// extraction the directory does not exist at all, and afterwards the binaries
/// sit under a *version* subdirectory (`<install>/18.6.0/bin/pg_dump`). The
/// crate's own default has no version segment (`<install>/bin/pg_dump`), so
/// both are tried.
///
/// **The pinned version first, and any other only as a fallback.** The
/// installation directory is shared with every other `postgresql_embedded`
/// application on the machine, so it can hold several versions -- and
/// `pg_dump` refuses to dump a server newer than itself. Taking whichever
/// subdirectory `read_dir` happened to yield first would make a backup fail
/// (or, worse, succeed against the wrong server) for a reason nothing in
/// knobas would explain.
pub(crate) fn find_tool(installation_dir: &Path, name: &str) -> Option<PathBuf> {
    // `PG_VERSION_REQ` is a semver *requirement* (`=18.6.0`); the directory is
    // named after the version alone.
    let pinned = installation_dir
        .join(PG_VERSION_REQ.trim_start_matches(['=', '^', '~', ' ']))
        .join("bin")
        .join(name);
    if pinned.is_file() {
        return Some(pinned);
    }
    let direct = installation_dir.join("bin").join(name);
    if direct.is_file() {
        return Some(direct);
    }
    std::fs::read_dir(installation_dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path().join("bin").join(name))
        .find(|path| path.is_file())
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
    // `127.0.0.1` alone, the one address every client here dials (#1775, and
    // ADR-0001's "TCP on 127.0.0.1 only").
    // PostgreSQL's default is `localhost`, which is `::1` and `127.0.0.1`,
    // and a bind failure on one of two addresses is only a warning: with a
    // stranger on `127.0.0.1:<port>` the postmaster came up on `::1` and our
    // clients reached the stranger. On one address a failed bind ends the
    // start, so a postmaster of ours is either the one answering on its
    // recorded port or not running -- what `adopt` relies on. Passed as `-c`
    // at every start, so a data directory an older build made gets it too.
    settings
        .configuration
        .insert("listen_addresses".to_owned(), HOST.to_owned());
    // Durable writes (#1896). `postgresql_embedded` 0.21 starts every server
    // as `pg_ctl start -o "-F -p <port> -c <key>=<value>..."`, and `-F` is
    // `fsync=off`: a crash or a power loss on such a server can leave the
    // data directory corrupt. The postmaster reads its arguments in order and
    // the last setting of a name wins, so the `-c fsync=on` this puts after
    // `-F` is the one the server runs with. The other two are PostgreSQL's
    // defaults, set here so no `postgresql.conf` can lower them.
    for (name, value) in DURABLE_SETTINGS {
        settings
            .configuration
            .insert(name.to_owned(), value.to_owned());
    }
    settings.timeout = Some(COMMAND_TIMEOUT);

    // The password `Settings::new()` invented is left in place here. Reading
    // the recorded one is `adopt_recorded_password`'s job, and it has to
    // happen under the bring-up lock -- see there.

    Ok(settings)
}

/// Replace the invented superuser password with the one `initdb` recorded.
///
/// `Settings::new()` invents a fresh random password on every call, but
/// `initdb` burned the *first* one into the data directory: without this,
/// every restart authenticates with the wrong password.
///
/// **Called under [`bring_up_lock`], never before it.** On a genuine first
/// launch `.pgpass` does not exist yet, and reading it early is how the loser
/// of a two-launch race keeps the password it invented for itself: it waits
/// for the lock correctly, finds the directory already initialised, skips
/// `initialize()` (which writes `.pgpass` only when the file is absent), and
/// then fails to authenticate against the winner's server -- the same
/// unrecoverable lockout the rest of this module exists to prevent. Under the
/// lock the file is either already there (adopt it) or genuinely absent (keep
/// the invented one, and `initialize()` records it).
fn adopt_recorded_password(settings: &mut Settings) -> Result<(), DbError> {
    if let Some(password) = read_password(&settings.password_file)? {
        settings.password = password;
    }
    Ok(())
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

/// Remove a data directory an interrupted `initdb` left (#1637, #1819), and
/// mark a finished one an older build made.
///
/// **Called under [`bring_up_lock`] and the ownership lock, before
/// anything reads the directory.** `postgresql_embedded` takes
/// `postgresql.conf` alone for "initialised", so it skips `initdb` on what a
/// killed one left, and the launch fails on it every time until the reader
/// deletes the directory by hand. What [`shape`] finds decides:
///
/// - **Marked**: knobas's `initdb` finished here ([`INITDB_FINISHED`]). Never
///   removed, whatever the directory lacks later.
/// - **Holds a database**, no marker: a finished `initdb` an older build ran,
///   every file it writes still there. Marked now and never removed.
/// - **Unfinished**: removed, so `initdb` runs again.
/// - **Unrecognised** or **not initialised**: left as it is.
///
/// A data directory that is a symlink loses the link alone (`remove_dir_all`
/// follows no symlink): what it points at is left as it was, and `initdb`
/// makes a directory in its place. Evidence:
/// `docs/decisions/2026-10-unattended-rulings.md`, #1637, #1819, #1985 and
/// #2007.
fn recover_or_mark(data_dir: &Path) -> Result<(), DbError> {
    match shape(data_dir) {
        Shape::Marked | Shape::NotInitialised => Ok(()),
        Shape::Unrecognised => {
            tracing::warn!(
                data_dir = %data_dir.display(),
                "this data directory is not one a finished initdb made, but may hold a \
                 database or a server ran on it: keeping it as it is"
            );
            Ok(())
        }
        Shape::HoldsDatabase => {
            mark_finished(data_dir);
            Ok(())
        }
        Shape::Unfinished if served(data_dir) => {
            tracing::warn!(
                data_dir = %data_dir.display(),
                "an interrupted initdb left this data directory, but a server answers on \
                 the port its postmaster.pid records: keeping it"
            );
            Ok(())
        }
        Shape::Unfinished => {
            tracing::warn!(
                data_dir = %data_dir.display(),
                "this data directory holds no database, as an interrupted initdb leaves \
                 it: removing it so initdb runs again"
            );
            remove_unfinished(data_dir)
        }
    }
}

/// Remove an unfinished `data_dir`, `PG_VERSION` first (#2007).
/// `remove_dir_all` removes entries in `readdir` order, so a launch killed
/// partway through could leave `PG_VERSION` with `base/` gone, which
/// [`shape`] reads as a database on a volume not mounted. Without
/// `PG_VERSION`, whatever the removal leaves reads as unfinished. A data
/// directory that is a symlink loses the link alone, and the `PG_VERSION`
/// in what it points at stays.
fn remove_unfinished(data_dir: &Path) -> Result<(), DbError> {
    let linked = std::fs::symlink_metadata(data_dir).is_ok_and(|meta| meta.is_symlink());
    if !linked {
        let version = data_dir.join("PG_VERSION");
        match std::fs::remove_file(&version) {
            Err(source) if source.kind() != io::ErrorKind::NotFound => {
                return Err(DbError::io(&version, source));
            }
            _ => {}
        }
    }
    std::fs::remove_dir_all(data_dir).map_err(|source| DbError::io(data_dir, source))
}

/// What [`recover_or_mark`] found in a data directory.
#[derive(Debug, PartialEq, Eq)]
enum Shape {
    /// Knobas's `initdb` finished here ([`INITDB_FINISHED`]).
    Marked,
    /// Nothing at all, or an empty directory.
    NotInitialised,
    /// Not what a finished `initdb` leaves, and a database in it or one
    /// that may be, or a server ran on it: no launch removes it or marks it.
    Unrecognised,
    /// No marker, every file `initdb` writes there, and it holds a database
    /// or may: a finished `initdb` an older build ran, or one whose databases
    /// cannot be listed.
    HoldsDatabase,
    /// What an interrupted `initdb` left: it holds no database.
    Unfinished,
}

/// Read `data_dir`'s shape. **A doubt about a database keeps the
/// directory**: an entry that cannot be examined counts as present, so an
/// unreadable marker keeps it, and a `base/` or `pg_tblspc/` that cannot be
/// listed counts as holding a database. With `PG_VERSION`, so does a missing
/// one.
///
/// Without `PG_VERSION` and with `postgresql.conf`, it is unfinished (#1637):
/// `initdb` writes `PG_VERSION` first and its cleanup removes it first.
///
/// Without either, and not empty, it is unfinished when it holds no database
/// (#1985): nothing under `base/` but `1`, `4` and `5`, and nothing under
/// `pg_tblspc/`, either of which may be missing. An `initdb` killed before it
/// wrote `PG_VERSION` leaves its subdirectories, and one killed during its
/// cleanup once `postgresql.conf` was gone leaves what the cleanup had not
/// reached, `base/5` included. A server never ran on either, so no `knobas`
/// database is in it. Anything else is [`Shape::Unrecognised`].
///
/// With `PG_VERSION`, it is unfinished when it **holds no database**: nothing
/// under `base/` but the templates `1` and `4`, and nothing under
/// `pg_tblspc/`. `initdb` makes `base/5`, the `postgres` database, last of
/// all, so a finished one always has it, and the reader's data lives in the
/// `knobas` database a launch creates after that. Every build has run
/// PostgreSQL 18.6. On a version before 15, where `postgres` and `template0`
/// had no fixed OIDs, a finished cluster still has a directory that is not
/// `1` or `4`.
///
/// With `PG_VERSION` and without one of the other files `initdb` writes
/// ([`INITDB_FILES`]), it is unfinished when it holds no database, `base/5`
/// allowed, and no server ever ran on it: no `postmaster.opts`, which every
/// server start writes and `initdb` never does (#2007). `initdb`'s cleanup
/// removes the top level's files in `readdir` order before any directory, so
/// on a filesystem that lists `PG_VERSION` after another of them (ext4 lists
/// by name hash), a kill during it leaves this. With a server's
/// `postmaster.opts` or a database, it is [`Shape::Unrecognised`]: marking it
/// would keep it from ever being recovered, and `initdb` refuses it or the
/// server cannot start on it either way.
fn shape(data_dir: &Path) -> Shape {
    let present = |name: &str| {
        !std::fs::symlink_metadata(data_dir.join(name))
            .is_err_and(|source| source.kind() == io::ErrorKind::NotFound)
    };
    if present(INITDB_FINISHED) {
        return Shape::Marked;
    }
    // Whether `dir` holds nothing but `allowed`, or `None` when it is not
    // there. Any other error is a doubt: it holds more.
    let holds_only = |dir: &str, allowed: &[&str]| match std::fs::read_dir(data_dir.join(dir)) {
        Ok(entries) => Some(entries.into_iter().all(|entry| {
            entry.is_ok_and(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| allowed.contains(&name))
            })
        })),
        Err(source) if source.kind() == io::ErrorKind::NotFound => None,
        Err(_) => Some(false),
    };
    if !present("PG_VERSION") {
        return if present("postgresql.conf") {
            Shape::Unfinished
        } else if holds_only("", &[]).unwrap_or(true) {
            Shape::NotInitialised
        } else if holds_only("base", &["1", "4", "5"]).unwrap_or(true)
            && holds_only("pg_tblspc", &[]).unwrap_or(true)
        {
            // A missing one holds nothing here: the kill came before
            // `initdb` made it, or after its cleanup removed it.
            Shape::Unfinished
        } else {
            Shape::Unrecognised
        };
    }
    // Even "not found" is a doubt here: `initdb` makes both before
    // `PG_VERSION`, and its cleanup removes them after every file, so a
    // missing one is a link to a volume not mounted, or a directory being
    // copied in, not a killed `initdb`.
    let empty_tablespaces = holds_only("pg_tblspc", &[]).unwrap_or(false);
    if holds_only("base", &["1", "4"]).unwrap_or(false) && empty_tablespaces {
        Shape::Unfinished
    } else if INITDB_FILES.iter().all(|name| present(name)) {
        Shape::HoldsDatabase
    } else if !present("postmaster.opts")
        && holds_only("base", &["1", "4", "5"]).unwrap_or(false)
        && empty_tablespaces
    {
        Shape::Unfinished
    } else {
        Shape::Unrecognised
    }
}

/// The files `initdb` writes at the top of a data directory beside
/// `PG_VERSION`, all of them before it bootstraps. Its cleanup removes them
/// before any directory (#2007).
const INITDB_FILES: [&str; 4] = [
    "postgresql.conf",
    "postgresql.auto.conf",
    "pg_hba.conf",
    "pg_ident.conf",
];

/// Whether `data_dir`'s `postmaster.pid` is a postmaster's whose port
/// something answers on, or cannot be read. A server running on an
/// unfinished directory is one an older build started on it and lost; its
/// files stay under it.
///
/// The lock `initdb`'s own backends write is not a server's: they run
/// standalone, record their pid negated and the default port 5432, and
/// listen on nothing. One left by a power loss would otherwise keep the
/// directory whenever any PostgreSQL answers on 5432.
fn served(data_dir: &Path) -> bool {
    match std::fs::read_to_string(data_dir.join("postmaster.pid")) {
        Ok(contents) => {
            lock_pid(&contents).is_some() && lock_port(&contents).is_some_and(port_answers)
        }
        Err(source) => source.kind() != io::ErrorKind::NotFound,
    }
}

/// Mark `data_dir` as one a finished `initdb` made, so no launch removes it.
/// A failure is logged: the next launch reads the same shape and tries again.
fn mark_finished(data_dir: &Path) {
    let marker = data_dir.join(INITDB_FINISHED);
    let note = "initdb finished in this directory; knobas never removes it (#1819)\n";
    if let Err(error) = std::fs::write(&marker, note) {
        tracing::warn!(%error, marker = %marker.display(), "could not mark the data directory finished");
    }
}

/// The file knobas writes into a data directory once its `initdb` has finished (#1819).
const INITDB_FINISHED: &str = "knobas-initdb-finished";

/// Name of the placeholder data directory [`install`] hands `setup()`.
const INSTALL_ONLY: &str = ".install-only";

/// How long a timed-out `initdb` gets to clean up after `SIGTERM` before
/// it is killed.
const INITDB_GRACE: Duration = Duration::from_secs(10);

/// Initialise `settings.data_dir` with an `initdb` of our own, unless it
/// already has `postgresql.conf`, the crate's mark of an initialised one.
///
/// The crate's `setup()` runs `initdb` too, but with no way to set
/// `kill_on_drop`, so a timed-out `initdb` kept running after knobas had given
/// up (#1637). This runs the crate's command, plus a fixed locale (#2013,
/// [`LIBC_LOCALE`]), and the `setup()` that follows finds `postgresql.conf`
/// and skips its own. A finished `initdb` is marked ([`INITDB_FINISHED`]), so
/// no later launch removes the directory.
///
/// The password file is created here, as the crate would, but readable by
/// its owner alone from the first byte (#1642).
///
/// Both checks use `Path::exists`, which reads an unreadable entry as absent,
/// and both fail safe: `initdb` refuses a data directory that is not empty,
/// and `create_new` refuses a password file that is there.
async fn initialise(root_dir: &Path, settings: &Settings) -> Result<(), DbError> {
    // The crate's own test (`is_initialized`), so `setup()` agrees with it.
    let conf = settings.data_dir.join("postgresql.conf");
    if off_runtime(&conf.clone(), move || conf.exists()).await? {
        return Ok(());
    }
    let binaries = install(root_dir, settings).await?;
    let (file, password) = (settings.password_file.clone(), settings.password.clone());
    off_runtime(&settings.password_file, move || {
        if file.exists() {
            Ok(())
        } else {
            write_password_file(&file, &password)
        }
    })
    .await??;
    let initdb = binaries.join(format!("initdb{}", std::env::consts::EXE_SUFFIX));
    run_initdb(&initdb, settings, COMMAND_TIMEOUT).await?;
    let data_dir = settings.data_dir.clone();
    off_runtime(&settings.data_dir, move || mark_finished(&data_dir)).await?;
    Ok(())
}

/// The file `pg_ctl start -l` appends the server's output to, in the data
/// directory (`postgresql_embedded`'s `start`).
const START_LOG: &str = "start.log";

/// How many lines of a start's log stand for the reason when none of them is
/// a `FATAL` one: all of a short log, the end of a long one.
const REASON_LINES: usize = 5;

/// The severity label of the line that says why a server stopped.
const FATAL: &str = "FATAL:";

/// Start `postgresql`'s server. A failed start is a [`DbError::ServerStart`]
/// naming the data directory and the reason the server gave (#2025).
///
/// Every server start goes through here. The handle is the caller's to forget
/// on a failure: `PostgreSQL::drop` stops whatever `postmaster.pid` names.
async fn start_server(postgresql: &mut PostgreSQL) -> Result<(), DbError> {
    let data_dir = postgresql.settings().data_dir.clone();
    let log = data_dir.join(START_LOG);
    // `pg_ctl` appends, so the log holds every earlier start's lines too.
    let since = tokio::fs::metadata(&log).await.map_or(0, |meta| meta.len());
    let Err(source) = postgresql.start().await else {
        return Ok(());
    };
    let written = tokio::fs::read(&log).await.unwrap_or_default();
    let reason = start_reason(&written, since).unwrap_or_else(|| source.to_string());
    Err(DbError::ServerStart {
        data_dir,
        reason,
        source,
    })
}

/// What a start's log says went wrong: the last `FATAL` line the start wrote
/// from that line's `FATAL:` on, or else its last [`REASON_LINES`] lines
/// joined. `None` when the start wrote nothing.
///
/// `log` is the whole file and `since` its length before the start, so an
/// earlier start's `FATAL` is never this one's reason. A log shorter than
/// `since` was replaced in between and is read whole.
fn start_reason(log: &[u8], since: u64) -> Option<String> {
    let from = usize::try_from(since).unwrap_or(usize::MAX);
    let written = String::from_utf8_lossy(log.get(from..).unwrap_or(log));
    let one_line = |line: &str| line.split_whitespace().collect::<Vec<_>>().join(" ");
    if let Some(fatal) = written.lines().rev().find_map(|line| {
        line.find(FATAL)
            .map(|at| &line[at + FATAL.len()..])
            .filter(|reason| !reason.trim().is_empty())
    }) {
        return Some(one_line(fatal));
    }
    let lines: Vec<String> = written
        .lines()
        .map(one_line)
        .filter(|line| !line.is_empty())
        .collect();
    let tail = &lines[lines.len().saturating_sub(REASON_LINES)..];
    // A line ending in a comma is a sentence the postmaster wrapped, so it
    // reads on into the next line rather than being cut off with `; `.
    (!tail.is_empty()).then(|| tail.join("; ").replace(",; ", ", "))
}

/// Run `work` on a blocking thread and return its answer. A dropped caller
/// does not cut the work off: it finishes and its answer is discarded. A task
/// that panicked or was cancelled is a [`DbError::Io`] on `path`.
async fn off_runtime<T: Send + 'static>(
    path: &Path,
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, DbError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| DbError::io(path, io::Error::other(error)))
}

/// Install the PostgreSQL binaries if they are not there yet, and return
/// their `bin` directory.
///
/// The install is the crate's own `setup()`, so a cold machine downloads and
/// unpacks exactly as before. It is pointed at a placeholder data directory
/// holding an empty `postgresql.conf`, which `setup()` takes for initialised,
/// so it installs and runs no `initdb`. `PostgreSQL::new` and `setup()` are
/// also what resolve the version directory the binaries live in.
async fn install(root_dir: &Path, settings: &Settings) -> Result<PathBuf, DbError> {
    let placeholder = root_dir.join(INSTALL_ONLY);
    tokio::fs::create_dir_all(&placeholder)
        .await
        .map_err(|source| DbError::io(&placeholder, source))?;
    let conf = placeholder.join("postgresql.conf");
    tokio::fs::write(&conf, "")
        .await
        .map_err(|source| DbError::io(&conf, source))?;

    let mut install_settings = settings.clone();
    install_settings.data_dir.clone_from(&placeholder);
    let mut postgresql = PostgreSQL::new(install_settings);
    let installed = postgresql.setup().await;
    let binaries = postgresql.settings().binary_dir();
    // Nothing to stop: the placeholder has no `postmaster.pid`.
    drop(postgresql);
    if let Err(error) = tokio::fs::remove_dir_all(&placeholder).await {
        tracing::warn!(%error, placeholder = %placeholder.display(), "could not remove the install placeholder");
    }
    installed?;
    Ok(binaries)
}

/// Write the superuser password `initdb --pwfile` reads, readable by its
/// owner alone from creation on (#1642). Refuses a file that is already there.
fn write_password_file(path: &Path, password: &str) -> Result<(), DbError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options
        .open(path)
        .and_then(|mut file| file.write_all(password.as_bytes()))
        .map_err(|source| DbError::io(path, source))
}

/// The C library's collation and ctype every cluster knobas creates gets,
/// beside the builtin provider's `C.UTF-8` ([`run_initdb`], #2013).
///
/// Left unset, `initdb` takes the locale from the environment, and an app
/// started from Finder inherits launchd's, which has no `LANG`: the cluster
/// came out libc `C`, where `lower()` and `to_tsvector` leave `Ü` alone and
/// `pg_trgm` drops it. The builtin provider is the database's default
/// collation: Unicode case folding on every platform, and titles, names and
/// paths sort by code point (Björn, triage of #2013; ADR-0027). `pg_trgm`
/// still reads the C library's ctype to tell a letter from a separator, so
/// that is fixed too: `C.UTF-8` where the C library has it, and on Windows,
/// which has none, `en-US`, a name every Windows 10 and 11 install has.
///
/// The other categories (`lc_messages`, `lc_numeric`, ...) are server
/// settings, not fixed per database, and still come from the environment.
const LIBC_LOCALE: &str = if cfg!(windows) { "en-US" } else { "C.UTF-8" };

/// Run `initdb` into `settings.data_dir`, as `postgresql_embedded` would, with
/// `kill_on_drop` and a bounded wait.
///
/// **A drop kills it**: the future dropped mid-`initdb` (the bring-up given
/// up, its runtime shut down) takes `initdb` with it, rather than leaving it
/// writing into a directory nobody is waiting on.
///
/// **A timeout terminates it first**: `SIGTERM` makes `initdb` remove what it
/// made, the whole data directory when it created it, so the next launch
/// starts from nothing. `SIGKILL`, which `kill_on_drop` sends, leaves a
/// directory with no marker, with `PG_VERSION` or, when it cuts that cleanup
/// short, without (#1985); the next launch's [`recover_or_mark`] removes
/// either. It is the fallback once [`INITDB_GRACE`] has passed.
///
/// A failure reads as the crate's did: `DatabaseInitializationError` with the
/// command's output, or *deadline has elapsed*.
async fn run_initdb(initdb: &Path, settings: &Settings, limit: Duration) -> Result<(), DbError> {
    let failed = |message: String| {
        DbError::Embedded(postgresql_embedded::Error::DatabaseInitializationError(
            message,
        ))
    };
    let mut command = tokio::process::Command::new(initdb);
    // The crate's `CREATE_NO_WINDOW`: no console flashes up over the app.
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    command
        .arg("--auth")
        .arg("password")
        .arg("--pgdata")
        .arg(&settings.data_dir)
        .arg("--encoding")
        .arg("UTF8")
        .arg("--locale-provider=builtin")
        .arg("--builtin-locale=C.UTF-8")
        .arg(format!("--lc-collate={LIBC_LOCALE}"))
        .arg(format!("--lc-ctype={LIBC_LOCALE}"))
        .arg("--pwfile")
        .arg(&settings.password_file)
        .arg("--username")
        .arg(postgresql_embedded::BOOTSTRAP_SUPERUSER)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    // The fork runs under `spawn_blocking`; a caller dropped meanwhile gets
    // the child dropped with the result, and `kill_on_drop` ends it.
    let child = tokio::task::spawn_blocking(move || command.spawn())
        .await
        .map_err(|error| DbError::io(initdb, io::Error::other(error)))?
        .map_err(|source| DbError::io(initdb, source))?;
    let pid = child.id();
    let mut output = std::pin::pin!(child.wait_with_output());
    let output = match tokio::time::timeout(limit, &mut output).await {
        Ok(output) => output.map_err(|source| DbError::io(initdb, source))?,
        Err(_) => {
            tracing::warn!(?pid, "initdb timed out: stopping it");
            // Its own cleanup runs before it exits. A drop past the grace
            // kills it.
            let sent = match pid {
                Some(pid) => tokio::task::spawn_blocking(move || terminate(pid))
                    .await
                    .unwrap_or(false),
                None => false,
            };
            if sent {
                let _ = tokio::time::timeout(INITDB_GRACE, &mut output).await;
            }
            return Err(failed("deadline has elapsed".to_owned()));
        }
    };
    if output.status.success() {
        Ok(())
    } else {
        Err(failed(format!(
            "Command error: stdout={}; stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )))
    }
}

/// Ask `pid` to terminate, and say whether the signal went out. Best-effort:
/// the caller's drop kills it anyway.
#[cfg(unix)]
fn terminate(pid: u32) -> bool {
    std::process::Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// No signal but the kill a drop sends, so there is nothing to wait for.
#[cfg(not(unix))]
fn terminate(_pid: u32) -> bool {
    false
}

/// Keep the password file and the data directory to their owner (#1642).
///
/// The password file is the whole key to the mirror: the server listens on
/// loopback with password auth, so any local account that reads it connects
/// as the superuser. A build before #1642 left it at `File::create`'s 0644,
/// so it is set to 0600 on every launch, not only on the one that writes it.
/// The data directory is tightened to 0700 if it is wider: `initdb` makes it
/// 0700, and PostgreSQL also accepts 0750.
///
/// A failure is logged, not returned: a volume that ignores Unix modes would
/// otherwise fail every launch over a mode it cannot hold.
#[cfg(unix)]
fn keep_to_owner(settings: &Settings) {
    use std::os::unix::fs::PermissionsExt;
    let tighten = |path: &Path, mode: u32| {
        let wider = std::fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & 0o777 & !mode != 0);
        let result = match wider {
            Ok(true) => std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)),
            Ok(false) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            tracing::warn!(%error, path = %path.display(), "could not keep this to its owner");
        }
    };
    tighten(&settings.password_file, 0o600);
    tighten(&settings.data_dir, 0o700);
}

/// Windows keeps a profile's files to its user through the profile's ACL.
#[cfg(not(unix))]
fn keep_to_owner(_settings: &Settings) {}

/// What `postmaster.pid` says about a data directory a start just failed on.
#[derive(Debug, PartialEq, Eq)]
enum Lock {
    /// There is no lock file, so it is not what refused the start.
    Absent,
    /// Nothing answered on the port the lock records, and the lock has been
    /// removed: the start is worth retrying.
    Cleared,
    /// Something is answering on the port the lock records. Whether it is
    /// *our* server is [`adopt`]'s to establish, and the recorded pid plays
    /// no part in it (#1775).
    Live { port: u16 },
}

/// Classify `data_dir`'s `postmaster.pid`, clearing it when it is stale.
///
/// Called only with `.owner.lock` held ([`start_managed`]), so no knobas has
/// this profile open, and only after PostgreSQL itself refused the start,
/// which it does for a lock naming any live process it may signal.
///
/// The **port** (line 4) is the one piece of evidence read. A TCP handshake
/// proves *something* is listening, but a recycled ephemeral port can put a
/// stranger there, so an answer is not proof the lock is live -- which is why
/// [`adopt`] asks the server what it serves before trusting it.
///
/// A **silent** port clears the lock **whatever the recorded pid is doing**
/// (#1789, reversing the rest of ruling B5). The pid (line 1) used to keep
/// the lock whenever it was alive, which locked a profile out on every launch
/// once a reboot had recycled the pid to some other process of ours. A
/// postmaster of ours answers on `127.0.0.1` from the moment it binds until
/// it exits, through crash recovery and shutdown alike ([`build_settings`]
/// pins that address), so a silent port with a live postmaster of ours behind
/// it is one in the few milliseconds between writing `postmaster.pid` and
/// binding, which no relaunch reaches unless the process is frozen there, or
/// an orphan an older build started on `localhost`. Clearing under either
/// puts no second server on the files: PostgreSQL's shared-memory interlock
/// lets only one of the two postmasters past it (see [`adopt`], *A stranger
/// on the port*), the other exits, and the survivor, whose lock file is then
/// gone, shuts itself down within a minute; the next start replays the WAL.
/// The evidence is recorded in `docs/decisions/2026-10-unattended-rulings.md`
/// under #1789.
fn inspect_lock(data_dir: &Path) -> Result<Lock, DbError> {
    let pid_file = data_dir.join("postmaster.pid");

    let contents = match std::fs::read_to_string(&pid_file) {
        Ok(contents) => contents,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(Lock::Absent),
        Err(source) => return Err(DbError::io(pid_file, source)),
    };

    if let Some(port) = lock_port(&contents)
        && port_answers(port)
    {
        tracing::warn!(
            port,
            "postmaster.pid names a port something is answering on"
        );
        return Ok(Lock::Live { port });
    }

    tracing::warn!(pid_file = %pid_file.display(), "removing stale postmaster.pid");
    clear_lock(data_dir)?;
    Ok(Lock::Cleared)
}

/// Whether the server a handle owns is still running (#1779).
///
/// Cheap and synchronous: one read of `postmaster.pid` and, while it still
/// names the postmaster this watch was made for, one `ps -p`. A postmaster
/// that exits removes the file, a killed one leaves it naming a dead
/// process, and a restarted one names another pid; each of the three is
/// gone. A caller on an async runtime runs it on a blocking thread.
#[derive(Clone, Debug)]
pub struct ServerWatch {
    data_dir: PathBuf,
    pid: u32,
}

impl ServerWatch {
    /// Whether the postmaster this watch was made for has stopped.
    #[must_use]
    pub fn is_gone(&self) -> bool {
        // Without `ps` there is no telling a killed postmaster from a live
        // one, and a watch that guessed would fail every window it watched.
        recorded_pid(&self.data_dir) != Some(self.pid) || (cfg!(unix) && !process_alive(self.pid))
    }
}

/// The postmaster's pid, line 1 of `data_dir`'s `postmaster.pid`, if the
/// file is there and names one.
fn recorded_pid(data_dir: &Path) -> Option<u32> {
    lock_pid(&std::fs::read_to_string(data_dir.join("postmaster.pid")).ok()?)
}

/// A `postmaster.pid`'s line 1, the postmaster's pid. A standalone backend
/// records its pid negated, which this reads as none.
fn lock_pid(contents: &str) -> Option<u32> {
    contents
        .lines()
        .next()?
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|pid| *pid != 0)
}

/// A `postmaster.pid`'s line 4, the port the postmaster last listened on.
fn lock_port(contents: &str) -> Option<u16> {
    contents
        .lines()
        .nth(3)?
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|port| *port != 0)
}

/// Whether a process with `pid` exists right now.
///
/// `ps -p` rather than `kill(pid, 0)`: this crate has no `libc` dependency and
/// nothing here is worth one. `ps` exits 0 when it printed a matching process
/// and 1 when it did not, on both macOS and Linux.
///
/// A pid is *not* proof of identity -- pids are recycled, so the process may
/// be anything at all. Only [`ServerWatch`] asks, about a pid it saw its own
/// server write minutes ago, where a recycled pid is as good as impossible.
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

/// No `ps` to ask, so nothing is known. [`ServerWatch::is_gone`] does not ask
/// off unix.
#[cfg(not(unix))]
fn process_alive(_pid: u32) -> bool {
    false
}

/// Whether a PostgreSQL answered and refused our credentials: SQLSTATE class
/// `28`, `invalid_password` (`28P01`) or `invalid_authorization_specification`
/// (`28000`, no such role or no `pg_hba` entry).
///
/// A server of ours has the `postgres` role with the password this profile
/// recorded and admits it on loopback, so a refusal says the server is not
/// ours. Any other database error (a missing database, a server still
/// starting) says nothing about whose it is.
fn refuses_our_credentials(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|code| code.starts_with("28"))
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

/// A window this module cannot otherwise be caught in, opened for tests.
///
/// The property at stake is what happens to *somebody else's* database, and it
/// only goes wrong in the two statements between clearing a stale
/// `postmaster.pid` and retrying the start: a racing process writes a fresh
/// lock file in there, and a handle dropped afterwards runs `pg_ctl stop -m
/// fast` against it. Nothing outside this function can get in there -- the
/// window is two statements wide and it is held under the bring-up lock -- so
/// a test that wants to occupy it has to be let in.
///
/// Compiled only with `test-util`, a feature no application build enables:
/// `knobas-app` depends on `knobas-db` without it, so none of this exists in
/// the shipped binary.
#[cfg(feature = "test-util")]
pub mod seam {
    use std::sync::{Arc, Mutex, OnceLock, PoisonError};

    type Hook = Arc<dyn Fn() + Send + Sync>;

    fn slot() -> &'static Mutex<Option<Hook>> {
        static SLOT: OnceLock<Mutex<Option<Hook>>> = OnceLock::new();
        SLOT.get_or_init(|| Mutex::new(None))
    }

    /// Run `hook` after a stale lock is cleared and before the start is
    /// retried. Process-wide; call [`forget_hooks`] when the test is done.
    pub fn after_clearing_a_stale_lock(hook: impl Fn() + Send + Sync + 'static) {
        *slot().lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(hook));
    }

    /// Drop whatever [`after_clearing_a_stale_lock`] installed.
    pub fn forget_hooks() {
        *slot().lock().unwrap_or_else(PoisonError::into_inner) = None;
    }

    /// Cloned out before it is called, so the lock is not held across a hook
    /// that might reach back in here.
    pub(crate) fn stale_lock_cleared() {
        let hook = slot()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(hook) = hook {
            hook();
        }
    }
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
    let created = path.clone();
    let file = off_runtime(&path, move || File::create(&created))
        .await?
        .map_err(|source| DbError::io(&path, source))?;

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

    /// #2025: the reason is this start's last `FATAL`, from `FATAL:` on and
    /// on one line, never an earlier start's.
    #[test]
    fn a_starts_reason_is_its_own_last_fatal_line() {
        let earlier = "2026-10-24 10:00:00.000 CEST [1] FATAL:  lock file \"postmaster.pid\" already exists\n";
        let log = format!(
            "{earlier}2026-10-24 10:00:01.000 CEST [2] LOG:  could not open configuration file\n\
             2026-10-24 10:00:01.000 CEST [2] FATAL:  could not load pg_hba.conf\n\
             2026-10-24 10:00:01.000 CEST [2] LOG:  database system is shut down\n"
        );
        assert_eq!(
            start_reason(log.as_bytes(), earlier.len() as u64).as_deref(),
            Some("could not load pg_hba.conf")
        );
    }

    /// #2025: a start that wrote no `FATAL` is reported by its own lines: all
    /// of a short log, the last five of a long one, an earlier start's
    /// `FATAL` left out. One that wrote nothing has no reason.
    #[test]
    fn a_start_without_a_fatal_line_is_reported_by_its_own_last_lines() {
        let earlier = "FATAL:  an earlier start's reason\n";
        let short = format!("{earlier}LOG:  one\n\n  LOG:   two\n");
        assert_eq!(
            start_reason(short.as_bytes(), earlier.len() as u64).as_deref(),
            Some("LOG: one; LOG: two")
        );
        let long = format!("{earlier}1\n2\n3\n4\n5\n6\n7\n");
        assert_eq!(
            start_reason(long.as_bytes(), earlier.len() as u64).as_deref(),
            Some("3; 4; 5; 6; 7")
        );
        assert_eq!(start_reason(earlier.as_bytes(), earlier.len() as u64), None);
        // The postmaster's own stderr wraps one sentence over lines that end
        // in a comma, which reads on as the sentence, not as `,;`.
        let wrapped = "postgres: could not find the database system\n\
                       Expected to find it in the directory \"/d\",\n\
                       but could not open file \"/d/global/pg_control\"\n";
        assert_eq!(
            start_reason(wrapped.as_bytes(), 0).as_deref(),
            Some(
                "postgres: could not find the database system; Expected to find it in the \
                 directory \"/d\", but could not open file \"/d/global/pg_control\""
            )
        );
    }

    /// #2025: a log shorter than it was before the start was replaced in
    /// between, and is read whole.
    #[test]
    fn a_log_shorter_than_before_the_start_is_read_whole() {
        assert_eq!(
            start_reason(b"FATAL:  could not load pg_hba.conf\n", 4096).as_deref(),
            Some("could not load pg_hba.conf")
        );
    }

    fn write(dir: &Path, name: &str, contents: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }

    /// The installation directory is shared with every other
    /// `postgresql_embedded` application on the machine, so it can hold more
    /// than one PostgreSQL. `pg_dump` refuses to dump a server newer than
    /// itself, so picking whichever version `read_dir` yielded first would
    /// make a backup fail against the very server knobas runs -- and the two
    /// directories are indistinguishable to anything but the pin.
    #[test]
    fn a_tool_comes_from_the_pinned_version_when_several_are_installed() {
        let install = tempfile::tempdir().unwrap();
        let pinned = PG_VERSION_REQ.trim_start_matches('=');

        for version in ["17.4.0", pinned, "19.0.0"] {
            let bin = install.path().join(version).join("bin");
            std::fs::create_dir_all(&bin).unwrap();
            write(&bin, "pg_dump", version);
        }

        let found = find_tool(install.path(), "pg_dump").expect("a pg_dump");
        assert_eq!(
            std::fs::read_to_string(&found).unwrap(),
            pinned,
            "{} is not the pinned version's tool",
            found.display()
        );
    }

    /// ...and a version-less layout (the crate's own default) still resolves.
    #[test]
    fn a_tool_is_still_found_without_a_version_directory() {
        let install = tempfile::tempdir().unwrap();
        let bin = install.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        write(&bin, "pg_dump", "flat");

        assert_eq!(
            find_tool(install.path(), "pg_dump"),
            Some(bin.join("pg_dump"))
        );
        assert_eq!(
            find_tool(install.path(), "pg_restore"),
            None,
            "a tool that is not there is absent, not a path that does not exist"
        );
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
    fn the_recorded_password_is_adopted_under_the_lock_not_before_it() {
        let dir = tempfile::tempdir().unwrap();
        // Recorded before the settings are even built, so the assertion below
        // is about *where* the file is read, not about whether it exists.
        write(dir.path(), ".pgpass", "s3cret\n");

        let mut settings = build_settings(dir.path()).unwrap();
        assert_ne!(
            settings.password, "s3cret",
            "build_settings must not read the password file: doing it there is what leaves a \
             launch that started before the winner wrote .pgpass holding a password nothing \
             accepts"
        );

        adopt_recorded_password(&mut settings).unwrap();

        assert_eq!(settings.password, "s3cret");
    }

    #[test]
    fn an_empty_password_file_leaves_the_invented_password_alone() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), ".pgpass", "");
        let mut settings = build_settings(dir.path()).unwrap();
        let invented = settings.password.clone();

        adopt_recorded_password(&mut settings).unwrap();

        assert_eq!(settings.password, invented);
        assert!(!settings.password.is_empty());
    }

    /// A launch whose settings were built before another process initialised
    /// the profile adopts that process's orphaned server with the password
    /// `initdb` recorded, not the one it invented.
    ///
    /// This is the cold machine, deterministically: `build_settings` runs
    /// while the profile is empty -- which is all a launch has to do to be
    /// holding a random password nothing accepts -- and only then does the
    /// winner initialise the directory, and die. If the recorded password
    /// were read anywhere but under the bring-up lock, the launch would fail
    /// `start()` against the orphan, reach `adopt`, be refused by its own
    /// server and, the orphan's postmaster being alive, report the profile as
    /// in use. Permanently: every later launch built that way does the same.
    ///
    /// The winner is orphaned rather than left running: a live owner refuses
    /// the second launch before the password is read at all (ruling B2).
    #[tokio::test]
    async fn a_launch_that_started_cold_authenticates_with_the_recorded_password() {
        let dir = tempfile::tempdir().unwrap();

        let cold = build_settings(dir.path()).unwrap();

        // The winner: initialises the profile, records the password, stays up.
        let winner = EmbeddedDb::start(DbConfig {
            root_dir: dir.path().to_path_buf(),
            existing_url: None,
        })
        .await
        .expect("the winner brings the profile up");
        winner.abandon();
        let recorded = read_password(&cold.password_file)
            .unwrap()
            .expect("initdb records a password");
        assert_ne!(
            cold.password, recorded,
            "settings built cold must carry a password the winner's server does not know"
        );

        // The loser, with the settings it built before any of that happened.
        let db = match start_managed(dir.path(), cold).await {
            Ok(Started::Ready(db)) => db,
            Ok(Started::StaleLock { serving, .. }) => {
                panic!("the winner serves this very directory, not {serving:?}")
            }
            Err(error) => panic!("the launch must adopt the winner's orphan: {error}"),
        };
        let one: (i32,) = sqlx::query_as("select 1")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(one.0, 1);

        // Adopted from an orphan, so it owns the server and its stop stops it.
        assert!(db.owns_server());
        (*db).stop().await.unwrap();
    }

    /// ...and an absent one too, which is the winner's own first launch:
    /// `initialize()` is what records the invented password.
    #[test]
    fn an_absent_password_file_leaves_the_invented_password_alone() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = build_settings(dir.path()).unwrap();
        let invented = settings.password.clone();

        adopt_recorded_password(&mut settings).unwrap();

        assert_eq!(settings.password, invented);
    }

    /// A pid that is certainly not running: a child that has already been
    /// reaped. Nothing else about a number is provably dead -- pids recycle.
    fn dead_pid() -> u32 {
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        pid
    }

    /// A port nothing is listening on right now: bind it, read it back, drop
    /// the listener.
    ///
    /// Only *probably* still free by the time anything probes it -- ephemeral
    /// ports are handed out round-robin, and a concurrent test in this binary
    /// can be given the same one moments later. Nothing can reserve a silent
    /// port, so callers that need the probe to find silence redraw instead:
    /// see [`inspect_with_a_silent_port`].
    fn free_port() -> u16 {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.local_addr().unwrap().port()
    }

    /// `inspect_lock` against a lock file naming `pid` and a port nothing
    /// answers on, redrawing the port if something took it in between.
    ///
    /// A `Lock::Live` here means the draw lost, not that the classification is
    /// wrong: the port answered, so `inspect_lock` did exactly what it should.
    fn inspect_with_a_silent_port(dir: &Path, pid: u32) -> (Lock, PathBuf) {
        for _ in 0..8 {
            let file = pid_file(dir, pid, free_port());
            let lock = inspect_lock(dir).unwrap();
            if !matches!(lock, Lock::Live { .. }) {
                return (lock, file);
            }
        }
        panic!("every port drawn was taken by something else");
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
        let (lock, pid_file) = inspect_with_a_silent_port(dir.path(), dead_pid());

        assert_eq!(lock, Lock::Cleared);
        assert!(!pid_file.exists());
    }

    /// #1789: a silent port clears the lock whatever the recorded pid is
    /// doing. A live pid behind it is, after a reboot, one recycled to some
    /// other process of ours, and keeping the lock refused every launch.
    #[test]
    fn inspect_lock_removes_a_pid_file_whose_process_lives_but_whose_port_is_silent() {
        let dir = tempfile::tempdir().unwrap();
        // Our own pid: alive for certain, for as long as this test runs.
        let (lock, pid_file) = inspect_with_a_silent_port(dir.path(), std::process::id());

        assert_eq!(lock, Lock::Cleared);
        assert!(!pid_file.exists());
    }

    #[test]
    fn inspect_lock_keeps_a_pid_file_a_live_listener_owns() {
        let dir = tempfile::tempdir().unwrap();
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let pid_file = pid_file(dir.path(), dead_pid(), port);

        assert_eq!(inspect_lock(dir.path()).unwrap(), Lock::Live { port });
        assert!(pid_file.exists());
    }

    /// #1779: a postmaster killed outright (`kill -9`, a crash) leaves its
    /// `postmaster.pid` behind, still naming it. The watch reads that as
    /// gone because the process is, and a file naming a live postmaster as
    /// running.
    #[test]
    fn a_watch_sees_a_killed_postmaster_whose_lock_file_stayed() {
        let dir = tempfile::tempdir().unwrap();
        let watch = |pid| ServerWatch {
            data_dir: dir.path().to_path_buf(),
            pid,
        };
        let alive = std::process::id();
        pid_file(dir.path(), alive, free_port());
        assert!(!watch(alive).is_gone(), "a live postmaster is running");

        let killed = dead_pid();
        pid_file(dir.path(), killed, free_port());
        assert!(watch(killed).is_gone(), "a killed postmaster is gone");
        assert!(watch(alive).is_gone(), "a file naming another pid is gone");
    }

    #[test]
    fn process_liveness_tells_a_running_process_from_a_reaped_one() {
        // Both directions, because a `process_alive` stuck at either answer
        // would satisfy one of the watch's cases above on its own.
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

    /// The destructive branch in `adopt` clears a lock file, and an
    /// unresolvable path is not evidence of anything. `provably_different` is
    /// deliberately not the negation of "same directory" for exactly that.
    #[test]
    fn a_path_that_cannot_be_resolved_is_never_proof_of_a_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("gone");
        assert!(
            !provably_different(&missing, dir.path()),
            "an unresolvable path proves nothing"
        );
        assert!(
            !provably_different(dir.path(), dir.path()),
            "one directory is not different from itself"
        );
        assert!(provably_different(
            dir.path(),
            std::env::temp_dir().as_path()
        ));
    }

    /// A listener that answers every connection the way a PostgreSQL that
    /// does not know our password does: it takes the startup packet and
    /// replies with a `FATAL` `28P01`. The real one is witnessed by the
    /// squatter test in `tests/embedded.rs`; this one keeps the live-pid half
    /// from needing a second server. Returns the port; the accept loop lives
    /// until the process exits.
    fn refusing_postgres() -> u16 {
        use std::io::{Read as _, Write as _};

        fn packet(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
            let mut len = [0u8; 4];
            stream.read_exact(&mut len)?;
            let mut body = vec![0u8; (u32::from_be_bytes(len) as usize).saturating_sub(4)];
            stream.read_exact(&mut body)?;
            Ok(body)
        }

        fn refuse(stream: &mut TcpStream) -> io::Result<()> {
            // An `SSLRequest` (code 80877103) first, when the client sends
            // one: decline it, and the startup packet follows.
            if packet(stream)? == 80_877_103u32.to_be_bytes() {
                stream.write_all(b"N")?;
                packet(stream)?;
            }
            let mut fields = Vec::new();
            for (tag, value) in [
                (b'S', "FATAL"),
                (b'V', "FATAL"),
                (b'C', "28P01"),
                (b'M', "password authentication failed for user \"postgres\""),
            ] {
                fields.push(tag);
                fields.extend_from_slice(value.as_bytes());
                fields.push(0);
            }
            fields.push(0);
            let len = u32::try_from(fields.len() + 4).unwrap_or(u32::MAX);
            stream.write_all(b"E")?;
            stream.write_all(&len.to_be_bytes())?;
            stream.write_all(&fields)
        }

        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let _ = refuse(&mut stream);
            }
        });
        port
    }

    /// An ownership lock of this test's own, for `adopt` to take over.
    fn owner_lock(dir: &Path) -> OwnerLock {
        match claim_ownership(dir).unwrap() {
            Ownership::Ours(lock) => lock,
            Ownership::Held { by } => panic!("a fresh directory's lock is held by {by:?}"),
        }
    }

    /// The two strangers that can sit on a recycled port: a program that does
    /// not speak PostgreSQL, and a PostgreSQL that is not ours.
    fn strangers() -> [(&'static str, u16); 2] {
        [
            ("a non-postgres listener", squatter()),
            ("a postgres that refuses our password", refusing_postgres()),
        ]
    }

    /// The recycled-port lockout: a dead postmaster's port now belongs to some
    /// unrelated program, or to another PostgreSQL (CORE-2, ruling B5). That
    /// is not a server of ours, and reporting it as in use leaves the user
    /// with an app that refuses to launch until they delete `postmaster.pid`
    /// by hand.
    #[tokio::test]
    async fn adopt_treats_a_stranger_on_the_recorded_port_as_a_stale_lock() {
        for (stranger, port) in strangers() {
            let dir = tempfile::tempdir().unwrap();
            match adopt(adoption_settings(dir.path()), port, owner_lock(dir.path())).await {
                Ok(Started::StaleLock { port: p, .. }) => assert_eq!(p, port, "{stranger}"),
                Ok(Started::Ready(_)) => panic!("{stranger} could not have been adopted"),
                Err(error) => panic!("{stranger} on the port is a stale lock, not {error:?}"),
            }
        }
    }

    /// The same strangers while the recorded pid is a live process, this
    /// test's own (#1775): still a stale lock. `adopt` holds the ownership
    /// lock, so no knobas has the profile open, and a postmaster of ours bound
    /// to the recorded port answers there itself. The pid is not read at all,
    /// which is what this pins: a lock file naming a live pid and a stranger's
    /// port, through the same path `start_managed` takes.
    #[tokio::test]
    async fn a_stranger_on_the_recorded_port_clears_the_lock_whatever_the_pid_is_doing() {
        for (stranger, port) in strangers() {
            let dir = tempfile::tempdir().unwrap();
            let data_dir = dir.path().join("data");
            std::fs::create_dir(&data_dir).unwrap();
            pid_file(&data_dir, std::process::id(), port);
            let Lock::Live { port: recorded } = inspect_lock(&data_dir).unwrap() else {
                panic!("{stranger} answers on the recorded port");
            };
            match adopt(
                adoption_settings(dir.path()),
                recorded,
                owner_lock(dir.path()),
            )
            .await
            {
                Ok(Started::StaleLock { port: p, .. }) => assert_eq!(p, port, "{stranger}"),
                Ok(Started::Ready(_)) => panic!("{stranger} could not have been adopted"),
                Err(error) => panic!("{stranger} beside a live pid is stale, not {error:?}"),
            }
        }
    }

    /// A failure the *PostgreSQL protocol* produced means a postmaster answered
    /// on that port. Only a refusal of our credentials says it is not ours
    /// (`refuses_our_credentials`); anything else, a missing database or a
    /// server still starting, leaves the lock alone.
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

    /// The lock file's creation does not hold a runtime worker (#1860). A
    /// FIFO in its place blocks `File::create` until something opens it for
    /// reading, so a creation on the worker would stop the one worker of this
    /// runtime: its timer task could not fire while the creation waits.
    #[cfg(unix)]
    #[test]
    fn the_bring_up_lock_is_created_off_the_runtime_worker() {
        let dir = tempfile::tempdir().unwrap();
        let fifo = dir.path().join(BRING_UP_LOCK);
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        // A watchdog frees the creation after ten seconds whatever happens, so
        // a creation that does hold the worker ends the test as a failure
        // rather than a hang. It is far past the bound below, so a loaded
        // machine cannot free a held worker in time to pass.
        let watchdog = fifo.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(10));
            let _ = File::open(&watchdog);
        });
        runtime.block_on(async {
            let root = dir.path().to_path_buf();
            let started = std::time::Instant::now();
            let lock = tokio::spawn(async move { bring_up_lock(&root).await.map(|_| ()) });
            // The tick runs on the one worker, beside the lock's creation.
            tokio::spawn(tokio::time::sleep(Duration::from_millis(200)))
                .await
                .unwrap();
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "the worker was held while the lock file was being created"
            );
            // Free the creation now rather than at the watchdog. This thread
            // is not a worker, and the open returns once the creation's own
            // open meets it.
            drop(File::open(&fifo).unwrap());
            // The lock itself is refused on a FIFO on some systems, so only
            // that it settles is asserted.
            let _ = lock.await.unwrap();
        });
    }

    /// A dropped ownership lock is free at once, even while another thread
    /// is spawning a process (#1802). A child between its fork and its exec
    /// holds a copy of every descriptor, the lock's among them, and an OS
    /// lock on a file lasts until the last copy closes: a release that only
    /// closed the descriptor left the lock held for as long as that child
    /// took to exec, and a relaunch in that instant was refused naming this
    /// very process. The embedded tests on other threads spawn `pg_ctl` and
    /// `initdb`; any program stands in for them.
    #[test]
    fn a_released_ownership_lock_is_free_while_children_are_spawned() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let dir = tempfile::tempdir().unwrap();
        let done = Arc::new(AtomicBool::new(false));
        let spawners: Vec<_> = (0..2)
            .map(|_| {
                let done = Arc::clone(&done);
                std::thread::spawn(move || {
                    while !done.load(Ordering::Relaxed) {
                        let _ = std::process::Command::new("true").status();
                    }
                })
            })
            .collect();

        let mut refused = 0;
        for _ in 0..300 {
            match claim_ownership(dir.path()).unwrap() {
                // Held across a moment of spawning, then released.
                Ownership::Ours(lock) => {
                    std::thread::sleep(Duration::from_micros(200));
                    drop(lock);
                }
                Ownership::Held { .. } => refused += 1,
            }
        }
        done.store(true, Ordering::Relaxed);
        for spawner in spawners {
            spawner.join().unwrap();
        }

        assert_eq!(refused, 0, "claims refused after their lock was dropped");
    }

    /// `initdb` set up to run into a fresh profile under `dir`, and its path.
    async fn initdb_for(dir: &Path) -> (Settings, PathBuf) {
        let settings = build_settings(dir).unwrap();
        write_password_file(&settings.password_file, &settings.password).unwrap();
        let initdb = install(dir, &settings).await.unwrap().join("initdb");
        assert!(
            !dir.join(INSTALL_ONLY).exists(),
            "the install placeholder is removed"
        );
        (settings, initdb)
    }

    /// #1637: an `initdb` that outlives its limit is stopped, and it removes
    /// the data directory it made on the way out, so the next launch starts
    /// from nothing rather than from a directory with `PG_VERSION` in it that
    /// nothing recovers.
    #[tokio::test]
    async fn a_timed_out_initdb_is_stopped_and_removes_what_it_made() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, initdb) = initdb_for(dir.path()).await;

        let error = run_initdb(&initdb, &settings, Duration::from_millis(150))
            .await
            .expect_err("a full initdb takes longer than 150 ms");
        assert!(
            error.to_string().contains("deadline has elapsed"),
            "{error}"
        );
        assert!(
            !settings.data_dir.exists(),
            "initdb removed the data directory it made"
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(!settings.data_dir.exists(), "and nothing wrote it again");
    }

    /// #1637: a half-written data directory that is a symlink loses the link,
    /// never what it points at, and one with a database in it keeps
    /// everything.
    #[cfg(unix)]
    #[test]
    fn a_half_written_symlinked_data_directory_loses_the_link_alone() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("elsewhere");
        std::fs::create_dir_all(target.join("base")).unwrap();
        std::fs::write(target.join("postgresql.conf"), "").unwrap();
        let data = dir.path().join("data");
        std::os::unix::fs::symlink(&target, &data).unwrap();

        recover_or_mark(&data).unwrap();

        assert!(
            std::fs::symlink_metadata(&data).is_err(),
            "the link is gone"
        );
        assert!(
            target.join("postgresql.conf").is_file(),
            "its target is kept"
        );
        assert!(target.join("base").is_dir(), "its target is kept");

        std::os::unix::fs::symlink(&target, &data).unwrap();
        std::fs::write(target.join("PG_VERSION"), "18\n").unwrap();
        std::fs::create_dir_all(target.join("base/16384")).unwrap();
        std::fs::create_dir_all(target.join("pg_tblspc")).unwrap();
        recover_or_mark(&data).unwrap();
        assert!(
            data.join("postgresql.conf").is_file(),
            "with a database in it, kept"
        );

        // #2007: `PG_VERSION` is removed first, but never through the link.
        std::fs::remove_dir_all(&target).unwrap();
        unfinished(&target);
        recover_or_mark(&data).unwrap();
        assert!(
            std::fs::symlink_metadata(&data).is_err(),
            "the link is gone"
        );
        assert!(target.join("PG_VERSION").is_file(), "its target is kept");
    }

    /// #2007: removing an unfinished directory removes `PG_VERSION` first,
    /// so a removal cut short leaves what still reads as unfinished.
    /// `remove_dir_all` removes entries in `readdir` order. On a filesystem
    /// that lists `base/` before `PG_VERSION`, a launch killed between the
    /// two left `PG_VERSION` without `base/`, which reads as a database on a
    /// volume not mounted. Here a `pg_wal/` that cannot be emptied cuts the
    /// removal short. APFS lists it before `PG_VERSION`, so on a Mac the test
    /// fails without the fix; where `pg_wal/` is listed later it passes either
    /// way.
    #[cfg(unix)]
    #[test]
    fn a_removal_cut_short_leaves_no_pg_version() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        unfinished(&data);
        let wal = data.join("pg_wal");
        std::fs::create_dir_all(&wal).unwrap();
        write(&wal, "000000010000000000000001", "");
        std::fs::set_permissions(&wal, std::fs::Permissions::from_mode(0o500)).unwrap();

        let removed = recover_or_mark(&data);
        std::fs::set_permissions(&wal, std::fs::Permissions::from_mode(0o700)).unwrap();

        assert!(removed.is_err(), "the removal was cut short");
        assert!(!data.join("PG_VERSION").exists(), "PG_VERSION went first");
        assert_eq!(shape(&data), Shape::Unfinished);
        recover_or_mark(&data).unwrap();
        assert!(!data.exists(), "and the next launch removes the rest");
    }

    /// #1637: dropping the future that runs `initdb` kills it, rather than
    /// leaving it writing into a directory nobody is waiting on. Held after
    /// the bootstrap, an `initdb` left running waits on its child past the
    /// 30 s [`interrupt_initdb`] gives it to be gone (#1908); before, a drop
    /// at 150 ms said nothing once load made `initdb` take longer than the
    /// 2 s the test waited.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_dropped_initdb_is_killed_with_its_future() {
        let dir = tempfile::tempdir().unwrap();
        let settings = interrupt_initdb(dir.path(), KillPoint::AfterBootstrap).await;
        assert!(
            !settings.data_dir.join("base/5").exists(),
            "the dropped initdb ran on to the end"
        );
    }

    /// Where a killed `initdb` stops (#1819), one state of the three that
    /// fail every launch. Each was named by starting a server on what the
    /// kill left and logging in (recorded under #1819 in
    /// `docs/decisions/2026-10-unattended-rulings.md`).
    ///
    /// **Each is a run `initdb` is held in** (#1908): its `postgres` waits
    /// there until the test has killed `initdb`. A kill timed by watching
    /// the process tree missed the single-user setup's window, about 25 ms,
    /// under load.
    #[cfg(unix)]
    #[derive(Clone, Copy, Debug)]
    enum KillPoint {
        /// The configuration probes, before the bootstrap: no `pg_control`.
        /// Held in the first `postgres --check`, which then runs.
        BeforeBootstrap,
        /// The bootstrap done, the setup not begun: our password is refused.
        /// Held in the single-user `postgres`, which then never starts.
        AfterBootstrap,
        /// The single-user setup, as soon as it runs: `initdb` has piped
        /// `setup_auth` and dies before the rest, so there is no `postgres`
        /// database. Held after `setup_auth`'s password, and then given what
        /// `initdb` piped before it died, at most a pipe's buffer.
        DuringSetup,
    }

    /// The `postgres` a held `initdb` runs: every run passes through to the
    /// real one, but the one its kill point names writes `held` and waits for
    /// `release`. The files it reads and writes are in its directory's parent.
    #[cfg(unix)]
    const HELD_POSTGRES: &str = r#"#!/bin/sh
dir=$(dirname "$(dirname "$0")")
real=$(cat "$dir/real-postgres")
point=$(cat "$dir/point")
case $point in
probes) run=--check ;;
*) run=--single ;;
esac
case " $* " in
*" $run "*) ;;
*) exec "$real" "$@" ;;
esac
hold() {
    : >"$dir/held"
    n=0
    while [ ! -e "$dir/release" ]; do
        # About 60 s, past the test's own 30 s waits: the test gave up,
        # so run nothing more.
        [ "$n" -lt 600 ] || exit 1
        sleep 0.1
        n=$((n + 1))
    done
}
case $point in
probes)
    hold
    "$real" "$@"
    ;;
bootstrapped)
    hold
    ;;
setup)
    {
        while IFS= read -r line; do
            printf '%s\n' "$line"
            case $line in 'ALTER USER '*) break ;; esac
        done
        hold
        cat
    } | "$real" "$@"
    ;;
esac
: >"$dir/done"
"#;

    /// A copy of `initdb` under `hold_dir` that is held at `point`, and its path.
    /// `initdb` runs the `postgres` beside it and reads `share/` beside its
    /// directory; its library is found the same way.
    #[cfg(unix)]
    fn held_initdb(hold_dir: &Path, initdb: &Path, point: KillPoint) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let bin = initdb.parent().unwrap();
        let install = bin.parent().unwrap();
        std::fs::create_dir(hold_dir.join("bin")).unwrap();
        for dir in ["lib", "share"] {
            std::os::unix::fs::symlink(install.join(dir), hold_dir.join(dir)).unwrap();
        }
        let copy = hold_dir.join("bin").join("initdb");
        std::fs::copy(initdb, &copy).unwrap();
        let postgres = write(&hold_dir.join("bin"), "postgres", HELD_POSTGRES);
        std::fs::set_permissions(&postgres, std::fs::Permissions::from_mode(0o755)).unwrap();
        write(
            hold_dir,
            "real-postgres",
            bin.join("postgres").to_str().unwrap(),
        );
        let point = match point {
            KillPoint::BeforeBootstrap => "probes",
            KillPoint::AfterBootstrap => "bootstrapped",
            KillPoint::DuringSetup => "setup",
        };
        write(hold_dir, "point", point);
        copy
    }

    /// The pids of the processes running `program`.
    #[cfg(unix)]
    fn pids_running(program: &Path) -> Vec<u32> {
        let listing = std::process::Command::new("ps")
            .args(["-ww", "-axo", "pid=,args="])
            .output()
            .unwrap();
        let program = program.to_str().unwrap();
        String::from_utf8_lossy(&listing.stdout)
            .lines()
            .filter_map(|line| {
                let (pid, args) = line.trim_start().split_once(' ')?;
                args.trim_start()
                    .starts_with(program)
                    .then(|| pid.parse().ok())?
            })
            .collect()
    }

    /// Wait up to 30 s for `done`, naming `what` when it does not come.
    #[cfg(unix)]
    async fn wait_until(what: &str, done: impl Fn() -> bool) {
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while !done() {
            assert!(std::time::Instant::now() < deadline, "{what} after 30 s");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Run `initdb` into a fresh profile under `dir` and drop it once it is
    /// held at `point`, which kills it as a bring-up given up does
    /// (`kill_on_drop`). Then let its held child go on, and wait for it to
    /// drain what `initdb` piped it and exit.
    #[cfg(unix)]
    async fn interrupt_initdb(dir: &Path, point: KillPoint) -> Settings {
        let (settings, initdb) = initdb_for(dir).await;
        let data = settings.data_dir.clone();
        let hold = tempfile::tempdir().unwrap();
        let hold_dir = hold.path();
        let initdb = held_initdb(hold_dir, &initdb, point);
        let reached = async {
            while !hold_dir.join("held").exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            // `initdb` waits on the held child, so it is still running.
            pids_running(&initdb)
        };
        let pids = tokio::select! {
            finished = run_initdb(&initdb, &settings, COMMAND_TIMEOUT) => {
                panic!("initdb finished before {point:?}: {finished:?}")
            }
            pids = reached => pids,
        };
        assert_eq!(pids.len(), 1, "{point:?}: one initdb runs, {pids:?}");
        wait_until("initdb outlived its kill", || !process_alive(pids[0])).await;
        write(hold_dir, "release", "");
        wait_until("the held child had not exited", || {
            hold_dir.join("done").exists()
        })
        .await;

        let has = |name: &str| data.join(name).exists();
        assert!(has("PG_VERSION") && has("postgresql.conf"), "{point:?}");
        match point {
            KillPoint::BeforeBootstrap => assert!(!has("global/pg_control"), "{point:?}"),
            KillPoint::AfterBootstrap | KillPoint::DuringSetup => {
                assert!(has("global/pg_control") && !has("base/4"), "{point:?}");
            }
        }
        settings
    }

    /// #1819: each state a killed `initdb` leaves with `PG_VERSION` in it
    /// comes back from a launch initialised. Before, `postgresql.conf` read
    /// as "initialised" and each failed every launch.
    #[cfg(unix)]
    #[tokio::test]
    async fn each_directory_a_killed_initdb_leaves_comes_back_from_a_launch() {
        for point in [
            KillPoint::BeforeBootstrap,
            KillPoint::AfterBootstrap,
            KillPoint::DuringSetup,
        ] {
            let dir = tempfile::tempdir().unwrap();
            let settings = interrupt_initdb(dir.path(), point).await;

            let db = EmbeddedDb::start(DbConfig {
                root_dir: dir.path().to_path_buf(),
                existing_url: None,
            })
            .await
            .unwrap_or_else(|error| panic!("{point:?}: the launch failed: {error}"));
            let one: Result<(i32,), _> = sqlx::query_as("select 1").fetch_one(db.pool()).await;
            db.stop().await.unwrap();
            assert_eq!(one.unwrap().0, 1, "{point:?}");
            assert!(
                settings.data_dir.join(INITDB_FINISHED).is_file(),
                "{point:?}: the new initdb is marked finished"
            );
        }
    }

    /// #1819: a directory a finished `initdb` made is kept, from this build
    /// and from the two older flows: #1637's `initdb`, which wrote no
    /// marker, and the crate's own `setup()` before it, followed by the
    /// launch that made the knobas database and a reader's row in it. An
    /// older build's directory is marked the first time a launch sees it.
    #[tokio::test]
    async fn a_directory_a_finished_initdb_made_is_never_removed() {
        // #1637's flow, stopped after `initdb` and before any start.
        let dir = tempfile::tempdir().unwrap();
        let (settings, initdb) = initdb_for(dir.path()).await;
        run_initdb(&initdb, &settings, COMMAND_TIMEOUT)
            .await
            .unwrap();
        let sentinel = write(&settings.data_dir, "keep-me", "a finished initdb\n");
        let cfg = DbConfig {
            root_dir: dir.path().to_path_buf(),
            existing_url: None,
        };
        EmbeddedDb::start(cfg).await.unwrap().stop().await.unwrap();
        assert!(sentinel.is_file(), "#1637's directory is kept");
        assert!(
            settings.data_dir.join(INITDB_FINISHED).is_file(),
            "and marked"
        );

        // The crate's own `setup()`, as every build before #1637 ran it.
        let dir = tempfile::tempdir().unwrap();
        let settings = build_settings(dir.path()).unwrap();
        let mut older = PostgreSQL::new(settings.clone());
        older.setup().await.unwrap();
        older.start().await.unwrap();
        older.create_database(DATABASE_NAME).await.unwrap();
        let url = older.settings().url(DATABASE_NAME);
        let mut conn = PgConnection::connect(&url).await.unwrap();
        for statement in [
            "create table reader (v text)",
            "insert into reader values ('kept')",
        ] {
            sqlx::query(statement).execute(&mut conn).await.unwrap();
        }
        conn.close().await.unwrap();
        older.stop().await.unwrap();
        drop(older);
        assert!(!settings.data_dir.join(INITDB_FINISHED).exists());

        let cfg = DbConfig {
            root_dir: dir.path().to_path_buf(),
            existing_url: None,
        };
        for launch in ["first", "second"] {
            let db = EmbeddedDb::start(cfg.clone()).await.unwrap();
            let row: Result<(String,), _> = sqlx::query_as("select v from reader")
                .fetch_one(db.pool())
                .await;
            db.stop().await.unwrap();
            let row = row.unwrap_or_else(|error| panic!("{launch} launch: {error}"));
            assert_eq!(row.0, "kept", "{launch} launch");
            assert!(
                settings.data_dir.join(INITDB_FINISHED).is_file(),
                "{launch} launch: marked"
            );
        }
    }

    /// What a killed `initdb` leaves, at `data`: `PG_VERSION` and the other
    /// files it writes, `base/1` alone and an empty `pg_tblspc/`.
    fn unfinished(data: &Path) {
        std::fs::create_dir_all(data.join("base/1")).unwrap();
        std::fs::create_dir_all(data.join("pg_tblspc")).unwrap();
        write(data, "PG_VERSION", "18\n");
        for name in INITDB_FILES {
            write(data, name, "");
        }
    }

    /// #1824 review: a `base/` or `pg_tblspc/` that is a symlink to nothing,
    /// a volume not mounted, counts as holding a database. Read as empty, it
    /// removed the directory: the cluster's catalog and WAL with it.
    #[cfg(unix)]
    #[test]
    fn a_directory_whose_databases_are_on_a_missing_volume_is_kept() {
        for moved in ["base", "pg_tblspc"] {
            let dir = tempfile::tempdir().unwrap();
            let data = dir.path().join("data");
            unfinished(&data);
            std::fs::remove_dir_all(data.join(moved)).unwrap();
            let unmounted = dir.path().join("unmounted").join(moved);
            std::os::unix::fs::symlink(&unmounted, data.join(moved)).unwrap();

            recover_or_mark(&data).unwrap();

            assert!(data.join("PG_VERSION").is_file(), "{moved}: kept");
        }
    }

    /// #1819: a `base/` that cannot be listed counts as holding a database,
    /// so the directory is kept.
    #[cfg(unix)]
    #[test]
    fn a_directory_whose_databases_cannot_be_listed_is_kept() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        unfinished(&data);
        let base = data.join("base");
        std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o000)).unwrap();

        let found = shape(&data);
        std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o700)).unwrap();

        assert_eq!(found, Shape::HoldsDatabase);
    }

    /// #1985: without `PG_VERSION` and `postgresql.conf`, a directory that
    /// holds a database, or whose `base/` cannot be listed, is kept unmarked.
    #[cfg(unix)]
    #[test]
    fn a_directory_without_pg_version_that_may_hold_a_database_is_kept() {
        use std::os::unix::fs::PermissionsExt;
        for fixture in ["base/16384", "pg_tblspc/16385", "unreadable base"] {
            let dir = tempfile::tempdir().unwrap();
            let data = dir.path().join("data");
            let base = data.join("base");
            std::fs::create_dir_all(base.join("1")).unwrap();
            std::fs::create_dir_all(data.join("pg_tblspc")).unwrap();
            if fixture == "unreadable base" {
                std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o000)).unwrap();
            } else {
                std::fs::create_dir_all(data.join(fixture)).unwrap();
            }

            let found = shape(&data);
            recover_or_mark(&data).unwrap();
            let kept = base.exists();
            let _ = std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o700));

            assert_eq!(found, Shape::Unrecognised, "{fixture}");
            assert!(kept, "{fixture}: kept");
            assert!(
                !data.join(INITDB_FINISHED).exists(),
                "{fixture}: not marked"
            );
        }
    }

    /// #2007: with `PG_VERSION` and without `postgresql.conf`, a directory
    /// that holds a database, may hold one (its `base/` missing), or that a
    /// server ran on is kept unmarked.
    #[test]
    fn a_directory_that_lost_an_initdb_file_and_may_hold_a_database_is_kept() {
        for fixture in [
            "base/16384",
            "pg_tblspc/16385",
            "no base",
            "postmaster.opts",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let data = dir.path().join("data");
            unfinished(&data);
            std::fs::create_dir_all(data.join("base/5")).unwrap();
            std::fs::remove_file(data.join("postgresql.conf")).unwrap();
            match fixture {
                "no base" => std::fs::remove_dir_all(data.join("base")).unwrap(),
                "postmaster.opts" => {
                    write(&data, fixture, "postgres -D data\n");
                }
                database => std::fs::create_dir_all(data.join(database)).unwrap(),
            }

            let found = shape(&data);
            recover_or_mark(&data).unwrap();

            assert_eq!(found, Shape::Unrecognised, "{fixture}");
            assert!(data.join("PG_VERSION").is_file(), "{fixture}: kept");
            assert!(
                !data.join(INITDB_FINISHED).exists(),
                "{fixture}: not marked"
            );
        }
    }

    /// #1985: an empty or absent data directory is still not initialised, and
    /// is left for `initdb`.
    #[test]
    fn an_empty_or_absent_data_directory_is_not_initialised() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        assert_eq!(shape(&data), Shape::NotInitialised, "absent");
        std::fs::create_dir_all(&data).unwrap();
        assert_eq!(shape(&data), Shape::NotInitialised, "empty");
        recover_or_mark(&data).unwrap();
        assert!(data.is_dir(), "left for initdb");
    }

    /// #1819: a marked directory is never removed, whatever it lacks later.
    #[test]
    fn a_marked_directory_is_never_removed() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        unfinished(&data);
        write(&data, INITDB_FINISHED, "");

        recover_or_mark(&data).unwrap();

        assert!(
            data.join("PG_VERSION").is_file(),
            "the marked directory is kept"
        );
    }

    /// #1819: an unfinished directory a postmaster answers for is kept, so
    /// its files are not removed under it. A standalone backend's lock, or a
    /// postmaster's whose port is silent, keeps nothing.
    #[test]
    fn an_unfinished_directory_is_kept_only_while_a_postmaster_answers_for_it() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        unfinished(&data);
        pid_file(&data, std::process::id(), port);

        recover_or_mark(&data).unwrap();
        assert!(
            data.join("PG_VERSION").is_file(),
            "kept while its port answers"
        );

        // `initdb`'s standalone backend: its pid negated, and no listener.
        pid_file(&data, std::process::id(), port);
        let standalone = format!(
            "-{}",
            std::fs::read_to_string(data.join("postmaster.pid")).unwrap()
        );
        write(&data, "postmaster.pid", &standalone);
        recover_or_mark(&data).unwrap();
        assert!(!data.exists(), "a standalone backend's lock keeps nothing");
        unfinished(&data);
        // A port below 1024 that no test can bind: nothing answers there.
        pid_file(&data, std::process::id(), 1);
        recover_or_mark(&data).unwrap();
        assert!(!data.exists(), "removed when nothing answers");
        drop(listener);
    }
}
