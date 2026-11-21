//! What one bring-up of the embedded server did, step by step, for a failure
//! that has to explain itself (ux28).
//!
//! A start failed on a Windows machine Björn has no admin rights on, with
//! "error communicating with database: ... (os error 10013)" and nothing else:
//! no log could be fetched from that machine, so the one line on the screen
//! was all there was to go on. A [`StartReport`] is what that line lacked: the
//! step the start reached, what the data folder held when it began (a fresh
//! start or an adoption), and every connection the start tried, with the
//! address or socket path it dialled and the operating system's own error.
//! `knobas-app` adds the machine's facts and puts it under the boot screen's
//! *Copy details*.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

/// How far a bring-up got. Ordered: a later step is a later variant.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    /// Locks, settings, the data folder's state: nothing started yet.
    #[default]
    Prepare,
    /// Unpacking or finding the PostgreSQL binaries.
    Install,
    /// Making the data folder (`initdb`), on a first start.
    Initdb,
    /// `pg_ctl start`: the server process coming up.
    ServerStart,
    /// The server came up but could not listen: its port or socket file.
    Bind,
    /// The server is up, and knobas's own connection to it failed before
    /// PostgreSQL said a word: the operating system refused or dropped it.
    ClientConnect,
    /// Connected, and the first statement failed.
    FirstQuery,
    /// Up and answering.
    Ready,
}

impl Step {
    /// The step in a few plain words, as *Copy details* prints it.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Step::Prepare => "preparing the data folder",
            Step::Install => "installing PostgreSQL",
            Step::Initdb => "creating the database files (initdb)",
            Step::ServerStart => "starting the server",
            Step::Bind => "the server listening on its port or socket",
            Step::ClientConnect => "knobas connecting to its server",
            Step::FirstQuery => "the first query",
            Step::Ready => "ready",
        }
    }
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// What the data folder held when the start began, read before the start
/// changed anything: it is what tells a fresh start from an adoption.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Found {
    /// No data folder: a first start.
    Nothing,
    /// A data folder and no `postmaster.pid`: a fresh start of a server that
    /// was stopped cleanly.
    Stopped,
    /// A `postmaster.pid`, naming this pid and port where it could be read:
    /// a server left running (adopted, if it answers) or a crash's leftover.
    Lock {
        pid: Option<u32>,
        port: Option<u16>,
        /// Line 5: the socket folder the server listened in (`.` is the data
        /// folder; empty when it made no socket).
        socket_dir: Option<String>,
        /// Line 6: the address it listened on.
        listen: Option<String>,
    },
}

impl Found {
    /// Read `data_dir` as a start finds it.
    #[must_use]
    pub fn read(data_dir: &Path) -> Found {
        match std::fs::read_to_string(data_dir.join("postmaster.pid")) {
            Ok(contents) => {
                let line = |at: usize| {
                    contents
                        .lines()
                        .nth(at)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_owned)
                };
                Found::Lock {
                    pid: crate::embedded::lock_pid(&contents),
                    port: crate::embedded::lock_port(&contents),
                    socket_dir: line(4),
                    listen: line(5),
                }
            }
            Err(_) if data_dir.is_dir() => Found::Stopped,
            Err(_) => Found::Nothing,
        }
    }
}

impl fmt::Display for Found {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Found::Nothing => f.write_str("no data folder yet (first start)"),
            Found::Stopped => f.write_str("no postmaster.pid (fresh start)"),
            Found::Lock {
                pid,
                port,
                socket_dir,
                listen,
            } => {
                f.write_str("postmaster.pid")?;
                match pid {
                    Some(pid) => write!(f, " pid {pid}")?,
                    None => f.write_str(" pid unreadable")?,
                }
                match port {
                    Some(port) => write!(f, ", port {port}")?,
                    None => f.write_str(", port unreadable")?,
                }
                if let Some(listen) = listen {
                    write!(f, ", listening on {listen}")?;
                }
                match socket_dir {
                    Some(dir) => write!(f, ", socket in {dir}"),
                    None => f.write_str(", no socket"),
                }
            }
        }
    }
}

/// One connection the start tried.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attempt {
    /// What was dialled: `tcp 127.0.0.1:54321`, `unix C:\...\.s.PGSQL.54321`.
    pub route: String,
    /// What came back: `None` when it connected.
    pub error: Option<String>,
    /// The operating system's error number, when the failure was one.
    pub os_code: Option<i32>,
    /// Whether the operating system refused it for permissions: Windows'
    /// WSAEACCES (10013) or a Unix `EACCES`/`EPERM` on a connect, the mark of
    /// a firewall rule or security software that blocks the program.
    pub denied: bool,
    /// Not tried: the route could not be offered (a socket path past the
    /// platform's limit, a folder that refused the file). `error` says why.
    pub skipped: bool,
}

