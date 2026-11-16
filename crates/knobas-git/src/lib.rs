//! A clone read in knobas's own process (the git-starts pass of PR #2479,
//! 2026-11-16, `docs/decisions/2026-11-16-a-clone-is-read-in-process.md`).
//!
//! knobas used to start `git` for every read of a cloned repository: a
//! folder, a file, a commit's diff, a compare. A process start is a few
//! milliseconds on a Mac and tens of milliseconds on Windows, so a page that
//! started git four times in a row missed its line there. This crate answers
//! the same questions from the repository's files, with no process.
//!
//! # The contract: git's own bytes, or "ask git"
//!
//! Each read names one `git` command line and answers **the bytes that
//! command prints**, so the caller parses them with the parser it already
//! had, and a test compares the two byte for byte:
//!
//! | Read | The command it answers |
//! |---|---|
//! | [`Repo::rev_parse_commit`] | `git rev-parse --verify --quiet <name>^{commit}` |
//! | [`Repo::shallow_and_format`] | `git rev-parse --is-shallow-repository --show-object-format` |
//! | [`Repo::merge_base`] | `git merge-base <a> <b>` |
//! | [`Repo::ls_tree`] | `git ls-tree -r -t -z --long --full-tree <commit>` |
//! | [`Repo::cat_blob`] | `git cat-file blob <blob>` |
//! | [`Repo::log`] | `git log --max-count=<n> --format=%H%x1f%an%x1f%ar%x1f%s%x1e <base>..<head>` |
//! | [`Repo::diff`] | `git diff --no-ext-diff --no-color --no-textconv -M [--raw [--numstat] -z] [-w] <old> <new> [-- <path>...]` |
//!
//! An answer is [`Answer::Out`] (what git prints) or [`Answer::Refused`]
//! (git would exit non-zero: the clone holds no such commit). Anything this
//! reader does not reproduce exactly is [`AskGit`]: a repository it cannot
//! open, a shallow clone's history, a diff setting or an attribute that
//! changes what `git diff` prints. The caller then runs git, as before, so
//! git stays the reference and the slow path is the old path.
//!
//! # What it never does
//!
//! It never writes: no index refresh, no lock, no ref. It never reaches the
//! network: an object a partial clone lacks is not fetched. It follows no
//! replace ref. It opens the repository at the path it is given and no
//! higher. It starts no process (`gix` is told not to ask the git binary for
//! its installation's configuration). These are ADR-0021's and ADR-0036's
//! terms, kept by construction.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

mod abbrev;
mod diff;
mod lines;
mod quote;
mod read;
mod renames;
pub mod stamp;
mod walk;

pub use diff::{DiffAsk, DiffForm};

/// This reader gives no answer here, and why: git is asked, as before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskGit(pub String);

impl AskGit {
    pub(crate) fn because(why: impl Into<String>) -> Self {
        Self(why.into())
    }
}

impl std::fmt::Display for AskGit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// What the git command would have answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// Its standard output, whole.
    Out(Vec<u8>),
    /// It would have exited non-zero: no such commit here, no merge base.
    Refused,
}

/// A read's result: git's answer, or [`AskGit`].
pub type Read = Result<Answer, AskGit>;

/// How long an opened repository is kept after its last read. Short on
/// purpose: an open repository holds its pack files mapped, and on Windows a
/// mapped pack cannot be deleted, so a `git gc` in a terminal would leave the
/// old packs behind while knobas held them.
pub const HELD_FOR: Duration = Duration::from_secs(20);

/// The most repositories kept open at once.
const HELD: usize = 8;

type Held = HashMap<PathBuf, (gix::ThreadSafeRepository, bool, Instant)>;

fn held() -> std::sync::MutexGuard<'static, Held> {
    static HELD_ONES: OnceLock<Mutex<Held>> = OnceLock::new();
    HELD_ONES
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// Let go of every repository not read for [`HELD_FOR`]: its packs are
/// unmapped and its files closed.
pub fn release_idle() {
    held().retain(|_, (_, _, at)| at.elapsed() < HELD_FOR);
}

