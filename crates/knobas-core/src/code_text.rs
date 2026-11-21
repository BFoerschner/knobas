//! **A repository's files as *Everything* reads them** (the everything-search
//! decisions, 2026-11-20, D4, D5, D8 and D10): which files are read, the words
//! of a file's text, of its path and of what was typed, which line of a file
//! is shown, and which of its letters are marked.
//!
//! Pure. The one place these rules live: the files pass (`knobas_sync::code_files`)
//! writes a file's words with [`tokens`], the index (`knobas-files`) asks for
//! what was typed with [`query`], and the launcher's file line is
//! [`best_line`] marked by [`marks`]. The tokenizer is the contract between
//! what is written and what is asked: change one side and the other reads it.
//!
//! # The words
//!
//! A **run** is letters, digits, `_` and `-` side by side; any other character
//! ends it. A **part** of a run starts at the run's start, after `_` or `-`,
//! at a capital after a small letter or a digit, at the last capital of a run
//! of capitals before a small letter, and at a letter after a digit. A
//! **token** is the run from a part's start to its end, lower-cased, without
//! `_` and `-`: `ADProxyServer` gives `adproxyserver`, `proxyserver` and
//! `server`. So a typed word, asked as a token's start, finds an identifier
//! from the start of any of its parts: `adproxy` and `proxy` find
//! `ADProxyServer`, and `dprox` does not.

use std::borrow::Cow;
use std::collections::BTreeSet;

/// The largest file whose text is read, known from the tree before any byte
/// is: 512 KiB (D4). Larger files are found by name only.
pub const SIZE_CAP: u64 = 512 * 1024;

/// A line longer than this marks a minified or data file (D4).
pub const LONGEST_LINE: usize = 2_000;

/// The longest token kept, in bytes, cut at a character's start (D5 rule 4).
pub const TOKEN_MAX: usize = 48;

/// A run of more than this many parts gives its first token only: a hash or
/// an encoded blob (D5 rule 5).
pub const PARTS_MAX: usize = 8;

/// The shortest term asked of the index, in characters (D8 rule 1).
pub const TERM_MIN: usize = 3;

/// A term of exactly this many characters is read whole, never as a word's
/// start (D8 rule 2): `con` would begin the words of most files.
pub const TERM_WHOLE: usize = 3;

/// Why a file's text is not read: the file is still found by its name. A
/// closed set, each with the snake_case word Settings counts it by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LeftOut {
    /// Not a regular file: a link or a submodule.
    NotRegular,
    /// Under a folder nobody wrote (`node_modules`, `vendor`, `target`, …).
    Folder,
    /// A lock file.
    LockFile,
    /// A made file, by its name (`.min.js`, `.map`, `.svg`, …).
    MadeFile,
    /// Over [`SIZE_CAP`].
    TooLarge,
    /// No bytes.
    Empty,
    /// A NUL byte anywhere.
    Binary,
    /// Neither UTF-8 nor Windows-1252 without a control character.
    NotText,
    /// A line over [`LONGEST_LINE`] bytes: minified or data.
    LongLine,
    /// Its head says it was generated.
    Generated,
    /// The root `.gitattributes` marks it `linguist-generated` or
    /// `linguist-vendored`.
    Marked,
    /// Past the repository's share: its first 20,000 text files and 256 MiB.
    Share,
    /// Past the bound on all the text kept (Settings › Search).
    Bound,
}

impl LeftOut {
    /// The word Settings counts it by, as `sync.code_repo.left_out` keys it.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::NotRegular => "not_regular",
            Self::Folder => "folder",
            Self::LockFile => "lock_file",
            Self::MadeFile => "made_file",
            Self::TooLarge => "too_large",
            Self::Empty => "empty",
            Self::Binary => "binary",
            Self::NotText => "not_text",
            Self::LongLine => "long_line",
            Self::Generated => "generated",
            Self::Marked => "marked",
            Self::Share => "share",
            Self::Bound => "bound",
        }
    }
}

/// Folders nobody wrote: a path with one of these as a folder is left out.
pub const SKIP_FOLDERS: &[&str] = &[
    ".git",
    "node_modules",
    "vendor",
    "third_party",
    "third-party",
    "target",
    "build",
    "dist",
    "out",
    ".gradle",
    ".idea",
    ".vscode",
    "__pycache__",
    ".venv",
    "venv",
    "bower_components",
    "Pods",
    ".next",
    ".nuxt",
    "coverage",
    ".terraform",
    ".mvn",
];

