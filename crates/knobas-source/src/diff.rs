//! The unified-diff parser every git adapter shares (ADR-0020,
//! `CONTEXT.md` **Diff**).
//!
//! A git server answers a commit's or a pull request's change as the text
//! `git diff` prints; this turns that text into the [`DiffFile`]s a
//! [`Block::Diff`](crate::view::Block::Diff) carries, so a Gitea and a later
//! GitLab adapter draw the same diff from the same parser.
//!
//! **A ceiling here, as a safety stop** (ADR-0020 rule 6; #2269, spec #2234
//! step 6a): at most [`MAX_LINES_PER_VIEW`] lines in all. The window draws a
//! long diff by windowing its lines and holds a file over 1,500 changed lines
//! behind *Load diff*, so nothing under the ceiling is cut. A file the
//! ceiling cuts keeps the lines before the cut, its **full** `additions` and
//! `deletions`, and `truncated: true`; the diff says it hit the ceiling and
//! names it, and the source's own page has the rest.

/// The ceiling: the most lines one diff carries over all its files. The
/// shell names it when a diff hits it (`tf/diff/diff.ts` `DIFF_CEILING`).
pub const MAX_LINES_PER_VIEW: usize = 50_000;

/// What happened to one file.
///
/// - `renamed`: Moved, with or without edits; `old_path` names where from.
/// - `binary`: A binary file changed (or was added or deleted): no lines to
///   draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "entity.ts")]
pub enum DiffChange {
    Added,
    Modified,
    Deleted,
    Renamed,
    Binary,
}

/// One file of a diff.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct DiffFile {
    /// Where the file is after the change (where it was, for a deletion).
    pub path: String,
    /// Where it was before, for a rename or a copy; `None` otherwise.
    pub old_path: Option<String>,
    pub change: DiffChange,
    /// Added lines, counted over the whole file even where it was cut.
    pub additions: u32,
    /// Removed lines, counted the same way.
    pub deletions: u32,
    pub hunks: Vec<Hunk>,
    /// Some of this file's lines were cut by the ceiling.
    pub truncated: bool,
    /// How alike the two sides of a rename or copy are, in percent, where
    /// git says (`similarity index 92%`; #2268).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub similarity: Option<u8>,
    /// The file's mode before and after, where it changed (`old mode
    /// 100644` / `new mode 100755`; #2268): an executable bit set or cleared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub old_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub new_mode: Option<String>,
    /// The review conversations on this file's lines (issue #709): what a
    /// source that keeps line comments read for it, in the order the file
    /// reads -- empty from the parser, which knows only the text. Absent on
    /// the wire reads as empty, so a diff drawn before #709 decodes.
    ///
    /// The outdated ones last.
    #[serde(default)]
    pub threads: Vec<LineThread>,
}

/// One conversation on one line of a file's diff (issue #709, `CONTEXT.md`
/// **Line thread**): the comments a review left there, oldest first.
///
/// `line` counts on `side`, as a [`ReviewComment`](crate::ReviewComment)'s
/// does: the file as it was (`old`) or as it becomes (`new`). The shell draws
/// the thread under the line holding that number on that side.
///
/// **Outdated is the source's answer** (ADR-0007): the line is no longer in
/// this diff as it was when the comment was made -- gone, or another text in
/// its place since a later push. An outdated thread is drawn muted at the end
/// of its file, never on a line it was not written about.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct LineThread {
    pub side: crate::ReviewSide,
    pub line: u32,
    pub outdated: bool,
    pub comments: Vec<LineComment>,
}

/// A [`LineThread`] on a file the diff no longer holds (issue #709): written
/// on `path` in an earlier round, which a later push reverted or dropped.
/// Outdated by definition -- there is no line left to be on -- and drawn in
/// the diff's own outdated list, never dropped.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct FileThread {
    pub path: String,
    pub thread: LineThread,
}

/// One comment of a [`LineThread`]. Source text, like every string of a view:
/// `body` is the source's Markdown, drawn legibly and never as markup
/// (ADR-0020 rule 5).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct LineComment {
    /// Who wrote it, as the source names them (a login).
    pub author: String,
    pub body: String,
    /// When, as the source's RFC 3339 instant; `None` where it sent none.
    pub at: Option<String>,
}