impl Attempt {
    /// A connection that worked.
    #[must_use]
    pub fn connected(route: impl Into<String>) -> Attempt {
        Attempt {
            route: route.into(),
            error: None,
            os_code: None,
            denied: false,
            skipped: false,
        }
    }

    /// A route that was not tried, and why.
    #[must_use]
    pub fn skipped(route: impl Into<String>, why: impl Into<String>) -> Attempt {
        Attempt {
            route: route.into(),
            error: Some(why.into()),
            os_code: None,
            denied: false,
            skipped: true,
        }
    }

    /// A connection that failed with `error`.
    #[must_use]
    pub fn failed(route: impl Into<String>, error: &(dyn std::error::Error + 'static)) -> Attempt {
        let io = io_error_in(error);
        let os_code = io.and_then(io::Error::raw_os_error);
        Attempt {
            route: route.into(),
            error: Some(error.to_string()),
            os_code,
            denied: io.is_some_and(is_access_denied),
            skipped: false,
        }
    }
}

impl fmt::Display for Attempt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.error {
            None => write!(f, "{}: connected", self.route),
            Some(why) if self.skipped => write!(f, "{}: not tried, {why}", self.route),
            Some(error) => {
                write!(f, "{}: {error}", self.route)?;
                // The number once, even when the message already carries it:
                // a reader searches for it.
                if let Some(code) = self.os_code
                    && !error.contains(&format!("os error {code}"))
                {
                    write!(f, " (os error {code})")?;
                }
                Ok(())
            }
        }
    }
}

/// Windows' WSAEACCES: "An attempt was made to access a socket in a way
/// forbidden by its access permissions". On a connect to the loopback it is
/// a Windows Filtering Platform filter: a firewall rule or security software
/// that blocks the program.
pub const WSAEACCES: i32 = 10013;

/// Whether `error` is the operating system refusing a socket for
/// permissions. 10013 counts on every platform (it is no Unix error number,
/// and the test seam raises it on any machine); `PermissionDenied` is how std
/// reads WSAEACCES on Windows and `EACCES`/`EPERM` on Unix.
#[must_use]
pub fn is_access_denied(error: &io::Error) -> bool {
    error.raw_os_error() == Some(WSAEACCES) || error.kind() == io::ErrorKind::PermissionDenied
}

/// The first [`io::Error`] in `error`'s chain, itself included. sqlx's
/// `Io` variant and `postgresql_embedded`'s `DatabaseError` both carry one.
#[must_use]
pub fn io_error_in<'a>(error: &'a (dyn std::error::Error + 'static)) -> Option<&'a io::Error> {
    let mut next = Some(error);
    while let Some(error) = next {
        if let Some(io) = error.downcast_ref::<io::Error>() {
            return Some(io);
        }
        if let Some(sqlx::Error::Io(io)) = error.downcast_ref::<sqlx::Error>() {
            return Some(io);
        }
        next = error.source();
    }
    None
}

/// What a bring-up did. Filled in as it goes, so a failure at any point
/// leaves the steps before it on record.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StartReport {
    /// The last step the start reached.
    pub step: Step,
    /// What the data folder held before the start touched it. `None` for
    /// `KNOBAS_DB_URL`, which has no folder of ours.
    pub found: Option<Found>,
    /// The data folder.
    pub data_dir: Option<PathBuf>,
    /// Whether the start adopted a server left running rather than start one.
    pub adopted: bool,
    /// The `postgres` executable the server runs as, once it is known.
    pub server_exe: Option<PathBuf>,
    /// Every connection tried, in order.
    pub attempts: Vec<Attempt>,
    /// What the server wrote to its log when it last failed to start, from
    /// each line's severity label on: the cause PostgreSQL names before its
    /// `FATAL` summary (the Windows pass on ux28). Empty when no start failed
    /// or a later one worked.
    pub server_said: Vec<String>,
}

impl StartReport {
    /// Move to `step`.
    pub fn reach(&mut self, step: Step) {
        self.step = step;
    }

