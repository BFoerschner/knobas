//! The pure half of *Clone* (#823, ADR-0023): the checked name, the URL built
//! from a Gitea source's address, git's argument list, and what git's
//! `--progress` lines and last words mean.
//!
//! `knobas_app::checkout::clone` runs the process, owns the folder and talks to
//! the database; nothing here does any of that.
//!
//! # The rules this module holds (ADR-0023)
//!
//! - **The URL is built, never taken from the mirror.** It is the source's
//!   address, which a person typed, plus a full name that matches
//!   [`FullName::parse`]'s pattern. A mirrored `web_url` never reaches git.
//! - **Only https and ssh**, twice: the address's scheme is checked here, and
//!   git is told `protocol.allow=never` with only [`PROTOCOLS`] allowed, so a
//!   redirect cannot take it anywhere else.
//! - **No shell, and `--` before the URL and the folder**, so neither can be
//!   read as an option.
//! - **No submodules.**

use std::ffi::OsString;
use std::fmt;
use std::path::Path;

/// The protocols git may use for a clone knobas starts.
pub const PROTOCOLS: [&str; 2] = ["https", "ssh"];

/// Why a clone is refused before git is started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloneRefusal {
    /// The full name is not `owner/repo` in the characters ADR-0023 allows.
    BadName(String),
    /// The source's address is not one git may clone from.
    BadAddress(String),
}

impl fmt::Display for CloneRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadName(name) => write!(
                f,
                "'{name}' is not a repository name knobas clones: it must be owner/name, \
                 each made of letters, digits, '.', '_' and '-'"
            ),
            Self::BadAddress(reason) => f.write_str(reason),
        }
    }
}

/// A repository's full name, checked: `owner/repo`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullName {
    owner: String,
    repo: String,
}

impl FullName {
    /// `name`, if it matches `^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$` and neither
    /// half is `.` or `..`.
    ///
    /// The second rule is stricter than ADR-0023's pattern, which lets `.` and
    /// `..` through: the repository half is the folder name, and
    /// `<clones root>/..` is not a new folder under the clones root.
    ///
    /// # Errors
    ///
    /// [`CloneRefusal::BadName`] for anything else.
    pub fn parse(name: &str) -> Result<Self, CloneRefusal> {
        let refuse = || CloneRefusal::BadName(name.to_owned());
        let (owner, repo) = name.split_once('/').ok_or_else(refuse)?;
        let allowed = |part: &str| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        };
        if !allowed(owner) || !allowed(repo) {
            return Err(refuse());
        }
        Ok(Self {
            owner: owner.to_owned(),
            repo: repo.to_owned(),
        })
    }

    #[must_use]
    pub fn owner(&self) -> &str {
        &self.owner
    }

    /// The repository half, which is also the folder the clone goes into.
    #[must_use]
    pub fn repo(&self) -> &str {
        &self.repo
    }
}

/// The URL git clones from: the source's address, then `owner/repo.git`.
///
/// `protocols` is [`PROTOCOLS`] everywhere but a test that clones from a
/// server or a folder the production rule refuses.
///
/// # Errors
///
/// [`CloneRefusal::BadAddress`] for an address with no scheme, a scheme not in
/// `protocols`, no host, or a query or fragment.
pub fn clone_url(
    base_url: &str,
    name: &FullName,
    protocols: &[&str],
) -> Result<String, CloneRefusal> {
    let base = base_url.trim().trim_end_matches('/');
    let (scheme, rest) = base.split_once("://").ok_or_else(|| {
        CloneRefusal::BadAddress(format!(
            "the source's address '{base_url}' has no scheme, so knobas cannot build a URL to clone from"
        ))
    })?;
    let scheme = scheme.to_ascii_lowercase();
    if !protocols.contains(&scheme.as_str()) {
        return Err(CloneRefusal::BadAddress(format!(
            "the source's address '{base_url}' is {scheme}, and knobas clones over {} only",
            protocols.join(" or ")
        )));
    }
    let host = rest.split('/').next().unwrap_or_default();
    if host.is_empty() {
        return Err(CloneRefusal::BadAddress(format!(
            "the source's address '{base_url}' names no server"
        )));
    }
    if rest.contains(['?', '#']) {
        return Err(CloneRefusal::BadAddress(format!(
            "the source's address '{base_url}' carries a query or a fragment, so a repository \
             path cannot be put after it"
        )));
    }
    Ok(format!(
        "{scheme}://{rest}/{}/{}.git",
        name.owner, name.repo
    ))
}

