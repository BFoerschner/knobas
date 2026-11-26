//! **The files pass** (the everything-search decisions, 2026-11-20, D2, D3,
//! D4 and D7; ADR-0050): every mirrored repository's files, on its default
//! branch's head, into the files index (`knobas-files`), so *Everything*
//! finds a repository by its files' names and text.
//!
//! **After a run, behind a person**, after the tree pass on the run's task:
//! it reads `sync.branch_tree` and `sync.git_tree`, which that pass keeps at
//! the mirror's head, so which files exist and which a push changed is known
//! with no request.
//!
//! **Only what changed.** A repository whose stored default branch stands at
//! the commit `sync.code_repo` records is passed by. One that moved is
//! compared with `sync.code_file` by blob: a file whose blob is the same is
//! not touched, and one whose text is no longer kept (the bound) is written
//! again without it. A repository whose row has no commit is compared whole.
//!
//! **Where the bytes come from** (D3), by blob: a checkout on this machine
//! ([`Clones`]), then the forge, one archive where more than
//! [`ARCHIVE_OVER`] files are wanted and the archive is mostly what is
//! wanted ([`by_archive`]), else a file at a time.
//!
//! **Two stores, one rule** (D7 point 8): the index commits first, and one
//! PostgreSQL transaction records it after. A crash between the two leaves
//! the row at its old commit, and the next pass makes the same difference
//! again; a write to the index replaces, so it is the same as once.
//! [`reconcile`] compares the two at bring-up.
//!
//! **Bounded**: [`BUDGET`] a run, at most [`ARCHIVES_PER_RUN`] archives and
//! [`READS_PER_RUN`] single reads. A repository the bounds cut keeps what it
//! wrote: its row stays at the old commit, and the next run finds most of
//! its files unchanged.
//!
//! **A cut goes last** (the small-fixes sweep, 2026-11-21): a repository the
//! bounds cut is read after every other the next run, and one whose archive
//! the deadline cut is read a file at a time from then on. A download is
//! kept whole before it is read, so a cut archive kept nothing, was asked
//! again every run, first in the queue, and held up every repository after
//! it; single reads keep what each wrote.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use knobas_core::code_text::{self, Attributes, LeftOut};
use knobas_files::{FileIndex, FileWriter, FilesError, Repo, Repos};
use knobas_source::Source;
use sqlx::PgPool;

/// How long one run's pass reads files, at most.
pub const BUDGET: Duration = Duration::from_secs(120);

/// The most archives one run asks the forge for: one repository each, one
/// at a time. A seat belt: [`BUDGET`] is the bound (Fable's ruling of
/// 2026-11-21 on the first fill: 20 a run left a scale home's 64 small
/// repositories to 11 runs, 55 minutes at the default interval).
pub const ARCHIVES_PER_RUN: u32 = 200;

/// The most single files one run asks the forge for: half of what the
/// forge's quota (10 a second) allows inside [`BUDGET`].
pub const READS_PER_RUN: u32 = 600;

/// More wanted files of a repository than this are read as one archive,
/// where the tree says the archive is mostly them ([`by_archive`]).
pub const ARCHIVE_OVER: usize = 3;

/// The largest whole tree read as one archive: past it a download the run's
/// deadline cuts would be asked again every run and hold up every repository
/// after it.
pub const ARCHIVE_TREE_CAP: u64 = 1 << 30;

/// Whether `missing` wanted files of a tree of `tree_bytes` in all are read
/// as one archive (Fable's ruling of 2026-11-21, amending D3): more than
/// [`ARCHIVE_OVER`] of them, and an archive no larger than the single reads
/// could have been, [`code_text::SIZE_CAP`] a file, and than
/// [`ARCHIVE_TREE_CAP`]. A ten-file repository beside a 50 MB binary is read
/// a file at a time.
#[must_use]
pub fn by_archive(missing: usize, tree_bytes: u64) -> bool {
    missing > ARCHIVE_OVER
        && tree_bytes <= (missing as u64).saturating_mul(code_text::SIZE_CAP)
        && tree_bytes <= ARCHIVE_TREE_CAP
}

/// The index commits every this many files written, and at a repository's end.
pub const COMMIT_EVERY: usize = 2_000;

/// A repository's share of text files, shallowest path first, then by path.
pub const SHARE_FILES: usize = 20_000;

/// A repository's share of text, in bytes.
pub const SHARE_BYTES: u64 = 256 << 20;

