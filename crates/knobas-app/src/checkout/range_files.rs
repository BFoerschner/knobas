//! A range's diff read a file at a time (the compare pass of PR #2479,
//! 2026-11-13, `docs/decisions/2026-11-13-a-branch-pick-draws-in-a-frame.md`):
//! the list of files a range changes first, each with its counts and no
//! lines, and then one file's lines at a time, paged, as the page scrolls
//! to it.
//!
//! [`range_diff`](super::range_diff) answers the whole diff in one: at
//! 10,000 changed files that was 9 MB of JSON parsed on the page's thread and
//! a card built for every file before the first one drew. Here:
//!
//! - **The list** ([`range_files`]): at a checkout that holds both ends, or
//!   that predicts a compare of two branch names (ADR-0041), `git diff --raw
//!   --numstat -z -M` (names, modes, blobs and counts, no text); every file
//!   is listed, where the whole diff's read stopped at its byte and line caps.
//!   Without one, the source's compare as [`source_range_diff`](super::source_range_diff)
//!   reads it, kept here whole for the session's next reads of its files, and
//!   answered without its lines. Gitea's compare has no call that lists files
//!   with their counts short of the diff itself (its commits listing names
//!   files per commit, without counts), so that read is the floor.
//! - **One file** ([`range_file`]): `git diff -- <path> [<old path>]` at the
//!   checkout, or the file out of the kept compare (read again by its two
//!   shas, which the source's view cache keeps for the session, ADR-0032, if
//!   this cache let it go). [`PAGE_LINES`] lines an answer, from a line the
//!   page names: a file of tens of thousands of lines draws its first page
//!   while the rest waits for the reader to scroll to it.
//!
//! **The same answer as the whole read** (pinned by the tests below): every
//! file [`range_files`] lists is the file `range_diff` parses, less its
//! lines, and its pages put back together are its lines.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use knobas_core::checkout::open::is_sha;
use knobas_source::diff::{DiffChange, DiffFile, Hunk, parse_view};
use serde::Serialize;
use sqlx::PgPool;

use super::range::{commit, empty_tree_if_whole, git_read};
use super::uncommitted::{READ_BYTES, READ_LINES, run_capped};
use super::working_tree::{APPLE_SHIM, command_line_tools_installed, is_dir_off_runtime, locate};
use super::{DiffReadAt, RangeDiff, branch_tip, repo_of, resolve};
use crate::IpcError;

/// The most lines one answer of [`range_file`] carries.
pub const PAGE_LINES: u32 = 2_000;

/// The files a range changes, without their lines: what Compare, *Open as
/// diff* and a pull request's *Files changed* list first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct RangeFiles {
    /// Each file as the whole read parses it, with `hunks` empty: its paths,
    /// change, counts, modes and blob.
    pub files: Vec<DiffFile>,
    /// The list was cut (past 16 MiB of git's listing; the source's ceiling).
    pub truncated: bool,
    /// As [`RangeDiff::range`]: the two whole shas a file's lines are read
    /// between ([`range_file`]).
    pub range: Option<knobas_source::view::DiffRange>,
    /// As [`RangeDiff::read_at`].
    pub read_at: DiffReadAt,
    /// As [`RangeDiff::predicted_at`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub predicted_at: Option<String>,
}

/// One page of one file's lines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct RangeFilePage {
    /// The file as the whole read parses it, `hunks` holding this page's
    /// lines only.
    pub file: DiffFile,
    /// The line this page starts at, counted over all the file's hunks.
    pub from: u32,
    /// The page's first hunk goes on from the last page's last one: its
    /// lines belong at the end of that hunk, under no header of their own.
    pub continues: bool,
    /// Where the next page starts; `None` on the last.
    pub next: Option<u32>,
}

// -- the list ----------------------------------------------------------------

/// The files `base`...`head` of the repository `entity_id` changes, without
/// their lines, read where [`range_diff`](super::range_diff) would read the
/// whole diff: at the checkout when it holds both ends, predicted there for
/// two branch names whose tips it holds (ADR-0041), else from the source,
/// whose answer is kept for [`range_file`].
///
/// # Errors
///
/// As [`range_diff`](super::range_diff).
#[allow(clippy::too_many_arguments)]
pub async fn range_files(
    pool: &PgPool,
    secrets: &Arc<dyn knobas_secrets::SecretStore>,
    registry: &Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    live: &crate::sources::live::LiveSources,
    entity_id: &str,
    base: &str,
    head: &str,
    ignore_whitespace: bool,
) -> Result<RangeFiles, IpcError> {
    if let Some(path) = checkout_path(pool, entity_id).await
        && let Some(local) = stat(&path, base, head, ignore_whitespace).await
    {
        return Ok(local.answer(entity_id, None));
    }
    if let Some(predicted) = predicted_files(
        pool,
        registry.as_ref(),
        entity_id,
        base,
        head,
        ignore_whitespace,
    )
    .await
    {
        return Ok(predicted);
    }
    super::fetch_missing_tip(pool, registry.as_ref(), entity_id, base, head).await;
    let diff =
        super::source_range_diff(pool, secrets, registry, live, entity_id, base, head).await?;
    Ok(keep(entity_id, base, head, diff))
}

