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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TreeRead {
    /// git answered.
    Read(WorkingTreeState),
    /// No `git` on `PATH`.
    NoGit,
    /// The path is not a git working tree, or not there at all.
    NotARepo,
    /// git ran and did not answer: it timed out, or refused with `reason`.
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
        &command_line_tools_installed,
    )
    .await
}

/// Apple's `git` on a Mac without the Command Line Tools is a shim that opens
/// an install dialog instead of running (#715).
const APPLE_SHIM: &str = "/usr/bin/git";

/// Whether the Command Line Tools are installed, asked once per process.
/// `xcode-select -p` only prints a path and never opens the dialog. Off macOS
/// there is no shim, so the answer is yes.
fn command_line_tools_installed() -> bool {
    static ANSWER: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ANSWER.get_or_init(|| {
        !cfg!(target_os = "macos")
            || std::process::Command::new("xcode-select")
                .arg("-p")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
    })
}

/// The decision seam: `probe` is asked only when the `git` found is `shim`,
/// and a failing probe answers [`TreeRead::NoGit`] without spawning it.
async fn read_guarded(
    search: &OsStr,
    path: &Path,
    shim: &Path,
    probe: &(dyn Fn() -> bool + Sync),
) -> TreeRead {
    let Some(git) = find_on(search, "git") else {
        return TreeRead::NoGit;
    };
    if git == shim && !probe() {
        return TreeRead::NoGit;
    }
    if !path.is_dir() {
        return TreeRead::NotARepo;
    }

    let mut command = tokio::process::Command::new(git);
    command
        .arg("-C")
        .arg(path)
        .args([
            "--no-optional-locks",
            "-c",
            "core.fsmonitor=false",
            "status",
            "--porcelain=v2",
            "--branch",
            "--untracked-files=normal",
        ])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        // git's own words, untranslated: `not a git repository` below is
        // matched in English.
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

    let output = match tokio::time::timeout(TIMEOUT, command.output()).await {
        Err(_) => {
            return TreeRead::Failed {
                reason: format!("git did not answer within {} s", TIMEOUT.as_secs()),
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
    if stderr.contains("not a git repository") {
        return TreeRead::NotARepo;
    }
    TreeRead::Failed {
        reason: stderr
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("git refused without saying why")
            .trim()
            .to_owned(),
    }
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

    /// The four arms serialise as the words `app/src/lib/ipc/entity.ts`
    /// narrows on, with the state's fields beside the tag.
    #[test]
    fn the_read_serialises_as_the_mirror_declares() {
        let mirror = include_str!("../../../../app/src/lib/ipc/entity.ts");
        let read = serde_json::to_value(TreeRead::Read(WorkingTreeState::default())).unwrap();
        let mut keys: Vec<String> = read.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(
            keys,
            [
                "ahead",
                "behind",
                "branch",
                "changed",
                "conflicted",
                "head",
                "status",
                "untracked",
                "upstream"
            ]
        );
        for key in &keys {
            assert!(
                mirror.contains(&format!("{key}:")),
                "WorkingTreeState.{key} is missing from app/src/lib/ipc/entity.ts"
            );
        }
        for (arm, word) in [
            (TreeRead::Read(WorkingTreeState::default()), "read"),
            (TreeRead::NoGit, "no_git"),
            (TreeRead::NotARepo, "not_a_repo"),
            (
                TreeRead::Failed {
                    reason: String::new(),
                },
                "failed",
            ),
        ] {
            assert_eq!(serde_json::to_value(arm).unwrap()["status"], word);
            assert!(
                mirror.contains(&format!("status: \"{word}\"")),
                "{word:?} is missing from app/src/lib/ipc/entity.ts"
            );
        }
    }

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
        let read = read_guarded(&search, work.path(), &shim, &|| false).await;
        assert_eq!(read, TreeRead::NoGit);
        assert!(!marker.exists(), "the shim was spawned");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_shim_with_command_line_tools_runs() {
        let (_bin, search, shim, marker) = marker_git();
        let work = tempfile::tempdir().unwrap();
        read_guarded(&search, work.path(), &shim, &|| true).await;
        assert!(marker.exists(), "git was not run");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn another_git_is_never_probed() {
        let (_bin, search, _git, marker) = marker_git();
        let work = tempfile::tempdir().unwrap();
        let other = Path::new("/usr/bin/git");
        read_guarded(&search, work.path(), other, &|| panic!("probed")).await;
        assert!(marker.exists(), "git was not run");
    }

    #[test]
    fn an_empty_path_entry_never_finds_a_program() {
        // `PATH=":"` names the working directory twice in a shell; here it
        // names nothing, so a `git` in the process's cwd is never run.
        assert_eq!(find_on(OsStr::new(":"), "git"), None);
    }
}