/// One `@@` hunk.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct Hunk {
    /// The `@@ -a,b +c,d @@ context` line as the server sent it.
    pub header: String,
    pub lines: Vec<DiffLine>,
}

/// Whether a line was kept, added or removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "entity.ts")]
pub enum DiffLineKind {
    Context,
    Add,
    Del,
}

/// One line of a hunk, with its number on each side it exists on.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct DiffLine {
    #[ts(inline)]
    pub kind: DiffLineKind,
    /// The line's number before the change; `None` for an added line.
    pub old: Option<u32>,
    /// The line's number after the change; `None` for a removed line.
    pub new: Option<u32>,
    /// The line without its `+`, `-` or ` ` marker.
    pub text: String,
}

/// Parse `git diff` text into files, under the ceiling.
#[must_use]
pub fn parse(text: &str) -> Vec<DiffFile> {
    parse_capped(text, usize::MAX, MAX_LINES_PER_VIEW).0
}

/// Parse `git diff` text under the ceiling, and say whether it cut
/// anything: the two halves of a [`Block::Diff`](crate::view::Block::Diff).
#[must_use]
pub fn parse_view(text: &str) -> (Vec<DiffFile>, bool) {
    parse_capped(text, usize::MAX, MAX_LINES_PER_VIEW)
}

/// Parse under explicit caps. Answers the files and whether `per_view` cut
/// anything.
///
/// Hunk lines are read by the counts their `@@` header gives, never by their
/// first characters alone: a removed `-- comment` prints as `--- comment`, and
/// only the count says it is a line and not a file header.
fn parse_capped(text: &str, per_file: usize, per_view: usize) -> (Vec<DiffFile>, bool) {
    let mut out = Vec::new();
    let mut file: Option<FileBuilder> = None;
    let mut hunk: Option<HunkState> = None;
    let mut view_used = 0usize;
    let mut view_cut = false;

    for raw in text.lines() {
        let open = hunk
            .as_ref()
            .is_some_and(|h| h.old_left > 0 || h.new_left > 0);
        if raw.starts_with('\\') && hunk.is_some() {
            // "\ No newline at end of file": about the line before it, and
            // not a line of the file.
            continue;
        }
        if open {
            let marked = match raw.as_bytes().first() {
                Some(b'+') => Some((DiffLineKind::Add, &raw[1..])),
                Some(b'-') => Some((DiffLineKind::Del, &raw[1..])),
                Some(b' ') => Some((DiffLineKind::Context, &raw[1..])),
                // Some tools strip the space off an empty context line.
                None => Some((DiffLineKind::Context, "")),
                // A malformed hunk: stop reading it as one.
                Some(_) => None,
            };
            if let (Some((kind, body)), Some(f), Some(h)) = (marked, file.as_mut(), hunk.as_mut()) {
                let line = h.next(kind, body);
                match kind {
                    DiffLineKind::Add => f.additions += 1,
                    DiffLineKind::Del => f.deletions += 1,
                    DiffLineKind::Context => {}
                }
                if f.kept < per_file && view_used < per_view {
                    f.kept += 1;
                    view_used += 1;
                    h.lines.push(line);
                } else {
                    f.truncated = true;
                    if f.kept < per_file {
                        view_cut = true;
                    }
                }
                continue;
            }
        }
        if let Some(h) = hunk.take()
            && let Some(f) = file.as_mut()
        {
            f.close_hunk(Some(h));
        }
        file_line(raw, &mut file, &mut hunk, &mut out);
    }
    if let Some(mut f) = file.take() {
        f.close_hunk(hunk.take());
        out.push(f.finish());
    }
    (out, view_cut)
}