/// The files of `base`...`head` as the source's compare answers them now,
/// never local: what a page that drew [`range_files`]' predicted answer asks
/// behind it (ADR-0041), as [`source_range_diff`](super::source_range_diff)
/// is for the whole diff; its answer is kept for [`range_file`].
///
/// # Errors
///
/// As [`source_range_diff`](super::source_range_diff).
#[allow(clippy::too_many_arguments)]
pub async fn source_range_files(
    pool: &PgPool,
    secrets: &Arc<dyn knobas_secrets::SecretStore>,
    registry: &Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    live: &crate::sources::live::LiveSources,
    entity_id: &str,
    base: &str,
    head: &str,
) -> Result<RangeFiles, IpcError> {
    let diff =
        super::source_range_diff(pool, secrets, registry, live, entity_id, base, head).await?;
    Ok(keep(entity_id, base, head, diff))
}

/// The source's whole answer, kept under the ends it was asked by and the two
/// shas it settled, and answered without its lines.
fn keep(entity_id: &str, base: &str, head: &str, diff: RangeDiff) -> RangeFiles {
    let diff = Arc::new(diff);
    kept().put(entity_id, base, head, &diff);
    if let Some(range) = &diff.range {
        kept().put(entity_id, &range.base, &range.head, &diff);
    }
    RangeFiles {
        files: diff.files.iter().map(listed).collect(),
        truncated: diff.truncated,
        range: diff.range.clone(),
        read_at: diff.read_at,
        predicted_at: diff.predicted_at.clone(),
    }
}

/// A file as the list carries it: everything but its lines.
fn listed(file: &DiffFile) -> DiffFile {
    DiffFile {
        hunks: Vec::new(),
        truncated: false,
        ..file.clone()
    }
}

/// [`range_files`] between two branch names, predicted at the checkout from
/// the tips the mirror holds for them, as [`predicted_diff`](super::predicted_diff).
async fn predicted_files(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    entity_id: &str,
    base: &str,
    head: &str,
    ignore_whitespace: bool,
) -> Option<RangeFiles> {
    let repo = repo_of(pool, entity_id).await.ok()?.entity_id?;
    let path = resolve(pool, &repo).await.ok()?.path?;
    let (base_tip, head_tip) = tokio::join!(
        branch_tip(pool, registry, &repo, base),
        branch_tip(pool, registry, &repo, head)
    );
    let (base_tip, head_tip) = (base_tip.ok()??, head_tip.ok()??);
    let local = stat_between(
        Path::new(&path),
        &base_tip.commit,
        &head_tip.commit,
        ignore_whitespace,
    )
    .await?;
    let predicted_at = match (base_tip.as_of, head_tip.as_of) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };
    Some(local.answer(&repo, Some(predicted_at.unwrap_or_default())))
}

async fn checkout_path(pool: &PgPool, entity_id: &str) -> Option<std::path::PathBuf> {
    resolve(pool, entity_id)
        .await
        .ok()
        .and_then(|c| c.path)
        .map(std::path::PathBuf::from)
}

/// The list as the checkout answered it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalStat {
    pub files: Vec<DiffFile>,
    /// The commits from the old side to the head, newest first, at most
    /// [`COMMITS`]: what Compare's *Commits* lists, as the source's compare
    /// lists them.
    pub commits: Vec<knobas_source::view::RangeCommit>,
    pub truncated: bool,
    /// The old side's whole sha; `None` for the empty tree.
    pub base: Option<String>,
    pub head: String,
}

impl LocalStat {
    fn answer(self, repository: &str, predicted_at: Option<String>) -> RangeFiles {
        RangeFiles {
            files: self.files,
            truncated: self.truncated,
            range: self.base.map(|base| knobas_source::view::DiffRange {
                repository: repository.to_owned(),
                base,
                head: self.head,
                commits: self.commits,
                reviewed: None,
            }),
            read_at: DiffReadAt::Checkout,
            predicted_at,
        }
    }
}

/// The two ends at the checkout, resolved as [`range::read`](super::range::read)
/// resolves them: each a checked sha it holds, the base possibly `<sha>^`
/// (the empty tree for a root commit). The git to run, the old side as git
/// is to name it, the old side's sha (`None`: the empty tree) and the head's.
///
/// Two whole shas need no `rev-parse` each: they are handed to git as
/// `<sha>^{commit}`, so a checkout that lacks one fails the read itself and
/// the source is asked, one start of git fewer each for a read the page
/// waits on (the compare pass: four starts of git in a row were most of a
/// list's time on a busy machine).
struct Ends {
    git: std::path::PathBuf,
    /// The old side as git is to name it, and the new.
    old: String,
    new: String,
    base: Option<String>,
    head: String,
}

