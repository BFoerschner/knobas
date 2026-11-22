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
//! it has no socket in the data folder: [`usable`] says so before the server
//! is asked. On Windows the socket then goes to a short folder of its own,
//! [`short_folder`] (the Windows pass on ux28: a scratch profile 41 bytes
//! deeper than Björn's started on TCP alone, which is exactly the managed
//! machine, with a long user name or a redirected AppData, the socket is
//! for). Where that folder cannot hold one either, the start is TCP alone, as
//! every start was before.

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

/// The folder a Windows server's socket goes to when its data folder cannot
/// hold one: `%LOCALAPPDATA%\knobas\<key>`, one per data folder ([`key`]), so
/// the demo and the default profile never share one. Local AppData, not a
/// shared folder: its inherited access list admits only the user, so a socket
/// in it is still our server's (any local user can make folders under
/// `C:\ProgramData` and listen there first), and `%TEMP%` is longer and gets
/// cleaned. `None` off Windows (TCP comes first there, and the only short
/// place is a shared `/tmp`) and where `LOCALAPPDATA` is unset or relative.
///
/// A test names its own root for it on any platform with
/// `embedded::seam::fall_back_to`.
#[must_use]
pub fn short_folder(data_dir: &Path) -> Option<PathBuf> {
    #[cfg(feature = "test-util")]
    if let Some(root) = crate::embedded::seam::fallback_root(data_dir) {
        return Some(root.join(key(data_dir)));
    }
    short_folder_under(cfg!(windows), std::env::var_os("LOCALAPPDATA"), data_dir)
}

/// [`short_folder`] off a test's root: Windows' `%LOCALAPPDATA%\knobas\<key>`
/// where it is an absolute path, and nothing on any other platform, whatever
/// the environment says.
fn short_folder_under(
    windows: bool,
    local_app_data: Option<std::ffi::OsString>,
    data_dir: &Path,
) -> Option<PathBuf> {
    if !windows {
        return None;
    }
    let local = PathBuf::from(local_app_data?);
    local
        .is_absolute()
        .then(|| local.join("knobas").join(key(data_dir)))
}

/// The folders a server of `data_dir` may have its socket in, in the order a
/// start offers them and a liveness check or an adoption looks: the data
/// folder, then [`short_folder`].
#[must_use]
pub fn folders(data_dir: &Path) -> Vec<PathBuf> {
    std::iter::once(data_dir.to_path_buf())
        .chain(short_folder(data_dir))
        .collect()
}

/// The first of [`folders`] that holds `port`'s socket file: where a server
/// left running listens. Not line 5 of `postmaster.pid`, which a data folder
/// copied from another machine carries from there.
#[must_use]
pub fn find(data_dir: &Path, port: u16) -> Option<PathBuf> {
    folders(data_dir)
        .into_iter()
        .find(|dir| file(dir, port).exists())
}

/// A data folder's name for its short socket folder: the first 8 hex digits
/// of FNV-1a 64 over its path in lower case. Written here, never
/// `DefaultHasher`, whose output may change between Rust releases: the name
/// has to be the same for every build that adopts the server.
#[must_use]
pub fn key(data_dir: &Path) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0100_0000_01b3;
    let lower = data_dir.to_string_lossy().to_lowercase();
    let hash = lower.bytes().fold(OFFSET, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(PRIME)
    });
    format!("{:08x}", hash >> 32)
}

/// Whether the server's command line can carry `dir` as an absolute
/// `unix_socket_directories`: `postgresql_embedded` passes each setting as
/// `-o "-c key=value"`, which `pg_ctl` splits on whitespace, so a space (or
/// anything past plain ASCII path characters) breaks the start. The data
/// folder itself never meets this: it is passed as `.`.
///
/// # Errors
/// The reason, for the start's report.
pub fn carriable(dir: &Path) -> Result<(), String> {
    let plain = |byte: u8| byte.is_ascii_alphanumeric() || b"\\/:._-~".contains(&byte);
    if dir.as_os_str().as_encoded_bytes().iter().all(|&b| plain(b)) {
        Ok(())
    } else {
        Err("the folder's name has characters the server's command line cannot carry".to_owned())
    }
}

/// `dir` as the server's `unix_socket_directories`: absolute, with forward
/// slashes, which PostgreSQL reads on Windows as on Unix and which needs no
/// quoting on its command line.
#[must_use]
pub fn setting(dir: &Path) -> String {
    dir.to_string_lossy().replace('\\', "/")
}

/// Make `dir` and check it takes a socket: [`carriable`], then the folder,
/// then [`usable`].
///
/// # Errors
/// A sentence naming what failed.
pub fn prepare(dir: &Path) -> Result<(), String> {
    carriable(dir)?;
    std::fs::create_dir_all(dir)
        .map_err(|error| format!("the folder could not be made: {error}"))?;
    usable(dir)
}

/// Remove the socket files a killed server left in `dir`: `.s.PGSQL.<n>` and
/// `.s.PGSQL.<n>.lock` for every `n` but `port`, the running server's. Every
/// start picks a new port, so each kill leaves a pair behind. Never fails:
/// what cannot be removed stays for the next start.
pub fn sweep(dir: &Path, port: u16) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(rest) = name.strip_prefix(".s.PGSQL.") else {
            continue;
        };
        let number = rest.strip_suffix(".lock").unwrap_or(rest);
        match number.parse::<u16>() {
            Ok(other) if other != port => {
                let _ = std::fs::remove_file(entry.path());
            }
            _ => {}
        }
    }
}