/// Held by a pass for its whole length: the index's one writer, taken in
/// turn rather than refused.
static PASSING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The repositories whose archive a run's deadline cut, read a file at a time
/// from then on. The process's: after a restart such a repository is still
/// read last (its row says its read was cut), and may ask its archive once
/// more with what that run leaves.
static ARCHIVE_CUT: std::sync::LazyLock<std::sync::Mutex<HashSet<String>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(HashSet::new()));

fn archive_cut(repository: &str) -> bool {
    ARCHIVE_CUT.lock().is_ok_and(|cut| cut.contains(repository))
}

/// `knobas.setting`'s key of *Keep at most* (Settings › Search, D13): the
/// file text kept on this computer, in MB, as a number in a string; `0` is
/// *None*. Absent, [`KEPT_MB`].
pub const FILES_KEPT_KEY: &str = "search.files_kept_mb";

/// *Keep at most* until set: 2 GB of file text.
pub const KEPT_MB: u64 = 2_048;

/// What reads a blob from a checkout of a repository on this machine: the
/// app's, which looks the repository's checkouts up as *Files* does
/// (`knobas_app::checkout::local`). `knobas-sync` sees no checkout itself.
pub trait Clones: Send + Sync {
    /// The bytes of blob `sha` (hex) from a checkout of `repository` on this
    /// machine, at most `max` of them, or `None`.
    fn blob<'a>(
        &'a self,
        repository: &'a str,
        sha: &'a str,
        max: usize,
    ) -> Pin<Box<dyn Future<Output = Option<Vec<u8>>> + Send + 'a>>;

    /// Whether a checkout of `repository` is on this machine: such a
    /// repository is read before the others, from it.
    fn holds<'a>(&'a self, repository: &'a str) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>>;
}

/// What the app hands the pass: the index and the checkouts' reader.
#[derive(Clone)]
pub struct Files {
    pub index: FileIndex,
    pub clones: Option<Arc<dyn Clones>>,
}

/// What failed.
#[derive(Debug, thiserror::Error)]
pub enum PassError {
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Index(#[from] FilesError),
}

/// The bounds of one run's pass.
#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    pub budget: Duration,
    pub archives: u32,
    pub reads: u32,
}

impl Default for Bounds {
    fn default() -> Self {
        Self {
            budget: BUDGET,
            archives: ARCHIVES_PER_RUN,
            reads: READS_PER_RUN,
        }
    }
}

/// What one pass did: what the log line and `code:files` say.
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Passed {
    /// Repositories whose files now stand at their head.
    pub repositories: u32,
    /// Repositories still owed a read: the bounds cut them, or they wait.
    pub owed: u32,
    /// Files written to the index.
    pub files: u32,
    /// Files removed from it.
    pub removed: u32,
    /// Bytes of text read.
    pub bytes: u64,
    /// Blobs read from a checkout.
    pub from_clones: u32,
    /// Archives asked of the forge.
    pub archives: u32,
    /// Single files asked of the forge.
    pub reads: u32,
    /// Repositories whose documents were removed (withdrawn, purged).
    pub gone: u32,
}

/// The progress the pass reports at each repository's end, as `code:files`
/// carries it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
pub struct FilesProgress {
    pub source_id: String,
    /// Repositories of the source whose files stand at their head.
    pub done: u32,
    /// Repositories of the source still owed a read.
    pub owed: u32,
    /// The file text kept, over every source, in bytes.
    #[ts(type = "number")]
    pub text_bytes: u64,
}

/// One repository owed a read.
#[derive(Debug, Clone, sqlx::FromRow)]
struct Owed {
    repository_id: String,
    commit: String,
    root: Vec<u8>,
    /// The commit the index holds it at; `None`: read it whole.
    held: Option<String>,
    no: Option<i32>,
    favourite: bool,
    updated: Option<chrono::DateTime<chrono::Utc>>,
    /// A read the bounds cut: begun (`read_at` set) and not at this commit.
    cut: bool,
}

/// One file of a stored tree.
#[derive(Debug, Clone, sqlx::FromRow)]
struct TreeFile {
    path: String,
    size: i64,
    sha: Vec<u8>,
}

/// One file as `sync.code_file` holds it.
#[derive(Debug, Clone)]
struct Held {
    sha: Vec<u8>,
    has_text: bool,
}

