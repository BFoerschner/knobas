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
pub(crate) const PG_CTL: &str = if cfg!(windows) {
    "pg_ctl.exe"
} else {
    "pg_ctl"
};

/// Pool size. The desktop app is a single user with a handful of concurrent
/// queries; a larger pool only buys idle backends.
const MAX_CONNECTIONS: u32 = 5;

/// Upper bound for `initdb` / `pg_ctl` invocations. The crate default is 5s,
/// which a cold `initdb` on a slow disk can exceed.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(60);

/// How long to wait for a TCP handshake when deciding whether a recorded
/// `postmaster.pid` still has a live server behind it.
const LIVENESS_TIMEOUT: Duration = Duration::from_millis(500);

/// Upper bound on the *first* connection to a server, before a pool exists.
///
/// M0 let `PgPoolOptions::connect` decide, and its acquire timeout is 30 s: a
/// `KNOBAS_DB_URL` pointing at a dead port therefore left the boot screen on
/// "Starting the local database" for a full half-minute before saying anything
/// (M1 carry-over, observed by stream D). A refused handshake is instant and a
/// live server answers in milliseconds, so five seconds is generous for the
/// question actually being asked -- and it bounds only the probe, not the
/// pool's own acquire timeout, which still has to cover a busy database.
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
    /// stopping the server. `None` when a live sibling owns it, and when
    /// `existing_url` points at a server knobas does not manage at all.
    owner_lock: Option<File>,
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
        }
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

    /// A pool of `max_connections` on the same server.
    ///
    /// # Errors
    /// [`sqlx::Error`] if the first connection fails.
    pub async fn pool(&self, max_connections: u32) -> Result<PgPool, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .connect_lazy_with(self.options.clone());
        probe(&pool).await?;
        Ok(pool)
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