/// A 40- or 64-digit sha: whole, so it names one commit and nothing else.
fn whole(sha: &str) -> bool {
    is_sha(sha) && matches!(sha.len(), 40 | 64)
}

async fn git_at_checkout(path: &Path) -> Option<std::path::PathBuf> {
    let search = std::env::var_os("PATH").unwrap_or_default();
    let git = locate(
        &search,
        Path::new(APPLE_SHIM),
        command_line_tools_installed,
        super::range::TIMEOUT,
    )
    .await
    .ok()?;
    is_dir_off_runtime(path).await.then_some(git)
}

async fn ends(path: &Path, base: &str, head: &str) -> Option<Ends> {
    let (stem, caret) = match base.strip_suffix('^') {
        Some(stem) => (stem, true),
        None => (base, false),
    };
    if !is_sha(stem) || !is_sha(head) {
        return None;
    }
    let git = git_at_checkout(path).await?;
    if !caret && whole(stem) && whole(head) {
        let (base, head) = (stem.to_ascii_lowercase(), head.to_ascii_lowercase());
        return Some(Ends {
            old: format!("{base}^{{commit}}"),
            new: format!("{head}^{{commit}}"),
            git,
            base: Some(base),
            head,
        });
    }
    let head = commit(&git, path, head).await?;
    let stem = commit(&git, path, stem).await?;
    let (old, base) = if caret {
        match commit(&git, path, &format!("{stem}^")).await {
            Some(parent) => (parent.clone(), Some(parent)),
            None => (empty_tree_if_whole(&git, path).await?, None),
        }
    } else {
        (stem.clone(), Some(stem))
    };
    Some(Ends {
        git,
        old,
        new: head.clone(),
        base,
        head,
    })
}

/// `git diff --raw --numstat -z -M <old> <new>` (`<old>` may be `A...B`),
/// parsed: the files and whether the listing was cut.
async fn listing(
    git: &Path,
    path: &Path,
    old: &str,
    new: Option<&str>,
    ignore_whitespace: bool,
) -> Option<(Vec<DiffFile>, bool)> {
    let mut command = git_read(git, path);
    command.args([
        "-c",
        "diff.autoRefreshIndex=false",
        "diff",
        "--no-ext-diff",
        "--no-color",
        "--no-textconv",
        "-M",
        "--raw",
        "--numstat",
        "-z",
    ]);
    if ignore_whitespace {
        command.arg("-w");
    }
    command.arg(old);
    if let Some(new) = new {
        command.arg(new);
    }
    command.arg("--");
    let (out, cut) = run_capped(command, usize::MAX, READ_BYTES).await.ok()?;
    let (files, cut_inside) = parse_stat(&out, cut);
    Some((files, cut || cut_inside))
}

/// The list a source's three-dot compare of two tips is, at the checkout
/// `path` (ADR-0041): `git diff A...B`, which diffs from their merge base,
/// with the merge base and `git log A..B` read beside it, three starts of
/// git at once. `None` where it cannot be read here, or the two share no
/// history.
pub async fn stat_between(
    path: &Path,
    base_tip: &str,
    head_tip: &str,
    ignore_whitespace: bool,
) -> Option<LocalStat> {
    if !whole(base_tip) || !whole(head_tip) {
        return None;
    }
    let key = listed_key(path, &format!("{base_tip}..."), head_tip, ignore_whitespace);
    if let Some(kept) = kept_lists().get(&key) {
        return Some(kept);
    }
    let all = async {
        let git = git_at_checkout(path).await?;
        let (a, b) = (base_tip.to_ascii_lowercase(), head_tip.to_ascii_lowercase());
        let range = format!("{a}^{{commit}}...{b}^{{commit}}");
        let (from, listed, commits) = tokio::join!(
            super::range::merge_base(path, &a, &b),
            listing(&git, path, &range, None, ignore_whitespace),
            log(&git, path, Some(&a), &b)
        );
        let (files, truncated) = listed?;
        Some(LocalStat {
            files,
            commits,
            truncated,
            base: Some(from?),
            head: b,
        })
    };
    let read = tokio::time::timeout(super::range::TIMEOUT, all)
        .await
        .ok()
        .flatten()?;
    kept_lists().put(key, &read);
    Some(read)
}

/// The last few lists the checkouts answered, by checkout, ends and
/// whitespace: each between two whole shas, so it cannot go stale, and a
/// branch picked again or *Into* picked back answers without git (the
/// compare pass: 10,000 files took 107 ms of `--numstat` a read).
struct Listed(Mutex<VecDeque<(String, LocalStat)>>);

const LISTED: usize = 4;

fn kept_lists() -> &'static Listed {
    static LISTED_ONES: OnceLock<Listed> = OnceLock::new();
    LISTED_ONES.get_or_init(|| Listed(Mutex::new(VecDeque::new())))
}