/// Lock files by name; any name ending `.lock` is one too.
pub const LOCK_FILES: &[&str] = &[
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "Cargo.lock",
    "Gemfile.lock",
    "poetry.lock",
    "composer.lock",
    "go.sum",
    "Pipfile.lock",
    "gradle.lockfile",
    "packages.lock.json",
    "flake.lock",
    "uv.lock",
    "bun.lock",
];

/// Made files by their name's end, any case.
pub const MADE_SUFFIXES: &[&str] = &[
    ".min.js",
    ".min.css",
    ".map",
    ".svg",
    ".snap",
    ".pb.go",
    ".g.dart",
    ".designer.cs",
    ".generated.cs",
    "_pb2.py",
    ".pbxproj",
    ".ipynb",
    ".csv",
    ".tsv",
];

/// What a generated file's head says, any case, in its first 600 bytes.
const GENERATED_MARKS: &[&str] = &[
    "@generated",
    "do not edit",
    "automatically generated",
    "auto-generated",
];

/// Why the file at `path` (a tree path, `/` between its parts) is not read,
/// by its name alone; `None` to read it.
#[must_use]
pub fn refuses_path(path: &str) -> Option<LeftOut> {
    let mut parts = path.rsplit('/');
    let name = parts.next().unwrap_or(path);
    if parts.any(|folder| SKIP_FOLDERS.contains(&folder)) {
        return Some(LeftOut::Folder);
    }
    if LOCK_FILES.contains(&name) || name.ends_with(".lock") {
        return Some(LeftOut::LockFile);
    }
    let lower = name.to_ascii_lowercase();
    if MADE_SUFFIXES.iter().any(|suffix| lower.ends_with(suffix)) {
        return Some(LeftOut::MadeFile);
    }
    None
}

/// Why a file of `size` bytes is not read, known from the tree; `None` to
/// read it.
#[must_use]
pub fn refuses_size(size: u64) -> Option<LeftOut> {
    if size == 0 {
        Some(LeftOut::Empty)
    } else if size > SIZE_CAP {
        Some(LeftOut::TooLarge)
    } else {
        None
    }
}

/// Why a file's `bytes` are not read; `None` to read them. The NUL check reads
/// the whole file: a binary with a clean head is still a binary.
#[must_use]
pub fn refuses_bytes(bytes: &[u8]) -> Option<LeftOut> {
    if let Some(left_out) = refuses_size(u64::try_from(bytes.len()).unwrap_or(u64::MAX)) {
        return Some(left_out);
    }
    if bytes.contains(&0) {
        return Some(LeftOut::Binary);
    }
    if decode(bytes).is_none() {
        return Some(LeftOut::NotText);
    }
    if bytes
        .split(|byte| *byte == b'\n')
        .any(|line| line.len() > LONGEST_LINE)
    {
        return Some(LeftOut::LongLine);
    }
    let head = &bytes[..bytes.len().min(600)];
    let head = String::from_utf8_lossy(head).to_lowercase();
    if GENERATED_MARKS.iter().any(|mark| head.contains(mark)) {
        return Some(LeftOut::Generated);
    }
    None
}

/// A file's text: UTF-8 as it is, else Windows-1252 where that reads with no
/// control character but tab, line feed and carriage return (a German shop's
/// older Java is often Latin-1); `None` for neither.
#[must_use]
pub fn decode(bytes: &[u8]) -> Option<Cow<'_, str>> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Some(Cow::Borrowed(text));
    }
    let (text, _, _) = encoding_rs::WINDOWS_1252.decode(bytes);
    let clean = !text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\t' | '\n' | '\r'));
    clean.then(|| Cow::Owned(text.into_owned()))
}

// -- `.gitattributes` --------------------------------------------------------

