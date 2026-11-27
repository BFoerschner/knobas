//! *Text search* in one repository (ADR-0062): every line of the files the
//! reader is looking at that holds what they typed, ripgrep's way.
//!
//! What is read is what the Files tab draws, never a guess:
//!
//! - **a branch whose commit a clone here holds**: that commit's tree, its
//!   blobs read in knobas's own process (`knobas-git`, ADR-0043) by the
//!   sha the tree kept for it ([`super::local::tree`]). Nothing is checked
//!   out and nothing is written: the working tree is never touched
//!   (ADR-0016).
//! - **the working tree** (`~working-tree`, ADR-0049): the files git lists
//!   there, tracked or untracked and not ignored ([`super::working::listing`]),
//!   read from disk as they are now, never through a link. git's own
//!   `.gitignore` reading decides what is left out, the same list the tab's
//!   `M`/`A` marks come from.
//! - **no clone, the default branch**: the text the files index keeps
//!   (ADR-0048), scanned line by line with the same matcher, and the answer
//!   says it came from there, at the branch the index reads.
//!
//! Anything else is no answer (`TextFrom::None`), with the reason: a branch
//! the index does not read and no clone that holds it cannot be searched
//! faithfully, and a search over the wrong branch would be a lie.
//!
//! What is typed is its words (#2561): each a literal, in the order typed,
//! on one line with anything between, so `xy room` finds `xy_room`,
//! `xyRoom` and `xy = room`; a `"`-quoted phrase is one literal, spaces
//! included, and text with no space is the literal it was. *.\** is a
//! pattern as typed. Not letter by letter: over a whole repository that
//! finds nearly every line.
//!
//! Each scan stops at [`MOST_HITS`] lines; a file over [`FILE_CAP`] bytes
//! and a file with a NUL in its first 8,000 bytes (git's binary sniff) are
//! counted and skipped. A newer search of the same place stops the one
//! before it ([`Turn`]), so typing never piles scans up behind each other.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

use regex::bytes::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

/// The most matching lines one search answers; past it the answer is `cut`.
pub const MOST_HITS: usize = 500;
/// The most lines one file shows; the rest are counted in its `more`.
pub const MOST_PER_FILE: usize = 20;
/// The most file names a search answers beside its lines.
pub const MOST_NAMES: usize = 50;
/// A file over this many bytes is skipped, as the files index skips one
/// (`knobas_source::file_text::BYTE_CAP` is the file view's own cap).
pub const FILE_CAP: u64 = 2 * 1024 * 1024;
/// A line shown is cut to about this many bytes around its first hit.
const LINE_SHOWN: usize = 240;
/// How much of the line before the first hit a cut line keeps.
const LINE_BEFORE: usize = 60;

/// What the reader asked: the words, and the two switches beside them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct TextQuery {
    /// What is typed: words, matched in order on one line with anything
    /// between (a `"`-quoted phrase as typed, text with no space as one
    /// literal), or a pattern when `regex` is on.
    pub text: String,
    /// *Aa*: match the case typed. Off, the case is smart: a query with an
    /// upper-case letter matches its case, one without matches every case.
    pub case_sensitive: bool,
    /// *.\**: `text` is a regular expression (Rust's `regex` syntax).
    pub regex: bool,
    /// Also answer the paths whose name holds `text` (*Search in
    /// repository*, which may be a file's name as well as its text).
    pub names: bool,
}

/// Where an answer's lines were read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "entity.ts")]
pub enum TextFrom {
    /// The branch's commit in a clone on this computer.
    Clone,
    /// The clone's working tree, as it is on disk.
    WorkingTree,
    /// The files index's copy of the default branch.
    Index,
    /// Nowhere: `why` says why.
    None,
}

/// One part of a line shown: matched or not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct TextPart {
    /// Raw file text: drawn as text, never as markup.
    pub text: String,
    pub hit: bool,
}

/// One matching line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct TextHit {
    /// From 1.
    pub line: u32,
    /// The line, cut around its first hit when long, in parts.
    pub parts: Vec<TextPart>,
}

/// One file with matching lines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct TextFile {
    /// From the repository's top.
    pub path: String,
    /// Its first [`MOST_PER_FILE`] matching lines.
    pub hits: Vec<TextHit>,
    /// How many more lines of it match.
    pub more: u32,
}

/// What a search found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct TextFound {
    pub from: TextFrom,
    /// The commit read: the clone's, or the working tree's `HEAD`.
    pub commit: Option<String>,
    /// The branch the files index reads, for [`TextFrom::Index`].
    pub branch: Option<String>,
    /// The files whose lines match, in path order.
    pub files: Vec<TextFile>,
    /// Paths whose name holds the text, when asked ([`TextQuery::names`]).
    pub names: Vec<String>,
    /// How many files were read.
    pub searched: u32,
    /// How many were skipped: binary, or over [`FILE_CAP`].
    pub skipped: u32,
    /// The search stopped at [`MOST_HITS`] lines (or the names at
    /// [`MOST_NAMES`]): there are more.
    pub cut: bool,
    /// The pattern does not parse: what is wrong with it.
    pub invalid: Option<String>,
    /// Why nothing could be searched, for [`TextFrom::None`].
    pub why: Option<String>,
    /// A newer search stopped this one: draw nothing of it.
    pub stale: bool,
}

impl TextFound {
    /// Nothing searched, and why.
    #[must_use]
    pub fn nowhere(why: impl Into<String>) -> Self {
        Self {
            why: Some(why.into()),
            ..Self::empty(TextFrom::None)
        }
    }

    fn empty(from: TextFrom) -> Self {
        Self {
            from,
            commit: None,
            branch: None,
            files: Vec::new(),
            names: Vec::new(),
            searched: 0,
            skipped: 0,
            cut: false,
            invalid: None,
            why: None,
            stale: false,
        }
    }
}

/// A query made ready to match bytes.
#[derive(Debug, Clone)]
pub struct Matcher {
    regex: Regex,
    /// The one literal typed, folded when the case is not matched: for names.
    name: Option<(String, bool)>,
    /// The text is a pattern, or words: names are matched by `regex` too.
    pattern: bool,
    /// Words typed apart: `regex` holds one group a word, and each group is
    /// marked, not the run between them.
    words: bool,
}