fn listed_key(path: &Path, base: &str, head: &str, ignore_whitespace: bool) -> String {
    format!(
        "{}\n{}\n{}\n{ignore_whitespace}",
        path.display(),
        base.to_ascii_lowercase(),
        head.to_ascii_lowercase()
    )
}

impl Listed {
    fn get(&self, key: &str) -> Option<LocalStat> {
        let mut all = self.0.lock().expect("the kept lists");
        let at = all.iter().position(|(k, _)| k == key)?;
        let found = all.remove(at)?;
        let stat = found.1.clone();
        all.push_front(found);
        Some(stat)
    }

    fn put(&self, key: String, stat: &LocalStat) {
        let mut all = self.0.lock().expect("the kept lists");
        all.retain(|(k, _)| *k != key);
        all.push_front((key, stat.clone()));
        all.truncate(LISTED);
    }
}

/// The files `base`..`head` changes at the checkout `path`, without their
/// lines: `git diff --raw --numstat -z -M`, read-only and never fetched
/// (ADR-0036's terms). `None` where it cannot be read here.
pub async fn stat(
    path: &Path,
    base: &str,
    head: &str,
    ignore_whitespace: bool,
) -> Option<LocalStat> {
    let key = (whole(base) && whole(head)).then(|| listed_key(path, base, head, ignore_whitespace));
    if let Some(kept) = key.as_deref().and_then(|key| kept_lists().get(key)) {
        return Some(kept);
    }
    let all = async {
        let Ends {
            git,
            old,
            new,
            base,
            head,
        } = ends(path, base, head).await?;
        let (listed, commits) = tokio::join!(
            listing(&git, path, &old, Some(&new), ignore_whitespace),
            log(&git, path, base.as_deref(), &head)
        );
        let (files, truncated) = listed?;
        Some(LocalStat {
            files,
            commits,
            truncated,
            base,
            head,
        })
    };
    let read = tokio::time::timeout(super::range::TIMEOUT, all)
        .await
        .ok()
        .flatten()?;
    if let Some(key) = key {
        kept_lists().put(key, &read);
    }
    Some(read)
}

/// The most commits [`stat`] lists.
pub const COMMITS: usize = 500;

/// The commits `base`..`head` at the checkout, newest first, at most
/// [`COMMITS`] (`git log`, read-only): each sha, subject, and `author · age`
/// as a commit row reads. None where it cannot be read, or from the empty
/// tree.
async fn log(
    git: &Path,
    path: &Path,
    base: Option<&str>,
    head: &str,
) -> Vec<knobas_source::view::RangeCommit> {
    let Some(base) = base else {
        return Vec::new();
    };
    let mut command = git_read(git, path);
    command.args([
        "log",
        "--no-color",
        &format!("--max-count={COMMITS}"),
        "--format=%H%x1f%an%x1f%ar%x1f%s%x1e",
        &format!("{base}..{head}"),
        "--",
    ]);
    let Ok((out, _)) = run_capped(command, usize::MAX, 4 * 1024 * 1024).await else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out)
        .split('\x1e')
        .filter_map(|record| {
            let mut parts = record.trim_start_matches('\n').splitn(4, '\x1f');
            let (sha, author, age, subject) =
                (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
            is_sha(sha).then(|| knobas_source::view::RangeCommit {
                sha: sha.to_owned(),
                title: if subject.trim().is_empty() {
                    sha[..7].to_owned()
                } else {
                    subject.trim().to_owned()
                },
                meta: Some(format!("{author} · {age}")),
            })
        })
        .collect()
}

/// One `--raw` record: the two modes, the two blobs, the status and its
/// score, and the paths (two for a rename or a copy).
struct Raw {
    old_mode: String,
    new_mode: String,
    old_blob: String,
    new_blob: String,
    status: u8,
    score: Option<u8>,
    paths: Vec<String>,
}