/// One line outside a hunk: a file header, a hunk header, or noise.
fn file_line(
    raw: &str,
    file: &mut Option<FileBuilder>,
    hunk: &mut Option<HunkState>,
    out: &mut Vec<DiffFile>,
) {
    if let Some(rest) = raw.strip_prefix("diff --git ") {
        if let Some(mut done) = file.take() {
            done.close_hunk(hunk.take());
            out.push(done.finish());
        }
        *file = Some(FileBuilder::new(rest));
        return;
    }
    let Some(f) = file.as_mut() else {
        // Anything before the first file: a mail header, a commit message.
        return;
    };
    if raw.starts_with("@@ ") {
        *hunk = HunkState::parse(raw);
    } else if raw.starts_with("new file mode") {
        f.added = true;
    } else if raw.starts_with("deleted file mode") {
        f.deleted = true;
    } else if let Some(from) = raw
        .strip_prefix("rename from ")
        .or_else(|| raw.strip_prefix("copy from "))
    {
        f.renamed |= raw.starts_with("rename");
        f.copied |= raw.starts_with("copy");
        f.from = Some(unquote(from));
    } else if let Some(to) = raw
        .strip_prefix("rename to ")
        .or_else(|| raw.strip_prefix("copy to "))
    {
        f.to = Some(unquote(to));
    } else if let Some(rest) = raw
        .strip_prefix("similarity index ")
        .and_then(|r| r.strip_suffix('%'))
    {
        f.similarity = rest.parse().ok();
    } else if let Some(mode) = raw.strip_prefix("old mode ") {
        f.old_mode = Some(mode.to_owned());
    } else if let Some(mode) = raw.strip_prefix("new mode ") {
        f.new_mode = Some(mode.to_owned());
    } else if raw.starts_with("Binary files ") || raw == "GIT binary patch" {
        f.binary = true;
    } else if let Some(minus) = raw.strip_prefix("--- ") {
        f.minus = side_path(minus, "a/");
    } else if let Some(plus) = raw.strip_prefix("+++ ") {
        f.plus = side_path(plus, "b/");
    }
}

/// One file, while its header and hunks are being read.
struct FileBuilder {
    /// The two paths of the `diff --git` line, the fallback when nothing more
    /// specific names them (a binary file, a pure rename).
    header: (Option<String>, Option<String>),
    minus: Option<String>,
    plus: Option<String>,
    from: Option<String>,
    to: Option<String>,
    added: bool,
    deleted: bool,
    renamed: bool,
    copied: bool,
    binary: bool,
    similarity: Option<u8>,
    old_mode: Option<String>,
    new_mode: Option<String>,
    additions: u32,
    deletions: u32,
    hunks: Vec<Hunk>,
    kept: usize,
    truncated: bool,
}

impl FileBuilder {
    fn new(header: &str) -> Self {
        Self {
            header: header_paths(header),
            minus: None,
            plus: None,
            from: None,
            to: None,
            added: false,
            deleted: false,
            renamed: false,
            copied: false,
            binary: false,
            similarity: None,
            old_mode: None,
            new_mode: None,
            additions: 0,
            deletions: 0,
            hunks: Vec::new(),
            kept: 0,
            truncated: false,
        }
    }

    fn close_hunk(&mut self, hunk: Option<HunkState>) {
        if let Some(h) = hunk
            && !h.lines.is_empty()
        {
            self.hunks.push(Hunk {
                header: h.header,
                lines: h.lines,
            });
        }
    }

    fn finish(self) -> DiffFile {
        let old_side = self.from.clone().or(self.minus.clone());
        let path = self
            .to
            .or(self.plus)
            .or(self.minus.clone())
            .or(self.header.1)
            .or(self.header.0.clone())
            .unwrap_or_default();
        let change = if self.binary {
            DiffChange::Binary
        } else if self.deleted {
            DiffChange::Deleted
        } else if self.added || self.copied {
            DiffChange::Added
        } else if self.renamed {
            DiffChange::Renamed
        } else {
            DiffChange::Modified
        };
        let old_path = if self.renamed || self.copied {
            old_side.or(self.header.0)
        } else {
            None
        };
        DiffFile {
            path,
            old_path,
            change,
            additions: self.additions,
            deletions: self.deletions,
            hunks: self.hunks,
            truncated: self.truncated,
            similarity: self.similarity,
            old_mode: self.old_mode,
            new_mode: self.new_mode,
            threads: Vec::new(),
        }
    }
}

/// The hunk being read: how many lines of each side its header promised
/// that have not arrived yet, and the numbers the next ones carry.
struct HunkState {
    header: String,
    old_left: u32,
    new_left: u32,
    old_no: u32,
    new_no: u32,
    lines: Vec<DiffLine>,
}

