//! The login shell's `PATH` (section 7). A knobas started from the Dock or a
//! desktop launcher has `/usr/bin:/bin:/usr/sbin:/sbin`, where neither
//! `cargo` nor `eslint` is. With *Use my shell's PATH* on, knobas asks the
//! login shell once a session, at the first run and never at start:
//! `$SHELL -l -c 'printf %s "$PATH"'`, with a 5 s limit, in the home folder,
//! and uses the answer as the base `PATH` for commands only.

use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use serde::Serialize;

/// How long the login shell may take.
pub const LIMIT: Duration = Duration::from_secs(5);

/// What the login shell answered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "app.ts", rename = "OneOffShellPath")]
pub struct ShellPath {
    /// The shell asked, `$SHELL`.
    pub shell: String,
    /// Its `PATH`, or `None` where it gave none.
    #[ts(optional = nullable)]
    pub path: Option<String>,
    /// How many folders that `PATH` holds.
    pub folders: u32,
    /// Unix milliseconds.
    pub read_at: i64,
    /// Why there is no `PATH`: the shell failed, printed nothing or took too
    /// long.
    #[ts(optional = nullable)]
    pub failed: Option<String>,
}

static READ: Mutex<Option<ShellPath>> = Mutex::new(None);

/// The answer of this session, read now where there is none or `again`.
pub async fn get(again: bool) -> ShellPath {
    if !again && let Some(known) = READ.lock().unwrap_or_else(PoisonError::into_inner).clone() {
        return known;
    }
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_owned());
    let read = ask(&shell).await;
    *READ.lock().unwrap_or_else(PoisonError::into_inner) = Some(read.clone());
    read
}

/// What this session read, without reading.
#[must_use]
pub fn known() -> Option<ShellPath> {
    READ.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

async fn ask(shell: &str) -> ShellPath {
    let read_at = chrono::Utc::now().timestamp_millis();
    let mut built = knobas_db::console::command(shell);
    knobas_db::console::scrub_knobas_env(&mut built, std::env::vars_os())
        .args(["-l", "-c", "printf %s \"$PATH\""])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    if let Some(home) = std::env::var_os("HOME") {
        built.current_dir(home);
    }
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut built, 0);
    let mut command = tokio::process::Command::from(built);
    command.kill_on_drop(true);
    let failed = |why: String| ShellPath {
        shell: shell.to_owned(),
        path: None,
        folders: 0,
        read_at,
        failed: Some(why),
    };
    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return failed(format!("{shell} could not be started: {error}")),
    };
    let pid = child.id();
    match tokio::time::timeout(LIMIT, child.wait_with_output()).await {
        Ok(Ok(out)) => {
            // A profile may print before the answer: the last line is it.
            let text = String::from_utf8_lossy(&out.stdout);
            let path = text.lines().last().unwrap_or_default().trim().to_owned();
            if path.is_empty() || !path.contains('/') {
                return failed(format!("{shell} printed no PATH"));
            }
            let folders = u32::try_from(path.split(':').filter(|p| !p.is_empty()).count())
                .unwrap_or(u32::MAX);
            ShellPath {
                shell: shell.to_owned(),
                path: Some(path),
                folders,
                read_at,
                failed: None,
            }
        }
        Ok(Err(error)) => failed(format!("{shell} failed: {error}")),
        Err(_) => {
            #[cfg(unix)]
            if let Some(pid) = pid
                .and_then(|p| i32::try_from(p).ok())
                .and_then(rustix::process::Pid::from_raw)
            {
                let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
            }
            let _ = pid;
            failed(format!("{shell} took longer than {} s", LIMIT.as_secs()))
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_shell_that_answers_and_one_that_hangs() {
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("good");
        std::fs::write(
            &good,
            "#!/bin/sh\necho 'welcome!'\necho /opt/x/bin:/usr/bin\n",
        )
        .unwrap();
        let hang = dir.path().join("hang");
        std::fs::write(&hang, "#!/bin/sh\nsleep 30\n").unwrap();
        for f in [&good, &hang] {
            std::os::unix::fs::PermissionsExt::set_mode(
                &mut std::fs::metadata(f).unwrap().permissions(),
                0o755,
            );
            std::fs::set_permissions(f, std::os::unix::fs::PermissionsExt::from_mode(0o755))
                .unwrap();
        }
        let read = ask(&good.to_string_lossy()).await;
        assert_eq!(read.path.as_deref(), Some("/opt/x/bin:/usr/bin"));
        assert_eq!(read.folders, 2);
        let started = std::time::Instant::now();
        let read = ask(&hang.to_string_lossy()).await;
        assert!(read.failed.unwrap().contains("longer than"));
        assert!(started.elapsed() < Duration::from_secs(8));
    }
}
