//! Every read of `knobas-git` against the `git` on `PATH`, over a real
//! repository's history: each answer must be git's standard output byte for
//! byte (the git-starts pass of PR #2479, 2026-11-16).
//!
//!     cargo run --release -p knobas-git --example parity -- <repository> [--commits 2000] [--pairs 300] [--seed 1]
//!
//! For each of the newest `--commits` commits: the commit resolved, its
//! parent, and its change against the first parent as the three forms of
//! `git diff` (`--raw`, `--raw --numstat`, the patch), each also with `-w`;
//! for every 50th its whole tree and a few files' bytes. For `--pairs`
//! pairs of commits picked by `--seed`: the merge base, `git log` between
//! them, and their diff (two-dot and three-dot), where renames and big
//! changes are. It prints each difference and a count, and exits 1 on any.
//!
//! An answer of "ask git" is counted apart: it is no difference (git is
//! then asked, as before), and the reasons are listed.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use knobas_git::{Answer, DiffAsk, DiffForm, Repo};

fn git(repo: &Path, now: i64, args: &[&str]) -> (bool, Vec<u8>) {
    let out = Command::new("git")
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
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C")
        .env("GIT_TEST_DATE_NOW", now.to_string())
        .env_remove("GIT_EXTERNAL_DIFF")
        .output()
        .expect("git runs");
    (out.status.success(), out.stdout)
}

struct Tally {
    same: u64,
    asked: BTreeMap<String, u64>,
    different: u64,
}

impl Tally {
    fn check(&mut self, what: &str, ours: knobas_git::Read, theirs: (bool, Vec<u8>)) {
        match ours {
            Err(why) => *self.asked.entry(why.0).or_default() += 1,
            Ok(Answer::Refused) if !theirs.0 => self.same += 1,
            Ok(Answer::Out(out)) if theirs.0 && out == theirs.1 => self.same += 1,
            Ok(answer) => {
                self.different += 1;
                if self.different <= 20 {
                    let ours = match &answer {
                        Answer::Refused => b"<refused>".to_vec(),
                        Answer::Out(out) => out.clone(),
                    };
                    let at = ours
                        .iter()
                        .zip(&theirs.1)
                        .position(|(a, b)| a != b)
                        .unwrap_or(ours.len().min(theirs.1.len()));
                    let from = at.saturating_sub(200);
                    eprintln!(
                        "DIFFERENT {what} (git ok {}, {} bytes; ours {} bytes; first difference at {at})\n--- git\n{}\n--- ours\n{}\n",
                        theirs.0,
                        theirs.1.len(),
                        ours.len(),
                        String::from_utf8_lossy(&theirs.1[from..(at + 300).min(theirs.1.len())]),
                        String::from_utf8_lossy(&ours[from..(at + 300).min(ours.len())]),
                    );
                }
            }
        }
    }
}

const DIFF: [&str; 8] = [
    "-c",
    "diff.autoRefreshIndex=false",
    "diff",
    "--no-ext-diff",
    "--no-color",
    "--no-textconv",
    "-M",
    "--end-of-options",
];

