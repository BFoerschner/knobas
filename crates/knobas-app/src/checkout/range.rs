//! The diff between two commits, read at a checkout that holds both (#2272,
//! ADR-0036, widening ADR-0026 from the uncommitted diff to any diff).
//!
//! `CONTEXT.md`, **Diff**: a range's diff comes from local read-only git when
//! the repository's checkout holds both of its commits, instantly and
//! offline, and *Ignore whitespace* is possible only here (the source's API
//! cannot return a diff with whitespace ignored, `docs/decisions/2026-10-30-
//! gitea-thread-resolve-and-diff-whitespace.md`). Anything else is
//! [`None`], and the caller asks the source.
//!
//! # What runs (ADR-0036, on ADR-0021's terms)
//!
//! Two read-only subcommands, under [`working_tree`](super::working_tree)'s
//! environment and guards, and nothing else (the module's tests record every
//! command line):
//!
//! - `git rev-parse --verify --quiet <sha>^{commit}` per end, which is the
//!   [`holds_commits`] check and resolves an end to its whole sha; for a root
//!   commit's `<sha>^`, `rev-parse --is-shallow-repository --show-object-format`
//!   tells a root from a shallow clone's cut and names the empty tree;
//! - `git diff --no-ext-diff --no-color --no-textconv -M <base> <head> --`,
//!   with `-w` for *Ignore whitespace*, read up to ADR-0020 rule 6's ceiling
//!   as the uncommitted diff is.
//!
//! **Never fetched.** Every command runs with `GIT_NO_LAZY_FETCH=1` and
//! `protocol.allow=never`, so a partial clone missing an object answers
//! "missing" and the source is asked, instead of git reaching the network.
//!
//! **Checked names only** (ADR-0034): an end reaches git only once it is
//! 7 to 64 hex digits (`is_sha`), the base with at most one trailing `^`. A
//! branch name is the source's: the checkout's branch of that name may stand
//! anywhere, so it is never read here.

use std::ffi::OsStr;
use std::path::Path;
use std::time::Duration;

use knobas_core::checkout::open::is_sha;
use knobas_source::diff::{DiffFile, parse_view};

use super::uncommitted::{READ_BYTES, READ_LINES, run_capped};
use super::working_tree::{
    APPLE_SHIM, command_line_tools_installed, git_at, is_dir_off_runtime, locate,
};

/// How long the whole local read may take, as for the uncommitted diff.
pub const TIMEOUT: Duration = super::uncommitted::TIMEOUT;

/// The empty tree of a SHA-1 repository: what a root commit's diff is from.
const EMPTY_TREE_SHA1: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
/// The empty tree of a SHA-256 repository.
const EMPTY_TREE_SHA256: &str = "6ef19b41225c5369f1c104d45d8d85efa9b057b53b14b4b9b939dd74decc5321";

/// A range's diff as local git answered it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalDiff {
    pub files: Vec<DiffFile>,
    /// The read or the parser's ceiling cut it.
    pub truncated: bool,
    /// The old side's whole sha; `None` when the range starts before the
    /// repository's first commit, so the old side is the empty tree.
    pub base: Option<String>,
    /// The new side's whole sha.
    pub head: String,
}

/// The diff from `base` to `head` at the checkout `path`, `-w` with
/// `ignore_whitespace`; `None` when it cannot be read here (no git, no
/// repository, a name that is not a sha, a commit the checkout does not
/// hold, git refusing or slow), so the source is asked.
pub async fn read(
    path: &Path,
    base: &str,
    head: &str,
    ignore_whitespace: bool,
) -> Option<LocalDiff> {
    let search = std::env::var_os("PATH").unwrap_or_default();
    read_with(&search, path, base, head, ignore_whitespace).await
}