/// After a clean stop: remove the short folder `dir` and `%LOCALAPPDATA%\knobas`
/// above it, each only when empty, so a quit leaves nothing behind. Never
/// fails, and never touches the data folder (the caller passes the short one
/// alone).
pub fn tidy_short(dir: &Path) {
    let _ = std::fs::remove_dir(dir);
    if let Some(parent) = dir.parent() {
        let _ = std::fs::remove_dir(parent);
    }
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

    /// The short folder is Windows' alone, under an absolute
    /// `%LOCALAPPDATA%`; elsewhere there is none even where the variable is
    /// set (the final suite's mutants: the platform test turned around).
    #[test]
    fn the_short_folder_is_windows_alone_under_an_absolute_local_app_data() {
        let data = Path::new("/deep/profile/data");
        let local = || {
            Some(std::ffi::OsString::from(if cfg!(windows) {
                r"C:\Users\b\AppData\Local"
            } else {
                "/home/b/.local"
            }))
        };
        assert_eq!(short_folder_under(false, local(), data), None);
        let under = short_folder_under(true, local(), data).expect("an absolute folder");
        assert!(
            under.ends_with(Path::new("knobas").join(key(data))),
            "{}",
            under.display()
        );
        assert_eq!(
            short_folder_under(true, Some("relative".into()), data),
            None
        );
        assert_eq!(short_folder_under(true, None, data), None);
    }

    /// A test's own root stands in for the short folder until it is undone.
    #[cfg(feature = "test-util")]
    #[test]
    fn a_tests_fallback_root_lasts_until_it_is_undone() {
        let data = Path::new("/tmp/knobas-seam-test/data");
        let root = Path::new("/tmp/knobas-seam-test/root");
        crate::embedded::seam::fall_back_to(data, root);
        assert_eq!(short_folder(data), Some(root.join(key(data))));
        crate::embedded::seam::no_fallback(data);
        if !cfg!(windows) {
            assert_eq!(short_folder(data), None, "undone");
        }
    }
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

    /// The short folder's name: 8 hex digits, the same for every build and
    /// for the path in any case, and another for another data folder.
    #[test]
    fn the_key_is_stable_ignores_case_and_tells_folders_apart() {
        let one = key(Path::new(
            r"C:\Users\Long.Name\AppData\Roaming\dev.knobas.desktop\db\data",
        ));
        assert_eq!(one.len(), 8);
        assert!(one.bytes().all(|b| b.is_ascii_hexdigit()), "{one}");
        assert_eq!(
            one,
            key(Path::new(
                r"c:\users\long.name\appdata\roaming\dev.knobas.desktop\db\data"
            ))
        );
        assert_ne!(
            one,
            key(Path::new(
                r"C:\Users\Long.Name\AppData\Roaming\dev.knobas.desktop\demo\db\data"
            ))
        );
        // FNV-1a 64 of "a" is af63dc4c8601ec8c: the key is its top 32 bits.
        assert_eq!(key(Path::new("A")), "af63dc4c");
    }

    /// A folder the server's command line cannot carry is refused with its
    /// reason, before anything is made.
    #[test]
    fn a_folder_with_a_space_is_not_carriable() {
        assert_eq!(
            carriable(Path::new(r"C:\Users\bjfoer\AppData\Local\knobas\1a2b3c4d")),
            Ok(())
        );
        let refused = carriable(Path::new(
            r"C:\Users\John Smith\AppData\Local\knobas\1a2b3c4d",
        ))
        .expect_err("a space");
        assert!(refused.contains("cannot carry"), "{refused}");
        assert!(carriable(Path::new("/tmp/björn/x")).is_err());
        assert!(prepare(Path::new("/tmp/kp space/x")).is_err());
        assert!(!Path::new("/tmp/kp space").exists(), "nothing made");
        assert_eq!(
            setting(Path::new(r"C:\Users\b\AppData\Local\knobas\1a")),
            "C:/Users/b/AppData/Local/knobas/1a"
        );
    }

    /// The data folder first, always; off Windows nothing else unless a test
    /// names a root.
    #[test]
    fn the_data_folder_comes_first() {
        let data = Path::new("/nowhere/data");
        let found = folders(data);
        assert_eq!(found[0], data);
        if !cfg!(windows) {
            assert_eq!(found.len(), 1, "{found:?}");
        }
        assert_eq!(find(data, 5432), None);
    }

    /// A killed server's pair goes, the running server's stays, and nothing
    /// else in the folder is touched.
    #[test]
    fn a_sweep_removes_other_ports_files_only() {
        let dir = tempfile::Builder::new().tempdir_in(short_root()).unwrap();
        for name in [
            ".s.PGSQL.5000",
            ".s.PGSQL.5000.lock",
            ".s.PGSQL.6000",
            ".s.PGSQL.6000.lock",
            "postgresql.conf",
            ".s.PGSQL.x",
        ] {
            std::fs::write(dir.path().join(name), "").unwrap();
        }
        sweep(dir.path(), 6000);
        let mut left: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(
            left,
            [
                ".s.PGSQL.6000",
                ".s.PGSQL.6000.lock",
                ".s.PGSQL.x",
                "postgresql.conf"
            ]
        );
    }

    /// A tidy removes the short folder and its parent when empty, and leaves
    /// a parent that holds something else.
    #[test]
    fn a_tidy_removes_only_empty_folders() {
        let root = tempfile::Builder::new().tempdir_in(short_root()).unwrap();
        let parent = root.path().join("knobas");
        let short = parent.join("1a2b3c4d");
        std::fs::create_dir_all(&short).unwrap();
        std::fs::create_dir_all(parent.join("other")).unwrap();
        tidy_short(&short);
        assert!(!short.exists());
        assert!(parent.exists(), "it holds another folder");
        std::fs::remove_dir(parent.join("other")).unwrap();
        std::fs::create_dir_all(&short).unwrap();
        tidy_short(&short);
        assert!(!parent.exists());
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