/// What is typed outside a pattern, as the literals it is matched by
/// (#2561): text with no space is one literal, quotes and all, as typed;
/// text with spaces is its words, a `"` at a word's start holding
/// everything up to the next `"` (or the end) as one phrase, spaces
/// included.
fn terms(text: &str) -> Vec<String> {
    let text = text.trim();
    if !text.contains(char::is_whitespace) {
        return vec![text.to_owned()];
    }
    let mut terms = Vec::new();
    let mut chars = text.chars().peekable();
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        let Some(first) = chars.next() else {
            return terms;
        };
        let term: String = if first == '"' {
            chars.by_ref().take_while(|c| *c != '"').collect()
        } else {
            std::iter::once(first)
                .chain(std::iter::from_fn(|| chars.next_if(|c| !c.is_whitespace())))
                .collect()
        };
        if !term.is_empty() {
            terms.push(term);
        }
    }
}

impl Matcher {
    /// The matcher for `query`.
    ///
    /// # Errors
    ///
    /// What is wrong with the query, in words: nothing typed, or a pattern
    /// that does not parse.
    pub fn new(query: &TextQuery) -> Result<Self, String> {
        if query.text.trim().is_empty() {
            return Err("Type something to search for.".to_owned());
        }
        let terms = if query.regex {
            Vec::new()
        } else {
            terms(&query.text)
        };
        let words = terms.len() > 1;
        let pattern = match terms.as_slice() {
            [] if query.regex => query.text.clone(),
            [] => return Err("Type something to search for.".to_owned()),
            [one] => regex::escape(one),
            // Each word a group, in the order typed, anything but a line
            // break between them, as little as reaches the next word.
            more => more
                .iter()
                .map(|term| format!("({})", regex::escape(term)))
                .collect::<Vec<_>>()
                .join("(?-u:[^\\n])*?"),
        };
        let insensitive = !query.case_sensitive && !typed_upper(&query.text, query.regex);
        let regex = RegexBuilder::new(&pattern)
            .case_insensitive(insensitive)
            .size_limit(8 * 1024 * 1024)
            .build()
            .map_err(|error| refusal(&error))?;
        let name = match terms.as_slice() {
            [one] if query.names => Some(if insensitive {
                (one.to_lowercase(), true)
            } else {
                (one.clone(), false)
            }),
            _ => None,
        };
        Ok(Self {
            regex,
            name,
            pattern: query.names && (query.regex || words),
            words,
        })
    }

    /// Whether `bytes` holds a hit anywhere: the cheap test before lines.
    #[must_use]
    pub fn finds(&self, bytes: &[u8]) -> bool {
        self.regex.is_match(bytes)
    }

    /// Whether a path's name holds the text, where names were asked.
    #[must_use]
    pub fn names(&self, path: &str) -> bool {
        if self.pattern {
            return self.regex.is_match(path.as_bytes());
        }
        let Some((text, folded)) = &self.name else {
            return false;
        };
        if *folded {
            path.to_lowercase().contains(text.as_str())
        } else {
            path.contains(text.as_str())
        }
    }

    /// The matching lines of `bytes`: the first `most`, and how many more.
    #[must_use]
    pub fn lines(&self, bytes: &[u8], most: usize) -> (Vec<TextHit>, u32) {
        let mut hits = Vec::new();
        let mut more = 0_u32;
        for (at, line) in bytes.split(|b| *b == b'\n').enumerate() {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            let found: Vec<(usize, usize)> = if self.words {
                self.regex
                    .captures_iter(line)
                    .flat_map(|caps| {
                        caps.iter()
                            .skip(1)
                            .flatten()
                            .map(|m| (m.start(), m.end()))
                            .collect::<Vec<_>>()
                    })
                    .collect()
            } else {
                self.regex
                    .find_iter(line)
                    .filter(|m| m.end() > m.start())
                    .map(|m| (m.start(), m.end()))
                    .collect()
            };
            if found.is_empty() {
                continue;
            }
            if hits.len() >= most {
                more += 1;
                continue;
            }
            hits.push(TextHit {
                line: u32::try_from(at + 1).unwrap_or(u32::MAX),
                parts: parts(line, &found),
            });
        }
        (hits, more)
    }
}

/// Whether the typed text holds an upper-case letter of its own: in a
/// pattern, a letter after `\` (`\W`, `\S`, `\P{…}`) is the syntax's, not
/// typed text, as ripgrep's smart case reads it.
fn typed_upper(text: &str, regex: bool) -> bool {
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if regex && c == '\\' {
            chars.next();
            continue;
        }
        if c.is_uppercase() {
            return true;
        }
    }
    false
}

/// A pattern's parse error in one line: its last, without `error: `.
fn refusal(error: &regex::Error) -> String {
    let words = error.to_string();
    let last = words
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");
    let last = last.trim().strip_prefix("error: ").unwrap_or(last.trim());
    format!("The pattern does not parse: {last}.")
}

/// `line` in parts, its hits `found` marked, cut around the first hit to
/// about [`LINE_SHOWN`] bytes, an ellipsis where it was cut.
fn parts(line: &[u8], found: &[(usize, usize)]) -> Vec<TextPart> {
    let (mut from, mut to) = (0, line.len());
    if line.len() > LINE_SHOWN {
        from = found[0].0.saturating_sub(LINE_BEFORE);
        while from > 0 && is_inside_char(line[from]) {
            from -= 1;
        }
        to = (from + LINE_SHOWN).min(line.len());
        while to < line.len() && is_inside_char(line[to]) {
            to -= 1;
        }
    }
    let mut parts = Vec::new();
    let mut push = |bytes: &[u8], hit: bool| {
        if !bytes.is_empty() {
            parts.push(TextPart {
                text: String::from_utf8_lossy(bytes).into_owned(),
                hit,
            });
        }
    };
    if from > 0 {
        push("…".as_bytes(), false);
    }
    let mut at = from;
    for &(start, end) in found {
        let (start, end) = (start.clamp(from, to), end.clamp(from, to));
        if end <= at {
            continue;
        }
        push(&line[at..start.max(at)], false);
        push(&line[start.max(at)..end], true);
        at = end;
    }
    push(&line[at..to], false);
    if to < line.len() {
        push("…".as_bytes(), false);
    }
    parts
}