/// git's argument list for cloning `url` into `dir`, the program name left
/// out.
///
/// `protocols` is [`PROTOCOLS`] in production; see [`clone_url`].
#[must_use]
pub fn clone_args(url: &str, dir: &Path, protocols: &[&str]) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["-c".into(), "protocol.allow=never".into()];
    for protocol in protocols {
        args.push("-c".into());
        args.push(format!("protocol.{protocol}.allow=always").into());
    }
    args.extend(
        [
            "-c",
            "submodule.recurse=false",
            "clone",
            "--progress",
            "--no-recurse-submodules",
            "--",
            url,
        ]
        .map(OsString::from),
    );
    args.push(dir.as_os_str().to_owned());
    args
}

/// One `--progress` line, read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    /// git's own name for the step: `Receiving objects`, `Resolving deltas`,
    /// `Updating files`, or the server's `Counting objects` and the like.
    pub phase: String,
    /// The whole clone, 0 to 100: see [`parse_progress`].
    pub percent: u8,
}

/// A `--progress` line as a step and a share of the whole clone, or nothing
/// for a line that is not one.
///
/// git counts each step from 0 to 100 on its own. The button says one number
/// ("Cloning… 52 %"), so the steps are weighted by what takes the time:
/// the server's steps (`remote: ...`) count as 0, receiving the objects is
/// 0–80, resolving deltas 80–95, and writing the files out 95–100.
#[must_use]
pub fn parse_progress(line: &str) -> Option<Progress> {
    let line = line.trim();
    let (remote, line) = match line.strip_prefix("remote:") {
        Some(rest) => (true, rest.trim_start()),
        None => (false, line),
    };
    let (phase, rest) = line.split_once(':')?;
    let rest = rest.trim_start();
    let digits = rest.split_once('%')?.0.trim();
    let step: u32 = digits.parse().ok().filter(|p| *p <= 100)?;
    let phase = phase.trim();
    let (from, span) = match (remote, phase) {
        (true, _) => (0, 0),
        (false, "Receiving objects") => (0, 80),
        (false, "Resolving deltas") => (80, 15),
        (false, "Updating files") => (95, 5),
        (false, _) => return None,
    };
    let percent = u8::try_from(from + step * span / 100).unwrap_or(100);
    Some(Progress {
        phase: phase.to_owned(),
        percent,
    })
}

/// The line of a failed clone's stderr to show: git's first `fatal:` line,
/// else its last `error:` line, else its last line.
///
/// Not simply the last line: a repository the server does not have ends with
/// "Please make sure you have the correct access rights / and the repository
/// exists.", which is advice, and the line that says what happened is the
/// first `fatal:` above it.
#[must_use]
pub fn failure_reason(stderr: &str) -> Option<&str> {
    let lines = || {
        stderr
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
    };
    lines()
        .find(|line| line.starts_with("fatal:"))
        .or_else(|| lines().rfind(|line| line.starts_with("error:")))
        .or_else(|| lines().next_back())
}

/// Whether the clone at `dir` finished: its `HEAD` names a commit.
///
/// A clone that was killed (knobas quit while it ran) keeps the `.git/config`
/// with its `origin` that git writes first, so it would match by remote and
/// be adopted. What it lacks is the branch ref git writes at the end, so
/// `HEAD` names a ref that is not there. Read from files, like the scan: a
/// detached `HEAD` is a commit; a ref is a loose file or a `packed-refs` line;
/// a repository on the reftable format cannot be read this way and counts as
/// finished.
#[must_use]
pub fn is_finished(dir: &Path) -> bool {
    let git = dir.join(".git");
    let Ok(head) = std::fs::read_to_string(git.join("HEAD")) else {
        return false;
    };
    if git.join("reftable").is_dir() {
        return true;
    }
    let Some(name) = head.trim().strip_prefix("ref:").map(str::trim) else {
        return !head.trim().is_empty();
    };
    if name.contains("..") {
        return false;
    }
    git.join(name).is_file()
        || std::fs::read_to_string(git.join("packed-refs")).is_ok_and(|packed| {
            packed
                .lines()
                .any(|line| line.split_once(' ').is_some_and(|(_, r)| r == name))
        })
}

