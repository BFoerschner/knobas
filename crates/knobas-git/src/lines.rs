//! Two texts compared line by line, as git's xdiff compares them: the
//! Myers algorithm with git's heuristics and its indent heuristic
//! (`gix-imara-diff`, which matched `git diff` on every one of 83,855 file
//! pairs of two real histories, `docs/decisions/2026-11-16-a-clone-is-read-in-process.md`),
//! and the hunks written as `xemit.c` writes them.

use std::hash::{Hash, Hasher};
use std::ops::Range;

use gix_imara_diff::{
    Algorithm, Diff, IndentHeuristic, IndentLevel, InternedInput, SliderHeuristic, Token,
};

use crate::walk::is_space;

/// The first bytes git looks at to tell a binary file (`FIRST_FEW_BYTES`).
const FIRST_FEW_BYTES: usize = 8000;

/// git's `buffer_is_binary`: a NUL among the first 8,000 bytes.
pub(crate) fn is_binary(data: &[u8]) -> bool {
    data[..data.len().min(FIRST_FEW_BYTES)].contains(&0)
}

/// `text` in lines, each with its newline; a last line without one is a
/// line too.
pub(crate) fn split(text: &[u8]) -> Vec<&[u8]> {
    text.split_inclusive(|b| *b == b'\n').collect()
}

/// A line compared with every blank, tab, newline and carriage return left
/// out: `-w` (xdiff's `XDF_IGNORE_WHITESPACE`).
#[derive(Clone, Copy, Default)]
struct Unspaced<'a>(&'a [u8]);

impl PartialEq for Unspaced<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.0
            .iter()
            .filter(|b| !is_space(**b))
            .eq(other.0.iter().filter(|b| !is_space(**b)))
    }
}
impl Eq for Unspaced<'_> {}
impl Hash for Unspaced<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for b in self.0.iter().filter(|b| !is_space(**b)) {
            state.write_u8(*b);
        }
        state.write_u8(0xff);
    }
}

/// The indent heuristic asked by place where the lines were compared
/// without their blanks: a line's indentation is its own, whatever line it
/// compared equal to.
struct ByPlace<'a, F> {
    compared_before: &'a [Token],
    exact_before: &'a [Token],
    exact_after: &'a [Token],
    inner: IndentHeuristic<F>,
}

impl<F: Fn(Token) -> IndentLevel> SliderHeuristic for ByPlace<'_, F> {
    fn best_slider_end(&mut self, tokens: &[Token], hunk: Range<u32>, earliest_end: u32) -> u32 {
        let exact = if std::ptr::eq(tokens.as_ptr(), self.compared_before.as_ptr()) {
            self.exact_before
        } else {
            self.exact_after
        };
        self.inner.best_slider_end(exact, hunk, earliest_end)
    }
}

/// One run of changed lines: lines `old` of the old text are replaced by
/// lines `new` of the new one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Change {
    pub old: Range<u32>,
    pub new: Range<u32>,
}

/// The changes from `old` to `new`, placed as git places them (`slide`),
/// or only counted right (`slide` off: a run may sit a few lines from where
/// git shows it, and its length is the same).
pub(crate) fn changes(
    old: &[&[u8]],
    new: &[&[u8]],
    ignore_space: bool,
    slide: bool,
) -> Vec<Change> {
    let diff = if ignore_space {
        let mut compared: InternedInput<Unspaced<'_>> = InternedInput::default();
        compared.update_before(old.iter().map(|l| Unspaced(l)));
        compared.update_after(new.iter().map(|l| Unspaced(l)));
        let mut diff = Diff::compute(Algorithm::Myers, &compared);
        if slide {
            let mut exact: InternedInput<&[u8]> = InternedInput::default();
            exact.update_before(old.iter().copied());
            exact.update_after(new.iter().copied());
            let heuristic = ByPlace {
                compared_before: &compared.before,
                exact_before: &exact.before,
                exact_after: &exact.after,
                inner: IndentHeuristic::new(|token| {
                    IndentLevel::for_ascii_line(exact.interner[token].iter().copied(), 8)
                }),
            };
            diff.postprocess_with(&compared.before, &compared.after, heuristic);
        }
        diff
    } else {
        let mut input: InternedInput<&[u8]> = InternedInput::default();
        input.update_before(old.iter().copied());
        input.update_after(new.iter().copied());
        let mut diff = Diff::compute(Algorithm::Myers, &input);
        if slide {
            diff.postprocess_lines(&input);
        }
        diff
    };
    diff.hunks()
        .map(|h| Change {
            old: h.before,
            new: h.after,
        })
        .collect()
}

