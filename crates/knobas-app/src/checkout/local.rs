//! A repository's tree and files read at its checkout, at the commit the
//! mirror names for the branch (ADR-0041, the smoothness pass of PR #2479).
//!
//! *Files* used to wait for the source before it drew anything: the tip,
//! then the top directory, then the README, each a round trip. A checkout on
//! this disk already holds the tree, so the tab reads it here first and the
//! source behind it. Which commit is the mirror's answer, never the
//! checkout's: a branch's declared `commit` as the last sync stored it, or
//! for a repository its declared default branch's. A branch name never
//! reaches git (ADR-0036): the checkout's branch of that name may stand
//! anywhere.
//!
//! # What runs (ADR-0041, on ADR-0036's terms)
//!
//! Under [`range`](super::range)'s environment (`git_read`: no optional
//! locks, no fsmonitor, no prompt, closed stdin, no lazy fetch, no transport,
//! no replace refs, the ceiling at the checkout's parent), and nothing else:
//!
//! - `git rev-parse --verify --quiet <sha>^{commit}`, which checks the
//!   checkout holds the commit;
//! - `git ls-tree -r -t -z --long --full-tree <sha>`, the whole tree once,
//!   kept in memory by checkout and commit (it cannot change);
//! - `git cat-file blob <blob sha>`, a file's bytes by the sha the tree
//!   gave, so a path never reaches git either.
//!
//! Each argument is a checked sha (`is_sha`). A file is cut and named as the
//! source's view cuts and names it ([`knobas_source::file_text`]).

use std::collections::VecDeque;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use knobas_core::checkout::open::is_sha;
use knobas_source::file_text::{BYTE_CAP, capped, is_binary, language_of};
use knobas_source::view::{Block, EntryKind};
use serde::Serialize;

use super::range::{commit, git_read};
use super::uncommitted::run_capped;
use super::working_tree::{APPLE_SHIM, command_line_tools_installed, is_dir_off_runtime, locate};

/// How long one local read may take before the source is asked instead.
pub const TIMEOUT: std::time::Duration = super::range::TIMEOUT;

/// The most entries a tree read here carries: the source's own walk stops at
/// twenty thousand (`knobas-source-gitea`'s `TREE_PAGES`), and this one too.
pub const ENTRY_CAP: usize = 20_000;

/// The most of `ls-tree`'s answer read: twenty thousand long paths.
const LISTING_BYTES: usize = 16 * 1024 * 1024;

/// How many trees are kept in memory: a few repositories' current branch.
const KEPT: usize = 8;

/// One entry of a tree read at a checkout.
///
/// The smoothness pass (PR #2479).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct LocalEntry {
    /// The whole path from the top (`src/lib.rs`).
    pub path: String,
    pub kind: EntryKind,
    /// A file's size in bytes; `None` for a directory or a submodule.
    pub size: Option<u64>,
    /// The blob's sha: what a file's bytes are read by.
    #[serde(skip)]
    #[ts(skip)]
    pub blob: Option<String>,
}

/// A repository's whole tree at one commit, as its checkout holds it.
///
/// The smoothness pass (PR #2479).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct LocalTree {
    /// The whole sha it was read at: the mirror's tip of the branch.
    pub commit: String,
    /// When the mirror last stored that tip (RFC 3339): how old the guess
    /// that it is still the tip is.
    pub as_of: Option<String>,
    /// Every directory and file, in git's order.
    pub entries: Vec<LocalEntry>,
    /// [`ENTRY_CAP`] or the read's own cap stopped it short.
    pub truncated: bool,
}

/// One kept tree: the checkout it was read at, the sha asked, and the tree.
type Kept = (PathBuf, String, Arc<LocalTree>);

/// What [`tree`] keeps: trees by checkout and commit, newest last.
static TREES: LazyLock<Mutex<VecDeque<Kept>>> = LazyLock::new(|| Mutex::new(VecDeque::new()));