impl HunkState {
    /// `@@ -a[,b] +c[,d] @@ context`; `None` for a header that is not one.
    fn parse(header: &str) -> Option<Self> {
        let rest = header.strip_prefix("@@ -")?;
        let (ranges, _) = rest.split_once(" @@")?;
        let (old, new) = ranges.split_once(" +")?;
        let range = |r: &str| -> Option<(u32, u32)> {
            match r.split_once(',') {
                Some((start, len)) => Some((start.parse().ok()?, len.parse().ok()?)),
                None => Some((r.parse().ok()?, 1)),
            }
        };
        let (old_no, old_left) = range(old)?;
        let (new_no, new_left) = range(new)?;
        Some(Self {
            header: header.to_owned(),
            old_left,
            new_left,
            old_no,
            new_no,
            lines: Vec::new(),
        })
    }

    /// Number one line and use up its share of the header's counts.
    fn next(&mut self, kind: DiffLineKind, text: &str) -> DiffLine {
        let (old, new) = match kind {
            DiffLineKind::Context => (Some(self.old_no), Some(self.new_no)),
            DiffLineKind::Del => (Some(self.old_no), None),
            DiffLineKind::Add => (None, Some(self.new_no)),
        };
        if old.is_some() {
            self.old_no += 1;
            self.old_left = self.old_left.saturating_sub(1);
        }
        if new.is_some() {
            self.new_no += 1;
            self.new_left = self.new_left.saturating_sub(1);
        }
        DiffLine {
            kind,
            old,
            new,
            text: text.to_owned(),
        }
    }
}

/// The path on a `---` or `+++` line: `None` for `/dev/null`, otherwise
/// unquoted, without git's trailing tab and without its `a/`/`b/` prefix.
fn side_path(value: &str, prefix: &str) -> Option<String> {
    let value = if value.starts_with('"') {
        unquote(value)
    } else {
        value.split('\t').next().unwrap_or(value).to_owned()
    };
    if value == "/dev/null" {
        return None;
    }
    Some(value.strip_prefix(prefix).unwrap_or(&value).to_owned())
}

/// The two paths of `diff --git a/x b/y`.
///
/// Ambiguous when a path holds `" b/"`, so the split that makes both halves
/// the same path wins, which is every case but a rename -- and a rename names
/// its paths again on its own lines.
fn header_paths(rest: &str) -> (Option<String>, Option<String>) {
    if rest.starts_with('"') {
        let (first, tail) = take_quoted(rest);
        let second = tail.trim_start();
        let second = if second.starts_with('"') {
            take_quoted(second).0
        } else {
            second.to_owned()
        };
        return (strip(&first, "a/"), strip(&second, "b/"));
    }
    let splits: Vec<usize> = rest.match_indices(" b/").map(|(i, _)| i).collect();
    let pick = splits
        .iter()
        .copied()
        .find(|&i| rest[..i].strip_prefix("a/") == Some(&rest[i + 3..]))
        .or_else(|| splits.first().copied());
    match pick {
        Some(i) => (strip(&rest[..i], "a/"), strip(&rest[i + 1..], "b/")),
        None => (None, None),
    }
}

fn strip(value: &str, prefix: &str) -> Option<String> {
    Some(value.strip_prefix(prefix).unwrap_or(value).to_owned())
}

/// A path git may have quoted: `"caf\303\251.txt"` → `café.txt`.
fn unquote(value: &str) -> String {
    if value.starts_with('"') {
        take_quoted(value).0
    } else {
        value.to_owned()
    }
}