/// [`read`] with the `PATH` to find `git` on, so a test can put a recording
/// `git` first.
pub async fn read_with(
    search: &OsStr,
    path: &Path,
    base: &str,
    head: &str,
    ignore_whitespace: bool,
) -> Option<LocalDiff> {
    let (stem, caret) = match base.strip_suffix('^') {
        Some(stem) => (stem, true),
        None => (base, false),
    };
    if !is_sha(stem) || !is_sha(head) {
        return None;
    }
    let git = locate(
        search,
        Path::new(APPLE_SHIM),
        command_line_tools_installed,
        TIMEOUT,
    )
    .await
    .ok()?;
    if !is_dir_off_runtime(path).await {
        return None;
    }
    let all = async {
        let head = commit(&git, path, head).await?;
        let stem = commit(&git, path, stem).await?;
        let base = if caret {
            match commit(&git, path, &format!("{stem}^")).await {
                Some(parent) => Some(parent),
                None => {
                    // No parent: a root commit, or a shallow clone's cut,
                    // whose parent exists and is not here.
                    let empty = empty_tree_if_whole(&git, path).await?;
                    return diff(&git, path, &empty, &head, ignore_whitespace)
                        .await
                        .map(|(files, truncated)| LocalDiff {
                            files,
                            truncated,
                            base: None,
                            head,
                        });
                }
            }
        } else {
            Some(stem)
        }?;
        let (files, truncated) = diff(&git, path, &base, &head, ignore_whitespace).await?;
        Some(LocalDiff {
            files,
            truncated,
            base: Some(base),
            head,
        })
    };
    tokio::time::timeout(TIMEOUT, all).await.ok().flatten()
}

/// Whether the checkout at `path` holds every one of `shas` as a commit:
/// each a checked sha (ADR-0034) that `rev-parse` resolves locally, never
/// fetched. What *Diff tool* (#2275) asks before it is offered.
pub async fn holds_commits(path: &Path, shas: &[&str]) -> bool {
    let search = std::env::var_os("PATH").unwrap_or_default();
    holds_commits_with(&search, path, shas).await
}

/// [`holds_commits`] with the `PATH` to find `git` on.
pub async fn holds_commits_with(search: &OsStr, path: &Path, shas: &[&str]) -> bool {
    if shas.iter().any(|sha| !is_sha(sha)) {
        return false;
    }
    let Ok(git) = locate(
        search,
        Path::new(APPLE_SHIM),
        command_line_tools_installed,
        TIMEOUT,
    )
    .await
    else {
        return false;
    };
    if !is_dir_off_runtime(path).await {
        return false;
    }
    let all = async {
        for sha in shas {
            if commit(&git, path, sha).await.is_none() {
                return false;
            }
        }
        true
    };
    tokio::time::timeout(TIMEOUT, all).await.unwrap_or(false)
}

/// `git_at` with the network shut: no lazy fetch of a partial clone's
/// missing object, and no transport at all.
fn git_read(git: &Path, path: &Path) -> tokio::process::Command {
    let mut command = git_at(git, path);
    command
        .args(["-c", "protocol.allow=never"])
        .env("GIT_NO_LAZY_FETCH", "1");
    command
}

/// The whole sha of the commit `name` names at the checkout, if it holds it.
/// `name` is a checked sha, or one with a `^` this module added.
async fn commit(git: &Path, path: &Path, name: &str) -> Option<String> {
    let mut command = git_read(git, path);
    command.args(["rev-parse", "--verify", "--quiet"]);
    command.arg(format!("{name}^{{commit}}"));
    let (out, cut) = run_capped(command, 2, 1024).await.ok()?;
    let sha = String::from_utf8_lossy(&out).trim().to_owned();
    (!cut && is_sha(&sha)).then_some(sha)
}

/// The empty tree's name in this repository's object format, unless the
/// repository is shallow: there a commit with no parent here may well have
/// one, and the source is asked instead.
async fn empty_tree_if_whole(git: &Path, path: &Path) -> Option<String> {
    let mut command = git_read(git, path);
    command.args([
        "rev-parse",
        "--is-shallow-repository",
        "--show-object-format",
    ]);
    let (out, _) = run_capped(command, 4, 1024).await.ok()?;
    let out = String::from_utf8_lossy(&out);
    let mut lines = out.lines().map(str::trim);
    if lines.next()? != "false" {
        return None;
    }
    match lines.next()? {
        "sha1" => Some(EMPTY_TREE_SHA1.to_owned()),
        "sha256" => Some(EMPTY_TREE_SHA256.to_owned()),
        _ => None,
    }
}