/// The root `.gitattributes` as it marks a path generated or vendored (D4):
/// three pattern shapes, a path (`src/gen.rs`), a folder (`gen/`, `gen/**`)
/// and a name glob with one `*` (`*.pb.go`). A pattern with no `/` but a
/// trailing one matches at any depth, as git reads it; one with a `/` inside
/// is from the root. A later line overrides an earlier one. Anything else in
/// the file is ignored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Attributes {
    rules: Vec<(Pattern, Option<bool>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Pattern {
    /// A path from the root.
    Path(String),
    /// A folder: from the root when `anchored`, else any folder of the name.
    Folder { name: String, anchored: bool },
    /// A file's name, `*` once in it.
    Glob { before: String, after: String },
}

impl Pattern {
    fn read(pattern: &str) -> Option<Self> {
        let anchored_lead = pattern.starts_with('/');
        let pattern = pattern.trim_start_matches('/');
        if pattern.is_empty() || pattern.contains(['?', '[', '!']) {
            return None;
        }
        if let Some(folder) = pattern
            .strip_suffix("/**")
            .or_else(|| pattern.strip_suffix('/'))
        {
            if folder.is_empty() || folder.contains('*') {
                return None;
            }
            return Some(Self::Folder {
                anchored: anchored_lead || folder.contains('/'),
                name: folder.to_owned(),
            });
        }
        match pattern.matches('*').count() {
            0 => Some(Self::Path(pattern.to_owned())),
            1 if !pattern.contains('/') => {
                let (before, after) = pattern.split_once('*')?;
                Some(Self::Glob {
                    before: before.to_owned(),
                    after: after.to_owned(),
                })
            }
            _ => None,
        }
    }

    fn matches(&self, path: &str) -> bool {
        match self {
            Self::Path(whole) => path == whole,
            Self::Folder {
                name,
                anchored: true,
            } => path
                .strip_prefix(name.as_str())
                .is_some_and(|rest| rest.starts_with('/')),
            Self::Folder {
                name,
                anchored: false,
            } => {
                let mut folders = path.split('/');
                folders.next_back();
                folders.any(|folder| folder == name)
            }
            Self::Glob { before, after } => {
                let file = path.rsplit('/').next().unwrap_or(path);
                file.len() >= before.len() + after.len()
                    && file.starts_with(before.as_str())
                    && file.ends_with(after.as_str())
            }
        }
    }
}

impl Attributes {
    /// Read the root `.gitattributes`' text.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut rules = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut words = line.split_whitespace();
            let Some(pattern) = words.next().and_then(Pattern::read) else {
                continue;
            };
            let mut said: Option<bool> = None;
            for word in words {
                for attribute in ["linguist-generated", "linguist-vendored"] {
                    let set = if word == attribute || word == format!("{attribute}=true") {
                        Some(true)
                    } else if word == format!("-{attribute}")
                        || word == format!("!{attribute}")
                        || word == format!("{attribute}=false")
                    {
                        Some(false)
                    } else {
                        None
                    };
                    if let Some(set) = set {
                        said = Some(said.unwrap_or(false) || set);
                    }
                }
            }
            if said.is_some() {
                rules.push((pattern, said));
            }
        }
        Self { rules }
    }

    /// Whether the file is marked generated or vendored.
    #[must_use]
    pub fn marks(&self, path: &str) -> bool {
        self.rules
            .iter()
            .rev()
            .find(|(pattern, _)| pattern.matches(path))
            .and_then(|(_, said)| *said)
            .unwrap_or(false)
    }
}

// -- the words ----------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Lower,
    Upper,
    Digit,
    /// `_` and `-`: inside a run, never in a token.
    Soft,
    /// Ends a run.
    Hard,
}

fn class(c: char) -> Class {
    match c {
        'a'..='z' => Class::Lower,
        'A'..='Z' => Class::Upper,
        '0'..='9' => Class::Digit,
        '_' | '-' => Class::Soft,
        _ if c.is_ascii() => Class::Hard,
        _ if c.is_alphabetic() && c.is_uppercase() => Class::Upper,
        _ if c.is_alphabetic() => Class::Lower,
        _ if c.is_numeric() => Class::Digit,
        _ => Class::Hard,
    }
}

/// One run of `text` read into its parts: `joined` is the run lower-cased
/// without `_` and `-`, `parts` where each part starts in `joined` (bytes)
/// and in the text (bytes), and `spans` each kept character's place, in
/// `joined` and in the text.
struct Run {
    joined: String,
    parts: Vec<(usize, usize)>,
    /// For each character of `joined`: (its byte in `joined`, its byte in the
    /// text, its byte length in the text).
    spans: Vec<(usize, usize, usize)>,
}

