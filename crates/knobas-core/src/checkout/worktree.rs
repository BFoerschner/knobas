//! The pure half of a pull request's worktree (#2285, ADR-0035) and of the
//! *Diff tool* target (#2275): where the worktree goes, and every git argument
//! list knobas runs for either.
//!
//! `knobas_app::checkout::worktree` runs the processes, keeps the list of
//! worktrees and talks to the database; nothing here does any of that.
//!
//! # The git commands, all of them (ADR-0035)
//!
//! | step | runs at | command |
//! |---|---|---|
//! | [`Step::Add`] | the person's checkout | `worktree add --detach --no-checkout` |
//! | [`Step::Fetch`] | the worktree | `fetch` of `refs/pull/<n>/head` |
//! | [`Step::Move`] | the worktree | `checkout --detach FETCH_HEAD` |
//! | [`Step::Remove`] | the worktree | `worktree remove` |
//! | [`Step::Discard`] | the worktree | `worktree remove --force`, only of a worktree this open made and never filled |
//!
//! The person's checkout is `-C` for [`Step::Add`] and nothing else: its
//! branch, its `HEAD`, its index and its files are never an argument to a
//! writing command. Every path comes after `--`. The fetch's URL is built as
//! *Clone* builds one (ADR-0023), and its refspec holds one number.
//!
//! [`difftool_args`] is the *Diff tool*'s one command, read-only: two shas
//! that are already in the checkout.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::clone::FullName;
use super::open::is_sha;

/// The folder under the clones root that holds every pull request worktree.
pub const FOLDER: &str = "pull-requests";

/// Where pull request `number` of `name` is checked out:
/// `<clones root>/pull-requests/<owner>/<repo>-<number>`.
///
/// Three levels down, so the clones-root scan (two levels, and a `.git` file
/// is never a clone to it) never takes a worktree for the repository's
/// checkout.
#[must_use]
pub fn folder(root: &Path, name: &FullName, number: u64) -> PathBuf {
    root.join(FOLDER)
        .join(name.owner())
        .join(format!("{}-{number}", name.repo()))
}

/// The ref a code host serves a pull request's head under. Gitea, Forgejo
/// and GitHub all serve `refs/pull/<n>/head`.
#[must_use]
pub fn pull_ref(number: u64) -> String {
    format!("refs/pull/{number}/head")
}

