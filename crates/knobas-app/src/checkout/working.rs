//! The working tree as an end of a comparison (ux26, ADR-0048,
//! `docs/decisions/2026-11-20-a-comparison-is-drawn-rendered-and-may-end-at-the-working-tree.md`):
//! the files of a repository's checkout as they stand on disk, uncommitted
//! changes included, with the untracked files git does not ignore, compared
//! with a commit the checkout holds. Written `~working-tree` ([`END`]) where a
//! range command takes a head; `~index` ([`INDEX`]) is an end of the byte
//! reads only (Local changes' halves).
//!
//! **git's own answer, always** (ADR-0043: the working tree stays git's): the
//! in-process reader is never asked a working-tree read. Each command runs
//! under ADR-0036's environment ([`git_read`](super::range::git_read): no
//! optional locks, no fsmonitor, no prompt, stdin closed, the protocols shut,
//! the ceiling at the checkout) and with `diff.autoRefreshIndex` off, so the
//! index is left byte for byte as it was:
//!
//! 1. the names: `git diff --raw -z -M <sha> --`;
//! 2. the names and counts: the same with `--numstat`;
//! 3. the lines: the patch of the literal paths a list named;
//! 4. what is untracked, and what differs from the index:
//!    `git status --porcelain=v2 -z --untracked-files=all`;
//! 5. the index's blobs: `git ls-files -s -z`.
//!
//! `<sha>` is a whole sha the checkout holds, resolved in knobas's process
//! first; a branch name is the merge base of the tip the mirror holds for it
//! and the checkout's `HEAD`.
//!
//! **Kept by the stamp.** Every answer is kept with the checkout's stamp
//! ([`stamp::of`](super::stamp::of)) and the untracked files' own metadata,
//! taken before git ran: an ask whose stamp matches answers with no git, and
//! a moved stamp drops all of that checkout's answers. Four checkouts are
//! kept. A checkout the stamp cannot read is read every time.

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use knobas_core::checkout::open::is_sha;
use knobas_source::diff::{DiffChange, DiffFile, DiffLine, DiffLineKind, Hunk, parse_view};
use serde::Serialize;

use super::range::git_read;
use super::range_files::{RangeFileAsk, parse_raw, parse_stat};
use super::reader::{Form, Reader};
use super::uncommitted::{MAX_UNTRACKED, READ_BYTES, READ_LINES, run_capped};
use super::working_tree::git_on_path;

/// The working tree, as an end: a name git refuses for a ref
/// (`git check-ref-format`), so no branch can be called it.
pub const END: &str = "~working-tree";
/// The index, as an end of a byte read (Local changes' halves).
pub const INDEX: &str = "~index";

/// Whether `end` is the working tree's token.
#[must_use]
pub fn is_end(end: &str) -> bool {
    end == END
}

/// An untracked file over this many bytes, or with a NUL in its first
/// [`SNIFF`] bytes, is listed as binary: no lines.
pub const TEXT_CAP: u64 = 2 * 1024 * 1024;
/// How far a file is looked into for a NUL, as git looks.
const SNIFF: usize = 8_000;

/// Where the working tree stood when an answer was read: what the page keeps
/// as its epoch, and what Compare's line says about the checkout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct WorkingAt {
    /// The checkout's stamp, as hex: the same while nothing in it changed.
    pub stamp: String,
    /// `HEAD`'s whole sha; `None` before the first commit.
    pub head: Option<String>,
    /// The branch `HEAD` is on; `None` for a detached head.
    pub branch: Option<String>,
    /// The listed files that are untracked.
    pub untracked: Vec<String>,
    /// More files were untracked than knobas lists ([`MAX_UNTRACKED`]).
    pub untracked_truncated: bool,
}

/// Why the working tree could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// No `git` on this machine.
    NoGit,
    /// The checkout does not hold the base: the words say what to do.
    Missing(String),
    /// git ran and did not answer.
    Failed(String),
}

impl Refused {
    /// As the page shows it.
    #[must_use]
    pub fn to_ipc(&self) -> crate::IpcError {
        match self {
            Self::NoGit => crate::IpcError::not_ready(
                "git was not found on this machine, so knobas cannot read the working tree.",
            ),
            Self::Missing(why) => crate::IpcError::not_found(why),
            Self::Failed(why) => {
                crate::IpcError::internal(format!("git could not read the working tree: {why}"))
            }
        }
    }
}

// -- the base ---------------------------------------------------------------

/// The base as git is to name it, and its whole sha (`None`: the empty tree).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Base {
    pub git: String,
    pub sha: Option<String>,
}

/// A sha base (7 to 64 hex digits, one `^` allowed) resolved at the checkout
/// in knobas's own process; `<root>^` is the empty tree, refused in a
/// shallow clone.
///
/// # Errors
///
/// [`Refused::Missing`] where the checkout does not hold it: a sha nobody
/// names is never fetched (ADR-0036).
pub async fn sha_base(path: &Path, base: &str) -> Result<Base, Refused> {
    let (stem, caret) = match base.strip_suffix('^') {
        Some(stem) => (stem, true),
        None => (base, false),
    };
    let short = &stem[..stem.len().min(7)];
    let missing = || {
        Refused::Missing(format!(
            "Your clone does not hold {short}. Fetch it in your terminal, then come back."
        ))
    };
    if !is_sha(stem) {
        return Err(missing());
    }
    let reader = Reader::at(path);
    let whole = reader.commit(stem).await.ok_or_else(missing)?;
    if !caret {
        return Ok(Base {
            git: whole.clone(),
            sha: Some(whole),
        });
    }
    match reader.commit(&format!("{whole}^")).await {
        Some(parent) => Ok(Base {
            git: parent.clone(),
            sha: Some(parent),
        }),
        None => reader
            .empty_tree_if_whole()
            .await
            .map(|tree| Base { git: tree, sha: None })
            .ok_or_else(|| Refused::Missing(format!("{short} has no parent in this clone: it is shallow, so the commit before it is not here."))),
    }
}

