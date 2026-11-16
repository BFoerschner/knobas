//! Every read of `knobas-git` against the `git` on `PATH`: the answer is
//! git's standard output byte for byte, or the test fails (the git-starts
//! pass of PR #2479, 2026-11-16).
//!
//! The fixtures are made by git itself, with fixed dates and no
//! configuration but the repository's own, and each read is asked the way
//! the app asks it (`crates/knobas-app/src/checkout/reader.rs`). The reader
//! is opened the same way ([`Repo::open_isolated`]), so an answer does not
//! depend on the machine's `~/.gitconfig`.
//!
//! `examples/parity.rs` runs the same comparison over a real repository's
//! whole history; these are the edge cases, one test a case, so a change
//! that breaks one names it.

use std::path::Path;
use std::process::Command;

use knobas_git::{Answer, AskGit, DiffAsk, DiffForm, Read, Repo};

/// The moment "now" is, for git and for the reader: relative dates read the
/// same on both sides.
const NOW: i64 = 1_790_000_000;
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

fn base(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("LC_ALL", "C")
        .env("GIT_TEST_DATE_NOW", NOW.to_string())
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_EXTERNAL_DIFF");
    command
}

/// git at `dir`, to make a fixture: it must succeed. `at` is the commit's
/// date, seconds after the fixtures' first.
fn git_at(dir: &Path, at: i64, args: &[&str]) -> String {
    let date = format!("{} +0000", 1_780_000_000 + at);
    let out = base(dir)
        .args([
            "-c",
            "user.name=Mara Okafor",
            "-c",
            "user.email=mara@knobas.invalid",
        ])
        .args(["-c", "commit.gpgsign=false", "-c", "core.autocrlf=false"])
        .args(args)
        .env("GIT_AUTHOR_DATE", &date)
        .env("GIT_COMMITTER_DATE", &date)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn git(dir: &Path, args: &[&str]) -> String {
    git_at(dir, 0, args)
}

/// What git answers a read, run as the app runs it.
fn oracle(dir: &Path, args: &[&str]) -> (bool, Vec<u8>) {
    let out = base(dir)
        .args([
            "--no-optional-locks",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "protocol.allow=never",
        ])
        .args(args)
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .expect("git runs");
    (out.status.success(), out.stdout)
}

fn open(dir: &Path) -> Repo {
    Repo::open_isolated(dir).expect("the fixture opens in-process")
}

/// The reader's answer is git's: the same bytes, or both refuse.
#[track_caller]
fn same(what: &str, ours: Read, theirs: (bool, Vec<u8>)) {
    match ours {
        Err(AskGit(why)) => panic!("{what}: left to git ({why}), where it should answer"),
        Ok(Answer::Refused) => assert!(
            !theirs.0,
            "{what}: refused, and git answered {:?}",
            String::from_utf8_lossy(&theirs.1)
        ),
        Ok(Answer::Out(out)) => {
            assert!(theirs.0, "{what}: answered, and git refused");
            assert!(
                out == theirs.1,
                "{what}\n--- git\n{}\n--- the reader\n{}",
                String::from_utf8_lossy(&theirs.1),
                String::from_utf8_lossy(&out)
            );
        }
    }
}

/// `git diff` between `old` and `new` (or the range `old`) in every form,
/// with and without `-w`, only `paths` when any: each the reader's answer.
#[track_caller]
fn same_diffs(dir: &Path, old: &str, new: Option<&str>, paths: &[&str]) {
    let repo = open(dir);
    for ignore_space in [false, true] {
        for form in [DiffForm::Raw, DiffForm::RawNumstat, DiffForm::Patch] {
            let mut args = vec![
                "-c",
                "diff.autoRefreshIndex=false",
                "diff",
                "--no-ext-diff",
                "--no-color",
                "--no-textconv",
                "-M",
            ];
            match form {
                DiffForm::Raw => args.extend(["--raw", "-z"]),
                DiffForm::RawNumstat => args.extend(["--raw", "--numstat", "-z"]),
                DiffForm::Patch => args.extend(["--src-prefix=a/", "--dst-prefix=b/"]),
            }
            if ignore_space {
                args.push("-w");
            }
            args.push(old);
            args.extend(new);
            args.push("--");
            args.extend(paths);
            let theirs = {
                let out = base(dir)
                    .args([
                        "--no-optional-locks",
                        "-c",
                        "core.fsmonitor=false",
                        "-c",
                        "protocol.allow=never",
                    ])
                    .args(&args)
                    .env("GIT_NO_LAZY_FETCH", "1")
                    .env("GIT_NO_REPLACE_OBJECTS", "1")
                    .env("GIT_LITERAL_PATHSPECS", "1")
                    .output()
                    .expect("git runs");
                (out.status.success(), out.stdout)
            };
            let ours = repo.diff(&DiffAsk {
                old,
                new,
                form,
                ignore_space,
                paths,
                max_lines: usize::MAX,
                max_bytes: usize::MAX,
            });
            same(
                &format!(
                    "diff {form:?}{} {old} {new:?} {paths:?}",
                    if ignore_space { " -w" } else { "" }
                ),
                ours,
                theirs,
            );
        }
    }
}

fn write(dir: &Path, path: &str, content: impl AsRef<[u8]>) {
    let at = dir.join(path);
    std::fs::create_dir_all(at.parent().expect("a parent")).expect("the folder");
    std::fs::write(at, content).expect("the file");
}

fn commit(dir: &Path, at: i64, message: &str) -> String {
    git(dir, &["add", "-A"]);
    git_at(dir, at, &["commit", "-q", "--allow-empty", "-m", message]);
    git(dir, &["rev-parse", "HEAD"])
}

/// `git mv`, into a folder that may not be there yet.
fn mv(dir: &Path, from: &str, to: &str) {
    std::fs::create_dir_all(dir.join(to).parent().expect("a parent")).expect("the folder");
    git(dir, &["mv", from, to]);
}

fn new_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a folder");
    git(dir.path(), &["init", "-q", "-b", "main"]);
    dir
}