/// `git diff <base> <head>`, parsed, cut as the uncommitted diff is cut.
async fn diff(
    git: &Path,
    path: &Path,
    base: &str,
    head: &str,
    ignore_whitespace: bool,
) -> Option<(Vec<DiffFile>, bool)> {
    let mut command = git_read(git, path);
    command.args([
        "-c",
        "diff.autoRefreshIndex=false",
        "diff",
        "--no-ext-diff",
        "--no-color",
        "--no-textconv",
        "-M",
        "--src-prefix=a/",
        "--dst-prefix=b/",
    ]);
    if ignore_whitespace {
        command.arg("-w");
    }
    command.args([base, head, "--"]);
    let (text, cut) = run_capped(command, READ_LINES, READ_BYTES).await.ok()?;
    let (mut files, mut truncated) = parse_view(&String::from_utf8_lossy(&text));
    if cut {
        truncated = true;
        if let Some(last) = files.last_mut() {
            last.truncated = true;
        }
    }
    Some((files, truncated))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::path::PathBuf;

    fn git(dir: &Path, args: &[&str]) -> String {
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
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    /// Three commits: `root` adds `pay.py`, `reformat` reindents it and
    /// nothing else, `edit` changes one line of it.
    struct Repo {
        dir: tempfile::TempDir,
        root: String,
        reformat: String,
        edit: String,
    }

    fn repo() -> Repo {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        git(p, &["init", "-q"]);
        std::fs::write(
            p.join("pay.py"),
            "def pay():\n    total = 1\n    return total\n",
        )
        .unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-q", "-m", "root"]);
        let root = git(p, &["rev-parse", "HEAD"]);
        std::fs::write(
            p.join("pay.py"),
            "def pay():\n  total  =  1   \n  return total\n",
        )
        .unwrap();
        git(p, &["commit", "-q", "-am", "reformat"]);
        let reformat = git(p, &["rev-parse", "HEAD"]);
        std::fs::write(
            p.join("pay.py"),
            "def pay():\n  total  =  2   \n  return total\n",
        )
        .unwrap();
        git(p, &["commit", "-q", "-am", "edit"]);
        let edit = git(p, &["rev-parse", "HEAD"]);
        Repo {
            dir,
            root,
            reformat,
            edit,
        }
    }

    /// A folder whose `git` writes its arguments, one call per line, to
    /// `calls` and then runs the real git.
    fn recording_git() -> (tempfile::TempDir, PathBuf, OsString) {
        let real = String::from_utf8(
            std::process::Command::new("sh")
                .args(["-c", "command -v git"])
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .to_owned();
        let bin = tempfile::tempdir().unwrap();
        let calls = bin.path().join("calls");
        let stub = bin.path().join("git");
        std::fs::write(
            &stub,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nexec '{real}' \"$@\"\n",
                calls.display()
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
        let search = std::env::join_paths(std::iter::once(bin.path().to_path_buf()).chain(
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
        ))
        .unwrap();
        (bin, calls, search)
    }

    fn paths(diff: &LocalDiff) -> Vec<&str> {
        diff.files.iter().map(|f| f.path.as_str()).collect()
    }

    #[tokio::test]
    async fn a_range_whose_commits_are_here_is_read_here() {
        let r = repo();
        let diff = read(r.dir.path(), &r.reformat, &r.edit, false)
            .await
            .expect("read here");
        assert_eq!(paths(&diff), ["pay.py"]);
        assert_eq!(diff.base.as_deref(), Some(r.reformat.as_str()));
        assert_eq!(diff.head, r.edit);
        assert_eq!((diff.files[0].additions, diff.files[0].deletions), (1, 1));
    }

    #[tokio::test]
    async fn a_short_sha_and_a_parent_resolve_to_whole_shas() {
        let r = repo();
        let base = format!("{}^", &r.edit[..12]);
        let diff = read(r.dir.path(), &base, &r.edit[..12], false)
            .await
            .expect("read here");
        assert_eq!(diff.base.as_deref(), Some(r.reformat.as_str()));
        assert_eq!(diff.head, r.edit);
    }

    #[tokio::test]
    async fn a_commit_the_checkout_lacks_is_not_read_here() {
        let r = repo();
        let missing = "0123456789abcdef0123456789abcdef01234567";
        assert_eq!(read(r.dir.path(), missing, &r.edit, false).await, None);
        assert_eq!(read(r.dir.path(), &r.root, missing, false).await, None);
        assert!(!holds_commits(r.dir.path(), &[&r.root, missing]).await);
        assert!(holds_commits(r.dir.path(), &[&r.root, &r.edit]).await);
    }

    #[tokio::test]
    async fn a_branch_name_is_never_read_here() {
        let r = repo();
        git(r.dir.path(), &["branch", "feature"]);
        assert_eq!(read(r.dir.path(), &r.root, "feature", false).await, None);
        assert_eq!(read(r.dir.path(), "-p", &r.edit, false).await, None);
        assert!(!holds_commits(r.dir.path(), &["feature"]).await);
    }

    #[tokio::test]
    async fn ignoring_whitespace_leaves_a_reformat_with_no_changes() {
        let r = repo();
        let shown = read(r.dir.path(), &r.root, &r.reformat, false)
            .await
            .unwrap();
        assert_eq!(paths(&shown), ["pay.py"]);
        let ignored = read(r.dir.path(), &r.root, &r.reformat, true)
            .await
            .unwrap();
        assert!(ignored.files.is_empty(), "{:?}", ignored.files);
        let real = read(r.dir.path(), &r.root, &r.edit, true).await.unwrap();
        assert_eq!((real.files[0].additions, real.files[0].deletions), (1, 1));
    }

    #[tokio::test]
    async fn a_root_commits_parent_is_the_empty_tree() {
        let r = repo();
        let diff = read(r.dir.path(), &format!("{}^", r.root), &r.root, false)
            .await
            .unwrap();
        assert_eq!(paths(&diff), ["pay.py"]);
        assert_eq!(diff.base, None);
        assert_eq!(diff.files[0].additions, 3);
    }

    #[tokio::test]
    async fn a_shallow_clones_cut_is_not_taken_for_a_root() {
        let r = repo();
        let shallow = tempfile::tempdir().unwrap();
        let url = format!("file://{}", r.dir.path().display());
        git(shallow.path(), &["clone", "-q", "--depth", "1", &url, "."]);
        assert_eq!(
            read(shallow.path(), &format!("{}^", r.edit), &r.edit, false).await,
            None
        );
    }

    /// Every git command the read runs is `rev-parse` or `diff`, with the
    /// network shut, and the repository's refs and index are as they were.
    #[tokio::test]
    async fn every_command_run_is_a_read() {
        let r = repo();
        let (_bin, calls, search) = recording_git();
        let git_dir = r.dir.path().join(".git");
        let before = (
            std::fs::read(git_dir.join("index")).unwrap(),
            git(r.dir.path(), &["for-each-ref"]),
        );

        for (base, head, ws) in [
            (r.reformat.clone(), r.edit.clone(), false),
            (r.root.clone(), r.edit.clone(), true),
            (format!("{}^", r.root), r.root.clone(), false),
            (
                "0123456789abcdef0123456789abcdef01234567".to_owned(),
                r.edit.clone(),
                false,
            ),
        ] {
            read_with(&search, r.dir.path(), &base, &head, ws).await;
        }
        assert!(holds_commits_with(&search, r.dir.path(), &[&r.root]).await);

        let calls = std::fs::read_to_string(&calls).unwrap();
        let calls: Vec<&str> = calls.lines().collect();
        assert!(calls.len() >= 8, "{calls:?}");
        for call in &calls {
            let words: Vec<&str> = call.split(' ').collect();
            assert!(call.contains("-c protocol.allow=never"), "{call}");
            let sub = words
                .iter()
                .enumerate()
                .find(|(i, w)| {
                    !w.starts_with('-') && *i > 0 && words[i - 1] != "-c" && words[i - 1] != "-C"
                })
                .map(|(_, w)| *w);
            assert!(matches!(sub, Some("rev-parse" | "diff")), "{call}");
            for write in ["--output", "-o", "--ext-diff", "--textconv"] {
                assert!(!words.contains(&write), "{call}");
            }
        }
        let after = (
            std::fs::read(git_dir.join("index")).unwrap(),
            git(r.dir.path(), &["for-each-ref"]),
        );
        assert_eq!(before, after);
    }
}