    /// Record a connection to `route` and what came of it.
    pub fn record<T, E: std::error::Error + 'static>(
        &mut self,
        route: impl Into<String>,
        outcome: &Result<T, E>,
    ) {
        self.attempts.push(match outcome {
            Ok(_) => Attempt::connected(route),
            Err(error) => Attempt::failed(route, error),
        });
    }

    /// Whether a connection was refused for permissions, and none worked
    /// after it: the case the boot screen names in plain words.
    #[must_use]
    pub fn blocked(&self) -> bool {
        let last_success = self.attempts.iter().rposition(|a| a.error.is_none());
        self.attempts
            .iter()
            .enumerate()
            .any(|(at, attempt)| attempt.denied && last_success.is_none_or(|ok| at > ok))
    }

    /// The routes that were tried and failed, each once, in the order first
    /// tried.
    #[must_use]
    pub fn failed_routes(&self) -> Vec<String> {
        let mut routes: Vec<String> = Vec::new();
        for attempt in self
            .attempts
            .iter()
            .filter(|a| a.error.is_some() && !a.skipped)
        {
            if !routes.contains(&attempt.route) {
                routes.push(attempt.route.clone());
            }
        }
        routes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(code: i32) -> sqlx::Error {
        sqlx::Error::Io(io::Error::from_raw_os_error(code))
    }

    /// The 10013 from the report Björn saw: sqlx's I/O error around it is
    /// read through, the number kept, and the attempt marked as refused for
    /// permissions.
    #[test]
    fn a_wsaeacces_connect_is_recorded_as_denied_with_its_number() {
        let mut report = StartReport::default();
        report.record("tcp 127.0.0.1:5", &Err::<(), _>(refused(WSAEACCES)));
        let attempt = &report.attempts[0];
        assert_eq!(attempt.os_code, Some(WSAEACCES));
        assert!(attempt.denied);
        assert!(report.blocked());
        let line = attempt.to_string();
        assert!(line.starts_with("tcp 127.0.0.1:5: "), "{line}");
        assert_eq!(line.matches("os error 10013").count(), 1, "{line}");
    }

    /// Through `postgresql_embedded`'s wrapper too, which is how the first
    /// connect of a fresh start failed in ux27.
    #[test]
    fn the_number_is_found_inside_postgresql_embedded_errors() {
        let wrapped = postgresql_embedded::Error::DatabaseError(refused(WSAEACCES));
        assert_eq!(
            io_error_in(&wrapped).and_then(io::Error::raw_os_error),
            Some(WSAEACCES)
        );
    }

    /// A refused connect that a later route got past is not "blocked": the
    /// start went on. A plain refusal (nothing listening) is not either.
    #[test]
    fn blocked_means_denied_and_nothing_worked_after() {
        let mut report = StartReport::default();
        report.record("tcp 127.0.0.1:5", &Err::<(), _>(refused(WSAEACCES)));
        report.record("unix /tmp/x", &Ok::<(), sqlx::Error>(()));
        assert!(!report.blocked());
        assert_eq!(report.failed_routes(), vec!["tcp 127.0.0.1:5".to_owned()]);

        // A route not offered is not one that failed.
        let mut report = StartReport::default();
        report
            .attempts
            .push(Attempt::skipped("unix /deep", "the path is 120 bytes"));
        report.record("tcp 127.0.0.1:5", &Err::<(), _>(refused(WSAEACCES)));
        assert!(report.blocked());
        assert_eq!(report.failed_routes(), vec!["tcp 127.0.0.1:5".to_owned()]);
        assert_eq!(
            report.attempts[0].to_string(),
            "unix /deep: not tried, the path is 120 bytes"
        );

        let mut report = StartReport::default();
        let not_listening = io::Error::from(io::ErrorKind::ConnectionRefused);
        report.record(
            "tcp 127.0.0.1:5",
            &Err::<(), _>(sqlx::Error::Io(not_listening)),
        );
        assert!(!report.blocked());
    }

    #[test]
    fn found_reads_the_lock_line_by_line() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        assert_eq!(Found::read(&data), Found::Nothing);
        std::fs::create_dir(&data).unwrap();
        assert_eq!(Found::read(&data), Found::Stopped);
        std::fs::write(
            data.join("postmaster.pid"),
            "4242\n/x\n1\n54321\n\n127.0.0.1\n",
        )
        .unwrap();
        let found = Found::read(&data);
        assert_eq!(
            found,
            Found::Lock {
                pid: Some(4242),
                port: Some(54321),
                socket_dir: None,
                listen: Some("127.0.0.1".to_owned()),
            }
        );
        assert_eq!(
            found.to_string(),
            "postmaster.pid pid 4242, port 54321, listening on 127.0.0.1, no socket"
        );
        std::fs::write(data.join("postmaster.pid"), "7\n/x\n1\n5\n.\n127.0.0.1\n").unwrap();
        assert_eq!(
            Found::read(&data).to_string(),
            "postmaster.pid pid 7, port 5, listening on 127.0.0.1, socket in ."
        );
    }
}