/// A Rust-like source of `n` functions, each a few lines.
fn source(n: usize, mark: &str) -> String {
    (0..n)
        .map(|i| format!("fn step_{i}(input: u32) -> u32 {{\n    let value = input + {i};\n    // {mark}\n    value * 2\n}}\n\n"))
        .collect()
}

// -- commits, trees and files ---------------------------------------------------

#[test]
fn a_commit_is_resolved_as_git_resolves_it() {
    let dir = new_repo();
    let p = dir.path();
    write(p, "a.txt", "one\n");
    let first = commit(p, 0, "first");
    write(p, "a.txt", "two\n");
    let second = commit(p, 60, "second");
    git(p, &["tag", "-a", "v1", "-m", "an annotated tag", &second]);
    let tag = git(p, &["rev-parse", "v1"]);
    let tree = git(p, &["rev-parse", &format!("{second}^{{tree}}")]);
    let repo = open(p);
    let ask = |name: &str| {
        oracle(
            p,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                "--end-of-options",
                &format!("{name}^{{commit}}"),
            ],
        )
    };
    for name in [
        first.clone(),
        second.clone(),
        second[..9].to_owned(),
        format!("{second}^"),
        format!("{}^", &second[..12]),
        // A root has no parent; a tag is the commit it names; a tree is none.
        format!("{first}^"),
        tag.clone(),
        format!("{tag}^"),
        tree,
        "0123456789abcdef0123456789abcdef01234567".to_owned(),
        "0123456".to_owned(),
    ] {
        same(
            &format!("rev-parse {name}"),
            repo.rev_parse_commit(&name),
            ask(&name),
        );
    }
    assert_eq!(
        repo.rev_parse_commit(&format!("{second}^")),
        Ok(Answer::Out(format!("{first}\n").into_bytes())),
        "a parent is the first commit, whole"
    );
    same(
        "shallow and format",
        repo.shallow_and_format(),
        oracle(
            p,
            &[
                "rev-parse",
                "--is-shallow-repository",
                "--show-object-format",
            ],
        ),
    );
    // Only a sha is a name here: a branch is git's to resolve, or no one's.
    assert!(repo.rev_parse_commit("main").is_err());
    assert!(repo.rev_parse_commit("-p").is_err());
}

/// A tree with every kind of entry, and names that sort differently as
/// files and as folders (`a-b` < `a/` < `a.txt` < `a0`).
fn kinds() -> (tempfile::TempDir, String) {
    let dir = new_repo();
    let p = dir.path();
    write(p, "README.md", "# kinds\n");
    write(p, "a/inner.txt", "in a\n");
    write(p, "a/deep/er/leaf.txt", "leaf\n");
    write(p, "a-b", "dash\n");
    write(p, "a.txt", "dot\n");
    write(p, "a0", "zero\n");
    write(p, "empty.txt", "");
    write(p, "run.sh", "#!/bin/sh\necho run\n");
    write(p, "logo.bin", [0u8, 159, 146, 150, 0, 1, 2, 3]);
    write(p, "with space.txt", "space\n");
    write(p, "ünï/cödé.txt", "unicode\n");
    write(p, "big.txt", "line\n".repeat(5000));
    #[cfg(unix)]
    std::os::unix::fs::symlink("README.md", p.join("link")).expect("a link");
    git(p, &["add", "-A"]);
    git(p, &["update-index", "--chmod=+x", "run.sh"]);
    // A submodule's entry: a commit this repository does not hold.
    git(
        p,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            "160000,1234567890123456789012345678901234567890,vendor/lib",
        ],
    );
    git_at(p, 0, &["commit", "-q", "-m", "every kind"]);
    let sha = git(p, &["rev-parse", "HEAD"]);
    (dir, sha)
}

#[test]
fn a_tree_is_listed_as_git_lists_it() {
    let (dir, sha) = kinds();
    let p = dir.path();
    let repo = open(p);
    let theirs = oracle(
        p,
        &["ls-tree", "-r", "-t", "-z", "--long", "--full-tree", &sha],
    );
    let listing = String::from_utf8_lossy(&theirs.1).into_owned();
    for kind in ["040000 tree", "100644 blob", "100755 blob", "160000 commit"] {
        assert!(listing.contains(kind), "the fixture holds a {kind}");
    }
    // A link is made where the test can make one.
    assert!(cfg!(not(unix)) || listing.contains("120000 blob"));
    same("ls-tree", repo.ls_tree(&sha), theirs);
    same(
        "ls-tree of a commit not here",
        repo.ls_tree("0123456789abcdef0123456789abcdef01234567"),
        oracle(
            p,
            &[
                "ls-tree",
                "-r",
                "-t",
                "-z",
                "--long",
                "--full-tree",
                "0123456789abcdef0123456789abcdef01234567",
            ],
        ),
    );
}

