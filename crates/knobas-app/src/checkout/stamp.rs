//! Whether anything in a checkout changed since its working tree was last
//! read: the stamp [`super::working_tree::read_known`] keeps beside each
//! answer, so a checkout nothing touched answers what `git status` said last
//! time without running it again (the scale pass on PR #2479, 2026-11-10).
//!
//! Code reads every repository's checkout each time the page is shown,
//! because a commit or a clone made in a terminal changes a checkout and no
//! sync brings that. At Björn's size that was 65 `git status` runs on every
//! return. The stamp is what keeps that truth without them.
//!
//! # What it covers
//!
//! Everything `git status --porcelain=v2 --branch --untracked-files=normal`
//! reads, by its file metadata (modified time to the nanosecond, size, inode
//! and kind), read with no process started and nothing written:
//!
//! - **git's own state**: `HEAD`, the index, `packed-refs`, every loose ref
//!   under `refs/`, `logs/HEAD`, `FETCH_HEAD`, `config` and `info/exclude`. A
//!   commit, a checkout, a stage, a fetch, a reset or a merge moves one.
//! - **every tracked file**, by the paths the index lists: an edit in an
//!   editor moves its file whether or not anything is staged.
//! - **every directory that holds a tracked file**, and the top: a file
//!   created, deleted or renamed in one moves the directory, which is how a
//!   new untracked file or folder shows.
//! - **the user's global ignore file** (`$XDG_CONFIG_HOME/git/ignore`, else
//!   `~/.config/git/ignore`), which can change what is untracked.
//!
//! Inside an untracked folder nothing is stamped, and need not be: `status`
//! counts such a folder once, however its contents change.
//!
//! # Where it gives up
//!
//! No stamp at all, so the caller runs `git status` as before, for anything it
//! does not read whole: a `.git` that is a file (a linked worktree or a
//! submodule), an index it cannot parse or of a version other than 2, 3 or 4,
//! a split or sparse index (`link`, `sdir`), and no index yet. Off Unix it
//! gives up always: the metadata it compares is Unix's.

#[cfg(unix)]
use std::collections::BTreeSet;
#[cfg(unix)]
use std::hash::{Hash, Hasher};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;

/// The stamp of the checkout at `tree`, or `None` where it cannot be read
/// whole (see the module's *Where it gives up*). Two equal stamps mean that
/// none of what the module covers changed in between.
#[cfg(not(unix))]
#[must_use]
pub fn of(_tree: &Path) -> Option<u64> {
    None
}

/// The stamp of the checkout at `tree`, or `None` where it cannot be read
/// whole (see the module's *Where it gives up*).
#[cfg(unix)]
#[must_use]
pub fn of(tree: &Path) -> Option<u64> {
    let git = tree.join(".git");
    if !std::fs::symlink_metadata(&git).ok()?.is_dir() {
        return None;
    }
    let index = std::fs::read(git.join("index")).ok()?;
    let paths = tracked_paths(&index)?;
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    for name in [
        "HEAD",
        "index",
        "packed-refs",
        "logs/HEAD",
        "FETCH_HEAD",
        "config",
        "info/exclude",
    ] {
        stat(&git.join(name), &mut hash);
    }
    refs(&git.join("refs"), &mut hash);
    if let Some(ignore) = global_ignore() {
        stat(&ignore, &mut hash);
    }
    let mut dirs: BTreeSet<&[u8]> = BTreeSet::new();
    dirs.insert(b"");
    for path in &paths {
        let mut end = path.len();
        while let Some(slash) = path[..end].iter().rposition(|&b| b == b'/') {
            if !dirs.insert(&path[..slash]) {
                break;
            }
            end = slash;
        }
        stat(&tree.join(as_path(path)), &mut hash);
    }
    for dir in dirs {
        stat(&tree.join(as_path(dir)), &mut hash);
    }
    Some(hash.finish())
}

/// A path the index spells in bytes, as this platform's path.
#[cfg(unix)]
fn as_path(bytes: &[u8]) -> &Path {
    use std::os::unix::ffi::OsStrExt;
    Path::new(std::ffi::OsStr::from_bytes(bytes))
}

