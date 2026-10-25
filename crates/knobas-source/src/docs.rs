//! The Docs frame's vocabulary: what a docs source declares so the shell can
//! draw its pages without knowing which source it is (ADR-0022, issue #859).
//!
//! ADR-0022 gives the shell the frame of the Docs view and has an adapter fill
//! it. The frame is drawn over spaces, a page tree, a page body as a block
//! list, comments, versions and labels. Three of those are already declared
//! paths on the `page` kind ([`KindPaths`](crate::KindPaths)): the space is the
//! project (`project_key`, `project_name`, ADR-0010), and the tree is the
//! declared parent (`parent`, ADR-0018). The rest -- the body, the comments,
//! the version and the labels -- live in the source's own markup, which only
//! the adapter can read (ADR-0007), so the adapter answers them as a
//! [`DocContent`] projected from the record it mirrored.
//!
//! **A projection, not a request.** [`DocProjection`] takes the mirrored
//! payload and nothing else: no network, no credential. The reader opens a page
//! from the mirror, offline, and a smarter projection re-reads every page the
//! mirror holds without a re-sync.
//!
//! **Frozen once landed.** Adding a block, an inline or a field is a §10.8
//! entry, as adding a [`view::Block`](crate::view::Block) is (ADR-0020).
//!
//! Every string is **source text**: the shell interpolates it and never renders
//! it as markup.

use serde::{Deserialize, Serialize};

pub mod markdown;

/// The entity kind the Docs frame draws: knobas' own word for a page since
/// migration `0001`. A docs source emits its pages as this kind.
pub const PAGE_KIND: &str = "page";

/// An adapter's projection of one mirrored record into what the reader draws:
/// `None` for a kind it does not draw as a page, or a payload of a shape it
/// does not know.
///
/// A plain function over the payload, so the app calls it without building a
/// source (no secret, no network).
pub type DocProjection = fn(kind: &str, payload: &serde_json::Value) -> Option<DocContent>;

/// What a page's own record says beyond the normalized fields: its body, its
/// comments, its version and its labels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocContent {
    /// The version number the source gives this revision, or `None` where the
    /// record names none.
    pub version: Option<u64>,
    /// The display name of who made this revision, where the record carries
    /// one; the account name is the normalized `author`.
    pub editor_name: Option<String>,
    /// The page's labels, in the source's order.
    pub labels: Vec<String>,
    /// The page body.
    pub blocks: Vec<DocBlock>,
    /// The page's comments, oldest first.
    pub comments: Vec<DocComment>,
    /// The page's sections in the order of its top-level headings, numbered
    /// from 1 as the reader numbers them, each with whether knobas edits it
    /// (#862, R-10.13).
    #[serde(default)]
    pub sections: Vec<DocSection>,
    /// The versions the record names, newest first: at least the one it is
    /// (R-10.15's *History*). Comparing them is out of scope.
    #[serde(default)]
    pub history: Vec<DocVersion>,
}

/// One section of a page body: a top-level heading and what follows it up to
/// the next one (#862).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct DocSection {
    /// 1 for the first top-level heading.
    pub number: u32,
    /// The section's body as the plain text the editor opens with (`- ` for
    /// a list item, a blank line between paragraphs, and a panel as a quote
    /// whose first line is its kind in bold, `> **Info** <title>`, #975),
    /// `None` when it is refused.
    pub text: Option<String>,
    /// Why knobas does not edit it, `None` when it does.
    pub refusal: Option<SectionRefusal>,
}

/// Why a section is edited in the source and not in knobas (R-10.13): a text
/// box round-trips plain text, and what it cannot hold would be lost.
///
/// - `table`: A table.
/// - `code`: A code block.
/// - `macro`: A macro, a layout, a mention, a page or ticket link: markup of
///   the source's own that text cannot carry. An info, note, warning or tip
///   panel is edited (#975) unless it carries more than a title.
/// - `formatting`: Bold, italics, links, a nested or a numbered list, which the
///   text box would flatten.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "entity.ts")]
pub enum SectionRefusal {
    Table,
    Code,
    Macro,
    Formatting,
}

/// One version of a page, as the record names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct DocVersion {
    pub number: Option<u64>,
    /// The account that made it.
    pub by: Option<String>,
    pub by_name: Option<String>,
    /// When, RFC 3339, as the source wrote it.
    pub at: Option<String>,
    /// The version's message, where the editor left one.
    pub message: Option<String>,
}

/// A section edit, made: the whole new body in the source's markup and the
/// version it was made against (`WriteOp::UpdatePage`'s fields).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionEdit {
    pub body: String,
    pub base_version: i64,
}

