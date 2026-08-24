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
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

/// The exact PostgreSQL version knobas runs against. Pinned rather than
/// ranged: PG 18 changed generated-column defaults, and the schema depends on
/// `GENERATED ALWAYS AS (...) STORED` behaving the way 18.6 does.
pub const PG_VERSION_REQ: &str = "=18.6.0";

/// Name of the application database inside the server.
pub const DATABASE_NAME: &str = "knobas";

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
/// When `DbConfig::existing_url` was set no server is owned; the handle is
/// then just the pool and `stop` only closes it.
pub struct EmbeddedDb {
    pool: PgPool,
    /// Connection URL of the running server, for callers that need a pool of
    /// their own rather than the shared one.
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
    /// # Errors
    ///
    /// Returns [`DbError`] if the install, `initdb`, start, database creation
    /// or the initial connection fails.
    pub async fn start(cfg: DbConfig) -> Result<EmbeddedDb, DbError> {
        if let Some(url) = cfg.existing_url.as_deref() {
            tracing::info!("connecting to externally managed postgres");
            return Ok(EmbeddedDb {
                pool: connect(url).await?,
                url: url.to_string(),
                postgresql: None,
            });
        }

        let settings = build_settings(&cfg.root_dir)?;
        let data_dir = settings.data_dir.clone();
        let mut postgresql = PostgreSQL::new(settings);

        postgresql.setup().await?;

        if let Err(error) = postgresql.start().await {
            // A hard-killed process (crash, `kill -9`, laptop shutdown) leaves
            // `postmaster.pid` behind and PostgreSQL then refuses to start.
            if clear_stale_lock(&data_dir)? {
                postgresql.start().await?;
            } else {
                return Err(error.into());
            }
        }

        if !postgresql.database_exists(DATABASE_NAME).await? {
            postgresql.create_database(DATABASE_NAME).await?;
        }

        let url = postgresql.settings().url(DATABASE_NAME);
        let pool = connect(&url).await?;

        tracing::info!(port = postgresql.settings().port, "embedded postgres ready");

        Ok(EmbeddedDb {
            pool,
            url,
            postgresql: Some(postgresql),
        })
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
    /// built [`pool`](Self::pool) needs its own. `test_util` is the one caller.
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

/// Remove `postmaster.pid` when it is left over from a dead server.
///
/// Returns `true` when a stale lock was cleared and the start is worth
/// retrying. Returns `false` when there is no lock file, or when something is
/// still answering on the port the lock records -- in that case the data
/// directory genuinely belongs to a live server and must not be stolen.
fn clear_stale_lock(data_dir: &Path) -> Result<bool, DbError> {
    let pid_file = data_dir.join("postmaster.pid");

    let contents = match std::fs::read_to_string(&pid_file) {
        Ok(contents) => contents,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(false),
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
        return Ok(false);
    }

    tracing::warn!(pid_file = %pid_file.display(), "removing stale postmaster.pid");
    std::fs::remove_file(&pid_file).map_err(|source| DbError::io(pid_file, source))?;
    Ok(true)
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
    fn clear_stale_lock_reports_nothing_to_do_without_a_pid_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!clear_stale_lock(dir.path()).unwrap());
    }

    #[test]
    fn clear_stale_lock_removes_a_pid_file_nobody_answers_for() {
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

        assert!(clear_stale_lock(dir.path()).unwrap());
        assert!(!pid_file.exists());
    }

    #[test]
    fn clear_stale_lock_keeps_a_pid_file_a_live_listener_owns() {
        let dir = tempfile::tempdir().unwrap();
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let pid_file = write(
            dir.path(),
            "postmaster.pid",
            &format!("4242\n{}\n1700000000\n{port}\n", dir.path().display()),
        );

        assert!(!clear_stale_lock(dir.path()).unwrap());
        assert!(pid_file.exists());
    }
}