/// A branch's base: the merge base of `tip` (the tip the mirror holds for
/// `name`) and the checkout's `HEAD`, in process; the tip itself where `HEAD`
/// is unborn or the two share no history.
///
/// # Errors
///
/// [`Refused::Missing`] where the checkout lacks the tip (the caller has it
/// fetched behind, ADR-0041).
pub async fn branch_base(path: &Path, name: &str, tip: &str) -> Result<Base, Refused> {
    let reader = Reader::at(path);
    let Some(tip) = reader.commit(tip).await else {
        return Err(Refused::Missing(format!(
            "Your clone does not hold {name}'s tip yet: it is being fetched."
        )));
    };
    let from = match reader.commit("HEAD").await {
        Some(head) => reader.merge_base(&tip, &head).await.unwrap_or(tip),
        None => tip,
    };
    Ok(Base {
        git: from.clone(),
        sha: Some(from),
    })
}

// -- what is kept -------------------------------------------------------------

/// `git status`'s answer, as the reads here use it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    /// Untracked files git does not ignore, in git's order, at most [`MAX_UNTRACKED`].
    pub untracked: Vec<String>,
    pub untracked_truncated: bool,
    /// Tracked files whose working copy differs from the index (or is gone).
    pub changed: HashSet<String>,
}

/// One checkout's answers, all read at one stamp.
#[derive(Default)]
struct Answers {
    stamp: u64,
    sig: u64,
    status: Option<Status>,
    index: Option<HashMap<String, (String, String)>>,
    head: Option<(Option<String>, Option<String>)>,
    names: HashMap<String, (Vec<DiffFile>, bool)>,
    counted: HashMap<String, (Vec<DiffFile>, bool)>,
    files: HashMap<String, Option<DiffFile>>,
    lines: usize,
}

/// The most changed lines kept a checkout, the oldest pages let go first.
const KEPT_LINES: usize = 60_000;
/// The checkouts kept.
const CHECKOUTS: usize = 4;

struct Kept(Mutex<VecDeque<(PathBuf, Answers)>>);

fn kept() -> &'static Kept {
    static KEPT: OnceLock<Kept> = OnceLock::new();
    KEPT.get_or_init(|| Kept(Mutex::new(VecDeque::new())))
}

/// The stamp of `path` and the untracked files' own metadata, read off the
/// runtime: `None` where the stamp cannot read the checkout.
async fn stamp_of(path: &Path, untracked: Vec<String>) -> Option<(u64, u64)> {
    let at = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let stamp = super::stamp::of(&at)?;
        Some((stamp, untracked_sig(&at, &untracked)))
    })
    .await
    .ok()
    .flatten()
}

/// The untracked files and their folders, by metadata: the stamp reads no
/// untracked file's content, and nothing inside an untracked folder.
fn untracked_sig(root: &Path, untracked: &[String]) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    let mut folders = HashSet::new();
    for rel in untracked {
        let full = root.join(rel);
        meta_into(&full, &mut hash);
        if let Some(parent) = Path::new(rel).parent()
            && folders.insert(parent.to_path_buf())
        {
            meta_into(&root.join(parent), &mut hash);
        }
    }
    hash.finish()
}

fn meta_into(path: &Path, hash: &mut impl Hasher) {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            meta.len().hash(hash);
            meta.modified().ok().hash(hash);
            meta.is_dir().hash(hash);
        }
        Err(_) => 0u8.hash(hash),
    }
}

/// The answers kept for `path` at its stamp now, through `with`; a moved
/// stamp starts them over. `None` from the stamp: nothing kept.
async fn with_kept<T>(path: &Path, with: impl FnOnce(&mut Answers) -> T) -> Option<T> {
    let untracked = {
        let all = kept().0.lock().expect("the working tree's answers");
        all.iter()
            .find(|(p, _)| p == path)
            .and_then(|(_, a)| a.status.as_ref())
            .map(|s| s.untracked.clone())
            .unwrap_or_default()
    };
    let (stamp, sig) = stamp_of(path, untracked).await?;
    let mut all = kept().0.lock().expect("the working tree's answers");
    let at = all.iter().position(|(p, _)| p == path);
    let mut answers = match at.and_then(|at| all.remove(at)) {
        Some((_, a)) if a.stamp == stamp && a.sig == sig => a,
        _ => Answers {
            stamp,
            sig,
            ..Answers::default()
        },
    };
    let out = with(&mut answers);
    all.push_front((path.to_path_buf(), answers));
    all.truncate(CHECKOUTS);
    Some(out)
}

/// The stamp as the page keeps it.
fn hex(stamp: u64) -> String {
    format!("{stamp:016x}")
}

// -- the reads ------------------------------------------------------------------

async fn git(path: &Path) -> Result<PathBuf, Refused> {
    let git = git_on_path().await.map_err(|_| Refused::NoGit)?;
    if super::working_tree::is_dir_off_runtime(path).await {
        Ok(git)
    } else {
        Err(Refused::Failed(format!("{} is not there", path.display())))
    }
}

/// `git status --porcelain=v2 -z --untracked-files=all`, parsed.
async fn read_status(path: &Path) -> Result<Status, Refused> {
    let git = git(path).await?;
    let mut command = git_read(&git, path);
    command.args(["status", "--porcelain=v2", "-z", "--untracked-files=all"]);
    let (out, cut) = run_capped(command, usize::MAX, READ_BYTES)
        .await
        .map_err(|why| Refused::Failed(format!("{why:?}")))?;
    Ok(parse_status(&out, cut))
}

