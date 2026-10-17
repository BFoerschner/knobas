//! The Docs frame's blocks as Markdown: the one converter (#832, #952,
//! R-10.17, D10).
//!
//! Spec #808, *Docs*: *"One block-to-Markdown converter serves Export as
//! Markdown and Export notes as Markdown (D10), and later a Markdown docs
//! source."* This module is that converter, and it reads the frame's own
//! [`DocBlock`]s (ADR-0022): one block vocabulary for the reader and the
//! export (#952). Its output is GitHub-flavoured Markdown text:
//!
//! * a heading is `#` to `######`;
//! * a list is `-` or `1.`, an item's further blocks indented under it;
//! * code is a fence longer than any run of backticks inside it;
//! * a table is a pipe table, its first row the header, its cells' pipes
//!   escaped;
//! * a panel is a quote that opens with its kind in bold (the spelling
//!   [`DocSection::text`](super::DocSection::text) edits, #975);
//! * a link to another page is a relative `.md` link, named by
//!   [`page_file_name`], so a folder of exported pages links up;
//! * a ticket chip is its key, a mention `@` and a first name;
//! * a placeholder is an HTML comment naming it, so the reader sees where
//!   something was without knobas inventing its content.
//!
//! A note is a page whose body is already Markdown ([`page_from_markdown`]):
//! the body goes through as typed, so a `[[ref]]` stays text.
//!
//! Pure and synchronous: no database, no files. The callers decide where the
//! text goes.

use super::{DocBlock, Inline, PanelKind, TableCell};

/// A page as Markdown: `title` as a level-1 heading, then `blocks`, ending in
/// one newline.
#[must_use]
pub fn page(title: &str, blocks: &[DocBlock]) -> String {
    document(title, &join_blocks(blocks))
}

/// A page whose body is already Markdown (a note's), passed through as it is
/// but for trailing whitespace. A blank body is the heading alone.
#[must_use]
pub fn page_from_markdown(title: &str, body: &str) -> String {
    document(title, body.trim_end())
}

fn document(title: &str, body: &str) -> String {
    let heading = format!("# {}", one_line(&escape(title)));
    if body.is_empty() {
        format!("{heading}\n")
    } else {
        format!("{heading}\n\n{body}\n")
    }
}

