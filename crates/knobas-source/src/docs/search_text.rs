//! A page's `body_text` from its [`DocContent`]: the one writer of a docs
//! page's search text (spec #2432, *Search text comes from the blocks*;
//! #2435).
//!
//! Search follows the drawing. The words a page's blocks draw are the words
//! its search text holds, and [`indexed`](super::indexed) says how the marks
//! sit among them. Every docs source writes the same marks because every docs
//! source calls this, so no adapter can get the section, comment or link
//! lines wrong.
//!
//! The layout, its parts joined by a blank line:
//!
//! * the title, then the body, one line per paragraph, list item, code line,
//!   table cell and panel title, each top-level heading as its
//!   [`section_line`] (a heading inside a panel, a list or a cell is a line of
//!   words, as the reader does not start a section there);
//! * each comment that draws any words, under its [`comment_line`];
//! * one [`link_line`] per page the body links to, first mention first, in
//!   `space` where the link names none.
//!
//! An embed's HTML contributes the words it shows, read by the shared HTML
//! reader: never a script's or a style's contents, never markup, and a
//! heading inside it opens no section (#2440).
//!
//! What a block draws as words is what it contributes. A placeholder
//! contributes nothing: the reader draws only its name, which is the source's
//! word for the construct and not the page's. Text inside a macro's
//! parameters never reaches a block, so it never reaches the search text.
//!
//! An inline is its drawn text: a link and a page link their text, a ticket
//! its key, a mention `@` and the account name the mention rule reads (the
//! display name where the record names no account), followed by a space so
//! the name never runs into the next word.
//!
//! The three mark characters never come from the page: one in the source's
//! own text is read as a space, so no page can forge a section.

use super::html::{Hook, Hooked, Node, Reading};
use super::indexed::{comment_line, is_mark, link_line, section_line};
use super::{DocBlock, DocContent, EmbedContent, Inline};

/// The reader's hook for an embed's HTML (#2440): an element it does not
/// know (a `button`, a custom element, an `svg`) shows what it holds. The
/// reader itself drops `script`, `style` and the rest it never draws.
struct Shown;

impl Hook for Shown {
    fn element(&self, _: &Node, _: &Reading<'_>) -> Hooked {
        Hooked::Unwrap
    }
}