/// Each run of `text`, read into its parts (D5 rules 1 and 2).
fn runs(text: &str, mut each: impl FnMut(&Run)) {
    let mut chars: Vec<(usize, char, Class)> = Vec::with_capacity(64);
    let mut flush = |chars: &mut Vec<(usize, char, Class)>| {
        let body: Vec<(usize, char, Class)> = chars
            .iter()
            .copied()
            .skip_while(|(_, _, c)| *c == Class::Soft)
            .collect();
        chars.clear();
        let mut run = Run {
            joined: String::with_capacity(body.len()),
            parts: Vec::new(),
            spans: Vec::with_capacity(body.len()),
        };
        let mut previous: Option<Class> = None;
        for (index, &(at, ch, cl)) in body.iter().enumerate() {
            if cl == Class::Soft {
                previous = Some(Class::Soft);
                continue;
            }
            let next = body.get(index + 1).map(|(_, _, c)| *c);
            let starts = match (previous, cl) {
                (None | Some(Class::Soft), _)
                | (Some(Class::Lower | Class::Digit), Class::Upper)
                | (Some(Class::Digit), Class::Lower) => true,
                (Some(Class::Upper), Class::Upper) => next == Some(Class::Lower),
                _ => false,
            };
            if starts {
                run.parts.push((run.joined.len(), at));
            }
            for lower in ch.to_lowercase() {
                run.spans.push((run.joined.len(), at, ch.len_utf8()));
                run.joined.push(lower);
            }
            previous = Some(cl);
        }
        if !run.parts.is_empty() {
            each(&run);
        }
    };
    for (at, ch) in text.char_indices() {
        let cl = class(ch);
        if cl == Class::Hard {
            if !chars.is_empty() {
                flush(&mut chars);
            }
        } else {
            chars.push((at, ch, cl));
        }
    }
    if !chars.is_empty() {
        flush(&mut chars);
    }
}

/// `text` cut to [`TOKEN_MAX`] bytes at a character's start.
fn capped(text: &str) -> &str {
    let mut end = text.len().min(TOKEN_MAX);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// The tokens of `text`, in order, each with the byte its part starts at in
/// `text` (D5 rules 1 to 5). A token under 2 characters is dropped; a run of
/// more than [`PARTS_MAX`] parts gives its first token only.
pub fn tokens(text: &str, mut emit: impl FnMut(&str, usize)) {
    runs(text, |run| {
        let parts = if run.parts.len() > PARTS_MAX {
            &run.parts[..1]
        } else {
            &run.parts[..]
        };
        for &(from, at) in parts {
            let tail = &run.joined[from..];
            if tail.chars().nth(1).is_some() {
                emit(capped(tail), at);
            }
        }
    });
}

/// The distinct tokens of `text`, sorted: what a file's text or path is
/// indexed by, a set with no positions or counts.
#[must_use]
pub fn distinct_tokens(text: &str) -> Vec<String> {
    let mut set: BTreeSet<String> = BTreeSet::new();
    tokens(text, |token, _| {
        if !set.contains(token) {
            set.insert(token.to_owned());
        }
    });
    set.into_iter().collect()
}

/// What was typed as the terms its runs give, each run's whole join (D5 rule
/// 6): `AD-Proxy server` asks `adproxy` and `server`. Terms under 2
/// characters are dropped, and each is cut as a token is.
#[must_use]
pub fn query_terms(typed: &str) -> Vec<String> {
    let mut out = Vec::new();
    runs(typed, |run| {
        if run.joined.chars().nth(1).is_some() {
            out.push(capped(&run.joined).to_owned());
        }
    });
    out
}

/// What a typed text asks of the files (D8 rule 1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    /// Every term every matching file must hold, each of [`TERM_MIN`]
    /// characters and more, in the order typed and once each.
    pub terms: Vec<String>,
    /// The quoted phrases, each as its terms: a shown line must hold them side
    /// by side (D8 rule 11).
    pub phrases: Vec<Vec<String>>,
}

impl Query {
    /// Whether nothing is asked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }
}

/// What `typed` asks of the files: the grammar's keys and markers are out of
/// it already. A `-word` or `!word` is dropped, not negated; a text with the
/// word `or` asks nothing, as the picker does; terms under [`TERM_MIN`]
/// characters are dropped.
#[must_use]
pub fn query(typed: &str) -> Query {
    let mut asked = Query::default();
    let add = |term: String, asked: &mut Query| {
        if term.chars().count() >= TERM_MIN && !asked.terms.contains(&term) {
            asked.terms.push(term);
        }
    };
    for (index, piece) in typed.split('"').enumerate() {
        // The odd pieces are inside quotes.
        if index % 2 == 1 {
            let phrase: Vec<String> = query_terms(piece);
            for term in &phrase {
                add(term.clone(), &mut asked);
            }
            if phrase.len() > 1 {
                asked.phrases.push(phrase);
            }
            continue;
        }
        for word in piece.split_whitespace() {
            if word.eq_ignore_ascii_case("or") {
                return Query::default();
            }
            if word.starts_with(['-', '!']) {
                continue;
            }
            for term in query_terms(word) {
                add(term, &mut asked);
            }
        }
    }
    asked
}

/// Whether the token `token` answers `term`: whole for a term of
/// [`TERM_WHOLE`] characters, as a word's start for a longer one.
#[must_use]
pub fn answers(token: &str, term: &str) -> bool {
    if term.chars().count() <= TERM_WHOLE {
        token == term
    } else {
        token.starts_with(term)
    }
}

