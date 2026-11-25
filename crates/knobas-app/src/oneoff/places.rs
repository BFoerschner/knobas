//! Places and findings in a command's output (ADR-0060, section 15).
//!
//! As a line ends, the backend looks for places in it: `path:line:col` and
//! `path:line`, `path(line,col)`, a Python traceback's `File "path", line N`,
//! ShellCheck's `In path line N:`, and ESLint's stylish shape, a path alone on
//! a line with `line:col  severity  words  rule` rows under it. A place is a
//! link only where it names **a file that exists inside the checkout**:
//! absolute under the root, or relative to the run's working directory, then
//! to the root, with no `..` that leaves and no link followed out. Nothing
//! else in a program's output is ever a link to a file.
//!
//! A **finding** is a place a program said something about:
//! `path:line:col: words`, a row under ESLint's path, or the place under
//! rustc's `error[E0308]: words` line. A stack frame and a traceback's row are
//! places and not findings.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use super::lines::{Finding, Place};

/// The longest line searched for places, in characters.
pub const SEARCHED: usize = 4096;
/// The most distinct paths one run asks the disk about.
pub const CANDIDATES: usize = 2000;

const PATH: &str = r#"(?:[A-Za-z]:[\\/])?[^\s:"'<>|()\[\]{}*?]+"#;