/// `path`'s metadata into `hash`, not following a symlink, as git compares
/// it; a path that is not there hashes as absent.
#[cfg(unix)]
fn stat(path: &Path, hash: &mut impl Hasher) {
    path.hash(hash);
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            (
                meta.mtime(),
                meta.mtime_nsec(),
                meta.ctime(),
                meta.ctime_nsec(),
                meta.size(),
                meta.ino(),
                meta.mode(),
            )
                .hash(hash);
        }
        Err(_) => 0u8.hash(hash),
    }
}

/// Every loose ref under `dir`, by its metadata, in path order.
#[cfg(unix)]
fn refs(dir: &Path, hash: &mut impl Hasher) {
    stat(dir, hash);
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_dir()) {
            refs(&path, hash);
        } else {
            stat(&path, hash);
        }
    }
}

/// The global excludes file git reads when `core.excludesFile` is not set.
#[cfg(unix)]
fn global_ignore() -> Option<PathBuf> {
    match std::env::var_os("XDG_CONFIG_HOME").filter(|dir| !dir.is_empty()) {
        Some(dir) => Some(PathBuf::from(dir).join("git").join("ignore")),
        None => std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config/git/ignore")),
    }
}

/// The paths a git index lists (`Documentation/gitformat-index.txt`),
/// versions 2 to 4; `None` for anything else, and for a split or sparse
/// index, whose entries are not all here.
#[must_use]
pub fn tracked_paths(index: &[u8]) -> Option<Vec<Vec<u8>>> {
    if index.len() < 12 + 20 || &index[..4] != b"DIRC" {
        return None;
    }
    let version = u32::from_be_bytes(index[4..8].try_into().ok()?);
    if !(2..=4).contains(&version) {
        return None;
    }
    let count = usize::try_from(u32::from_be_bytes(index[8..12].try_into().ok()?)).ok()?;
    // The trailing checksum is not an entry or an extension.
    let body = &index[..index.len() - 20];
    let mut at: usize = 12;
    let mut paths: Vec<Vec<u8>> = Vec::with_capacity(count);
    let mut previous: Vec<u8> = Vec::new();
    for _ in 0..count {
        let start = at;
        // ctime, mtime, dev, ino, mode, uid, gid, size, then the object id.
        at = at.checked_add(40 + 20)?;
        let flags = u16::from_be_bytes(body.get(at..at + 2)?.try_into().ok()?);
        at += 2;
        if version >= 3 && flags & 0x4000 != 0 {
            let extended = u16::from_be_bytes(body.get(at..at + 2)?.try_into().ok()?);
            // `skip-worktree` (0x4000): a sparse checkout, whose files are
            // not on disk by design.
            if extended & 0x4000 != 0 {
                return None;
            }
            at += 2;
        }
        let name = if version == 4 {
            // How many bytes of the previous name to drop, as a varint.
            let (strip, used) = varint(body.get(at..)?)?;
            at += used;
            let end = at + body.get(at..)?.iter().position(|&b| b == 0)?;
            let keep = previous.len().checked_sub(strip)?;
            let mut name = previous[..keep].to_vec();
            name.extend_from_slice(&body[at..end]);
            at = end + 1;
            name
        } else {
            let end = at + body.get(at..)?.iter().position(|&b| b == 0)?;
            let name = body[at..end].to_vec();
            // Padded with NULs to a multiple of eight from the entry's start.
            let length = end - start;
            at = start + (length / 8 + 1) * 8;
            name
        };
        if name.is_empty() {
            return None;
        }
        previous.clone_from(&name);
        paths.push(name);
    }
    // The extensions: a split index (`link`) or a sparse directory entry
    // (`sdir`) means the entries above are not every tracked path.
    while at + 8 <= body.len() {
        let signature = &body[at..at + 4];
        if signature == b"link" || signature == b"sdir" {
            return None;
        }
        let size =
            usize::try_from(u32::from_be_bytes(body[at + 4..at + 8].try_into().ok()?)).ok()?;
        at = at.checked_add(8)?.checked_add(size)?;
    }
    paths.dedup();
    Some(paths)
}