/// Read one C-quoted token off the front of `value` (which starts with `"`),
/// answering it unescaped and what follows it.
fn take_quoted(value: &str) -> (String, &str) {
    let bytes = value.as_bytes();
    let mut out: Vec<u8> = Vec::new();
    let mut i = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => return (String::from_utf8_lossy(&out).into_owned(), &value[i + 1..]),
            b'\\' if i + 1 < bytes.len() => {
                if let Some(byte) = bytes.get(i + 1..i + 4).and_then(octal) {
                    out.push(byte);
                    i += 4;
                    continue;
                }
                out.push(match bytes[i + 1] {
                    b'n' => b'\n',
                    b't' => b'\t',
                    b'r' => b'\r',
                    other => other,
                });
                i += 2;
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    (String::from_utf8_lossy(&out).into_owned(), "")
}

/// The byte a three-digit octal escape names (`303` → `0xC3`), or `None`
/// unless the bytes are all octal digits and name one byte. Read
/// as bytes: the text is whatever the instance answered, and a slice of the
/// `&str` could end inside a character (#1645).
fn octal(digits: &[u8]) -> Option<u8> {
    if !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    u8::from_str_radix(std::str::from_utf8(digits).ok()?, 8).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(text: &str) -> DiffFile {
        let mut files = parse(text);
        assert_eq!(files.len(), 1, "{files:#?}");
        files.remove(0)
    }

    fn line(kind: DiffLineKind, old: Option<u32>, new: Option<u32>, text: &str) -> DiffLine {
        DiffLine {
            kind,
            old,
            new,
            text: text.to_owned(),
        }
    }

    use DiffLineKind::{Add, Context, Del};

    /// A hunk header with a function context, and line numbers on both sides
    /// starting where the header says.
    #[test]
    fn a_modified_file_numbers_its_lines_from_the_hunk_header() {
        let file = one(include_str!("../fixtures/diff/modify.diff"));
        assert_eq!(file.path, "lib.rs");
        assert_eq!(file.old_path, None);
        assert_eq!(file.change, DiffChange::Modified);
        assert_eq!((file.additions, file.deletions), (3, 2));
        assert!(!file.truncated);
        assert_eq!(file.hunks.len(), 1);
        let hunk = &file.hunks[0];
        assert_eq!(hunk.header, "@@ -3,6 +3,7 @@ fn main() {");
        assert_eq!(
            hunk.lines,
            vec![
                line(Context, Some(3), Some(3), "}"),
                line(Context, Some(4), Some(4), ""),
                line(
                    Context,
                    Some(5),
                    Some(5),
                    "fn retry(attempts: u32) -> u32 {"
                ),
                line(Del, Some(6), None, "    let base = 30;"),
                line(Del, Some(7), None, "    base * attempts"),
                line(Add, None, Some(6), "    let base = 45;"),
                line(Add, None, Some(7), "    let jitter = 3;"),
                line(Add, None, Some(8), "    base * attempts + jitter"),
                line(Context, Some(8), Some(9), "}"),
            ]
        );
    }

    #[test]
    fn an_added_file_has_only_new_line_numbers() {
        let file = one(include_str!("../fixtures/diff/add.diff"));
        assert_eq!(file.path, "added.txt");
        assert_eq!(file.change, DiffChange::Added);
        assert_eq!((file.additions, file.deletions), (2, 0));
        assert_eq!(
            file.hunks[0].lines,
            vec![
                line(Add, None, Some(1), "brand"),
                line(Add, None, Some(2), "new")
            ]
        );
    }

    #[test]
    fn a_deleted_file_keeps_its_path() {
        let file = one(include_str!("../fixtures/diff/delete.diff"));
        assert_eq!(file.path, "old.txt");
        assert_eq!(file.old_path, None);
        assert_eq!(file.change, DiffChange::Deleted);
        assert_eq!((file.additions, file.deletions), (0, 2));
        assert_eq!(
            file.hunks[0].lines,
            vec![
                line(Del, Some(1), None, "gone"),
                line(Del, Some(2), None, "away")
            ]
        );
    }

    #[test]
    fn a_rename_with_edits_names_both_paths_and_carries_its_hunk() {
        let file = one(include_str!("../fixtures/diff/rename-with-edits.diff"));
        assert_eq!(file.path, "renamed.txt");
        assert_eq!(file.old_path.as_deref(), Some("moved.txt"));
        assert_eq!(file.change, DiffChange::Renamed);
        assert_eq!((file.additions, file.deletions), (1, 1));
        assert_eq!(file.hunks[0].header, "@@ -5,4 +5,4 @@ delta");
        assert_eq!(file.hunks[0].lines.len(), 5);
    }

    #[test]
    fn a_pure_rename_has_no_hunks() {
        let file = one(include_str!("../fixtures/diff/rename-pure.diff"));
        assert_eq!(file.path, "pure-renamed.txt");
        assert_eq!(file.old_path.as_deref(), Some("pure.txt"));
        assert_eq!(file.change, DiffChange::Renamed);
        assert_eq!((file.additions, file.deletions), (0, 0));
        assert!(file.hunks.is_empty());
    }

    #[test]
    fn a_binary_file_says_so_and_has_no_lines() {
        let file = one(include_str!("../fixtures/diff/binary.diff"));
        assert_eq!(file.path, "logo.png");
        assert_eq!(file.change, DiffChange::Binary);
        assert!(file.hunks.is_empty());

        let added = one(include_str!("../fixtures/diff/binary-added.diff"));
        assert_eq!(added.path, "icon.ico");
        assert_eq!(added.change, DiffChange::Binary);
    }

    /// The marker is about the line before it, not a line of the file: it
    /// counts nowhere and draws nowhere.
    #[test]
    fn no_newline_at_end_of_file_is_not_a_line() {
        let file = one(include_str!("../fixtures/diff/no-newline.diff"));
        assert_eq!((file.additions, file.deletions), (1, 1));
        assert_eq!(
            file.hunks[0].lines,
            vec![
                line(Context, Some(1), Some(1), "line one"),
                line(Del, Some(2), None, "line two"),
                line(Add, None, Some(2), "line two"),
            ]
        );
    }

    /// A removed `-- comment` prints as `--- comment` and an added `++ x` as
    /// `+++ x`: inside a hunk they are lines, never file headers. And a path
    /// with a space carries git's trailing tab on its `---`/`+++` lines.
    #[test]
    fn a_content_line_that_looks_like_a_header_is_a_line() {
        let file = one(include_str!("../fixtures/diff/header-lookalike.diff"));
        assert_eq!(file.path, "my query.sql");
        assert_eq!((file.additions, file.deletions), (1, 1));
        assert_eq!(
            file.hunks[0].lines,
            vec![
                line(Context, Some(1), Some(1), "select 1;"),
                line(Del, Some(2), None, "-- old comment"),
                line(Add, None, Some(2), "++ not a header"),
                line(Context, Some(3), Some(3), "select 2;"),
            ]
        );
    }

    /// Git quotes a path with bytes outside ASCII and escapes them in octal.
    /// And `@@ -1 +1 @@` leaves the count out, meaning one.
    #[test]
    fn a_quoted_path_is_unquoted() {
        let file = one(include_str!("../fixtures/diff/quoted-path.diff"));
        assert_eq!(file.path, "café.txt");
        assert_eq!(
            file.hunks[0].lines,
            vec![
                line(Del, Some(1), None, "bonjour"),
                line(Add, None, Some(1), "bonsoir")
            ]
        );
    }

    /// The diff is whatever the instance answered, so a path git would never
    /// write -- a backslash, one digit, then a character of more than one
    /// byte -- reads as the escaped character, not as a panic (#1645).
    #[test]
    fn a_short_escape_before_a_multibyte_character_is_the_character() {
        let file = one(concat!(
            "diff --git \"a/\\1aé.txt\" \"b/\\1aé.txt\"\n",
            "--- \"a/\\1aé.txt\"\n",
            "+++ \"b/\\1aé.txt\"\n",
            "@@ -1 +1 @@\n",
            "-bonjour\n",
            "+bonsoir\n",
        ));
        assert_eq!(file.path, "1aé.txt");
        assert_eq!(unquote("\"\\1aé.txt\""), "1aé.txt");
    }

    /// An octal escape is three digits: two digits, or a sign before them,
    /// are the characters themselves.
    #[test]
    fn an_escape_short_of_three_digits_is_its_characters() {
        assert_eq!(unquote("\"a\\12x.txt\""), "a12x.txt");
        assert_eq!(unquote("\"a\\12\""), "a12");
        assert_eq!(unquote("\"a\\+12.txt\""), "a+12.txt");
    }

    #[test]
    fn a_whole_commit_is_every_file_in_order() {
        let files = parse(include_str!("../fixtures/diff/whole-commit.diff"));
        let summary: Vec<_> = files
            .iter()
            .map(|f| (f.path.as_str(), f.change, f.additions, f.deletions))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("added.txt", DiffChange::Added, 2, 0),
                ("lib.rs", DiffChange::Modified, 3, 2),
                ("logo.png", DiffChange::Binary, 0, 0),
                ("notes.txt", DiffChange::Modified, 1, 1),
                ("old.txt", DiffChange::Deleted, 0, 2),
                ("pure-renamed.txt", DiffChange::Renamed, 0, 0),
                ("renamed.txt", DiffChange::Renamed, 1, 1),
            ]
        );
    }

    #[test]
    fn text_before_the_first_file_and_an_empty_diff_are_nothing() {
        assert!(parse("").is_empty());
        let files = parse(&format!(
            "From c90d11a\nSubject: x\n\n{}",
            include_str!("../fixtures/diff/add.diff")
        ));
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "added.txt");
    }

    /// A file of `n` added lines, as git prints it.
    fn added_file(name: &str, n: usize) -> String {
        let mut text = format!(
            "diff --git a/{name} b/{name}\nnew file mode 100644\nindex 0000000..1111111\n--- /dev/null\n+++ b/{name}\n@@ -0,0 +1,{n} @@\n"
        );
        for i in 1..=n {
            text.push_str(&format!("+line {i}\n"));
        }
        text
    }

    /// The ceiling from both sides, on one file: at it nothing is cut, one
    /// over is cut to it with the counts still whole. No cap below it.
    #[test]
    fn the_ceiling_cuts_one_line_over_and_not_at() {
        let at = one(&added_file("at.txt", MAX_LINES_PER_VIEW));
        assert!(!at.truncated);
        assert_eq!(at.hunks[0].lines.len(), MAX_LINES_PER_VIEW);

        let (files, cut) = parse_view(&added_file("over.txt", MAX_LINES_PER_VIEW + 1));
        assert!(cut, "the diff says the ceiling cut it");
        assert!(files[0].truncated);
        let kept: usize = files[0].hunks.iter().map(|h| h.lines.len()).sum();
        assert_eq!(kept, MAX_LINES_PER_VIEW);
        assert_eq!(
            files[0].additions as usize,
            MAX_LINES_PER_VIEW + 1,
            "counts stay full"
        );
    }

    /// #2269: a 20,000-line diff over 40 files, each 500 lines (a 1,500-line
    /// cap and a 6,000-line one each cut it before), comes through whole.
    #[test]
    fn a_twenty_thousand_line_diff_over_forty_files_is_not_cut() {
        let text: String = (0..40)
            .map(|n| added_file(&format!("f{n}.txt"), 500))
            .collect();
        let (files, cut) = parse_view(&text);
        assert!(!cut);
        assert_eq!(files.len(), 40);
        assert!(files.iter().all(|f| !f.truncated && f.hunks[0].lines.len() == 500));
        let big = one(&added_file("big.txt", 3_000));
        assert!(!big.truncated, "a 3,000-line file is not cut either");
        assert_eq!(big.hunks[0].lines.len(), 3_000);
    }

    /// #2268: a rename's similarity and a mode change are read from the
    /// extended header.
    #[test]
    fn a_rename_says_its_similarity_and_a_mode_change_its_modes() {
        let renamed = one(
            "diff --git a/src/old_queue.rs b/src/queue/legacy.rs\n\
             similarity index 92%\n\
             rename from src/old_queue.rs\n\
             rename to src/queue/legacy.rs\n",
        );
        assert_eq!(renamed.change, DiffChange::Renamed);
        assert_eq!(renamed.similarity, Some(92));
        let mode = one(
            "diff --git a/deploy.sh b/deploy.sh\nold mode 100644\nnew mode 100755\n",
        );
        assert_eq!(
            (mode.old_mode.as_deref(), mode.new_mode.as_deref()),
            (Some("100644"), Some("100755"))
        );
        assert_eq!(mode.similarity, None);
    }

    /// A per-view cap from both sides, over files each under it: exactly the cap draws everything, one more cuts the last file,
    /// and a file wholly past the cut keeps its counts and no lines.
    #[test]
    fn the_per_view_cap_cuts_one_line_over_and_not_at() {
        let per_file = 1_000;
        let at = format!(
            "{}{}",
            added_file("a.txt", per_file),
            added_file("b.txt", per_file)
        );
        let (files, cut) = parse_capped(&at, MAX_LINES_PER_VIEW, 2 * per_file);
        assert!(!cut);
        assert!(files.iter().all(|f| !f.truncated));

        let over = format!(
            "{}{}{}",
            added_file("a.txt", per_file),
            added_file("b.txt", per_file + 1),
            added_file("c.txt", 3)
        );
        let (files, cut) = parse_capped(&over, MAX_LINES_PER_VIEW, 2 * per_file);
        assert!(cut);
        assert!(!files[0].truncated);
        assert!(files[1].truncated);
        assert_eq!(files[1].hunks[0].lines.len(), per_file);
        assert_eq!(files[1].additions as usize, per_file + 1);
        assert!(files[2].truncated);
        assert!(files[2].hunks.iter().all(|h| h.lines.is_empty()));
        assert_eq!(files[2].additions, 3);

        let (_, cut) = parse_view(&over);
        assert!(!cut, "under the real caps this is not cut");
    }
}
