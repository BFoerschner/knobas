//! Where a checkout's working tree stands, read by the real `git` (#704,
//! ADR-0021).
//!
//! Every repository here is made by `git` itself in a temp dir, and the read
//! runs the same `git` the app would: ADR-0021's witness for a read-only
//! subprocess is the subprocess against a real repository, not a desktop run
//! and not a mock (ADR-0013). The fixture's commands are isolated from the
//! machine's own git configuration; the read under test is not, because the
//! app's is not either.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use knobas_app::checkout::working_tree::{TreeRead, WorkingTreeState, read, read_with};

/// Run `git` in `dir` for the fixture, away from the machine's configuration,
/// and fail the test with git's own words if it refuses.
fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=knobas test",
            "-c",
            "user.email=test@knobas.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
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
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A repository on `main` with one commit holding `a.txt`.
fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    git(dir.path(), &["add", "a.txt"]);
    git(dir.path(), &["commit", "-q", "-m", "first"]);
    dir
}

fn commit(dir: &Path, file: &str, body: &str, message: &str) {
    fs::write(dir.join(file), body).unwrap();
    git(dir, &["add", file]);
    git(dir, &["commit", "-q", "-m", message]);
}

async fn state(dir: &Path) -> WorkingTreeState {
    match read(dir).await {
        TreeRead::Read(state) => state,
        other => panic!("expected a read of {}, got {other:?}", dir.display()),
    }
}

#[tokio::test]
async fn a_clean_tree_names_its_branch_and_head_and_nothing_to_do() {
    let dir = repo();
    let head = git(dir.path(), &["rev-parse", "HEAD"]);

    let state = state(dir.path()).await;
    assert_eq!(state.branch.as_deref(), Some("main"));
    assert_eq!(state.head.as_deref(), Some(head.as_str()));
    assert_eq!(state.upstream, None);
    assert_eq!((state.ahead, state.behind), (0, 0));
    assert_eq!(
        (state.changed, state.untracked, state.conflicted),
        (0, 0, 0)
    );
}

#[tokio::test]
async fn modified_staged_and_untracked_files_are_counted_apart() {
    let dir = repo();
    commit(dir.path(), "b.txt", "two\n", "second");
    // One modified in the tree, one staged, two untracked.
    fs::write(dir.path().join("a.txt"), "changed\n").unwrap();
    fs::write(dir.path().join("b.txt"), "staged\n").unwrap();
    git(dir.path(), &["add", "b.txt"]);
    fs::write(dir.path().join("new-1.txt"), "x").unwrap();
    fs::write(dir.path().join("new-2.txt"), "y").unwrap();

    let state = state(dir.path()).await;
    assert_eq!(state.changed, 2);
    assert_eq!(state.untracked, 2);
    assert_eq!(state.conflicted, 0);
}

#[tokio::test]
async fn a_rename_is_one_change() {
    let dir = repo();
    git(dir.path(), &["mv", "a.txt", "renamed.txt"]);

    let state = state(dir.path()).await;
    assert_eq!((state.changed, state.untracked), (1, 0));
}

#[tokio::test]
async fn a_merge_conflict_is_counted_as_conflicted_and_not_as_changed() {
    let dir = repo();
    git(dir.path(), &["checkout", "-q", "-b", "other"]);
    commit(dir.path(), "a.txt", "theirs\n", "theirs");
    git(dir.path(), &["checkout", "-q", "main"]);
    commit(dir.path(), "a.txt", "ours\n", "ours");
    // The merge fails on the conflict, which is the state under test.
    let merge = Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t.invalid",
            "merge",
            "other",
        ])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(!merge.status.success(), "the merge was meant to conflict");

    let state = state(dir.path()).await;
    assert_eq!(state.conflicted, 1);
    assert_eq!(state.changed, 0);
    assert_eq!(state.branch.as_deref(), Some("main"));
}

#[tokio::test]
async fn a_detached_head_has_no_branch_and_keeps_its_commit() {
    let dir = repo();
    let first = git(dir.path(), &["rev-parse", "HEAD"]);
    commit(dir.path(), "b.txt", "two\n", "second");
    git(dir.path(), &["checkout", "-q", "--detach", &first]);

    let state = state(dir.path()).await;
    assert_eq!(state.branch, None);
    assert_eq!(state.head.as_deref(), Some(first.as_str()));
}