#[test]
fn a_files_bytes_are_gits_up_to_the_cap() {
    let (dir, sha) = kinds();
    let p = dir.path();
    let repo = open(p);
    for path in [
        "README.md",
        "empty.txt",
        "logo.bin",
        "big.txt",
        "a/deep/er/leaf.txt",
    ] {
        let blob = git(p, &["rev-parse", &format!("{sha}:{path}")]);
        let (ok, whole) = oracle(p, &["cat-file", "blob", &blob]);
        assert!(ok);
        for max in [usize::MAX, whole.len() + 1, whole.len(), 7, 1] {
            // The cap is the reader's of git's output: the same cut.
            assert_eq!(
                repo.cat_blob(&blob, max).expect("answered").expect("held"),
                knobas_git::capped(&whole, usize::MAX, max),
                "{path} at {max}"
            );
        }
    }
    assert_eq!(
        repo.cat_blob("0123456789abcdef0123456789abcdef01234567", 10),
        Ok(None)
    );
    // A tree is no blob: git's to refuse.
    let tree = git(p, &["rev-parse", &format!("{sha}^{{tree}}")]);
    assert!(repo.cat_blob(&tree, 10).is_err());
}

// -- diffs: one test an edge case -----------------------------------------------

#[test]
fn an_edit_is_shown_with_its_function_line_and_context() {
    let dir = new_repo();
    let p = dir.path();
    write(p, "src/lib.rs", source(40, "v1"));
    let first = commit(p, 0, "first");
    let mut text = source(40, "v1");
    text = text.replace("let value = input + 7;", "let value = input + 70;");
    text = text.replace(
        "let value = input + 8;",
        "let value = input + 80;\n    let more = 1;",
    );
    text = text.replace(
        "fn step_30(input: u32) -> u32 {\n",
        "fn step_30(input: u32) -> u32 {\n    // a comment added\n",
    );
    text.push_str("fn last() {}\n");
    write(p, "src/lib.rs", &text);
    let second = commit(p, 60, "second");
    same_diffs(p, &first, Some(&second), &[]);
    same_diffs(p, &second, Some(&first), &[]);
    same_diffs(
        p,
        &format!("{first}^{{commit}}"),
        Some(&format!("{second}^{{commit}}")),
        &[],
    );
    same_diffs(p, EMPTY_TREE, Some(&first), &[]);
}

#[test]
fn a_rename_is_paired_and_scored_as_git_pairs_it() {
    let dir = new_repo();
    let p = dir.path();
    let guide: String = (0..120)
        .map(|i| format!("Line {i} of the guide, with words enough to be a line.\n"))
        .collect();
    write(p, "docs/guide.md", &guide);
    write(p, "src/util.rs", source(12, "util"));
    write(p, "src/same_a.txt", "identical\n");
    write(p, "src/same_b.txt", "identical\n");
    write(
        p,
        "keep/notes.txt",
        "notes one\nnotes two\nnotes three\nnotes four\n",
    );
    write(p, "old/name.c", source(20, "c"));
    write(
        p,
        "gone.txt",
        "this one is deleted and nothing like it is added\n",
    );
    write(p, "empty_old.txt", "");
    let first = commit(p, 0, "first");
    // The same content elsewhere, with and without the same file name.
    mv(p, "src/util.rs", "lib/util.rs");
    mv(p, "src/same_a.txt", "moved/same_a.txt");
    mv(p, "src/same_b.txt", "moved/other_name.txt");
    mv(p, "empty_old.txt", "empty_new.txt");
    // Edited and moved: by name where the file name stays, by content else.
    mv(p, "keep/notes.txt", "kept/notes.txt");
    write(
        p,
        "kept/notes.txt",
        "notes one\nnotes two\nnotes three\nnotes four\nnotes five\n",
    );
    mv(p, "docs/guide.md", "docs/manual.md");
    write(
        p,
        "docs/manual.md",
        guide
            .replace("Line 7 ", "Row 7 ")
            .replace("Line 90 ", "Row 90 "),
    );
    mv(p, "old/name.c", "new/renamed.c");
    write(
        p,
        "new/renamed.c",
        source(20, "c").replace("input + 3;", "input + 33;"),
    );
    std::fs::remove_file(p.join("gone.txt")).expect("removed");
    write(
        p,
        "added.txt",
        "nothing like the deleted one: a new file altogether\n",
    );
    let second = commit(p, 60, "moved");
    let repo = open(p);
    let raw = repo
        .diff(&DiffAsk {
            old: &first,
            new: Some(&second),
            form: DiffForm::Raw,
            ignore_space: false,
            paths: &[],
            max_lines: usize::MAX,
            max_bytes: usize::MAX,
        })
        .expect("answered");
    let Answer::Out(raw) = raw else {
        panic!("refused")
    };
    let raw = String::from_utf8_lossy(&raw).into_owned();
    assert!(raw.contains("R100"), "an exact rename: {raw}");
    assert!(
        raw.matches(" R0").count() >= 2,
        "renames with edits, scored: {raw}"
    );
    assert!(
        raw.contains(" D\0gone.txt") && raw.contains(" A\0added.txt"),
        "{raw}"
    );
    same_diffs(p, &first, Some(&second), &[]);
    same_diffs(p, &second, Some(&first), &[]);
    // One file of it, as a page of the diff asks: with its old path it is
    // the rename, alone it is an added file.
    same_diffs(
        p,
        &first,
        Some(&second),
        &["docs/manual.md", "docs/guide.md"],
    );
    same_diffs(p, &first, Some(&second), &["docs/manual.md"]);
    same_diffs(p, &first, Some(&second), &["lib/util.rs", "src/util.rs"]);
    same_diffs(p, &first, Some(&second), &["new"]);
}

#[test]
fn many_files_of_the_same_content_pair_in_gits_order() {
    let dir = new_repo();
    let p = dir.path();
    for i in 0..6 {
        write(p, &format!("from/f{i}.txt"), "the very same content\n");
    }
    write(p, "from/other.txt", "other content\n");
    let first = commit(p, 0, "first");
    std::fs::remove_dir_all(p.join("from")).expect("removed");
    for i in 0..4 {
        write(p, &format!("to/g{i}.txt"), "the very same content\n");
    }
    write(p, "to/f3.txt", "the very same content\n");
    write(p, "to/other.txt", "other content\n");
    let second = commit(p, 60, "second");
    same_diffs(p, &first, Some(&second), &[]);
}