/// The offset varint of index version 4: `(value, bytes read)`.
fn varint(bytes: &[u8]) -> Option<(usize, usize)> {
    let mut value: usize = 0;
    for (n, &byte) in bytes.iter().enumerate().take(10) {
        if n > 0 {
            value = value.checked_add(1)?.checked_shl(7)?;
        }
        value |= usize::from(byte & 0x7f);
        if byte & 0x80 == 0 {
            return Some((value, n + 1));
        }
    }
    None
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// `git` run in `dir`, quietly, as a person at a terminal would.
    fn git(dir: &Path, args: &[&str]) -> String {
        let out = knobas_db::console::command("git")
            .current_dir(dir)
            .args([
                "-c",
                "user.name=T",
                "-c",
                "user.email=t@example.com",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// A clone with two commits, a nested file, an ignored folder and an
    /// upstream ref.
    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        git(p, &["init", "-q", "-b", "main"]);
        std::fs::write(p.join("README.md"), "hello\n").unwrap();
        std::fs::create_dir_all(p.join("src/deep")).unwrap();
        std::fs::write(p.join("src/deep/lib.rs"), "fn a() {}\n").unwrap();
        std::fs::write(p.join(".gitignore"), "target/\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-q", "-m", "one"]);
        std::fs::write(p.join("src/main.rs"), "fn main() {}\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-q", "-m", "two"]);
        git(p, &["update-ref", "refs/remotes/origin/main", "HEAD~1"]);
        std::fs::create_dir_all(p.join("target")).unwrap();
        dir
    }

    /// The index lists exactly the paths `git ls-files` does, in every
    /// version git writes.
    #[test]
    fn the_index_lists_what_git_tracks_in_versions_two_to_four() {
        let dir = repo();
        let p = dir.path();
        let listed: Vec<Vec<u8>> = git(p, &["ls-files", "-z"])
            .split('\0')
            .filter(|name| !name.is_empty())
            .map(|name| name.as_bytes().to_vec())
            .collect();
        for version in ["2", "3", "4"] {
            git(p, &["update-index", "--index-version", version]);
            let read = tracked_paths(&std::fs::read(p.join(".git/index")).unwrap());
            assert_eq!(read.as_ref(), Some(&listed), "index version {version}");
        }
    }

    /// Nothing done, the same stamp; and every change `git status` would see
    /// moves it: an edit nobody staged, a new file, a new folder, a deleted
    /// file, a stage, a commit made in a terminal, a fetch, a checkout.
    #[test]
    fn every_change_status_would_see_moves_the_stamp() {
        let dir = repo();
        let p = dir.path();
        let mut before = of(p).expect("a plain clone is stamped");
        assert_eq!(of(p), Some(before), "nothing changed");
        // Something git does not see: a file in an ignored folder.
        std::fs::write(p.join("target/out.o"), "x").unwrap();
        assert_eq!(of(p), Some(before), "an ignored folder's contents");
        let steps: [(&str, &dyn Fn()); 8] = [
            ("an unstaged edit", &|| {
                std::fs::write(p.join("src/deep/lib.rs"), "fn b() {}\n").unwrap()
            }),
            ("a new file", &|| {
                std::fs::write(p.join("src/deep/new.rs"), "").unwrap()
            }),
            ("a new folder", &|| {
                std::fs::create_dir(p.join("notes")).unwrap()
            }),
            ("a deleted file", &|| {
                std::fs::remove_file(p.join("README.md")).unwrap()
            }),
            ("a stage", &|| {
                git(p, &["add", "-A"]);
            }),
            ("a commit", &|| {
                git(p, &["commit", "-q", "-m", "three"]);
            }),
            ("a fetch", &|| {
                git(p, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
            }),
            ("a checkout", &|| {
                git(p, &["checkout", "-q", "-b", "topic"]);
            }),
        ];
        for (what, step) in steps {
            // Past the file system's clock tick, so an mtime can move.
            std::thread::sleep(std::time::Duration::from_millis(20));
            step();
            let after = of(p).expect("still stamped");
            assert_ne!(after, before, "{what} did not move the stamp");
            before = after;
        }
    }

    /// A linked worktree's `.git` is a file, and is never stamped.
    #[test]
    fn a_linked_worktree_is_not_stamped() {
        let dir = repo();
        let linked = dir.path().join("linked");
        git(
            dir.path(),
            &[
                "worktree",
                "add",
                "-q",
                linked.to_str().unwrap(),
                "-b",
                "other",
            ],
        );
        assert_eq!(of(&linked), None);
    }
}
