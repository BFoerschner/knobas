//! The embedded server's Unix-domain socket, in its own data folder (ux28,
//! ADR-0053).
//!
//! The server listens on `127.0.0.1` and on `<data folder>/.s.PGSQL.<port>`,
//! and a bring-up picks one of the two for every connection it hands out: the
//! socket first on Windows, where it is faster and where a firewall rule or
//! security software that blocks the app's loopback TCP connect (os error
//! 10013) does not see it; TCP first elsewhere, where large answers are
//! faster over TCP. Windows has AF_UNIX stream sockets since Windows 10 1803;
//! sqlx reaches them through the patched `vendor/sqlx-core`.
//!
//! A socket path has a length limit (`sun_path`: 108 bytes with its NUL on
//! Windows and Linux, 104 on macOS), so a profile folder deep enough to pass
//! it has no socket: [`usable`] says so before the server is asked, and that
//! start is TCP alone, as every start was before.

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The longest socket path the platform takes, in bytes, without its NUL.
const MAX_PATH: usize = if cfg!(target_os = "macos") { 103 } else { 107 };

/// The name [`usable`] tries: as long as the longest `.s.PGSQL.<port>`, so a
/// folder that passes has room for every port.
const PROBE: &str = ".s.knobas.sock";

/// `<dir>/.s.PGSQL.<port>`: the file PostgreSQL listens on.
#[must_use]
pub fn file(dir: &Path, port: u16) -> PathBuf {
    dir.join(format!(".s.PGSQL.{port}"))
}

/// Whether a socket can be bound and connected in `dir`: the path fits, the
/// platform has AF_UNIX, and the folder takes the file. `Err` says why not,
/// for the start's report. The probe's file is removed either way.
///
/// # Errors
/// A sentence naming what failed.
pub fn usable(dir: &Path) -> Result<(), String> {
    let probe = dir.join(PROBE);
    let length = probe.as_os_str().as_encoded_bytes().len();
    if length > MAX_PATH {
        return Err(format!(
            "the path is {length} bytes, past the {MAX_PATH} a socket path may have"
        ));
    }
    // A probe a killed start left behind would make the bind fail.
    let _ = std::fs::remove_file(&probe);
    let tried = bind_and_connect(&probe);
    let _ = std::fs::remove_file(&probe);
    tried.map_err(|error| format!("a socket in this folder failed: {error}"))
}

#[cfg(unix)]
fn bind_and_connect(path: &Path) -> io::Result<()> {
    let listener = std::os::unix::net::UnixListener::bind(path)?;
    let _client = std::os::unix::net::UnixStream::connect(path)?;
    drop(listener);
    Ok(())
}

#[cfg(windows)]
fn bind_and_connect(path: &Path) -> io::Result<()> {
    use socket2::{Domain, SockAddr, Socket, Type};
    let address = SockAddr::unix(path)?;
    let listener = Socket::new(Domain::UNIX, Type::STREAM, None)?;
    listener.bind(&address)?;
    listener.listen(1)?;
    let client = Socket::new(Domain::UNIX, Type::STREAM, None)?;
    client.connect(&address)?;
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn bind_and_connect(_path: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "no Unix-domain sockets here",
    ))
}

/// Whether a server answers on `dir`'s socket for `port` within `within`: a
/// connect, no PostgreSQL conversation. A socket in our own data folder is
/// our server's, so an answer is stronger evidence than one on the port
/// (which a stranger can hold). A missing or dead file refuses at once.
#[must_use]
pub fn answers(dir: &Path, port: u16, within: Duration) -> bool {
    let path = file(dir, port);
    if !path.exists() {
        return false;
    }
    // A connect on a helper thread, so a deadline bounds it even where the
    // platform's connect has none.
    let (sent, answer) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("knobas-socket-probe".to_owned())
        .spawn(move || {
            let _ = sent.send(connect(&path).is_ok());
        });
    spawned.is_ok() && answer.recv_timeout(within).unwrap_or(false)
}

#[cfg(unix)]
fn connect(path: &Path) -> io::Result<()> {
    std::os::unix::net::UnixStream::connect(path).map(drop)
}

#[cfg(windows)]
fn connect(path: &Path) -> io::Result<()> {
    use socket2::{Domain, SockAddr, Socket, Type};
    let client = Socket::new(Domain::UNIX, Type::STREAM, None)?;
    client.connect(&SockAddr::unix(path)?)
}

#[cfg(not(any(unix, windows)))]
fn connect(_path: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "no Unix-domain sockets here",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A short folder takes a socket, and the probe leaves nothing behind.
    #[test]
    fn a_short_folder_is_usable_and_left_clean() {
        let dir = tempfile::Builder::new().tempdir_in(short_root()).unwrap();
        assert_eq!(usable(dir.path()), Ok(()));
        assert!(!dir.path().join(PROBE).exists());
    }

    /// A folder past the limit is refused before any socket is made, and the
    /// reason gives the length.
    #[test]
    fn a_folder_past_the_limit_is_refused_with_its_length() {
        let deep = PathBuf::from("/").join("d".repeat(MAX_PATH));
        let refused = usable(&deep).expect_err("too long");
        assert!(refused.contains("bytes, past the"), "{refused}");
        // One byte under the limit with the probe's name is still accepted
        // by the length check (it then fails on the missing folder instead).
        let fits = PathBuf::from("/").join("d".repeat(MAX_PATH - PROBE.len() - 2));
        let reason = usable(&fits).expect_err("no such folder");
        assert!(!reason.contains("bytes, past the"), "{reason}");
    }

    /// Nothing listening: no answer, at once.
    #[test]
    fn a_missing_socket_does_not_answer() {
        let dir = tempfile::Builder::new().tempdir_in(short_root()).unwrap();
        assert!(!answers(dir.path(), 5432, Duration::from_millis(200)));
    }

    #[cfg(unix)]
    #[test]
    fn a_listening_socket_answers() {
        let dir = tempfile::Builder::new().tempdir_in(short_root()).unwrap();
        let _listener = std::os::unix::net::UnixListener::bind(file(dir.path(), 5432)).unwrap();
        assert!(answers(dir.path(), 5432, Duration::from_secs(2)));
        assert!(!answers(dir.path(), 5433, Duration::from_millis(200)));
    }

    /// The Mac's `$TMPDIR` is too deep for a socket path; `/tmp` is not.
    fn short_root() -> PathBuf {
        if cfg!(unix) {
            PathBuf::from("/tmp")
        } else {
            std::env::temp_dir()
        }
    }
}