fn diffs(
    tally: &mut Tally,
    path: &Path,
    repo: &Repo,
    now: i64,
    what: &str,
    old: &str,
    new: Option<&str>,
) {
    for ignore_space in [false, true] {
        for form in [DiffForm::Raw, DiffForm::RawNumstat, DiffForm::Patch] {
            let mut args: Vec<&str> = DIFF[..7].to_vec();
            match form {
                DiffForm::Raw => args.extend(["--raw", "-z"]),
                DiffForm::RawNumstat => args.extend(["--raw", "--numstat", "-z"]),
                DiffForm::Patch => args.extend(["--src-prefix=a/", "--dst-prefix=b/"]),
            }
            if ignore_space {
                args.push("-w");
            }
            args.push(old);
            if let Some(new) = new {
                args.push(new);
            }
            args.push("--");
            let ask = DiffAsk {
                old,
                new,
                form,
                ignore_space,
                paths: &[],
                max_lines: usize::MAX,
                max_bytes: usize::MAX,
            };
            tally.check(
                &format!("{what} {form:?}{}", if ignore_space { " -w" } else { "" }),
                repo.diff(&ask),
                git(path, now, &args),
            );
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let opt = |name: &str, default: u64| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let path = Path::new(&args[1]);
    let (commits, pairs, mut seed) = (
        opt("--commits", 2000),
        opt("--pairs", 300),
        opt("--seed", 1),
    );
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock")
        .as_secs() as i64;
    let repo = Repo::open(path).expect("the repository opens in-process");
    let (_, listed) = git(
        path,
        now,
        &["rev-list", &format!("--max-count={commits}"), "HEAD"],
    );
    let shas: Vec<String> = String::from_utf8_lossy(&listed)
        .lines()
        .map(str::to_owned)
        .collect();
    let mut tally = Tally {
        same: 0,
        asked: BTreeMap::new(),
        different: 0,
    };
    let started = std::time::Instant::now();
    for (at, sha) in shas.iter().enumerate() {
        let peeled = format!("{sha}^{{commit}}");
        let parent = format!("{sha}^");
        tally.check(
            "rev-parse",
            repo.rev_parse_commit(sha),
            git(
                path,
                now,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    "--end-of-options",
                    &peeled,
                ],
            ),
        );
        let has_parent = git(
            path,
            now,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                "--end-of-options",
                &format!("{parent}^{{commit}}"),
            ],
        );
        tally.check(
            "rev-parse ^",
            repo.rev_parse_commit(&parent),
            has_parent.clone(),
        );
        tally.check(
            "rev-parse short",
            repo.rev_parse_commit(&sha[..9]),
            git(
                path,
                now,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    "--end-of-options",
                    &format!("{}^{{commit}}", &sha[..9]),
                ],
            ),
        );
        if has_parent.0 {
            let old = String::from_utf8_lossy(&has_parent.1).trim().to_owned();
            diffs(
                &mut tally,
                path,
                &repo,
                now,
                &format!("commit {sha}"),
                &old,
                Some(sha),
            );
        } else {
            diffs(
                &mut tally,
                path,
                &repo,
                now,
                &format!("root {sha}"),
                "4b825dc642cb6eb9a060e54bf8d69288fbee4904",
                Some(sha),
            );
        }
        if at % 50 == 0 {
            let theirs = git(
                path,
                now,
                &["ls-tree", "-r", "-t", "-z", "--long", "--full-tree", sha],
            );
            // A few of the tree's files, by the blob the listing names.
            let blobs: Vec<String> = theirs
                .1
                .split(|b| *b == 0)
                .filter_map(|record| {
                    let text = String::from_utf8_lossy(record);
                    let mut fields = text.split_whitespace();
                    (fields.next()?, fields.next()? == "blob")
                        .1
                        .then(|| fields.next().map(str::to_owned))
                        .flatten()
                })
                .step_by(37)
                .take(12)
                .collect();
            tally.check("ls-tree", repo.ls_tree(sha), theirs);
            for blob in blobs {
                for max in [usize::MAX, 1024] {
                    let theirs = git(path, now, &["cat-file", "blob", &blob]);
                    let ours = repo.cat_blob(&blob, max).map(|found| match found {
                        Some((bytes, _)) => Answer::Out(bytes),
                        None => Answer::Refused,
                    });
                    let (kept, _) = knobas_git::capped(&theirs.1, usize::MAX, max);
                    tally.check("cat-file", ours, (theirs.0, kept));
                }
            }
        }
        if at % 500 == 499 {
            eprintln!(
                "{} commits, {} answers the same, {} different, {:.0?}",
                at + 1,
                tally.same,
                tally.different,
                started.elapsed()
            );
        }
    }
    let mut next = || {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (seed >> 33) as usize
    };
    for _ in 0..pairs {
        if shas.len() < 2 {
            break;
        }
        // One near pair and one far pair, alternately.
        let a = next() % shas.len();
        let b = if next() % 2 == 0 {
            (a + 1 + next() % 40).min(shas.len() - 1)
        } else {
            next() % shas.len()
        };
        let (x, y) = (shas[a].as_str(), shas[b].as_str());
        tally.check(
            "merge-base",
            repo.merge_base(x, y),
            git(path, now, &["merge-base", "--end-of-options", x, y]),
        );
        for (base, head) in [(x, y), (y, x)] {
            let range = format!("{base}..{head}");
            tally.check(
                &format!("log {range}"),
                repo.log(base, head, 500, now),
                git(
                    path,
                    now,
                    &[
                        "log",
                        "--no-color",
                        "--max-count=500",
                        "--format=%H%x1f%an%x1f%ar%x1f%s%x1e",
                        &range,
                        "--",
                    ],
                ),
            );
        }
        diffs(
            &mut tally,
            path,
            &repo,
            now,
            &format!("range {x} {y}"),
            x,
            Some(y),
        );
        let three = format!("{x}^{{commit}}...{y}^{{commit}}");
        diffs(
            &mut tally,
            path,
            &repo,
            now,
            &format!("range {three}"),
            &three,
            None,
        );
    }
    println!(
        "{} answers the same as git, {} different, {} left to git, in {:.0?}",
        tally.same,
        tally.different,
        tally.asked.values().sum::<u64>(),
        started.elapsed()
    );
    for (why, count) in &tally.asked {
        println!("  left to git {count} times: {why}");
    }
    if tally.different > 0 {
        std::process::exit(1);
    }
}