/// A UTF-8 continuation byte: no place to cut.
fn is_inside_char(byte: u8) -> bool {
    byte & 0b1100_0000 == 0b1000_0000
}

// -- the scan -------------------------------------------------------------------

/// The searches under way, one lane a place searched (the tab's entity, a
/// repository or a branch): each search takes its lane's next turn when it is
/// asked, before it reads anything,
/// and a scan whose turn is no longer its lane's last stops, so the keys
/// typed before the last one cost nothing more once it starts.
fn lanes() -> std::sync::MutexGuard<'static, std::collections::HashMap<String, Arc<AtomicU64>>> {
    static LANES: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, Arc<AtomicU64>>>,
    > = std::sync::OnceLock::new();
    LANES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// One search's turn in its lane ([`lanes`]).
#[derive(Debug, Clone)]
pub struct Turn(Arc<AtomicU64>, u64);

impl Turn {
    /// The next turn in `lane`: every scan before it there stops.
    #[must_use]
    pub fn take(lane: &str) -> Self {
        let counter = lanes().entry(lane.to_owned()).or_default().clone();
        let n = counter.fetch_add(1, Ordering::SeqCst) + 1;
        Self(counter, n)
    }

    fn passed(&self) -> bool {
        self.0.load(Ordering::SeqCst) != self.1
    }
}

/// A file to read: its path, its size where it is known before the read.
struct Item {
    path: String,
    size: Option<u64>,
}

/// What a read of one file found.
enum Read<'a> {
    Bytes(std::borrow::Cow<'a, [u8]>),
    /// Over [`FILE_CAP`], or binary.
    Skipped,
    /// Not there (any more): neither read nor skipped.
    Gone,
}

/// Every item read on a few threads, each with its own `reader()`, matched
/// by `matcher`.
///
/// The threads take the items in order and finish each they took, so what
/// was read when the cap is reached is every item up to some place: the
/// answer is the first [`MOST_HITS`] lines in the items' order, the same
/// whichever thread read which.
fn scan<'a, R>(
    items: &'a [Item],
    reader: &(dyn Fn() -> R + Sync),
    matcher: &Matcher,
    turn: &Turn,
    from: TextFrom,
) -> TextFound
where
    R: FnMut(usize, &'a Item) -> Read<'a>,
{
    let next = AtomicUsize::new(0);
    let shown = AtomicUsize::new(0);
    let searched = AtomicU32::new(0);
    let skipped = AtomicU32::new(0);
    let stop = AtomicBool::new(false);
    let stale = AtomicBool::new(false);
    let threads = std::thread::available_parallelism()
        .map_or(2, std::num::NonZero::get)
        .clamp(1, 8)
        .min(items.len().max(1));
    let found: Vec<(usize, TextFile)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut read = reader();
                    let mut mine = Vec::new();
                    while !stop.load(Ordering::Relaxed) {
                        if turn.passed() {
                            stale.store(true, Ordering::Relaxed);
                            stop.store(true, Ordering::Relaxed);
                            break;
                        }
                        let at = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(at) else {
                            break;
                        };
                        if item.size.is_some_and(|size| size > FILE_CAP) {
                            skipped.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }
                        let bytes = match read(at, item) {
                            Read::Bytes(bytes) => bytes,
                            Read::Skipped => {
                                skipped.fetch_add(1, Ordering::Relaxed);
                                continue;
                            }
                            Read::Gone => continue,
                        };
                        if bytes.len() as u64 > FILE_CAP
                            || knobas_source::file_text::is_binary(&bytes)
                        {
                            skipped.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }
                        searched.fetch_add(1, Ordering::Relaxed);
                        if !matcher.finds(&bytes) {
                            continue;
                        }
                        let (hits, more) = matcher.lines(&bytes, MOST_PER_FILE);
                        if hits.is_empty() {
                            continue;
                        }
                        if shown.fetch_add(hits.len(), Ordering::Relaxed) + hits.len() >= MOST_HITS
                        {
                            stop.store(true, Ordering::Relaxed);
                        }
                        mine.push((
                            at,
                            TextFile {
                                path: item.path.clone(),
                                hits,
                                more,
                            },
                        ));
                    }
                    mine
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap_or_default())
            .collect()
    });
    let mut answer = TextFound::empty(from);
    if stale.load(Ordering::Relaxed) {
        answer.stale = true;
        return answer;
    }
    let mut found = found;
    found.sort_unstable_by_key(|(at, _)| *at);
    keep_first(found.into_iter().map(|(_, file)| file), &mut answer);
    // The cap was reached exactly with nothing after it read: still cut,
    // since the items after were never read.
    if stop.load(Ordering::Relaxed) && next.load(Ordering::Relaxed) < items.len() {
        answer.cut = true;
    }
    answer.searched = searched.load(Ordering::Relaxed);
    answer.skipped = skipped.load(Ordering::Relaxed);
    if matcher.name.is_some() {
        for item in items {
            if matcher.names(&item.path) {
                if answer.names.len() == MOST_NAMES {
                    answer.cut = true;
                    break;
                }
                answer.names.push(item.path.clone());
            }
        }
    }
    answer
}

/// `files`, in their order, into `answer` up to [`MOST_HITS`] lines: the
/// file the cap falls in counts the lines cut in its `more`.
fn keep_first(files: impl IntoIterator<Item = TextFile>, answer: &mut TextFound) {
    let mut left = MOST_HITS;
    for mut file in files {
        if left == 0 {
            answer.cut = true;
            break;
        }
        if file.hits.len() > left {
            let dropped = file.hits.len() - left;
            file.hits.truncate(left);
            file.more += u32::try_from(dropped).unwrap_or(u32::MAX);
            answer.cut = true;
        }
        left -= file.hits.len();
        answer.files.push(file);
    }
}