/// What a failed clone's stderr says went wrong, for the sentence and the
/// action the row shows (R-8.25).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureCause {
    /// git could not sign in: the person's ssh agent or credential helper.
    Auth,
    /// The disk is full.
    DiskFull,
    /// The network, the server, or anything else: git's last line says.
    Other,
}

/// Which [`FailureCause`] git's stderr names. Matched in English, so the
/// process runs with `LC_ALL=C`.
#[must_use]
pub fn failure_cause(stderr: &str) -> FailureCause {
    const AUTH: [&str; 6] = [
        "Authentication failed",
        "could not read Username",
        "could not read Password",
        "terminal prompts disabled",
        "Permission denied (publickey",
        "HTTP Basic: Access denied",
    ];
    if stderr.contains("No space left on device") {
        FailureCause::DiskFull
    } else if AUTH.iter().any(|needle| stderr.contains(needle)) {
        FailureCause::Auth
    } else {
        FailureCause::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(s: &str) -> FullName {
        FullName::parse(s).unwrap()
    }

    #[test]
    fn a_full_name_is_owner_slash_repo_in_the_allowed_characters() {
        let ok = name("tidewater/payout-service_2.0");
        assert_eq!(ok.owner(), "tidewater");
        assert_eq!(ok.repo(), "payout-service_2.0");
        for bad in [
            "",
            "payout-service",
            "/payout-service",
            "tidewater/",
            "a/b/c",
            "tide water/x",
            "tidewater/x;rm",
            "tidewater/$(x)",
            "tidewater/x\n",
            "tidewater/é",
            "../x",
            "tidewater/..",
            "tidewater/.",
            "--upload-pack=x/y z",
        ] {
            assert_eq!(
                FullName::parse(bad),
                Err(CloneRefusal::BadName(bad.to_owned())),
                "{bad:?} was accepted"
            );
        }
        // A leading dash is a legal name; `--` is what keeps it from being an
        // option.
        assert!(FullName::parse("-o/-x").is_ok());
    }

    #[test]
    fn the_url_is_the_address_and_the_full_name() {
        let n = name("tidewater/payout-service");
        assert_eq!(
            clone_url("https://git.example.com/", &n, &PROTOCOLS).unwrap(),
            "https://git.example.com/tidewater/payout-service.git"
        );
        assert_eq!(
            clone_url(" HTTPS://git.example.com/gitea ", &n, &PROTOCOLS).unwrap(),
            "https://git.example.com/gitea/tidewater/payout-service.git"
        );
        assert_eq!(
            clone_url("ssh://git@git.example.com:2222", &n, &PROTOCOLS).unwrap(),
            "ssh://git@git.example.com:2222/tidewater/payout-service.git"
        );
    }

    #[test]
    fn an_address_outside_https_and_ssh_is_refused() {
        let n = name("tidewater/payout-service");
        for bad in [
            "http://127.0.0.1:3000",
            "file:///tmp/x",
            "ext::sh -c touch% /tmp/pwned",
            "git://git.example.com",
            "git.example.com",
            "https://",
            "https://git.example.com/?a=b",
            "https://git.example.com/#x",
        ] {
            assert!(
                matches!(
                    clone_url(bad, &n, &PROTOCOLS),
                    Err(CloneRefusal::BadAddress(_))
                ),
                "{bad:?} was accepted"
            );
        }
        // And what a test admits on purpose is admitted.
        assert!(clone_url("http://127.0.0.1:3000", &n, &["http"]).is_ok());
    }

    /// The whole list, spelled out: the flags are ADR-0023's rules, and a
    /// reordering that put the URL before `--` would be the hole.
    #[test]
    fn the_argument_list_is_the_adrs() {
        let args = clone_args(
            "https://git.example.com/tidewater/payout-service.git",
            Path::new("/home/me/code/payout-service"),
            &PROTOCOLS,
        );
        let args: Vec<&str> = args.iter().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(
            args,
            [
                "-c",
                "protocol.allow=never",
                "-c",
                "protocol.https.allow=always",
                "-c",
                "protocol.ssh.allow=always",
                "-c",
                "submodule.recurse=false",
                "clone",
                "--progress",
                "--no-recurse-submodules",
                "--",
                "https://git.example.com/tidewater/payout-service.git",
                "/home/me/code/payout-service",
            ]
        );
    }

    #[test]
    fn the_production_protocols_are_https_and_ssh() {
        assert_eq!(PROTOCOLS, ["https", "ssh"]);
    }

    #[test]
    fn progress_lines_become_one_share_of_the_whole_clone() {
        let read = |line: &str| parse_progress(line).map(|p| (p.phase, p.percent));
        assert_eq!(
            read("remote: Counting objects:  44% (4/9)        "),
            Some(("Counting objects".to_owned(), 0))
        );
        assert_eq!(
            read("Receiving objects:  50% (520/1040), 1.20 MiB | 3.00 MiB/s"),
            Some(("Receiving objects".to_owned(), 40))
        );
        assert_eq!(
            read("Receiving objects: 100% (9/9), done."),
            Some(("Receiving objects".to_owned(), 80))
        );
        assert_eq!(
            read("Resolving deltas: 100% (1/1)"),
            Some(("Resolving deltas".to_owned(), 95))
        );
        assert_eq!(
            read("Updating files:  60% (3/5)"),
            Some(("Updating files".to_owned(), 98))
        );
        for not in [
            "Cloning into 'x'...",
            "fatal: repository not found",
            "Receiving objects: lots",
            "Receiving objects: 400% (1/1)",
            "Something else: 10% (1/10)",
        ] {
            assert_eq!(parse_progress(not), None, "{not:?}");
        }
    }

    #[test]
    fn a_clone_is_finished_when_its_head_names_a_commit() {
        let dir = tempfile::tempdir().unwrap();
        let git = dir.path().join(".git");
        std::fs::create_dir_all(git.join("refs/heads")).unwrap();
        assert!(!is_finished(dir.path()), "no HEAD");
        std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        assert!(
            !is_finished(dir.path()),
            "HEAD names a ref that is not there"
        );
        std::fs::write(git.join("packed-refs"), "abc refs/heads/mainline\n").unwrap();
        assert!(!is_finished(dir.path()), "a ref that only starts the same");
        std::fs::write(
            git.join("packed-refs"),
            "# pack-refs\nabc refs/heads/main\n",
        )
        .unwrap();
        assert!(is_finished(dir.path()), "a packed ref");
        std::fs::remove_file(git.join("packed-refs")).unwrap();
        std::fs::write(git.join("refs/heads/main"), "abc\n").unwrap();
        assert!(is_finished(dir.path()), "a loose ref");
        std::fs::write(
            git.join("HEAD"),
            "8a3aaa77d487179f54af3ab02f51f196e4e91373\n",
        )
        .unwrap();
        assert!(is_finished(dir.path()), "a detached HEAD");
    }

    #[test]
    fn a_failure_is_said_with_gits_own_line() {
        assert_eq!(
            failure_reason(
                "Cloning into 'x'...\nfatal: '/srv/x.git' does not appear to be a git repository\n\
                 fatal: Could not read from remote repository.\n\nPlease make sure you have the \
                 correct access rights\nand the repository exists.\n"
            ),
            Some("fatal: '/srv/x.git' does not appear to be a git repository")
        );
        assert_eq!(
            failure_reason("Cloning into 'x'...\nerror: one\nerror: two\nwarning: three\n"),
            Some("error: two")
        );
        assert_eq!(
            failure_reason("Cloning into 'x'...\nKilled\n"),
            Some("Killed")
        );
        assert_eq!(failure_reason("\n  \n"), None);
    }

    #[test]
    fn a_failure_names_its_cause() {
        assert_eq!(
            failure_cause(
                "Cloning into 'x'...\nfatal: could not read Username for 'https://git.example.com': \
                 terminal prompts disabled\n"
            ),
            FailureCause::Auth
        );
        assert_eq!(
            failure_cause("git@git.example.com: Permission denied (publickey).\n"),
            FailureCause::Auth
        );
        assert_eq!(
            failure_cause("error: unable to write file x: No space left on device\n"),
            FailureCause::DiskFull
        );
        assert_eq!(
            failure_cause("fatal: unable to access 'https://x/': Could not resolve host: x\n"),
            FailureCause::Other
        );
    }
}
