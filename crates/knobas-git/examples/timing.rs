//! What each read of a clone costs in-process and as a start of `git`, on
//! this machine (the git-starts pass of PR #2479, 2026-11-16).
//!
//!     cargo run --release -p knobas-git --example timing -- <repository> <old commit> <new commit> [--runs 9]
//!
//! For the two commits: the new one resolved, its whole tree, one file's
//! bytes, the merge base, the log between them, and their diff as the names,
//! the names with counts, the whole patch and one file's patch. Each is
//! timed `--runs` times both ways and the medians printed, with the answer's
//! size and whether the two were the same bytes. Then the working tree's
//! stamp, read by file and by folder, beside the `git status` it stands in
//! for (the last column there is the status's size). On Windows the `git`
//! column is mostly the process start, which is what this pass removed; the
//! first line printed is that start alone (`git --version`).
//!
//! The Windows workflow builds this and `parity` for release and keeps them
//! as the artifact `knobas-git-tools-windows`, for a machine nobody builds
//! on.

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use knobas_git::{Answer, DiffAsk, DiffForm, Repo};

fn git(repo: &Path, args: &[&str]) -> Vec<u8> {
    Command::new("git")
        .arg("-C")
        .arg(repo)
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
        .env("LC_ALL", "C")
        .output()
        .expect("git runs")
        .stdout
}