#[test]
fn a_binary_file_is_told_and_never_diffed() {
    let dir = new_repo();
    let p = dir.path();
    write(p, "logo.bin", [137u8, 80, 78, 71, 0, 0, 1, 2, 3]);
    write(p, "late_nul.txt", [vec![b'a'; 9000], vec![0u8]].concat());
    write(p, "text.txt", "text\n");
    let first = commit(p, 0, "first");
    write(p, "logo.bin", [137u8, 80, 78, 71, 0, 0, 9, 9, 9, 9]);
    write(p, "late_nul.txt", [vec![b'b'; 9000], vec![0u8]].concat());
    write(p, "new.bin", [0u8, 1, 2]);
    write(
        p,
        "text.txt",
        [b"now ".to_vec(), vec![0u8], b" binary\n".to_vec()].concat(),
    );
    let second = commit(p, 60, "second");
    mv(p, "new.bin", "moved.bin");
    let third = commit(p, 120, "a binary file moved");
    same_diffs(p, &first, Some(&second), &[]);
    same_diffs(p, &second, Some(&third), &[]);
    same_diffs(p, EMPTY_TREE, Some(&second), &[]);
    let numstat = oracle(p, &["diff", "--numstat", &first, &second]).1;
    assert!(String::from_utf8_lossy(&numstat).contains("-\t-\tlogo.bin"));
    // A NUL past the first 8,000 bytes is not looked at: a text.
    assert!(!String::from_utf8_lossy(&numstat).contains("-\t-\tlate_nul.txt"));
}

#[test]
fn crlf_lines_are_compared_as_they_are_stored() {
    let dir = new_repo();
    let p = dir.path();
    write(p, "dos.txt", "one\r\ntwo\r\nthree\r\nfour\r\n");
    write(p, "mixed.txt", "one\ntwo\r\nthree\n");
    write(
        p,
        "moved_from.txt",
        "alpha\r\nbeta\r\ngamma\r\ndelta\r\nepsilon\r\nzeta\r\neta\r\ntheta\r\n",
    );
    let first = commit(p, 0, "first");
    write(p, "dos.txt", "one\r\n2\r\nthree\r\nfour\n");
    write(p, "mixed.txt", "one\r\ntwo\nthree\n");
    mv(p, "moved_from.txt", "moved_to.txt");
    // Line endings changed and a line edited: a rename still, and how alike
    // counts no carriage return of a text.
    write(
        p,
        "moved_to.txt",
        "alpha\nbeta\ngamma\ndelta\nepsilon\nzeta\neta\nTHETA\n",
    );
    let second = commit(p, 60, "second");
    same_diffs(p, &first, Some(&second), &[]);
}

#[test]
fn a_mode_a_link_and_a_change_of_kind_are_written_as_git_writes_them() {
    let dir = new_repo();
    let p = dir.path();
    write(p, "run.sh", "#!/bin/sh\necho run\n");
    write(p, "both.sh", "#!/bin/sh\necho both\n");
    write(p, "kind", "a plain file\nof two lines\n");
    write(p, "target.txt", "target\n");
    #[cfg(unix)]
    std::os::unix::fs::symlink("target.txt", p.join("link")).expect("a link");
    let first = commit(p, 0, "first");
    git(p, &["update-index", "--chmod=+x", "run.sh"]);
    write(p, "both.sh", "#!/bin/sh\necho both, edited\n");
    git(p, &["add", "both.sh"]);
    git(p, &["update-index", "--chmod=+x", "both.sh"]);
    #[cfg(unix)]
    {
        std::fs::remove_file(p.join("link")).expect("removed");
        std::os::unix::fs::symlink("elsewhere/target.txt", p.join("link")).expect("a link");
        std::fs::remove_file(p.join("kind")).expect("removed");
        std::os::unix::fs::symlink("target.txt", p.join("kind")).expect("a link");
        git(p, &["add", "link", "kind"]);
    }
    git_at(p, 60, &["commit", "-q", "-m", "modes and links"]);
    let second = git(p, &["rev-parse", "HEAD"]);
    same_diffs(p, &first, Some(&second), &[]);
    same_diffs(p, &second, Some(&first), &[]);
    same_diffs(p, &first, Some(&second), &["kind"]);
}

#[test]
fn a_submodule_is_a_line_naming_its_commit() {
    let dir = new_repo();
    let p = dir.path();
    write(p, "a.txt", "a\n");
    git(p, &["add", "-A"]);
    git(
        p,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            "160000,1111111111111111111111111111111111111111,vendor/lib",
        ],
    );
    git_at(p, 0, &["commit", "-q", "-m", "a submodule"]);
    let first = git(p, &["rev-parse", "HEAD"]);
    git(
        p,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            "160000,2222222222222222222222222222222222222222,vendor/lib",
        ],
    );
    git(
        p,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            "160000,3333333333333333333333333333333333333333,vendor/new",
        ],
    );
    git_at(p, 60, &["commit", "-q", "-m", "moved on"]);
    let second = git(p, &["rev-parse", "HEAD"]);
    git(p, &["update-index", "--force-remove", "vendor/lib"]);
    git_at(p, 120, &["commit", "-q", "-m", "gone"]);
    let third = git(p, &["rev-parse", "HEAD"]);
    same_diffs(p, &first, Some(&second), &[]);
    same_diffs(p, &second, Some(&third), &[]);
    same_diffs(p, EMPTY_TREE, Some(&second), &[]);
}