/// Why a section edit was not made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditRefused {
    /// The section is one knobas does not edit.
    Section(SectionRefusal),
    /// The record has no such section, no body or no version.
    Missing,
}

/// A docs source's writes in its own markup (#862, ADR-0022's `create_page`
/// and `update_page` by section): plain functions, as [`DocProjection`] is.
#[derive(Debug, Clone, Copy)]
pub struct DocWrites {
    /// Replace section `number`'s body of the mirrored record with `text`
    /// (the editor's plain text, in [`DocSection::text`]'s spelling, panels
    /// included), keeping everything else byte for byte.
    pub edit_section: fn(
        kind: &str,
        payload: &serde_json::Value,
        number: u32,
        text: &str,
    ) -> Result<SectionEdit, EditRefused>,
    /// A new page's body from plain text: `## ` starts a section, `- ` a list
    /// item, a blank line a paragraph.
    pub body_from_text: fn(text: &str) -> String,
}

/// One comment on a page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct DocComment {
    /// The source's id for the comment, where it names one: Confluence's
    /// content id, the id its comment write answered (#2074). Docs drops a
    /// queued comment by it once the read holds it, whatever the words say by
    /// then. `None` from a source whose comments carry no id.
    #[serde(default)]
    pub id: Option<String>,
    /// The author's account name, the spelling `@me` matches. `None` where the
    /// record names nobody.
    pub author: Option<String>,
    /// The author's display name, where the record carries one.
    pub author_name: Option<String>,
    /// When it was written, RFC 3339, as the source wrote it.
    pub at: Option<String>,
    pub blocks: Vec<DocBlock>,
}

/// One block of a page body. The fixed set R-10.11 names, and nothing else.
///
/// Internally tagged (`{"type": "heading", ...}`) so the TypeScript mirror is
/// a discriminated union on `type`.
///
/// - `heading`: A section heading. `level` is 1 to 6 as the source wrote it;
///   the reader draws every level as a section head.
/// - `list`: A list. A nested list's items are drawn as a list inside their
///   item.
/// - `code`: A code block, verbatim, with the language the source named.
/// - `table`: A table, row by row; a header cell says so.
/// - `panel`: An info, note, warning or tip panel, with the blocks inside it.
/// - `placeholder`: Something the source has that the reader does not draw: the
///   source's own name for it (a Confluence macro's `ac:name`). Never dropped,
///   so a page with a part knobas cannot draw says so (story 134).
///
/// Source text, never markup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export_to = "entity.ts")]
pub enum DocBlock {
    Heading {
        level: u8,
        text: Vec<Inline>,
    },
    Paragraph {
        text: Vec<Inline>,
    },
    List {
        ordered: bool,
        items: Vec<Vec<DocBlock>>,
    },
    Code {
        code: String,
        language: Option<String>,
    },
    Table {
        rows: Vec<Vec<TableCell>>,
    },
    Panel {
        kind: PanelKind,
        title: Option<String>,
        blocks: Vec<DocBlock>,
    },
    Placeholder {
        name: String,
    },
}

/// One cell of a [`DocBlock::Table`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct TableCell {
    pub header: bool,
    pub text: Vec<Inline>,
}

/// Which panel a [`DocBlock::Panel`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "entity.ts")]
pub enum PanelKind {
    Info,
    Note,
    Warning,
    Tip,
}

/// One run of text inside a block.
///
/// - `text`: Words, with the marks the source put on them.
/// - `link`: A link out of the docs source, to a web address.
/// - `page_link`: A link to another page of the docs source, by its title and
///   space. `entity_id` is the mirrored page it names, filled by knobas when
///   the reader is read; the adapter leaves it `None`.
/// - `issue`: A ticket reference (a Confluence Jira-issue macro), drawn as a
///   live chip. The adapter fills `key`; knobas fills the rest from the mirror
///   when the reader is read, and leaves them `None` for a ticket it does not
///   hold.
/// - `mention`: A mention of a person. `user` is the account name where the
///   source says it (the spelling `@me` matches), `name` the display name where
///   it is known. `me` is filled by knobas when the reader is read.
/// - `break`: A hard line break inside a block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export_to = "entity.ts", rename = "DocInline")]
pub enum Inline {
    Text {
        text: String,
        #[serde(default)]
        strong: bool,
        #[serde(default)]
        emphasis: bool,
        #[serde(default)]
        code: bool,
    },
    Link {
        text: String,
        href: String,
    },
    PageLink {
        text: String,
        title: String,
        space: Option<String>,
        #[serde(default)]
        entity_id: Option<String>,
    },
    Issue {
        key: String,
        #[serde(default)]
        entity_id: Option<String>,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        status: Option<String>,
    },
    Mention {
        user: Option<String>,
        name: Option<String>,
        #[serde(default)]
        me: bool,
    },
    Break,
}

