//! Who answers a read of a clone: knobas's own process, and `git` behind it
//! (the git-starts pass of PR #2479, 2026-11-16,
//! `docs/decisions/2026-11-16-a-clone-is-read-in-process.md`, amending
//! ADR-0021, ADR-0036 and ADR-0041).
//!
//! Every read the checkout module made of a commit, a tree, a file or a diff
//! started `git`: one to four processes a press. A start is a few
//! milliseconds on a Mac and tens of milliseconds on Windows, so a page that
//! was fast here missed its line there. [`Reader`] answers each of those
//! command lines in-process first (`knobas-git`, which prints what git
//! prints, byte for byte, and is pinned to that by its tests), and runs the
//! command only where the in-process reader says it cannot answer exactly:
//! [`knobas_git::AskGit`]. The callers parse the same bytes either way.
//!
//! # ADR-0021's and ADR-0036's terms hold for both
//!
//! Read-only, no network, no replace ref, the repository at the path and no
//! higher, checked shas only. In-process they hold by construction (the
//! reader has no write and no transport); for git they are
//! [`git_read`](super::range::git_read)'s flags, as before.
//!
//! # What still starts git
//!
//! The working tree's state and the uncommitted diff
//! ([`working_tree`](super::working_tree), [`uncommitted`](super::uncommitted)):
//! what is changed in a checkout depends on the person's filters, line-ending
//! settings, sparse rules and submodules, which only their git applies. The
//! writes: `clone`, `fetch`, `worktree add`. And any read here the in-process
//! reader hands back.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use knobas_core::checkout::open::is_sha;
use knobas_git::{Answer, DiffAsk, DiffForm};

use super::range::{TIMEOUT, git_read};
use super::uncommitted::run_capped;
use super::working_tree::{APPLE_SHIM, command_line_tools_installed, is_dir_off_runtime, locate};

/// The empty tree of a SHA-1 repository: what a root commit's diff is from.
const EMPTY_TREE_SHA1: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
/// The empty tree of a SHA-256 repository.
const EMPTY_TREE_SHA256: &str = "6ef19b41225c5369f1c104d45d8d85efa9b057b53b14b4b9b939dd74decc5321";

/// How many reads the in-process reader handed to git since the app started:
/// what a slow clone's log line counts.
static ASKED_GIT: AtomicU64 = AtomicU64::new(0);

/// The reads handed to git so far.
#[must_use]
pub fn asked_git() -> u64 {
    ASKED_GIT.load(Ordering::Relaxed)
}

/// Which of `git diff`'s outputs a read asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Form {
    /// `--raw -z`.
    Raw,
    /// `--raw --numstat -z`.
    RawNumstat,
    /// The patch, with `a/` and `b/`.
    Patch,
}

/// One clone's reader for one answer.
pub(super) struct Reader {
    path: PathBuf,
    /// The `PATH` git is looked for on, should a read need it.
    search: OsString,
    /// Whether knobas's own process answers first.
    inside: bool,
    /// The git to run, located once and only when a read falls to it.
    git: tokio::sync::OnceCell<Option<PathBuf>>,
}

