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

use super::reader::{Form, Reader};
use super::uncommitted::{READ_BYTES, READ_LINES};
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
    /// The list is what knobas last read of the source's compare between
    /// these two branch names, held between visits and sessions (RFC 3339,
    /// when it was read; the forge-waits pass of PR #2479, 2026-11-13): the
    /// page draws it at once under the held line and asks the source's list
    /// behind it ([`source_range_files`]). Absent on every other answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub held_at: Option<String>,
    /// The first page of the files a page draws first, where the lines are
    /// in hand as the list is answered (the ux18 pass, 2026-11-17): the
    /// source's compare just read, or the one knobas held or kept, between
    /// two whole shas. Each is what [`range_file`] answers for its file from
    /// line 0, whole, in the page's draw order ([`first_pages`]), so the
    /// page draws the first files' lines with the list, with no second
    /// read. Absent on every other answer (a checkout's list holds no text):
    /// the page asks for the lines as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub first_pages: Option<Vec<RangeFilePage>>,
}

/// The names of the files a range changes, without their counts (the
/// stored-compare pass, 2026-11-14, [`range_names`]): what Compare draws
/// while the list with counts is read beside it. Compact on the wire, a
/// tenth of [`RangeFiles`]' bytes per file at 10,000 files, since the page
/// draws it on the way to its first frame: the page makes each file of it
/// as the list with counts names it, `additions` and `deletions` `0`, no
/// file `binary` and no blob.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct RangeNames {
    /// Each file's path (where it is after the change), in the list's order.
    pub paths: Vec<String>,
    /// Each file's change, one letter a file in the same order: `A` added,
    /// `D` deleted, `M` modified, `R` renamed.
    pub changes: String,
    /// Each renamed file: its place in `paths`, where it was, and how alike
    /// the two sides are in percent.
    pub renamed: Vec<(u32, String, Option<u8>)>,
    /// Each file whose mode changed: its place in `paths`, and the mode
    /// before and after.
    pub modes: Vec<(u32, String, String)>,
    /// The listing was cut (past 16 MiB of git's listing).
    pub truncated: bool,
    /// The two commits the files' lines are read between: the merge base
    /// and the head, as [`RangeFiles::range`] names them.
    pub range: knobas_source::view::DiffRange,
    /// As [`RangeFiles::predicted_at`]: when the mirror last stored the two
    /// tips the names were read between.
    pub predicted_at: String,
}

impl RangeNames {
    fn of(
        files: &[DiffFile],
        truncated: bool,
        range: knobas_source::view::DiffRange,
        predicted_at: String,
    ) -> Self {
        let mut names = RangeNames {
            paths: Vec::with_capacity(files.len()),
            changes: String::with_capacity(files.len()),
            renamed: Vec::new(),
            modes: Vec::new(),
            truncated,
            range,
            predicted_at,
        };
        for (at, file) in files.iter().enumerate() {
            let at = u32::try_from(at).unwrap_or(u32::MAX);
            names.paths.push(file.path.clone());
            names.changes.push(match file.change {
                DiffChange::Added => 'A',
                DiffChange::Deleted => 'D',
                DiffChange::Renamed => 'R',
                DiffChange::Modified | DiffChange::Binary => 'M',
            });
            if let Some(old) = &file.old_path {
                names.renamed.push((at, old.clone(), file.similarity));
            }
            if let (Some(old), Some(new)) = (&file.old_mode, &file.new_mode) {
                names.modes.push((at, old.clone(), new.clone()));
            }
        }
        names
    }
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
    if let Some(held) = held_list(pool, secrets, registry, live, entity_id, base, head).await {
        return Ok(held);
    }
    let diff =
        super::source_range_diff(pool, secrets, registry, live, entity_id, base, head).await?;
    let (answer, whole_diff) = keep(entity_id, base, head, diff);
    hold_list(
        pool, secrets, registry, live, entity_id, base, head, &answer,
    )
    .await;
    hold_diff(pool, secrets, registry, live, entity_id, &whole_diff).await;
    Ok(answer)
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
    let (answer, whole_diff) = keep(entity_id, base, head, diff);
    hold_list(
        pool, secrets, registry, live, entity_id, base, head, &answer,
    )
    .await;
    hold_diff(pool, secrets, registry, live, entity_id, &whole_diff).await;
    Ok(answer)
}

// -- what the source answered, held between sessions -------------------------

/// A compare's list as it is held (the forge-waits pass of PR #2479,
/// 2026-11-13): what the source's compare between two branch names listed,
/// without lines, under `range_files:<base>...<head>` of the repository in
/// `sync.held_read` (ADR-0042), so a later visit or session draws it at once
/// and checks it behind; the next answer replaces it. A list over the store's
/// row cap (about 13,000 files) is not held.
#[derive(Serialize, serde::Deserialize)]
struct HeldList {
    files: Vec<DiffFile>,
    truncated: bool,
    range: Option<knobas_source::view::DiffRange>,
}

fn held_ask(base: &str, head: &str) -> String {
    format!("range_files:{base}...{head}")
}