impl Inline {
    /// Plain text with no marks.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Inline::Text {
            text: text.into(),
            strong: false,
            emphasis: false,
            code: false,
        }
    }
}

/// Every inline in `blocks`, depth first, for knobas' fill-in pass.
pub fn inlines_mut(blocks: &mut [DocBlock]) -> Vec<&mut Inline> {
    let mut out = Vec::new();
    for block in blocks {
        collect(block, &mut out);
    }
    out
}

fn collect<'a>(block: &'a mut DocBlock, out: &mut Vec<&'a mut Inline>) {
    match block {
        DocBlock::Heading { text, .. } | DocBlock::Paragraph { text } => {
            out.extend(text.iter_mut())
        }
        DocBlock::List { items, .. } => {
            for item in items {
                for inner in item {
                    collect(inner, out);
                }
            }
        }
        DocBlock::Table { rows } => {
            for cell in rows.iter_mut().flatten() {
                out.extend(cell.text.iter_mut());
            }
        }
        DocBlock::Panel { blocks, .. } => {
            for inner in blocks {
                collect(inner, out);
            }
        }
        DocBlock::Code { .. } | DocBlock::Placeholder { .. } => {}
    }
}

/// What a docs source writes into a page's `body_text` beside the words, at
/// sync (#860, #861; spec #808, *Docs*): the section each run of text sits
/// in, who wrote each comment, and the pages the body links to.
///
/// `body_text` is what full-text search indexes, and the Docs search needs
/// two things the words alone lose: the section a snippet comes from ("Backoff
/// policy: …", "Comment by Priya: …") and, for *Pages that link here*, which
/// pages a page links to. Re-projecting every hit's markup per keystroke would
/// put the adapter's parse on the search's hot path, so the adapter writes
/// both once, when it syncs, as **lines that open with a control character**.
/// The characters are not words: `to_tsvector` and `ts_headline` read them as
/// separators, so the index holds what it held before, plus the titles of the
/// linked pages.
///
/// The lines, in the order a source writes them:
///
/// * the page's own words first, its title and any text before the first
///   heading (no mark);
/// * [`SECTION`] and a heading: the text below it, up to the next mark, is
///   that section's;
/// * [`COMMENT`] and the author's display name (empty when the record names
///   nobody): the text below it is that comment's;
/// * [`LINK`], the linked page's space key, [`LINK`], its title, [`LINK`]:
///   one line per page the body links to, last, so a backlink is a substring
///   ([`link_line`]).
pub mod indexed {
    /// Opens a section's heading line.
    pub const SECTION: char = '\u{1E}';
    /// Opens a comment's author line.
    pub const COMMENT: char = '\u{1F}';
    /// Delimits a page-link line's space and title.
    pub const LINK: char = '\u{1D}';

    /// Whether `ch` is one of the three marks.
    #[must_use]
    pub fn is_mark(ch: char) -> bool {
        matches!(ch, SECTION | COMMENT | LINK)
    }

    /// The line a source writes for a section heading.
    #[must_use]
    pub fn section_line(heading: &str) -> String {
        format!("{SECTION}{}", heading.trim())
    }

    /// The line a source writes before a comment's text.
    #[must_use]
    pub fn comment_line(author_name: Option<&str>) -> String {
        format!("{COMMENT}{}", author_name.unwrap_or_default().trim())
    }

    /// The line a source writes for a link to the page `title` in the space
    /// `space`. Closed by a mark, so `strpos(body_text, link_line(..))` finds
    /// exactly that page and never a longer title that starts the same way.
    #[must_use]
    pub fn link_line(space: &str, title: &str) -> String {
        format!("{LINK}{}{LINK}{}{LINK}", space.trim(), title.trim())
    }