fn median(mut times: Vec<Duration>) -> Duration {
    times.sort();
    times[times.len() / 2]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("timing <repository> <old commit> <new commit> [--runs 9]");
        std::process::exit(2);
    }
    let path = Path::new(&args[1]);
    let (old, new) = (args[2].as_str(), args[3].as_str());
    let runs: usize = args
        .iter()
        .position(|a| a == "--runs")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(9);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock")
        .as_secs() as i64;
    // A start of git with nothing to read: the floor under every figure of
    // the `git` column, and what each start a press made cost on this
    // machine.
    let alone: Vec<Duration> = (0..runs.max(9))
        .map(|_| {
            let at = Instant::now();
            let _ = Command::new("git").arg("--version").output();
            at.elapsed()
        })
        .collect();
    println!(
        "a start of git alone: {:.2?} (the median of {})",
        median(alone),
        runs.max(9)
    );
    // The first changed file, and the blob it is after the change.
    let raw = git(
        path,
        &["diff", "--raw", "-z", "--no-abbrev", old, new, "--"],
    );
    let text = String::from_utf8_lossy(&raw).into_owned();
    let mut fields = text.split('\0');
    let (meta, one_path) = (
        fields.next().unwrap_or_default().to_owned(),
        fields.next().unwrap_or_default().to_owned(),
    );
    let blob = meta.split(' ').nth(3).unwrap_or_default().to_owned();
    let range = format!("{old}..{new}");
    let ask = |form, paths: &'static [&'static str]| DiffAsk {
        old,
        new: Some(new),
        form,
        ignore_space: false,
        paths,
        max_lines: usize::MAX,
        max_bytes: usize::MAX,
    };
    let one: &'static [&'static str] =
        Box::leak(vec![&*Box::leak(one_path.clone().into_boxed_str())].into_boxed_slice());
    let peeled = format!("{new}^{{commit}}");
    type Ours<'a> = Box<dyn Fn(&Repo) -> Option<Vec<u8>> + 'a>;
    let out = |read: knobas_git::Read| match read {
        Ok(Answer::Out(out)) => Some(out),
        _ => None,
    };
    let cases: Vec<(&str, Vec<&str>, Ours<'_>)> = vec![
        (
            "a commit resolved",
            vec![
                "rev-parse",
                "--verify",
                "--quiet",
                "--end-of-options",
                &peeled,
            ],
            Box::new(|r| out(r.rev_parse_commit(new))),
        ),
        (
            "the whole tree",
            vec!["ls-tree", "-r", "-t", "-z", "--long", "--full-tree", new],
            Box::new(|r| out(r.ls_tree(new))),
        ),
        (
            "one file's bytes",
            vec!["cat-file", "blob", &blob],
            Box::new(|r| r.cat_blob(&blob, usize::MAX).ok().flatten().map(|(b, _)| b)),
        ),
        (
            "the merge base",
            vec!["merge-base", "--end-of-options", old, new],
            Box::new(|r| out(r.merge_base(old, new))),
        ),
        (
            "the log between",
            vec![
                "log",
                "--no-color",
                "--max-count=500",
                "--format=%H%x1f%an%x1f%ar%x1f%s%x1e",
                &range,
                "--",
            ],
            Box::new(|r| out(r.log(old, new, 500, now))),
        ),
        (
            "the names",
            vec![
                "diff",
                "--no-ext-diff",
                "--no-color",
                "--no-textconv",
                "-M",
                "--raw",
                "-z",
                old,
                new,
                "--",
            ],
            Box::new(|r| out(r.diff(&ask(DiffForm::Raw, &[])))),
        ),
        (
            "the names and counts",
            vec![
                "diff",
                "--no-ext-diff",
                "--no-color",
                "--no-textconv",
                "-M",
                "--raw",
                "--numstat",
                "-z",
                old,
                new,
                "--",
            ],
            Box::new(|r| out(r.diff(&ask(DiffForm::RawNumstat, &[])))),
        ),
        (
            "the whole patch",
            vec![
                "diff",
                "--no-ext-diff",
                "--no-color",
                "--no-textconv",
                "-M",
                "--src-prefix=a/",
                "--dst-prefix=b/",
                old,
                new,
                "--",
            ],
            Box::new(|r| out(r.diff(&ask(DiffForm::Patch, &[])))),
        ),
        (
            "one file's patch",
            vec![
                "diff",
                "--no-ext-diff",
                "--no-color",
                "--no-textconv",
                "-M",
                "--src-prefix=a/",
                "--dst-prefix=b/",
                old,
                new,
                "--",
                &one_path,
            ],
            Box::new(|r| out(r.diff(&ask(DiffForm::Patch, one)))),
        ),
    ];
    println!(
        "{:22} {:>12} {:>12} {:>12} {:>10}  same",
        "read", "in-process", "first, cold", "git", "bytes"
    );
    for (name, args, ours) in &cases {
        // Cold: the repository opened anew, as the first read after a pause.
        knobas_git::release_all();
        let cold = Instant::now();
        let repo = Repo::open(path).expect("the repository opens in-process");
        let first = ours(&repo);
        let cold = cold.elapsed();
        let mut inside = Vec::new();
        for _ in 0..runs {
            let at = Instant::now();
            let repo = Repo::open(path).expect("opens");
            let _ = ours(&repo);
            inside.push(at.elapsed());
        }
        let mut outside = Vec::new();
        let mut theirs = Vec::new();
        for _ in 0..runs {
            let at = Instant::now();
            theirs = git(path, args);
            outside.push(at.elapsed());
        }
        println!(
            "{name:22} {:>10.2?} {:>12.2?} {:>12.2?} {:>10}  {}",
            median(inside),
            cold,
            median(outside),
            theirs.len(),
            match &first {
                Some(first) if *first == theirs => "yes",
                Some(_) => "NO",
                None => "left to git",
            }
        );
    }
    // The working tree: the stamp that says nothing changed, read each of
    // its two ways, against the `git status` it stands in for.
    let status = [
        "status",
        "--porcelain=v2",
        "--branch",
        "--untracked-files=normal",
    ];
    let mut outside = Vec::new();
    let mut said = Vec::new();
    for _ in 0..runs {
        let at = Instant::now();
        said = git(path, &status);
        outside.push(at.elapsed());
    }
    let outside = median(outside);
    for (name, by_listing) in [
        ("the stamp, by file", false),
        ("the stamp, by folder", true),
    ] {
        let cold = Instant::now();
        let stamped = knobas_git::stamp::of_read(path, by_listing);
        let cold = cold.elapsed();
        let inside: Vec<Duration> = (0..runs)
            .map(|_| {
                let at = Instant::now();
                let _ = knobas_git::stamp::of_read(path, by_listing);
                at.elapsed()
            })
            .collect();
        println!(
            "{name:22} {:>10.2?} {:>12.2?} {:>12.2?} {:>10}  {}",
            median(inside),
            cold,
            outside,
            said.len(),
            if stamped.is_some() {
                "stamped"
            } else {
                "not stamped"
            }
        );
    }
}