/// One thread that lets idle repositories go while any is held, so a clone
/// a person stopped reading is not kept open until the next read comes. It
/// ends once nothing is held and starts again with the next repository.
fn keep_releasing() {
    static RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    use std::sync::atomic::Ordering;
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let started = std::thread::Builder::new()
        .name("knobas-git-release".to_owned())
        .spawn(|| {
            loop {
                std::thread::sleep(HELD_FOR / 4);
                let mut all = held();
                all.retain(|_, (_, _, at)| at.elapsed() < HELD_FOR);
                if all.is_empty() {
                    // Under the lock, so an `open` that inserts next sees
                    // the thread gone and starts another.
                    RUNNING.store(false, Ordering::SeqCst);
                    return;
                }
            }
        });
    if started.is_err() {
        RUNNING.store(false, Ordering::SeqCst);
    }
}

/// Let go of every repository now. For tests, and for a clone about to be
/// removed.
pub fn release_all() {
    held().clear();
}

/// Where the system-wide git configuration is, when the caller knows it
/// without asking git (on Windows it sits beside the located `git.exe`).
/// Unset, the Unix default `/etc/gitconfig` is read, and on Windows none.
pub fn set_system_config(path: Option<PathBuf>) {
    *system_config()
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = path;
}

fn system_config() -> &'static Mutex<Option<PathBuf>> {
    static PATH: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
    PATH.get_or_init(Mutex::default)
}

/// A clone, opened for reading.
pub struct Repo {
    pub(crate) repo: gix::Repository,
    /// `.git/shallow` is there: history is cut somewhere, and where is git's
    /// to say.
    pub(crate) shallow: bool,
}

impl Repo {
    /// The repository whose working tree (or git directory) is `path`,
    /// opened there and no higher, read-only. Kept open for [`HELD_FOR`]
    /// after its last read.
    ///
    /// # Errors
    ///
    /// [`AskGit`] where it cannot be read here: no repository, one whose
    /// owner is someone else (git refuses it too), an object format or an
    /// extension this reader does not know, a grafts file.
    pub fn open(path: &Path) -> Result<Self, AskGit> {
        let found = {
            let mut all = held();
            all.retain(|_, (_, _, at)| at.elapsed() < HELD_FOR);
            // A clone removed or moved since it was opened is no clone,
            // though its mapped packs would still answer.
            if all
                .get(path)
                .is_some_and(|(repo, _, _)| !repo.git_dir().join("HEAD").exists())
            {
                all.remove(path);
            }
            all.get_mut(path).map(|(repo, shallow, at)| {
                *at = Instant::now();
                (repo.clone(), *shallow)
            })
        };
        let (shared, shallow) = match found {
            Some(found) => found,
            None => {
                let opened = open_fresh(path, false)?;
                let mut all = held();
                if all.len() >= HELD {
                    let oldest = all
                        .iter()
                        .min_by_key(|(_, (_, _, at))| *at)
                        .map(|(path, _)| path.clone());
                    if let Some(oldest) = oldest {
                        all.remove(&oldest);
                    }
                }
                all.insert(
                    path.to_path_buf(),
                    (opened.0.clone(), opened.1, Instant::now()),
                );
                drop(all);
                keep_releasing();
                opened
            }
        };
        let mut repo = shared.to_thread_local();
        // Commits and trees read again within one answer (a walk, a diff)
        // are kept decoded.
        repo.object_cache_size_if_unset(16 * 1024 * 1024);
        Ok(Self { repo, shallow })
    }
}

impl Repo {
    /// [`Repo::open`] reading the repository's own configuration alone: no
    /// system file, no user file. Opened afresh each time. For tests, whose
    /// answers must not depend on the machine they run on, as a `git` run
    /// with `GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1` does not.
    ///
    /// # Errors
    ///
    /// As [`Repo::open`].
    pub fn open_isolated(path: &Path) -> Result<Self, AskGit> {
        let (shared, shallow) = open_fresh(path, true)?;
        let mut repo = shared.to_thread_local();
        repo.object_cache_size_if_unset(16 * 1024 * 1024);
        Ok(Self { repo, shallow })
    }
}

