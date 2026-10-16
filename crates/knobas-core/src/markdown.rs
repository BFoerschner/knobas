//! Blocks to Markdown: the one converter (#832, R-10.17, D10).
//!
//! Spec #808, *Docs*: *"One block-to-Markdown converter serves Export as
//! Markdown and Export notes as Markdown (D10), and later a Markdown docs
//! source."* This module is that converter. Its input is a list of
//! [`DocBlock`]s and its output is GitHub-flavoured Markdown text:
//!
//! * a heading is `#` to `######`;
//! * a list is `-` or `1.`, an item's further blocks indented under it;
//! * code is a fence longer than any run of backticks inside it;
//! * a table is a pipe table, its cells' pipes escaped;
//! * a panel (Confluence's info, note, warning and tip) is a quote that opens
//!   with its kind in bold;
//! * a link to another page is a relative `.md` link, named by
//!   [`page_file_name`], so a folder of exported pages links up;
//! * a macro the vocabulary has no block for is an HTML comment naming it, so
//!   the reader sees where something was without knobas inventing its content;
//! * Markdown that is already Markdown -- a note's body -- goes through as it
//!   is. That is how a note keeps its `[[refs]]` as text.
//!
//! Pure and synchronous: no database, no files. The callers decide where the
//! text goes (`knobas_app::notes_export` writes one file per note).

use serde::{Deserialize, Serialize};

/// One block of a document, as the converter reads it.
///
/// Internally tagged, like the source-view vocabulary, so a TypeScript mirror
/// would be a union on `type`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DocBlock {
    /// A heading, `level` 1 to 6; a level outside that range is clamped.
    Heading { level: u8, text: Vec<Inline> },
    /// A paragraph of running text.
    Paragraph { text: Vec<Inline> },
    /// A bulleted or numbered list. Each item is a list of blocks, so an item
    /// can hold a second paragraph or a nested list.
    List {
        ordered: bool,
        items: Vec<Vec<DocBlock>>,
    },
    /// A code block, with the language the source named, if any.
    Code {
        language: Option<String>,
        text: String,
    },
    /// A table. An empty `header` takes the first row as the header, because
    /// a pipe table cannot be drawn without one.
    Table {
        header: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    /// A panel: Confluence's `info`, `note`, `warning`, `tip`. Drawn as a
    /// quote whose first line names the kind.
    Panel {
        kind: String,
        title: Option<String>,
        blocks: Vec<DocBlock>,
    },
    /// A macro the vocabulary has no block for, kept as an HTML comment.
    Macro {
        name: String,
        summary: Option<String>,
    },
    /// Text that is already Markdown, passed through unchanged.
    Markdown { text: String },
}

/// One run of text inside a block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Inline {
    /// Plain text. Characters Markdown would read as markup are escaped.
    Text {
        text: String,
    },
    Strong {
        text: Vec<Inline>,
    },
    Emphasis {
        text: Vec<Inline>,
    },
    Code {
        text: String,
    },
    /// A link out, to a URL.
    Link {
        text: Vec<Inline>,
        url: String,
    },
    /// A link to another page, by its title: a relative `.md` link.
    PageLink {
        title: String,
    },
}

/// The whole document as Markdown, ending in one newline (or empty).
#[must_use]
pub fn to_markdown(blocks: &[DocBlock]) -> String {
    let body = join_blocks(blocks);
    if body.is_empty() {
        body
    } else {
        format!("{body}\n")
    }
}