/// The file a page with this title is exported as: lower case, letters and
/// digits kept, everything else one `-`, at most [`FILE_STEM_CHARS`]
/// characters, and `.md`. A title with nothing to keep is `untitled.md`.
///
/// Only letters, digits and `-` survive, so no title can name a path outside
/// the export folder (`/`, `..` and `\` all become `-`). The cap keeps a long
/// title under the 255-byte name limit, which would otherwise fail the whole
/// export. A stem Windows reserves for a device (`con`, `nul`, `com1`, ...)
/// gets `-page`, since `con.md` cannot be written there.
///
/// One function for both ends of a link: [`Inline::PageLink`] points at the
/// name the exporter writes.
#[must_use]
pub fn page_file_name(title: &str) -> String {
    let mut slug = String::new();
    for ch in title.chars().flat_map(char::to_lowercase) {
        if slug.chars().count() >= FILE_STEM_CHARS {
            break;
        }
        if ch.is_alphanumeric() {
            slug.push(ch);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        "untitled.md".to_owned()
    } else if is_windows_device(slug) {
        format!("{slug}-page.md")
    } else {
        format!("{slug}.md")
    }
}

/// The longest stem [`page_file_name`] writes, in characters. Four bytes a
/// character at worst, plus `-99.md`, stays under 255 bytes.
pub const FILE_STEM_CHARS: usize = 60;

/// Whether Windows reserves `stem` for a device, whatever the extension.
fn is_windows_device(stem: &str) -> bool {
    matches!(stem, "con" | "prn" | "aux" | "nul")
        || ["com", "lpt"].iter().any(|prefix| {
            stem.strip_prefix(prefix)
                .is_some_and(|n| n.len() == 1 && n.as_bytes()[0].is_ascii_digit())
        })
}

fn join_blocks(blocks: &[DocBlock]) -> String {
    blocks
        .iter()
        .map(block)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn block(block: &DocBlock) -> String {
    match block {
        DocBlock::Heading { level, text } => {
            let hashes = "#".repeat(usize::from((*level).clamp(1, 6)));
            format!("{hashes} {}", one_line(&inlines(text)))
        }
        DocBlock::Paragraph { text } => guard_line_start(&inlines(text)),
        DocBlock::List { ordered, items } => list(*ordered, items),
        DocBlock::Code { code, language } => fenced(language.as_deref(), code),
        DocBlock::Table { rows } => table(rows),
        DocBlock::Panel {
            kind,
            title,
            blocks,
        } => panel(*kind, title.as_deref(), blocks),
        DocBlock::Placeholder { name } => {
            // `--` inside a comment ends it early in some readers.
            format!("<!-- macro: {} -->", name.replace("--", "- -"))
        }
    }
}

fn list(ordered: bool, items: &[Vec<DocBlock>]) -> String {
    items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let marker = if ordered {
                format!("{}. ", index + 1)
            } else {
                "- ".to_owned()
            };
            let pad = " ".repeat(marker.len());
            let body = join_blocks(item);
            let mut lines = body.lines();
            let mut out = format!("{marker}{}", lines.next().unwrap_or(""));
            for line in lines {
                out.push('\n');
                if !line.is_empty() {
                    out.push_str(&pad);
                    out.push_str(line);
                }
            }
            out
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn fenced(language: Option<&str>, code: &str) -> String {
    let fence = "`".repeat(longest_run(code, '`').max(2) + 1);
    let language = language.map(str::trim).unwrap_or("");
    let code = code.strip_suffix('\n').unwrap_or(code);
    format!("{fence}{language}\n{code}\n{fence}")
}

/// A pipe table cannot be drawn without a header, so the first row is it,
/// whether or not the source marked its cells as header cells.
fn table(rows: &[Vec<TableCell>]) -> String {
    let Some((header, rows)) = rows.split_first() else {
        return String::new();
    };
    let width = rows
        .iter()
        .map(Vec::len)
        .chain(std::iter::once(header.len()))
        .max()
        .unwrap_or(0);
    let line = |cells: &[TableCell]| {
        let mut out = String::from("|");
        for column in 0..width {
            let cell = cells
                .get(column)
                .map(|cell| inlines(&cell.text))
                .unwrap_or_default();
            out.push(' ');
            out.push_str(&table_cell(&cell));
            out.push_str(" |");
        }
        out
    };
    let mut out = vec![line(header), format!("|{}", " --- |".repeat(width))];
    out.extend(rows.iter().map(|row| line(row)));
    out.join("\n")
}

fn table_cell(text: &str) -> String {
    text.trim()
        .replace('|', "\\|")
        .lines()
        .collect::<Vec<_>>()
        .join("<br>")
}

fn panel(kind: PanelKind, title: Option<&str>, blocks: &[DocBlock]) -> String {
    let mut first = format!("**{}**", panel_word(kind));
    if let Some(title) = title.map(str::trim).filter(|title| !title.is_empty()) {
        first.push(' ');
        first.push_str(&one_line(&escape(title)));
    }
    let body = join_blocks(blocks);
    let text = if body.is_empty() {
        first
    } else {
        format!("{first}\n\n{body}")
    };
    text.lines()
        .map(|line| {
            if line.is_empty() {
                ">".to_owned()
            } else {
                format!("> {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The word a panel's quote opens with.
fn panel_word(kind: PanelKind) -> &'static str {
    match kind {
        PanelKind::Info => "Info",
        PanelKind::Note => "Note",
        PanelKind::Warning => "Warning",
        PanelKind::Tip => "Tip",
    }
}

fn inlines(runs: &[Inline]) -> String {
    runs.iter().map(inline).collect()
}

fn inline(run: &Inline) -> String {
    match run {
        Inline::Text {
            text,
            strong,
            emphasis,
            code,
        } => {
            if *code {
                return code_span(text);
            }
            let mut out = escape(text);
            if *emphasis {
                out = format!("*{out}*");
            }
            if *strong {
                out = format!("**{out}**");
            }
            out
        }
        Inline::Link { text, href } => {
            let href = if href.contains([' ', '(', ')']) {
                format!("<{href}>")
            } else {
                href.clone()
            };
            format!("[{}]({href})", escape(text))
        }
        Inline::PageLink { title, .. } => {
            format!("[{}]({})", escape(title), page_file_name(title))
        }
        Inline::Issue { key, .. } => escape(key),
        Inline::Mention { user, name, .. } => {
            let first = name
                .as_deref()
                .and_then(|n| n.split_whitespace().next())
                .or(user.as_deref())
                .unwrap_or("someone");
            escape(&format!("@{first}"))
        }
        Inline::Break => " ".to_owned(),
    }
}

fn code_span(text: &str) -> String {
    let ticks = "`".repeat(longest_run(text, '`') + 1);
    let pad = if text.starts_with('`') || text.ends_with('`') {
        " "
    } else {
        ""
    };
    format!("{ticks}{pad}{text}{pad}{ticks}")
}

/// Escape what Markdown reads as markup inside running text.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if matches!(ch, '\\' | '`' | '*' | '_' | '[' | ']' | '<') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// A paragraph that starts like a heading, a quote, a list item or a numbered
/// item would be read as one; escape its first character.
fn guard_line_start(text: &str) -> String {
    if text.starts_with(['#', '>', '-', '+']) {
        return format!("\\{text}");
    }
    let digits = text.len()
        - text
            .trim_start_matches(|ch: char| ch.is_ascii_digit())
            .len();
    if digits > 0 && text[digits..].starts_with(". ") {
        return format!("{}\\{}", &text[..digits], &text[digits..]);
    }
    text.to_owned()
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn longest_run(text: &str, of: char) -> usize {
    let (mut best, mut run) = (0, 0);
    for ch in text.chars() {
        run = if ch == of { run + 1 } else { 0 };
        best = best.max(run);
    }
    best
}

#[cfg(test)]
mod tests;