/// The lines added and removed, as `--numstat` counts them.
pub(crate) fn counts(changes: &[Change]) -> (u32, u32) {
    changes.iter().fold((0, 0), |(added, removed), c| {
        (added + c.new.len() as u32, removed + c.old.len() as u32)
    })
}

/// git's default function line (`xemit.c`'s `def_ff`): a line that starts
/// with a letter, `_` or `$`, cut at `room` bytes and then stripped of the
/// blanks at its end.
fn function_line(line: &[u8], room: usize) -> Option<&[u8]> {
    let first = *line.first()?;
    if !(first.is_ascii_alphabetic() || first == b'_' || first == b'$') {
        return None;
    }
    let mut len = line.len().min(room);
    while len > 0 && is_space(line[len - 1]) {
        len -= 1;
    }
    Some(&line[..len])
}

/// How much of `text` git prints in a hunk's header (`diff.c`'s
/// `sane_truncate_line`): up to the first byte that does not begin a whole
/// UTF-8 character, so a function line cut at 80 bytes never ends inside
/// one, and a line in another encoding ends where it stops being UTF-8.
/// U+FFFE and U+FFFF are no characters to git.
fn whole_characters(text: &[u8]) -> usize {
    let valid = match std::str::from_utf8(text) {
        Ok(_) => text.len(),
        Err(error) => error.valid_up_to(),
    };
    text[..valid]
        .windows(3)
        .position(|w| w[0] == 0xef && w[1] == 0xbf && (w[2] & 0xfe) == 0xbe)
        .unwrap_or(valid)
}