/// `git diff --raw --numstat -z` output as the files the patch parser would
/// read out of the same diff, `hunks` empty: the raw records first, then
/// one numstat record for each, in the same order. `cut`: the read stopped
/// short, so a last record may be partial and is dropped. Answers the files
/// and whether a record was dropped.
pub fn parse_stat(out: &[u8], cut: bool) -> (Vec<DiffFile>, bool) {
    let text = String::from_utf8_lossy(out);
    let mut tokens = text
        .split('\0')
        .map(|t| t.trim_start_matches('\n'))
        .peekable();
    let mut raws: Vec<Raw> = Vec::new();
    let mut counts: Vec<(Option<u32>, Option<u32>)> = Vec::new();
    while let Some(token) = tokens.next() {
        if token.is_empty() {
            continue;
        }
        if let Some(meta) = token.strip_prefix(':') {
            let mut parts = meta.split(' ');
            let (Some(old_mode), Some(new_mode), Some(old_blob), Some(new_blob), Some(status)) = (
                parts.next(),
                parts.next(),
                parts.next(),
                parts.next(),
                parts.next(),
            ) else {
                break;
            };
            let letter = status.bytes().next().unwrap_or(b'M');
            let score = status.get(1..).and_then(|s| s.parse::<u8>().ok());
            let n = if matches!(letter, b'R' | b'C') { 2 } else { 1 };
            let paths: Vec<String> = tokens.by_ref().take(n).map(str::to_owned).collect();
            if paths.len() < n {
                break;
            }
            raws.push(Raw {
                old_mode: old_mode.to_owned(),
                new_mode: new_mode.to_owned(),
                old_blob: old_blob.to_owned(),
                new_blob: new_blob.to_owned(),
                status: letter,
                score,
                paths,
            });
            continue;
        }
        // numstat: `<added>\t<deleted>\t<path>`, or `...\t` and the two
        // paths of a rename as tokens of their own; `-` for a binary file.
        let mut parts = token.splitn(3, '\t');
        let (Some(added), Some(deleted), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            break;
        };
        if path.is_empty() && (tokens.next().is_none() || tokens.next().is_none()) {
            break;
        }
        counts.push((added.parse().ok(), deleted.parse().ok()));
    }
    let whole = raws.len().min(counts.len());
    let dropped = raws.len() != counts.len() || (cut && whole > 0);
    // A cut read may have stopped inside the last record whose count came.
    let keep = if cut { whole.saturating_sub(1) } else { whole };
    let files = raws
        .into_iter()
        .zip(counts)
        .take(keep)
        .map(|(raw, (added, deleted))| file_of(raw, added, deleted))
        .collect();
    (files, dropped)
}

/// What the patch parser makes of the same file (`knobas_source::diff`): a
/// binary file is `binary` whatever else, an added or copied one `added`, a
/// rename `renamed` with its old path, the modes where they changed, and the
/// blob after the change (before it, for a deletion).
fn file_of(raw: Raw, added: Option<u32>, deleted: Option<u32>) -> DiffFile {
    let binary = added.is_none() || deleted.is_none();
    let (old_path, path) = match raw.paths.as_slice() {
        [old, new] => (Some(old.clone()), new.clone()),
        [one] => (None, one.clone()),
        _ => (None, String::new()),
    };
    let change = if binary {
        DiffChange::Binary
    } else {
        match raw.status {
            b'A' | b'C' => DiffChange::Added,
            b'D' => DiffChange::Deleted,
            b'R' => DiffChange::Renamed,
            _ => DiffChange::Modified,
        }
    };
    let moved = matches!(raw.status, b'R' | b'C');
    // The patch says `old mode`/`new mode` only for a file on both sides
    // whose mode changed; an added or deleted one names a single mode.
    let modes = !matches!(raw.status, b'A' | b'D') && raw.old_mode != raw.new_mode;
    let gone = |b: &str| b.is_empty() || b.bytes().all(|c| c == b'0');
    // The patch prints `index <before>..<after>` only where the content changed: a pure rename or a mode change names
    // no blob.
    let blob = if raw.old_blob == raw.new_blob {
        None
    } else if !gone(&raw.new_blob) {
        Some(raw.new_blob)
    } else if !gone(&raw.old_blob) {
        Some(raw.old_blob)
    } else {
        None
    };
    DiffFile {
        path,
        old_path: if moved { old_path } else { None },
        change,
        additions: added.unwrap_or(0),
        deletions: deleted.unwrap_or(0),
        hunks: Vec::new(),
        truncated: false,
        // The patch names a similarity for a rename or a copy only.
        similarity: if moved { raw.score } else { None },
        old_mode: modes.then_some(raw.old_mode),
        new_mode: modes.then(|| raw.new_mode),
        threads: Vec::new(),
        blob,
    }
}

// -- one file ------------------------------------------------------------------

/// Page `from` of the file at `path` (moved from `old_path`) in `base`...`head`
/// of the repository `entity_id`: at the checkout when it holds both ends
/// (whitespace ignored when asked), else out of the source's compare that
/// [`range_files`] kept, or read again by the two ends where it was let go.
/// The ends are what the list answered as its `range`, or the names it was
/// asked by where it settled none.
///
/// # Errors
///
/// [`NotFound`](crate::IpcErrorCode::NotFound) where the diff holds no such
/// file; otherwise as [`range_diff`](super::range_diff).
#[allow(clippy::too_many_arguments)]
pub async fn range_file(
    pool: &PgPool,
    secrets: &Arc<dyn knobas_secrets::SecretStore>,
    registry: &Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    live: &crate::sources::live::LiveSources,
    entity_id: &str,
    base: &str,
    head: &str,
    path: &str,
    old_path: Option<&str>,
    ignore_whitespace: bool,
    from: u32,
) -> Result<RangeFilePage, IpcError> {
    let missing = || {
        IpcError::not_found(format!(
            "the diff from {base} to {head} changes no file {path}"
        ))
    };
    if let Some(repo) = checkout_of(pool, entity_id).await
        && let Some(file) = file(&repo, base, head, path, old_path, ignore_whitespace).await
    {
        return file.map(|file| page(&file, from)).ok_or_else(missing);
    }
    let diff = match kept().get(entity_id, base, head) {
        Some(diff) => diff,
        None => {
            let diff = Arc::new(
                super::source_range_diff(pool, secrets, registry, live, entity_id, base, head)
                    .await?,
            );
            kept().put(entity_id, base, head, &diff);
            diff
        }
    };
    diff.files
        .iter()
        .find(|file| file.path == path && file.old_path.as_deref() == old_path)
        .or_else(|| diff.files.iter().find(|file| file.path == path))
        .map(|file| page(file, from))
        .ok_or_else(missing)
}

