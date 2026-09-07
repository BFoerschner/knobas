//! The clones-root scan, over a real directory tree (#499).
//!
//! In a test file rather than beside the pure functions in
//! `src/checkout.rs` because this one needs a filesystem: `tempfile` is a
//! dev-dependency and the module itself stays free of one.
//!
//! The tree every test builds is the one spec #491 describes and the ticket
//! names: a clone directly under the root, a clone one level further down, a
//! directory that is not a repository at all, and a clone **three** levels
//! down that must not be found.

use std::fs;
use std::path::{Path, PathBuf};

use knobas_core::checkout::{RemoteKey, find, remote_key, scan};

const REPO_URL: &str = "https://gitea.example.com/tidewater/payout-service";

/// A directory with a `.git/config` naming `remote` as its `origin`.
fn clone_at(root: &Path, relative: &str, remote: &str) -> PathBuf {
    let dir = root.join(relative);
    fs::create_dir_all(dir.join(".git")).unwrap();
    fs::write(
        dir.join(".git").join("config"),
        format!("[core]\n\tbare = false\n[remote \"origin\"]\n\turl = {remote}\n"),
    )
    .unwrap();
    dir
}

/// The tree the acceptance criteria describe.
///
/// Two clones that match the repository under test -- one at each allowed
/// depth, and spelled with the two remote forms so the match is doing the
/// normalising rather than a string comparison -- plus a directory that is not
/// a repository, a clone of a *different* repository, and one three levels
/// down.
struct Tree {
    _root: tempfile::TempDir,
    path: PathBuf,
    shallow: PathBuf,
    deep: PathBuf,
    too_deep: PathBuf,
}

fn tree() -> Tree {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_path_buf();

    // Depth 1, https with a `.git` suffix.
    let shallow = clone_at(&path, "payout-service", &format!("{REPO_URL}.git"));
    // Depth 2, the ssh spelling of the same repository.
    let deep = clone_at(
        &path,
        "gitea.example.com/payout-service",
        "git@gitea.example.com:tidewater/payout-service.git",
    );
    // A directory that is no repository: it must be walked *through* (the
    // depth-2 clone above sits under one) and never reported.
    fs::create_dir_all(path.join("notes").join("scratch")).unwrap();
    fs::write(path.join("notes").join("todo.md"), "nothing here\n").unwrap();
    // Another repository, so a match is a match on the key and not on "the
    // only clone there was".
    clone_at(
        &path,
        "ledger",
        "https://gitea.example.com/tidewater/ledger.git",
    );
    // Depth 3. `archive` and `2024` are ordinary directories.
    let too_deep = clone_at(&path, "archive/2024/payout-service", REPO_URL);

    Tree {
        _root: root,
        path,
        shallow,
        deep,
        too_deep,
    }
}

/// Every clone at depth one or two, and nothing deeper.
#[test]
fn the_scan_reaches_two_levels_and_no_further() {
    let tree = tree();
    let found = scan(&tree.path);

    let paths: Vec<&Path> = found.iter().map(|clone| clone.path.as_path()).collect();
    assert!(paths.contains(&tree.shallow.as_path()), "{paths:?}");
    assert!(paths.contains(&tree.deep.as_path()), "{paths:?}");
    assert!(
        !paths.contains(&tree.too_deep.as_path()),
        "a clone three levels down is out of scope: {paths:?}"
    );
    assert_eq!(
        found.len(),
        3,
        "two clones of the repository under test plus the ledger: {paths:?}"
    );
    // The directory that is not a repository is not a clone, and the walk
    // still went through it -- which the depth-2 clone above proves.
    assert!(
        !paths.iter().any(|path| path.ends_with("notes")),
        "a directory with no .git in it is not a clone: {paths:?}"
    );
}

/// The two spellings in the tree reduce to one key, which is what makes a
/// checkout findable however the person cloned it.
#[test]
fn both_spellings_of_the_remote_carry_the_same_key() {
    let tree = tree();
    let found = scan(&tree.path);
    let wanted = remote_key(REPO_URL).expect("the repo's URL is a key");
    let matching: Vec<&RemoteKey> = found
        .iter()
        .filter(|clone| clone.key == wanted)
        .map(|clone| &clone.key)
        .collect();
    assert_eq!(
        matching.len(),
        2,
        "one https clone and one ssh clone: {found:?}"
    );
}

/// Two clones of one repository resolve to the **same** one on every read, and
/// it is the shallower one.
///
/// Load-bearing rather than cosmetic: without an order, `find` answers whatever
/// `read_dir` handed back first, and the editor #501 opens would land somewhere
/// new from one click to the next. `gitea.example.com/payout-service` sorts
/// *before* `payout-service` by path alone, so a plain path sort would answer
/// the deep one -- which is what this asserts is not what happens.
#[test]
fn the_shallower_of_two_clones_wins_and_the_answer_is_stable() {
    let tree = tree();
    assert_eq!(
        find(&tree.path, REPO_URL).as_deref(),
        Some(tree.shallow.as_path())
    );
    for _ in 0..5 {
        assert_eq!(
            find(&tree.path, REPO_URL).as_deref(),
            Some(tree.shallow.as_path())
        );
    }
    assert!(
        tree.deep < tree.shallow,
        "the deep clone sorts first by path, which is what makes this test mean something"
    );
}

/// The match, end to end: the repo's stored URL in, a path on this disk out.
#[test]
fn a_repo_url_finds_its_checkout() {
    let tree = tree();
    assert_eq!(
        find(&tree.path, REPO_URL).as_deref(),
        Some(tree.shallow.as_path())
    );
    // Whichever way the *stored* URL is spelled, since the key is what matches.
    assert_eq!(
        find(
            &tree.path,
            "git@gitea.example.com:tidewater/payout-service.git"
        )
        .as_deref(),
        Some(tree.shallow.as_path())
    );
}

/// A repository with no clone under the root is a miss, and so is a root that
/// is not there at all -- the state a *no checkout* line reports.
#[test]
fn a_repository_with_no_clone_is_a_miss() {
    let tree = tree();
    assert_eq!(
        find(
            &tree.path,
            "https://gitea.example.com/tidewater/nowhere.git"
        ),
        None
    );
    assert_eq!(
        find(
            &tree.path,
            "https://other.example.com/tidewater/payout-service"
        ),
        None,
        "the host is part of the key: two forges may hold the same owner/repo"
    );
    assert_eq!(find(&tree.path.join("missing"), REPO_URL), None);
    assert!(scan(&tree.path.join("missing")).is_empty());
}

/// A linked worktree carries a `.git` **file**, not a directory. It is not
/// found by the scan, deliberately: spec #491 gives that case to the hand-set
/// override, and following the pointer would mean guessing at a `commondir`.
#[test]
fn a_worktree_is_not_found_by_the_scan() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("payout-service-wt");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(".git"), "gitdir: /elsewhere/.git/worktrees/wt\n").unwrap();
    assert!(scan(root.path()).is_empty());
    assert_eq!(find(root.path(), REPO_URL), None);
}

/// A clone is not descended into, so a submodule inside one is not reported as
/// a checkout of its own.
#[test]
fn a_submodule_inside_a_clone_is_not_a_second_checkout() {
    let root = tempfile::tempdir().unwrap();
    clone_at(root.path(), "payout-service", REPO_URL);
    clone_at(
        root.path(),
        "payout-service/vendor",
        "https://gitea.example.com/tidewater/ledger.git",
    );
    let found = scan(root.path());
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].path, root.path().join("payout-service"));
}