#[test]
fn a_last_line_without_a_newline_and_an_empty_file_say_so() {
    let dir = new_repo();
    let p = dir.path();
    write(p, "no_newline.txt", "one\ntwo");
    write(p, "gains_one.txt", "one\ntwo");
    write(p, "loses_one.txt", "one\ntwo\n");
    write(p, "empty.txt", "");
    write(p, "emptied.txt", "content\n");
    let first = commit(p, 0, "first");
    write(p, "no_newline.txt", "one\nTWO");
    write(p, "gains_one.txt", "one\ntwo\n");
    write(p, "loses_one.txt", "one\ntwo");
    write(p, "empty.txt", "now a line\n");
    write(p, "emptied.txt", "");
    write(p, "new_empty.txt", "");
    let second = commit(p, 60, "second");
    same_diffs(p, &first, Some(&second), &[]);
    same_diffs(p, &second, Some(&first), &[]);
    same_diffs(p, EMPTY_TREE, Some(&first), &[]);
}

#[test]
fn a_name_is_quoted_where_git_quotes_it() {
    let dir = new_repo();
    let p = dir.path();
    // A tab and a double quote are no file names on Windows.
    let names: &[&str] = if cfg!(unix) {
        &[
            "with space.txt",
            "ünï/cödé.txt",
            "tab\there.txt",
            "quote\"d.txt",
            "plain.txt",
        ]
    } else {
        &["with space.txt", "ünï/cödé.txt", "plain.txt"]
    };
    for name in names {
        write(p, name, format!("{name} one\n"));
    }
    let first = commit(p, 0, "first");
    for name in names {
        write(p, name, format!("{name} two\n"));
    }
    mv(p, "plain.txt", "now with space.txt");
    let second = commit(p, 60, "second");
    same_diffs(p, &first, Some(&second), &[]);
    // `core.quotePath` off: the bytes past ASCII as they are.
    git(p, &["config", "core.quotePath", "false"]);
    same_diffs(p, &first, Some(&second), &[]);
    same_diffs(p, &first, Some(&second), &["ünï/cödé.txt"]);
}

#[test]
fn blanks_ignored_leave_what_else_changed() {
    let dir = new_repo();
    let p = dir.path();
    write(
        p,
        "indent.py",
        "def pay():\n    total = 1\n    return total\n\n\ndef other():\n    return 2\n",
    );
    write(p, "only_blanks.txt", "a b\nc\td\n");
    write(p, "mode_and_blanks.sh", "echo  one\n");
    let first = commit(p, 0, "first");
    write(
        p,
        "indent.py",
        "def pay():\n  total  =  1   \n  return total\n\n\ndef other():\n    return 3\n",
    );
    write(p, "only_blanks.txt", "a  b\nc d\n");
    write(p, "mode_and_blanks.sh", "echo one\n");
    git(p, &["add", "-A"]);
    git(p, &["update-index", "--chmod=+x", "mode_and_blanks.sh"]);
    git_at(p, 60, &["commit", "-q", "-m", "reindented"]);
    let second = git(p, &["rev-parse", "HEAD"]);
    same_diffs(p, &first, Some(&second), &[]);
    // A block that can sit in two places is placed by its own indentation,
    // though it was compared without its blanks.
    let block = |n: usize| -> String {
        (0..n)
            .map(|i| format!("    if x == {i} {{\n        run({i});\n    }}\n"))
            .collect()
    };
    write(p, "slide.rs", format!("fn f() {{\n{}}}\n", block(6)));
    let third = commit(p, 120, "third");
    write(
        p,
        "slide.rs",
        format!(
            "fn f() {{\n{}    if x == 2 {{\n\t\trun(2);\n    }}\n{}}}\n",
            block(3),
            block(6).split_at(block(3).len()).1
        ),
    );
    let fourth = commit(p, 180, "fourth");
    same_diffs(p, &third, Some(&fourth), &[]);
}

#[test]
fn a_file_no_diff_is_wanted_of_is_binary_and_one_forced_is_text() {
    let dir = new_repo();
    let p = dir.path();
    write(
        p,
        ".gitattributes",
        "*.lock -diff\n*.dat diff\nvendor/** binary\n",
    );
    write(p, "deps.lock", "a = 1\nb = 2\n");
    write(
        p,
        "raw.dat",
        [b"text ".to_vec(), vec![0u8], b" forced\n".to_vec()].concat(),
    );
    write(p, "vendor/lib.js", "var a = 1;\n");
    write(p, "plain.txt", "plain\n");
    let first = commit(p, 0, "first");
    write(p, "deps.lock", "a = 1\nb = 3\n");
    write(
        p,
        "raw.dat",
        [b"text ".to_vec(), vec![0u8], b" forced, edited\n".to_vec()].concat(),
    );
    write(p, "vendor/lib.js", "var a = 2;\n");
    write(p, "plain.txt", "plain, edited\n");
    let second = commit(p, 60, "second");
    let patch = String::from_utf8_lossy(&oracle(p, &["diff", &first, &second]).1).into_owned();
    assert!(
        patch.contains("Binary files a/deps.lock and b/deps.lock differ"),
        "{patch}"
    );
    assert!(
        patch.contains("Binary files a/vendor/lib.js and b/vendor/lib.js differ"),
        "{patch}"
    );
    assert!(patch.contains("+++ b/raw.dat"), "{patch}");
    same_diffs(p, &first, Some(&second), &[]);
}

