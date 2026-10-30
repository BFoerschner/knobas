//! A checkout's uncommitted changes, file by file (#824, ADR-0026).
//!
//! `CONTEXT.md`, **Diff**, read at a [checkout](super) rather than from a git
//! server: what is changed in the working tree and not yet staged, what is
//! staged and not yet committed, and the paths git does not track. The
//! *Local changes* tab (requirement R-8.9) draws it.
//!
//! # What runs (ADR-0026, on ADR-0021's terms)
//!
//! Three read-only git commands at the checkout path, under
//! [`working_tree`](super::working_tree)'s environment and guards:
//!
//! - `git diff --no-ext-diff --no-color --no-textconv` for the unstaged half,
//! - the same with `--cached` for the staged half,
//! - `git status --porcelain=v2 -z --untracked-files=normal` for the untracked
//!   paths, which `diff` does not show.
//!
//! `-c diff.autoRefreshIndex=false` keeps `git diff` from rewriting the
//! index's stat data, a write `--no-optional-locks` does not stop.
//! `--src-prefix=a/ --dst-prefix=b/` pin the prefixes the shared parser strips,
//! so a checkout whose configuration sets `diff.noprefix` or
//! `diff.mnemonicPrefix` still reads as the same paths. The checkout path is
//! the only value from outside; no branch, commit or file name reaches git.
//!
//! # Capped, and git is stopped past the cap
//!
//! Each half is parsed by [`knobas_source::diff::parse_view`], so it carries
//! the parser's line ceiling of ADR-0020 rule 6 (#2269) like a pull
//! request's diff. Past [`READ_LINES`] lines or [`READ_BYTES`] bytes of one
//! half's output git is killed and the rest is never read: that half is then
//! `truncated`, and so is the file the cut fell in, whose `additions` and
//! `deletions` count only what was read.

use std::ffi::OsStr;
use std::path::Path;
use std::time::Duration;

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

use knobas_source::diff::{DiffFile, MAX_LINES_PER_VIEW, parse_view};

use super::working_tree::{
    APPLE_SHIM, Unrunnable, command_line_tools_installed, git_at, is_dir_off_runtime, locate,
    not_a_repo, refusal, spawn_off_runtime,
};

/// How long the whole read (all three commands) may take.
///
/// Longer than `status`'s 2 seconds because a diff reads file contents
/// (ADR-0026). Measured 2026-10-15 on an Apple-silicon Mac, a 16,970-file
/// checkout (514 MB) with 400 files changed and 200 staged: the three
/// commands take 0.25 s with the index's stat data fresh, and 3.7 s with every
/// file's stat stale, the worst case, since the read never refreshes the index
/// (`diff.autoRefreshIndex` off) and so compares every such file's content.
/// Ten seconds is that worst case with room for a slower disk.
pub const TIMEOUT: Duration = Duration::from_secs(10);

/// The most lines of one half's `git diff` output read before git is
/// stopped: four times the parser's ceiling, so the ceiling, not this read,
/// is what cuts a long diff and says so.
pub const READ_LINES: usize = 4 * MAX_LINES_PER_VIEW;

/// The most bytes of one half's output read before git is stopped, for the
/// file of a few very long lines that [`READ_LINES`] would not catch.
pub const READ_BYTES: usize = 16 * 1024 * 1024;

/// The most untracked paths listed. A new directory is one path
/// (`--untracked-files=normal`), as in the working tree state's count.
pub const MAX_UNTRACKED: usize = 1_000;

/// One half of the uncommitted diff, as a diff block carries it.
///
/// Issue #824.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct CappedDiff {
    pub files: Vec<DiffFile>,
    /// A cap cut this half: the per-view cap of the parser, or the read cap
    /// that stopped git.
    pub truncated: bool,
}

/// What is uncommitted at a checkout.
///
/// Issue #824, ADR-0026.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct UncommittedDiff {
    /// Changed in the working tree and not staged (`git diff`).
    pub unstaged: CappedDiff,
    /// Staged and not committed (`git diff --cached`).
    pub staged: CappedDiff,
    /// Paths git does not track and does not ignore, in git's order; a new
    /// directory once, with its trailing `/`.
    pub untracked: Vec<String>,
    /// More than [`MAX_UNTRACKED`] paths were untracked; the rest are not
    /// listed.
    pub untracked_truncated: bool,
}