/// The kept file text's bound in bytes, `None` for no bound read (a test).
///
/// # Errors
///
/// A database failure.
pub async fn kept_bytes(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let stored: Option<serde_json::Value> =
        sqlx::query_scalar("select value from knobas.setting where key = $1")
            .bind(FILES_KEPT_KEY)
            .fetch_optional(pool)
            .await?;
    let mb = stored
        .as_ref()
        .and_then(|value| {
            value
                .as_str()
                .and_then(|text| text.trim().parse::<u64>().ok())
                .or_else(|| value.as_u64())
        })
        .unwrap_or(KEPT_MB);
    Ok(mb.saturating_mul(1 << 20))
}

/// Run one source's pass inside the default bounds.
///
/// # Errors
///
/// A database or index failure; a file the forge fails to answer is left for
/// the next run, not an error.
pub async fn refresh(
    pool: &PgPool,
    source_id: &str,
    source: &dyn Source,
    files: &Files,
    report: &(dyn Fn(FilesProgress) + Send + Sync),
) -> Result<Passed, PassError> {
    let bound = kept_bytes(pool).await?;
    refresh_within(
        pool,
        source_id,
        source,
        files,
        Bounds::default(),
        bound,
        report,
    )
    .await
}

/// Run `work`, which blocks (an index write), off the runtime's other tasks
/// where the runtime has threads to spare: the launcher's reads go on beside
/// it. On a single-threaded runtime (a test) it simply runs.
fn blocking<T>(work: impl FnOnce() -> T) -> T {
    match tokio::runtime::Handle::try_current().map(|handle| handle.runtime_flavor()) {
        Ok(tokio::runtime::RuntimeFlavor::MultiThread) => tokio::task::block_in_place(work),
        _ => work(),
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// [`refresh`] with its bounds and the bound on kept text (bytes) given.
///
/// # Errors
///
/// As [`refresh`].
#[allow(clippy::too_many_lines)]
pub async fn refresh_within(
    pool: &PgPool,
    source_id: &str,
    source: &dyn Source,
    files: &Files,
    bounds: Bounds,
    bound: u64,
    report: &(dyn Fn(FilesProgress) + Send + Sync),
) -> Result<Passed, PassError> {
    let mut passed = Passed::default();
    if !files.index.is_open() {
        return Ok(passed);
    }
    let Some(shape) = source.tree_shape() else {
        return Ok(passed);
    };
    // One pass at a time over the index, whichever source's run it follows:
    // the index has one writer, and a second pass that found it busy read
    // nothing until its source's next run (two forges' first runs end
    // together). The wait is not the budget's.
    let _one = PASSING.lock().await;
    let started = Instant::now();
    let deadline = started + bounds.budget;

    // 1. Gone: every repository whose documents are owed a removal.
    passed.gone = remove_gone(pool, &files.index).await?;

    // The bound lowered under what is kept: the largest repositories are
    // read whole again, their text dropped, until the rest fits (D7).
    shrink_to(pool, bound).await?;

    // 2. Owed.
    let path: Vec<String> = shape
        .default_branch
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let mut owed: Vec<Owed> = sqlx::query_as(
        "select r.entity_id as repository_id, bt.commit_sha as commit, bt.root,
                cr.commit_sha as held, cr.no,
                exists (select 1 from knobas.favourite f where f.entity_id = r.entity_id) as favourite,
                r.item_updated_at as updated,
                (cr.read_at is not null and cr.commit_sha is null) as cut
           from sync.item r
           join knobas.entity e on e.id = r.entity_id and e.deleted_at is null
           join sync.branch_tree bt on bt.repository_id = r.entity_id
                                   and bt.branch = r.payload #>> $2::text[]
           left join sync.code_repo cr on cr.repository_id = r.entity_id
          where r.source_id = $1 and r.kind = $3 and bt.root is not null
            and cr.commit_sha is distinct from bt.commit_sha",
    )
    .bind(source_id)
    .bind(&path)
    .bind(shape.repository)
    .fetch_all(pool)
    .await?;
    let mut held_here: HashSet<String> = HashSet::new();
    if let Some(clones) = &files.clones {
        for repository in &owed {
            if clones.holds(&repository.repository_id).await {
                held_here.insert(repository.repository_id.clone());
            }
        }
    }
    let cloned = |repository: &str| held_here.contains(repository);
    let last = |repository: &Owed| repository.cut || archive_cut(&repository.repository_id);
    owed.sort_by(|a, b| {
        last(a)
            .cmp(&last(b))
            .then_with(|| b.favourite.cmp(&a.favourite))
            .then_with(|| cloned(&b.repository_id).cmp(&cloned(&a.repository_id)))
            .then_with(|| b.updated.cmp(&a.updated))
            .then_with(|| a.repository_id.cmp(&b.repository_id))
    });
    let total_owed = u32::try_from(owed.len()).unwrap_or(u32::MAX);

    let mut writer = files.index.writer()?;
    let mut budget = (bounds.archives, bounds.reads);
    let mut done_before: u32 = sqlx::query_scalar::<_, i64>(
        "select count(*) from sync.code_repo where source_id = $1 and commit_sha is not null",
    )
    .bind(source_id)
    .fetch_one(pool)
    .await?
    .try_into()
    .unwrap_or(0);
    for (index, repository) in owed.iter().enumerate() {
        if Instant::now() >= deadline {
            // With the repositories this run cut before it, counted above.
            passed.owed += total_owed - u32::try_from(index).unwrap_or(0);
            break;
        }
        let whole = read_repository(
            pool,
            source_id,
            source,
            files,
            &mut writer,
            repository,
            bound,
            &mut budget,
            deadline,
            &mut passed,
        )
        .await?;
        if whole {
            passed.repositories += 1;
            if repository.held.is_none() {
                done_before += 1;
            }
        } else {
            passed.owed += 1;
        }
        let text_bytes: i64 =
            sqlx::query_scalar("select coalesce(sum(text_bytes), 0)::bigint from sync.code_repo")
                .fetch_one(pool)
                .await?;
        report(FilesProgress {
            source_id: source_id.to_owned(),
            done: done_before,
            owed: total_owed - u32::try_from(index + 1).unwrap_or(0) + passed.owed,
            text_bytes: u64::try_from(text_bytes).unwrap_or(0),
        });
    }
    drop(writer);
    if passed.repositories > 0 || passed.gone > 0 {
        files.index.set_repos(load_repos(pool).await?);
    }
    if passed != Passed::default() {
        tracing::info!(
            source_id,
            repositories = passed.repositories,
            owed = passed.owed,
            files = passed.files,
            removed = passed.removed,
            bytes = passed.bytes,
            from_clones = passed.from_clones,
            archives = passed.archives,
            reads = passed.reads,
            gone = passed.gone,
            ms = started.elapsed().as_millis(),
            "files pass"
        );
    }
    Ok(passed)
}

/// Remove every repository `sync.code_gone` names from the index, then the
/// rows: the index first, so a crash between leaves the rows to do again.
async fn remove_gone(pool: &PgPool, index: &FileIndex) -> Result<u32, PassError> {
    let gone: Vec<i32> = sqlx::query_scalar("select no from sync.code_gone order by no")
        .fetch_all(pool)
        .await?;
    if gone.is_empty() {
        return Ok(0);
    }
    blocking(|| -> Result<(), PassError> {
        let mut writer = index.writer()?;
        for no in &gone {
            writer.remove_repo(u64::try_from(*no).unwrap_or(0));
        }
        writer.commit()?;
        Ok(())
    })?;
    sqlx::query("delete from sync.code_gone where no = any($1)")
        .bind(&gone)
        .execute(pool)
        .await?;
    Ok(u32::try_from(gone.len()).unwrap_or(u32::MAX))
}

/// Where the text kept is over `bound`: the largest repositories that keep
/// text are set to be read whole again, until what the rest keep fits. Their
/// read then writes them with names only (`left_out` says `bound`).
async fn shrink_to(pool: &PgPool, bound: u64) -> Result<(), sqlx::Error> {
    let kept: Vec<(String, i64)> = sqlx::query_as(
        "select repository_id, text_bytes from sync.code_repo
          where text_bytes > 0 order by text_bytes desc, repository_id",
    )
    .fetch_all(pool)
    .await?;
    let mut total: u64 = kept
        .iter()
        .map(|(_, bytes)| u64::try_from(*bytes).unwrap_or(0))
        .sum();
    let mut again = Vec::new();
    for (repository, bytes) in kept {
        if total <= bound {
            break;
        }
        total = total.saturating_sub(u64::try_from(bytes).unwrap_or(0));
        again.push(repository);
    }
    if !again.is_empty() {
        sqlx::query("update sync.code_repo set commit_sha = null where repository_id = any($1)")
            .bind(&again)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// A tree path's depth: its folders.
fn depth(path: &str) -> usize {
    path.bytes().filter(|byte| *byte == b'/').count()
}

/// Read one repository owed a read, write what changed, and record it.
/// Answers whether it now stands at its head: `false` where the bounds cut
/// it, its row left at the old commit.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
async fn read_repository(
    pool: &PgPool,
    source_id: &str,
    source: &dyn Source,
    files: &Files,
    writer: &mut FileWriter,
    owed: &Owed,
    bound: u64,
    budget: &mut (u32, u32),
    deadline: Instant,
    passed: &mut Passed,
) -> Result<bool, PassError> {
    let repository = owed.repository_id.as_str();
    // Its number: the row made now, at no commit, so a crash leaves it to be
    // read whole.
    let no: i32 = match owed.no {
        Some(no) => no,
        None => {
            sqlx::query_scalar(
                "insert into sync.code_repo (repository_id, source_id) values ($1, $2)
                 on conflict (repository_id) do update set source_id = excluded.source_id
                 returning no",
            )
            .bind(repository)
            .bind(source_id)
            .fetch_one(pool)
            .await?
        }
    };
    let no_index = u64::try_from(no).unwrap_or(0);
    let whole = owed.held.is_none();

    // 3. The tree, flat: every regular file's path, size and blob.
    let tree: Vec<TreeFile> = sqlx::query_as(FLAT_TREE_SQL)
        .bind(source_id)
        .bind(&owed.root)
        .fetch_all(pool)
        .await?;
    // 4. What the index holds of it.
    let held: HashMap<String, Held> = sqlx::query_as::<_, (String, Vec<u8>, bool)>(
        "select path, sha, has_text from sync.code_file where repository_id = $1",
    )
    .bind(repository)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(path, sha, has_text)| (path, Held { sha, has_text }))
    .collect();

    // 5. The rules by name and size, the `.gitattributes` first.
    let attributes = match tree.iter().find(|file| file.path == ".gitattributes") {
        Some(file) => read_one(
            files,
            source,
            repository,
            &owed.commit,
            file,
            budget,
            passed,
        )
        .await
        .and_then(|bytes| code_text::decode(&bytes).map(|text| Attributes::parse(&text)))
        .unwrap_or_default(),
        None => Attributes::default(),
    };
    let mut left_out: HashMap<&'static str, u32> = HashMap::new();
    let mut eligible: Vec<&TreeFile> = Vec::new();
    for file in &tree {
        let reason = code_text::refuses_path(&file.path)
            .or_else(|| code_text::refuses_size(u64::try_from(file.size).unwrap_or(u64::MAX)))
            .or_else(|| attributes.marks(&file.path).then_some(LeftOut::Marked));
        match reason {
            Some(reason) => *left_out.entry(reason.word()).or_default() += 1,
            None => eligible.push(file),
        }
    }
    // The repository's share, shallowest path first, then by path.
    eligible.sort_by(|a, b| {
        depth(&a.path)
            .cmp(&depth(&b.path))
            .then_with(|| a.path.cmp(&b.path))
    });
    let (mut share_files, mut share_bytes) = (0_usize, 0_u64);
    let mut text_wanted: HashSet<&str> = HashSet::new();
    for file in &eligible {
        let size = u64::try_from(file.size).unwrap_or(0);
        if share_files >= SHARE_FILES || share_bytes + size > SHARE_BYTES {
            *left_out.entry(LeftOut::Share.word()).or_default() += 1;
            continue;
        }
        share_files += 1;
        share_bytes += size;
        text_wanted.insert(file.path.as_str());
    }
    // The bound over every repository's text: this one's planned text with
    // what the others keep.
    let others: i64 = sqlx::query_scalar(
        "select coalesce(sum(text_bytes), 0)::bigint from sync.code_repo where repository_id <> $1",
    )
    .bind(repository)
    .fetch_one(pool)
    .await?;
    if u64::try_from(others).unwrap_or(0) + share_bytes > bound {
        *left_out.entry(LeftOut::Bound.word()).or_default() +=
            u32::try_from(text_wanted.len()).unwrap_or(u32::MAX);
        text_wanted.clear();
    }

    // What to write: a new or changed file, and one whose text is no longer
    // kept. A repository read whole (no commit) is compared all the same: a
    // file a cut pass already wrote is not read again, and one bring-up
    // found differing has no row left to compare with.
    let mut names_only: Vec<&TreeFile> = Vec::new();
    let mut need_text: Vec<&TreeFile> = Vec::new();
    for file in &tree {
        let wants_text = text_wanted.contains(file.path.as_str());
        let stale = match held.get(&file.path) {
            None => true,
            Some(held) => held.sha != file.sha || (held.has_text && !wants_text),
        };
        if !stale {
            continue;
        }
        if wants_text {
            need_text.push(file);
        } else {
            names_only.push(file);
        }
    }
    let present: HashSet<&str> = tree.iter().map(|file| file.path.as_str()).collect();
    let removed: Vec<&String> = held
        .keys()
        .filter(|path| !present.contains(path.as_str()))
        .collect();

    // 6. The bytes: the clone, then one archive or single reads.
    let mut bytes: HashMap<String, Vec<u8>> = HashMap::new();
    if let Some(clones) = &files.clones {
        for file in &need_text {
            if let Some(read) = clones
                .blob(
                    repository,
                    &hex(&file.sha),
                    usize::try_from(code_text::SIZE_CAP).unwrap_or(usize::MAX),
                )
                .await
            {
                passed.from_clones += 1;
                bytes.insert(file.path.clone(), read);
            }
        }
    }
    let missing: Vec<&TreeFile> = need_text
        .iter()
        .copied()
        .filter(|file| !bytes.contains_key(&file.path))
        .collect();
    let mut complete = true;
    let tree_bytes: u64 = tree
        .iter()
        .map(|file| u64::try_from(file.size).unwrap_or(0))
        .sum();
    if by_archive(missing.len(), tree_bytes)
        && budget.0 > 0
        && Instant::now() < deadline
        && !archive_cut(repository)
    {
        let wanted: HashSet<String> = missing.iter().map(|file| file.path.clone()).collect();
        let wants = |path: &str| wanted.contains(path);
        let mut got: Vec<(String, Vec<u8>)> = Vec::new();
        let mut each = |path: String, read: Vec<u8>| got.push((path, read));
        let asked = tokio::time::timeout_at(
            tokio::time::Instant::from_std(deadline),
            source.read_archive(repository, &owed.commit, &wants, &mut each),
        )
        .await;
        match asked {
            Ok(Ok(Some(requests))) => {
                budget.0 -= 1;
                passed.archives += requests.max(1);
                bytes.extend(got);
            }
            // No archive from this source: a file at a time below.
            Ok(Ok(None)) => {}
            Ok(Err(error)) => {
                budget.0 -= 1;
                passed.archives += 1;
                tracing::debug!(source_id, repository, %error, "an archive was not read");
            }
            // Cut by the deadline: nothing of it kept, and asked no more.
            Err(_) => {
                complete = false;
                if let Ok(mut cut) = ARCHIVE_CUT.lock() {
                    cut.insert(repository.to_owned());
                }
                tracing::info!(
                    source_id,
                    repository,
                    "an archive the run's deadline cut: read a file at a time from now"
                );
            }
        }
    }
    for file in &need_text {
        if bytes.contains_key(&file.path) {
            continue;
        }
        if budget.1 == 0 || Instant::now() >= deadline {
            complete = false;
            break;
        }
        match source
            .read_blob(repository, &owed.commit, &file.path, code_text::SIZE_CAP)
            .await
        {
            Ok(Some(read)) => {
                budget.1 -= 1;
                passed.reads += 1;
                bytes.insert(file.path.clone(), read);
            }
            Ok(None) => {
                budget.1 -= 1;
                passed.reads += 1;
            }
            Err(error) => {
                budget.1 -= 1;
                passed.reads += 1;
                if crate::stored_trees::forge_failed(&error) {
                    complete = false;
                    break;
                }
            }
        }
    }

    // 7. The rules on the bytes, the decode, and the writes.
    let mut written: Vec<(&TreeFile, bool)> = Vec::new();
    blocking(|| -> Result<(), PassError> {
        let mut since_commit = 0_usize;
        let mut put = |writer: &mut FileWriter,
                       file: &TreeFile,
                       text: Option<&str>|
         -> Result<(), PassError> {
            writer.put(no_index, &file.path, text)?;
            since_commit += 1;
            if since_commit >= COMMIT_EVERY {
                writer.commit()?;
                since_commit = 0;
            }
            Ok(())
        };
        for file in &names_only {
            put(writer, file, None)?;
            written.push((file, false));
        }
        for file in &need_text {
            let Some(read) = bytes.get(&file.path) else {
                // Not read inside the bounds: left as it was for the next run.
                continue;
            };
            let text = match code_text::refuses_bytes(read) {
                Some(reason) => {
                    *left_out.entry(reason.word()).or_default() += 1;
                    None
                }
                None => code_text::decode(read),
            };
            if let Some(text) = &text {
                passed.bytes += u64::try_from(text.len()).unwrap_or(0);
            }
            put(writer, file, text.as_deref())?;
            written.push((file, text.is_some()));
        }
        for path in &removed {
            writer.remove(no_index, path);
        }
        // 8. Commit, then record.
        writer.commit()?;
        Ok(())
    })?;
    passed.files += u32::try_from(written.len()).unwrap_or(u32::MAX);
    passed.removed += u32::try_from(removed.len()).unwrap_or(u32::MAX);

    let mut tx = pool.begin().await?;
    if !removed.is_empty() {
        sqlx::query("delete from sync.code_file where repository_id = $1 and path = any($2)")
            .bind(repository)
            .bind(&removed)
            .execute(&mut *tx)
            .await?;
    }
    if !written.is_empty() {
        let paths: Vec<&str> = written.iter().map(|(file, _)| file.path.as_str()).collect();
        let shas: Vec<&[u8]> = written
            .iter()
            .map(|(file, _)| file.sha.as_slice())
            .collect();
        let sizes: Vec<i64> = written.iter().map(|(file, _)| file.size).collect();
        let texts: Vec<bool> = written.iter().map(|(_, text)| *text).collect();
        sqlx::query(
            "insert into sync.code_file (repository_id, path, sha, size, has_text)
             select $1, w.path, w.sha, w.size, w.has_text
               from unnest($2::text[], $3::bytea[], $4::bigint[], $5::boolean[]) w (path, sha, size, has_text)
             on conflict (repository_id, path) do update
               set sha = excluded.sha, size = excluded.size, has_text = excluded.has_text",
        )
        .bind(repository)
        .bind(&paths)
        .bind(&shas)
        .bind(&sizes)
        .bind(&texts)
        .execute(&mut *tx)
        .await?;
    }
    let left_out = serde_json::to_value(&left_out).unwrap_or_default();
    sqlx::query(
        "update sync.code_repo cr
            set commit_sha = $2, read_at = now(),
                files = c.files, text_files = c.text_files, text_bytes = c.text_bytes,
                left_out = case when $4 then $3 else cr.left_out || $3 end
           from (select count(*)::int as files,
                        (count(*) filter (where has_text))::int as text_files,
                        coalesce(sum(size) filter (where has_text), 0)::bigint as text_bytes
                   from sync.code_file where repository_id = $1) c
          where cr.repository_id = $1",
    )
    .bind(repository)
    .bind(if complete {
        Some(owed.commit.as_str())
    } else {
        owed.held.as_deref()
    })
    .bind(&left_out)
    .bind(whole)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(complete)
}

/// One file's bytes, from a checkout or a single read: the `.gitattributes`.
async fn read_one(
    files: &Files,
    source: &dyn Source,
    repository: &str,
    commit: &str,
    file: &TreeFile,
    budget: &mut (u32, u32),
    passed: &mut Passed,
) -> Option<Vec<u8>> {
    if let Some(clones) = &files.clones
        && let Some(read) = clones.blob(repository, &hex(&file.sha), 64 << 10).await
    {
        passed.from_clones += 1;
        return Some(read);
    }
    if budget.1 == 0 {
        return None;
    }
    budget.1 -= 1;
    passed.reads += 1;
    source
        .read_blob(repository, commit, &file.path, 64 << 10)
        .await
        .ok()
        .flatten()
}

/// Every regular file of the stored tree from root `$2` of source `$1`: its
/// path, size and blob, each level read by `sync.git_tree`'s key (D7 point
/// 3), a lateral lookup a level so no plan reads every level of the source
/// (D15: a join on the key alone was planned as a scan of the source's
/// levels on tables never analyzed). No link and no submodule.
pub const FLAT_TREE_SQL: &str = "\
with recursive walk (sha, prefix) as (
  select $2::bytea, ''::text
  union all
  select e.sha, w.prefix || e.name || '/'
    from walk w
   cross join lateral (select t.names, t.modes, t.shas from sync.git_tree t
                        where t.source_id = $1 and t.sha = w.sha limit 1) t
   cross join lateral unnest(t.names, t.modes, t.shas) as e (name, mode, sha)
   where e.mode = 16384
)
select w.prefix || e.name as path, e.size, e.sha
  from walk w
 cross join lateral (select t.names, t.modes, t.sizes, t.shas from sync.git_tree t
                      where t.source_id = $1 and t.sha = w.sha limit 1) t
 cross join lateral unnest(t.names, t.modes, t.sizes, t.shas) as e (name, mode, size, sha)
 where e.mode in (33188, 33261)";

/// One repository as [`load_repos`] reads it: its number, entity id,
/// source, whether it is a favourite, and when it changed.
type RepoRow = (
    i32,
    String,
    String,
    bool,
    Option<chrono::DateTime<chrono::Utc>>,
    Option<String>,
);

/// The repositories the index knows, from the database: each number's
/// entity id, source, whether it is a favourite, and when it changed.
///
/// # Errors
///
/// A database failure.
pub async fn load_repos(pool: &PgPool) -> Result<Repos, sqlx::Error> {
    let rows: Vec<RepoRow> = sqlx::query_as(
        "select cr.no, cr.repository_id, cr.source_id,
                exists (select 1 from knobas.favourite f where f.entity_id = cr.repository_id),
                i.item_updated_at,
                (select bt.branch from sync.branch_tree bt
                  where bt.repository_id = cr.repository_id and bt.commit_sha = cr.commit_sha
                  order by bt.branch = 'main' desc, bt.branch limit 1)
           from sync.code_repo cr
           join sync.item i on i.entity_id = cr.repository_id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(no, entity_id, source_id, favourite, updated, branch)| {
            (
                u64::try_from(no).unwrap_or(0),
                Repo {
                    entity_id,
                    source_id,
                    favourite,
                    updated: updated.map_or(0, |at| at.timestamp()),
                    branch: branch.unwrap_or_default(),
                },
            )
        })
        .collect())
}

/// What bring-up's comparison found (D7).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Reconciled {
    /// Repositories whose counts differed: their documents removed, to be
    /// read whole.
    pub again: u32,
    /// Numbers the index held and the database did not: owed a removal.
    pub strays: u32,
}