/// The file a page with this title is exported as: lower case, letters and
/// digits kept, everything else one `-`, and `.md`. A title with nothing to
/// keep is `untitled.md`.
///
/// One function for both ends of a link: [`Inline::PageLink`] points at the
/// name the exporter writes.
#[must_use]
pub fn page_file_name(title: &str) -> String {
    let mut slug = String::new();
    for ch in title.chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() {
            slug.push(ch);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        "untitled.md".to_owned()
    } else {
        format!("{slug}.md")
    }
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
        DocBlock::Code { language, text } => code(language.as_deref(), text),
        DocBlock::Table { header, rows } => table(header, rows),
        DocBlock::Panel {
            kind,
            title,
            blocks,
        } => panel(kind, title.as_deref(), blocks),
        DocBlock::Macro { name, summary } => {
            let said = match summary {
                Some(summary) if !summary.trim().is_empty() => {
                    format!("macro: {name}: {}", one_line(summary.trim()))
                }
                _ => format!("macro: {name}"),
            };
            // `--` inside a comment ends it early in some readers.
            format!("<!-- {} -->", said.replace("--", "- -"))
        }
        DocBlock::Markdown { text } => text.trim_end().to_owned(),
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

fn code(language: Option<&str>, text: &str) -> String {
    let fence = "`".repeat(longest_run(text, '`').max(2) + 1);
    let language = language.map(str::trim).unwrap_or("");
    let text = text.strip_suffix('\n').unwrap_or(text);
    format!("{fence}{language}\n{text}\n{fence}")
}

fn table(header: &[Vec<Inline>], rows: &[Vec<Vec<Inline>>]) -> String {
    let (header, rows) = if header.is_empty() {
        match rows.split_first() {
            Some((first, rest)) => (first.as_slice(), rest),
            None => return String::new(),
        }
    } else {
        (header, rows)
    };
    let width = rows
        .iter()
        .map(Vec::len)
        .chain(std::iter::once(header.len()))
        .max()
        .unwrap_or(0);
    let line = |cells: &[Vec<Inline>]| {
        let mut out = String::from("|");
        for column in 0..width {
            let cell = cells
                .get(column)
                .map(|cell| inlines(cell))
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

fn panel(kind: &str, title: Option<&str>, blocks: &[DocBlock]) -> String {
    let mut first = String::new();
    let mut chars = kind.trim().chars();
    if let Some(head) = chars.next() {
        first = format!("**{}{}**", head.to_uppercase(), chars.as_str());
    }
    if let Some(title) = title.map(str::trim).filter(|title| !title.is_empty()) {
        if !first.is_empty() {
            first.push(' ');
        }
        first.push_str(&one_line(&escape(title)));
    }
    let body = join_blocks(blocks);
    let text = match (first.is_empty(), body.is_empty()) {
        (true, _) => body,
        (false, true) => first,
        (false, false) => format!("{first}\n\n{body}"),
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

fn inlines(runs: &[Inline]) -> String {
    runs.iter().map(inline).collect()
}

fn inline(run: &Inline) -> String {
    match run {
        Inline::Text { text } => escape(text),
        Inline::Strong { text } => format!("**{}**", inlines(text)),
        Inline::Emphasis { text } => format!("*{}*", inlines(text)),
        Inline::Code { text } => {
            let ticks = "`".repeat(longest_run(text, '`') + 1);
            let pad = if text.starts_with('`') || text.ends_with('`') {
                " "
            } else {
                ""
            };
            format!("{ticks}{pad}{text}{pad}{ticks}")
        }
        Inline::Link { text, url } => {
            let url = if url.contains([' ', '(', ')']) {
                format!("<{url}>")
            } else {
                url.clone()
            };
            format!("[{}]({url})", inlines(text))
        }
        Inline::PageLink { title } => {
            format!("[{}]({})", escape(title), page_file_name(title))
        }
    }
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
mod tests {
    use super::*;

    fn text(text: &str) -> Vec<Inline> {
        vec![Inline::Text {
            text: text.to_owned(),
        }]
    }

    fn para(words: &str) -> DocBlock {
        DocBlock::Paragraph { text: text(words) }
    }

    #[test]
    fn a_heading_is_hashes_by_level_and_clamped() {
        let md = to_markdown(&[
            DocBlock::Heading {
                level: 2,
                text: text("Retry policy"),
            },
            DocBlock::Heading {
                level: 9,
                text: text("Deep"),
            },
            DocBlock::Heading {
                level: 0,
                text: text("Top\nline"),
            },
        ]);
        assert_eq!(md, "## Retry policy\n\n###### Deep\n\n# Top line\n");
    }

    #[test]
    fn a_paragraph_escapes_markup_and_a_block_start() {
        assert_eq!(
            to_markdown(&[para("use *only* [this] <b> a_b")]),
            "use \\*only\\* \\[this\\] \\<b> a\\_b\n"
        );
        assert_eq!(
            to_markdown(&[para("# not a heading")]),
            "\\# not a heading\n"
        );
        assert_eq!(to_markdown(&[para("- not a list")]), "\\- not a list\n");
        assert_eq!(
            to_markdown(&[para("12. not numbered")]),
            "12\\. not numbered\n"
        );
        assert_eq!(to_markdown(&[para("12 apples")]), "12 apples\n");
    }

    #[test]
    fn inline_runs_keep_their_marks() {
        let md = to_markdown(&[DocBlock::Paragraph {
            text: vec![
                Inline::Strong { text: text("bold") },
                Inline::Text {
                    text: " and ".to_owned(),
                },
                Inline::Emphasis { text: text("soft") },
                Inline::Text {
                    text: " and ".to_owned(),
                },
                Inline::Code {
                    text: "a`b".to_owned(),
                },
                Inline::Text {
                    text: " ".to_owned(),
                },
                Inline::Link {
                    text: text("Jira"),
                    url: "https://jira.example/browse/PAY-231".to_owned(),
                },
            ],
        }]);
        assert_eq!(
            md,
            "**bold** and *soft* and ``a`b`` [Jira](https://jira.example/browse/PAY-231)\n"
        );
    }

    #[test]
    fn a_list_is_bulleted_or_numbered_and_nests_under_its_item() {
        let md = to_markdown(&[
            DocBlock::List {
                ordered: false,
                items: vec![
                    vec![para("one")],
                    vec![
                        para("two"),
                        DocBlock::List {
                            ordered: true,
                            items: vec![vec![para("a")], vec![para("b")]],
                        },
                    ],
                ],
            },
            DocBlock::List {
                ordered: true,
                items: vec![vec![para("first")], vec![para("second")]],
            },
        ]);
        assert_eq!(
            md,
            "- one\n- two\n\n  1. a\n  2. b\n\n1. first\n2. second\n"
        );
    }

    #[test]
    fn code_is_fenced_longer_than_any_backtick_run_inside() {
        assert_eq!(
            to_markdown(&[DocBlock::Code {
                language: Some("rust".to_owned()),
                text: "fn main() {}\n".to_owned(),
            }]),
            "```rust\nfn main() {}\n```\n"
        );
        assert_eq!(
            to_markdown(&[DocBlock::Code {
                language: None,
                text: "```\nnested\n```".to_owned(),
            }]),
            "````\n```\nnested\n```\n````\n"
        );
    }

    #[test]
    fn a_table_is_a_pipe_table_with_its_pipes_escaped() {
        let md = to_markdown(&[DocBlock::Table {
            header: vec![text("Service"), text("Owner")],
            rows: vec![
                vec![text("payouts|api"), text("Mara")],
                vec![text("ledger")],
            ],
        }]);
        assert_eq!(
            md,
            "| Service | Owner |\n| --- | --- |\n| payouts\\|api | Mara |\n| ledger |  |\n"
        );
    }

    #[test]
    fn a_table_without_a_header_takes_its_first_row() {
        let md = to_markdown(&[DocBlock::Table {
            header: vec![],
            rows: vec![vec![text("a"), text("b")], vec![text("1"), text("2")]],
        }]);
        assert_eq!(md, "| a | b |\n| --- | --- |\n| 1 | 2 |\n");
        assert_eq!(
            to_markdown(&[DocBlock::Table {
                header: vec![],
                rows: vec![]
            }]),
            ""
        );
    }

    #[test]
    fn a_panel_is_a_quote_that_names_its_kind() {
        let md = to_markdown(&[DocBlock::Panel {
            kind: "warning".to_owned(),
            title: Some("Before you deploy".to_owned()),
            blocks: vec![para("Drain the queue."), para("Then deploy.")],
        }]);
        assert_eq!(
            md,
            "> **Warning** Before you deploy\n>\n> Drain the queue.\n>\n> Then deploy.\n"
        );
    }

    #[test]
    fn a_page_link_is_a_relative_md_link() {
        let md = to_markdown(&[DocBlock::Paragraph {
            text: vec![
                Inline::Text {
                    text: "See ".to_owned(),
                },
                Inline::PageLink {
                    title: "SEPA retry: runbook".to_owned(),
                },
            ],
        }]);
        assert_eq!(md, "See [SEPA retry: runbook](sepa-retry-runbook.md)\n");
    }

    #[test]
    fn a_macro_is_an_html_comment_naming_it() {
        let md = to_markdown(&[
            DocBlock::Macro {
                name: "jira-chart".to_owned(),
                summary: Some("open bugs -- by sprint".to_owned()),
            },
            DocBlock::Macro {
                name: "toc".to_owned(),
                summary: None,
            },
        ]);
        assert_eq!(
            md,
            "<!-- macro: jira-chart: open bugs - - by sprint -->\n\n<!-- macro: toc -->\n"
        );
    }

    #[test]
    fn markdown_goes_through_as_it_is_refs_included() {
        let md = to_markdown(&[DocBlock::Markdown {
            text: "Ask about [[mock:PAY-231]] *today*\n\n".to_owned(),
        }]);
        assert_eq!(md, "Ask about [[mock:PAY-231]] *today*\n");
    }

    #[test]
    fn a_file_name_keeps_letters_and_digits() {
        assert_eq!(
            page_file_name("SEPA retry: runbook"),
            "sepa-retry-runbook.md"
        );
        assert_eq!(page_file_name("  Größe / 2026  "), "größe-2026.md");
        assert_eq!(page_file_name("???"), "untitled.md");
    }
}