/// `git status --porcelain=v2 -z` output, as [`Status`].
#[must_use]
pub fn parse_status(out: &[u8], cut: bool) -> Status {
    let mut status = Status {
        untracked_truncated: cut,
        ..Status::default()
    };
    let mut entries: Vec<&[u8]> = out.split(|b| *b == 0).collect();
    if cut {
        entries.pop();
    }
    let mut entries = entries.into_iter();
    while let Some(entry) = entries.next() {
        let text = String::from_utf8_lossy(entry);
        if let Some(name) = text.strip_prefix("? ") {
            if status.untracked.len() == MAX_UNTRACKED {
                status.untracked_truncated = true;
                continue;
            }
            status.untracked.push(name.to_owned());
        } else if let Some(rest) = text.strip_prefix("1 ") {
            // `1 XY sub mH mI mW hH hI path`
            let fields: Vec<&str> = rest.splitn(8, ' ').collect();
            if let (Some(xy), Some(path)) = (fields.first(), fields.get(7))
                && xy.as_bytes().get(1).is_some_and(|y| *y != b'.')
            {
                status.changed.insert((*path).to_owned());
            }
        } else if let Some(rest) = text.strip_prefix("2 ") {
            // `2 XY sub mH mI mW hH hI X<score> path`, then the original path.
            let fields: Vec<&str> = rest.splitn(9, ' ').collect();
            if let (Some(xy), Some(path)) = (fields.first(), fields.get(8))
                && xy.as_bytes().get(1).is_some_and(|y| *y != b'.')
            {
                status.changed.insert((*path).to_owned());
            }
            entries.next();
        } else if let Some(rest) = text.strip_prefix("u ") {
            // Unmerged: `u XY sub m1 m2 m3 mW h1 h2 h3 path`.
            if let Some(path) = rest.splitn(10, ' ').nth(9) {
                status.changed.insert(path.to_owned());
            }
        }
    }
    status
}

/// `git ls-files -s -z`, parsed: each tracked path's mode and blob (stage 0,
/// or the first stage of an unmerged one).
async fn read_index(path: &Path) -> Result<HashMap<String, (String, String)>, Refused> {
    let git = git(path).await?;
    let mut command = git_read(&git, path);
    command.args(["ls-files", "-s", "-z"]);
    let (out, cut) = run_capped(command, usize::MAX, 64 * 1024 * 1024)
        .await
        .map_err(|why| Refused::Failed(format!("{why:?}")))?;
    if cut {
        return Err(Refused::Failed(
            "the index lists more than knobas reads".to_owned(),
        ));
    }
    let mut index = HashMap::new();
    for entry in out.split(|b| *b == 0) {
        let text = String::from_utf8_lossy(entry);
        // `<mode> <blob> <stage>\t<path>`
        let Some((meta, name)) = text.split_once('\t') else {
            continue;
        };
        let mut parts = meta.split(' ');
        if let (Some(mode), Some(blob)) = (parts.next(), parts.next()) {
            index
                .entry(name.to_owned())
                .or_insert_with(|| (mode.to_owned(), blob.to_owned()));
        }
    }
    Ok(index)
}

/// The status, kept by the stamp.
pub async fn status(path: &Path) -> Result<Status, Refused> {
    if let Some(Some(status)) = with_kept(path, |a| a.status.clone()).await {
        return Ok(status);
    }
    let read = read_status(path).await?;
    // Kept under the stamp read after it, with the untracked files it named.
    let untracked = read.untracked.clone();
    if let Some((stamp, sig)) = stamp_of(path, untracked).await {
        let mut all = kept().0.lock().expect("the working tree's answers");
        let at = all.iter().position(|(p, _)| p == path);
        let mut answers = match at.and_then(|at| all.remove(at)) {
            Some((_, a)) if a.stamp == stamp => Answers { sig, ..a },
            _ => Answers {
                stamp,
                sig,
                ..Answers::default()
            },
        };
        answers.status = Some(read.clone());
        all.push_front((path.to_path_buf(), answers));
        all.truncate(CHECKOUTS);
    }
    Ok(read)
}

/// `HEAD`'s whole sha and its branch, read in process (the checkout's own
/// `HEAD` file for the branch).
async fn head_of(path: &Path) -> (Option<String>, Option<String>) {
    if let Some(Some(head)) = with_kept(path, |a| a.head.clone()).await {
        return head;
    }
    let sha = Reader::at(path).commit("HEAD").await;
    let branch = tokio::fs::read_to_string(path.join(".git").join("HEAD"))
        .await
        .ok()
        .and_then(|text| {
            text.trim()
                .strip_prefix("ref: refs/heads/")
                .map(str::to_owned)
        });
    let head = (sha, branch);
    let _ = with_kept(path, |a| a.head = Some(head.clone())).await;
    head
}

/// Where the working tree stands, for an answer.
async fn at_now(path: &Path, status: &Status) -> WorkingAt {
    let (head, branch) = head_of(path).await;
    let stamp = stamp_of(path, status.untracked.clone()).await.map_or_else(
        || "unstamped".to_owned(),
        |(stamp, sig)| hex(stamp ^ sig.rotate_left(1)),
    );
    WorkingAt {
        stamp,
        head,
        branch,
        untracked: status.untracked.clone(),
        untracked_truncated: status.untracked_truncated,
    }
}

/// git's diff of the working tree against `base`, in `form`, with no paths
/// or the literal `paths`: never in process.
async fn diff(
    path: &Path,
    form: Form,
    base: &str,
    ignore_whitespace: bool,
    paths: &[&str],
    max_lines: usize,
) -> Result<(Vec<u8>, bool), Refused> {
    let search = std::env::var_os("PATH").unwrap_or_default();
    git(path).await?;
    Reader::git_alone(&search, path)
        .diff(
            form,
            base,
            None,
            ignore_whitespace,
            paths,
            max_lines,
            READ_BYTES,
        )
        .await
        .ok_or_else(|| Refused::Failed("git diff did not answer".to_owned()))
}

/// An untracked file as listed: added, with no lines and no counts yet.
fn untracked_listed(rel: &str) -> DiffFile {
    DiffFile {
        path: rel.to_owned(),
        old_path: None,
        change: DiffChange::Added,
        additions: 0,
        deletions: 0,
        hunks: Vec::new(),
        truncated: false,
        similarity: None,
        old_mode: None,
        new_mode: None,
        threads: Vec::new(),
        blob: None,
    }
}

