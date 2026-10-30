//! A checkout's uncommitted diff, read by the real `git` (#824, ADR-0026).
//!
//! As for the working tree state (`working_tree.rs`): every repository is made
//! by `git` in a temp dir and the read runs the `git` the app would. The
//! fixture's commands are isolated from the machine's git configuration; the
//! read under test is not, because the app's is not either.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use knobas_app::checkout::uncommitted::{DiffRead, UncommittedDiff, read, read_with};
use knobas_source::diff::{DiffChange, DiffLineKind, MAX_LINES_PER_VIEW};

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

/// A repository on `main` with one commit holding `a.txt` and `b.txt`.
fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    fs::write(dir.path().join("a.txt"), "one\ntwo\nthree\n").unwrap();
    fs::write(dir.path().join("b.txt"), "bee\n").unwrap();
    git(dir.path(), &["add", "a.txt", "b.txt"]);
    git(dir.path(), &["commit", "-q", "-m", "first"]);
    dir
}

async fn diff(dir: &Path) -> UncommittedDiff {
    match read(dir).await {
        DiffRead::Read(diff) => diff,
        other => panic!("expected a read of {}, got {other:?}", dir.display()),
    }
}

#[tokio::test]
async fn a_modified_file_is_in_the_unstaged_diff_with_its_lines() {
    let dir = repo();
    fs::write(dir.path().join("a.txt"), "one\nTWO\nthree\n").unwrap();

    let diff = diff(dir.path()).await;

    assert!(diff.staged.files.is_empty(), "{:?}", diff.staged);
    assert_eq!(diff.unstaged.files.len(), 1);
    let file = &diff.unstaged.files[0];
    assert_eq!(file.path, "a.txt");
    assert_eq!(file.change, DiffChange::Modified);
    assert_eq!((file.additions, file.deletions), (1, 1));
    let changed: Vec<(DiffLineKind, &str)> = file.hunks[0]
        .lines
        .iter()
        .filter(|line| line.kind != DiffLineKind::Context)
        .map(|line| (line.kind, line.text.as_str()))
        .collect();
    assert_eq!(
        changed,
        [(DiffLineKind::Del, "two"), (DiffLineKind::Add, "TWO")]
    );
    assert!(!diff.unstaged.truncated);
    assert!(diff.untracked.is_empty());
}

#[tokio::test]
async fn a_staged_file_is_in_the_staged_diff_and_not_the_unstaged_one() {
    let dir = repo();
    fs::write(dir.path().join("b.txt"), "bee\nsting\n").unwrap();
    git(dir.path(), &["add", "b.txt"]);

    let diff = diff(dir.path()).await;

    assert!(diff.unstaged.files.is_empty(), "{:?}", diff.unstaged);
    assert_eq!(diff.staged.files.len(), 1);
    let file = &diff.staged.files[0];
    assert_eq!(
        (file.path.as_str(), file.change),
        ("b.txt", DiffChange::Modified)
    );
    assert_eq!((file.additions, file.deletions), (1, 0));
}