static SHAPES: LazyLock<[Regex; 4]> = LazyLock::new(|| {
    [
        // src/a.ts:12:7   C:\x\a.ts:12   --> src/main.rs:4:9   a.xml:118: element …
        Regex::new(&format!(r"({PATH}):(\d+)(?::(\d+))?")).expect("shape"),
        // src/a.ts(12,7): error TS2322 (tsc, MSVC)
        Regex::new(&format!(r"({PATH})\((\d+)(?:,(\d+))?\)")).expect("shape"),
        // File "scripts/report.py", line 41 (Python)
        Regex::new(r#"File "([^"]+)", line (\d+)"#).expect("shape"),
        // In scripts/deploy.sh line 12: (ShellCheck)
        Regex::new(&format!(r"\bIn ({PATH}) line (\d+):")).expect("shape"),
    ]
});
static ALONE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(\S(?:.*\S)?)\s*$").expect("alone"));
static ROW: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s+(\d+):(\d+)\s+\S").expect("row"));
static SEVERITY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^(?:element [^:]+:\s*)?(?:Schemas\s+)?(fatal error|validity error|parser error|error|warning|warn|note|info|hint|style)\b[\s:]*(.*)$",
    )
    .expect("severity")
});
static ELEMENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^element ([^:]+):").expect("element"));
static RULE_HEAD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^((?:TS|SC|E|W|F|C|N|D|B|S|RUF|PL[CERW])\d{3,5})[:\s]+").expect("rule head")
});
static RULE_TAIL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s{2,}(\S+)$").expect("rule"));
static RULE_BRACKET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s\[([A-Za-z0-9_\-/@.]+)\]$").expect("rule"));
static SAID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(error|warning)(?:\[([A-Za-z0-9_:]+)\])?: (.+)$").expect("said")
});
static RUFF_SAID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([A-Z]{1,4}\d{3,4}) (?:\[\*\] )?(\S.*)$").expect("ruff said"));
static CARET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(\^-*\^?)\s+(SC\d{4}) \((error|warning|info|style)\): (.+)$").expect("caret")
});
static ELEMENT_SAID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^Element '([^']+)':\s*(.*)$").expect("element said"));
static URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"https?://[^\s"'<>`{}]+[^\s"'<>`{}.,;:!?)\]]"#).expect("url"));

/// Whether a file exists inside the checkout, asked of the disk once a path.
pub trait Exists {
    fn exists(&mut self, relative: &str) -> bool;
}

/// The disk, with a cache of answers and a cap on what it asks.
pub struct Disk {
    root: PathBuf,
    real_root: Option<PathBuf>,
    answers: HashMap<String, bool>,
    /// A generated folder's files (ADR-0060, amended 2026-11-25): only these
    /// are places, never a file a program added beside them.
    only: Option<std::sync::Arc<std::collections::HashSet<String>>>,
}

impl Disk {
    #[must_use]
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            real_root: std::fs::canonicalize(root).ok(),
            answers: HashMap::new(),
            only: None,
        }
    }

    /// Places only at these paths from the root, which must also exist.
    #[must_use]
    pub fn only(mut self, held: Option<std::sync::Arc<std::collections::HashSet<String>>>) -> Self {
        self.only = held;
        self
    }
}

impl Exists for Disk {
    fn exists(&mut self, relative: &str) -> bool {
        if let Some(known) = self.answers.get(relative) {
            return *known;
        }
        if self.answers.len() >= CANDIDATES {
            return false;
        }
        if self
            .only
            .as_ref()
            .is_some_and(|held| !held.contains(relative))
        {
            self.answers.insert(relative.to_owned(), false);
            return false;
        }
        let answer = self.real_root.as_ref().is_some_and(|root| {
            let joined = relative
                .split('/')
                .fold(self.root.clone(), |path, part| path.join(part));
            std::fs::canonicalize(joined).is_ok_and(|real| real.starts_with(root) && real.is_file())
        });
        self.answers.insert(relative.to_owned(), answer);
        answer
    }
}

/// Where a run's paths are read against.
pub struct Ctx<'a> {
    /// The checkout, with `/`.
    pub root: String,
    /// The root as WSL sees it, where the command was given WSL's paths.
    pub alias: Option<String>,
    /// The working directory from the root, with `/`, `""` for the top.
    pub cwd: String,
    /// Paths compare without regard to case (Windows).
    pub caseless: bool,
    pub disk: &'a mut dyn Exists,
}

/// What carries from one line to the next.
#[derive(Debug, Default)]
pub struct State {
    /// ESLint's file, from a path alone on its line.
    file: Option<String>,
    /// rustc's `error[E0308]: words` or Ruff's `F401 words`, waiting for its
    /// `-->` line.
    said: Option<Finding>,
    /// ShellCheck's `In path line N:`, for the `^-- SC2086 (info): words`
    /// rows under it.
    shellcheck: Option<(String, u32)>,
}

/// A web address in a line: opened in the browser, nothing else.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "app.ts", rename = "OneOffUrl")]
pub struct Url {
    pub from: u32,
    pub to: u32,
    pub href: String,
}

/// `byte`'s offset in `text` in UTF-16 units.
fn u16_at(text: &str, byte: usize) -> u32 {
    u32::try_from(text[..byte].encode_utf16().count()).unwrap_or(u32::MAX)
}

fn norm(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out.join("/")
}

/// No `..` climbs above where the path starts.
fn stays_inside(path: &str) -> bool {
    let mut depth = 0_i32;
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => depth += 1,
        }
    }
    true
}

fn starts_with_dir(path: &str, root: &str, caseless: bool) -> Option<usize> {
    let root = root.trim_end_matches('/');
    let head = path.get(..root.len())?;
    let same = if caseless {
        head.eq_ignore_ascii_case(root)
    } else {
        head == root
    };
    (same && path.as_bytes().get(root.len()) == Some(&b'/')).then_some(root.len() + 1)
}

/// A path a program printed, as a file of the checkout, or `None`.
fn resolve(raw: &str, ctx: &mut Ctx<'_>) -> Option<String> {
    let mut p = raw.replace('\\', "/");
    if let Some(rest) = p.strip_prefix("./") {
        p = rest.to_owned();
    }
    let bytes = p.as_bytes();
    let absolute = p.starts_with('/')
        || (bytes.len() > 2
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && bytes[2] == b'/');
    if absolute {
        let cut = starts_with_dir(&p, &ctx.root, ctx.caseless).or_else(|| {
            ctx.alias
                .as_deref()
                .and_then(|alias| starts_with_dir(&p, alias, ctx.caseless))
        })?;
        p = p[cut..].to_owned();
    } else if !ctx.cwd.is_empty() {
        let joined = format!("{}/{p}", ctx.cwd);
        if stays_inside(&joined) {
            let normal = norm(&joined);
            if !normal.is_empty() && ctx.disk.exists(&normal) {
                return Some(normal);
            }
        }
    }
    if !stays_inside(&p) {
        return None;
    }
    let normal = norm(&p);
    (!normal.is_empty() && ctx.disk.exists(&normal)).then_some(normal)
}

/// The bytes a path in [`PATH`] never holds, which a run's own root may
/// (`Application Support`, `Program Files (x86)`).
const NOT_IN_PATH: &[u8] = b" \t:\"'<>|()[]{}*?";

/// `text` with the run's own root (and its WSL alias) made one word wherever
/// it stands before a separator, so an absolute path under a root whose name
/// holds a space or a bracket still reads as one path: a generated folder kept
/// under the profile's `Application Support` on macOS, a checkout under
/// `Program Files (x86)` (Björn, 2026-11-25). Only ASCII bytes change, one
/// for one, so every offset is the text's own and the path is read from
/// `text`. `None` where nothing changed.
fn masked(text: &str, ctx: &Ctx<'_>) -> Option<String> {
    let fold = |b: u8| {
        if b == b'\\' {
            b'/'
        } else if ctx.caseless {
            b.to_ascii_lowercase()
        } else {
            b
        }
    };
    let seen: Vec<u8> = text.bytes().map(fold).collect();
    let mut bytes = text.as_bytes().to_vec();
    let mut changed = false;
    for root in std::iter::once(ctx.root.as_str()).chain(ctx.alias.as_deref()) {
        let root = root.trim_end_matches('/');
        if root.is_empty() || !root.bytes().any(|b| NOT_IN_PATH.contains(&b)) {
            continue;
        }
        let want: Vec<u8> = root.bytes().map(fold).collect();
        let mut from = 0;
        while let Some(at) = seen
            .get(from..)
            .and_then(|rest| rest.windows(want.len()).position(|w| w == want.as_slice()))
        {
            let start = from + at;
            let end = start + want.len();
            if seen.get(end) == Some(&b'/') {
                for b in &mut bytes[start..end] {
                    if NOT_IN_PATH.contains(b) {
                        *b = b'_';
                        changed = true;
                    }
                }
            }
            from = start + 1;
        }
    }
    if changed {
        String::from_utf8(bytes).ok()
    } else {
        None
    }
}

/// The places in one line, in order, each naming a file of the checkout.
pub fn find(text: &str, ctx: &mut Ctx<'_>, state: &mut State) -> Vec<Place> {
    find_marked(text, ctx, state).0
}

/// [`find`], and whether the line is a row under ESLint's path.
fn find_marked(text: &str, ctx: &mut Ctx<'_>, state: &mut State) -> (Vec<Place>, bool) {
    if text.chars().count() > SEARCHED {
        return (Vec::new(), false);
    }
    let mut found: Vec<(usize, usize, Place, bool)> = Vec::new();
    // The shapes are matched on `hay`, the paths read from `text`: the same
    // bytes but for the root's (see [`masked`]).
    let hay_owned = masked(text, ctx);
    let hay = hay_owned.as_deref().unwrap_or(text);
    for (shape, re) in SHAPES.iter().enumerate() {
        for caps in re.captures_iter(hay) {
            let (Some(whole), Some(path)) = (caps.get(0), caps.get(1)) else {
                continue;
            };
            let start = path.start();
            let mut end = whole.end();
            if shape == 3 && whole.as_str().ends_with(':') {
                end -= 1;
            }
            if found.iter().any(|(a, b, ..)| start < *b && end > *a) {
                continue;
            }
            // `node:internal/…`: a word before the colon that is no file.
            let Some(resolved) = resolve(&text[path.range()], ctx) else {
                continue;
            };
            let number = |i: usize| caps.get(i).and_then(|m| m.as_str().parse::<u32>().ok());
            if shape == 3 {
                state.shellcheck = number(2).map(|line| (resolved.clone(), line));
            }
            found.push((
                start,
                end,
                Place {
                    from: u16_at(text, start),
                    to: u16_at(text, end),
                    path: resolved,
                    line: number(2),
                    col: number(3),
                },
                false,
            ));
        }
    }
    if found.is_empty()
        && let Some(caps) = ALONE.captures(text)
        && let Some(alone) = caps.get(1)
        && !alone.as_str().contains("  ")
        && let Some(resolved) = resolve(alone.as_str(), ctx)
    {
        state.file = Some(resolved.clone());
        let place = Place {
            from: u16_at(text, alone.start()),
            to: u16_at(text, alone.end()),
            path: resolved,
            line: None,
            col: None,
        };
        return (vec![place], false);
    }
    if found.is_empty()
        && let Some(file) = &state.file
        && let Some(caps) = ROW.captures(text)
    {
        let (line, col) = (caps.get(1).expect("row"), caps.get(2).expect("row"));
        found.push((
            line.start(),
            col.end(),
            Place {
                from: u16_at(text, line.start()),
                to: u16_at(text, col.end()),
                path: file.clone(),
                line: line.as_str().parse().ok(),
                col: col.as_str().parse().ok(),
            },
            true,
        ));
    }
    found.sort_by_key(|(start, ..)| *start);
    let row = found.first().is_some_and(|(.., row)| *row);
    (
        found.into_iter().map(|(_, _, place, _)| place).collect(),
        row,
    )
}

/// The web addresses in a line.
#[must_use]
pub fn urls(text: &str) -> Vec<Url> {
    if text.len() > SEARCHED * 4 {
        return Vec::new();
    }
    URL.find_iter(text)
        .map(|m| Url {
            from: u16_at(text, m.start()),
            to: u16_at(text, m.end()),
            href: m.as_str().to_owned(),
        })
        .collect()
}

/// rustc and clippy say what is wrong on one line and where on the next: the
/// first is kept for the second.
pub fn note_said(text: &str, state: &mut State) {
    if let Some(caps) = SAID.captures(text) {
        state.said = Some(Finding {
            severity: caps[1].to_owned(),
            said: caps[3].to_owned(),
            rule: caps
                .get(2)
                .map_or_else(String::new, |m| m.as_str().to_owned()),
        });
    } else if let Some(caps) = RUFF_SAID.captures(text) {
        state.said = Some(Finding {
            severity: String::new(),
            said: caps[2].trim().to_owned(),
            rule: caps[1].to_owned(),
        });
    }
}

/// ShellCheck's `^-- SC2086 (info): words` under an `In path line N:`: a
/// place at the carets and its finding.
fn shellcheck_row(text: &str, state: &State) -> Option<(Place, Finding)> {
    let (path, line) = state.shellcheck.clone()?;
    let caps = CARET.captures(text)?;
    let carets = caps.get(1)?;
    let severity = match &caps[3] {
        "error" => "error",
        "warning" => "warning",
        _ => "note",
    };
    Some((
        Place {
            from: u16_at(text, carets.start()),
            to: u16_at(text, carets.end()),
            path,
            line: Some(line),
            col: u32::try_from(text[..carets.start()].chars().count() + 1).ok(),
        },
        Finding {
            severity: severity.to_owned(),
            said: caps[4].trim().to_owned(),
            rule: caps[2].to_owned(),
        },
    ))
}

/// The byte offset in `text` of UTF-16 offset `at`.
fn byte_at(text: &str, at: u32) -> usize {
    let mut units = 0_u32;
    for (byte, ch) in text.char_indices() {
        if units >= at {
            return byte;
        }
        units += u32::try_from(ch.len_utf16()).unwrap_or(1);
    }
    text.len()
}

/// The finding `place` starts in `text`, or `None` where the line only names
/// a place.
pub fn finding_of(text: &str, place: &Place, row: bool, state: &mut State) -> Option<Finding> {
    let end = byte_at(text, place.to);
    let rest = &text[end..];
    let after = if row {
        rest.trim().to_owned()
    } else if rest.starts_with(':') && rest[1..].trim_start().chars().next().is_some() {
        rest.trim_start_matches(|c: char| c == ':' || c.is_whitespace())
            .to_owned()
    } else if state.said.is_some()
        && text[..byte_at(text, place.from)]
            .trim_end()
            .ends_with("-->")
    {
        return state.said.take();
    } else {
        return None;
    };
    let (severity, said) = match SEVERITY.captures(&after) {
        Some(caps) => (caps[1].to_ascii_lowercase(), caps[2].trim().to_owned()),
        None => (String::new(), after.trim().to_owned()),
    };
    let severity = if severity.contains("error") {
        "error"
    } else if severity.starts_with("warn") {
        "warning"
    } else if severity.is_empty() {
        ""
    } else {
        "note"
    };
    let mut said = said;
    let mut rule = String::new();
    if let Some(caps) = RULE_HEAD.captures(&said) {
        rule = caps[1].to_owned();
        said = said[caps[0].len()..].trim().to_owned();
    } else if let Some(caps) = RULE_TAIL
        .captures(&said)
        .or_else(|| RULE_BRACKET.captures(&said))
    {
        rule = caps[1].to_owned();
        said = said[..caps.get(0).expect("whole").start()]
            .trim()
            .to_owned();
    }
    if rule.is_empty()
        && let Some(caps) = ELEMENT.captures(&after)
    {
        rule = caps[1].to_owned();
    }
    if rule.is_empty()
        && let Some(caps) = ELEMENT_SAID.captures(&said)
    {
        rule = caps[1].to_owned();
        said = caps[2].trim().to_owned();
    }
    Some(Finding {
        severity: severity.to_owned(),
        said,
        rule,
    })
}

/// One ended line's places, its finding and its addresses.
pub fn read_line(
    text: &str,
    ctx: &mut Ctx<'_>,
    state: &mut State,
) -> (Vec<Place>, Option<Finding>, Vec<Url>) {
    if let Some((place, finding)) = shellcheck_row(text, state) {
        return (vec![place], Some(finding), urls(text));
    }
    note_said(text, state);
    let (places, row) = find_marked(text, ctx, state);
    let finding = places
        .first()
        .filter(|p| p.line.is_some())
        .and_then(|place| finding_of(text, place, row, state));
    (places, finding, urls(text))
}

#[cfg(test)]
mod tests;