/// One git command of a pull request's worktree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step<'a> {
    /// Make the worktree, detached at the checkout's `HEAD` and with no files
    /// yet: `again` when knobas made one at this folder before and it is gone
    /// from the disk, which git still has registered.
    Add {
        checkout: &'a Path,
        worktree: &'a Path,
        again: bool,
    },
    /// Fetch the pull request's head into the worktree's own `FETCH_HEAD`.
    Fetch {
        worktree: &'a Path,
        url: &'a str,
        number: u64,
        protocols: &'a [&'a str],
    },
    /// Move the worktree to what was fetched. git refuses when a change in
    /// the worktree is in the way, and that refusal stands.
    Move { worktree: &'a Path },
    /// Remove the worktree; git refuses one with changes or untracked files.
    Remove { worktree: &'a Path },
    /// Remove a worktree this open made and could not fill: its index is
    /// empty, so git counts every file as a change.
    Discard { worktree: &'a Path },
}

impl Step<'_> {
    /// git's argument list for this step, the program name left out.
    #[must_use]
    pub fn args(&self) -> Vec<OsString> {
        let os = |words: &[&str]| words.iter().map(OsString::from).collect::<Vec<_>>();
        match *self {
            Self::Add {
                checkout,
                worktree,
                again,
            } => {
                let mut args = vec!["-C".into(), checkout.as_os_str().to_owned()];
                args.extend(os(&["worktree", "add", "--detach", "--no-checkout"]));
                if again {
                    // Only past git's "missing but already registered": the
                    // worktree is detached, so no branch is in question.
                    args.push("--force".into());
                }
                args.push("--".into());
                args.push(worktree.as_os_str().to_owned());
                args
            }
            Self::Fetch {
                worktree,
                url,
                number,
                protocols,
            } => {
                let mut args: Vec<OsString> = vec!["-c".into(), "protocol.allow=never".into()];
                for protocol in protocols {
                    args.push("-c".into());
                    args.push(format!("protocol.{protocol}.allow=always").into());
                }
                args.extend(os(&["-c", "submodule.recurse=false", "-C"]));
                args.push(worktree.as_os_str().to_owned());
                args.extend(os(&[
                    "fetch",
                    "--no-tags",
                    "--no-recurse-submodules",
                    "--no-auto-maintenance",
                    "--quiet",
                    "--",
                    url,
                ]));
                args.push(pull_ref(number).into());
                args
            }
            Self::Move { worktree } => {
                let mut args = os(&["-c", "submodule.recurse=false", "-C"]);
                args.push(worktree.as_os_str().to_owned());
                args.extend(os(&["checkout", "--detach", "--quiet", "FETCH_HEAD", "--"]));
                args
            }
            Self::Remove { worktree } | Self::Discard { worktree } => {
                let mut args = vec!["-C".into(), worktree.as_os_str().to_owned()];
                args.extend(os(&["worktree", "remove"]));
                if matches!(self, Self::Discard { .. }) {
                    args.push("--force".into());
                }
                args.push("--".into());
                args.push(worktree.as_os_str().to_owned());
                args
            }
        }
    }
}

/// Whether `end` is a sha the *Diff tool* passes: 7 to 64 hex digits, and a
/// base may end in one `^` (the diff address's `<sha>^`).
#[must_use]
pub fn is_diff_end(end: &str, caret: bool) -> bool {
    let bare = if caret {
        end.strip_suffix('^').unwrap_or(end)
    } else {
        end
    };
    is_sha(bare)
}

/// The *Diff tool*'s argument list at `checkout` (#2275), the program left
/// out: `git difftool --dir-diff --gui --no-prompt <base> <head> --`.
///
/// Shas only, never a branch name: a local branch of the same name may stand
/// somewhere else than the source's, and a sha is never an option. `--gui`
/// takes `diff.guitool`, and git falls back to `diff.tool` when that is unset,
/// so the tool is the one the person configured, an editor's diff mode
/// included. `None` for an end that is not a sha.
#[must_use]
pub fn difftool_args(checkout: &Path, base: &str, head: &str) -> Option<Vec<OsString>> {
    if !is_diff_end(base, true) || !is_diff_end(head, false) {
        return None;
    }
    let mut args = vec!["-C".into(), checkout.as_os_str().to_owned()];
    args.extend(
        [
            "difftool",
            "--dir-diff",
            "--gui",
            "--no-prompt",
            base,
            head,
            "--",
        ]
        .map(OsString::from),
    );
    Some(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[OsString]) -> Vec<String> {
        args.iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    fn name() -> FullName {
        FullName::parse("tidewater/payout-service").unwrap()
    }

    #[test]
    fn a_worktree_is_three_levels_under_the_root_and_named_by_its_pull_request() {
        assert_eq!(
            folder(Path::new("/code"), &name(), 7),
            Path::new("/code/pull-requests/tidewater/payout-service-7")
        );
        assert_eq!(pull_ref(7), "refs/pull/7/head");
    }

    /// Each step's whole argument list, spelled out: the ADR's table.
    #[test]
    fn each_step_is_the_command_the_adr_lists() {
        let (checkout, wt) = (Path::new("/code/payout-service"), Path::new("/code/pr/x-7"));
        let protocols = ["https", "ssh"];
        let cases = [
            (
                Step::Add {
                    checkout,
                    worktree: wt,
                    again: false,
                },
                "-C /code/payout-service worktree add --detach --no-checkout -- /code/pr/x-7",
            ),
            (
                Step::Add {
                    checkout,
                    worktree: wt,
                    again: true,
                },
                "-C /code/payout-service worktree add --detach --no-checkout --force -- /code/pr/x-7",
            ),
            (
                Step::Fetch {
                    worktree: wt,
                    url: "https://git.example.com/tidewater/payout-service.git",
                    number: 7,
                    protocols: &protocols,
                },
                "-c protocol.allow=never -c protocol.https.allow=always -c protocol.ssh.allow=always \
                 -c submodule.recurse=false -C /code/pr/x-7 fetch --no-tags --no-recurse-submodules \
                 --no-auto-maintenance --quiet -- https://git.example.com/tidewater/payout-service.git \
                 refs/pull/7/head",
            ),
            (
                Step::Move { worktree: wt },
                "-c submodule.recurse=false -C /code/pr/x-7 checkout --detach --quiet FETCH_HEAD --",
            ),
            (
                Step::Remove { worktree: wt },
                "-C /code/pr/x-7 worktree remove -- /code/pr/x-7",
            ),
            (
                Step::Discard { worktree: wt },
                "-C /code/pr/x-7 worktree remove --force -- /code/pr/x-7",
            ),
        ];
        for (step, expected) in cases {
            assert_eq!(strings(&step.args()).join(" "), expected, "{step:?}");
        }
    }

    /// The dispatch's check: every writing command is a fetch, a worktree
    /// command, or the worktree's own move; none pushes; and the person's
    /// checkout is `-C` only to add a worktree.
    #[test]
    fn no_step_pushes_and_only_add_runs_at_the_persons_checkout() {
        let (checkout, wt) = (Path::new("/c/mine"), Path::new("/c/pull-requests/o/r-1"));
        let protocols = ["https"];
        let steps = [
            Step::Add {
                checkout,
                worktree: wt,
                again: true,
            },
            Step::Fetch {
                worktree: wt,
                url: "https://h/o/r.git",
                number: 1,
                protocols: &protocols,
            },
            Step::Move { worktree: wt },
            Step::Remove { worktree: wt },
            Step::Discard { worktree: wt },
        ];
        for step in steps {
            let args = strings(&step.args());
            let at = args.iter().position(|a| a == "-C").unwrap();
            let dir = &args[at + 1];
            let command = &args[at + 2];
            assert!(
                ["worktree", "fetch", "checkout"].contains(&command.as_str()),
                "{step:?}"
            );
            assert!(!args.iter().any(|a| a == "push"), "{step:?}");
            if dir == "/c/mine" {
                assert_eq!(args[at + 2..at + 4], ["worktree", "add"], "{step:?}");
            } else {
                assert_eq!(dir, "/c/pull-requests/o/r-1", "{step:?}");
            }
        }
    }

    #[test]
    fn the_diff_tool_takes_two_shas_and_a_base_may_be_a_parent() {
        let at = Path::new("/code/payout-service");
        assert_eq!(
            strings(&difftool_args(at, "7be0e4f15c60^", "c90d11e4a7b2").unwrap()).join(" "),
            "-C /code/payout-service difftool --dir-diff --gui --no-prompt 7be0e4f15c60^ c90d11e4a7b2 --"
        );
        for (base, head) in [
            ("main", "c90d11e4a7b2"),
            ("7be0e4f15c60", "feature/x"),
            ("--output=/tmp/x", "c90d11e4a7b2"),
            ("7be0e4f15c60", "c90d11e4a7b2^"),
            ("abc", "c90d11e4a7b2"),
        ] {
            assert_eq!(difftool_args(at, base, head), None, "{base} {head}");
        }
    }
}