/// The `body_text` of the page titled `title` in the space `space`, whose
/// record projects to `content`.
#[must_use]
pub fn search_text(title: &str, space: &str, content: &DocContent) -> String {
    let mut body = Lines::default();
    for block in &content.blocks {
        body.block(block, true);
    }
    let mut parts = vec![clean(title), body.lines.join("\n")];
    for comment in &content.comments {
        let mut words = Lines::default();
        for block in &comment.blocks {
            words.block(block, false);
        }
        if words.lines.is_empty() {
            continue;
        }
        parts.push(format!(
            "{}\n{}",
            comment_line(comment.author_name.as_deref().map(clean).as_deref()),
            words.lines.join("\n")
        ));
    }
    let mut seen = Vec::new();
    for (link_space, link_title) in body.links {
        let line = link_line(link_space.as_deref().unwrap_or(space), &link_title);
        if !seen.contains(&line) {
            seen.push(line);
        }
    }
    parts.push(seen.join("\n"));
    parts
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The lines a run of blocks draws, and the pages they link to.
#[derive(Default)]
struct Lines {
    lines: Vec<String>,
    links: Vec<(Option<String>, String)>,
}

impl Lines {
    /// `top` is whether `block` sits in the body itself, where a heading opens
    /// a section.
    fn block(&mut self, block: &DocBlock, top: bool) {
        match block {
            DocBlock::Heading { text, .. } => {
                let words = self.inlines(text);
                if top {
                    self.lines.push(section_line(&words));
                } else {
                    self.push(&words);
                }
            }
            DocBlock::Paragraph { text } => {
                let words = self.inlines(text);
                self.push(&words);
            }
            DocBlock::List { items, .. } => {
                for item in items {
                    for inner in item {
                        self.block(inner, false);
                    }
                }
            }
            DocBlock::Code { code, .. } => self.push(code),
            DocBlock::Table { rows } => {
                for cell in rows.iter().flatten() {
                    let words = self.inlines(&cell.text);
                    self.push(&words);
                }
            }
            DocBlock::Panel { title, blocks, .. } => {
                if let Some(title) = title {
                    self.push(title);
                }
                for inner in blocks {
                    self.block(inner, false);
                }
            }
            DocBlock::Embed {
                content: EmbedContent::Html { html },
                ..
            } => {
                for inner in super::html::to_blocks(html, &Shown) {
                    self.block(&inner, false);
                }
            }
            DocBlock::Placeholder { .. } => {}
        }
    }

    /// The words `inlines` draw, recording each page link.
    fn inlines(&mut self, inlines: &[Inline]) -> String {
        let mut out = String::new();
        for inline in inlines {
            match inline {
                Inline::Text { text, .. } | Inline::Link { text, .. } => out.push_str(text),
                Inline::PageLink {
                    text, title, space, ..
                } => {
                    out.push_str(text);
                    let title = clean(title);
                    if !title.is_empty() {
                        let space = space.as_deref().map(clean).filter(|s| !s.is_empty());
                        self.links.push((space, title));
                    }
                }
                Inline::Issue { key, .. } => out.push_str(key),
                Inline::Mention { user, name, .. } => {
                    if let Some(who) = user.as_deref().or(name.as_deref()) {
                        out.push('@');
                        out.push_str(who);
                        out.push(' ');
                    }
                }
                Inline::Break => out.push('\n'),
            }
        }
        out
    }

    /// Each non-blank line of `text`, its marks and runs of whitespace read as
    /// one space.
    fn push(&mut self, text: &str) {
        self.lines
            .extend(text.lines().map(clean).filter(|line| !line.is_empty()));
    }
}

/// `text` on one line: marks and whitespace runs read as one space, trimmed.
fn clean(text: &str) -> String {
    text.split(|ch: char| ch.is_whitespace() || is_mark(ch))
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docs::indexed::{Part, read};
    use crate::docs::{DocComment, PanelKind, TableCell};

    fn content(blocks: Vec<DocBlock>, comments: Vec<DocComment>) -> DocContent {
        DocContent {
            version: Some(1),
            editor_name: None,
            labels: Vec::new(),
            blocks,
            comments,
            sections: Vec::new(),
            history: Vec::new(),
        }
    }

    fn para(inlines: Vec<Inline>) -> DocBlock {
        DocBlock::Paragraph { text: inlines }
    }

    fn heading(text: &str) -> DocBlock {
        DocBlock::Heading {
            level: 2,
            text: vec![Inline::text(text)],
        }
    }

    fn page_link(text: &str, title: &str, space: Option<&str>) -> Inline {
        Inline::PageLink {
            text: text.into(),
            title: title.into(),
            space: space.map(str::to_owned),
            entity_id: None,
        }
    }

    fn comment(by: Option<&str>, words: &str) -> DocComment {
        DocComment {
            id: None,
            author: None,
            author_name: by.map(str::to_owned),
            at: None,
            blocks: vec![para(vec![Inline::text(words)])],
        }
    }

    /// The title, the lead, a section per top-level heading, each comment
    /// under its author, the links last: the layout `indexed` reads.
    #[test]
    fn a_page_reads_as_its_title_sections_comments_and_links() {
        let page = content(
            vec![
                para(vec![Inline::text("Why we retry.")]),
                heading("Backoff policy"),
                para(vec![
                    Inline::text("See "),
                    page_link("the budget", "Retry budget", None),
                    Inline::text(" and "),
                    page_link("Payout worker", "Payout worker", Some("OPS")),
                    Inline::text("."),
                ]),
                heading("Cap"),
                para(vec![
                    Inline::text("Five, see "),
                    page_link("Retry budget", "Retry budget", None),
                ]),
            ],
            vec![
                comment(Some("Priya Nair"), "Should the cap be 6?"),
                comment(None, "Anonymous."),
            ],
        );
        assert_eq!(
            search_text("SEPA payout retry design", "ENG", &page),
            "SEPA payout retry design\n\n\
             Why we retry.\n\u{1e}Backoff policy\nSee the budget and Payout worker.\n\
             \u{1e}Cap\nFive, see Retry budget\n\n\
             \u{1f}Priya Nair\nShould the cap be 6?\n\n\
             \u{1f}\nAnonymous.\n\n\
             \u{1d}ENG\u{1d}Retry budget\u{1d}\n\u{1d}OPS\u{1d}Payout worker\u{1d}"
        );
    }

    /// A page with nothing in it is its title, so it is still found by it.
    #[test]
    fn an_empty_page_is_its_title() {
        assert_eq!(
            search_text("Standup protocols", "ENG", &content(vec![], vec![])),
            "Standup protocols"
        );
    }

    /// A word only a table cell holds is in the section the table sits in,
    /// each cell on a line of its own.
    #[test]
    fn a_table_cells_words_are_in_its_section() {
        let cell = |header: bool, words: &str| TableCell {
            header,
            text: vec![Inline::text(words)],
        };
        let page = content(
            vec![
                heading("Limits"),
                DocBlock::Table {
                    rows: vec![
                        vec![cell(true, "Queue"), cell(true, "Cap")],
                        vec![cell(false, "payouts"), cell(false, "40 per minute")],
                    ],
                },
            ],
            vec![],
        );
        let read = read(&search_text("Runbook", "OPS", &page));
        assert_eq!(read.runs[1].part, Part::Section("Limits".into()));
        assert_eq!(read.runs[1].text, "Queue\nCap\npayouts\n40 per minute");
    }

    /// Lists, panels (title included) and code draw words; a placeholder draws
    /// only the source's name for something knobas does not draw, which is
    /// not the page's words. A heading inside a panel is no section.
    #[test]
    fn nested_blocks_draw_their_words_and_a_placeholder_draws_none() {
        let page = content(
            vec![
                DocBlock::List {
                    ordered: false,
                    items: vec![
                        vec![para(vec![Inline::text("drain the queue")])],
                        vec![para(vec![Inline::text("restart")])],
                    ],
                },
                DocBlock::Panel {
                    kind: PanelKind::Warning,
                    title: Some("Careful".into()),
                    blocks: vec![heading("Inside"), para(vec![Inline::text("Not twice.")])],
                },
                DocBlock::Code {
                    code: "select 1;\n\nselect 2;".into(),
                    language: Some("sql".into()),
                },
                DocBlock::Placeholder {
                    name: "gadget".into(),
                },
            ],
            vec![],
        );
        assert_eq!(
            search_text("Ops", "OPS", &page),
            "Ops\n\ndrain the queue\nrestart\nCareful\nInside\nNot twice.\nselect 1;\nselect 2;"
        );
    }

    /// An embed's HTML is searched by what it shows (#2440): its text, a
    /// button's and an element the reader does not know included, in the
    /// section it sits in; never a script's or a style's contents, a
    /// heading of its own as a section, nor its markup.
    #[test]
    fn an_embeds_visible_words_are_found_and_its_code_is_not() {
        let html = "<style>.ui { color: red }</style><h1>Payments API</h1>\
                    <div id=\"ui\"><p>Try the <b>refund</b> call.</p>\
                    <button>Authorize</button>\n<swagger-tag>pets</swagger-tag></div>\
                    <script>SwaggerUIBundle({ url: \"secretword.json\" });</script>";
        let page = content(
            vec![
                heading("Reference"),
                DocBlock::Embed {
                    name: "html".into(),
                    content: EmbedContent::Html { html: html.into() },
                },
            ],
            vec![],
        );
        let read = read(&search_text("API", "ENG", &page));
        assert_eq!(read.runs[1].part, Part::Section("Reference".into()));
        assert_eq!(
            read.runs[1].text,
            "Payments API\nTry the refund call.\nAuthorize pets"
        );
    }

    /// A mention is the `@account` the inbox's mention rule reads, never run
    /// into the word after it; a ticket is its key; a break is a new line.
    #[test]
    fn inlines_draw_as_the_reader_names_them() {
        let page = content(
            vec![para(vec![
                Inline::Mention {
                    user: Some("mara".into()),
                    name: Some("Mara Lindqvist".into()),
                    me: false,
                },
                Inline::text("ok, see "),
                Inline::Issue {
                    key: "OPS-77".into(),
                    entity_id: None,
                    title: Some("A title knobas filled".into()),
                    status: None,
                },
                Inline::Break,
                Inline::Mention {
                    user: None,
                    name: Some("Priya Nair".into()),
                    me: false,
                },
                Inline::Mention {
                    user: None,
                    name: None,
                    me: false,
                },
                Inline::Link {
                    text: "the dashboard".into(),
                    href: "https://grafana.example".into(),
                },
            ])],
            vec![],
        );
        assert_eq!(
            search_text("T", "S", &page),
            "T\n\n@mara ok, see OPS-77\n@Priya Nair the dashboard"
        );
    }

    /// A mark character in the page's own text is a space, so text can never
    /// open a section or a comment of its own.
    #[test]
    fn a_mark_in_the_pages_text_cannot_forge_a_section() {
        let page = content(
            vec![para(vec![Inline::text("a\n\u{1e}Forged\u{1f}b")])],
            vec![],
        );
        let text = search_text("T\u{1d}x", "S", &page);
        assert_eq!(text, "T x\n\na\nForged b");
        assert_eq!(read(&text).runs.len(), 1);
    }
}