impl Reader {
    /// The clone at `path`, read in-process and by the `git` on this
    /// process's `PATH` behind it.
    pub(super) fn at(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            search: std::env::var_os("PATH").unwrap_or_default(),
            inside: true,
            git: tokio::sync::OnceCell::new(),
        }
    }

    /// The clone at `path`, read by the `git` on `search` alone: what the
    /// tests that record git's command lines ask, and the oracle the
    /// in-process reader is compared with.
    pub(super) fn git_alone(search: &OsStr, path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            search: search.to_owned(),
            inside: false,
            git: tokio::sync::OnceCell::new(),
        }
    }

    /// [`Reader::at`] with the `PATH` to find git on, so a test can put a
    /// recording git there and see that none was started.
    #[cfg(test)]
    pub(super) fn at_with(search: &OsStr, path: &Path) -> Self {
        Self {
            search: search.to_owned(),
            ..Self::at(path)
        }
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    /// The git to run at this clone, or `None`: no git, or no folder there.
    async fn git(&self) -> Option<&Path> {
        self.git
            .get_or_init(|| async {
                let git = locate(
                    &self.search,
                    Path::new(APPLE_SHIM),
                    command_line_tools_installed,
                    TIMEOUT,
                )
                .await
                .ok()?;
                is_dir_off_runtime(&self.path).await.then_some(git)
            })
            .await
            .as_deref()
    }

    /// `read` asked of the in-process reader, off the async runtime. `None`:
    /// git is to be asked.
    async fn inside<T: Send + 'static>(
        &self,
        read: impl FnOnce(&knobas_git::Repo) -> Result<T, knobas_git::AskGit> + Send + 'static,
    ) -> Option<T> {
        if !self.inside {
            return None;
        }
        let path = self.path.clone();
        let search = self.search.clone();
        let asked = tokio::task::spawn_blocking(move || {
            // Once: where this machine's git keeps its system configuration.
            static TOLD: std::sync::Once = std::sync::Once::new();
            TOLD.call_once(|| {
                knobas_git::set_system_config(super::working_tree::system_config_beside(&search));
            });
            let repo = knobas_git::Repo::open(&path)?;
            read(&repo)
        })
        .await;
        match asked {
            Ok(Ok(answer)) => Some(answer),
            Ok(Err(why)) => {
                ASKED_GIT.fetch_add(1, Ordering::Relaxed);
                tracing::debug!(checkout = %self.path.display(), %why, "a read left to git");
                None
            }
            Err(_) => None,
        }
    }

    /// The whole sha of the commit `name` names at the clone, if it holds
    /// it (`git rev-parse --verify --quiet <name>^{commit}`). `name` is a
    /// checked sha, or one with a `^`.
    pub(super) async fn commit(&self, name: &str) -> Option<String> {
        let asked = name.to_owned();
        match self.inside(move |repo| repo.rev_parse_commit(&asked)).await {
            Some(Answer::Out(out)) => return sha_of(&out),
            Some(Answer::Refused) => return None,
            None => {}
        }
        let mut command = git_read(self.git().await?, &self.path);
        command.args(["rev-parse", "--verify", "--quiet", "--end-of-options"]);
        command.arg(format!("{name}^{{commit}}"));
        let (out, cut) = run_capped(command, 2, 1024).await.ok()?;
        (!cut).then(|| sha_of(&out)).flatten()
    }

    /// The empty tree's name in this repository's object format, unless the
    /// repository is shallow: there a commit with no parent here may well
    /// have one, and the source is asked instead.
    pub(super) async fn empty_tree_if_whole(&self) -> Option<String> {
        let out = match self.inside(knobas_git::Repo::shallow_and_format).await {
            Some(Answer::Out(out)) => out,
            Some(Answer::Refused) => return None,
            None => {
                let mut command = git_read(self.git().await?, &self.path);
                command.args([
                    "rev-parse",
                    "--is-shallow-repository",
                    "--show-object-format",
                ]);
                run_capped(command, 4, 1024).await.ok()?.0
            }
        };
        let out = String::from_utf8_lossy(&out);
        let mut lines = out.lines().map(str::trim);
        if lines.next()? != "false" {
            return None;
        }
        match lines.next()? {
            "sha1" => Some(EMPTY_TREE_SHA1.to_owned()),
            "sha256" => Some(EMPTY_TREE_SHA256.to_owned()),
            _ => None,
        }
    }

    /// The best common ancestor of two checked shas (`git merge-base`).
    pub(super) async fn merge_base(&self, a: &str, b: &str) -> Option<String> {
        let (x, y) = (a.to_owned(), b.to_owned());
        match self.inside(move |repo| repo.merge_base(&x, &y)).await {
            Some(Answer::Out(out)) => return sha_of(&out),
            Some(Answer::Refused) => return None,
            None => {}
        }
        let mut command = git_read(self.git().await?, &self.path);
        command.args(["merge-base", "--end-of-options", a, b]);
        let (out, cut) = run_capped(command, 2, 1024).await.ok()?;
        (!cut).then(|| sha_of(&out)).flatten()
    }

    /// `git diff --no-ext-diff --no-color --no-textconv -M` between `old`
    /// and `new` (or the one range `old` when `new` is `None`) in `form`,
    /// with `-w` for `ignore_whitespace`, only the literal `paths` when any
    /// is named: its output up to `max_lines` lines and `max_bytes` bytes,
    /// and whether the read stopped there.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn diff(
        &self,
        form: Form,
        old: &str,
        new: Option<&str>,
        ignore_whitespace: bool,
        paths: &[&str],
        max_lines: usize,
        max_bytes: usize,
    ) -> Option<(Vec<u8>, bool)> {
        let (asked_old, asked_new) = (old.to_owned(), new.map(str::to_owned));
        let asked_paths: Vec<String> = paths.iter().map(|p| (*p).to_owned()).collect();
        let inside = self
            .inside(move |repo| {
                let paths: Vec<&str> = asked_paths.iter().map(String::as_str).collect();
                repo.diff(&DiffAsk {
                    old: &asked_old,
                    new: asked_new.as_deref(),
                    form: match form {
                        Form::Raw => DiffForm::Raw,
                        Form::RawNumstat => DiffForm::RawNumstat,
                        Form::Patch => DiffForm::Patch,
                    },
                    ignore_space: ignore_whitespace,
                    paths: &paths,
                    max_lines,
                    max_bytes,
                })
            })
            .await;
        match inside {
            Some(Answer::Out(out)) => return Some(knobas_git::capped(&out, max_lines, max_bytes)),
            Some(Answer::Refused) => return None,
            None => {}
        }
        let mut command = git_read(self.git().await?, &self.path);
        command.args([
            "-c",
            "diff.autoRefreshIndex=false",
            "diff",
            "--no-ext-diff",
            "--no-color",
            "--no-textconv",
            "-M",
        ]);
        match form {
            Form::Raw => command.args(["--raw", "-z"]),
            Form::RawNumstat => command.args(["--raw", "--numstat", "-z"]),
            Form::Patch => command.args(["--src-prefix=a/", "--dst-prefix=b/"]),
        };
        if ignore_whitespace {
            command.arg("-w");
        }
        command.arg(old);
        if let Some(new) = new {
            command.arg(new);
        }
        command.arg("--");
        if !paths.is_empty() {
            // Literal paths: a name is the file's, never a pattern.
            command.env("GIT_LITERAL_PATHSPECS", "1");
            command.args(paths);
        }
        run_capped(command, max_lines, max_bytes).await.ok()
    }

    /// `git log --max-count=<max> --format=%H%x1f%an%x1f%ar%x1f%s%x1e
    /// <base>..<head>`: its output, or `None` where it cannot be read.
    pub(super) async fn log(&self, base: &str, head: &str, max: usize) -> Option<Vec<u8>> {
        let (x, y) = (base.to_owned(), head.to_owned());
        let now = chrono::Utc::now().timestamp();
        match self.inside(move |repo| repo.log(&x, &y, max, now)).await {
            Some(Answer::Out(out)) => return Some(out),
            Some(Answer::Refused) => return None,
            None => {}
        }
        let mut command = git_read(self.git().await?, &self.path);
        command.args([
            "log",
            "--no-color",
            &format!("--max-count={max}"),
            "--format=%H%x1f%an%x1f%ar%x1f%s%x1e",
            &format!("{base}..{head}"),
            "--",
        ]);
        run_capped(command, usize::MAX, 4 * 1024 * 1024)
            .await
            .ok()
            .map(|(out, _)| out)
    }

    /// `git ls-tree -r -t -z --long --full-tree <sha>` up to `max_bytes`,
    /// and whether the read stopped there. `sha` is a whole sha the clone
    /// holds ([`Reader::commit`]).
    pub(super) async fn ls_tree(&self, sha: &str, max_bytes: usize) -> Option<(Vec<u8>, bool)> {
        let asked = sha.to_owned();
        match self.inside(move |repo| repo.ls_tree(&asked)).await {
            Some(Answer::Out(out)) => return Some(knobas_git::capped(&out, usize::MAX, max_bytes)),
            Some(Answer::Refused) => return None,
            None => {}
        }
        let mut command = git_read(self.git().await?, &self.path);
        command.args(["ls-tree", "-r", "-t", "-z", "--long", "--full-tree"]);
        command.arg(sha);
        run_capped(command, usize::MAX, max_bytes).await.ok()
    }

    /// `git cat-file blob <blob>`, read up to `max` bytes; the flag says the
    /// read stopped at `max`.
    pub(super) async fn blob(&self, blob: &str, max: usize) -> Option<(Vec<u8>, bool)> {
        if !is_sha(blob) {
            return None;
        }
        let asked = blob.to_owned();
        match self.inside(move |repo| repo.cat_blob(&asked, max)).await {
            Some(Some(read)) => return Some(read),
            Some(None) => return None,
            None => {}
        }
        let mut command = git_read(self.git().await?, &self.path);
        command.args(["cat-file", "blob"]);
        command.arg(blob);
        run_capped(command, usize::MAX, max).await.ok()
    }
}