/// The answer the *Local changes* tab draws: the diff, or why there is none.
/// The same four arms as the working tree's
/// [`TreeRead`](super::working_tree::TreeRead).
///
/// - `read`: git answered.
/// - `no_git`: No `git` on `PATH`.
/// - `not_a_repo`: The path is not a git working tree, or not there at all.
/// - `failed`: git ran and did not answer: it timed out, or refused with
///   `reason`.
///
/// Issue #824. None of the arms is an error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(tag = "status", rename_all = "snake_case")]
#[ts(export_to = "entity.ts")]
pub enum DiffRead {
    Read(UncommittedDiff),
    NoGit,
    NotARepo,
    Failed { reason: String },
}

/// The read caps, a parameter so a test can reach the cut with a small file.
#[derive(Debug, Clone, Copy)]
struct ReadCaps {
    lines: usize,
    bytes: usize,
    untracked: usize,
}

const CAPS: ReadCaps = ReadCaps {
    lines: READ_LINES,
    bytes: READ_BYTES,
    untracked: MAX_UNTRACKED,
};

/// Read the uncommitted diff at `path` with the `git` on this process's
/// `PATH`.
pub async fn read(path: &Path) -> DiffRead {
    let search = std::env::var_os("PATH").unwrap_or_default();
    read_with(&search, path).await
}

/// [`read`], with the `PATH` to look for `git` on passed in, so "git is not
/// installed" is reachable from a test on a machine that has git.
pub async fn read_with(search: &OsStr, path: &Path) -> DiffRead {
    read_guarded(search, path, TIMEOUT, CAPS).await
}

async fn read_guarded(search: &OsStr, path: &Path, timeout: Duration, caps: ReadCaps) -> DiffRead {
    let git = match locate(
        search,
        Path::new(APPLE_SHIM),
        command_line_tools_installed,
        timeout,
    )
    .await
    {
        Ok(git) => git,
        Err(Unrunnable::NoGit) => return DiffRead::NoGit,
        Err(Unrunnable::ProbeFailed(reason)) => return DiffRead::Failed { reason },
    };
    if !is_dir_off_runtime(path).await {
        return DiffRead::NotARepo;
    }
    let all = async {
        let unstaged = half(&git, path, false, caps).await?;
        let staged = half(&git, path, true, caps).await?;
        let (untracked, untracked_truncated) = untracked(&git, path, caps.untracked).await?;
        Ok(UncommittedDiff {
            unstaged,
            staged,
            untracked,
            untracked_truncated,
        })
    };
    match tokio::time::timeout(timeout, all).await {
        Err(_) => DiffRead::Failed {
            reason: format!("git did not answer within {} s", timeout.as_secs()),
        },
        Ok(Err(answer)) => answer,
        Ok(Ok(diff)) => DiffRead::Read(diff),
    }
}

/// One half: `git diff`, or `git diff --cached`, read up to the caps.
async fn half(
    git: &Path,
    path: &Path,
    cached: bool,
    caps: ReadCaps,
) -> Result<CappedDiff, DiffRead> {
    let mut command = git_at(git, path);
    // `diff.autoRefreshIndex` off: on a file whose stat no longer matches the
    // index, `git diff` otherwise rewrites the index's stat data after the
    // diff, and `--no-optional-locks` does not stop it (git 2.50, witnessed by
    // `the_read_leaves_the_index_and_the_refs_as_they_were`).
    command.args([
        "-c",
        "diff.autoRefreshIndex=false",
        "diff",
        "--no-ext-diff",
        "--no-color",
        "--no-textconv",
        "--src-prefix=a/",
        "--dst-prefix=b/",
    ]);
    if cached {
        command.arg("--cached");
    }
    let (text, cut) = run_capped(command, caps.lines, caps.bytes).await?;
    let (mut files, mut truncated) = parse_view(&String::from_utf8_lossy(&text));
    if cut {
        truncated = true;
        if let Some(last) = files.last_mut() {
            last.truncated = true;
        }
    }
    Ok(CappedDiff { files, truncated })
}

/// The untracked paths, from `status`'s `?` entries.
async fn untracked(git: &Path, path: &Path, cap: usize) -> Result<(Vec<String>, bool), DiffRead> {
    let mut command = git_at(git, path);
    command.args(["status", "--porcelain=v2", "-z", "--untracked-files=normal"]);
    let (out, cut) = run_capped(command, usize::MAX, READ_BYTES).await?;
    let mut paths = Vec::new();
    let mut truncated = cut;
    let mut entries: Vec<&[u8]> = out.split(|byte| *byte == 0).collect();
    if cut {
        // The entry the cut fell in is partial.
        entries.pop();
    }
    let mut entries = entries.into_iter();
    while let Some(entry) = entries.next() {
        if entry.starts_with(b"2 ") {
            // A rename or copy: its original path is the next field.
            entries.next();
        } else if let Some(name) = entry.strip_prefix(b"? ") {
            if paths.len() == cap {
                truncated = true;
                break;
            }
            paths.push(String::from_utf8_lossy(name).into_owned());
        }
    }
    Ok((paths, truncated))
}