/// `query` over the text the files index keeps of repository `repo` (an
/// entity id), the copy of its default branch the index reads (ADR-0048):
/// what is searched where no clone holds that branch. `None` where the
/// index knows no such repository. A file the index keeps by its path alone
/// (binary, too large, text not kept) is neither read nor named.
pub async fn scan_index(
    index: &knobas_files::FileIndex,
    repo: &str,
    matcher: &Matcher,
    turn: &Turn,
) -> Option<TextFound> {
    let repos = index.repos();
    let (no, branch) = repos
        .iter()
        .find(|(_, known)| known.entity_id == repo)
        .map(|(no, known)| (*no, known.branch.clone()))?;
    let turn = turn.clone();
    let index = index.clone();
    let matcher = matcher.clone();
    tokio::task::spawn_blocking(move || {
        let mut answer = TextFound::empty(TextFrom::Index);
        let mut files: Vec<TextFile> = Vec::new();
        let mut names: Vec<String> = Vec::new();
        let mut shown = 0_usize;
        index.each_text(no, &mut |path, text| {
            if turn.passed() {
                answer.stale = true;
                return false;
            }
            answer.searched += 1;
            if matcher.names(path) {
                names.push(path.to_owned());
            }
            let bytes = text.as_bytes();
            if !matcher.finds(bytes) {
                return true;
            }
            let (hits, more) = matcher.lines(bytes, MOST_PER_FILE);
            if hits.is_empty() {
                return true;
            }
            shown += hits.len();
            files.push(TextFile {
                path: path.to_owned(),
                hits,
                more,
            });
            // The index gives files in no order: the first lines by path are
            // known only once all are read, so the walk reads on well past
            // the cap and stops only to bound what it holds.
            if shown >= 4 * MOST_HITS {
                answer.cut = true;
                return false;
            }
            true
        });
        if answer.stale {
            return TextFound {
                stale: true,
                ..TextFound::empty(TextFrom::Index)
            };
        }
        files.sort_unstable_by(|a, b| a.path.cmp(&b.path));
        keep_first(files, &mut answer);
        names.sort_unstable();
        if names.len() > MOST_NAMES {
            names.truncate(MOST_NAMES);
            answer.cut = true;
        }
        answer.names = names;
        answer.branch = (!branch.is_empty()).then_some(branch);
        answer
    })
    .await
    .ok()
}

/// `query` over the tree at `sha` in the clone at `path`, its blobs read in
/// knobas's own process (git where that cannot answer one, as every read of
/// a clone): `None` where the clone does not hold that commit.
pub async fn scan_tree(
    path: &Path,
    sha: &str,
    matcher: &Matcher,
    turn: &Turn,
) -> Option<TextFound> {
    let tree = super::local::tree(path, sha).await?;
    let turn = turn.clone();
    let path = path.to_path_buf();
    let matcher = matcher.clone();
    let runtime = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        use knobas_source::view::EntryKind;
        let (items, blobs): (Vec<Item>, Vec<&str>) = tree
            .entries
            .iter()
            .filter(|entry| entry.kind == EntryKind::File)
            .filter_map(|entry| {
                let blob = entry.blob.as_deref()?;
                Some((
                    Item {
                        path: entry.path.clone(),
                        size: entry.size,
                    },
                    blob,
                ))
            })
            .unzip();
        let want = usize::try_from(FILE_CAP).unwrap_or(usize::MAX) + 1;
        // A `gix` repository is a thread's own: each worker opens its handle.
        let reader = || {
            let repo = knobas_git::Repo::open(&path).ok();
            let (path, runtime, blobs) = (&path, &runtime, &blobs);
            move |at: usize, _: &Item| -> Read<'_> {
                let blob = blobs[at];
                let bytes = match repo.as_ref().map(|repo| repo.cat_blob(blob, want)) {
                    Some(Ok(bytes)) => bytes,
                    // Not answered here: git reads it, as every other read of a clone.
                    Some(Err(_)) | None => runtime.block_on(super::local::blob(path, blob, want)),
                };
                match bytes {
                    Some((_, true)) => Read::Skipped,
                    Some((bytes, false)) => Read::Bytes(bytes.into()),
                    None => Read::Gone,
                }
            }
        };
        let mut found = scan(&items, &reader, &matcher, &turn, TextFrom::Clone);
        found.commit = Some(tree.commit.clone());
        found
    })
    .await
    .ok()
}

/// `query` over the working tree of the clone at `path`, as it is on disk:
/// every file git lists there, tracked or untracked and not ignored
/// ([`super::working::listing`], the list the tab's marks come from), never
/// a link and never a submodule. `None` where git cannot list it.
pub async fn scan_disk(path: &Path, matcher: &Matcher, turn: &Turn) -> Option<TextFound> {
    let (status, index) = super::working::listing(path).await.ok()?;
    let head = super::working::head(path).await;
    let turn = turn.clone();
    let root = path.to_path_buf();
    let matcher = matcher.clone();
    tokio::task::spawn_blocking(move || {
        let mut paths: Vec<&str> = index
            .iter()
            // A link and a submodule are no file of text.
            .filter(|(_, (mode, _))| mode != "120000" && mode != "160000")
            .map(|(path, _)| path.as_str())
            .chain(status.untracked.iter().map(String::as_str))
            .collect();
        paths.sort_unstable();
        paths.dedup();
        let items: Vec<Item> = paths
            .into_iter()
            .map(|path| Item {
                path: path.to_owned(),
                size: None,
            })
            .collect();
        let reader = || {
            let root = &root;
            move |_: usize, item: &Item| -> Read<'_> {
                let Some(full) = super::working::safe_join(root, &item.path) else {
                    return Read::Gone;
                };
                if std::fs::symlink_metadata(&full).is_ok_and(|meta| meta.file_type().is_symlink())
                {
                    return Read::Gone;
                }
                match super::working::read_no_follow(&full, FILE_CAP) {
                    super::working::Bytes::Read(bytes) => Read::Bytes(bytes.into()),
                    super::working::Bytes::TooLarge(_) => Read::Skipped,
                    super::working::Bytes::NotThere => Read::Gone,
                }
            }
        };
        let mut found = scan(&items, &reader, &matcher, &turn, TextFrom::WorkingTree);
        found.commit = head;
        found
    })
    .await
    .ok()
}

// -- which place a search reads -------------------------------------------------