/// A file staged and then changed again is in both halves, each with its
/// own change.
#[tokio::test]
async fn a_file_staged_and_changed_again_is_in_both_halves() {
    let dir = repo();
    fs::write(dir.path().join("b.txt"), "staged\n").unwrap();
    git(dir.path(), &["add", "b.txt"]);
    fs::write(dir.path().join("b.txt"), "staged\nand more\n").unwrap();

    let diff = diff(dir.path()).await;

    let line = |half: &knobas_app::checkout::uncommitted::CappedDiff| {
        half.files[0].hunks[0]
            .lines
            .iter()
            .filter(|l| l.kind == DiffLineKind::Add)
            .map(|l| l.text.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(line(&diff.staged), ["staged"]);
    assert_eq!(line(&diff.unstaged), ["and more"]);
}

/// `diff` does not show a new file git does not track; the read lists it
/// from `status`, a new directory once.
#[tokio::test]
async fn an_untracked_file_is_listed_by_path_and_is_in_neither_diff() {
    let dir = repo();
    fs::write(dir.path().join("new.txt"), "fresh\n").unwrap();
    fs::create_dir(dir.path().join("dir")).unwrap();
    fs::write(dir.path().join("dir").join("one.txt"), "1").unwrap();
    fs::write(dir.path().join("dir").join("two.txt"), "2").unwrap();
    fs::write(dir.path().join(".gitignore"), "ignored.log\n").unwrap();
    fs::write(dir.path().join("ignored.log"), "x").unwrap();

    let diff = diff(dir.path()).await;

    assert!(diff.unstaged.files.is_empty());
    assert!(diff.staged.files.is_empty());
    let mut untracked = diff.untracked.clone();
    untracked.sort();
    assert_eq!(untracked, [".gitignore", "dir/", "new.txt"]);
    assert!(!diff.untracked_truncated);
}

/// A staged rename's original path is a field of its own in `status -z`, and
/// is never taken for an untracked path -- not even one whose name reads like
/// an untracked entry (`? odd.txt`).
#[tokio::test]
async fn a_staged_rename_is_one_renamed_file_and_no_untracked_path() {
    let dir = repo();
    fs::write(dir.path().join("? odd.txt"), "odd\n").unwrap();
    git(dir.path(), &["add", "? odd.txt"]);
    git(dir.path(), &["commit", "-q", "-m", "odd"]);
    git(dir.path(), &["mv", "? odd.txt", "moved.txt"]);

    let diff = diff(dir.path()).await;

    assert!(diff.untracked.is_empty(), "{:?}", diff.untracked);
    assert_eq!(diff.staged.files.len(), 1);
    let file = &diff.staged.files[0];
    assert_eq!(file.change, DiffChange::Renamed);
    assert_eq!(
        (file.path.as_str(), file.old_path.as_deref()),
        ("moved.txt", Some("? odd.txt"))
    );
}

#[tokio::test]
async fn a_changed_binary_file_is_binary_with_no_lines() {
    let dir = repo();
    fs::write(dir.path().join("logo.bin"), [0u8, 1, 2, 3, 0, 255]).unwrap();
    git(dir.path(), &["add", "logo.bin"]);
    git(dir.path(), &["commit", "-q", "-m", "logo"]);
    fs::write(dir.path().join("logo.bin"), [0u8, 9, 9, 9, 0, 255]).unwrap();

    let diff = diff(dir.path()).await;

    assert_eq!(diff.unstaged.files.len(), 1);
    let file = &diff.unstaged.files[0];
    assert_eq!(
        (file.path.as_str(), file.change),
        ("logo.bin", DiffChange::Binary)
    );
    assert!(file.hunks.is_empty());
}

/// A file over the ceiling (#2269) keeps the lines before the cut and its
/// full counts, says it was cut, and the half says the ceiling cut it; the
/// file after it keeps its counts and no lines.
#[tokio::test]
async fn a_file_over_the_ceiling_is_cut_and_keeps_its_counts() {
    let dir = repo();
    let big: String = (0..MAX_LINES_PER_VIEW + 200)
        .map(|n| format!("line {n}\n"))
        .collect();
    fs::write(dir.path().join("a.txt"), big).unwrap();
    fs::write(dir.path().join("b.txt"), "bee\nafter\n").unwrap();

    let diff = diff(dir.path()).await;

    let files = &diff.unstaged.files;
    assert_eq!(files.len(), 2);
    let a = &files[0];
    assert_eq!(a.path, "a.txt");
    assert!(a.truncated);
    assert_eq!(a.additions as usize, MAX_LINES_PER_VIEW + 200);
    let kept: usize = a.hunks.iter().map(|h| h.lines.len()).sum();
    assert_eq!(kept, MAX_LINES_PER_VIEW);
    assert!(diff.unstaged.truncated);
    let b = &files[1];
    assert_eq!(
        (b.path.as_str(), b.truncated, b.additions),
        ("b.txt", true, 1)
    );
    assert!(b.hunks.iter().all(|h| h.lines.is_empty()));
}

/// The paths the shared parser strips stay `a/` and `b/` whatever prefixes
/// the checkout's own configuration asks git for: `diff.mnemonicPrefix`
/// (`i/`, `w/`, `c/`) would leave its letter on every path.
#[tokio::test]
async fn a_mnemonic_prefix_does_not_change_the_paths() {
    let dir = repo();
    git(dir.path(), &["config", "diff.mnemonicPrefix", "true"]);
    fs::write(dir.path().join("a.txt"), "changed\n").unwrap();
    fs::write(dir.path().join("b.txt"), "changed\n").unwrap();
    git(dir.path(), &["add", "b.txt"]);

    let diff = diff(dir.path()).await;

    assert_eq!(diff.unstaged.files[0].path, "a.txt");
    assert_eq!(diff.staged.files[0].path, "b.txt");
}

/// Under `diff.noprefix` a file in a directory called `b` would lose that
/// directory to the parser's prefix strip.
#[tokio::test]
async fn no_prefix_does_not_cost_a_directory_called_b() {
    let dir = repo();
    fs::create_dir(dir.path().join("b")).unwrap();
    fs::write(dir.path().join("b").join("x.txt"), "x\n").unwrap();
    git(dir.path(), &["add", "b/x.txt"]);
    git(dir.path(), &["commit", "-q", "-m", "b dir"]);
    git(dir.path(), &["config", "diff.noprefix", "true"]);
    fs::write(dir.path().join("b").join("x.txt"), "changed\n").unwrap();

    let diff = diff(dir.path()).await;

    assert_eq!(diff.unstaged.files[0].path, "b/x.txt");
}

/// A diff driver or a textconv filter the checkout's configuration names is
/// never run: `--no-ext-diff` and `--no-textconv` (ADR-0026).
#[cfg(unix)]
#[tokio::test]
async fn a_configured_diff_driver_is_never_run() {
    let dir = repo();
    let marker = dir.path().join(".git").join("driver-ran");
    let touch = format!("touch '{}'; cat", marker.display());
    git(dir.path(), &["config", "diff.ext.command", &touch]);
    git(dir.path(), &["config", "diff.conv.textconv", &touch]);
    git(dir.path(), &["config", "diff.external", &touch]);
    let info = dir.path().join(".git").join("info");
    fs::create_dir_all(&info).unwrap();
    fs::write(info.join("attributes"), "a.txt diff=ext\nb.txt diff=conv\n").unwrap();
    fs::write(dir.path().join("a.txt"), "changed\n").unwrap();
    fs::write(dir.path().join("b.txt"), "changed\n").unwrap();
    git(dir.path(), &["add", "b.txt"]);

    let diff = diff(dir.path()).await;

    assert!(!marker.exists(), "a configured diff driver ran");
    assert_eq!(diff.unstaged.files[0].path, "a.txt");
    assert_eq!(diff.staged.files[0].path, "b.txt");
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
/// Set up so a plain `git diff` **would** write: a tracked file rewritten with
/// the same bytes a second later no longer matches the index's stat, and
/// `diff` refreshes the index's stat data after the diff. `--no-optional-locks`
/// does not stop that; `-c diff.autoRefreshIndex=false` does.
#[tokio::test]
async fn the_read_leaves_the_index_and_the_refs_as_they_were() {
    let dir = repo();
    fs::write(dir.path().join("b.txt"), "staged\n").unwrap();
    git(dir.path(), &["add", "b.txt"]);
    std::thread::sleep(std::time::Duration::from_millis(1100));
    fs::write(dir.path().join("a.txt"), "one\ntwo\nthree\n").unwrap();
    fs::write(dir.path().join("new.txt"), "x").unwrap();

    let index_before = index_of(dir.path());
    let refs_before = git(
        dir.path(),
        &["for-each-ref", "--format=%(refname) %(objectname)"],
    );
    let head_before = fs::read(dir.path().join(".git").join("HEAD")).unwrap();

    let diff = diff(dir.path()).await;
    assert!(
        diff.unstaged.files.is_empty(),
        "same bytes, so nothing changed"
    );
    assert_eq!(diff.staged.files.len(), 1);

    assert_eq!(index_of(dir.path()), index_before, ".git/index was written");
    assert_eq!(
        git(
            dir.path(),
            &["for-each-ref", "--format=%(refname) %(objectname)"]
        ),
        refs_before,
        "a ref moved"
    );
    assert_eq!(
        fs::read(dir.path().join(".git").join("HEAD")).unwrap(),
        head_before
    );
}

#[tokio::test]
async fn a_directory_that_is_not_a_repository_and_a_missing_path_say_so() {
    let plain = tempfile::tempdir().unwrap();
    assert_eq!(read(plain.path()).await, DiffRead::NotARepo);
    assert_eq!(read(&plain.path().join("gone")).await, DiffRead::NotARepo);
}

#[tokio::test]
async fn no_git_on_the_path_says_so() {
    let dir = repo();
    let empty = tempfile::tempdir().unwrap();
    assert_eq!(
        read_with(empty.path().as_os_str(), dir.path()).await,
        DiffRead::NoGit
    );
}