fn open_fresh(path: &Path, isolated: bool) -> Result<(gix::ThreadSafeRepository, bool), AskGit> {
    use gix::sec::Permission::{Allow, Deny};
    // An inherited `GIT_DIR`, `GIT_OBJECT_DIRECTORY` or `GIT_CONFIG_*` would
    // point the read somewhere the path does not: none is read. The home
    // directory is, for the user's own configuration.
    let env = gix::open::permissions::Environment {
        xdg_config_home: Allow,
        home: Allow,
        http_transport: Deny,
        identity: Deny,
        objects: Deny,
        git_prefix: Deny,
        other: Deny,
        ssh_prefix: Deny,
    };
    let system = system_config()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let config = gix::open::permissions::Config {
        // Never: it runs `git config` to find the installation's own file.
        git_binary: false,
        // Off Unix the system file is found by running git, unless the
        // caller named it.
        system: !isolated && (cfg!(unix) || system.is_some()),
        git: !isolated,
        user: !isolated,
        env: false,
        includes: true,
    };
    let attributes = gix::open::permissions::Attributes {
        git_binary: false,
        // Its place is found by running git off Unix.
        system: !isolated && cfg!(unix),
        git: !isolated,
    };
    let mut options = gix::open::Options::default()
        .permissions(gix::open::Permissions {
            env,
            config,
            attributes,
        })
        // git refuses a repository someone else owns; so does this.
        .bail_if_untrusted(true)
        // A sha means the commit it names (ADR-0036): no replace ref.
        .config_overrides(["core.useReplaceRefs=false"]);
    if let Some(system) = system.filter(|_| !isolated) {
        options = options.system_config_path(system);
    }
    let shared = options
        .open(path)
        .map_err(|error| AskGit::because(format!("not opened in-process: {error}")))?;
    let repo = shared.to_thread_local();
    if repo.object_hash() != gix::hash::Kind::Sha1 {
        return Err(AskGit::because("an object format other than sha1"));
    }
    let common = repo.common_dir();
    // Grafts rewrite parents; git still honours the file, this reader not.
    if common.join("info").join("grafts").exists() {
        return Err(AskGit::because("a grafts file"));
    }
    let shallow = common.join("shallow").exists();
    Ok((shared, shallow))
}

/// Whether `name` is hex digits only, 7 to 64 of them (ADR-0034's sha).
pub(crate) fn is_hex(name: &str) -> bool {
    (7..=64).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_hexdigit())
}

/// What git's `run_capped` reader would have kept of `out`: at most
/// `max_lines` lines and `max_bytes` bytes, and whether it stopped there.
/// The same cut as the app's read of a git's standard output, so a capped
/// in-process answer is the capped git answer byte for byte: the reader
/// checks the caps before each line, takes a line whole unless the byte cap
/// falls inside it, and says "cut" as soon as a cap is reached, without
/// looking for the end.
#[must_use]
pub fn capped(out: &[u8], max_lines: usize, max_bytes: usize) -> (Vec<u8>, bool) {
    let mut kept = 0usize;
    let mut lines = 0usize;
    loop {
        if lines >= max_lines || kept >= max_bytes {
            return (out[..kept].to_vec(), true);
        }
        if kept == out.len() {
            return (out.to_vec(), false);
        }
        let left = max_bytes - kept;
        let rest = &out[kept..];
        let window = &rest[..rest.len().min(left)];
        let line = window
            .iter()
            .position(|b| *b == b'\n')
            .map_or(window.len(), |at| at + 1);
        kept += line;
        lines += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::capped;

    /// The cut is the reader's: lines whole, the byte cap inside a line, and
    /// "cut" at a cap reached exactly.
    #[test]
    fn the_cap_cuts_as_the_reader_of_gits_output_cut() {
        assert_eq!(
            capped(b"a\nb\nc\n", 10, 100),
            (b"a\nb\nc\n".to_vec(), false)
        );
        assert_eq!(capped(b"a\nb\nc\n", 2, 100), (b"a\nb\n".to_vec(), true));
        // Exactly at the line cap: cut, though nothing follows.
        assert_eq!(capped(b"a\nb\n", 2, 100), (b"a\nb\n".to_vec(), true));
        assert_eq!(capped(b"abcdef\n", 10, 4), (b"abcd".to_vec(), true));
        assert_eq!(capped(b"abc", 10, 3), (b"abc".to_vec(), true));
        assert_eq!(capped(b"abc", 10, 4), (b"abc".to_vec(), false));
        assert_eq!(capped(b"", 10, 4), (Vec::new(), false));
        assert_eq!(capped(b"", 0, 4), (Vec::new(), true));
    }
}