/// The diff's files and the untracked ones, by path; an untracked path the
/// diff lists already (taken out of the index and left on disk) once.
fn with_untracked(
    mut files: Vec<DiffFile>,
    status: &Status,
    counted: Option<&HashMap<String, Option<u32>>>,
) -> Vec<DiffFile> {
    let listed: HashSet<String> = files.iter().map(|f| f.path.clone()).collect();
    for rel in &status.untracked {
        if listed.contains(rel) || rel.ends_with('/') {
            continue;
        }
        let mut file = untracked_listed(rel);
        if let Some(counts) = counted {
            match counts.get(rel).copied().flatten() {
                Some(lines) => file.additions = lines,
                None => file.change = DiffChange::Binary,
            }
        }
        files.push(file);
    }
    files.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
    files
}

/// The files of git's `--raw` list that changed: with `diff.autoRefreshIndex`
/// off (it must be, or git rewrites the index) a file only touched is listed
/// with no content change (Git for Windows with `core.autocrlf`, witnessed
/// 2026-11-20), which the counted read then leaves out, so the names would
/// draw a row that goes. A file whose working copy is the index's (`status`
/// names it not) and whose index blob is the base's blob changed nothing.
fn only_changed(
    files: Vec<DiffFile>,
    raw: &[u8],
    status: &Status,
    index: &HashMap<String, (String, String)>,
) -> Vec<DiffFile> {
    // Each listed path's blob on the base's side, from the raw records themselves (the parsed files keep only one blob).
    let text = String::from_utf8_lossy(raw);
    let mut base_blob: HashMap<String, String> = HashMap::new();
    let mut tokens = text.split('\0').map(|t| t.trim_start_matches('\n'));
    while let Some(token) = tokens.next() {
        let Some(meta) = token.strip_prefix(':') else {
            continue;
        };
        let parts: Vec<&str> = meta.split(' ').collect();
        let (Some(old_blob), Some(new_blob), Some(letter)) = (
            parts.get(2),
            parts.get(3),
            parts.get(4).and_then(|s| s.bytes().next()),
        ) else {
            break;
        };
        let n = if matches!(letter, b'R' | b'C') { 2 } else { 1 };
        let paths: Vec<&str> = tokens.by_ref().take(n).collect();
        if letter == b'M'
            && n == 1
            && new_blob.bytes().all(|b| b == b'0')
            && let Some(path) = paths.first()
        {
            base_blob.insert((*path).to_owned(), (*old_blob).to_owned());
        }
    }
    files
        .into_iter()
        .filter(|file| {
            let Some(old) = base_blob.get(&file.path) else {
                return true;
            };
            let same_as_index = !status.changed.contains(&file.path);
            let index_is_base = index
                .get(&file.path)
                .is_some_and(|(_, blob)| !old.is_empty() && blob.starts_with(old.as_str()));
            !(same_as_index && index_is_base)
        })
        .collect()
}

/// The working tree's files against `base`, with no counts: what the page
/// draws first.
///
/// # Errors
///
/// As [`Refused`].
pub async fn names(path: &Path, base: &str) -> Result<(Vec<DiffFile>, bool, WorkingAt), Refused> {
    let key = base.to_owned();
    if let Some(Some((files, cut))) = with_kept(path, |a| a.names.get(&key).cloned()).await {
        let status = status(path).await?;
        return Ok((files, cut, at_now(path, &status).await));
    }
    let (listed, status, index) = tokio::join!(
        diff(path, Form::Raw, base, false, &[], usize::MAX),
        status(path),
        index(path)
    );
    let ((out, cut), status, index) = (listed?, status?, index?);
    let (files, cut) = parse_raw(&out, cut);
    let files = with_untracked(only_changed(files, &out, &status, &index), &status, None);
    let _ = with_kept(path, |a| a.names.insert(key, (files.clone(), cut))).await;
    Ok((files, cut, at_now(path, &status).await))
}

/// The working tree's files against `base` with their counts: an untracked
/// file's are its lines, binary past [`TEXT_CAP`] or with a NUL.
///
/// # Errors
///
/// As [`Refused`].
pub async fn counted(
    path: &Path,
    base: &str,
    ignore_whitespace: bool,
) -> Result<(Vec<DiffFile>, bool, WorkingAt), Refused> {
    let key = format!("{base}\n{ignore_whitespace}");
    if let Some(Some((files, cut))) = with_kept(path, |a| a.counted.get(&key).cloned()).await {
        let status = status(path).await?;
        return Ok((files, cut, at_now(path, &status).await));
    }
    let (listed, status) = tokio::join!(
        diff(
            path,
            Form::RawNumstat,
            base,
            ignore_whitespace,
            &[],
            usize::MAX
        ),
        status(path)
    );
    let ((out, cut), status) = (listed?, status?);
    let (files, cut_inside) = parse_stat(&out, cut);
    let counts = untracked_counts(path, &status.untracked).await;
    let files = with_untracked(files, &status, Some(&counts));
    let cut = cut || cut_inside;
    let _ = with_kept(path, |a| a.counted.insert(key, (files.clone(), cut))).await;
    Ok((files, cut, at_now(path, &status).await))
}