/// The checkout of `entity_id`, or of its repository for a branch.
async fn checkout_of(pool: &PgPool, entity_id: &str) -> Option<std::path::PathBuf> {
    if let Some(path) = checkout_path(pool, entity_id).await {
        return Some(path);
    }
    let repo = repo_of(pool, entity_id).await.ok()?.entity_id?;
    checkout_path(pool, &repo).await
}

/// One file of `base`..`head` at the checkout `repo`: `git diff -M -- <path>
/// [<old path>]`, parsed. `None` where the checkout cannot answer (so the
/// source is asked); `Some(None)` where it answered and the diff holds no
/// such file.
pub async fn file(
    repo: &Path,
    base: &str,
    head: &str,
    path: &str,
    old_path: Option<&str>,
    ignore_whitespace: bool,
) -> Option<Option<DiffFile>> {
    let all = async {
        let Ends { git, old, new, .. } = ends(repo, base, head).await?;
        let mut command = git_read(&git, repo);
        command.args([
            "-c",
            "diff.autoRefreshIndex=false",
            "diff",
            "--no-ext-diff",
            "--no-color",
            "--no-textconv",
            "-M",
            "--src-prefix=a/",
            "--dst-prefix=b/",
        ]);
        if ignore_whitespace {
            command.arg("-w");
        }
        // Literal paths: a name is the file's, never a pattern.
        command.env("GIT_LITERAL_PATHSPECS", "1");
        command.args([old.as_str(), new.as_str(), "--", path]);
        if let Some(old_path) = old_path.filter(|o| *o != path) {
            command.arg(old_path);
        }
        let (text, cut) = run_capped(command, READ_LINES, READ_BYTES).await.ok()?;
        let (mut files, _) = parse_view(&String::from_utf8_lossy(&text));
        if cut && let Some(last) = files.last_mut() {
            last.truncated = true;
        }
        Some(
            files
                .iter()
                .position(|f| f.path == path)
                .map(|at| files.swap_remove(at)),
        )
    };
    tokio::time::timeout(super::range::TIMEOUT, all)
        .await
        .ok()
        .flatten()
}

/// Page `from` of `file`'s lines: up to [`PAGE_LINES`] of them from line
/// `from`, counted over all its hunks; a hunk the page starts inside goes
/// on from the last page (`continues`).
#[must_use]
pub fn page(file: &DiffFile, from: u32) -> RangeFilePage {
    let from = from as usize;
    let end = from + PAGE_LINES as usize;
    let mut hunks = Vec::new();
    let mut continues = false;
    let mut at = 0usize;
    let mut total = 0usize;
    for hunk in &file.hunks {
        let (start, stop) = (at, at + hunk.lines.len());
        at = stop;
        total = stop;
        if stop <= from || start >= end {
            continue;
        }
        let (a, b) = (from.max(start) - start, end.min(stop) - start);
        if a > 0 {
            continues = true;
        }
        hunks.push(Hunk {
            header: hunk.header.clone(),
            lines: hunk.lines[a..b].to_vec(),
        });
    }
    let next = (end < total).then(|| u32::try_from(end).unwrap_or(u32::MAX));
    RangeFilePage {
        file: DiffFile {
            hunks,
            // Only the last page says the ceiling cut the file.
            truncated: file.truncated && next.is_none(),
            ..listed(file)
        },
        from: u32::try_from(from).unwrap_or(u32::MAX),
        continues,
        next,
    }
}

// -- what the source answered, kept --------------------------------------------

/// The source's compares the page is reading files of: the last few, whole,
/// under the ends each was asked by and the shas it settled. Small: a page
/// reads one compare at a time, and the view cache keeps a compare between
/// two shas for the session anyway (ADR-0032), so one let go costs a read of
/// that cache, not of the source.
struct Kept(Mutex<VecDeque<((String, String, String), Arc<RangeDiff>)>>);

const KEPT: usize = 4;

fn kept() -> &'static Kept {
    static KEPT_ONES: OnceLock<Kept> = OnceLock::new();
    KEPT_ONES.get_or_init(|| Kept(Mutex::new(VecDeque::new())))
}

impl Kept {
    fn get(&self, entity: &str, base: &str, head: &str) -> Option<Arc<RangeDiff>> {
        let mut all = self.0.lock().expect("the kept compares");
        let at = all
            .iter()
            .position(|((e, b, h), _)| e == entity && b == base && h == head)?;
        let found = all.remove(at)?;
        let diff = Arc::clone(&found.1);
        all.push_front(found);
        Some(diff)
    }