/// The hunks of `changes` between `old` and `new`, written as `git diff`
/// writes them with `context` lines around each change and changes closer
/// than `2 * context + inter` lines in one hunk: each header with the
/// function line above it, the lines, and the note for a last line with no
/// newline. Context is read from the new text, as git reads it.
pub(crate) fn write_hunks(
    out: &mut Vec<u8>,
    old: &[&[u8]],
    new: &[&[u8]],
    changes: &[Change],
    context: u32,
    inter: u32,
) {
    use std::io::Write as _;
    let (n1, n2) = (old.len() as u32, new.len() as u32);
    let mut function: &[u8] = b"";
    // The line the last search for a function line stopped above.
    let mut searched: i64 = -1;
    let mut at = 0;
    while at < changes.len() {
        let mut last = at;
        while last + 1 < changes.len()
            && changes[last + 1].old.start - changes[last].old.end <= 2 * context + inter
        {
            last += 1;
        }
        let (first, end) = (&changes[at], &changes[last]);
        let s1 = first.old.start.saturating_sub(context);
        let mut s2 = first.new.start.saturating_sub(context);
        let after = context.min(n1 - end.old.end).min(n2 - end.new.end);
        let (e1, e2) = (end.old.end + after, end.new.end + after);
        // The nearest function line above the hunk, down to where the last
        // hunk's search began; none found keeps the last one.
        let mut line = i64::from(s1) - 1;
        while line > searched && line >= 0 {
            if let Some(found) = function_line(old[line as usize], 80) {
                function = found;
                break;
            }
            line -= 1;
        }
        searched = i64::from(s1) - 1;
        let (c1, c2) = (e1 - s1, e2 - s2);
        let mut header = Vec::with_capacity(64);
        let _ = write!(header, "@@ -{}", if c1 == 0 { s1 } else { s1 + 1 });
        if c1 != 1 {
            let _ = write!(header, ",{c1}");
        }
        let _ = write!(header, " +{}", if c2 == 0 { s2 } else { s2 + 1 });
        if c2 != 1 {
            let _ = write!(header, ",{c2}");
        }
        header.extend_from_slice(b" @@");
        if !function.is_empty() {
            header.push(b' ');
            // xdiff's header buffer holds 128 bytes, the newline included.
            let room = 127usize.saturating_sub(header.len());
            let function = &function[..function.len().min(room)];
            header.extend_from_slice(&function[..whole_characters(function)]);
        }
        header.push(b'\n');
        out.extend_from_slice(&header);
        let mut emit = |sign: u8, line: &[u8]| {
            out.push(sign);
            out.extend_from_slice(line);
            if line.last() != Some(&b'\n') {
                out.extend_from_slice(b"\n\\ No newline at end of file\n");
            }
        };
        for change in &changes[at..=last] {
            while s2 < change.new.start {
                emit(b' ', new[s2 as usize]);
                s2 += 1;
            }
            for line in change.old.clone() {
                emit(b'-', old[line as usize]);
            }
            for line in change.new.clone() {
                emit(b'+', new[line as usize]);
            }
            s2 = change.new.end;
        }
        while s2 < e2 {
            emit(b' ', new[s2 as usize]);
            s2 += 1;
        }
        at = last + 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch(old: &str, new: &str, ignore_space: bool) -> String {
        let (a, b) = (split(old.as_bytes()), split(new.as_bytes()));
        let found = changes(&a, &b, ignore_space, true);
        let mut out = Vec::new();
        write_hunks(&mut out, &a, &b, &found, 3, 0);
        String::from_utf8(out).expect("utf-8")
    }

    #[test]
    fn a_hunk_names_the_function_above_it_and_a_missing_newline() {
        let old = "fn one() {\n    a();\n    b();\n    c();\n    d();\n    e();\n}\n";
        let new = "fn one() {\n    a();\n    b();\n    c();\n    d();\n    f();\n}";
        assert_eq!(
            patch(old, new, false),
            "@@ -3,5 +3,5 @@ fn one() {\n     b();\n     c();\n     d();\n-    e();\n-}\n+    f();\n+}\n\\ No newline at end of file\n"
        );
    }

    /// A function line of more than 80 bytes is cut there, then stripped of
    /// the blanks at its end, then of a character the cut split; a line that
    /// is not UTF-8 ends where it stops being so.
    #[test]
    fn a_function_line_is_cut_as_git_cuts_it() {
        let header = |line: &[u8]| {
            let mut old = line.to_vec();
            old.extend_from_slice(b"\n x\n x\n x\n x\n x\n x\n old\n");
            let mut new = line.to_vec();
            new.extend_from_slice(b"\n x\n x\n x\n x\n x\n x\n new\n");
            let (a, b) = (split(&old), split(&new));
            let found = changes(&a, &b, false, true);
            let mut out = Vec::new();
            write_hunks(&mut out, &a, &b, &found, 3, 0);
            let end = out.iter().position(|b| *b == b'\n').expect("a header");
            out[..end].to_vec()
        };
        let named = |n: usize| [b"@@ -5,4 +5,4 @@ ".as_slice(), &vec![b'f'; n]].concat();
        let ell = "…".as_bytes();
        assert_eq!(
            header(&[&vec![b'f'; 77][..], ell, b"tail"].concat()),
            [&named(77)[..], ell].concat()
        );
        assert_eq!(
            header(&[&vec![b'f'; 78][..], ell, b"tail"].concat()),
            named(78)
        );
        assert_eq!(header(&[&vec![b'f'; 81][..]].concat()), named(80));
        assert_eq!(
            header(&[&vec![b'f'; 75][..], b"     more"].concat()),
            named(75)
        );
        // Latin-1: the header ends before the first byte that is not UTF-8.
        assert_eq!(header(b"f\xe9t\xe9"), named(1));
        // The blanks before a split character stay: they were not at the cut.
        assert_eq!(
            header(&[&vec![b'f'; 70][..], b"        ", ell].concat()),
            [&named(70)[..], b"        "].concat()
        );
        assert_eq!(whole_characters("a\u{ffff}b".as_bytes()), 1);
    }

    #[test]
    fn blanks_alone_are_no_change_when_ignored() {
        assert_eq!(patch("a  b\nc\n", "a b\n\tc\n", true), "");
        assert_ne!(patch("a  b\nc\n", "a b\n\tc\n", false), "");
    }

    #[test]
    fn an_added_file_starts_at_line_nought() {
        assert_eq!(patch("", "one\n", false), "@@ -0,0 +1 @@\n+one\n");
        assert_eq!(
            patch("one\ntwo\n", "", false),
            "@@ -1,2 +0,0 @@\n-one\n-two\n"
        );
    }
}