/// The sha a `rev-parse` or `merge-base` printed.
fn sha_of(out: &[u8]) -> Option<String> {
    let sha = String::from_utf8_lossy(out).trim().to_owned();
    is_sha(&sha).then_some(sha)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.name=knobas test",
                "-c",
                "user.email=test@knobas.invalid",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            // Years back, so "6 years ago" reads the same a second later.
            .env("GIT_AUTHOR_DATE", "1600000000 +0000")
            .env("GIT_COMMITTER_DATE", "1600000000 +0000")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    /// Two branches off one commit: a file edited, one renamed with an
    /// edit, one binary, and `main` moved on beside them.
    fn repo() -> (tempfile::TempDir, String, String, String) {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        git(p, &["init", "-q", "-b", "main"]);
        let text: String = (0..40).map(|i| format!("line {i} of the file\n")).collect();
        std::fs::write(p.join("a.txt"), &text).unwrap();
        std::fs::write(p.join("old.txt"), &text).unwrap();
        std::fs::write(p.join("logo.bin"), [0u8, 1, 2, 3]).unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-q", "-m", "base"]);
        let base = git(p, &["rev-parse", "HEAD"]);
        git(p, &["checkout", "-q", "-b", "topic"]);
        std::fs::write(p.join("a.txt"), text.replace("line 7 ", "row 7 ")).unwrap();
        git(p, &["mv", "old.txt", "new.txt"]);
        std::fs::write(p.join("new.txt"), text.replace("line 30 ", "row 30 ")).unwrap();
        std::fs::write(p.join("logo.bin"), [0u8, 9, 9, 9]).unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-q", "-m", "topic"]);
        let topic = git(p, &["rev-parse", "HEAD"]);
        git(p, &["checkout", "-q", "main"]);
        std::fs::write(p.join("main.txt"), "main\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-q", "-m", "main moves on"]);
        let main = git(p, &["rev-parse", "HEAD"]);
        (dir, base, topic, main)
    }

    /// A folder whose `git` writes its arguments, one call a line, to
    /// `calls` and then runs the real git.
    fn recording_git() -> (tempfile::TempDir, PathBuf, OsString) {
        let real = String::from_utf8(
            std::process::Command::new("sh")
                .args(["-c", "command -v git"])
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .to_owned();
        let bin = tempfile::tempdir().unwrap();
        let calls = bin.path().join("calls");
        let stub = bin.path().join("git");
        std::fs::write(
            &stub,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nexec '{real}' \"$@\"\n",
                calls.display()
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
        let search = std::env::join_paths(std::iter::once(bin.path().to_path_buf()).chain(
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
        ))
        .unwrap();
        (bin, calls, search)
    }

    /// Every read a page makes of a clone, asked of `reader`: a commit, a
    /// parent, the tree, a file, the merge base, the log, and the diff as
    /// names, as counts, whole and one file of it.
    async fn every_read(
        reader: &Reader,
        base: &str,
        topic: &str,
        main: &str,
    ) -> Vec<Option<Vec<u8>>> {
        let range = format!("{main}^{{commit}}...{topic}^{{commit}}");
        let blob = git(reader.path(), &["rev-parse", &format!("{topic}:a.txt")]);
        let sha = |found: Option<String>| found.map(String::into_bytes);
        let out = |found: Option<(Vec<u8>, bool)>| found.map(|(out, _)| out);
        vec![
            sha(reader.commit(topic).await),
            sha(reader.commit(&format!("{topic}^")).await),
            sha(reader.commit(&topic[..10]).await),
            sha(reader
                .commit("0123456789abcdef0123456789abcdef01234567")
                .await),
            sha(reader.empty_tree_if_whole().await),
            sha(reader.merge_base(main, topic).await),
            reader.log(base, topic, 500).await,
            out(reader.ls_tree(topic, usize::MAX).await),
            out(reader.blob(&blob, usize::MAX).await),
            out(reader.blob(&blob, 10).await),
            out(reader
                .diff(Form::Raw, &range, None, false, &[], usize::MAX, usize::MAX)
                .await),
            out(reader
                .diff(
                    Form::RawNumstat,
                    &range,
                    None,
                    false,
                    &[],
                    usize::MAX,
                    usize::MAX,
                )
                .await),
            out(reader
                .diff(
                    Form::RawNumstat,
                    base,
                    Some(topic),
                    true,
                    &[],
                    usize::MAX,
                    usize::MAX,
                )
                .await),
            out(reader
                .diff(
                    Form::Patch,
                    base,
                    Some(topic),
                    false,
                    &[],
                    usize::MAX,
                    usize::MAX,
                )
                .await),
            out(reader
                .diff(Form::Patch, base, Some(topic), false, &[], 5, usize::MAX)
                .await),
            out(reader
                .diff(
                    Form::Patch,
                    base,
                    Some(topic),
                    false,
                    &["new.txt", "old.txt"],
                    usize::MAX,
                    usize::MAX,
                )
                .await),
            out(reader
                .diff(
                    Form::Patch,
                    base,
                    Some(topic),
                    true,
                    &["a.txt"],
                    usize::MAX,
                    usize::MAX,
                )
                .await),
        ]
    }

    /// **No process on a read a person waits on**: every read answered
    /// in-process starts no git at all, and each answer is the bytes git
    /// gives, asked alone.
    #[tokio::test]
    async fn every_read_is_answered_with_no_git_started_and_as_git_answers() {
        let (dir, base, topic, main) = repo();
        let (_bin, calls, search) = recording_git();
        let inside = every_read(&Reader::at_with(&search, dir.path()), &base, &topic, &main).await;
        assert!(
            !calls.exists(),
            "an in-process read started git: {}",
            std::fs::read_to_string(&calls).unwrap_or_default()
        );
        let by_git = every_read(
            &Reader::git_alone(&search, dir.path()),
            &base,
            &topic,
            &main,
        )
        .await;
        let started = std::fs::read_to_string(&calls).unwrap().lines().count();
        assert!(started >= 16, "git alone runs a command a read: {started}");
        for (at, (ours, theirs)) in inside.iter().zip(&by_git).enumerate() {
            assert_eq!(
                ours.as_ref()
                    .map(|o| String::from_utf8_lossy(o).into_owned()),
                theirs
                    .as_ref()
                    .map(|o| String::from_utf8_lossy(o).into_owned()),
                "read {at}"
            );
        }
        assert!(inside.iter().filter(|a| a.is_some()).count() >= 15);
    }

    /// What the in-process reader hands back is asked of git, as before: a
    /// repository whose `diff.algorithm` is not git's default is diffed by
    /// git, and its trees and files are still read in-process.
    #[tokio::test]
    async fn a_read_the_reader_hands_back_is_gits() {
        let (dir, base, topic, _main) = repo();
        git(dir.path(), &["config", "diff.algorithm", "histogram"]);
        knobas_git::release_all();
        let (_bin, calls, search) = recording_git();
        let reader = Reader::at_with(&search, dir.path());
        let before = asked_git();
        assert!(reader.ls_tree(&topic, usize::MAX).await.is_some());
        assert!(
            !calls.exists(),
            "a tree needs no git whatever the diff settings"
        );
        let patch = reader
            .diff(
                Form::Patch,
                &base,
                Some(&topic),
                false,
                &[],
                usize::MAX,
                usize::MAX,
            )
            .await
            .expect("git answers");
        let recorded = std::fs::read_to_string(&calls).unwrap();
        assert_eq!(recorded.lines().count(), 1, "{recorded}");
        assert!(recorded.contains(" diff "), "{recorded}");
        assert!(asked_git() > before);
        let alone = Reader::git_alone(&search, dir.path())
            .diff(
                Form::Patch,
                &base,
                Some(&topic),
                false,
                &[],
                usize::MAX,
                usize::MAX,
            )
            .await
            .expect("git answers");
        assert_eq!(patch, alone);
    }
}