#[test]
fn a_function_line_is_cut_where_git_cuts_it() {
    let dir = new_repo();
    let p = dir.path();
    let body = " x\n".repeat(8);
    let long = format!("fn {}(…tail) {{\n", "f".repeat(74));
    let latin1 = [
        b"static caf\xe9 = 1; // not UTF-8\n".to_vec(),
        body.clone().into_bytes(),
        b" old\n".to_vec(),
    ]
    .concat();
    write(p, "long.rs", format!("{long}{body} old\n"));
    write(p, "latin1.c", &latin1);
    write(
        p,
        "spaces.rs",
        format!("fn {}        (…) {{\n{body} old\n", "g".repeat(67)),
    );
    let first = commit(p, 0, "first");
    write(p, "long.rs", format!("{long}{body} new\n"));
    write(
        p,
        "latin1.c",
        [&latin1[..latin1.len() - 5], b" new\n"].concat(),
    );
    write(
        p,
        "spaces.rs",
        format!("fn {}        (…) {{\n{body} new\n", "g".repeat(67)),
    );
    let second = commit(p, 60, "second");
    same_diffs(p, &first, Some(&second), &[]);
}

#[test]
fn a_range_from_the_merge_base_is_the_three_dot_diff() {
    let dir = new_repo();
    let p = dir.path();
    write(p, "a.txt", "one\ntwo\nthree\n");
    let base = commit(p, 0, "base");
    git(p, &["checkout", "-q", "-b", "topic"]);
    write(p, "topic.txt", "topic\n");
    write(p, "a.txt", "one\ntwo\nthree\nfour\n");
    let topic = commit(p, 60, "topic");
    git(p, &["checkout", "-q", "main"]);
    write(p, "main.txt", "main\n");
    let main = commit(p, 120, "main moves on");
    let range = format!("{main}^{{commit}}...{topic}^{{commit}}");
    same_diffs(p, &range, None, &[]);
    same_diffs(p, &format!("{main}...{topic}"), None, &[]);
    same_diffs(p, &base, Some(&topic), &[]);
    // No shared history: git refuses, and so does the reader.
    git(p, &["checkout", "-q", "--orphan", "alone"]);
    let alone = commit(p, 180, "alone");
    same_diffs(p, &format!("{main}...{alone}"), None, &[]);
    same_diffs(
        p,
        "0123456789abcdef0123456789abcdef01234567",
        Some(&main),
        &[],
    );
}

/// The caps are the reader's of git's output: writing stops past them, and
/// what is kept is the cut git's output would have had.
#[test]
fn a_capped_diff_is_gits_output_cut_at_the_cap() {
    let dir = new_repo();
    let p = dir.path();
    for i in 0..30 {
        write(p, &format!("f{i:02}.txt"), format!("file {i}\n").repeat(40));
    }
    let first = commit(p, 0, "first");
    for i in 0..30 {
        write(
            p,
            &format!("f{i:02}.txt"),
            format!("file {i}, edited\n").repeat(40),
        );
    }
    let second = commit(p, 60, "second");
    let repo = open(p);
    let (_, whole) = oracle(
        p,
        &[
            "diff",
            "--no-ext-diff",
            "--no-color",
            "-M",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            &first,
            &second,
            "--",
        ],
    );
    for (max_lines, max_bytes) in [
        (100, usize::MAX),
        (usize::MAX, 3000),
        (1, 10),
        (usize::MAX, usize::MAX),
    ] {
        let ours = repo
            .diff(&DiffAsk {
                old: &first,
                new: Some(&second),
                form: DiffForm::Patch,
                ignore_space: false,
                paths: &[],
                max_lines,
                max_bytes,
            })
            .expect("answered");
        let Answer::Out(ours) = ours else {
            panic!("refused")
        };
        assert_eq!(
            knobas_git::capped(&ours, max_lines, max_bytes),
            knobas_git::capped(&whole, max_lines, max_bytes),
            "{max_lines} lines, {max_bytes} bytes"
        );
        if max_lines == 100 {
            assert!(ours.len() < whole.len() / 2, "writing stopped near the cap");
        }
    }
}

// -- the working tree is nothing to these reads ----------------------------------

#[test]
fn a_dirty_tree_an_untracked_file_and_a_detached_head_change_no_answer() {
    let dir = new_repo();
    let p = dir.path();
    write(p, "src/lib.rs", source(10, "v1"));
    write(p, "notes.txt", "notes\n");
    let first = commit(p, 0, "first");
    write(p, "src/lib.rs", source(10, "v2"));
    let second = commit(p, 60, "second");
    let answers = |repo: &Repo| {
        (
            repo.ls_tree(&second),
            repo.rev_parse_commit(&second),
            repo.log(&first, &second, 500, NOW),
            repo.diff(&DiffAsk {
                old: &first,
                new: Some(&second),
                form: DiffForm::Patch,
                ignore_space: false,
                paths: &[],
                max_lines: usize::MAX,
                max_bytes: usize::MAX,
            }),
        )
    };
    let clean = answers(&open(p));
    // A head on no branch, then edited and not staged, staged and not
    // committed, and files git does not track.
    git(p, &["checkout", "-q", "--detach", &first]);
    write(p, "src/lib.rs", "everything rewritten\n");
    write(p, "notes.txt", "staged\n");
    git(p, &["add", "notes.txt"]);
    write(p, "untracked.txt", "untracked\n");
    write(p, "new_folder/also.txt", "untracked\n");
    let before = (
        std::fs::read(p.join(".git/index")).expect("an index"),
        git(p, &["for-each-ref"]),
        std::fs::read(p.join(".git/HEAD")).expect("a head"),
    );
    assert_eq!(answers(&open(p)), clean);
    same_diffs(p, &first, Some(&second), &[]);
    same(
        "ls-tree",
        open(p).ls_tree(&second),
        oracle(
            p,
            &[
                "ls-tree",
                "-r",
                "-t",
                "-z",
                "--long",
                "--full-tree",
                &second,
            ],
        ),
    );
    // And reading wrote nothing: the index, the refs and the head as they were.
    let after = (
        std::fs::read(p.join(".git/index")).expect("an index"),
        git(p, &["for-each-ref"]),
        std::fs::read(p.join(".git/HEAD")).expect("a head"),
    );
    assert_eq!(before, after);
    assert_eq!(
        std::fs::read_to_string(p.join("untracked.txt")).expect("still there"),
        "untracked\n"
    );
}