/// Run `command` and read its stdout up to `max_lines` lines or `max_bytes`
/// bytes; past either git is killed. Answers what was read and whether the
/// cut came, or the refusal git gave.
async fn run_capped(
    command: tokio::process::Command,
    max_lines: usize,
    max_bytes: usize,
) -> Result<(Vec<u8>, bool), DiffRead> {
    let mut child = match spawn_off_runtime(command).await {
        Ok(Ok(child)) => child,
        Ok(Err(error)) => {
            return Err(DiffRead::Failed {
                reason: format!("git could not be run: {error}"),
            });
        }
        Err(error) => {
            return Err(DiffRead::Failed {
                reason: format!("git could not be run: its start did not finish: {error}"),
            });
        }
    };
    let stdout = child.stdout.take().expect("stdout is piped");
    let mut stderr = child.stderr.take().expect("stderr is piped");

    let read_out = async {
        let mut reader = BufReader::new(stdout);
        let mut out = Vec::new();
        let mut lines = 0usize;
        loop {
            if lines >= max_lines || out.len() >= max_bytes {
                // The reader is dropped here, closing git's stdout: git dies
                // of SIGPIPE on its next write, so its stderr ends and the
                // read below returns before the kill.
                drop(reader);
                return Ok::<_, std::io::Error>((out, true));
            }
            // At most the bytes left under the cap, so one line longer than
            // the cap is cut inside it rather than read whole.
            let left = (max_bytes - out.len()) as u64;
            if (&mut reader).take(left).read_until(b'\n', &mut out).await? == 0 {
                return Ok((out, false));
            }
            lines += 1;
        }
    };
    let read_err = async {
        let mut err = Vec::new();
        // Up to 64 KiB of git's complaint kept (the first line is what is
        // used), and the rest drained, so a git that writes more cannot block
        // on a full stderr pipe.
        let _ = (&mut stderr).take(64 * 1024).read_to_end(&mut err).await;
        let _ = tokio::io::copy(&mut stderr, &mut tokio::io::sink()).await;
        err
    };
    let (out, err) = tokio::join!(read_out, read_err);
    let (out, cut) = out.map_err(|error| DiffRead::Failed {
        reason: format!("git's answer could not be read: {error}"),
    })?;
    if cut {
        // Stopped past the cap: the rest is never read (ADR-0026).
        let _ = child.start_kill();
        let _ = child.wait().await;
        return Ok((out, true));
    }
    let status = child.wait().await.map_err(|error| DiffRead::Failed {
        reason: format!("git could not be run: {error}"),
    })?;
    if status.success() {
        return Ok((out, false));
    }
    let err = String::from_utf8_lossy(&err);
    if not_a_repo(&err) {
        return Err(DiffRead::NotARepo);
    }
    Err(DiffRead::Failed {
        reason: refusal(&err),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Long enough that git finishes on a loaded machine.
    const PATIENT: Duration = Duration::from_secs(30);

    fn git(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.name=knobas test",
                "-c",
                "user.email=test@knobas.invalid",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// A repository with `a.txt` and `z.txt` committed, both then rewritten
    /// with 100 new lines each.
    fn two_long_changes() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q"]);
        std::fs::write(dir.path().join("a.txt"), "a\n").unwrap();
        std::fs::write(dir.path().join("z.txt"), "z\n").unwrap();
        git(dir.path(), &["add", "."]);
        git(dir.path(), &["commit", "-q", "-m", "first"]);
        let long: String = (0..100).map(|n| format!("new {n}\n")).collect();
        std::fs::write(dir.path().join("a.txt"), &long).unwrap();
        std::fs::write(dir.path().join("z.txt"), &long).unwrap();
        dir
    }

    async fn read_under(dir: &Path, caps: ReadCaps) -> UncommittedDiff {
        let search = std::env::var_os("PATH").unwrap_or_default();
        match read_guarded(&search, dir, PATIENT, caps).await {
            DiffRead::Read(diff) => diff,
            other => panic!("expected a read, got {other:?}"),
        }
    }

    /// Past the read cap git's output is not read further: the half is
    /// truncated, the file the cut fell in says so, and the file after it is
    /// never seen.
    #[tokio::test]
    async fn past_the_read_cap_the_rest_is_never_read() {
        let dir = two_long_changes();
        let caps = ReadCaps { lines: 40, ..CAPS };

        let diff = read_under(dir.path(), caps).await;

        let paths: Vec<&str> = diff
            .unstaged
            .files
            .iter()
            .map(|f| f.path.as_str())
            .collect();
        assert_eq!(paths, ["a.txt"]);
        assert!(diff.unstaged.truncated);
        let a = &diff.unstaged.files[0];
        assert!(a.truncated);
        let kept: usize = a.hunks.iter().map(|h| h.lines.len()).sum();
        assert!(kept < 40, "{kept} lines kept from a 40-line read");
    }

    /// The same diff under the default caps reads both files whole: the cut
    /// above is the read cap's and not the fixture's.
    #[tokio::test]
    async fn under_the_read_cap_every_file_is_read() {
        let dir = two_long_changes();

        let diff = read_under(dir.path(), CAPS).await;

        let paths: Vec<&str> = diff
            .unstaged
            .files
            .iter()
            .map(|f| f.path.as_str())
            .collect();
        assert_eq!(paths, ["a.txt", "z.txt"]);
        assert!(!diff.unstaged.truncated);
        assert!(diff.unstaged.files.iter().all(|f| !f.truncated));
    }

    /// The byte cap cuts the same way, for output of few long lines.
    #[tokio::test]
    async fn past_the_byte_cap_the_rest_is_never_read() {
        let dir = two_long_changes();
        let caps = ReadCaps { bytes: 300, ..CAPS };

        let diff = read_under(dir.path(), caps).await;

        let paths: Vec<&str> = diff
            .unstaged
            .files
            .iter()
            .map(|f| f.path.as_str())
            .collect();
        assert_eq!(paths, ["a.txt"]);
        assert!(diff.unstaged.truncated);
        assert!(diff.unstaged.files[0].truncated);
    }

    /// One line longer than the byte cap is not read whole: the cap holds
    /// inside a line, not only between lines.
    #[tokio::test]
    async fn the_byte_cap_holds_inside_one_long_line() {
        let dir = two_long_changes();
        let line = "x".repeat(200_000);
        std::fs::write(dir.path().join("a.txt"), format!("{line}\n")).unwrap();
        let caps = ReadCaps { bytes: 300, ..CAPS };

        let diff = read_under(dir.path(), caps).await;

        assert!(diff.unstaged.truncated);
        let kept: usize = diff
            .unstaged
            .files
            .iter()
            .flat_map(|f| &f.hunks)
            .flat_map(|h| &h.lines)
            .map(|l| l.text.len())
            .sum();
        assert!(
            kept <= 300,
            "{kept} bytes of line text kept under a 300-byte cap"
        );
    }

    #[tokio::test]
    async fn past_the_untracked_cap_the_list_is_cut_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q"]);
        for n in 0..5 {
            std::fs::write(dir.path().join(format!("new-{n}.txt")), "x").unwrap();
        }
        let caps = ReadCaps {
            untracked: 3,
            ..CAPS
        };

        let diff = read_under(dir.path(), caps).await;

        assert_eq!(diff.untracked.len(), 3);
        assert!(diff.untracked_truncated);
        let all = read_under(dir.path(), CAPS).await;
        assert_eq!(all.untracked.len(), 5);
        assert!(!all.untracked_truncated);
    }

    /// A git that does not answer is abandoned at the timeout. The real git,
    /// slowed by a clean filter in the repository's own configuration, which
    /// `diff` runs to compare a file whose stat no longer matches the index.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_git_that_does_not_answer_is_abandoned_at_the_timeout() {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q"]);
        std::fs::write(dir.path().join("a.txt"), "one\n").unwrap();
        git(dir.path(), &["add", "."]);
        git(dir.path(), &["commit", "-q", "-m", "first"]);
        git(
            dir.path(),
            &["config", "filter.slow.clean", "sleep 10; cat"],
        );
        let info = dir.path().join(".git").join("info");
        std::fs::create_dir_all(&info).unwrap();
        std::fs::write(info.join("attributes"), "a.txt filter=slow\n").unwrap();
        std::thread::sleep(Duration::from_millis(1100));
        std::fs::write(dir.path().join("a.txt"), "one\n").unwrap();

        let search = std::env::var_os("PATH").unwrap_or_default();
        let started = std::time::Instant::now();
        let answer = read_guarded(&search, dir.path(), Duration::from_secs(1), CAPS).await;

        assert_eq!(
            answer,
            DiffRead::Failed {
                reason: "git did not answer within 1 s".to_owned()
            }
        );
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