/// Take one connection out of `pool`, under [`FIRST_CONNECT_TIMEOUT`].
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
            let connector = Connector::parse(url)?;
            return Ok(EmbeddedDb {
                pool: connector.pool(MAX_CONNECTIONS).await?,
                connector,
                postgresql: None,
                stop_settings: None,
                // Never ours to stop, whatever any lock file says.
                owner_lock: None,
            });
        }

        let settings = build_settings(&cfg.root_dir)?;
        launch(&cfg.root_dir, settings).await
    }

    /// [`start`](Self::start) for a managed server with a larger connection
    /// limit than the desktop app's server has.
    ///
    /// For the test server `just test` runs: every test binary of a gate opens
    /// its pools on that one server, so PostgreSQL's default ceiling of 100 is
    /// what a parallel run would hit first. The knob is exposed here rather
    /// than through [`DbConfig`] because no shipped path wants it -- hence
    /// `test-util` only.
    ///
    /// # Errors
    ///
    /// As [`start`](Self::start).
    #[cfg(feature = "test-util")]
    pub async fn start_with_max_connections(
        root_dir: PathBuf,
        max_connections: u32,
    ) -> Result<EmbeddedDb, DbError> {
        let mut settings = build_settings(&root_dir)?;
        settings
            .configuration
            .insert("max_connections".to_owned(), max_connections.to_string());
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
    /// False for an externally managed server (`existing_url`), and for a
    /// server a live sibling instance owns.
    #[must_use]
    pub fn owns_server(&self) -> bool {
        self.owner_lock.is_some()
    }

    /// Drop this handle the way a signal does: no `pg_ctl stop`, no pool
    /// close, the ownership lock released by closing its file. The server
    /// keeps running, owned by nobody -- which is exactly the state a
    /// `kill -9` leaves behind.
    ///
    /// Test-only, and the only way to reach the orphan case in-process: a
    /// `kill -9` is not something a `#[tokio::test]` can do to itself, and the
    /// property under test -- that the *next* launch adopts an orphan **and
    /// takes responsibility for it** -- is the one M0 could not deliver.
    ///
    /// The two halves are deliberately different. The lock file is **dropped**,
    /// because the kernel is what releases an OS lock when a process dies and
    /// an orphan with a live lock holder would be indistinguishable from a
    /// live sibling's server. The pool and the `PostgreSQL` handle are
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
    /// database`s' worth of files for that checkpoint to sync: 1.2 s of fast
    /// stop against 0.2 s of immediate (#421). This is `-m immediate`: the
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
    /// [`DbError::Io`] if `pg_ctl` cannot be found or reports a failure.
    #[cfg(feature = "test-util")]
    pub async fn discard(self) -> Result<(), DbError> {
        close_pool_bounded(&self.pool).await;
        if self.owner_lock.is_none() {
            tracing::info!("not this process's server to stop");
            return Ok(());
        }
        let data_dir = match (&self.postgresql, &self.stop_settings) {
            (Some(postgresql), _) => postgresql.settings().data_dir.clone(),
            (None, Some(settings)) => settings.data_dir.clone(),
            // Externally managed: never ours, whatever the lock says.
            (None, None) => return Ok(()),
        };
        let Some(pg_ctl) = pg_ctl_stop_immediate(&data_dir) else {
            return Err(DbError::io(
                installation_dir(),
                io::Error::new(io::ErrorKind::NotFound, format!("no {PG_CTL} under it")),
            ));
        };
        let output = tokio::process::Command::from(pg_ctl)
            .output()
            .await
            .map_err(|source| DbError::io(&data_dir, source))?;
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
            clear_lock(&data_dir)?;
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

/// Claim responsibility for the server serving `root_dir`, if nobody else has.
///
/// Non-blocking on purpose: a live sibling holding this lock is the answer,
/// not a delay. `None` means "somebody else owns it" -- which is exactly when
/// stopping the server would take a running instance's database down with it.
fn claim_ownership(root_dir: &Path) -> Result<Option<File>, DbError> {
    let path = root_dir.join(OWNER_LOCK);
    let file = File::create(&path).map_err(|source| DbError::io(&path, source))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => {
            tracing::info!("another knobas instance owns this server; it will stop it");
            Ok(None)
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
///
/// The superuser password is read **here**, under that lock, and deliberately
/// not by [`build_settings`]: see [`adopt_recorded_password`].
async fn start_managed(root_dir: &Path, mut settings: Settings) -> Result<Started, DbError> {
    let _guard = bring_up_lock(root_dir).await?;
    adopt_recorded_password(&mut settings)?;

    let data_dir = settings.data_dir.clone();
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
                #[cfg(feature = "test-util")]
                seam::stale_lock_cleared();
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
                return adopt(root_dir, adoption, port, pid).await;
            }
            Lock::LiveProcess { pid } => {
                std::mem::forget(postgresql);
                return Err(DbError::AlreadyRunning {
                    data_dir,
                    reason: format!(
                        "postmaster.pid records process {pid}, which is still running, but \
                         nothing answers on the port it recorded (the start failed with: {error})"
                    ),
                });
            }
        }
    }

    if !postgresql.database_exists(DATABASE_NAME).await? {
        postgresql.create_database(DATABASE_NAME).await?;
    }

    let url = postgresql.settings().url(DATABASE_NAME);
    let connector = Connector::parse(&url)?;
    let pool = connector.pool(MAX_CONNECTIONS).await?;

    // We started it, so we stop it -- but the claim still goes through the
    // lock, so `owns_server` has exactly one meaning and a sibling that later
    // adopts this server can see that somebody is already responsible for it.
    let owner_lock = claim_ownership(root_dir)?;

    tracing::info!(port = postgresql.settings().port, "embedded postgres ready");

    Ok(Started::Ready(Box::new(EmbeddedDb {
        pool,
        connector,
        postgresql: Some(postgresql),
        stop_settings: None,
        owner_lock,
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
/// **Ownership is decided here, and M1 changed the answer.** M0 adopted
/// without owning: `postgresql` stayed `None` and `stop` left the server up,
/// so a server orphaned by a signal-kill was never stopped by anybody -- "from
/// then until the machine reboots there is one PostgreSQL running per
/// profile". Now the adopter tries [`claim_ownership`]:
///
/// * **Nobody holds the lock** -- the orphan case. This process takes
///   responsibility and stops the server on a clean quit, which closes M0's
///   one-way door.
/// * **A live sibling holds it** -- two knobas windows on one profile. The
///   adopter borrows only; the instance that started the server is the one
///   that stops it, and taking it down under a live sibling is precisely what
///   the lock prevents.
///
/// # The last uncovered corner
///
/// A **live recycled PID** whose recorded port is answered by something that
/// is not PostgreSQL still yields [`DbError::AlreadyRunning`]: the pid check
/// says the lock may still be owned, and nothing else identifies what
/// answered. Accepted as documented rather than covered by more machinery --
/// the recovery is to delete `data/postmaster.pid` by hand. Every other shape
/// of stale lock is handled above.
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
async fn adopt(
    root_dir: &Path,
    mut settings: Settings,
    port: u16,
    pid: Option<u32>,
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
    if provably_different(Path::new(&serving.0), &data_dir) {
        // A mismatch clears the lock **whatever the recorded pid is doing** --
        // deliberately, and not an oversight of the rule the wire-level arm
        // above applies ("a live recorded process means the lock may still be
        // owned"). Ruled, and here is the argument:
        //
        // A postmaster serves exactly one data directory for its whole life.
        // This one answered on the port our lock file names and reported some
        // *other* directory, so it is not the postmaster our lock describes --
        // and no postmaster of ours can be listening there either, since that
        // port is taken. Whatever the recorded pid is, it is not a live
        // postmaster holding this directory. That is strictly more evidence
        // than the wire-level arm has, where nothing identifies what answered
        // and the pid is the only witness left.
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
            // exists`, so the lookup above is a check-then-act two adopters
            // can interleave: the loser gets this, and the outcome it wanted
            // -- a database that exists -- is exactly what happened.
            let raced = source
                .as_database_error()
                .and_then(sqlx::error::DatabaseError::code)
                .is_some_and(|code| code == "42P04");
            if !raced {
                return Err(unreachable(format!(
                    "cannot create the {DATABASE_NAME} database: {source}"
                )));
            }
            tracing::info!("another adopter created the {DATABASE_NAME} database first");
        }
    }
    let _ = admin.close().await;

    let connector = Connector::parse(&url)?;
    let pool = connector.pool(MAX_CONNECTIONS).await.map_err(|source| {
        unreachable(format!(
            "cannot connect to {DATABASE_NAME} on port {port}: {source}"
        ))
    })?;

    let owner_lock = claim_ownership(root_dir)?;
    tracing::warn!(
        port,
        owned = owner_lock.is_some(),
        "adopting the postgres already serving this data directory"
    );
    Ok(Started::Ready(Box::new(EmbeddedDb {
        pool,
        connector,
        postgresql: None,
        // `settings.port` was set to the recorded port above, and
        // `adopt_recorded_password` gave it the password `initdb` burned in,
        // so this is a handle over the server we are actually connected to.
        stop_settings: Some(settings),
        owner_lock,
    })))
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

    /// The loser of a first-launch race joins the winner's server, using the
    /// password `initdb` recorded rather than the one it invented.
    ///
    /// This is the cold machine, deterministically: `build_settings` runs
    /// while the profile is empty -- which is all a loser has to do to be
    /// holding a random password nothing accepts -- and only then does the
    /// winner initialise the directory. If the recorded password were read
    /// anywhere but under the bring-up lock, the loser would fail
    /// `start()` against the winner's live server, reach `adopt`, and be told
    /// its own profile belongs to another instance. Permanently: every later
    /// launch that lost by a microsecond does the same thing.
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
            Err(error) => panic!("the loser must join the winner's server: {error}"),
        };
        let one: (i32,) = sqlx::query_as("select 1")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(one.0, 1);

        // Adopted, so it owns nothing; the winner still owns the server.
        (*db).stop().await.unwrap();
        winner.stop().await.unwrap();
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

    /// A silent port is *not* on its own proof that the lock is free: a
    /// postmaster still starting up, or wedged before it opened its socket,
    /// fails the handshake while owning the data directory. Clearing the lock
    /// there is how a second server ends up writing the same files.
    #[test]
    fn inspect_lock_keeps_a_pid_file_whose_process_is_still_alive() {
        let dir = tempfile::tempdir().unwrap();
        // Our own pid: alive for certain, for as long as this test runs.
        let alive = std::process::id();
        let (lock, pid_file) = inspect_with_a_silent_port(dir.path(), alive);

        assert_eq!(lock, Lock::LiveProcess { pid: alive });
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

    /// The recycled-port lockout: a dead postmaster's port now belongs to some
    /// unrelated program. That is not another knobas instance, and reporting it
    /// as one leaves the user with an app that refuses to launch until they
    /// delete `postmaster.pid` by hand.
    #[tokio::test]
    async fn adopt_treats_a_non_postgres_listener_as_a_stale_lock() {
        let dir = tempfile::tempdir().unwrap();
        let port = squatter();

        match adopt(
            dir.path(),
            adoption_settings(dir.path()),
            port,
            Some(dead_pid()),
        )
        .await
        {
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
            dir.path(),
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
