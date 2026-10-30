//! The one builder for a console program knobas starts: git, docker, the
//! PostgreSQL tools (#2232).
//!
//! The release app is `windows_subsystem = "windows"`, so it has no console.
//! Windows gives every console program such a process starts a console of its
//! own, and with it a window, unless the start asks for none. The Code view
//! reads its checkout on every window focus; each git it ran opened a window
//! that took the focus, and the focus coming back started the next read.
//!
//! Every production `Command::new` goes through [`command`];
//! `knobas-app`'s `console_choke_point` test fails on one that does not.

use std::ffi::OsStr;
use std::process::Command;

/// `program` as a [`Command`] that opens no window on Windows. Elsewhere it is
/// `Command::new`. A tokio caller takes it with `tokio::process::Command::from`.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    #[allow(unused_mut, reason = "only Windows sets a flag on it")]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// The program gets a console, as a console program needs, but no window.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::os::windows::process::CommandExt;
    use std::path::Path;
    use std::process::Stdio;

    /// Which of the three processes this run of the test is.
    const ROLE: &str = "KNOBAS_CONSOLE_PROBE";
    /// Where the run reports: a file for the probe, a directory for the app.
    /// A file, not stdout: a probe given a console of its own does not write
    /// to the pipe its parent read (the first Windows run printed into the
    /// job's log and handed the parent nothing).
    const REPORT: &str = "KNOBAS_CONSOLE_REPORT";
    const NAME: &str = "console::tests::a_console_program_gets_no_window";
    /// What a GUI app is to the programs it starts: a process with no console.
    const DETACHED_PROCESS: u32 = 0x0000_0008;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetConsoleWindow() -> *mut std::ffi::c_void;
    }

    /// This test again, in `role`, started by `command`, reporting to `report`.
    fn rerun(mut command: Command, role: &str, report: &Path) {
        let status = command
            .args([NAME, "--exact", "--test-threads=1"])
            .env(ROLE, role)
            .env(REPORT, report)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("the test binary runs again");
        assert!(status.success(), "the {role} run failed: {status}");
    }

    fn read(report: &Path) -> String {
        std::fs::read_to_string(report)
            .unwrap_or_else(|error| format!("no report at {}: {error}", report.display()))
    }

    /// Three processes: the test starts an app with no console, the app starts
    /// the probe twice, once plain and once through [`command`], and the probe
    /// says whether it got a console window. The plain start must get one, or
    /// this machine cannot show the bug and the test says so.
    #[test]
    fn a_console_program_gets_no_window() {
        let me = std::env::current_exe().expect("the test binary's path");
        match std::env::var(ROLE).as_deref() {
            Ok("probe") => {
                // SAFETY: no arguments; it only reads the calling process's console.
                let window = unsafe { GetConsoleWindow() };
                let report = std::env::var_os(REPORT).expect("the probe's report file");
                std::fs::write(report, format!("window={}", !window.is_null()))
                    .expect("the probe writes its report");
            }
            Ok("app") => {
                let dir = std::path::PathBuf::from(std::env::var_os(REPORT).expect("the app's report dir"));
                rerun(Command::new(&me), "probe", &dir.join("plain"));
                rerun(command(&me), "probe", &dir.join("built"));
            }
            _ => {
                let dir = std::env::temp_dir().join(format!("knobas-console-{}", std::process::id()));
                std::fs::create_dir_all(&dir).expect("a report directory");
                let mut app = Command::new(&me);
                app.creation_flags(DETACHED_PROCESS);
                rerun(app, "app", &dir);
                let (plain, built) = (read(&dir.join("plain")), read(&dir.join("built")));
                let _ = std::fs::remove_dir_all(&dir);
                assert_eq!(
                    plain, "window=true",
                    "a plain start from a process with no console got no window, so \
                     this machine cannot witness the bug"
                );
                assert_eq!(
                    built, "window=false",
                    "a console program started through `console::command` got a window"
                );
            }
        }
    }
}