/// Each untracked file's line count, `None` for a binary one.
async fn untracked_counts(root: &Path, untracked: &[String]) -> HashMap<String, Option<u32>> {
    let (root, untracked) = (root.to_path_buf(), untracked.to_vec());
    tokio::task::spawn_blocking(move || {
        untracked
            .iter()
            .map(|rel| {
                (
                    rel.clone(),
                    text_of(&root, rel).map(|text| line_count(&text)),
                )
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

fn line_count(text: &str) -> u32 {
    u32::try_from(text.lines().count()).unwrap_or(u32::MAX)
}

/// An untracked file's text, or `None` where it is binary, too big or not a
/// plain file.
fn text_of(root: &Path, rel: &str) -> Option<String> {
    let full = safe_join(root, rel)?;
    let meta = std::fs::symlink_metadata(&full).ok()?;
    if !meta.is_file() || meta.len() > TEXT_CAP {
        return None;
    }
    let bytes = std::fs::read(&full).ok()?;
    if bytes.iter().take(SNIFF).any(|b| *b == 0) {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// An untracked file with its lines: all added.
fn untracked_file(root: &Path, rel: &str) -> DiffFile {
    let mut file = untracked_listed(rel);
    let Some(text) = text_of(root, rel) else {
        file.change = DiffChange::Binary;
        return file;
    };
    let lines: Vec<DiffLine> = text
        .lines()
        .enumerate()
        .map(|(n, line)| DiffLine {
            kind: DiffLineKind::Add,
            old: None,
            new: Some(u32::try_from(n + 1).unwrap_or(u32::MAX)),
            text: line.to_owned(),
        })
        .collect();
    file.additions = u32::try_from(lines.len()).unwrap_or(u32::MAX);
    if !lines.is_empty() {
        file.hunks.push(Hunk {
            header: format!("@@ -0,0 +1,{} @@", lines.len()),
            lines,
        });
    }
    file
}

/// `asks` of the working tree against `base`, each as its own read draws it,
/// in the order asked: the tracked ones in one `git diff -- <paths>`, an
/// untracked one from disk. `None` in a file's place where the diff holds
/// no such file.
///
/// # Errors
///
/// As [`Refused`].
pub async fn files(
    path: &Path,
    base: &str,
    asks: &[RangeFileAsk],
    ignore_whitespace: bool,
) -> Result<Vec<Option<DiffFile>>, Refused> {
    let key = |ask: &RangeFileAsk| {
        format!(
            "{base}\n{ignore_whitespace}\n{}\n{}",
            ask.path,
            ask.old_path.as_deref().unwrap_or("")
        )
    };
    let had: Vec<Option<Option<DiffFile>>> = with_kept(path, |a| {
        asks.iter()
            .map(|ask| a.files.get(&key(ask)).cloned())
            .collect()
    })
    .await
    .unwrap_or_else(|| vec![None; asks.len()]);
    if had.iter().all(Option::is_some) {
        return Ok(had.into_iter().map(Option::flatten).collect());
    }
    let status = status(path).await?;
    let untracked: HashSet<&str> = status.untracked.iter().map(String::as_str).collect();
    let mut paths: Vec<&str> = Vec::new();
    for (ask, kept) in asks.iter().zip(&had) {
        if kept.is_some() || untracked.contains(ask.path.as_str()) {
            continue;
        }
        paths.push(&ask.path);
        if let Some(old) = ask.old_path.as_deref().filter(|o| *o != ask.path) {
            paths.push(old);
        }
    }
    let mut parsed = if paths.is_empty() {
        Vec::new()
    } else {
        let (text, cut) = diff(
            path,
            Form::Patch,
            base,
            ignore_whitespace,
            &paths,
            READ_LINES,
        )
        .await?;
        let (mut parsed, _) = parse_view(&String::from_utf8_lossy(&text));
        if cut {
            parsed.pop();
        }
        parsed
    };
    let root = path.to_path_buf();
    let mut out = Vec::with_capacity(asks.len());
    let mut made = Vec::new();
    for (ask, kept) in asks.iter().zip(had) {
        if let Some(kept) = kept {
            out.push(kept);
            continue;
        }
        let file = if untracked.contains(ask.path.as_str()) {
            Some(untracked_file(&root, &ask.path))
        } else {
            parsed
                .iter()
                .position(|f| f.path == ask.path && f.old_path == ask.old_path)
                .or_else(|| parsed.iter().position(|f| f.path == ask.path))
                .map(|at| parsed.swap_remove(at))
        };
        made.push((key(ask), file.clone()));
        out.push(file);
    }
    let _ = with_kept(path, |a| {
        for (key, file) in made {
            a.lines += file
                .as_ref()
                .map_or(0, |f| f.hunks.iter().map(|h| h.lines.len()).sum());
            a.files.insert(key, file);
        }
        // Past the cap the pages go, all of them: they are read again by a batch as the page reaches them.
        if a.lines > KEPT_LINES {
            a.files.clear();
            a.lines = 0;
        }
    })
    .await;
    Ok(out)
}

/// The whole diff of the working tree against `base`, capped as ADR-0026
/// caps the uncommitted diff, with the untracked files' lines.
///
/// # Errors
///
/// As [`Refused`].
pub async fn whole(
    path: &Path,
    base: &str,
    ignore_whitespace: bool,
) -> Result<(Vec<DiffFile>, bool, WorkingAt), Refused> {
    let (patch, status) = tokio::join!(
        diff(path, Form::Patch, base, ignore_whitespace, &[], READ_LINES),
        status(path)
    );
    let ((text, cut), status) = (patch?, status?);
    let (mut files, mut truncated) = parse_view(&String::from_utf8_lossy(&text));
    if cut {
        truncated = true;
        if let Some(last) = files.last_mut() {
            last.truncated = true;
        }
    }
    let listed: HashSet<String> = files.iter().map(|f| f.path.clone()).collect();
    for rel in &status.untracked {
        if !listed.contains(rel) && !rel.ends_with('/') {
            files.push(untracked_file(path, rel));
        }
    }
    files.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
    Ok((
        files,
        truncated || status.untracked_truncated,
        at_now(path, &status).await,
    ))
}

// -- a file's bytes -------------------------------------------------------------

/// `rel` under `root`, where it is a plain relative path: no `..`, no root,
/// no drive, no NUL, and no folder on the way that is a link.
fn safe_join(root: &Path, rel: &str) -> Option<PathBuf> {
    if rel.is_empty() || rel.contains('\0') || rel.contains('\\') {
        return None;
    }
    let mut at = root.to_path_buf();
    let parts: Vec<Component> = Path::new(rel).components().collect();
    for (n, part) in parts.iter().enumerate() {
        let Component::Normal(name) = part else {
            return None;
        };
        at.push(name);
        // A folder on the way that is a link leads out of the checkout.
        if n + 1 < parts.len()
            && std::fs::symlink_metadata(&at)
                .ok()?
                .file_type()
                .is_symlink()
        {
            return None;
        }
    }
    Some(at)
}

/// What a read of a file's bytes found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bytes {
    Read(Vec<u8>),
    /// Over the cap: its size.
    TooLarge(u64),
    /// git lists no such file there (tracked, or untracked and not ignored).
    NotThere,
}

/// The file `rel` of the working tree, up to `cap` bytes: only where git
/// lists it there, tracked or untracked and not ignored; never through a
/// link (a link itself answers its target, as git holds it).
///
/// # Errors
///
/// As [`Refused`].
pub async fn file_bytes(path: &Path, rel: &str, cap: u64) -> Result<Bytes, Refused> {
    let Some(full) = safe_join(path, rel) else {
        return Ok(Bytes::NotThere);
    };
    let (index, status) = tokio::join!(index(path), status(path));
    let (index, status) = (index?, status?);
    if !index.contains_key(rel) && !status.untracked.iter().any(|u| u == rel) {
        return Ok(Bytes::NotThere);
    }
    tokio::task::spawn_blocking(move || read_no_follow(&full, cap))
        .await
        .map_err(|why| Refused::Failed(why.to_string()))
}

fn read_no_follow(full: &Path, cap: u64) -> Bytes {
    let Ok(meta) = std::fs::symlink_metadata(full) else {
        return Bytes::NotThere;
    };
    if meta.file_type().is_symlink() {
        return std::fs::read_link(full).map_or(Bytes::NotThere, |target| {
            Bytes::Read(target.to_string_lossy().into_owned().into_bytes())
        });
    }
    if !meta.is_file() {
        return Bytes::NotThere;
    }
    if meta.len() > cap {
        return Bytes::TooLarge(meta.len());
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // An open of a link fails rather than reading its target.
        options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits().cast_signed());
    }
    let Ok(mut opened) = options.open(full) else {
        return Bytes::NotThere;
    };
    let mut bytes = Vec::new();
    use std::io::Read;
    match (&mut opened).take(cap + 1).read_to_end(&mut bytes) {
        Ok(_) if bytes.len() as u64 > cap => Bytes::TooLarge(bytes.len() as u64),
        Ok(_) => Bytes::Read(bytes),
        Err(_) => Bytes::NotThere,
    }
}

/// The index list, kept by the stamp.
async fn index(path: &Path) -> Result<HashMap<String, (String, String)>, Refused> {
    if let Some(Some(index)) = with_kept(path, |a| a.index.clone()).await {
        return Ok(index);
    }
    let read = read_index(path).await?;
    let _ = with_kept(path, |a| a.index = Some(read.clone())).await;
    Ok(read)
}

/// The file `rel` as the index holds it (its staged blob), up to `cap` bytes.
///
/// # Errors
///
/// As [`Refused`].
pub async fn index_bytes(path: &Path, rel: &str, cap: u64) -> Result<Bytes, Refused> {
    let index = index(path).await?;
    let Some((_, blob)) = index.get(rel) else {
        return Ok(Bytes::NotThere);
    };
    let wanted = usize::try_from(cap).unwrap_or(usize::MAX).saturating_add(1);
    match super::local::blob(path, blob, wanted).await {
        Some((_, true)) => Ok(Bytes::TooLarge(cap + 1)),
        Some((bytes, false)) => Ok(Bytes::Read(bytes)),
        None => Ok(Bytes::NotThere),
    }
}

/// The working tree's status and index for a folder renderer's copy (step 6
/// of the build): which tracked files differ from the index, and each
/// tracked file's blob.
///
/// # Errors
///
/// As [`Refused`].
pub async fn listing(path: &Path) -> Result<(Status, HashMap<String, (String, String)>), Refused> {
    let (status, index) = tokio::join!(status(path), index(path));
    Ok((status?, index?))
}

#[cfg(all(test, unix))]
mod tests {
    //! The working tree's reads against the real `git`, in temp repositories.
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

    fn write(dir: &Path, rel: &str, text: &str) {
        let full = dir.join(rel);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, text).unwrap();
    }

    /// A repository with three files and an ignore rule committed, then: one file
    /// edited, one staged and edited again, one deleted, one renamed with an edit,
    /// an untracked file, an untracked file in a new folder, and an ignored one.
    fn changed() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let at = dir.path();
        git(at, &["init", "-q", "-b", "main"]);
        write(at, ".gitignore", ".env\nbuild/\n");
        write(at, "a.md", "# A\n\nfirst\n");
        write(at, "b.md", "# B\n");
        write(at, "gone.txt", "bye\n");
        write(
            at,
            "docs/long name é.txt",
            (0..30)
                .map(|n| format!("line {n}\n"))
                .collect::<String>()
                .as_str(),
        );
        git(at, &["add", "."]);
        git(at, &["commit", "-q", "-m", "first"]);
        let sha = git(at, &["rev-parse", "HEAD"]);
        write(at, "a.md", "# A\n\nsecond\n");
        write(at, "b.md", "# B staged\n");
        git(at, &["add", "b.md"]);
        write(at, "b.md", "# B staged and edited\n");
        std::fs::remove_file(at.join("gone.txt")).unwrap();
        git(at, &["mv", "docs/long name é.txt", "docs/moved.txt"]);
        write(
            at,
            "docs/moved.txt",
            &(0..30)
                .map(|n| {
                    if n == 3 {
                        "changed\n".to_owned()
                    } else {
                        format!("line {n}\n")
                    }
                })
                .collect::<String>(),
        );
        write(at, "new.md", "# New\n\none\ntwo\n");
        write(at, "fresh/inner.md", "# Inner\n");
        write(at, ".env", "SECRET=1\n");
        write(at, "build/out.txt", "made\n");
        (dir, sha)
    }

    fn paths(files: &[DiffFile]) -> Vec<(String, DiffChange)> {
        files.iter().map(|f| (f.path.clone(), f.change)).collect()
    }

    #[tokio::test]
    async fn the_working_tree_against_a_commit_lists_what_git_diff_lists_and_its_untracked_files() {
        let (dir, sha) = changed();
        let (files, cut, at) = counted(dir.path(), &sha, false).await.unwrap();
        assert!(!cut);
        assert_eq!(
            paths(&files),
            vec![
                ("a.md".to_owned(), DiffChange::Modified),
                ("b.md".to_owned(), DiffChange::Modified),
                ("docs/moved.txt".to_owned(), DiffChange::Renamed),
                ("fresh/inner.md".to_owned(), DiffChange::Added),
                ("gone.txt".to_owned(), DiffChange::Deleted),
                ("new.md".to_owned(), DiffChange::Added),
            ]
        );
        // git's own counts for the tracked ones, the untracked files' lines for theirs.
        let numstat = git(
            dir.path(),
            &[
                "-c",
                "diff.autoRefreshIndex=false",
                "diff",
                "--numstat",
                "-M",
                &sha,
                "--",
                "a.md",
                "b.md",
            ],
        );
        assert_eq!(numstat.lines().count(), 2);
        let new = files.iter().find(|f| f.path == "new.md").unwrap();
        assert_eq!((new.additions, new.deletions), (4, 0));
        assert_eq!(
            at.untracked,
            vec!["fresh/inner.md".to_owned(), "new.md".to_owned()]
        );
        assert_eq!(at.branch.as_deref(), Some("main"));
        assert_eq!(at.head.as_deref(), Some(sha.as_str()));
        // The ignored files are nowhere.
        assert!(
            !files
                .iter()
                .any(|f| f.path.contains(".env") || f.path.starts_with("build/"))
        );
    }

    #[tokio::test]
    async fn a_file_only_touched_is_in_neither_list() {
        let (dir, sha) = changed();
        let at = dir.path();
        write(at, "same.txt", "unchanged\n");
        git(at, &["add", "same.txt"]);
        git(at, &["commit", "-q", "-m", "same"]);
        let head = git(at, &["rev-parse", "HEAD"]);
        // Rewritten with the same bytes and a later time: git's raw list names it, with no change.
        std::thread::sleep(std::time::Duration::from_millis(20));
        write(at, "same.txt", "unchanged\n");
        let (named, _, _) = names(at, &head).await.unwrap();
        let (counted, _, _) = counted(at, &head, false).await.unwrap();
        assert!(
            !named.iter().any(|f| f.path == "same.txt"),
            "{:?}",
            paths(&named)
        );
        assert_eq!(paths(&named), paths(&counted));
        // Against the first commit, which lacks it, it is added in both.
        let (named, _, _) = names(at, &sha).await.unwrap();
        assert!(
            named
                .iter()
                .any(|f| f.path == "same.txt" && f.change == DiffChange::Added)
        );
    }

    #[tokio::test]
    async fn the_names_are_the_counted_list_less_its_counts_in_its_order() {
        let (dir, sha) = changed();
        let (named, _, _) = names(dir.path(), &sha).await.unwrap();
        let (counted, _, _) = counted(dir.path(), &sha, false).await.unwrap();
        assert_eq!(paths(&named), paths(&counted));
    }

    #[tokio::test]
    async fn an_untracked_file_s_lines_are_its_text_and_a_tracked_one_s_are_git_s_patch() {
        let (dir, sha) = changed();
        let asks = [
            RangeFileAsk {
                path: "new.md".to_owned(),
                old_path: None,
            },
            RangeFileAsk {
                path: "a.md".to_owned(),
                old_path: None,
            },
            RangeFileAsk {
                path: "nowhere.md".to_owned(),
                old_path: None,
            },
        ];
        let got = files(dir.path(), &sha, &asks, false).await.unwrap();
        let new = got[0].as_ref().unwrap();
        assert_eq!(new.change, DiffChange::Added);
        let texts: Vec<&str> = new.hunks[0].lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, vec!["# New", "", "one", "two"]);
        assert!(
            new.hunks[0]
                .lines
                .iter()
                .all(|l| l.kind == DiffLineKind::Add)
        );
        let a = got[1].as_ref().unwrap();
        assert!(
            a.hunks[0]
                .lines
                .iter()
                .any(|l| l.kind == DiffLineKind::Add && l.text == "second")
        );
        assert!(got[2].is_none());
    }

    #[tokio::test]
    async fn a_big_or_binary_untracked_file_has_no_lines() {
        let (dir, sha) = changed();
        std::fs::write(dir.path().join("blob.bin"), [1u8, 0, 2, 3]).unwrap();
        let big: String = "x".repeat(usize::try_from(TEXT_CAP).unwrap() + 1);
        std::fs::write(dir.path().join("big.txt"), big).unwrap();
        let (files, _, _) = counted(dir.path(), &sha, false).await.unwrap();
        for path in ["blob.bin", "big.txt"] {
            let file = files.iter().find(|f| f.path == path).unwrap();
            assert_eq!(file.change, DiffChange::Binary, "{path}");
        }
        let got = super::files(
            dir.path(),
            &sha,
            &[RangeFileAsk {
                path: "blob.bin".to_owned(),
                old_path: None,
            }],
            false,
        )
        .await
        .unwrap();
        assert!(got[0].as_ref().unwrap().hunks.is_empty());
    }

    #[tokio::test]
    async fn the_read_leaves_the_index_and_the_refs_as_they_were() {
        let (dir, sha) = changed();
        // A file whose stat moved with no change: git would refresh the index for it if it were let.
        let old = std::fs::read(dir.path().join("a.md")).unwrap();
        std::fs::write(dir.path().join("a.md"), &old).unwrap();
        let index = std::fs::read(dir.path().join(".git/index")).unwrap();
        let refs = git(dir.path(), &["for-each-ref"]);
        let objects = git(dir.path(), &["count-objects", "-v"]);
        counted(dir.path(), &sha, false).await.unwrap();
        names(dir.path(), &sha).await.unwrap();
        files(
            dir.path(),
            &sha,
            &[RangeFileAsk {
                path: "a.md".to_owned(),
                old_path: None,
            }],
            false,
        )
        .await
        .unwrap();
        file_bytes(dir.path(), "a.md", 1024).await.unwrap();
        index_bytes(dir.path(), "b.md", 1024).await.unwrap();
        assert_eq!(std::fs::read(dir.path().join(".git/index")).unwrap(), index);
        assert_eq!(git(dir.path(), &["for-each-ref"]), refs);
        assert_eq!(git(dir.path(), &["count-objects", "-v"]), objects);
    }

    #[tokio::test]
    async fn a_touched_file_reads_again() {
        let (dir, sha) = changed();
        let (before, _, at) = counted(dir.path(), &sha, false).await.unwrap();
        let (again, _, at_again) = counted(dir.path(), &sha, false).await.unwrap();
        assert_eq!(before, again);
        assert_eq!(at.stamp, at_again.stamp);
        std::thread::sleep(std::time::Duration::from_millis(20));
        write(dir.path(), "a.md", "# A\n\nthird\nfourth\n");
        write(dir.path(), "new.md", "# New\n\none\ntwo\nthree\nfour\n");
        let (after, _, at_after) = counted(dir.path(), &sha, false).await.unwrap();
        assert_ne!(at.stamp, at_after.stamp);
        assert_eq!(
            after.iter().find(|f| f.path == "new.md").unwrap().additions,
            6
        );
        assert_ne!(
            after.iter().find(|f| f.path == "a.md").unwrap().additions,
            before.iter().find(|f| f.path == "a.md").unwrap().additions
        );
    }

    #[tokio::test]
    async fn a_working_file_is_read_only_if_git_lists_it() {
        let (dir, _) = changed();
        let at = dir.path();
        std::os::unix::fs::symlink("a.md", at.join("link.md")).unwrap();
        let outside = tempfile::tempdir().unwrap();
        write(outside.path(), "x.md", "outside\n");
        std::os::unix::fs::symlink(outside.path(), at.join("out")).unwrap();
        git(at, &["add", "link.md"]);
        for refused in [
            ".env",
            "../x",
            "/etc/hosts",
            ".git/config",
            "out/x.md",
            "build/out.txt",
            "a.md\0b",
        ] {
            assert_eq!(
                file_bytes(at, refused, 1024).await.unwrap(),
                Bytes::NotThere,
                "{refused}"
            );
        }
        assert_eq!(
            file_bytes(at, "a.md", 1024).await.unwrap(),
            Bytes::Read(b"# A\n\nsecond\n".to_vec())
        );
        assert_eq!(
            file_bytes(at, "new.md", 1024).await.unwrap(),
            Bytes::Read(b"# New\n\none\ntwo\n".to_vec())
        );
        // A link answers its target, as git holds it.
        assert_eq!(
            file_bytes(at, "link.md", 1024).await.unwrap(),
            Bytes::Read(b"a.md".to_vec())
        );
        assert_eq!(
            file_bytes(at, "a.md", 4).await.unwrap(),
            Bytes::TooLarge(12)
        );
    }

    #[tokio::test]
    async fn the_index_side_is_the_staged_blob() {
        let (dir, _) = changed();
        assert_eq!(
            index_bytes(dir.path(), "b.md", 1024).await.unwrap(),
            Bytes::Read(b"# B staged\n".to_vec())
        );
        assert_eq!(
            index_bytes(dir.path(), "new.md", 1024).await.unwrap(),
            Bytes::NotThere
        );
    }

    #[tokio::test]
    async fn a_base_the_checkout_lacks_is_missing_and_a_root_base_is_the_empty_tree() {
        let (dir, sha) = changed();
        let missing = sha_base(dir.path(), "0123456789abcdef0123456789abcdef01234567").await;
        assert!(
            matches!(missing, Err(Refused::Missing(ref why)) if why.contains("0123456")),
            "{missing:?}"
        );
        let root = sha_base(dir.path(), &format!("{sha}^")).await.unwrap();
        assert_eq!(
            root,
            Base {
                git: "4b825dc642cb6eb9a060e54bf8d69288fbee4904".to_owned(),
                sha: None
            }
        );
        let short = sha_base(dir.path(), &sha[..8]).await.unwrap();
        assert_eq!(short.sha.as_deref(), Some(sha.as_str()));
        let (files, _, _) = counted(dir.path(), &root.git, false).await.unwrap();
        assert!(
            files
                .iter()
                .all(|f| f.change == DiffChange::Added || f.change == DiffChange::Binary)
        );
    }

    #[tokio::test]
    async fn a_branch_base_is_the_merge_base_of_its_tip_and_head() {
        let (dir, first) = changed();
        let at = dir.path();
        git(at, &["stash", "-u", "-q"]);
        git(at, &["checkout", "-q", "-b", "feature"]);
        write(at, "f.md", "feature\n");
        git(at, &["add", "f.md"]);
        git(at, &["commit", "-q", "-m", "feature"]);
        git(at, &["checkout", "-q", "main"]);
        write(at, "m.md", "main\n");
        git(at, &["add", "m.md"]);
        git(at, &["commit", "-q", "-m", "main moves"]);
        let main_tip = git(at, &["rev-parse", "main"]);
        git(at, &["checkout", "-q", "feature"]);
        let base = branch_base(at, "main", &main_tip).await.unwrap();
        assert_eq!(base.sha.as_deref(), Some(first.as_str()));
        let lacking = branch_base(at, "main", "0123456789abcdef0123456789abcdef01234567").await;
        assert!(matches!(lacking, Err(Refused::Missing(ref why)) if why.contains("being fetched")));
    }

    #[test]
    fn a_status_names_untracked_and_changed_files() {
        let out = b"1 .M N... 100644 100644 100644 aaa aaa a.md\0? new.md\x002 R. N... 100644 100644 100644 aaa aaa R100 moved.txt\0old.txt\0? fresh/inner.md\0";
        let status = parse_status(out, false);
        assert_eq!(
            status.untracked,
            vec!["new.md".to_owned(), "fresh/inner.md".to_owned()]
        );
        assert!(status.changed.contains("a.md"));
        assert!(!status.changed.contains("moved.txt"));
    }
}
