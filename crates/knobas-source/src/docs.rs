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
}

/// One comment on a page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocComment {
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DocBlock {
    /// A section heading. `level` is 1 to 6 as the source wrote it; the
    /// reader draws every level as a section head.
    Heading { level: u8, text: Vec<Inline> },
    Paragraph { text: Vec<Inline> },
    /// A list. A nested list's items are drawn as a list inside their item.
    List {
        ordered: bool,
        items: Vec<Vec<DocBlock>>,
    },
    /// A code block, verbatim, with the language the source named.
    Code {
        code: String,
        language: Option<String>,
    },
    /// A table, row by row; a header cell says so.
    Table { rows: Vec<Vec<TableCell>> },
    /// An info, note, warning or tip panel, with the blocks inside it.
    Panel {
        kind: PanelKind,
        title: Option<String>,
        blocks: Vec<DocBlock>,
    },
    /// Something the source has that the reader does not draw: the source's
    /// own name for it (a Confluence macro's `ac:name`). Never dropped, so a
    /// page with a part knobas cannot draw says so (story 134).
    Placeholder { name: String },
}

/// One cell of a [`DocBlock::Table`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableCell {
    pub header: bool,
    pub text: Vec<Inline>,
}

/// Which panel a [`DocBlock::Panel`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelKind {
    Info,
    Note,
    Warning,
    Tip,
}

/// One run of text inside a block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Inline {
    /// Words, with the marks the source put on them.
    Text {
        text: String,
        #[serde(default)]
        strong: bool,
        #[serde(default)]
        emphasis: bool,
        #[serde(default)]
        code: bool,
    },
    /// A link out of the docs source, to a web address.
    Link { text: String, href: String },
    /// A link to another page of the docs source, by its title and space.
    /// `entity_id` is the mirrored page it names, filled by knobas when the
    /// reader is read; the adapter leaves it `None`.
    PageLink {
        text: String,
        title: String,
        space: Option<String>,
        #[serde(default)]
        entity_id: Option<String>,
    },
    /// A ticket reference (a Confluence Jira-issue macro), drawn as a live
    /// chip. The adapter fills `key`; knobas fills the rest from the mirror
    /// when the reader is read, and leaves them `None` for a ticket it does
    /// not hold.
    Issue {
        key: String,
        #[serde(default)]
        entity_id: Option<String>,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        status: Option<String>,
    },
    /// A mention of a person. `user` is the account name where the source
    /// says it (the spelling `@me` matches), `name` the display name where it
    /// is known. `me` is filled by knobas when the reader is read.
    Mention {
        user: Option<String>,
        name: Option<String>,
        #[serde(default)]
        me: bool,
    },
    /// A hard line break inside a block.
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
        DocBlock::Heading { text, .. } | DocBlock::Paragraph { text } => out.extend(text.iter_mut()),
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
                items: vec![vec![DocBlock::Paragraph { text: vec![mention()] }]],
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
}