// -- history ---------------------------------------------------------------------

/// Two branches that merged each other's work both ways, and commits made
/// at one moment: the order is git's, ties and all.
fn history() -> (tempfile::TempDir, Vec<String>) {
    let dir = new_repo();
    let p = dir.path();
    let mut shas = Vec::new();
    write(p, "a.txt", "0\n");
    shas.push(commit(p, 0, "root\n\nwith a body the subject leaves out"));
    git(p, &["checkout", "-q", "-b", "left"]);
    for i in 0..4 {
        write(p, "left.txt", format!("{i}\n"));
        // Every commit of the fixture at one of two moments: ties.
        shas.push(commit(p, 60 * (i % 2), &format!("left {i}")));
    }
    git(p, &["checkout", "-q", "main"]);
    git(p, &["checkout", "-q", "-b", "right"]);
    for i in 0..4 {
        write(p, "right.txt", format!("{i}\n"));
        shas.push(commit(
            p,
            60 * (i % 2),
            &format!("  right {i}  \nsecond line of the subject"),
        ));
    }
    // Each side merges the other's second commit: two merge bases to pick from.
    let (l2, r2) = (shas[2].clone(), shas[6].clone());
    git_at(p, 120, &["merge", "-q", "--no-ff", "--no-edit", &l2]);
    shas.push(git(p, &["rev-parse", "HEAD"]));
    git(p, &["checkout", "-q", "left"]);
    git_at(p, 120, &["merge", "-q", "--no-ff", "--no-edit", &r2]);
    shas.push(git(p, &["rev-parse", "HEAD"]));
    write(p, "left.txt", "after the merge\n");
    shas.push(commit(p, 180, "left, after"));
    git(p, &["checkout", "-q", "right"]);
    write(p, "right.txt", "after the merge\n");
    shas.push(commit(p, 180, "right, after"));
    git(p, &["checkout", "-q", "--orphan", "alone"]);
    shas.push(commit(p, 30, "alone"));
    (dir, shas)
}

#[test]
fn a_merge_base_is_gits_pick() {
    let (dir, shas) = history();
    let p = dir.path();
    let repo = open(p);
    for a in &shas {
        for b in &shas {
            same(
                &format!("merge-base {a} {b}"),
                repo.merge_base(a, b),
                oracle(p, &["merge-base", "--end-of-options", a, b]),
            );
        }
    }
    same(
        "merge-base of a commit not here",
        repo.merge_base(&shas[0], "0123456789abcdef0123456789abcdef01234567"),
        oracle(
            p,
            &[
                "merge-base",
                "--end-of-options",
                &shas[0],
                "0123456789abcdef0123456789abcdef01234567",
            ],
        ),
    );
}

#[test]
fn a_log_lists_what_git_lists_in_its_order() {
    let (dir, shas) = history();
    let p = dir.path();
    let repo = open(p);
    for max in [500usize, 3] {
        for a in &shas {
            for b in &shas {
                let range = format!("{a}..{b}");
                same(
                    &format!("log {range} --max-count={max}"),
                    repo.log(a, b, max, NOW),
                    oracle(
                        p,
                        &[
                            "log",
                            "--no-color",
                            &format!("--max-count={max}"),
                            "--format=%H%x1f%an%x1f%ar%x1f%s%x1e",
                            &range,
                            "--",
                        ],
                    ),
                );
            }
        }
    }
    let Ok(Answer::Out(out)) = repo.log(&shas[0], &shas[10], 500, NOW) else {
        panic!("no log")
    };
    let out = String::from_utf8_lossy(&out).into_owned();
    assert!(out.contains("\x1fMara Okafor\x1f"), "{out}");
    assert!(
        out.contains("months ago\x1f")
            || out.contains("weeks ago\x1f")
            || out.contains("days ago\x1f"),
        "{out}"
    );
    // The first paragraph on one line, its lines' trailing blanks dropped.
    assert!(
        out.contains("right 1 second line of the subject\x1e"),
        "{out}"
    );
}

// -- what only git knows ----------------------------------------------------------

#[test]
fn what_changes_gits_answer_is_left_to_git() {
    let dir = new_repo();
    let p = dir.path();
    write(p, "src/lib.rs", source(10, "v1"));
    let first = commit(p, 0, "first");
    write(p, "src/lib.rs", source(10, "v2"));
    let second = commit(p, 60, "second");
    let ask = |form| DiffAsk {
        old: &first,
        new: Some(&second),
        form,
        ignore_space: false,
        paths: &[],
        max_lines: usize::MAX,
        max_bytes: usize::MAX,
    };
    assert!(open(p).diff(&ask(DiffForm::Patch)).is_ok());
    // Each setting alone, then taken back.
    for (key, value) in [
        ("diff.algorithm", "histogram"),
        ("diff.algorithm", "patience"),
        ("diff.indentHeuristic", "false"),
        ("diff.context", "0"),
        ("diff.orderFile", "order.txt"),
        ("diff.suppressBlankEmpty", "true"),
        ("diff.submodule", "log"),
        ("core.abbrev", "12"),
    ] {
        git(p, &["config", key, value]);
        let form = if key == "core.abbrev" {
            DiffForm::Raw
        } else {
            DiffForm::Patch
        };
        assert!(
            open(p).diff(&ask(form)).is_err(),
            "{key}={value} is git's to read"
        );
        git(p, &["config", "--unset", key]);
    }
    // Settings the writer follows are answered, and as git answers.
    for (key, value) in [
        ("diff.context", "5"),
        ("diff.context", "1"),
        ("diff.interHunkContext", "4"),
        ("diff.algorithm", "myers"),
        ("diff.renameLimit", "1"),
        ("core.abbrev", "auto"),
    ] {
        git(p, &["config", key, value]);
        same_diffs(p, &first, Some(&second), &[]);
        git(p, &["config", "--unset", key]);
    }
    // A named diff driver brings its own function lines.
    write(p, ".gitattributes", "*.rs diff=rust\n");
    assert!(
        open(p).diff(&ask(DiffForm::Patch)).is_err(),
        "a named driver"
    );
    assert!(
        open(p).diff(&ask(DiffForm::Raw)).is_ok(),
        "the names read no content"
    );
    std::fs::remove_file(p.join(".gitattributes")).expect("removed");
    // A short sha that a branch is also called: git reads the branch.
    git(p, &["branch", &second[..8], &first]);
    assert!(open(p).rev_parse_commit(&second[..8]).is_err());
    assert!(open(p).rev_parse_commit(&second).is_ok());
}

