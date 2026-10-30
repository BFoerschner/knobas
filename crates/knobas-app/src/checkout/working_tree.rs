//! Where a checkout's working tree stands: the one git command knobas runs
//! (#704, ADR-0021).
//!
//! `CONTEXT.md`, **Working tree state**: the branch a checkout is on, its
//! head, its upstream and the distance to it, and how many paths are changed,
//! untracked or conflicted. Read by `git status --porcelain=v2 --branch`, which
//! is git's documented machine format for exactly this question.
//!
//! # Read-only, and the path is the only argument (ADR-0016, ADR-0021)
//!
//! `--no-optional-locks` and `GIT_OPTIONAL_LOCKS=0` keep `status` from
//! refreshing the index it reads, which is the one write a plain `git status`
//! makes. `core.fsmonitor` is off, so no hook or daemon a repository's config
//! names is started. No network: `status` compares against the upstream ref
//! already on disk and never fetches. The only value from outside the program
//! is the checkout path, which `checkout::view` resolved off this disk; no
//! field of the mirror reaches the command, and the mirrored branch name is
//! compared to [`WorkingTreeState::branch`] by whoever draws it.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::Serialize;

/// How long a read may take before it is abandoned and the panel says so.
pub const TIMEOUT: Duration = Duration::from_secs(2);

/// What a checkout's working tree is doing right now.
///
/// Issue #704.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct WorkingTreeState {
    /// The branch checked out; `None` for a detached head.
    pub branch: Option<String>,
    /// The commit checked out; `None` in a repository with no commit yet.
    pub head: Option<String>,
    /// The upstream the branch tracks, as git names it (`origin/main`).
    pub upstream: Option<String>,
    /// Commits on the branch its upstream does not have: *to push*. Zero
    /// without an upstream, or with one whose ref is not on disk.
    pub ahead: u32,
    /// Commits on the upstream the branch does not have: *to pull*.
    pub behind: u32,
    /// Tracked paths that differ from `HEAD`, staged or not, renames as one.
    pub changed: u32,
    /// Paths git does not track and does not ignore. A new directory counts
    /// once, however many files it holds (`--untracked-files=normal`).
    pub untracked: u32,
    /// Paths with an unresolved merge conflict.
    pub conflicted: u32,
}

/// The answer the checkout panel draws: a state, or why there is none.
///
/// None of these is an error. A checkout panel is worth drawing without a
/// state, and each arm is a sentence the panel says.
///
/// - `read`: git answered.
/// - `no_git`: No `git` on `PATH`.
/// - `not_a_repo`: The path is not a git working tree, or not there at all.
/// - `failed`: git ran and did not answer: it timed out, or refused with
///   `reason`.
///
/// Issue #704.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(tag = "status", rename_all = "snake_case")]
#[ts(export_to = "entity.ts")]
pub enum TreeRead {
    Read(WorkingTreeState),
    NoGit,
    NotARepo,
    Failed { reason: String },
}

/// Read the working tree at `path` with the `git` on this process's `PATH`.
pub async fn read(path: &Path) -> TreeRead {
    let search = std::env::var_os("PATH").unwrap_or_default();
    read_with(&search, path).await
}

/// [`read`], with the `PATH` to look for `git` on passed in.
///
/// An argument so "git is not installed" is reachable from a test on a machine
/// that has git, and for no other reason.
pub async fn read_with(search: &OsStr, path: &Path) -> TreeRead {
    read_guarded(
        search,
        path,
        Path::new(APPLE_SHIM),
        command_line_tools_installed,
        TIMEOUT,
    )
    .await
}

/// Apple's `git` on a Mac without the Command Line Tools is a shim that opens
/// an install dialog instead of running (#715).
pub(super) const APPLE_SHIM: &str = "/usr/bin/git";