/// Bring-up's comparison of the two stores (D7): the files the index holds
/// of every repository against `sync.code_repo.files`. A repository whose
/// counts differ has its documents removed, its files' rows deleted and its
/// commit set to null, so the next pass reads it whole; a number the index
/// holds and the database does not goes into `sync.code_gone`. Then the
/// repositories are loaded into the index.
///
/// # Errors
///
/// A database or index failure.
pub async fn reconcile(pool: &PgPool, index: &FileIndex) -> Result<Reconciled, PassError> {
    let mut reconciled = Reconciled::default();
    if !index.is_open() {
        return Ok(reconciled);
    }
    let held = index.held()?;
    let rows: Vec<(i32, String, i32)> =
        sqlx::query_as("select no, repository_id, files from sync.code_repo")
            .fetch_all(pool)
            .await?;
    let known: HashSet<u64> = rows
        .iter()
        .map(|(no, _, _)| u64::try_from(*no).unwrap_or(0))
        .collect();
    let differ: Vec<(u64, String)> = rows
        .iter()
        .filter(|(no, _, files)| {
            held.get(&u64::try_from(*no).unwrap_or(0))
                .copied()
                .unwrap_or(0)
                != u32::try_from(*files).unwrap_or(0)
        })
        .map(|(no, repository, _)| (u64::try_from(*no).unwrap_or(0), repository.clone()))
        .collect();
    let strays: Vec<i32> = held
        .keys()
        .filter(|no| !known.contains(no))
        .map(|no| i32::try_from(*no).unwrap_or(i32::MAX))
        .collect();
    if !differ.is_empty() {
        blocking(|| -> Result<(), PassError> {
            let mut writer = index.writer()?;
            for (no, _) in &differ {
                writer.remove_repo(*no);
            }
            writer.commit()?;
            Ok(())
        })?;
        let repositories: Vec<&String> = differ.iter().map(|(_, repository)| repository).collect();
        let mut tx = pool.begin().await?;
        sqlx::query("delete from sync.code_file where repository_id = any($1)")
            .bind(&repositories)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "update sync.code_repo
                set commit_sha = null, files = 0, text_files = 0, text_bytes = 0
              where repository_id = any($1)",
        )
        .bind(&repositories)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        reconciled.again = u32::try_from(differ.len()).unwrap_or(u32::MAX);
    }
    if !strays.is_empty() {
        sqlx::query(
            "insert into sync.code_gone (no) select unnest($1::int[]) on conflict do nothing",
        )
        .bind(&strays)
        .execute(pool)
        .await?;
        reconciled.strays = u32::try_from(strays.len()).unwrap_or(u32::MAX);
    }
    index.set_repos(load_repos(pool).await?);
    if reconciled != Reconciled::default() {
        tracing::info!(
            again = reconciled.again,
            strays = reconciled.strays,
            "the files index and the database differed"
        );
    }
    Ok(reconciled)
}