#[test]
fn a_shallow_clones_history_is_left_to_git() {
    let dir = new_repo();
    let p = dir.path();
    write(p, "a.txt", "one\n");
    let first = commit(p, 0, "first");
    write(p, "a.txt", "two\n");
    let second = commit(p, 60, "second");
    let shallow = tempfile::tempdir().expect("a folder");
    let url = if cfg!(windows) {
        format!("file:///{}", p.display().to_string().replace('\\', "/"))
    } else {
        format!("file://{}", p.display())
    };
    git(shallow.path(), &["clone", "-q", "--depth", "1", &url, "."]);
    let repo = open(shallow.path());
    same(
        "shallow",
        repo.shallow_and_format(),
        oracle(
            shallow.path(),
            &[
                "rev-parse",
                "--is-shallow-repository",
                "--show-object-format",
            ],
        ),
    );
    assert_eq!(
        repo.shallow_and_format(),
        Ok(Answer::Out(b"true\nsha1\n".to_vec()))
    );
    // What it holds is read; where its history is cut is git's to say.
    assert!(matches!(repo.rev_parse_commit(&second), Ok(Answer::Out(_))));
    assert!(repo.rev_parse_commit(&format!("{second}^")).is_err());
    assert!(repo.log(&second, &second, 10, NOW).is_err());
    assert!(repo.merge_base(&second, &second).is_err());
    assert_eq!(repo.rev_parse_commit(&first), Ok(Answer::Refused));
    // And a folder that is no repository at all.
    let nowhere = tempfile::tempdir().expect("a folder");
    assert!(Repo::open_isolated(nowhere.path()).is_err());
    assert!(Repo::open_isolated(&p.join("no-such-folder")).is_err());
}

// -- many small histories ---------------------------------------------------------

/// A history of edits picked by a seed: lines changed, blocks added, moved
/// and removed, files renamed with and without edits. Every commit's diff
/// against its parent and against the first commit is git's.
#[test]
fn a_seeded_history_of_edits_is_diffed_as_git_diffs_it() {
    let dir = new_repo();
    let p = dir.path();
    let mut seed: u64 = 0x5eed;
    let mut next = |n: usize| {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((seed >> 33) as usize) % n
    };
    let mut files: Vec<(String, Vec<String>)> = (0..4)
        .map(|f| {
            let lines = source(25 + f * 10, &format!("file {f}"))
                .lines()
                .map(str::to_owned)
                .collect();
            (format!("src/mod{f}.rs"), lines)
        })
        .collect();
    let store = |files: &[(String, Vec<String>)]| {
        let _ = std::fs::remove_dir_all(p.join("src"));
        for (path, lines) in files {
            write(
                p,
                path,
                lines.iter().map(|l| format!("{l}\n")).collect::<String>(),
            );
        }
    };
    store(&files);
    let mut shas = vec![commit(p, 0, "first")];
    for round in 0..24 {
        for _ in 0..(1 + next(3)) {
            let f = next(files.len());
            let lines = &mut files[f].1;
            let at = next(lines.len().max(1));
            match next(6) {
                0 => lines[at] = format!("{} // edited in round {round}", lines[at]),
                1 => {
                    let block: Vec<String> = lines[at..(at + 6).min(lines.len())].to_vec();
                    let to = next(lines.len());
                    lines.splice(to..to, block);
                }
                2 => {
                    let end = (at + 1 + next(8)).min(lines.len());
                    lines.drain(at..end);
                }
                3 => {
                    let end = (at + 5).min(lines.len());
                    let block: Vec<String> = lines.drain(at..end).collect();
                    let to = next(lines.len().max(1));
                    lines.splice(to..to, block);
                }
                4 => lines.insert(at, format!("    let added_{round} = {};", next(100))),
                _ => lines[at] = lines[at].replace("    ", "\t"),
            }
            if lines.is_empty() {
                lines.push("fn empty() {}".to_owned());
            }
        }
        if round % 5 == 4 {
            let f = next(files.len());
            files[f].0 = format!("src/moved{round}/mod{f}.rs");
        }
        store(&files);
        shas.push(commit(p, 60 * (round + 1), &format!("round {round}")));
    }
    for at in 1..shas.len() {
        same_diffs(p, &shas[at - 1], Some(&shas[at]), &[]);
    }
    for at in [5, 12, 24] {
        same_diffs(p, &shas[0], Some(&shas[at]), &[]);
        same_diffs(p, &shas[at], Some(&shas[0]), &[]);
    }
}