// -- the line shown -----------------------------------------------------------

/// One line of a file, with the lines around it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// Its number, from 1.
    pub number: u32,
    /// The line before, or empty.
    pub before: String,
    /// The line.
    pub text: String,
    /// The line after, or empty.
    pub after: String,
}

/// Words a line that defines something holds.
const DEFINES: &[&str] = &[
    "class ",
    "interface ",
    "enum ",
    "struct ",
    "trait ",
    "record ",
    "fn ",
    "def ",
    "func ",
    "function ",
    "object ",
    "type ",
];

/// What an import line starts with, after its indent.
const IMPORTS: &[&str] = &[
    "package ",
    "import ",
    "use ",
    "#include",
    "from ",
    "require",
    "using ",
    "namespace ",
    "mod ",
];

/// Whether `line` may hold `term`: its letters in order, any case, `_` and `-`
/// skipped (D8 rule 10), so only a line that passes is tokenized. Always for
/// a term that is not ASCII.
fn may_hold(line: &str, term: &str) -> bool {
    let term = term.as_bytes();
    if term.is_empty() || !term.is_ascii() {
        return true;
    }
    let bytes = line.as_bytes();
    'start: for start in 0..bytes.len() {
        if !bytes[start].eq_ignore_ascii_case(&term[0]) {
            continue;
        }
        let mut at = start + 1;
        for want in &term[1..] {
            while at < bytes.len() && matches!(bytes[at], b'_' | b'-') {
                at += 1;
            }
            if at >= bytes.len() || !bytes[at].eq_ignore_ascii_case(want) {
                continue 'start;
            }
            at += 1;
        }
        return true;
    }
    false
}

/// Whether `line` holds one of `terms` as a token answers it.
fn holds_term(line: &str, terms: &[String]) -> bool {
    let maybe: Vec<&String> = terms.iter().filter(|term| may_hold(line, term)).collect();
    if maybe.is_empty() {
        return false;
    }
    let mut found = false;
    tokens(line, |token, _| {
        found = found || maybe.iter().any(|term| answers(token, term));
    });
    found
}

/// Whether `line` holds each of `phrases` side by side: a phrase's terms the
/// whole joins of runs that follow each other, the first answering as a
/// token's start, the last as its own term does.
#[must_use]
pub fn holds_phrases(line: &str, phrases: &[Vec<String>]) -> bool {
    if phrases.is_empty() {
        return true;
    }
    let words = query_terms(line);
    phrases.iter().all(|phrase| {
        words.windows(phrase.len()).any(|window| {
            window
                .iter()
                .zip(phrase)
                .enumerate()
                .all(|(index, (word, term))| {
                    if index + 1 == phrase.len() {
                        answers(word, term) || word == term
                    } else {
                        word == term
                    }
                })
        })
    })
}

/// The line to show of a file that holds `asked` (D8 rule 9): the first that
/// holds a term and defines something without being an import; else the first
/// that holds a term and is no import; else the first that holds a term. A
/// line must hold the query's phrases. `None` where no line does.
#[must_use]
pub fn best_line(text: &str, asked: &Query) -> Option<Line> {
    let lines: Vec<&str> = text.lines().collect();
    let mut plain: Option<usize> = None;
    let mut any: Option<usize> = None;
    for (index, line) in lines.iter().enumerate() {
        if !holds_term(line, &asked.terms) || !holds_phrases(line, &asked.phrases) {
            continue;
        }
        let lead = line.trim_start();
        let import = IMPORTS.iter().any(|word| lead.starts_with(word));
        any.get_or_insert(index);
        if import {
            continue;
        }
        if DEFINES.iter().any(|word| line.contains(word)) {
            return Some(line_at(&lines, index));
        }
        plain.get_or_insert(index);
    }
    plain.or(any).map(|index| line_at(&lines, index))
}

fn line_at(lines: &[&str], index: usize) -> Line {
    let around = |at: Option<usize>| {
        at.and_then(|at| lines.get(at))
            .map_or_else(String::new, |line| (*line).to_owned())
    };
    Line {
        number: u32::try_from(index + 1).unwrap_or(u32::MAX),
        before: around(index.checked_sub(1)),
        text: lines[index].to_owned(),
        after: around(Some(index + 1)),
    }
}