/// The list knobas last read of `base`...`head` from the source, where it
/// holds one: answered with `held_at`, so the page checks it behind.
async fn held_list(
    pool: &PgPool,
    secrets: &Arc<dyn knobas_secrets::SecretStore>,
    registry: &Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    live: &crate::sources::live::LiveSources,
    entity_id: &str,
    base: &str,
    head: &str,
) -> Option<RangeFiles> {
    let lent = live.source(pool, secrets, registry, entity_id).await.ok()?;
    let (read_at, answer) = live
        .held_answer(&lent, entity_id, &held_ask(base, head))
        .await?;
    let held: HeldList = serde_json::from_value(answer).ok()?;
    // The lines the page reads first, where knobas keeps or holds the
    // whole diff between the list's two shas: the read the page's first
    // pages would make one hop later.
    let first = match &held.range {
        Some(range) if whole(&range.base) && whole(&range.head) => {
            let diff = match kept().get(entity_id, &range.base, &range.head) {
                Some(diff) => Some(diff),
                None => {
                    let diff = held_diff(
                        pool,
                        secrets,
                        registry,
                        live,
                        entity_id,
                        &range.base,
                        &range.head,
                    )
                    .await;
                    if let Some(diff) = &diff {
                        kept().put(entity_id, &range.base, &range.head, diff);
                    }
                    diff
                }
            };
            diff.and_then(|diff| first_pages(&diff.files, Some(range)))
        }
        _ => None,
    };
    Some(RangeFiles {
        files: held.files,
        truncated: held.truncated,
        range: held.range,
        read_at: DiffReadAt::Source,
        predicted_at: None,
        held_at: Some(read_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
        first_pages: first,
    })
}

/// Hold the source's list of `base`...`head` for the next visit and session.
/// Only a compare of two names: one between two shas is the source's fixed
/// view, which the view cache keeps already.
#[allow(clippy::too_many_arguments)]
async fn hold_list(
    pool: &PgPool,
    secrets: &Arc<dyn knobas_secrets::SecretStore>,
    registry: &Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    live: &crate::sources::live::LiveSources,
    entity_id: &str,
    base: &str,
    head: &str,
    answer: &RangeFiles,
) {
    if whole(base.trim_end_matches('^')) && whole(head) {
        return;
    }
    let Ok(lent) = live.source(pool, secrets, registry, entity_id).await else {
        return;
    };
    let held = HeldList {
        files: answer.files.clone(),
        truncated: answer.truncated,
        range: answer.range.clone(),
    };
    let Ok(value) = serde_json::to_value(&held) else {
        return;
    };
    live.hold_answer(
        &lent,
        entity_id,
        &held_ask(base, head),
        chrono::Utc::now(),
        &value,
    )
    .await;
}

/// A compare's whole diff, lines and all, as it is held under its two shas:
/// `range_diff:<base sha>...<head sha>`, which never changes, so a held list's
/// files draw their lines at once in a later session too. Only where the whole
/// fits the store's row cap (a few hundred ordinary files); a bigger one's
/// lines are read from the source when the page reaches them, as before.
#[derive(Serialize, serde::Deserialize)]
struct HeldDiff {
    files: Vec<DiffFile>,
    truncated: bool,
}

fn diff_ask(base: &str, head: &str) -> String {
    format!("range_diff:{base}...{head}")
}

/// The diff between two whole shas knobas holds, kept for the session's reads.
async fn held_diff(
    pool: &PgPool,
    secrets: &Arc<dyn knobas_secrets::SecretStore>,
    registry: &Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    live: &crate::sources::live::LiveSources,
    entity_id: &str,
    base: &str,
    head: &str,
) -> Option<Arc<RangeDiff>> {
    if !(whole(base) && whole(head)) {
        return None;
    }
    let lent = live.source(pool, secrets, registry, entity_id).await.ok()?;
    let (_, answer) = live
        .held_answer(&lent, entity_id, &diff_ask(base, head))
        .await?;
    let held: HeldDiff = serde_json::from_value(answer).ok()?;
    Some(Arc::new(RangeDiff {
        files: held.files,
        truncated: held.truncated,
        range: None,
        read_at: DiffReadAt::Source,
        predicted_at: None,
    }))
}

/// Hold the source's whole diff under the two shas it settled, where it fits.
async fn hold_diff(
    pool: &PgPool,
    secrets: &Arc<dyn knobas_secrets::SecretStore>,
    registry: &Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    live: &crate::sources::live::LiveSources,
    entity_id: &str,
    diff: &RangeDiff,
) {
    let Some(range) = &diff.range else {
        return;
    };
    if !(whole(&range.base) && whole(&range.head)) {
        return;
    }
    let Ok(lent) = live.source(pool, secrets, registry, entity_id).await else {
        return;
    };
    let held = HeldDiff {
        files: diff.files.clone(),
        truncated: diff.truncated,
    };
    let Ok(value) = serde_json::to_value(&held) else {
        return;
    };
    live.hold_answer(
        &lent,
        entity_id,
        &diff_ask(&range.base, &range.head),
        chrono::Utc::now(),
        &value,
    )
    .await;
}

/// One lock per compare whose files are being read from the source: the
/// diff's four reads at once wait for the first one's answer instead of each
/// asking the source for the whole compare.
fn one_read(entity_id: &str, base: &str, head: &str) -> Arc<tokio::sync::Mutex<()>> {
    type Reads = Mutex<
        std::collections::HashMap<
            (String, String, String),
            std::sync::Weak<tokio::sync::Mutex<()>>,
        >,
    >;
    static READS: OnceLock<Reads> = OnceLock::new();
    let mut all = READS
        .get_or_init(Mutex::default)
        .lock()
        .expect("the compares being read");
    all.retain(|_, lock| lock.strong_count() > 0);
    let key = (entity_id.to_owned(), base.to_owned(), head.to_owned());
    if let Some(lock) = all.get(&key).and_then(std::sync::Weak::upgrade) {
        return lock;
    }
    let lock = Arc::new(tokio::sync::Mutex::new(()));
    all.insert(key, Arc::downgrade(&lock));
    lock
}

/// The source's whole answer, kept under the ends it was asked by and the two
/// shas it settled, and answered without its lines; the whole answer beside it.
fn keep(entity_id: &str, base: &str, head: &str, diff: RangeDiff) -> (RangeFiles, Arc<RangeDiff>) {
    let diff = Arc::new(diff);
    kept().put(entity_id, base, head, &diff);
    if let Some(range) = &diff.range {
        kept().put(entity_id, &range.base, &range.head, &diff);
    }
    let answer = RangeFiles {
        files: diff.files.iter().map(listed).collect(),
        truncated: diff.truncated,
        range: diff.range.clone(),
        read_at: diff.read_at,
        predicted_at: diff.predicted_at.clone(),
        held_at: None,
        first_pages: first_pages(&diff.files, diff.range.as_ref()),
    };
    (answer, diff)
}

/// At most this many files' first pages go beside a list ([`first_pages`]):
/// the view's first read of the files near it asks as many
/// (`TfDiffView`'s `NEAR_AT_ONCE`).
pub const FIRST_FILES: usize = 40;
/// At most this many lines across the first pages beside a list.
pub const FIRST_LINES: usize = 4_000;
/// At most this many bytes of line text across the first pages beside a
/// list: the worst parse the page pays for them, about 25 ms at 6x, is under
/// the IPC round they replace.
pub const FIRST_BYTES: usize = 256 * 1024;
/// How many files [`first_pages`] looks at for ones that fit, in draw order.
const FIRST_LOOKED: usize = 200;
/// A file over this many changed lines draws *Load diff* in place of its
/// lines (`tree.ts`' `LOAD_OVER`), so the page reads none of them at first.
const LOAD_OVER: u32 = 1_500;

/// The first page of each of the files a page draws first, out of a whole
/// diff `files` between the two whole shas of `range` (Fable's ruling, the
/// ux18 pass, 2026-11-17): in the order the page's tree draws them (each
/// folder's files after the root's, folders by path, files by name within,
/// `tree.ts`' `treeOf`), the binary ones, those with no changed line and
/// those over [`LOAD_OVER`] passed over, each page [`page`]`(file, 0)`
/// whole, up to [`FIRST_FILES`] files, [`FIRST_LINES`] lines and
/// [`FIRST_BYTES`] bytes of text. A file that does not fit what is left is
/// passed over and the next looked at, up to [`FIRST_LOOKED`], so one big
/// file at the root starves nothing after it. `None` where the range is not
/// two whole shas (the page keeps a page only by them) or nothing fits.
fn first_pages(
    files: &[DiffFile],
    range: Option<&knobas_source::view::DiffRange>,
) -> Option<Vec<RangeFilePage>> {
    let range = range?;
    if !(whole(&range.base) && whole(&range.head)) {
        return None;
    }
    let split = |path: &str| -> (String, String) {
        match path.rfind('/') {
            Some(at) => (path[..at].to_owned(), path[at + 1..].to_owned()),
            None => (String::new(), path.to_owned()),
        }
    };
    let mut order: Vec<((String, String), &DiffFile)> =
        files.iter().map(|file| (split(&file.path), file)).collect();
    order.sort_by(|a, b| a.0.cmp(&b.0));
    let (mut lines, mut bytes) = (0usize, 0usize);
    let mut pages = Vec::new();
    for (_, file) in order.into_iter().take(FIRST_LOOKED) {
        if pages.len() >= FIRST_FILES {
            break;
        }
        let changed = file.additions + file.deletions;
        if file.change == DiffChange::Binary || changed == 0 || changed > LOAD_OVER {
            continue;
        }
        let first = page(file, 0);
        let (n, b) = first.file.hunks.iter().fold((0, 0), |(n, b), hunk| {
            (
                n + hunk.lines.len(),
                b + hunk.lines.iter().map(|line| line.text.len()).sum::<usize>(),
            )
        });
        if lines + n > FIRST_LINES || bytes + b > FIRST_BYTES {
            continue;
        }
        lines += n;
        bytes += b;
        pages.push(first);
    }
    (!pages.is_empty()).then_some(pages)
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

/// The names of the files `base`...`head` changes, between two branch
/// names, at the checkout of the repository `entity_id` and the tips the
/// mirror holds for them, as [`predicted_files`] reads them less the counts
/// (the stored-compare pass, 2026-11-14): `git diff --raw -z -M A...B`, which
/// pairs renames as the counted read does but reads no file's lines (25 ms
/// for 10,000 files where `--numstat` took 100), with `git merge-base` beside
/// it for the two commits each file's lines are read between. The page draws
/// it while [`range_files`], read beside it, brings the counts.
///
/// `None` where the checkout cannot answer, or where the counted list is
/// kept already (it answers as fast).
pub async fn range_names(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    entity_id: &str,
    base: &str,
    head: &str,
) -> Option<RangeNames> {
    let repo = repo_of(pool, entity_id).await.ok()?.entity_id?;
    let path = resolve(pool, &repo).await.ok()?.path?;
    let (base_tip, head_tip) = tokio::join!(
        branch_tip(pool, registry, &repo, base),
        branch_tip(pool, registry, &repo, head)
    );
    let (base_tip, head_tip) = (base_tip.ok()??, head_tip.ok()??);
    let (a, b) = (
        base_tip.commit.to_ascii_lowercase(),
        head_tip.commit.to_ascii_lowercase(),
    );
    if !whole(&a) || !whole(&b) {
        return None;
    }
    let path = Path::new(&path);
    if kept_lists().has(&listed_key(path, &format!("{a}..."), &b, false)) {
        return None;
    }
    let reader = Reader::at(path);
    let all = async {
        let range = format!("{a}^{{commit}}...{b}^{{commit}}");
        let (from, named) = tokio::join!(reader.merge_base(&a, &b), names(&reader, &range));
        Some((from?, named?))
    };
    let (from, (files, truncated)) = tokio::time::timeout(super::range::TIMEOUT, all)
        .await
        .ok()
        .flatten()?;
    let predicted_at = match (base_tip.as_of, head_tip.as_of) {
        (Some(x), Some(y)) => Some(x.max(y)),
        (x, y) => x.or(y),
    };
    Some(RangeNames::of(
        &files,
        truncated,
        knobas_source::view::DiffRange {
            repository: repo,
            base: from,
            head: b,
            commits: Vec::new(),
            reviewed: None,
        },
        predicted_at.unwrap_or_default(),
    ))
}

/// `git diff --raw -z -M <range>`, parsed: the files [`listing`] reads, in
/// its order, with no counts.
async fn names(reader: &Reader, range: &str) -> Option<(Vec<DiffFile>, bool)> {
    let (out, cut) = reader
        .diff(Form::Raw, range, None, false, &[], usize::MAX, READ_BYTES)
        .await?;
    Some(parse_raw(&out, cut))
}

/// `git diff --raw -z` output as [`parse_stat`] draws the same files, less
/// their counts: no file is `binary`, since only the counts say so.
pub fn parse_raw(out: &[u8], cut: bool) -> (Vec<DiffFile>, bool) {
    let text = String::from_utf8_lossy(out);
    let mut tokens = text.split('\0').map(|t| t.trim_start_matches('\n'));
    let mut files = Vec::new();
    let mut dropped = false;
    while let Some(token) = tokens.next() {
        let Some(meta) = token.strip_prefix(':') else {
            continue;
        };
        let mut parts = meta.split(' ');
        let (Some(old_mode), Some(new_mode), Some(old_blob), Some(new_blob), Some(status)) = (
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
        ) else {
            dropped = true;
            break;
        };
        let letter = status.bytes().next().unwrap_or(b'M');
        let n = if matches!(letter, b'R' | b'C') { 2 } else { 1 };
        let paths: Vec<String> = tokens.by_ref().take(n).map(str::to_owned).collect();
        if paths.len() < n {
            dropped = true;
            break;
        }
        files.push(file_of(
            Raw {
                old_mode: old_mode.to_owned(),
                new_mode: new_mode.to_owned(),
                old_blob: old_blob.to_owned(),
                new_blob: new_blob.to_owned(),
                status: letter,
                score: status.get(1..).and_then(|s| s.parse::<u8>().ok()),
                paths,
            },
            Some(0),
            Some(0),
        ));
    }
    // A cut read may have stopped inside the last record's last path.
    if cut && files.pop().is_some() {
        dropped = true;
    }
    (files, dropped || cut)
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
            held_at: None,
            first_pages: None,
        }
    }
}