#[tokio::test]
async fn ahead_and_behind_are_measured_against_a_local_upstream() {
    let dir = repo();
    git(dir.path(), &["branch", "upstream-branch"]);
    git(
        dir.path(),
        &["branch", "--set-upstream-to=upstream-branch", "main"],
    );
    // Two on main the upstream lacks, one on the upstream main lacks.
    commit(dir.path(), "b.txt", "two\n", "second");
    commit(dir.path(), "c.txt", "three\n", "third");
    git(dir.path(), &["checkout", "-q", "upstream-branch"]);
    commit(dir.path(), "d.txt", "four\n", "elsewhere");
    git(dir.path(), &["checkout", "-q", "main"]);

    let state = state(dir.path()).await;
    assert_eq!(state.upstream.as_deref(), Some("upstream-branch"));
    assert_eq!((state.ahead, state.behind), (2, 1));
}

#[tokio::test]
async fn a_repository_with_no_commit_yet_has_a_branch_and_no_head() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);

    let state = state(dir.path()).await;
    assert_eq!(state.branch.as_deref(), Some("main"));
    assert_eq!(state.head, None);
}

#[tokio::test]
async fn a_directory_that_is_not_a_repository_says_so() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(read(dir.path()).await, TreeRead::NotARepo);
    // And a path that is not there at all: an override typed by hand can
    // outlive the directory it named.
    assert_eq!(read(&dir.path().join("gone")).await, TreeRead::NotARepo);
}

#[tokio::test]
async fn git_missing_from_path_is_a_state_and_not_an_error() {
    let dir = repo();
    let empty = tempfile::tempdir().unwrap();
    let search: OsString = empty.path().as_os_str().to_owned();
    assert_eq!(read_with(&search, dir.path()).await, TreeRead::NoGit);
}

/// Every ref under `.git`, by name and target, and `HEAD`'s own file.
fn refs(dir: &Path) -> (String, Vec<u8>) {
    (
        git(dir, &["for-each-ref", "--format=%(refname) %(objectname)"]),
        fs::read(dir.join(".git").join("HEAD")).unwrap(),
    )
}

fn index_of(dir: &Path) -> (Vec<u8>, std::time::SystemTime) {
    let path: PathBuf = dir.join(".git").join("index");
    (
        fs::read(&path).unwrap(),
        fs::metadata(&path).unwrap().modified().unwrap(),
    )
}

/// The read changes nothing: not the index, not a ref.
///
/// The tree is set up so a plain `git status` **would** write: the tracked
/// file is rewritten with the same bytes a second later, so its stat no longer
/// matches the index and status refreshes the index it read. Only
/// `--no-optional-locks` keeps that refresh in memory.
#[tokio::test]
async fn the_read_writes_neither_the_index_nor_a_ref() {
    let dir = repo();
    git(dir.path(), &["branch", "upstream-branch"]);
    git(
        dir.path(),
        &["branch", "--set-upstream-to=upstream-branch", "main"],
    );
    std::thread::sleep(std::time::Duration::from_millis(1100));
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();

    let index_before = index_of(dir.path());
    let refs_before = refs(dir.path());

    let state = state(dir.path()).await;
    assert_eq!(state.changed, 0, "same bytes, so nothing changed");

    assert_eq!(index_of(dir.path()), index_before, ".git/index was written");
    assert_eq!(refs(dir.path()), refs_before, "a ref moved");
}

/// A git that does not answer is abandoned at the timeout, and the panel says
/// so rather than waiting with it.
///
/// The real `git`, made slow by the repository's own configuration: a clean
/// filter that sleeps, on a tracked file whose stat no longer matches the
/// index, so `status` runs the filter to compare its content. ADR-0013 asks a
/// fault the real program can be driven into to be witnessed by the real
/// program, and git can be (a slow LFS filter, or any other, does the same).
#[cfg(unix)]
#[tokio::test]
async fn a_git_that_does_not_answer_is_abandoned_at_the_timeout() {
    let dir = repo();
    git(
        dir.path(),
        &["config", "filter.slow.clean", "sleep 10; cat"],
    );
    let info = dir.path().join(".git").join("info");
    fs::create_dir_all(&info).unwrap();
    fs::write(info.join("attributes"), "a.txt filter=slow\n").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();

    let started = std::time::Instant::now();
    let answer = read(dir.path()).await;
    let took = started.elapsed();

    assert_eq!(
        answer,
        TreeRead::Failed {
            reason: "git did not answer within 2 s".to_owned()
        }
    );
    assert!(
        took < std::time::Duration::from_secs(10),
        "the read waited {took:?} for a git that never answers"
    );
}