    fn put(&self, entity: &str, base: &str, head: &str, diff: &Arc<RangeDiff>) {
        let mut all = self.0.lock().expect("the kept compares");
        all.retain(|((e, b, h), _)| !(e == entity && b == base && h == head));
        all.push_front((
            (entity.to_owned(), base.to_owned(), head.to_owned()),
            Arc::clone(diff),
        ));
        all.truncate(KEPT);
    }
}

#[cfg(test)]
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

    /// A range with every kind of change the parser tells apart: an edit,
    /// an addition, a deletion, a rename with an edit, a pure rename, a
    /// binary file, a mode change, an empty file added, a path with a space
    /// and one that is not ASCII, and a file of 5,000 changed lines.
    fn repo() -> (tempfile::TempDir, String, String) {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        git(p, &["init", "-q"]);
        let lines = |n: usize, tag: &str| {
            (0..n)
                .map(|i| format!("{tag} line {i}\n"))
                .collect::<String>()
        };
        std::fs::create_dir_all(p.join("src/deep")).unwrap();
        std::fs::write(p.join("edit.rs"), lines(40, "edit")).unwrap();
        std::fs::write(p.join("gone.txt"), lines(5, "gone")).unwrap();
        std::fs::write(p.join("src/moved.rs"), lines(30, "moved")).unwrap();
        std::fs::write(p.join("src/same.rs"), lines(30, "same")).unwrap();
        std::fs::write(p.join("blob.bin"), [0u8, 1, 2, 3, 0, 255]).unwrap();
        std::fs::write(p.join("run.sh"), "echo hi\n").unwrap();
        std::fs::write(p.join("big.txt"), lines(3000, "big")).unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-q", "-m", "base"]);
        let base = git(p, &["rev-parse", "HEAD"]);
        let mut edit = lines(40, "edit");
        edit = edit
            .replace("edit line 3\n", "edit line three\n")
            .replace("edit line 30\n", "");
        std::fs::write(p.join("edit.rs"), edit).unwrap();
        std::fs::remove_file(p.join("gone.txt")).unwrap();
        std::fs::write(p.join("added.md"), "# new\n").unwrap();
        std::fs::write(p.join("empty.txt"), "").unwrap();
        git(p, &["mv", "src/moved.rs", "src/deep/moved.rs"]);
        std::fs::write(
            p.join("src/deep/moved.rs"),
            lines(30, "moved").replace("moved line 9\n", "moved line nine\n"),
        )
        .unwrap();
        git(p, &["mv", "src/same.rs", "src/renamed.rs"]);
        std::fs::write(p.join("blob.bin"), [0u8, 9, 9, 9, 0, 255]).unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(p.join("run.sh"), std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }
        std::fs::write(p.join("with space.txt"), "a b\n").unwrap();
        std::fs::write(p.join("ünï.txt"), "ü\n").unwrap();
        std::fs::write(p.join("big.txt"), lines(5000, "bigger")).unwrap();
        git(p, &["add", "-A"]);
        git(p, &["commit", "-q", "-m", "head"]);
        let head = git(p, &["rev-parse", "HEAD"]);
        (dir, base, head)
    }

    fn whole(dir: &Path, base: &str, head: &str) -> Vec<DiffFile> {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "core.quotepath=true",
                "diff",
                "--no-ext-diff",
                "--no-color",
                "--no-textconv",
                "-M",
                "--src-prefix=a/",
                "--dst-prefix=b/",
                base,
                head,
                "--",
            ])
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        knobas_source::diff::parse_view(&String::from_utf8_lossy(&out.stdout)).0
    }

    #[tokio::test]
    async fn the_list_is_the_whole_reads_files_less_their_lines() {
        let (dir, base, head) = repo();
        let all = whole(dir.path(), &base, &head);
        let listed = stat(dir.path(), &base, &head, false)
            .await
            .expect("the checkout answers");
        assert!(!listed.truncated);
        assert_eq!(listed.base.as_deref(), Some(base.as_str()));
        assert_eq!(listed.head, head);
        let less: Vec<DiffFile> = all.iter().map(super::listed).collect();
        assert_eq!(
            listed.files, less,
            "every file, in the whole read's order, as it parses it"
        );
        assert!(
            listed
                .files
                .iter()
                .any(|f| f.change == DiffChange::Renamed && f.similarity == Some(100))
        );
        assert!(listed.files.iter().any(|f| f.change == DiffChange::Binary));
        assert!(listed.files.iter().any(|f| f.path == "run.sh"
            && f.new_mode.as_deref() == Some("100755")
            && f.blob.is_none()));
    }

    #[tokio::test]
    async fn two_tips_list_what_their_three_dot_compare_changes_with_its_commits() {
        let (dir, base, head) = repo();
        let p = dir.path();
        // `main` moves on beside the branch: the compare is from where they parted.
        git(p, &["checkout", "-q", "-b", "side", &base]);
        std::fs::write(p.join("side.txt"), "only on main\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-q", "-m", "main moves on"]);
        let tip = git(p, &["rev-parse", "HEAD"]);
        let listed = stat_between(p, &tip, &head, false)
            .await
            .expect("the checkout answers");
        assert_eq!(
            listed.base.as_deref(),
            Some(base.as_str()),
            "the merge base"
        );
        let less: Vec<DiffFile> = whole(p, &base, &head).iter().map(super::listed).collect();
        assert_eq!(listed.files, less);
        assert!(listed.files.iter().all(|f| f.path != "side.txt"));
        assert_eq!(
            listed
                .commits
                .iter()
                .map(|c| c.title.as_str())
                .collect::<Vec<_>>(),
            ["head"]
        );
        assert_eq!(listed.commits[0].sha, head);
        // An end the checkout does not hold is no answer, so the source is asked.
        assert!(
            stat_between(p, &"a".repeat(40), &head, false)
                .await
                .is_none()
        );
        assert!(stat(p, &"b".repeat(40), &head, false).await.is_none());
    }

    #[tokio::test]
    async fn a_files_pages_put_together_are_its_lines_in_the_whole_read() {
        let (dir, base, head) = repo();
        for want in whole(dir.path(), &base, &head) {
            let got = file(
                dir.path(),
                &base,
                &head,
                &want.path,
                want.old_path.as_deref(),
                false,
            )
            .await
            .expect("the checkout answers")
            .expect("the file is in the diff");
            assert_eq!(
                got, want,
                "{} read alone is the file the whole read holds",
                want.path
            );
            // Paged and put back together.
            let mut from = Some(0);
            let mut hunks: Vec<Hunk> = Vec::new();
            while let Some(at) = from {
                let p = page(&got, at);
                let mut more = p.file.hunks.into_iter();
                if p.continues {
                    let first = more.next().expect("a continuing page holds lines");
                    hunks
                        .last_mut()
                        .expect("a hunk to go on")
                        .lines
                        .extend(first.lines);
                }
                hunks.extend(more);
                from = p.next;
            }
            assert_eq!(hunks, want.hunks, "{}'s pages are its hunks", want.path);
        }
        let big = whole(dir.path(), &base, &head)
            .into_iter()
            .find(|f| f.path == "big.txt")
            .unwrap();
        let first = page(&big, 0);
        assert_eq!(
            first
                .file
                .hunks
                .iter()
                .map(|h| h.lines.len())
                .sum::<usize>(),
            PAGE_LINES as usize
        );
        assert_eq!(first.next, Some(PAGE_LINES));
        assert!(!first.continues);
        assert!(
            page(&big, PAGE_LINES).continues,
            "the one hunk of big.txt goes on"
        );
    }

    #[test]
    fn a_cut_listing_drops_the_record_it_stopped_in() {
        let out = b":100644 100644 aaaaaaa bbbbbbb M\0a.txt\0:100644 100644 ccccccc ddddddd M\0b.txt\x001\t2\ta.txt\x003\t4\tb.t";
        let (files, dropped) = parse_stat(out, true);
        assert!(dropped);
        assert_eq!(
            files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            ["a.txt"]
        );
        let whole = b":100644 100644 aaaaaaa bbbbbbb M\0a.txt\x001\t2\ta.txt\0";
        let (files, dropped) = parse_stat(whole, false);
        assert!(!dropped);
        assert_eq!((files[0].additions, files[0].deletions), (1, 2));
    }

    #[tokio::test]
    async fn a_list_read_again_is_the_list_kept_and_needs_no_checkout() {
        let (dir, base, head) = repo();
        let p = dir.path().to_path_buf();
        let first = stat(&p, &base, &head, false)
            .await
            .expect("the checkout answers");
        // The checkout gone: the same two shas answer from what was kept, and other ends do not.
        std::fs::remove_dir_all(p.join(".git")).unwrap();
        assert_eq!(stat(&p, &base, &head, false).await, Some(first));
        assert!(
            stat(&p, &base, &head, true).await.is_none(),
            "whitespace ignored is another list"
        );
    }

    #[test]
    fn the_kept_compares_are_the_last_few() {
        let k = Kept(Mutex::new(VecDeque::new()));
        let diff = |n: usize| {
            Arc::new(RangeDiff {
                files: Vec::new(),
                truncated: n % 2 == 0,
                range: None,
                read_at: DiffReadAt::Source,
                predicted_at: None,
            })
        };
        for n in 0..=KEPT {
            k.put("repo", &format!("b{n}"), "h", &diff(n));
        }
        assert!(k.get("repo", "b0", "h").is_none(), "the oldest went");
        assert!(k.get("repo", &format!("b{KEPT}"), "h").is_some());
        assert!(k.get("other", "b1", "h").is_none());
    }
}