    /// Which part of a page a run of indexed text belongs to.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Part {
        /// The title and whatever comes before the first heading.
        Lead,
        /// The section under this heading.
        Section(String),
        /// A comment by this display name; empty where the record named
        /// nobody.
        Comment(String),
    }

    /// One part and its text, its lines joined by `\n`.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Run {
        pub part: Part,
        pub text: String,
    }

    /// A page's `body_text`, read back: its parts in order, and the pages it
    /// links to as `(space, title)`.
    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct Indexed {
        pub runs: Vec<Run>,
        pub links: Vec<(String, String)>,
    }

    /// Read a `body_text` written by these rules. Text with no mark at all
    /// (a page synced before the marks, or a source that writes none) is one
    /// [`Part::Lead`].
    #[must_use]
    pub fn read(body_text: &str) -> Indexed {
        let mut out = Indexed::default();
        let mut part = Part::Lead;
        let mut lines: Vec<&str> = Vec::new();
        let flush = |out: &mut Indexed, part: &Part, lines: &mut Vec<&str>| {
            let text = lines.join("\n").trim().to_owned();
            lines.clear();
            if !text.is_empty() || !matches!(part, Part::Lead) {
                out.runs.push(Run {
                    part: part.clone(),
                    text,
                });
            }
        };
        for line in body_text.lines() {
            if let Some(heading) = line.strip_prefix(SECTION) {
                flush(&mut out, &part, &mut lines);
                part = Part::Section(heading.trim().to_owned());
            } else if let Some(name) = line.strip_prefix(COMMENT) {
                flush(&mut out, &part, &mut lines);
                part = Part::Comment(name.trim().to_owned());
            } else if let Some(rest) = line.strip_prefix(LINK) {
                if let Some((space, title)) = rest.trim_end_matches(LINK).split_once(LINK) {
                    out.links.push((space.to_owned(), title.to_owned()));
                }
            } else {
                lines.push(line);
            }
        }
        flush(&mut out, &part, &mut lines);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_cross_as_a_union_on_type() {
        let block = DocBlock::Panel {
            kind: PanelKind::Warning,
            title: None,
            blocks: vec![DocBlock::Paragraph {
                text: vec![
                    Inline::text("See "),
                    Inline::Issue {
                        key: "OPS-77".into(),
                        entity_id: None,
                        title: None,
                        status: None,
                    },
                ],
            }],
        };
        let wire = serde_json::to_value(&block).unwrap();
        assert_eq!(wire["type"], "panel");
        assert_eq!(wire["kind"], "warning");
        assert_eq!(wire["blocks"][0]["type"], "paragraph");
        assert_eq!(wire["blocks"][0]["text"][1]["type"], "issue");
        let back: DocBlock = serde_json::from_value(wire).unwrap();
        assert_eq!(back, block);
    }

    #[test]
    fn the_fill_in_pass_reaches_inlines_inside_lists_tables_and_panels() {
        let mention = || Inline::Mention {
            user: Some("mara".into()),
            name: None,
            me: false,
        };
        let mut blocks = vec![
            DocBlock::List {
                ordered: false,
                items: vec![vec![DocBlock::Paragraph {
                    text: vec![mention()],
                }]],
            },
            DocBlock::Table {
                rows: vec![vec![TableCell {
                    header: false,
                    text: vec![mention()],
                }]],
            },
            DocBlock::Panel {
                kind: PanelKind::Info,
                title: None,
                blocks: vec![DocBlock::Heading {
                    level: 2,
                    text: vec![mention()],
                }],
            },
        ];
        assert_eq!(inlines_mut(&mut blocks).len(), 3);
    }

    #[test]
    fn indexed_text_reads_back_its_sections_comments_and_links() {
        use indexed::*;
        let body = [
            "SEPA payout retry design".to_owned(),
            "Why we retry.".to_owned(),
            section_line("Backoff policy"),
            "Base 30 s, factor 2.".to_owned(),
            "Cap at 5 attempts.".to_owned(),
            section_line("Empty"),
            comment_line(Some("Priya Nair")),
            "Should the cap be 6?".to_owned(),
            comment_line(None),
            "Anonymous.".to_owned(),
            link_line("ENG", "Retry budget"),
            link_line("OPS", "Payout worker restart"),
        ]
        .join("\n");
        let read = read(&body);
        let parts: Vec<(Part, &str)> = read
            .runs
            .iter()
            .map(|r| (r.part.clone(), r.text.as_str()))
            .collect();
        assert_eq!(
            parts,
            vec![
                (Part::Lead, "SEPA payout retry design\nWhy we retry."),
                (
                    Part::Section("Backoff policy".into()),
                    "Base 30 s, factor 2.\nCap at 5 attempts."
                ),
                (Part::Section("Empty".into()), ""),
                (Part::Comment("Priya Nair".into()), "Should the cap be 6?"),
                (Part::Comment(String::new()), "Anonymous."),
            ]
        );
        assert_eq!(
            read.links,
            vec![
                ("ENG".to_owned(), "Retry budget".to_owned()),
                ("OPS".to_owned(), "Payout worker restart".to_owned())
            ]
        );
    }

    #[test]
    fn text_with_no_marks_is_one_lead() {
        let read = indexed::read("Title\n\nJust words.");
        assert_eq!(read.runs.len(), 1);
        assert_eq!(read.runs[0].part, indexed::Part::Lead);
        assert!(read.links.is_empty());
    }

    #[test]
    fn the_marks_are_not_whitespace_so_a_tidy_pass_keeps_them() {
        for mark in [indexed::SECTION, indexed::COMMENT, indexed::LINK] {
            assert!(!mark.is_whitespace(), "{mark:?}");
        }
    }
}