fn kept(path: &Path, sha: &str) -> Option<Arc<LocalTree>> {
    let trees = TREES.lock().unwrap_or_else(PoisonError::into_inner);
    trees
        .iter()
        .find(|(at, commit, _)| at == path && commit == sha)
        .map(|(_, _, tree)| tree.clone())
}

fn keep(path: &Path, tree: Arc<LocalTree>) {
    let mut trees = TREES.lock().unwrap_or_else(PoisonError::into_inner);
    trees.retain(|(at, commit, _)| !(at == path && *commit == tree.commit));
    trees.push_back((path.to_owned(), tree.commit.clone(), tree));
    while trees.len() > KEPT {
        trees.pop_front();
    }
}

/// The tree at `sha` in the checkout at `path`, or `None` when it cannot be
/// read here: no git, no repository, a sha the checkout does not hold, git
/// refusing or slow. `as_of` is left `None` for the caller to fill.
pub async fn tree(path: &Path, sha: &str) -> Option<Arc<LocalTree>> {
    let search = std::env::var_os("PATH").unwrap_or_default();
    tree_with(&search, path, sha).await
}

/// [`tree`] with the `PATH` to find `git` on.
pub async fn tree_with(search: &OsStr, path: &Path, sha: &str) -> Option<Arc<LocalTree>> {
    if !is_sha(sha) {
        return None;
    }
    if let Some(tree) = kept(path, sha) {
        return Some(tree);
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
    let read = async {
        let whole = commit(&git, path, sha).await?;
        let mut command = git_read(&git, path);
        command.args(["ls-tree", "-r", "-t", "-z", "--long", "--full-tree"]);
        command.arg(&whole);
        let (out, cut) = run_capped(command, usize::MAX, LISTING_BYTES).await.ok()?;
        let (entries, short) = parse_listing(&out);
        Some(Arc::new(LocalTree {
            commit: whole,
            as_of: None,
            entries,
            truncated: cut || short,
        }))
    };
    let tree = tokio::time::timeout(TIMEOUT, read).await.ok().flatten()?;
    keep(path, tree.clone());
    // A short sha asked for answers under the whole one too.
    if tree.commit != sha {
        keep_alias(path, sha, &tree);
    }
    Some(tree)
}

fn keep_alias(path: &Path, sha: &str, tree: &Arc<LocalTree>) {
    let mut trees = TREES.lock().unwrap_or_else(PoisonError::into_inner);
    trees.push_back((path.to_owned(), sha.to_owned(), tree.clone()));
    while trees.len() > KEPT {
        trees.pop_front();
    }
}

/// `ls-tree -r -t -z --long`'s records: `<mode> <type> <object> <size>\t<path>`,
/// each ended by a NUL. A record the read cut off is dropped. The second
/// value says [`ENTRY_CAP`] stopped it.
fn parse_listing(out: &[u8]) -> (Vec<LocalEntry>, bool) {
    let mut records: Vec<&[u8]> = out.split(|b| *b == 0).collect();
    // The last piece is empty after a whole answer, and a part record after a
    // cut one.
    records.pop();
    let mut entries = Vec::new();
    for record in records {
        if entries.len() >= ENTRY_CAP {
            return (entries, true);
        }
        let Some(tab) = record.iter().position(|b| *b == b'\t') else {
            continue;
        };
        let meta = String::from_utf8_lossy(&record[..tab]);
        let path = String::from_utf8_lossy(&record[tab + 1..]).into_owned();
        let mut fields = meta.split_whitespace();
        let (Some(mode), Some(kind), Some(object), Some(size)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let (kind, size, blob) = match (kind, mode) {
            ("tree", _) => (EntryKind::Dir, None, None),
            ("commit", _) => (EntryKind::Submodule, None, None),
            ("blob", "120000") => (EntryKind::Symlink, size.parse().ok(), Some(object)),
            ("blob", _) => (EntryKind::File, size.parse().ok(), Some(object)),
            _ => continue,
        };
        entries.push(LocalEntry {
            path,
            kind,
            size,
            blob: blob.map(str::to_owned),
        });
    }
    (entries, false)
}

/// The file at `file` in the tree at `sha`, drawn as the source's view draws
/// a file (its text cut at whole lines, binary told by a NUL, its blob sha
/// the base an edit commits against), or `None` when the checkout cannot
/// answer: the tree is not readable here, or holds no file there.
pub async fn file(path: &Path, sha: &str, file: &str) -> Option<Block> {
    let tree = tree(path, sha).await?;
    let entry = tree
        .entries
        .iter()
        .find(|e| e.path == file && matches!(e.kind, EntryKind::File | EntryKind::Symlink))?;
    let blob = entry.blob.as_deref().filter(|b| is_sha(b))?;
    let bytes = blob_bytes(path, blob, BYTE_CAP + 1).await?;
    let (bytes, cut) = bytes;
    let binary = is_binary(&bytes);
    let (text, truncated) = if binary {
        (String::new(), false)
    } else {
        capped(&bytes, cut)
    };
    Some(Block::File {
        trail: Vec::new(),
        path: file.to_owned(),
        text,
        language: language_of(file).map(str::to_owned),
        size: entry.size.unwrap_or(bytes.len() as u64),
        truncated,
        binary,
        url: None,
        sha: Some(blob.to_owned()),
        commit: Some(tree.commit.clone()),
    })
}

/// The bytes of the file at `file` in the tree at `sha`, whole up to `cap`:
/// `Some(Ok(bytes))`, `Some(Err(size))` for a file over the cap, `None` when
/// the checkout cannot answer.
pub async fn file_bytes(
    path: &Path,
    sha: &str,
    file: &str,
    cap: u64,
) -> Option<Result<Vec<u8>, u64>> {
    let tree = tree(path, sha).await?;
    let entry = tree
        .entries
        .iter()
        .find(|e| e.path == file && matches!(e.kind, EntryKind::File | EntryKind::Symlink))?;
    let size = entry.size?;
    if size > cap {
        return Some(Err(size));
    }
    let blob = entry.blob.as_deref().filter(|b| is_sha(b))?;
    let wanted = usize::try_from(size).ok()?.saturating_add(1);
    let (bytes, cut) = blob_bytes(path, blob, wanted).await?;
    (!cut).then_some(Ok(bytes))
}

/// `git cat-file blob <blob>`, read up to `max` bytes; the flag says the read
/// stopped at `max`.
async fn blob_bytes(path: &Path, blob: &str, max: usize) -> Option<(Vec<u8>, bool)> {
    let search = std::env::var_os("PATH").unwrap_or_default();
    let git = locate(
        &search,
        Path::new(APPLE_SHIM),
        command_line_tools_installed,
        TIMEOUT,
    )
    .await
    .ok()?;
    let mut command = git_read(&git, path);
    command.args(["cat-file", "blob"]);
    command.arg(blob);
    tokio::time::timeout(TIMEOUT, run_capped(command, usize::MAX, max))
        .await
        .ok()?
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// A repository of two commits: the first holds `README.md`,
    /// `src/lib.rs`, a binary `logo.png` and a link; the second changes the
    /// README in the working tree only after committing, so what is read is
    /// the commit and never the files on disk.
    fn repo() -> (tempfile::TempDir, String, String) {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        git(p, &["init", "-q"]);
        std::fs::create_dir(p.join("src")).unwrap();
        std::fs::write(p.join("README.md"), "# Payout\n").unwrap();
        std::fs::write(p.join("src/lib.rs"), "fn main() {}\n").unwrap();
        std::fs::write(p.join("logo.png"), b"PNG\0\x01\x02").unwrap();
        std::os::unix::fs::symlink("README.md", p.join("current")).unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-q", "-m", "first"]);
        let first = git(p, &["rev-parse", "HEAD"]);
        std::fs::write(p.join("README.md"), "# Payout service\n").unwrap();
        git(p, &["commit", "-q", "-am", "second"]);
        let second = git(p, &["rev-parse", "HEAD"]);
        std::fs::write(p.join("README.md"), "uncommitted\n").unwrap();
        (dir, first, second)
    }

    #[tokio::test]
    async fn the_tree_is_read_at_the_commit_named_with_every_kind() {
        let (dir, first, _) = repo();
        let tree = tree(dir.path(), &first)
            .await
            .expect("the checkout holds it");
        assert_eq!(tree.commit, first);
        let seen: Vec<(&str, EntryKind)> = tree
            .entries
            .iter()
            .map(|e| (e.path.as_str(), e.kind))
            .collect();
        assert_eq!(
            seen,
            [
                ("README.md", EntryKind::File),
                ("current", EntryKind::Symlink),
                ("logo.png", EntryKind::File),
                ("src", EntryKind::Dir),
                ("src/lib.rs", EntryKind::File),
            ]
        );
        assert_eq!(tree.entries[0].size, Some(9));
        assert!(!tree.truncated);
    }

    #[tokio::test]
    async fn a_file_is_the_commits_never_the_working_trees() {
        let (dir, first, second) = repo();
        let Some(Block::File {
            text, sha, commit, ..
        }) = file(dir.path(), &first, "README.md").await
        else {
            panic!("a file block");
        };
        assert_eq!(text, "# Payout\n");
        assert_eq!(commit.as_deref(), Some(first.as_str()));
        let blob = git(dir.path(), &["rev-parse", &format!("{first}:README.md")]);
        assert_eq!(sha.as_deref(), Some(blob.as_str()));
        let Some(Block::File { text, .. }) = file(dir.path(), &second, "README.md").await else {
            panic!("a file block");
        };
        assert_eq!(text, "# Payout service\n");
    }

    #[tokio::test]
    async fn a_binary_file_has_no_text_and_its_bytes_read_whole() {
        let (dir, first, _) = repo();
        let Some(Block::File { binary, text, .. }) = file(dir.path(), &first, "logo.png").await
        else {
            panic!("a file block");
        };
        assert!(binary);
        assert!(text.is_empty());
        assert_eq!(
            file_bytes(dir.path(), &first, "logo.png", 1024).await,
            Some(Ok(b"PNG\0\x01\x02".to_vec()))
        );
        assert_eq!(
            file_bytes(dir.path(), &first, "logo.png", 3).await,
            Some(Err(6))
        );
    }

    #[tokio::test]
    async fn what_the_checkout_cannot_answer_is_none() {
        let (dir, first, _) = repo();
        let missing = "0123456789abcdef0123456789abcdef01234567";
        assert!(tree(dir.path(), missing).await.is_none(), "not held");
        assert!(
            tree(dir.path(), "main").await.is_none(),
            "a name, not a sha"
        );
        assert!(file(dir.path(), &first, "nope.txt").await.is_none());
        assert!(
            file(dir.path(), &first, "src").await.is_none(),
            "a directory"
        );
        let elsewhere = tempfile::tempdir().unwrap();
        assert!(
            tree(elsewhere.path(), &first).await.is_none(),
            "no repository"
        );
    }

    #[tokio::test]
    async fn reading_writes_nothing_at_the_checkout() {
        let (dir, first, _) = repo();
        let index = std::fs::read(dir.path().join(".git/index")).unwrap();
        let refs = git(dir.path(), &["for-each-ref"]);
        let status = git(dir.path(), &["status", "--porcelain"]);
        let _ = tree(dir.path(), &first).await.unwrap();
        let _ = file(dir.path(), &first, "README.md").await.unwrap();
        assert_eq!(std::fs::read(dir.path().join(".git/index")).unwrap(), index);
        assert_eq!(git(dir.path(), &["for-each-ref"]), refs);
        assert_eq!(git(dir.path(), &["status", "--porcelain"]), status);
    }

    #[test]
    fn a_record_the_read_cut_is_dropped() {
        let out = b"100644 blob aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa      12\ta.txt\x00100644 blob bbbb";
        let (entries, short) = parse_listing(out);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "a.txt");
        assert!(!short);
    }
}
