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
//! Each scan stops at [`MOST_HITS`] lines; a file over [`FILE_CAP`] bytes
//! and a file with a NUL in its first 8,000 bytes (git's binary sniff) are
//! counted and skipped. A newer search stops the one before it
//! ([`Turn`]), so typing never piles scans up behind each other.

use std::path::Path;
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
    /// What is typed: a literal, or a pattern when `regex` is on.
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
    /// The text as typed, folded when the case is not matched: for names.
    name: Option<(String, bool)>,
    /// The text is a pattern: names are matched by it too.
    pattern: bool,
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
        let pattern = if query.regex {
            query.text.clone()
        } else {
            regex::escape(&query.text)
        };
        let insensitive = !query.case_sensitive && !typed_upper(&query.text, query.regex);
        let regex = RegexBuilder::new(&pattern)
            .case_insensitive(insensitive)
            .size_limit(8 * 1024 * 1024)
            .build()
            .map_err(|error| refusal(&error))?;
        let name = query.names.then(|| {
            if insensitive {
                (query.text.to_lowercase(), true)
            } else {
                (query.text.clone(), false)
            }
        });
        Ok(Self {
            regex,
            name,
            pattern: query.regex,
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
        let Some((text, folded)) = &self.name else {
            return false;
        };
        if self.pattern {
            return self.regex.is_match(path.as_bytes());
        }
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
            let found: Vec<(usize, usize)> = self
                .regex
                .find_iter(line)
                .filter(|m| m.end() > m.start())
                .map(|m| (m.start(), m.end()))
                .collect();
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
    let last = words.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("");
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

/// The searches under way: each takes the next turn, and a scan whose turn
/// is no longer the last stops, so the keys typed before the last one cost
/// nothing more once it starts.
static TURN: AtomicU64 = AtomicU64::new(0);

/// One search's turn ([`TURN`]).
#[derive(Debug, Clone, Copy)]
pub struct Turn(u64);

impl Turn {
    /// The next turn: every scan before it stops.
    #[must_use]
    pub fn take() -> Self {
        Self(TURN.fetch_add(1, Ordering::SeqCst) + 1)
    }

    fn passed(self) -> bool {
        TURN.load(Ordering::SeqCst) != self.0
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
    turn: Turn,
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
                        if shown.fetch_add(hits.len(), Ordering::Relaxed) + hits.len()
                            >= MOST_HITS
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
    let mut left = MOST_HITS;
    for (_, mut file) in found {
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

/// `query` over the tree at `sha` in the clone at `path`, its blobs read in
/// knobas's own process (git where that cannot answer one, as every read of
/// a clone): `None` where the clone does not hold that commit.
pub async fn scan_tree(path: &Path, sha: &str, matcher: &Matcher) -> Option<TextFound> {
    let tree = super::local::tree(path, sha).await?;
    let turn = Turn::take();
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
        let mut found = scan(&items, &reader, &matcher, turn, TextFrom::Clone);
        found.commit = Some(tree.commit.clone());
        found
    })
    .await
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(lines(&exact, TEXT), vec![(2, "let [retry] = 1;".to_owned())]);
    }

    #[test]
    fn a_literal_is_taken_as_typed_and_a_regex_as_a_pattern() {
        let literal = Matcher::new(&asked("Retry()", false, false)).unwrap();
        assert_eq!(lines(&literal, TEXT), vec![(1, "fn [Retry()] {}".to_owned())]);
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
        assert_eq!(
            lines(&two, "see\n"),
            vec![(1, "s[e][e]".to_owned())]
        );
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
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
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
        write(at, "src/pay.rs", b"fn pay() {\n    retry_on_feature();\n}\n");
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
        let on_feature = scan_tree(at, &feature, &matcher).await.expect("the clone holds it");
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
        let on_main = scan_tree(at, &main, &matcher).await.unwrap();
        assert_eq!(found_lines(&on_main), vec!["src/pay.rs:2:    [retry]_on_main();".to_owned()]);
        // Nothing was checked out: the checkout is still on main, clean.
        assert_eq!(git(at, &["rev-parse", "--abbrev-ref", "HEAD"]), "main");
        assert_eq!(git(at, &["status", "--porcelain"]), "");
        // A commit the clone does not hold is no answer.
        let missing = "0123456789abcdef0123456789abcdef01234567";
        assert!(scan_tree(at, missing, &matcher).await.is_none());
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
        let first = scan_tree(at, &sha, &matcher).await.unwrap();
        assert!(first.cut);
        let shown: usize = first.files.iter().map(|file| file.hits.len()).sum();
        assert_eq!(shown, MOST_HITS);
        // Each file shows its first lines and counts the rest.
        assert_eq!(first.files[0].path, "f00.txt");
        assert_eq!(first.files[0].hits.len(), MOST_PER_FILE);
        assert_eq!(first.files[0].more, 10);
        // The same answer each time, whichever thread read what.
        for _ in 0..5 {
            let again = scan_tree(at, &sha, &matcher).await.unwrap();
            assert_eq!(found_lines(&again), found_lines(&first));
        }
    }
}