/// Whether the files index's copy is the tree the tab shows: the repository
/// itself (its default branch), or a branch of the name the index reads.
fn index_shows(at: &str, repo: &str, branch: Option<&str>, indexed: &str) -> bool {
    at == repo || (!indexed.is_empty() && branch == Some(indexed))
}

/// *Text search* in the tree the Files tab shows for `at` (a repository or
/// a branch, ADR-0062): its working tree when `working`, else the clone's
/// copy of `commit` (the tab's commit; the mirror's tip without one), else
/// the files index's copy of the default branch. `branch` is the branch's
/// name, which says whether the index's copy is that branch.
///
/// # Errors
///
/// What [`super::resolve`] and [`super::mirror_tip`] refuse with. A query
/// that does not parse, and a tree that cannot be searched, are answers.
#[allow(clippy::too_many_arguments)]
pub async fn search(
    pool: &sqlx::PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    index: &knobas_files::FileIndex,
    at: &str,
    commit: Option<&str>,
    working: bool,
    branch: Option<&str>,
    query: &TextQuery,
) -> Result<TextFound, crate::IpcError> {
    // The turn is taken before anything is awaited: the reads below finish in
    // any order, and a search asked before this one must never take a later
    // turn than it and stop it (the page draws only the newest question's
    // answer, and a stopped one draws nothing).
    let turn = Turn::take(at);
    let matcher = match Matcher::new(query) {
        Ok(matcher) => matcher,
        Err(why) => {
            return Ok(TextFound {
                invalid: Some(why),
                ..TextFound::empty(TextFrom::None)
            });
        }
    };
    let repo = super::repo_of(pool, at)
        .await?
        .entity_id
        .unwrap_or_else(|| at.to_owned());
    let clone = super::resolve(pool, &repo)
        .await?
        .path
        .map(std::path::PathBuf::from);
    if working {
        let Some(clone) = clone else {
            return Ok(TextFound::nowhere(
                "This repository is not cloned on this computer, so it has no working tree here.",
            ));
        };
        return Ok(scan_disk(&clone, &matcher, &turn)
            .await
            .unwrap_or_else(|| TextFound::nowhere("git could not list this working tree.")));
    }
    if let Some(clone) = &clone {
        let sha = match commit.filter(|sha| knobas_core::checkout::open::is_sha(sha)) {
            Some(sha) => Some(sha.to_owned()),
            None => super::mirror_tip(pool, registry, at)
                .await?
                .map(|tip| tip.commit),
        };
        if let Some(sha) = sha
            && let Some(found) = scan_tree(clone, &sha, &matcher, &turn).await
        {
            return Ok(found);
        }
    }
    let indexed = index
        .repos()
        .values()
        .find(|known| known.entity_id == repo)
        .map(|known| known.branch.clone());
    match indexed {
        Some(indexed) if index_shows(at, &repo, branch, &indexed) => {
            Ok(scan_index(index, &repo, &matcher, &turn)
                .await
                .unwrap_or_else(|| {
                    TextFound::nowhere("knobas holds no copy of this repository's files.")
                }))
        }
        Some(_) => Ok(TextFound::nowhere(if clone.is_some() {
            "Your clone does not hold this branch's commit yet, and knobas keeps the text of the default branch only."
        } else {
            "Clone this repository to search this branch: knobas keeps the text of the default branch only."
        })),
        None => Ok(TextFound::nowhere(if clone.is_some() {
            "Your clone does not hold this branch's commit yet."
        } else {
            "Clone this repository to search its text: knobas holds no copy of its files."
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A turn of a lane of its own: tests run side by side.
    fn turn() -> Turn {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        Turn::take(&format!(
            "test-lane-{}",
            NEXT.fetch_add(1, Ordering::SeqCst)
        ))
    }

    fn asked(text: &str, case_sensitive: bool, regex: bool) -> TextQuery {
        TextQuery {
            text: text.to_owned(),
            case_sensitive,
            regex,
            names: false,
        }
    }

    fn shown(hit: &TextHit) -> String {
        hit.parts
            .iter()
            .map(|part| {
                if part.hit {
                    format!("[{}]", part.text)
                } else {
                    part.text.clone()
                }
            })
            .collect()
    }

    fn lines(matcher: &Matcher, text: &str) -> Vec<(u32, String)> {
        matcher
            .lines(text.as_bytes(), 100)
            .0
            .iter()
            .map(|hit| (hit.line, shown(hit)))
            .collect()
    }

    const TEXT: &str = "fn Retry() {}\nlet retry = 1;\nRETRY again\n";

    #[test]
    fn lowercase_typed_matches_every_case_and_an_uppercase_letter_turns_case_on() {
        let any = Matcher::new(&asked("retry", false, false)).unwrap();
        assert_eq!(
            lines(&any, TEXT),
            vec![
                (1, "fn [Retry]() {}".to_owned()),
                (2, "let [retry] = 1;".to_owned()),
                (3, "[RETRY] again".to_owned()),
            ]
        );
        let smart = Matcher::new(&asked("Retry", false, false)).unwrap();
        assert_eq!(lines(&smart, TEXT), vec![(1, "fn [Retry]() {}".to_owned())]);
    }

    #[test]
    fn aa_matches_the_case_typed_only() {
        let exact = Matcher::new(&asked("retry", true, false)).unwrap();
        assert_eq!(
            lines(&exact, TEXT),
            vec![(2, "let [retry] = 1;".to_owned())]
        );
    }

    #[test]
    fn a_literal_is_taken_as_typed_and_a_regex_as_a_pattern() {
        let literal = Matcher::new(&asked("Retry()", false, false)).unwrap();
        assert_eq!(
            lines(&literal, TEXT),
            vec![(1, "fn [Retry()] {}".to_owned())]
        );
        let pattern = Matcher::new(&asked(r"r\w+y\b", false, true)).unwrap();
        assert_eq!(
            lines(&pattern, TEXT),
            vec![
                (1, "fn [Retry]() {}".to_owned()),
                (2, "let [retry] = 1;".to_owned()),
                (3, "[RETRY] again".to_owned()),
            ]
        );
        // Every hit of a line is marked, and a line is matched alone.
        let two = Matcher::new(&asked("e", true, false)).unwrap();
        assert_eq!(lines(&two, "see\n"), vec![(1, "s[e][e]".to_owned())]);
    }

    const ROOMS: &str =
        "let xy_room = 1;\nxyRoom()\nxy-room\nxy.room\nxy = room\nroom xy\nxy\nroom\n";

    #[test]
    fn words_typed_with_spaces_match_in_order_on_one_line_with_anything_between() {
        let words = Matcher::new(&asked("xy room", false, false)).unwrap();
        assert_eq!(
            lines(&words, ROOMS),
            vec![
                (1, "let [xy]_[room] = 1;".to_owned()),
                (2, "[xy][Room]()".to_owned()),
                (3, "[xy]-[room]".to_owned()),
                (4, "[xy].[room]".to_owned()),
                (5, "[xy] = [room]".to_owned()),
            ]
        );
        // The order typed: `room xy` is another query.
        let turned = Matcher::new(&asked("room xy", false, false)).unwrap();
        assert_eq!(lines(&turned, ROOMS), vec![(6, "[room] [xy]".to_owned())]);
        // One line: the words never meet across a line break, in a file's
        // first test either.
        assert!(!words.finds(b"xy\nroom\n"));
        assert!(words.finds(b"a\nxy_room\n"));
        // Spaces around and between the words are not part of them.
        let spaced = Matcher::new(&asked("  xy   room ", false, false)).unwrap();
        assert_eq!(lines(&spaced, ROOMS).len(), 5);
    }

    #[test]
    fn a_quoted_phrase_is_matched_as_typed_and_one_word_as_before() {
        let phrase = Matcher::new(&asked("\"xy room\"", false, false)).unwrap();
        assert_eq!(
            lines(&phrase, "xy room\nxy_room\nxy  room\n"),
            vec![(1, "[xy room]".to_owned())]
        );
        // A phrase among words: each is one term, in the order typed.
        let mixed = Matcher::new(&asked("let \"xy_room\" 1", false, false)).unwrap();
        assert_eq!(
            lines(&mixed, ROOMS),
            vec![(1, "[let] [xy_room] = [1];".to_owned())]
        );
        // One word, quotes and all, is the literal it was.
        let one = Matcher::new(&asked("\"name\":", false, false)).unwrap();
        assert_eq!(
            lines(&one, "{\"name\": 1}\nname: 2\n"),
            vec![(1, "{[\"name\":] 1}".to_owned())]
        );
        // Every hit of one word is still marked.
        let each = Matcher::new(&asked("xy", false, false)).unwrap();
        assert_eq!(lines(&each, "xy xy\n"), vec![(1, "[xy] [xy]".to_owned())]);
    }

    #[test]
    fn smart_case_reads_the_whole_query_and_aa_holds_every_word() {
        let any = Matcher::new(&asked("xy room", false, false)).unwrap();
        assert_eq!(lines(&any, "XY_ROOM\n").len(), 1);
        // One upper-case letter anywhere turns case on for every word.
        let smart = Matcher::new(&asked("xy Room", false, false)).unwrap();
        assert_eq!(
            lines(&smart, "xyRoom\nXYRoom\nxy_room\n"),
            vec![(1, "[xy][Room]".to_owned())]
        );
        let exact = Matcher::new(&asked("xy room", true, false)).unwrap();
        assert_eq!(lines(&exact, "xy_room\nxyRoom\n").len(), 1);
        // A pattern is a pattern: its space is a space.
        let pattern = Matcher::new(&asked("xy room", false, true)).unwrap();
        assert_eq!(
            lines(&pattern, "xy room\nxy_room\n"),
            vec![(1, "[xy room]".to_owned())]
        );
    }

    #[test]
    fn names_hold_the_words_in_order_too() {
        let words = Matcher::new(&TextQuery {
            names: true,
            ..asked("xy room", false, false)
        })
        .unwrap();
        assert!(words.names("src/xy_room.rs"));
        assert!(words.names("src/xy/Room.rs"));
        assert!(!words.names("src/room_xy.rs"));
    }

    #[test]
    fn a_pattern_that_does_not_parse_says_why() {
        let refused = Matcher::new(&asked("(open", false, true)).unwrap_err();
        assert!(refused.contains("unclosed"), "{refused}");
        assert!(Matcher::new(&asked("   ", false, false)).is_err());
    }
    // -- the scans, against real repositories ----------------------------------

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

    fn write(dir: &Path, rel: &str, bytes: &[u8]) {
        let full = dir.join(rel);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, bytes).unwrap();
    }

    /// `main` and `feature`, each with its own `retry`; a binary file holding
    /// the word; the checkout left on `main`.
    fn branches() -> (tempfile::TempDir, String, String) {
        let dir = tempfile::tempdir().unwrap();
        let at = dir.path();
        git(at, &["init", "-q", "-b", "main"]);
        write(at, "src/pay.rs", b"fn pay() {\n    retry_on_main();\n}\n");
        write(at, "README.md", b"# Pay\n");
        write(at, "logo.bin", b"retry\0\x01\x02");
        git(at, &["add", "."]);
        git(at, &["commit", "-q", "-m", "main"]);
        let main = git(at, &["rev-parse", "HEAD"]);
        git(at, &["checkout", "-q", "-b", "feature"]);
        write(
            at,
            "src/pay.rs",
            b"fn pay() {\n    retry_on_feature();\n}\n",
        );
        write(at, "src/new.rs", b"// retry later\n");
        git(at, &["add", "."]);
        git(at, &["commit", "-q", "-m", "feature"]);
        let feature = git(at, &["rev-parse", "HEAD"]);
        git(at, &["checkout", "-q", "main"]);
        (dir, main, feature)
    }

    fn found_lines(found: &TextFound) -> Vec<String> {
        found
            .files
            .iter()
            .flat_map(|file| {
                file.hits
                    .iter()
                    .map(move |hit| format!("{}:{}:{}", file.path, hit.line, shown(hit)))
            })
            .collect()
    }

    #[tokio::test]
    async fn a_branch_is_searched_at_its_own_commit_and_the_checkout_is_not_touched() {
        let (dir, main, feature) = branches();
        let at = dir.path();
        let matcher = Matcher::new(&asked("retry", false, false)).unwrap();
        let on_feature = scan_tree(at, &feature, &matcher, &turn())
            .await
            .expect("the clone holds it");
        assert_eq!(on_feature.from, TextFrom::Clone);
        assert_eq!(on_feature.commit.as_deref(), Some(feature.as_str()));
        assert_eq!(
            found_lines(&on_feature),
            vec![
                "src/new.rs:1:// [retry] later".to_owned(),
                "src/pay.rs:2:    [retry]_on_feature();".to_owned(),
            ]
        );
        // The binary file holds the word too: skipped and counted.
        assert_eq!(on_feature.skipped, 1);
        assert_eq!(on_feature.searched, 3);
        let on_main = scan_tree(at, &main, &matcher, &turn()).await.unwrap();
        assert_eq!(
            found_lines(&on_main),
            vec!["src/pay.rs:2:    [retry]_on_main();".to_owned()]
        );
        // Nothing was checked out: the checkout is still on main, clean.
        assert_eq!(git(at, &["rev-parse", "--abbrev-ref", "HEAD"]), "main");
        assert_eq!(git(at, &["status", "--porcelain"]), "");
        // A commit the clone does not hold is no answer.
        let missing = "0123456789abcdef0123456789abcdef01234567";
        assert!(scan_tree(at, missing, &matcher, &turn()).await.is_none());
    }

    #[tokio::test]
    async fn the_scan_stops_at_its_cap_and_answers_the_first_lines_in_path_order() {
        let dir = tempfile::tempdir().unwrap();
        let at = dir.path();
        git(at, &["init", "-q", "-b", "main"]);
        // 40 files of 30 matching lines: 1,200 lines, more than the cap.
        for n in 0..40 {
            write(at, &format!("f{n:02}.txt"), "hit\n".repeat(30).as_bytes());
        }
        git(at, &["add", "."]);
        git(at, &["commit", "-q", "-m", "many"]);
        let sha = git(at, &["rev-parse", "HEAD"]);
        let matcher = Matcher::new(&asked("hit", false, false)).unwrap();
        let first = scan_tree(at, &sha, &matcher, &turn()).await.unwrap();
        assert!(first.cut);
        let shown: usize = first.files.iter().map(|file| file.hits.len()).sum();
        assert_eq!(shown, MOST_HITS);
        // Each file shows its first lines and counts the rest.
        assert_eq!(first.files[0].path, "f00.txt");
        assert_eq!(first.files[0].hits.len(), MOST_PER_FILE);
        assert_eq!(first.files[0].more, 10);
        // The same answer each time, whichever thread read what.
        for _ in 0..5 {
            let again = scan_tree(at, &sha, &matcher, &turn()).await.unwrap();
            assert_eq!(found_lines(&again), found_lines(&first));
        }
    }
    #[tokio::test]
    async fn the_working_tree_is_read_from_disk_and_what_git_ignores_is_left_out() {
        let (dir, _, _) = branches();
        let at = dir.path();
        write(at, ".gitignore", b"target/\n*.log\n");
        git(at, &["add", ".gitignore"]);
        git(at, &["commit", "-q", "-m", "ignore"]);
        // Edited, not committed; a new file; ignored files that hold the word.
        write(at, "src/pay.rs", b"fn pay() {\n    retry_on_disk();\n}\n");
        write(at, "src/added.rs", b"retry added\n");
        write(at, "target/debug/out.rs", b"retry ignored\n");
        write(at, "build.log", b"retry ignored\n");
        // A tracked file deleted on disk is no line.
        std::fs::remove_file(at.join("README.md")).unwrap();
        let matcher = Matcher::new(&asked("retry", false, false)).unwrap();
        let found = scan_disk(at, &matcher, &turn())
            .await
            .expect("a working tree");
        assert_eq!(found.from, TextFrom::WorkingTree);
        assert_eq!(
            found.commit.as_deref(),
            Some(git(at, &["rev-parse", "HEAD"]).as_str())
        );
        assert_eq!(
            found_lines(&found),
            vec![
                "src/added.rs:1:[retry] added".to_owned(),
                "src/pay.rs:2:    [retry]_on_disk();".to_owned(),
            ]
        );
        assert_eq!(found.skipped, 1, "logo.bin");
    }

    #[tokio::test]
    async fn words_find_their_separated_forms_in_a_branch_and_on_the_working_tree() {
        let (dir, _, feature) = branches();
        let at = dir.path();
        // `feature`'s pay.rs says `retry_on_feature`; the disk says it
        // camel-cased, in a file git does not track yet.
        write(at, "src/disk.rs", b"retryOnDisk();\non retry\n");
        let words = Matcher::new(&asked("retry on", false, false)).unwrap();
        let branch = scan_tree(at, &feature, &words, &turn()).await.unwrap();
        assert_eq!(
            found_lines(&branch),
            vec!["src/pay.rs:2:    [retry]_[on]_feature();".to_owned()]
        );
        let disk = scan_disk(at, &words, &turn()).await.unwrap();
        assert_eq!(
            found_lines(&disk),
            vec![
                "src/disk.rs:1:[retry][On]Disk();".to_owned(),
                "src/pay.rs:2:    [retry]_[on]_main();".to_owned(),
            ]
        );
    }

    #[tokio::test]
    async fn names_are_answered_beside_lines_when_asked() {
        let (dir, _, feature) = branches();
        let mut query = asked("PAY", false, false);
        query.names = true;
        // Smart case: an upper-case query matches its case, in names too.
        let exact = scan_tree(
            dir.path(),
            &feature,
            &Matcher::new(&query).unwrap(),
            &turn(),
        )
        .await
        .unwrap();
        assert!(exact.names.is_empty());
        query.text = "pay".to_owned();
        let any = scan_tree(
            dir.path(),
            &feature,
            &Matcher::new(&query).unwrap(),
            &turn(),
        )
        .await
        .unwrap();
        assert_eq!(any.names, vec!["src/pay.rs".to_owned()]);
        assert_eq!(
            found_lines(&any),
            vec![
                "README.md:1:# [Pay]".to_owned(),
                "src/pay.rs:1:fn [pay]() {".to_owned()
            ]
        );
    }
    fn index_of(
        files: &[(u64, &str, Option<&str>)],
    ) -> (tempfile::TempDir, knobas_files::FileIndex) {
        let dir = tempfile::tempdir().unwrap();
        let index = knobas_files::FileIndex::open(&dir.path().join("files-1")).unwrap();
        index.set_repos(
            [(
                7,
                knobas_files::Repo {
                    entity_id: "gitea:acme/pay".to_owned(),
                    source_id: "gitea".to_owned(),
                    favourite: false,
                    updated: 0,
                    branch: "main".to_owned(),
                },
            )]
            .into_iter()
            .collect(),
        );
        let mut writer = index.writer().unwrap();
        for (no, path, text) in files {
            writer.put(*no, path, *text).unwrap();
        }
        writer.commit().unwrap();
        (dir, index)
    }

    #[tokio::test]
    async fn with_no_clone_the_index_text_is_scanned_and_says_its_branch() {
        let (_dir, index) = index_of(&[
            (7, "src/z.rs", Some("let ok = retry();\n")),
            (
                7,
                "src/a.rs",
                Some("// nothing\n// Retry here, mid-word: preretry\n"),
            ),
            (7, "logo.png", None),
            (8, "src/a.rs", Some("retry elsewhere\n")),
        ]);
        let matcher = Matcher::new(&asked("retry", false, false)).unwrap();
        let found = scan_index(&index, "gitea:acme/pay", &matcher, &turn())
            .await
            .expect("indexed");
        assert_eq!(found.from, TextFrom::Index);
        assert_eq!(found.branch.as_deref(), Some("main"));
        // Lines as a literal finds them, inside a word too, in path order.
        assert_eq!(
            found_lines(&found),
            vec![
                "src/a.rs:2:// [Retry] here, mid-word: pre[retry]".to_owned(),
                "src/z.rs:1:let ok = [retry]();".to_owned(),
            ]
        );
        assert_eq!(found.searched, 2);
        // A repository the index does not know is no answer.
        assert!(
            scan_index(&index, "gitea:acme/other", &matcher, &turn())
                .await
                .is_none()
        );
        // A pattern scans the same text.
        let pattern = Matcher::new(&asked(r"^let \w+", false, true)).unwrap();
        let by_pattern = scan_index(&index, "gitea:acme/pay", &pattern, &turn())
            .await
            .unwrap();
        assert_eq!(
            found_lines(&by_pattern),
            vec!["src/z.rs:1:[let ok] = retry();".to_owned()]
        );
    }
    #[test]
    fn a_newer_turn_passes_the_older_one_in_its_lane_only() {
        let first = Turn::take("lane-test-a");
        let other = Turn::take("lane-test-b");
        assert!(!first.passed());
        let second = Turn::take("lane-test-a");
        assert!(first.passed());
        assert!(!second.passed());
        assert!(!other.passed());
    }

    #[tokio::test]
    async fn a_scan_whose_turn_was_overtaken_answers_stale_and_the_newer_one_answers() {
        let (dir, _, feature) = branches();
        let matcher = Matcher::new(&asked("retry", false, false)).unwrap();
        // Asked in this order; the newer one's read starts first, as reads may.
        let older = Turn::take("lane-test-overtaken");
        let newer = Turn::take("lane-test-overtaken");
        let answered = scan_tree(dir.path(), &feature, &matcher, &newer)
            .await
            .unwrap();
        assert!(!answered.stale);
        assert_eq!(found_lines(&answered).len(), 2);
        let late = scan_tree(dir.path(), &feature, &matcher, &older)
            .await
            .unwrap();
        assert!(late.stale);
        assert!(late.files.is_empty());
        // The older scan reading last did not stop the newer one's turn.
        assert!(!newer.passed());
    }
    #[test]
    fn the_index_stands_for_the_repository_and_for_the_branch_it_reads_only() {
        assert!(index_shows(
            "gitea:acme/pay",
            "gitea:acme/pay",
            None,
            "main"
        ));
        assert!(index_shows(
            "gitea:acme/pay:main",
            "gitea:acme/pay",
            Some("main"),
            "main"
        ));
        assert!(!index_shows(
            "gitea:acme/pay:feature",
            "gitea:acme/pay",
            Some("feature"),
            "main"
        ));
        // An index that does not say its branch stands for the repository alone.
        assert!(!index_shows(
            "gitea:acme/pay:main",
            "gitea:acme/pay",
            Some("main"),
            ""
        ));
    }
    /// Timings on a real clone, for the PR's measurement: `KNOBAS_TEXT_BENCH=<clone>`
    /// `cargo test -p knobas-app --lib text_search_timings -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "a measurement on a clone named by KNOBAS_TEXT_BENCH"]
    async fn text_search_timings() {
        let Some(clone) = std::env::var_os("KNOBAS_TEXT_BENCH") else {
            return;
        };
        let at = Path::new(&clone);
        let head = git(at, &["rev-parse", "HEAD"]);
        let cases = [
            ("rare literal", asked("zqxjkv_none", false, false)),
            ("common literal", asked("return", false, false)),
            ("rare words", asked("zqxjkv none", false, false)),
            ("common words", asked("return self", false, false)),
            ("smart-case literal", asked("Config", false, false)),
            ("pattern", asked(r"fn\s+\w+_new\b|\bstd::\w+", false, true)),
        ];
        // The tree read once, as the tab has read it before anyone types.
        let _ = super::super::local::tree(at, &head).await;
        for (name, query) in &cases {
            let matcher = Matcher::new(query).unwrap();
            for run in 0..3 {
                let started = std::time::Instant::now();
                let found = scan_tree(at, &head, &matcher, &turn()).await.unwrap();
                println!(
                    "tree {name} run {run}: {:?} searched {} skipped {} files {} cut {}",
                    started.elapsed(),
                    found.searched,
                    found.skipped,
                    found.files.len(),
                    found.cut
                );
            }
            for run in 0..2 {
                let started = std::time::Instant::now();
                let found = scan_disk(at, &matcher, &turn()).await.unwrap();
                println!(
                    "disk {name} run {run}: {:?} searched {} files {} cut {}",
                    started.elapsed(),
                    found.searched,
                    found.files.len(),
                    found.cut
                );
            }
        }
    }
}
