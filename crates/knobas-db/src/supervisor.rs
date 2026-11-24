//! The server stops when knobas ends, however knobas ends (2026-11-24).
//!
//! A quit stops the embedded PostgreSQL itself ([`EmbeddedDb::stop`]), and a
//! `SIGTERM`, `SIGINT` or `SIGHUP` is a quit since the ux30 end gate. Some
//! ends run no quit at all. On Linux the commonest is **the display going
//! away**: a logout ends the X server or the Wayland compositor, and GTK ends
//! the process the moment it notices (Xlib's I/O-error handler calls
//! `exit(1)` from whichever thread touches the dead connection first, GDK's
//! Wayland backend `_exit(1)`). That happens before a quit or in the middle
//! of one: a `SIGTERM` followed at once by the display's end logged
//! "stopping the embedded postgres" and then exited 1 before `pg_ctl stop`
//! ran, three runs in three, and the postmaster was left running. A
//! `SIGKILL`, a crash and a second stop signal during a quit leave it the
//! same way, on the Mac too.
//!
//! So every server this process owns gets a supervisor: a `/bin/sh` whose
//! stdin is a pipe from this process. The kernel closes the pipe when this
//! process ends, however it ends; the supervisor reads end-of-file and stops
//! the server with `pg_ctl stop -m fast`, the stop a quit makes, so the data
//! directory is checkpointed and the next start opens it cleanly. A clean
//! stop, or a handle dropped in-process, kills the supervisor first, so it
//! acts only when the process ended with the server still in its hands.
//!
//! The test harness's per-binary server has had the same kind of supervisor
//! since 34ad77a5 (`test_util::supervise`); this one keeps the data.
//!
//! What it cannot cover: a supervisor killed itself (a `SIGKILL` to every
//! process of the session, which ends the postmaster too). A server left
//! that way is adopted by the next start, as before (`adopt`).
//!
//! Unix only: Windows has no `/bin/sh`, and a GUI process there gets its
//! log-off as window messages; a server a crash leaves there is adopted by
//! the next start.
//!
//! [`EmbeddedDb::stop`]: crate::EmbeddedDb::stop

#[cfg(unix)]
use std::fs::File;
#[cfg(unix)]
use std::path::Path;
#[cfg(unix)]
use std::process::{Child, ChildStdin, Stdio};

/// What the supervisor runs, with `$1` the `pg_ctl` binary and `$2` the data
/// directory.
///
/// Hang-up, interrupt and terminate are ignored, and stay ignored across
/// `pg_ctl`'s `exec`: the supervisor's job is to outlive whatever ends knobas,
/// a Ctrl-C at the terminal or a session's `SIGTERM` to every process
/// included. The `read` loop ends at end-of-file, since nothing is ever
/// written to the pipe. Its stdout is the ownership lock (see [`spawn`]), so
/// nothing in here may write to it. `-w` waits until `postmaster.pid` is
/// gone, bounded by `-t 60`; a server already stopped makes `pg_ctl` return
/// at once. One line, so `ps` shows it on one.
#[cfg(unix)]
const SCRIPT: &str = r#"trap '' HUP INT TERM; while read -r _; do :; done; "$1" stop -D "$2" -m fast -w -t 60 >/dev/null 2>&1"#;

/// The name the supervisor shows in `ps`, after `/bin/sh -c`.
pub const NAME: &str = "knobas-db-supervisor";

/// A running supervisor. Dropping it kills it, so it stops nothing: only
/// this process's end, with the handle still alive, lets it act.
#[cfg(unix)]
pub(crate) struct Supervisor {
    child: Child,
    /// The pipe's only write end, never written: its close is the signal.
    _stdin: ChildStdin,
}

/// Never made: see the module's last paragraph.
#[cfg(not(unix))]
pub(crate) enum Supervisor {}

#[cfg(unix)]
impl Drop for Supervisor {
    fn drop(&mut self) {
        // Its own child, not yet waited for, so the pid is still its own.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Start the supervisor of the server serving `data_dir`, whose `pg_ctl` is
/// under `installation_dir` (or the shared default installation).
///
/// **The pipe.** Its stdin is a pipe whose only write end is this process's,
/// opened close-on-exec, so no server, `git` or `pg_ctl` this process starts
/// holds a copy, and end-of-file means this process has ended. (Where there
/// is no `pipe2`, macOS among them, the flag is set just after the pipe is
/// made: a process another thread spawns in that instant holds a copy, and
/// the supervisor waits for that one to end as well.)
///
/// **The lock.** Its stdout is a duplicate of the ownership lock, `lock`. A
/// `flock` belongs to the open file description, which the supervisor
/// shares, so after an unclean end the profile stays owned until the stop is
/// done: a start in that instant is refused rather than adopting the server
/// the supervisor is about to stop. A clean stop releases the lock itself
/// (`OwnerLock`'s drop unlocks the description), after killing this.
///
/// **Its own process group**, so a Ctrl-C at the terminal, or a signal to
/// knobas's group, never reaches it.
///
/// `None`, logged, when it cannot be started: the server is then stopped by
/// a quit alone, as before.
#[cfg(unix)]
pub(crate) fn spawn(data_dir: &Path, installation_dir: &Path, lock: &File) -> Option<Supervisor> {
    use std::os::unix::process::CommandExt;

    let pg_ctl =
        crate::embedded::find_tool(installation_dir, crate::embedded::PG_CTL).or_else(|| {
            crate::embedded::find_tool(
                &crate::embedded::installation_dir(),
                crate::embedded::PG_CTL,
            )
        });
    let Some(pg_ctl) = pg_ctl else {
        tracing::warn!("no pg_ctl found: the server has no supervisor");
        return None;
    };
    let held = match lock.try_clone() {
        Ok(held) => held,
        Err(error) => {
            tracing::warn!(%error, "the server has no supervisor");
            return None;
        }
    };
    let spawned = crate::console::command("/bin/sh")
        .arg("-c")
        .arg(SCRIPT)
        .arg(NAME)
        .arg(pg_ctl)
        .arg(data_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::from(held))
        .stderr(Stdio::null())
        .process_group(0)
        .spawn();
    match spawned {
        Ok(mut child) => {
            let stdin = child.stdin.take()?;
            tracing::info!(pid = child.id(), "the server's supervisor runs");
            Some(Supervisor {
                child,
                _stdin: stdin,
            })
        }
        Err(error) => {
            tracing::warn!(%error, "the server has no supervisor");
            None
        }
    }
}

/// No `/bin/sh`: see the module's last paragraph.
#[cfg(not(unix))]
pub(crate) fn spawn(
    _data_dir: &std::path::Path,
    _installation_dir: &std::path::Path,
    _lock: &std::fs::File,
) -> Option<Supervisor> {
    None
}