/// Whether the Command Line Tools are installed, asked once per process once
/// it has answered. `xcode-select -p` only prints a path and never opens the
/// dialog. Off macOS there is no shim, so the answer is yes.
///
/// The answer is kept for the process, a `no` included: tools installed while
/// knobas runs are noticed after a restart. A probe that has not answered is
/// not kept (the caller abandons it, `kill_on_drop` ends it) and is asked
/// again on the next read.
pub(super) async fn command_line_tools_installed() -> bool {
    static ANSWER: tokio::sync::OnceCell<bool> = tokio::sync::OnceCell::const_new();
    *ANSWER
        .get_or_init(|| async {
            if !cfg!(target_os = "macos") {
                return true;
            }
            let mut command = tokio::process::Command::from(knobas_db::console::command("xcode-select"));
            command
                .arg("-p")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true);
            // A dropped probe kills the child, also when its spawn task
            // finishes after the drop (the child is dropped with the result).
            match spawn_off_runtime(command).await {
                Ok(Ok(mut child)) => child.wait().await.is_ok_and(|status| status.success()),
                _ => false,
            }
        })
        .await
}

/// The decision seam: `probe` is asked only when the `git` found is `shim`,
/// within `timeout` ([`TIMEOUT`] in production), and a failing probe answers
/// [`TreeRead::NoGit`] without spawning it. `timeout` is a parameter so a test
/// on a loaded machine can give a spawned script time to run (#794).
async fn read_guarded(
    search: &OsStr,
    path: &Path,
    shim: &Path,
    probe: impl AsyncFnOnce() -> bool,
    timeout: Duration,
) -> TreeRead {
    let git = match locate(search, shim, probe, timeout).await {
        Ok(git) => git,
        Err(Unrunnable::NoGit) => return TreeRead::NoGit,
        Err(Unrunnable::ProbeFailed(reason)) => return TreeRead::Failed { reason },
    };
    if !is_dir_off_runtime(path).await {
        return TreeRead::NotARepo;
    }

    let mut command = git_at(&git, path);
    command.args([
        "status",
        "--porcelain=v2",
        "--branch",
        "--untracked-files=normal",
    ]);

    let run = async {
        match spawn_off_runtime(command).await {
            Ok(Ok(child)) => child.wait_with_output().await,
            Ok(Err(error)) => Err(error),
            Err(error) => Err(std::io::Error::other(format!(
                "its start did not finish: {error}"
            ))),
        }
    };
    let output = match tokio::time::timeout(timeout, run).await {
        Err(_) => {
            return TreeRead::Failed {
                reason: format!("git did not answer within {} s", timeout.as_secs()),
            };
        }
        Ok(Err(error)) => {
            return TreeRead::Failed {
                reason: format!("git could not be run: {error}"),
            };
        }
        Ok(Ok(output)) => output,
    };

    if output.status.success() {
        return TreeRead::Read(parse(&String::from_utf8_lossy(&output.stdout)));
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if not_a_repo(&stderr) {
        return TreeRead::NotARepo;
    }
    TreeRead::Failed {
        reason: refusal(&stderr),
    }
}

/// Why no `git` is run at a checkout.
pub(super) enum Unrunnable {
    /// Not on the search path, or Apple's shim on a Mac without the Command
    /// Line Tools.
    NoGit,
    /// The Command Line Tools probe did not answer, or the `PATH` lookup's
    /// task did not finish.
    ProbeFailed(String),
}

/// The `git` to run, or why none is.
///
/// Shared by every read of a checkout ([`read`] and
/// [`super::uncommitted::read`]), so each refuses the shim the same way.
pub(super) async fn locate(
    search: &OsStr,
    shim: &Path,
    probe: impl AsyncFnOnce() -> bool,
    timeout: Duration,
) -> Result<PathBuf, Unrunnable> {
    let Some(git) = find_off_runtime(search, "git").await? else {
        return Err(Unrunnable::NoGit);
    };
    if git == shim {
        match tokio::time::timeout(timeout, probe()).await {
            Err(_) => {
                return Err(Unrunnable::ProbeFailed(format!(
                    "xcode-select did not answer within {} s",
                    timeout.as_secs()
                )));
            }
            Ok(false) => return Err(Unrunnable::NoGit),
            Ok(true) => {}
        }
    }
    Ok(git)
}

/// The `git` on this process's `PATH` that [`read`] would run, for the one
/// other git command knobas runs, `checkout::clone` (#823).
pub(crate) async fn git_on_path() -> Result<PathBuf, TreeRead> {
    let search = std::env::var_os("PATH").unwrap_or_default();
    locate(
        &search,
        Path::new(APPLE_SHIM),
        command_line_tools_installed,
        TIMEOUT,
    )
    .await
    .map_err(|why| match why {
        Unrunnable::NoGit => TreeRead::NoGit,
        Unrunnable::ProbeFailed(reason) => TreeRead::Failed { reason },
    })
}

/// `git -C <path>` with ADR-0021's read-only terms, before the subcommand:
/// no optional locks, no fsmonitor, no terminal prompt, git's own English, an
/// inherited repository location removed, stdin closed and the process killed
/// when its future is dropped. The caller adds the subcommand.
pub(super) fn git_at(git: &Path, path: &Path) -> tokio::process::Command {
    let mut command = tokio::process::Command::from(knobas_db::console::command(git));
    command
        .arg("-C")
        .arg(path)
        .args(["--no-optional-locks", "-c", "core.fsmonitor=false"])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        // git's own words, untranslated: `not a git repository` is matched in
        // English.
        .env("LC_ALL", "C")
        // A repository is named by `-C` and nothing else: an inherited
        // `GIT_DIR` (or one of its parts) would point the read somewhere the
        // path does not.
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command
}

/// Whether git's stderr says the path is no repository: `status` says
/// "not a git repository", `diff` warns "Not a git repository" first.
pub(super) fn not_a_repo(stderr: &str) -> bool {
    stderr.to_ascii_lowercase().contains("not a git repository")
}

/// git's first non-blank line on stderr, as the reason a read failed.
pub(super) fn refusal(stderr: &str) -> String {
    stderr
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("git refused without saying why")
        .trim()
        .to_owned()
}

/// `command.spawn()` off the async runtime: the fork and exec are synchronous
/// syscalls, also inside `tokio::process::Command::{output,status}`. The
/// blocking thread still carries the runtime's context, which the child's
/// pipes and its reaping need. The outer error is a task that panicked or was
/// cancelled; the inner one is the spawn's own. A caller that is dropped while
/// the spawn runs does not cut it off: the child arrives, is dropped with the
/// result, and `kill_on_drop` ends it.
pub(super) async fn spawn_off_runtime(
    mut command: tokio::process::Command,
) -> Result<std::io::Result<tokio::process::Child>, tokio::task::JoinError> {
    tokio::task::spawn_blocking(move || command.spawn()).await
}

/// `path.is_dir()` off the async runtime (`tokio::fs` runs it under
/// `spawn_blocking`; a dropped caller does not cut the stat off). A path that
/// cannot be stat'ed is not a folder.
pub(super) async fn is_dir_off_runtime(path: &Path) -> bool {
    tokio::fs::metadata(path)
        .await
        .is_ok_and(|metadata| metadata.is_dir())
}

/// [`find_on`] off the async runtime: it stats each `PATH` entry. A task that
/// panicked or was cancelled is a failed lookup, `ProbeFailed`: [`locate`]'s
/// callers answer in [`TreeRead`] or `DiffRead`, and neither has an internal
/// error.
async fn find_off_runtime(search: &OsStr, name: &str) -> Result<Option<PathBuf>, Unrunnable> {
    let (search, name) = (search.to_owned(), name.to_owned());
    tokio::task::spawn_blocking(move || find_on(&search, &name))
        .await
        .map_err(|error| {
            Unrunnable::ProbeFailed(format!("the PATH lookup did not finish: {error}"))
        })
}

/// The first executable called `name` in a `PATH`-style list.
fn find_on(search: &OsStr, name: &str) -> Option<PathBuf> {
    let file: OsString = if cfg!(windows) {
        format!("{name}.exe").into()
    } else {
        name.into()
    };
    std::env::split_paths(search)
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(|dir| dir.join(&file))
        .find(|candidate| candidate.is_file())
}

/// `git status --porcelain=v2 --branch`'s output, as a state.
///
/// Line by line: without `-z`, git quotes a path holding a newline, so one
/// entry is one line. A header or entry this does not know is skipped, which
/// is what the format's documentation asks of a reader.
fn parse(porcelain: &str) -> WorkingTreeState {
    let mut state = WorkingTreeState::default();
    for line in porcelain.lines() {
        if let Some(header) = line.strip_prefix("# ") {
            let (key, value) = header.split_once(' ').unwrap_or((header, ""));
            match key {
                "branch.oid" if value != "(initial)" => state.head = Some(value.to_owned()),
                "branch.head" if value != "(detached)" => state.branch = Some(value.to_owned()),
                "branch.upstream" => state.upstream = Some(value.to_owned()),
                "branch.ab" => {
                    for part in value.split_whitespace() {
                        if let Some(n) = part.strip_prefix('+') {
                            state.ahead = n.parse().unwrap_or(0);
                        } else if let Some(n) = part.strip_prefix('-') {
                            state.behind = n.parse().unwrap_or(0);
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        match line.split_once(' ').map_or(line, |(kind, _)| kind) {
            "1" | "2" => state.changed += 1,
            "u" => state.conflicted += 1,
            "?" => state.untracked += 1,
            _ => {}
        }
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Long enough that a spawned script finishes on a machine at load 20; a
    /// test that expects git to run must not race the production [`TIMEOUT`].
    const PATIENT: Duration = Duration::from_secs(30);
    /// For the test that expects the cut-off.
    const IMPATIENT: Duration = Duration::from_millis(100);

    /// A header git may add later, and an ignored entry, change nothing.
    #[test]
    fn what_the_reader_does_not_know_is_skipped() {
        let state = parse("# branch.oid abc\n# branch.head main\n# stash 3\n! ignored.log\n");
        assert_eq!(
            state,
            WorkingTreeState {
                branch: Some("main".to_owned()),
                head: Some("abc".to_owned()),
                ..WorkingTreeState::default()
            }
        );
    }

    /// A `git` that leaves a marker when it is spawned, and the `PATH` that
    /// finds it. The dialog itself is unwitnessed: a Mac without the Command
    /// Line Tools is not available here (ADR-0013).
    #[cfg(unix)]
    fn marker_git() -> (tempfile::TempDir, std::ffi::OsString, PathBuf, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let bin = tempfile::tempdir().unwrap();
        let marker = bin.path().join("spawned");
        let git = bin.path().join("git");
        std::fs::write(&git, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
        std::fs::set_permissions(&git, std::fs::Permissions::from_mode(0o755)).unwrap();
        let search = bin.path().as_os_str().to_owned();
        (bin, search, git, marker)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_shim_without_command_line_tools_is_not_spawned() {
        let (_bin, search, shim, marker) = marker_git();
        let work = tempfile::tempdir().unwrap();
        let read = read_guarded(&search, work.path(), &shim, async || false, PATIENT).await;
        assert_eq!(read, TreeRead::NoGit);
        assert!(!marker.exists(), "the shim was spawned");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_shim_with_command_line_tools_runs() {
        let (_bin, search, shim, marker) = marker_git();
        let work = tempfile::tempdir().unwrap();
        read_guarded(&search, work.path(), &shim, async || true, PATIENT).await;
        assert!(marker.exists(), "git was not run");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_probe_that_hangs_does_not_hang_the_read() {
        let (_bin, search, shim, marker) = marker_git();
        let work = tempfile::tempdir().unwrap();
        let read = read_guarded(
            &search,
            work.path(),
            &shim,
            async || std::future::pending::<bool>().await,
            IMPATIENT,
        )
        .await;
        assert!(matches!(read, TreeRead::Failed { .. }), "{read:?}");
        assert!(!marker.exists(), "the shim was spawned");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn another_git_is_never_probed() {
        let (_bin, search, _git, marker) = marker_git();
        let work = tempfile::tempdir().unwrap();
        let other = Path::new("/usr/bin/git");
        read_guarded(
            &search,
            work.path(),
            other,
            async || panic!("probed"),
            PATIENT,
        )
        .await;
        assert!(marker.exists(), "git was not run");
    }

    #[test]
    fn an_empty_path_entry_never_finds_a_program() {
        // `PATH=":"` names the working directory twice in a shell; here it
        // names nothing, so a `git` in the process's cwd is never run.
        assert_eq!(find_on(OsStr::new(":"), "git"), None);
    }
}