/// The two ends at the checkout, resolved as [`range::read`](super::range::read)
/// resolves them: each a checked sha it holds, the base possibly `<sha>^`
/// (the empty tree for a root commit). The old side as git is to name it,
/// the old side's sha (`None`: the empty tree) and the head's.
///
/// Two whole shas need no `rev-parse` each: they are handed to git as
/// `<sha>^{commit}`, so a checkout that lacks one fails the read itself and
/// the source is asked, one start of git fewer each for a read the page
/// waits on (the compare pass: four starts of git in a row were most of a
/// list's time on a busy machine).
struct Ends {
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

async fn ends(reader: &Reader, base: &str, head: &str) -> Option<Ends> {
    let (stem, caret) = match base.strip_suffix('^') {
        Some(stem) => (stem, true),
        None => (base, false),
    };
    if !is_sha(stem) || !is_sha(head) {
        return None;
    }
    if !caret && whole(stem) && whole(head) {
        let (base, head) = (stem.to_ascii_lowercase(), head.to_ascii_lowercase());
        return Some(Ends {
            old: format!("{base}^{{commit}}"),
            new: format!("{head}^{{commit}}"),
            base: Some(base),
            head,
        });
    }
    let head = reader.commit(head).await?;
    let stem = reader.commit(stem).await?;
    let (old, base) = if caret {
        match reader.commit(&format!("{stem}^")).await {
            Some(parent) => (parent.clone(), Some(parent)),
            None => (reader.empty_tree_if_whole().await?, None),
        }
    } else {
        (stem.clone(), Some(stem))
    };
    Some(Ends {
        old,
        new: head.clone(),
        base,
        head,
    })
}

/// `git diff --raw --numstat -z -M <old> <new>` (`<old>` may be `A...B`),
/// parsed: the files and whether the listing was cut.
async fn listing(
    reader: &Reader,
    old: &str,
    new: Option<&str>,
    ignore_whitespace: bool,
) -> Option<(Vec<DiffFile>, bool)> {
    let (out, cut) = reader
        .diff(
            Form::RawNumstat,
            old,
            new,
            ignore_whitespace,
            &[],
            usize::MAX,
            READ_BYTES,
        )
        .await?;
    let (files, cut_inside) = parse_stat(&out, cut);
    Some((files, cut || cut_inside))
}

/// The list a source's three-dot compare of two tips is, at the checkout
/// `path` (ADR-0041): `git diff A...B`, which diffs from their merge base,
/// with the merge base and `git log A..B` read beside it, three reads at
/// once. `None` where it cannot be read here, or the two share no history.
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
    let reader = Reader::at(path);
    let all = async {
        let (a, b) = (base_tip.to_ascii_lowercase(), head_tip.to_ascii_lowercase());
        let range = format!("{a}^{{commit}}...{b}^{{commit}}");
        let (from, listed, commits) = tokio::join!(
            reader.merge_base(&a, &b),
            listing(&reader, &range, None, ignore_whitespace),
            log(&reader, Some(&a), &b)
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
    fn has(&self, key: &str) -> bool {
        self.0
            .lock()
            .expect("the kept lists")
            .iter()
            .any(|(k, _)| k == key)
    }

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
    let reader = Reader::at(path);
    let all = async {
        let Ends {
            old,
            new,
            base,
            head,
        } = ends(&reader, base, head).await?;
        let (listed, commits) = tokio::join!(
            listing(&reader, &old, Some(&new), ignore_whitespace),
            log(&reader, base.as_deref(), &head)
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
    reader: &Reader,
    base: Option<&str>,
    head: &str,
) -> Vec<knobas_source::view::RangeCommit> {
    let Some(base) = base else {
        return Vec::new();
    };
    let Some(out) = reader.log(base, head, COMMITS).await else {
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
/// the numstat records, each paired with its raw record by the file's path.
/// `cut`: the read stopped short, so a last record may be partial and is
/// dropped. Answers the files and whether a record was dropped.
///
/// By path, not by place (the git-starts pass, 2026-11-16): with blanks
/// ignored the two halves need not hold the same files. A git before 2.52
/// lists a file whose lines differ in blanks alone among the raw records and
/// gives it no count, so every file after it took the next file's count and
/// the list said it was cut. A git since 2.52, and knobas's own reader,
/// leave such a file out of both. A file is listed where both name it: it
/// has something left to show.
pub fn parse_stat(out: &[u8], cut: bool) -> (Vec<DiffFile>, bool) {
    let text = String::from_utf8_lossy(out);
    let mut tokens = text
        .split('\0')
        .map(|t| t.trim_start_matches('\n'))
        .peekable();
    let mut raws: Vec<Raw> = Vec::new();
    // Each count with its file's path, in the order read.
    type Counted<'a> = (&'a str, (Option<u32>, Option<u32>));
    let mut counted: Vec<Counted> = Vec::new();
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
        let path = if path.is_empty() {
            // A rename: where it was, then where it is.
            let (Some(_), Some(new)) = (tokens.next(), tokens.next()) else {
                break;
            };
            new
        } else {
            path
        };
        counted.push((path, (added.parse().ok(), deleted.parse().ok())));
    }
    // A cut read stopped inside the counts: the last one read may have
    // stopped part way, and the files past it have none.
    let dropped = cut && !raws.is_empty();
    if cut {
        counted.pop();
    }
    let counts: std::collections::HashMap<&str, (Option<u32>, Option<u32>)> =
        counted.into_iter().collect();
    let files = raws
        .into_iter()
        .filter_map(|raw| {
            let (added, deleted) = counts.get(raw.paths.last()?.as_str()).copied()?;
            Some(file_of(raw, added, deleted))
        })
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
        new_mode: modes.then_some(raw.new_mode),
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
    let diff = kept_diff(pool, secrets, registry, live, entity_id, base, head).await?;
    diff.files
        .iter()
        .find(|file| file.path == path && file.old_path.as_deref() == old_path)
        .or_else(|| diff.files.iter().find(|file| file.path == path))
        .map(|file| page(file, from))
        .ok_or_else(missing)
}

/// The source's compare between `base` and `head`, kept for the session's
/// reads of its files: the one kept, else the one knobas held, else read
/// (once, however many files ask at the same time).
async fn kept_diff(
    pool: &PgPool,
    secrets: &Arc<dyn knobas_secrets::SecretStore>,
    registry: &Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    live: &crate::sources::live::LiveSources,
    entity_id: &str,
    base: &str,
    head: &str,
) -> Result<Arc<RangeDiff>, IpcError> {
    if let Some(diff) = kept().get(entity_id, base, head) {
        return Ok(diff);
    }
    // The diff's pages are read four files at once: one read of the source between them (the forge-waits pass).
    let one = one_read(entity_id, base, head);
    let _reading = one.lock().await;
    if let Some(diff) = kept().get(entity_id, base, head) {
        return Ok(diff);
    }
    let diff = match held_diff(pool, secrets, registry, live, entity_id, base, head).await {
        Some(diff) => diff,
        None => {
            let diff =
                super::source_range_diff(pool, secrets, registry, live, entity_id, base, head)
                    .await?;
            let diff = Arc::new(diff);
            hold_diff(pool, secrets, registry, live, entity_id, &diff).await;
            diff
        }
    };
    kept().put(entity_id, base, head, &diff);
    Ok(diff)
}

/// The most files one [`range_file_pages`] reads at once: where the read
/// falls to git ([`Reader`]), a command line of paths well inside Windows'
/// 32,767 characters.
pub const PAGES_AT_ONCE: usize = 200;

/// One file [`range_file_pages`] is asked for: as the list named it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct RangeFileAsk {
    pub path: String,
    #[serde(default)]
    pub old_path: Option<String>,
}

/// The first page of each of `files` in `base`..`head` of the repository
/// `entity_id`, in one read (the big-compare pass of PR #2479, 2026-11-16):
/// at the checkout one `git diff -M -- <paths>` for all of them (in-process
/// since the git-starts pass of the same day, [`Reader`]), where
/// [`range_file`] reads one file a call; else out of the kept compare. Each answer is the page
/// [`range_file`] answers for that file from line 0, in the order asked;
/// `None` for a file this read could not answer whole (past the read's
/// ceiling, or not in the diff), which the page then reads alone. At most
/// [`PAGES_AT_ONCE`] files.
///
/// # Errors
///
/// As [`range_file`], for the read as a whole.
#[allow(clippy::too_many_arguments)]
pub async fn range_file_pages(
    pool: &PgPool,
    secrets: &Arc<dyn knobas_secrets::SecretStore>,
    registry: &Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    live: &crate::sources::live::LiveSources,
    entity_id: &str,
    base: &str,
    head: &str,
    files: &[RangeFileAsk],
    ignore_whitespace: bool,
) -> Result<Vec<Option<RangeFilePage>>, IpcError> {
    if files.len() > PAGES_AT_ONCE {
        return Err(IpcError::invalid(format!(
            "at most {PAGES_AT_ONCE} files a read, not {}",
            files.len()
        )));
    }
    if files.is_empty() {
        return Ok(Vec::new());
    }
    if let Some(repo) = checkout_of(pool, entity_id).await
        && let Some(found) = files_at(&repo, base, head, files, ignore_whitespace).await
    {
        return Ok(found
            .iter()
            .map(|file| file.as_ref().map(|f| page(f, 0)))
            .collect());
    }
    let diff = kept_diff(pool, secrets, registry, live, entity_id, base, head).await?;
    Ok(files
        .iter()
        .map(|ask| {
            diff.files
                .iter()
                .find(|file| file.path == ask.path && file.old_path == ask.old_path)
                .or_else(|| diff.files.iter().find(|file| file.path == ask.path))
                .map(|file| page(file, 0))
        })
        .collect())
}

/// `files` of `base`..`head` at the checkout `repo`, in one `git diff -M --
/// <path> [<old path>]…`, parsed: each as [`file`] reads it alone, in the
/// order asked. `None` where the checkout cannot answer; a file the read
/// cut, or the diff does not hold, is `None` in its place.
async fn files_at(
    repo: &Path,
    base: &str,
    head: &str,
    files: &[RangeFileAsk],
    ignore_whitespace: bool,
) -> Option<Vec<Option<DiffFile>>> {
    let reader = Reader::at(repo);
    let all = async {
        let Ends { old, new, .. } = ends(&reader, base, head).await?;
        // Literal paths: a name is the file's, never a pattern.
        let mut paths: Vec<&str> = Vec::with_capacity(files.len());
        for ask in files {
            paths.push(&ask.path);
            if let Some(old_path) = ask.old_path.as_deref().filter(|o| *o != ask.path) {
                paths.push(old_path);
            }
        }
        let (text, cut) = reader
            .diff(
                Form::Patch,
                &old,
                Some(&new),
                ignore_whitespace,
                &paths,
                READ_LINES,
                READ_BYTES,
            )
            .await?;
        let (mut parsed, _) = parse_view(&String::from_utf8_lossy(&text));
        // The last file of a cut read stopped part way: read alone, it has its own pages.
        if cut {
            parsed.pop();
        }
        Some(
            files
                .iter()
                .map(|ask| {
                    let at = parsed
                        .iter()
                        .position(|f| f.path == ask.path && f.old_path == ask.old_path)
                        .or_else(|| parsed.iter().position(|f| f.path == ask.path))?;
                    Some(parsed.swap_remove(at))
                })
                .collect(),
        )
    };
    tokio::time::timeout(super::range::TIMEOUT, all)
        .await
        .ok()
        .flatten()
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
    let reader = Reader::at(repo);
    let all = async {
        let Ends { old, new, .. } = ends(&reader, base, head).await?;
        let mut paths = vec![path];
        if let Some(old_path) = old_path.filter(|o| *o != path) {
            paths.push(old_path);
        }
        // Literal paths: a name is the file's, never a pattern.
        let (text, cut) = reader
            .diff(
                Form::Patch,
                &old,
                Some(&new),
                ignore_whitespace,
                &paths,
                READ_LINES,
                READ_BYTES,
            )
            .await?;
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
/// A kept compare: its repository and two ends, and the source's answer.
type KeptCompare = ((String, String, String), Arc<RangeDiff>);

struct Kept(Mutex<VecDeque<KeptCompare>>);

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

    /// Many files in one read are each the file read alone (the big-compare
    /// pass): all of the fixture's at once, in two halves, one at a time and
    /// in the opposite order, the renames with their old paths beside them.
    #[tokio::test]
    async fn many_files_read_at_once_are_each_the_file_read_alone() {
        let (dir, base, head) = repo();
        let p = dir.path();
        let all = whole(p, &base, &head);
        let asks: Vec<RangeFileAsk> = all
            .iter()
            .map(|f| RangeFileAsk {
                path: f.path.clone(),
                old_path: f.old_path.clone(),
            })
            .collect();
        let mut reversed = asks.clone();
        reversed.reverse();
        let half = asks.len() / 2;
        let batches: Vec<Vec<RangeFileAsk>> = vec![
            asks.clone(),
            asks[..half].to_vec(),
            asks[half..].to_vec(),
            reversed,
        ]
        .into_iter()
        .chain(asks.iter().map(|a| vec![a.clone()]))
        .collect();
        for batch in batches {
            let got = files_at(p, &base, &head, &batch, false)
                .await
                .expect("the checkout answers");
            assert_eq!(got.len(), batch.len());
            for (ask, got) in batch.iter().zip(got) {
                let alone = file(p, &base, &head, &ask.path, ask.old_path.as_deref(), false)
                    .await
                    .expect("the checkout answers");
                assert_eq!(got, alone, "{} in a read of {}", ask.path, batch.len());
                assert!(got.is_some(), "{} is in the diff", ask.path);
            }
        }
        // Whitespace left out the same way.
        let ws = files_at(p, &base, &head, &asks, true)
            .await
            .expect("answers");
        for (ask, got) in asks.iter().zip(ws) {
            let alone = file(p, &base, &head, &ask.path, ask.old_path.as_deref(), true)
                .await
                .expect("answers");
            assert_eq!(got, alone, "{} with whitespace left out", ask.path);
        }
        // A file the diff does not hold is none, and the others still answer.
        let mut with_stranger = asks[..2].to_vec();
        with_stranger.insert(
            1,
            RangeFileAsk {
                path: "not/there.rs".into(),
                old_path: None,
            },
        );
        let got = files_at(p, &base, &head, &with_stranger, false)
            .await
            .expect("answers");
        assert!(got[1].is_none());
        assert!(got[0].is_some() && got[2].is_some());
    }

    /// The names drawn first are the counted list less its counts, in its
    /// order (the stored-compare pass): renames paired as the counted read
    /// pairs them, so no row comes or goes when the counts land.
    #[tokio::test]
    async fn the_names_are_the_counted_list_less_its_counts_in_its_order() {
        let (dir, base, head) = repo();
        let p = dir.path();
        let reader = Reader::at(p);
        let range = format!("{base}...{head}");
        let (named, cut) = names(&reader, &range).await.expect("the clone answers");
        assert!(!cut);
        let (counted, _) = listing(&reader, &range, None, false)
            .await
            .expect("the clone answers");
        let less = |f: &DiffFile| DiffFile {
            additions: 0,
            deletions: 0,
            // Only the counts say a file is binary: its change as the raw record names it.
            change: if f.change == DiffChange::Binary {
                DiffChange::Modified
            } else {
                f.change
            },
            ..f.clone()
        };
        assert_eq!(named, counted.iter().map(less).collect::<Vec<_>>());
        // And on the wire, as the page makes each file of it again (`names.ts`): the counted list less its counts and blobs.
        let range = knobas_source::view::DiffRange {
            repository: "r".into(),
            base: base.clone(),
            head: head.clone(),
            commits: Vec::new(),
            reviewed: None,
        };
        let wire = RangeNames::of(&named, false, range, String::new());
        let made: Vec<DiffFile> = wire
            .paths
            .iter()
            .zip(wire.changes.chars())
            .enumerate()
            .map(|(at, (path, letter))| {
                let at = u32::try_from(at).unwrap();
                let moved = wire.renamed.iter().find(|r| r.0 == at);
                let mode = wire.modes.iter().find(|m| m.0 == at);
                DiffFile {
                    path: path.clone(),
                    old_path: moved.map(|r| r.1.clone()),
                    change: match letter {
                        'A' => DiffChange::Added,
                        'D' => DiffChange::Deleted,
                        'R' => DiffChange::Renamed,
                        _ => DiffChange::Modified,
                    },
                    additions: 0,
                    deletions: 0,
                    hunks: Vec::new(),
                    truncated: false,
                    similarity: moved.and_then(|r| r.2),
                    old_mode: mode.map(|m| m.1.clone()),
                    new_mode: mode.map(|m| m.2.clone()),
                    threads: Vec::new(),
                    blob: None,
                }
            })
            .collect();
        assert_eq!(
            made,
            counted
                .iter()
                .map(|f| DiffFile {
                    blob: None,
                    ..less(f)
                })
                .collect::<Vec<_>>()
        );
        assert!(
            named
                .iter()
                .any(|f| f.change == DiffChange::Renamed && f.similarity.is_some_and(|s| s < 100)),
            "an edited rename is paired too"
        );
        // A cut read drops the record it may have stopped inside.
        let out = b":100644 100644 aaaaaaa bbbbbbb M\0a.txt\0:100644 100644 ccccccc ddddddd M\0b.t";
        let (files, dropped) = parse_raw(out, true);
        assert!(dropped);
        assert_eq!(
            files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            ["a.txt"]
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

    /// With blanks ignored the two halves of git's listing need not hold
    /// the same files, and each count is its own file's whichever git wrote
    /// them (the git-starts pass, 2026-11-16).
    #[test]
    fn a_count_is_paired_with_its_file_by_path() {
        // A git before 2.52: `blanks.txt` among the raw records, with no count.
        let older = b":100644 100644 aaaaaaa bbbbbbb M\0a.txt\0:100644 100644 1111111 2222222 M\0blanks.txt\0:100644 100644 ccccccc ddddddd M\0z.txt\x001\t2\ta.txt\x003\t4\tz.txt\0";
        let (files, dropped) = parse_stat(older, false);
        assert!(
            !dropped,
            "nothing was cut: a file with nothing to show is none"
        );
        assert_eq!(
            files
                .iter()
                .map(|f| (f.path.as_str(), f.additions, f.deletions))
                .collect::<Vec<_>>(),
            [("a.txt", 1, 2), ("z.txt", 3, 4)],
            "z.txt keeps its own count"
        );
        // A count no raw record names is no file of the list.
        let stray = b":100644 100644 aaaaaaa bbbbbbb M\0a.txt\0:100644 100644 ccccccc ddddddd M\0z.txt\x001\t2\ta.txt\x007\t7\tother.txt\x003\t4\tz.txt\0";
        let (files, dropped) = parse_stat(stray, false);
        assert!(!dropped);
        assert_eq!(
            files
                .iter()
                .map(|f| (f.path.as_str(), f.additions, f.deletions))
                .collect::<Vec<_>>(),
            [("a.txt", 1, 2), ("z.txt", 3, 4)]
        );
        // A rename with an edit is paired by where the file is now.
        let renamed =
            b":100644 100644 aaaaaaa bbbbbbb R090\0old.txt\0new.txt\x005\t6\t\0old.txt\0new.txt\0";
        let (files, _) = parse_stat(renamed, false);
        assert_eq!(files[0].path, "new.txt");
        assert_eq!(files[0].old_path.as_deref(), Some("old.txt"));
        assert_eq!((files[0].additions, files[0].deletions), (5, 6));
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

    fn range_of(base: &str, head: &str) -> knobas_source::view::DiffRange {
        knobas_source::view::DiffRange {
            repository: "repo".to_owned(),
            base: base.to_owned(),
            head: head.to_owned(),
            commits: Vec::new(),
            reviewed: None,
        }
    }

    /// **The source's list carries the first pages the page draws first**
    /// (Fable's ruling, the ux18 pass, 2026-11-17): in the tree's draw
    /// order, binary files, files with no changed line and files over
    /// `LOAD_OVER` passed over, each the page `range_file_pages` answers out
    /// of the kept compare (`page(file, 0)`); none where the range is not
    /// two whole shas.
    #[tokio::test]
    async fn a_source_list_carries_the_first_pages_in_the_pages_draw_order() {
        let (dir, base, head) = repo();
        let files = whole(dir.path(), &base, &head);
        let diff = RangeDiff {
            files: files.clone(),
            truncated: false,
            range: Some(range_of(&base, &head)),
            read_at: DiffReadAt::Source,
            predicted_at: None,
        };
        let (answer, kept_diff) = keep("repo:first-pages", "main", "topic", diff);
        let first = answer.first_pages.expect("the lines are in hand");
        let drawn: Vec<&str> = first.iter().map(|p| p.file.path.as_str()).collect();
        // The root's files by name, then each folder's; no binary, no empty, not `big.txt`'s 8,000 lines.
        assert_eq!(
            drawn,
            [
                "added.md",
                "edit.rs",
                "gone.txt",
                "with space.txt",
                "ünï.txt",
                "src/deep/moved.rs",
            ]
        );
        for one in &first {
            let in_kept = kept_diff
                .files
                .iter()
                .find(|f| f.path == one.file.path)
                .unwrap();
            assert_eq!(one, &page(in_kept, 0), "{}", one.file.path);
        }
        assert!(
            answer.files.iter().all(|f| f.hunks.is_empty()),
            "the list itself still carries no lines"
        );
        let by_name = RangeDiff {
            files,
            truncated: false,
            range: Some(range_of("main", &head)),
            read_at: DiffReadAt::Source,
            predicted_at: None,
        };
        assert!(
            keep("repo:first-pages", "main", "x", by_name)
                .0
                .first_pages
                .is_none(),
            "kept only by two whole shas"
        );
    }

    /// The first pages stop at `FIRST_FILES` files, and a file that does
    /// not fit the lines left is passed over, not the end of them.
    #[test]
    fn the_first_pages_hold_to_their_budget_and_a_big_file_starves_nothing() {
        let file = |path: &str, n: usize| {
            let lines = (0..n)
                .map(|i| knobas_source::diff::DiffLine {
                    kind: knobas_source::diff::DiffLineKind::Add,
                    old: None,
                    new: Some(u32::try_from(i).unwrap() + 1),
                    text: format!("line {i}"),
                })
                .collect();
            DiffFile {
                path: path.to_owned(),
                old_path: None,
                change: DiffChange::Added,
                additions: u32::try_from(n).unwrap(),
                deletions: 0,
                hunks: vec![Hunk {
                    header: format!("@@ -0,0 +1,{n} @@"),
                    lines,
                }],
                truncated: false,
                similarity: None,
                old_mode: None,
                new_mode: None,
                blob: None,
                threads: Vec::new(),
            }
        };
        let sha = |c: char| c.to_string().repeat(40);
        let range = range_of(&sha('a'), &sha('b'));
        // A root file of 1,400 lines first, then three of 1,400: the fourth would pass 4,000.
        let mut files = vec![
            file("a.txt", 1_400),
            file("b.txt", 1_400),
            file("c.txt", 1_400),
        ];
        files.push(file("d.txt", 10));
        let first = first_pages(&files, Some(&range)).unwrap();
        assert_eq!(
            first
                .iter()
                .map(|p| p.file.path.as_str())
                .collect::<Vec<_>>(),
            ["a.txt", "b.txt", "d.txt"],
            "c.txt does not fit; d.txt after it does"
        );
        let many: Vec<DiffFile> = (0..100).map(|n| file(&format!("f{n:03}.txt"), 2)).collect();
        assert_eq!(first_pages(&many, Some(&range)).unwrap().len(), FIRST_FILES);
    }

    #[test]
    fn the_kept_compares_are_the_last_few() {
        let k = Kept(Mutex::new(VecDeque::new()));
        let diff = |n: usize| {
            Arc::new(RangeDiff {
                files: Vec::new(),
                truncated: n.is_multiple_of(2),
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
