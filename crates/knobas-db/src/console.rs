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
    Command::new(program)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::os::windows::process::CommandExt;
    use std::process::Stdio;

    /// Which of the three processes this run of the test is.
    const ROLE: &str = "KNOBAS_CONSOLE_PROBE";
    const NAME: &str = "console::tests::a_console_program_gets_no_window";
    /// What a GUI app is to the programs it starts: a process with no console.
    const DETACHED_PROCESS: u32 = 0x0000_0008;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetConsoleWindow() -> *mut std::ffi::c_void;
    }

    /// This test again, in `role`, started by `command`; what it printed.
    fn rerun(mut command: Command, role: &str) -> String {
        let out = command
            .args([NAME, "--exact", "--nocapture", "--test-threads=1"])
            .env(ROLE, role)
            .stdin(Stdio::null())
            .output()
            .expect("the test binary runs again");
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// The probe's verdict out of what its run printed, or all of it.
    fn verdict(said: &str) -> &str {
        said.lines()
            .find(|line| line.starts_with("probe:"))
            .unwrap_or(said)
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
                println!("probe: window={}", !window.is_null());
            }
            Ok("app") => {
                println!("plain: {}", verdict(&rerun(Command::new(&me), "probe")));
                println!("built: {}", verdict(&rerun(command(&me), "probe")));
            }
            _ => {
                let mut app = Command::new(&me);
                app.creation_flags(DETACHED_PROCESS);
                let said = rerun(app, "app");
                assert!(
                    said.contains("plain: probe: window=true"),
                    "a plain start from a process with no console got no window, so \
                     this machine cannot witness the bug: {said}"
                );
                assert!(
                    said.contains("built: probe: window=false"),
                    "a console program started through `console::command` got a \
                     window: {said}"
                );
            }
        }
    }
}