/// The letters of `text` the terms cover (D10): for each part of a run whose
/// token answers a term, from the part's start over as many of its kept
/// letters as the term has, `_` and `-` between them included. Byte ranges of
/// `text`, ascending and never overlapping.
#[must_use]
pub fn marks(text: &str, terms: &[String]) -> Vec<(usize, usize)> {
    let mut spans: Vec<(usize, usize)> = Vec::new();
    runs(text, |run| {
        for &(from, _) in &run.parts {
            let token = &run.joined[from..];
            for term in terms {
                if !answers(capped(token), term) {
                    continue;
                }
                // The characters of `joined` the term covers, back to the text.
                let first = run.spans.iter().position(|(at, _, _)| *at == from);
                let last = run
                    .spans
                    .iter()
                    .rposition(|(at, _, _)| *at < from + term.len());
                if let (Some(first), Some(last)) = (first, last) {
                    let start = run.spans[first].1;
                    let (_, at, len) = run.spans[last];
                    spans.push((start, at + len));
                }
            }
        }
    });
    spans.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(spans.len());
    for (start, end) in spans {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(text: &str) -> Vec<String> {
        let mut out = Vec::new();
        tokens(text, |token, _| out.push(token.to_owned()));
        out
    }

    fn terms(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| (*w).to_owned()).collect()
    }

    fn marked<'a>(text: &'a str, words: &[&str]) -> Vec<&'a str> {
        marks(text, &terms(words))
            .into_iter()
            .map(|(start, end)| &text[start..end])
            .collect()
    }

    // -- D4 --

    #[test]
    fn a_path_is_left_out_by_its_folder_its_lock_file_or_its_made_name() {
        assert_eq!(refuses_path("src/main.rs"), None);
        assert_eq!(
            refuses_path("web/node_modules/x/index.js"),
            Some(LeftOut::Folder)
        );
        assert_eq!(refuses_path("vendor/a.go"), Some(LeftOut::Folder));
        // A file named like a folder is still read.
        assert_eq!(refuses_path("docs/build"), None);
        assert_eq!(refuses_path("Cargo.lock"), Some(LeftOut::LockFile));
        assert_eq!(refuses_path("sub/go.sum"), Some(LeftOut::LockFile));
        assert_eq!(refuses_path("deps.lock"), Some(LeftOut::LockFile));
        assert_eq!(refuses_path("Cargo.toml"), None);
        assert_eq!(refuses_path("static/app.MIN.JS"), Some(LeftOut::MadeFile));
        assert_eq!(refuses_path("api/svc.pb.go"), Some(LeftOut::MadeFile));
        assert_eq!(refuses_path("api/svc.go"), None);
    }

    #[test]
    fn a_size_is_read_from_the_tree() {
        assert_eq!(refuses_size(0), Some(LeftOut::Empty));
        assert_eq!(refuses_size(SIZE_CAP), None);
        assert_eq!(refuses_size(SIZE_CAP + 1), Some(LeftOut::TooLarge));
    }

    #[test]
    fn bytes_are_left_out_for_a_nul_anywhere_a_long_line_a_generated_head_or_no_text() {
        assert_eq!(refuses_bytes(b"fn main() {}\n"), None);
        assert_eq!(refuses_bytes(b""), Some(LeftOut::Empty));
        // Past the first 8 KiB, where a head-only check stopped looking.
        let late = format!("{}\0", format!("{}\n", "a".repeat(99)).repeat(100));
        assert_eq!(refuses_bytes(late.as_bytes()), Some(LeftOut::Binary));
        let long = format!("x = 1\n{}\n", "y".repeat(LONGEST_LINE + 1));
        assert_eq!(refuses_bytes(long.as_bytes()), Some(LeftOut::LongLine));
        let edge = format!("{}\n", "y".repeat(LONGEST_LINE));
        assert_eq!(refuses_bytes(edge.as_bytes()), None);
        assert_eq!(
            refuses_bytes(b"// Code generated by protoc. DO NOT EDIT.\npackage x\n"),
            Some(LeftOut::Generated)
        );
        assert_eq!(
            refuses_bytes(b"/* @Generated */\nx\n"),
            Some(LeftOut::Generated)
        );
        let head_clean = format!("{}\n// do not edit\n", "a\n".repeat(400));
        assert_eq!(refuses_bytes(head_clean.as_bytes()), None);
        // Not UTF-8, and a control character as Windows-1252.
        assert_eq!(refuses_bytes(b"ab\x81\x01cd"), Some(LeftOut::NotText));
        let big = vec![b'a'; usize::try_from(SIZE_CAP).unwrap() + 1];
        assert_eq!(refuses_bytes(&big), Some(LeftOut::TooLarge));
    }

    #[test]
    fn latin_1_text_is_kept_and_found_by_its_words() {
        let bytes = b"// Gr\xf6\xdfe \xe4ndern\nint gr\xf6\xdfe = 1;\n";
        assert_eq!(refuses_bytes(bytes), None);
        let text = decode(bytes).expect("Windows-1252");
        assert!(text.contains("Größe ändern"));
        assert!(distinct_tokens(&text).contains(&"größe".to_owned()));
        assert!(matches!(decode("é".as_bytes()), Some(Cow::Borrowed(_))));
    }

    #[test]
    fn gitattributes_marks_a_path_a_folder_and_a_name_glob() {
        let attributes = Attributes::parse(
            "# made\n*.pb.go linguist-generated\ngen/ linguist-generated=true\n/third/** linguist-vendored\n\
             src/schema.rs linguist-generated\nsrc/keep.pb.go -linguist-generated\n*.md text\nweird?.x linguist-generated\n",
        );
        assert!(attributes.marks("api/svc.pb.go"));
        assert!(
            !attributes.marks("src/keep.pb.go"),
            "a later line unmarks it"
        );
        assert!(attributes.marks("gen/a.rs"));
        assert!(
            attributes.marks("deep/gen/a.rs"),
            "a folder of the name at any depth"
        );
        assert!(attributes.marks("third/x/y.c"));
        assert!(!attributes.marks("deep/third/y.c"), "from the root only");
        assert!(attributes.marks("src/schema.rs"));
        assert!(!attributes.marks("src/schema.rs.bak"));
        assert!(!attributes.marks("README.md"));
        assert!(
            !attributes.marks("weird1.x"),
            "an unread shape marks nothing"
        );
        assert!(!Attributes::parse("").marks("a"));
        assert!(!Attributes::parse("* linguist-generated=false").marks("a.rs"));
    }

    #[test]
    fn every_left_out_has_its_word() {
        let all = [
            LeftOut::NotRegular,
            LeftOut::Folder,
            LeftOut::LockFile,
            LeftOut::MadeFile,
            LeftOut::TooLarge,
            LeftOut::Empty,
            LeftOut::Binary,
            LeftOut::NotText,
            LeftOut::LongLine,
            LeftOut::Generated,
            LeftOut::Marked,
            LeftOut::Share,
            LeftOut::Bound,
        ];
        let words: BTreeSet<&str> = all.iter().map(|left| left.word()).collect();
        assert_eq!(words.len(), all.len());
        assert!(
            words
                .iter()
                .all(|w| w.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
        );
    }

    // -- D5 --

    #[test]
    fn a_run_gives_a_token_from_each_parts_start() {
        assert_eq!(
            toks("ADProxyServer"),
            ["adproxyserver", "proxyserver", "server"]
        );
        assert_eq!(
            toks("abcMappingService"),
            ["abcmappingservice", "mappingservice", "service"]
        );
        assert_eq!(toks("ad_proxy-url"), ["adproxyurl", "proxyurl", "url"]);
        assert_eq!(toks("com.acme.adproxy"), ["com", "acme", "adproxy"]);
        assert_eq!(toks("http2server"), ["http2server", "server"]);
        assert_eq!(toks("HTTPServer x"), ["httpserver", "server"]);
        assert_eq!(toks("Größe ändern"), ["größe", "ändern"]);
        assert_eq!(toks("_private __dunder__"), ["private", "dunder"]);
        assert_eq!(toks("a b c"), Vec::<String>::new());
    }

    #[test]
    fn a_hash_is_one_token_and_a_long_identifier_is_cut_at_a_characters_start() {
        let hex = "a1b2c3d4".repeat(40);
        assert_eq!(toks(&hex).len(), 1);
        let long = format!("{}ä{}", "x".repeat(47), "y".repeat(20));
        let token = &toks(&long)[0];
        assert!(token.len() <= TOKEN_MAX);
        assert_eq!(token, &"x".repeat(47));
    }

    #[test]
    fn what_was_typed_is_one_term_a_run() {
        assert_eq!(query_terms("AD-Proxy server"), ["adproxy", "server"]);
        assert_eq!(query_terms("SettleBridgeServer"), ["settlebridgeserver"]);
        assert_eq!(query_terms("a"), Vec::<String>::new());
    }

    /// Property: every token of a text answers, as a prefix, the term its own
    /// run gives when typed, from a part's start.
    #[test]
    fn every_token_is_found_by_typing_its_own_runs_tail() {
        let texts = [
            "public final class ADProxyServer {",
            "import com.acme.adproxy.config.AdProxyConfig;",
            "image: registry.acme.example/ad-proxy:4.2.0",
            "let http2server = HTTPServer::new(größe_ändern);",
            "fn parse_jwt_token(raw: &str) -> Result<JwtClaims, Error>",
        ];
        for text in texts {
            tokens(text, |token, at| {
                // The text from the part's start to the run's end, typed.
                let rest = &text[at..];
                let end = rest
                    .find(|c: char| class(c) == Class::Hard)
                    .unwrap_or(rest.len());
                let typed = query_terms(&rest[..end]);
                assert_eq!(typed, [token], "{text}");
                assert!(answers(token, &typed[0]));
            });
        }
    }

    #[test]
    fn a_query_drops_short_and_negated_words_and_or_asks_nothing() {
        assert_eq!(
            query("adproxy co -legacy !old jwt").terms,
            terms(&["adproxy", "jwt"])
        );
        assert!(query("adproxy or gateway").is_empty());
        assert!(query("co").is_empty());
        let phrased = query("\"connection refused\" retry");
        assert_eq!(phrased.terms, terms(&["connection", "refused", "retry"]));
        assert_eq!(phrased.phrases, vec![terms(&["connection", "refused"])]);
        assert_eq!(
            query("Settle-Bridge settlebridge").terms,
            terms(&["settlebridge"])
        );
    }

    #[test]
    fn three_letters_are_whole_and_four_a_words_start() {
        assert!(answers("jwt", "jwt"));
        assert!(!answers("jwtfilter", "jwt"));
        assert!(answers("jwtfilter", "jwtf"));
        assert!(!answers("dproxy", "adproxy"));
    }

    // -- D8 and D10 --

    #[test]
    fn the_line_shown_defines_something_and_is_no_import() {
        let text = "package com.acme.adproxy;\n\nimport com.acme.adproxy.config.AdProxyConfig;\n\
                    /** Terminates TLS for adproxy clients. */\npublic final class ADProxyServer {\n    private final AdProxyConfig config;\n}\n";
        let line = best_line(text, &query("adproxy")).expect("a line");
        assert_eq!(line.text, "public final class ADProxyServer {");
        assert_eq!(line.number, 5);
        assert_eq!(line.before, "/** Terminates TLS for adproxy clients. */");
        assert_eq!(line.after, "    private final AdProxyConfig config;");
        // No definition: the first that is no import.
        let plain = best_line(
            "import x.adproxy;\nwe call adproxy here\n",
            &query("adproxy"),
        )
        .unwrap();
        assert_eq!(plain.text, "we call adproxy here");
        // Only imports: the first.
        let only = best_line("import x.adproxy;\nuse adproxy::y;\n", &query("adproxy")).unwrap();
        assert_eq!((only.number, only.before.as_str()), (1, ""));
        assert!(best_line("nothing here\n", &query("adproxy")).is_none());
    }

    #[test]
    fn the_cheap_check_skips_underscores_and_dashes() {
        assert!(may_hold("let ad_proxy = 1;", "adproxy"));
        assert!(may_hold("image: ad-proxy:4", "adproxy"));
        assert!(may_hold("AD_PROXY", "adproxy"));
        assert!(!may_hold("a dproxy", "adproxy"));
        let line = best_line("x\nlet ad_proxy_url = 1;\n", &query("adproxy")).unwrap();
        assert_eq!(line.number, 2);
    }

    #[test]
    fn a_phrase_must_stand_side_by_side() {
        let asked = query("\"connection refused\"");
        assert!(
            best_line("refused connection\nthe connection refused it\n", &asked)
                .is_some_and(|l| l.number == 2)
        );
        assert!(best_line("connection was refused\n", &asked).is_none());
    }

    #[test]
    fn marks_cover_the_letters_a_term_takes_from_a_parts_start() {
        assert_eq!(
            marked("public final class ADProxyServer {", &["adproxy"]),
            ["ADProxy"]
        );
        assert_eq!(
            marked("image: registry/ad-proxy:4.2.0", &["adproxy"]),
            ["ad-proxy"]
        );
        assert_eq!(marked("class MyADProxy", &["adproxy"]), ["ADProxy"]);
        assert_eq!(marked("ADProxyServer", &["proxy"]), ["Proxy"]);
        assert_eq!(
            marked(
                "settle_bridge SettleBridgeServer",
                &["settlebridge", "server"]
            ),
            ["settle_bridge", "SettleBridgeServer"]
        );
        assert_eq!(marked("parseJwt JwtFilter", &["jwt"]), ["Jwt"]);
        assert_eq!(marked("Größe ändern", &["größe"]), ["Größe"]);
        assert!(marked("dproxy", &["adproxy"]).is_empty());
    }
}
