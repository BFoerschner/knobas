//! Confluence **storage format** in, the Docs reader's blocks out (#859,
//! ADR-0022).
//!
//! The reader draws a fixed set of blocks (`knobas_source::docs`). This module
//! maps a stated subset of the storage format onto them, and everything else
//! onto the placeholder block, so a part knobas cannot draw says so rather than
//! vanishing (story 134).
//!
//! # The subset
//!
//! | Storage format | Block |
//! |---|---|
//! | `h1` to `h6` | heading |
//! | `p`, loose text | paragraph |
//! | `ul`, `ol` (nested too) | list |
//! | `ac:task-list` (nested too) | list, each item checked or not |
//! | `table` with `th` / `td` | table |
//! | `pre`, the `code` and `noformat` macros | code, with the macro's `language` |
//! | the `info`, `note`, `warning`, `tip` and `panel` macros | panel |
//! | the `jira` macro with a `key` | an issue chip (inline) |
//! | `ac:link` to `ri:user` | a mention (inline) |
//! | `ac:link` to `ri:page` | a page link (inline) |
//! | `a href` | a link (inline) |
//! | `strong`/`b`, `em`/`i`, `code` | text marks |
//! | **any other macro, `ac:image`** | placeholder, named |
//!
//! `div`, `section`, `span`, `blockquote`, `thead`, `tbody` and Confluence's
//! layout elements are transparent: their children are read in their place.
//!
//! # Over the shared reader
//!
//! Storage format is XHTML with Confluence's own `ac:` and `ri:` elements.
//! The plain HTML half is read by the shared reader
//! (`knobas_source::docs::html`, spec #2432); this module is only its hook,
//! [`Storage`], which maps the `ac:`/`ri:` half: macros, page and user links,
//! emoticons, images, task lists and layouts. The same tolerant tree carries
//! the section numbering and *Edit section*'s byte spans (#862). Nothing here
//! fails: a body that would not parse still reads as the text it holds.

use std::collections::HashMap;

use knobas_source::docs::{
    DocBlock, DocComment, DocContent, DocSection, DocVersion, EditRefused, EmbedContent, Inline,
    ListItem, PanelKind, SectionEdit, SectionRefusal,
};
use serde_json::Value;

use knobas_source::docs::html::{
    self, Hook, Hooked, Node, Reading, collapse, decode_entities, parse, placeholder,
};

/// A user the record names: account name and display name, found by key.
#[derive(Debug, Clone, Default)]
pub(crate) struct Person {
    pub username: Option<String>,
    pub display_name: Option<String>,
}

/// Who a `ri:userkey` names, as far as the record says.
///
/// A Confluence mention carries a **key**. The page's record names some users
/// outright -- its editor, its creator, every comment's author -- each with
/// key, username and display name, so a mention of any of them resolves. Every
/// other key the body or a comment mentions the sync looked up and kept beside
/// the record at [`crate::users::MENTIONED_USERS`] (#953). A key neither names
/// resolves to nobody: the miss, never a guess.
pub(crate) type People = HashMap<String, Person>;

/// The Docs projection for this adapter: `knobas_source::docs::DocProjection`.
#[must_use]
pub fn doc_content(kind: &str, payload: &Value) -> Option<DocContent> {
    if kind != crate::KIND_PAGE {
        return None;
    }
    let storage = payload.pointer("/body/storage/value")?.as_str()?;
    let people = people(payload);
    let version = payload.pointer("/version/number").and_then(Value::as_u64);
    let labels = payload
        .pointer("/metadata/labels/results")
        .and_then(Value::as_array)
        .map(|labels| {
            labels
                .iter()
                .filter_map(|l| l.get("name").and_then(Value::as_str))
                .filter(|name| !name.trim().is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let comments = payload
        .pointer("/children/comment/results")
        .and_then(Value::as_array)
        .map(|comments| {
            comments
                .iter()
                .filter_map(|c| comment(c, &people))
                .collect()
        })
        .unwrap_or_default();
    let editor_name = payload
        .pointer("/version/by/displayName")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .map(str::to_owned);
    Some(DocContent {
        version,
        editor_name,
        labels,
        blocks: to_blocks(storage, &people),
        comments,
        sections: sections(storage),
        history: history(payload),
    })
}

/// The versions the record names (R-10.15): the one it is, and, when that is
/// not the first, the page's creation as version 1.
fn history(payload: &Value) -> Vec<DocVersion> {
    let string = |v: Option<&Value>| {
        v.and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let mut out = Vec::new();
    let number = payload.pointer("/version/number").and_then(Value::as_u64);
    if payload.get("version").is_some() {
        out.push(DocVersion {
            number,
            by: string(payload.pointer("/version/by/username")),
            by_name: string(payload.pointer("/version/by/displayName")),
            at: string(payload.pointer("/version/when")),
            message: string(payload.pointer("/version/message")),
        });
    }
    if number.is_some_and(|n| n > 1) && payload.pointer("/history/createdBy").is_some() {
        out.push(DocVersion {
            number: Some(1),
            by: string(payload.pointer("/history/createdBy/username")),
            by_name: string(payload.pointer("/history/createdBy/displayName")),
            at: string(payload.pointer("/history/createdDate")),
            message: None,
        });
    }
    out
}

// -- sections and their edits (#862) ------------------------------------------

/// Elements the reader reads through to their children (`Blocks::node`'s
/// transparent arm): a heading inside one is still one of the page's.
fn is_transparent(name: &str) -> bool {
    matches!(
        name,
        "div"
            | "section"
            | "blockquote"
            | "ac:layout"
            | "ac:layout-section"
            | "ac:layout-cell"
            | "ac:rich-text-body"
    )
}

fn is_heading(name: &str) -> bool {
    matches!(name, "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
}

/// The headings the reader numbers, in order, each with whether it sits at
/// the top of the body (and not inside a layout or a wrapper).
fn headings<'a>(nodes: &'a [Node], top: bool, out: &mut Vec<(&'a Node, bool)>) {
    for node in nodes {
        match node.name() {
            Some(name) if is_heading(name) => out.push((node, top)),
            Some(name) if is_transparent(name) => headings(node.children(), false, out),
            _ => {}
        }
    }
}

/// The worst thing a section holds, for its refusal: a table outranks code,
/// code a macro, a macro formatting.
fn refusal_of(node: &Node, in_item: bool) -> Option<SectionRefusal> {
    let Node::Element { name, children, .. } = node else {
        return None;
    };
    let own = match name.as_str() {
        "table" => Some(SectionRefusal::Table),
        "pre" => Some(SectionRefusal::Code),
        "ac:structured-macro" | "ac:macro"
            if matches!(node.attr("ac:name"), Some("code" | "noformat")) =>
        {
            Some(SectionRefusal::Code)
        }
        // The editor's text has no checkbox (spec #2432). A task's own
        // parts are the list's, not macros; what its body holds still counts.
        "ac:task-list" => Some(SectionRefusal::Formatting),
        "ac:task" | "ac:task-id" | "ac:task-uuid" | "ac:task-status" | "ac:task-body" => None,
        n if n.starts_with("ac:") || n.starts_with("ri:") => Some(SectionRefusal::Macro),
        // A numbered list would come back bulleted: the editor's text has
        // only `- ` items, and `from_plain` writes them as `<ul>`.
        "ol" => Some(SectionRefusal::Formatting),
        "ul" if in_item => Some(SectionRefusal::Formatting),
        "p" | "ul" | "li" | "br" => None,
        _ => Some(SectionRefusal::Formatting),
    };
    let item = in_item || name == "li";
    children
        .iter()
        .filter_map(|c| refusal_of(c, item))
        .chain(own)
        .min_by_key(|r| rank(*r))
}

/// The panels *Edit section* keeps (R-10.13, #975): the macro's name and the
/// word the editor's text names it by.
const PANELS: [(&str, &str); 4] = [
    ("info", "Info"),
    ("note", "Note"),
    ("warning", "Warning"),
    ("tip", "Tip"),
];

/// The editor's word for a panel *Edit section* keeps, for a structured macro
/// that is one. The generic `panel` macro is not one: its colours and borders
/// are parameters a text edit would drop.
fn panel_word(node: &Node) -> Option<&'static str> {
    if node.name() != Some("ac:structured-macro") {
        return None;
    }
    let name = node.attr("ac:name")?.to_ascii_lowercase();
    PANELS
        .iter()
        .find(|(macro_name, _)| *macro_name == name)
        .map(|(_, word)| *word)
}

/// For a panel *Edit section* keeps, at the top of a section: the worst thing
/// it holds, by the rules a section's own body follows. A parameter other
/// than the title (`icon`, say) refuses as a macro, since the text would
/// drop it. `None` for any other node.
fn panel_refusal(node: &Node) -> Option<Option<SectionRefusal>> {
    panel_word(node)?;
    let worst = node
        .children()
        .iter()
        .filter_map(|child| match child {
            Node::Text(t) if t.trim().is_empty() => None,
            Node::Element { name, .. }
                if name == "ac:parameter" && child.attr("ac:name") == Some("title") =>
            {
                None
            }
            Node::Element { name, children, .. } if name == "ac:rich-text-body" => children
                .iter()
                .filter_map(|c| refusal_of(c, false))
                .min_by_key(|r| rank(*r)),
            _ => Some(SectionRefusal::Macro),
        })
        .min_by_key(|r| rank(*r));
    Some(worst)
}

fn rank(refusal: SectionRefusal) -> u8 {
    match refusal {
        SectionRefusal::Table => 0,
        SectionRefusal::Code => 1,
        SectionRefusal::Macro => 2,
        SectionRefusal::Formatting => 3,
    }
}

/// One section's place in the body: where its text starts and ends, and
/// whether knobas edits it.
struct Span {
    from: usize,
    to: usize,
    verdict: Result<String, SectionRefusal>,
}

fn spans(storage: &str) -> Vec<Span> {
    let nodes = parse(storage);
    let mut found = Vec::new();
    headings(&nodes, true, &mut found);
    found
        .iter()
        .enumerate()
        .map(|(i, (heading, top))| {
            let next = found.get(i + 1);
            let Node::Element {
                span: (_, from), ..
            } = heading
            else {
                unreachable!("a heading is an element")
            };
            let to = match next {
                Some((
                    Node::Element {
                        span: (start, _), ..
                    },
                    _,
                )) => *start,
                _ => storage.len(),
            };
            let verdict = if !top || next.is_some_and(|(_, next_top)| !next_top) {
                Err(SectionRefusal::Macro)
            } else {
                let inside: Vec<&Node> = nodes
                    .iter()
                    .skip_while(|n| !std::ptr::eq(*n, *heading))
                    .skip(1)
                    .take_while(|n| next.is_none_or(|(h, _)| !std::ptr::eq(*n, *h)))
                    .collect();
                match inside
                    .iter()
                    .filter_map(|n| {
                        panel_refusal(n)
                            .unwrap_or_else(|| refusal_of(n, false).or_else(|| reads_as_a_panel(n)))
                    })
                    .min_by_key(|r| rank(*r))
                {
                    Some(refusal) => Err(refusal),
                    None => Ok(plain_text(&inside)),
                }
            };
            Span {
                from: *from,
                to,
                verdict,
            }
        })
        .collect()
}

/// An editable section's body as the editor's plain text.
fn plain_text(nodes: &[&Node]) -> String {
    fn line(node: &Node) -> String {
        match node {
            Node::Element { name, .. } if name == "br" => "\n".to_owned(),
            // A paragraph ends its line, so two inside one list item read
            // as two sentences and not as one run-together word.
            Node::Element { name, children, .. } if name == "p" => {
                children.iter().map(line).chain(["\n".to_owned()]).collect()
            }
            Node::Element { children, .. } => children.iter().map(line).collect(),
            Node::Text(t) | Node::Cdata(t) => t.clone(),
        }
    }
    let tidy = |text: String| {
        text.lines()
            .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_owned()
    };
    let mut blocks: Vec<String> = Vec::new();
    for node in nodes {
        if let Some(word) = panel_word(node) {
            blocks.push(panel_text(node, word));
            continue;
        }
        match node.name() {
            Some("ul" | "ol") => {
                let items: Vec<String> = node
                    .children()
                    .iter()
                    .filter(|c| c.name() == Some("li"))
                    .map(|li| format!("- {}", tidy(line(li)).replace('\n', " ")))
                    .collect();
                if !items.is_empty() {
                    blocks.push(items.join("\n"));
                }
            }
            _ => {
                let text = tidy(line(node));
                if !text.is_empty() {
                    blocks.push(text);
                }
            }
        }
    }
    blocks.join("\n\n")
}

/// A panel as the editor's text: a quote whose first line is the kind in
/// bold and the title beside it, then the panel's body, as the Markdown
/// export spells a panel (`knobas_source::docs::markdown`).
fn panel_text(node: &Node, word: &str) -> String {
    let mut first = format!("**{word}**");
    if let Some(title) = param(node, "title") {
        first.push(' ');
        first.push_str(&title.split_whitespace().collect::<Vec<_>>().join(" "));
    }
    let inner: Vec<&Node> = node
        .child("ac:rich-text-body")
        .map(|body| body.children().iter().collect())
        .unwrap_or_default();
    let body = plain_text(&inner);
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

/// The opening line of a panel in the editor's text: `> **Info**`, the kind
/// in any case, then the title if any. Answers the panel's macro name and its
/// title (empty when it has none).
fn panel_opening(line: &str) -> Option<(&'static str, &str)> {
    let rest = line.trim().strip_prefix('>')?.trim_start();
    let rest = rest.strip_prefix("**")?;
    let (word, after) = rest.split_once("**")?;
    let (name, _) = PANELS.iter().find(|(_, w)| w.eq_ignore_ascii_case(word))?;
    if !(after.is_empty() || after.starts_with(char::is_whitespace)) {
        return None;
    }
    Some((name, after.trim()))
}

/// A block other than a panel whose text holds a line `> **Info**` refuses as
/// formatting: the editor's text would write it back as a panel, so saving
/// the section unchanged would turn the words into a macro.
fn reads_as_a_panel(node: &Node) -> Option<SectionRefusal> {
    plain_text(&[node])
        .lines()
        .any(|line| panel_opening(line).is_some())
        .then_some(SectionRefusal::Formatting)
}

/// The editor's text in storage format: [`crate::storage::from_plain`]'s
/// rules, and a quote that opens with a panel's kind in bold
/// ([`panel_opening`]) at the start of a block is that panel, for as long as
/// its lines start with `>`. Any other quote stays text, as typed.
fn from_section_text(text: &str) -> String {
    let text = text.replace("\r\n", "\n");
    let mut out = String::with_capacity(text.len() + 64);
    let mut plain: Vec<&str> = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        let starts_a_block = plain.last().is_none_or(|l| l.trim().is_empty());
        let Some((name, title)) = panel_opening(line).filter(|_| starts_a_block) else {
            plain.push(line);
            continue;
        };
        out.push_str(&crate::storage::from_plain(&plain.join("\n")));
        plain.clear();
        let mut body: Vec<&str> = Vec::new();
        while let Some(next) = lines.next_if(|l| l.trim_start().starts_with('>')) {
            let inner = next.trim_start().strip_prefix('>').unwrap_or_default();
            body.push(inner.strip_prefix(' ').unwrap_or(inner));
        }
        out.push_str(&crate::storage::panel(
            name,
            title,
            &crate::storage::from_plain(&body.join("\n")),
        ));
    }
    out.push_str(&crate::storage::from_plain(&plain.join("\n")));
    out
}

/// The page's sections as the reader numbers them, each with its text or the
/// reason knobas does not edit it.
fn sections(storage: &str) -> Vec<DocSection> {
    spans(storage)
        .into_iter()
        .zip(1..)
        .map(|(span, number)| match span.verdict {
            Ok(text) => DocSection {
                number,
                text: Some(text),
                refusal: None,
            },
            Err(refusal) => DocSection {
                number,
                text: None,
                refusal: Some(refusal),
            },
        })
        .collect()
}

/// `knobas_source::docs::DocWrites::edit_section` for this adapter: section
/// `number`'s body replaced with `text` in storage format, everything outside
/// it byte for byte as it was, against the record's version.
///
/// # Errors
///
/// [`EditRefused::Section`] for a section knobas does not edit,
/// [`EditRefused::Missing`] for a record with no such section, no body or no
/// version.
pub fn edit_section(
    kind: &str,
    payload: &Value,
    number: u32,
    text: &str,
) -> Result<SectionEdit, EditRefused> {
    if kind != crate::KIND_PAGE {
        return Err(EditRefused::Missing);
    }
    let storage = payload
        .pointer("/body/storage/value")
        .and_then(Value::as_str)
        .ok_or(EditRefused::Missing)?;
    let base_version = payload
        .pointer("/version/number")
        .and_then(Value::as_i64)
        .ok_or(EditRefused::Missing)?;
    let index = usize::try_from(number)
        .ok()
        .and_then(|n| n.checked_sub(1))
        .ok_or(EditRefused::Missing)?;
    let span = spans(storage)
        .into_iter()
        .nth(index)
        .ok_or(EditRefused::Missing)?;
    span.verdict.map_err(EditRefused::Section)?;
    let mut body = String::with_capacity(storage.len() + text.len());
    body.push_str(&storage[..span.from]);
    body.push_str(&from_section_text(text));
    body.push_str(&storage[span.to..]);
    Ok(SectionEdit { body, base_version })
}

fn comment(raw: &Value, people: &People) -> Option<DocComment> {
    let storage = raw.pointer("/body/storage/value")?.as_str()?;
    let by = raw
        .pointer("/history/createdBy")
        .or_else(|| raw.pointer("/version/by"));
    let string = |v: Option<&Value>, key: &str| {
        v.and_then(|v| v.get(key))
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .map(str::to_owned)
    };
    Some(DocComment {
        // Confluence's content id for the comment: the id a comment write
        // answers (#2074), so Docs knows its own comment after an edit.
        id: string(Some(raw), "id"),
        author: string(by, "username"),
        author_name: string(by, "displayName"),
        at: raw
            .pointer("/history/createdDate")
            .or_else(|| raw.pointer("/version/when"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        blocks: to_blocks(storage, people),
    })
}

/// Every user object the record carries, by key.
pub(crate) fn people(payload: &Value) -> People {
    let mut out = People::new();
    let mut add = |user: Option<&Value>| {
        let Some(user) = user else { return };
        let Some(key) = user.get("userKey").and_then(Value::as_str) else {
            return;
        };
        let get = |k: &str| user.get(k).and_then(Value::as_str).map(str::to_owned);
        out.entry(key.to_owned()).or_insert(Person {
            username: get("username"),
            display_name: get("displayName"),
        });
    };
    add(payload.pointer("/version/by"));
    add(payload.pointer("/history/createdBy"));
    if let Some(comments) = payload
        .pointer("/children/comment/results")
        .and_then(Value::as_array)
    {
        for c in comments {
            add(c.pointer("/version/by"));
            add(c.pointer("/history/createdBy"));
        }
    }
    // Last, so a user the record names itself keeps that record's spelling.
    if let Some(users) = payload
        .get(crate::users::MENTIONED_USERS)
        .and_then(Value::as_array)
    {
        for user in users {
            add(Some(user));
        }
    }
    out
}

// -- storage format over the shared reader ------------------------------------

/// A macro's `ac:parameter` by name.
fn param(node: &Node, key: &str) -> Option<String> {
    node.children()
        .iter()
        .find(|c| c.name() == Some("ac:parameter") && c.attr("ac:name") == Some(key))
        .map(|p| p.text().trim().to_owned())
        .filter(|v| !v.is_empty())
}

/// A storage body as the reader's blocks: the shared HTML reader
/// (`knobas_source::docs::html`) with [`Storage`] as its hook.
pub(crate) fn to_blocks(storage: &str, people: &People) -> Vec<DocBlock> {
    html::to_blocks(storage, &Storage { people })
}

/// The `ac:` / `ri:` half of storage format: what plain HTML does not have.
struct Storage<'a> {
    people: &'a People,
}

impl Hook for Storage<'_> {
    fn element(&self, node: &Node, reading: &Reading<'_>) -> Hooked {
        match node.name().unwrap_or_default() {
            "ac:structured-macro" | "ac:macro" => r#macro(node, reading),
            "ac:image" => Hooked::Blocks(vec![placeholder("image")]),
            "ac:task-list" => Hooked::Blocks(vec![task_list(node, reading)]),
            "ac:layout" | "ac:layout-section" | "ac:layout-cell" | "ac:rich-text-body" => {
                Hooked::Blocks(reading.blocks(node.children()))
            }
            "ac:link" => Hooked::Inlines(self.link(node, reading)),
            "ac:emoticon" => Hooked::Inlines(
                node.attr("ac:emoji-fallback")
                    .map(|fallback| Inline::text(decode_entities(fallback)))
                    .into_iter()
                    .collect(),
            ),
            "ac:parameter" => Hooked::Inlines(vec![]),
            // Anything else (an inline comment marker, a placeholder's
            // instruction text, an element storage format added later) reads
            // as what it holds.
            _ => Hooked::Unwrap,
        }
    }
}

/// `ac:task-list`: a list whose items carry their checked state (spec
/// #2432). A nested task list may sit in a task's body, in the task beside
/// its body, or after the task in the outer list; each lands inside the task
/// it belongs to.
fn task_list(node: &Node, reading: &Reading<'_>) -> DocBlock {
    let mut items: Vec<ListItem> = Vec::new();
    for child in node.children() {
        match child.name() {
            Some("ac:task") => {
                let done = child
                    .child("ac:task-status")
                    .is_some_and(|s| s.text().trim().eq_ignore_ascii_case("complete"));
                let mut blocks = child
                    .child("ac:task-body")
                    .map(|body| reading.blocks(body.children()))
                    .unwrap_or_default();
                blocks.extend(
                    child
                        .children()
                        .iter()
                        .filter(|c| c.name() == Some("ac:task-list"))
                        .map(|inner| task_list(inner, reading)),
                );
                items.push(ListItem::task(done, blocks));
            }
            Some("ac:task-list") => {
                let inner = task_list(child, reading);
                match items.last_mut() {
                    Some(item) => item.blocks.push(inner),
                    None => items.push(ListItem::new(vec![inner])),
                }
            }
            _ => {}
        }
    }
    DocBlock::List {
        ordered: false,
        items,
    }
}

fn r#macro(node: &Node, reading: &Reading<'_>) -> Hooked {
    let name = node
        .attr("ac:name")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let block = match name.as_str() {
        "code" | "noformat" => DocBlock::Code {
            code: node
                .child("ac:plain-text-body")
                .map(Node::text)
                .unwrap_or_default(),
            language: param(node, "language"),
        },
        "info" | "note" | "warning" | "tip" | "panel" => DocBlock::Panel {
            kind: match name.as_str() {
                "info" => PanelKind::Info,
                "warning" => PanelKind::Warning,
                "tip" => PanelKind::Tip,
                _ => PanelKind::Note,
            },
            title: param(node, "title"),
            blocks: node
                .child("ac:rich-text-body")
                .map(|body| reading.blocks(body.children()))
                .unwrap_or_default(),
        },
        // The body as written, scripts and all (#2440): the shell runs it in
        // an isolated frame. A body of nothing is nothing to run.
        "html" => match node.child("ac:plain-text-body").map(Node::text) {
            Some(html) if !html.trim().is_empty() => DocBlock::Embed {
                name,
                content: EmbedContent::Html { html },
            },
            _ => placeholder("html"),
        },
        "jira" => match param(node, "key") {
            Some(key) => {
                return Hooked::Inlines(vec![Inline::Issue {
                    key,
                    entity_id: None,
                    title: None,
                    status: None,
                }]);
            }
            None => placeholder("jira"),
        },
        _ => placeholder(&name),
    };
    Hooked::Blocks(vec![block])
}

impl Storage<'_> {
    /// `ac:link`: a mention, a page link, or (anything else it can point at)
    /// its link text.
    fn link(&self, node: &Node, reading: &Reading<'_>) -> Vec<Inline> {
        let body = node
            .child("ac:plain-text-link-body")
            .or_else(|| node.child("ac:link-body"))
            .map(|b| collapse(&b.text()))
            .filter(|t| !t.is_empty());
        if let Some(user) = node.child("ri:user") {
            let by_name = user
                .attr("ri:username")
                .filter(|n| !n.is_empty())
                .map(decode_entities);
            let person = user
                .attr("ri:userkey")
                .and_then(|key| self.people.get(key))
                .cloned()
                .unwrap_or_default();
            let user_name = by_name.or(person.username);
            return vec![Inline::Mention {
                name: person.display_name.or_else(|| body.clone()),
                user: user_name,
                me: false,
            }];
        }
        if let Some(page) = node.child("ri:page") {
            let title = page
                .attr("ri:content-title")
                .map(decode_entities)
                .unwrap_or_default();
            if !title.is_empty() {
                return vec![Inline::PageLink {
                    text: body.unwrap_or_else(|| title.clone()),
                    title,
                    space: page.attr("ri:space-key").map(decode_entities),
                    entity_id: None,
                }];
            }
        }
        body.map(|text| reading.text(text)).into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use knobas_source::docs::{ListItem, TableCell};
    use serde_json::json;

    fn blocks(storage: &str) -> Vec<DocBlock> {
        to_blocks(storage, &People::new())
    }

    fn para(text: &str) -> DocBlock {
        DocBlock::Paragraph {
            text: vec![Inline::text(text)],
        }
    }

    #[test]
    fn headings_paragraphs_and_marks() {
        assert_eq!(
            blocks(
                "<h2>Backoff policy</h2><p>Retry <strong>five</strong> times, <em>then</em> stop.</p>"
            ),
            vec![
                DocBlock::Heading {
                    level: 2,
                    text: vec![Inline::text("Backoff policy")]
                },
                DocBlock::Paragraph {
                    text: vec![
                        Inline::text("Retry "),
                        Inline::Text {
                            text: "five".into(),
                            strong: true,
                            emphasis: false,
                            code: false
                        },
                        Inline::text(" times, "),
                        Inline::Text {
                            text: "then".into(),
                            strong: false,
                            emphasis: true,
                            code: false
                        },
                        Inline::text(" stop."),
                    ]
                },
            ]
        );
    }

    #[test]
    fn markup_in_text_stays_text() {
        assert_eq!(
            blocks("<p>&lt;script&gt; &amp; co</p>"),
            vec![para("<script> & co")]
        );
    }

    #[test]
    fn nested_lists_are_lists_inside_their_item() {
        assert_eq!(
            blocks("<ol><li>Export the rows<ul><li>as CSV</li></ul></li><li>Group them</li></ol>"),
            vec![DocBlock::List {
                ordered: true,
                items: vec![
                    ListItem::new(vec![
                        para("Export the rows"),
                        DocBlock::List {
                            ordered: false,
                            items: vec![ListItem::new(vec![para("as CSV")])]
                        }
                    ]),
                    ListItem::new(vec![para("Group them")]),
                ]
            }]
        );
    }

    #[test]
    fn a_code_macro_is_a_code_block_with_its_language() {
        let storage = r#"<ac:structured-macro ac:name="code" ac:schema-version="1" ac:macro-id="a1"><ac:parameter ac:name="language">sql</ac:parameter><ac:parameter ac:name="title">Flagged</ac:parameter><ac:plain-text-body><![CDATA[select * from recon.flagged
where amount < 0;]]></ac:plain-text-body></ac:structured-macro>"#;
        assert_eq!(
            blocks(storage),
            vec![DocBlock::Code {
                code: "select * from recon.flagged\nwhere amount < 0;".into(),
                language: Some("sql".into()),
            }]
        );
    }

    #[test]
    fn a_noformat_macro_and_a_pre_are_code_with_no_language() {
        assert_eq!(
            blocks(
                r#"<ac:structured-macro ac:name="noformat"><ac:plain-text-body><![CDATA[docker ps]]></ac:plain-text-body></ac:structured-macro><pre>a &lt; b</pre>"#
            ),
            vec![
                DocBlock::Code {
                    code: "docker ps".into(),
                    language: None
                },
                DocBlock::Code {
                    code: "a < b".into(),
                    language: None
                },
            ]
        );
    }

    #[test]
    fn the_four_panels_and_the_generic_panel_map_to_kinds() {
        let panel = |name: &str| {
            format!(
                r#"<ac:structured-macro ac:name="{name}" ac:schema-version="1"><ac:parameter ac:name="title">Heads up</ac:parameter><ac:rich-text-body><p>Old topology.</p></ac:rich-text-body></ac:structured-macro>"#
            )
        };
        for (name, kind) in [
            ("info", PanelKind::Info),
            ("note", PanelKind::Note),
            ("warning", PanelKind::Warning),
            ("tip", PanelKind::Tip),
            ("panel", PanelKind::Note),
        ] {
            assert_eq!(
                blocks(&panel(name)),
                vec![DocBlock::Panel {
                    kind,
                    title: Some("Heads up".into()),
                    blocks: vec![para("Old topology.")],
                }],
                "{name}"
            );
        }
    }

    #[test]
    fn a_table_keeps_header_cells_and_rows() {
        let storage = "<table><colgroup><col /><col /></colgroup><tbody><tr><th><p>After</p></th><th><p>Who</p></th></tr><tr><td><p>15 min</p></td><td>second on-call</td></tr></tbody></table>";
        let cell = |header, text: &str| TableCell {
            header,
            text: vec![Inline::text(text)],
        };
        assert_eq!(
            blocks(storage),
            vec![DocBlock::Table {
                rows: vec![
                    vec![cell(true, "After"), cell(true, "Who")],
                    vec![cell(false, "15 min"), cell(false, "second on-call")],
                ]
            }]
        );
    }

    #[test]
    fn a_jira_macro_is_an_issue_chip_inside_its_sentence() {
        let storage = r#"<p>That was <ac:structured-macro ac:name="jira" ac:schema-version="1"><ac:parameter ac:name="server">Jira</ac:parameter><ac:parameter ac:name="serverId">abc</ac:parameter><ac:parameter ac:name="key">PAY-228</ac:parameter></ac:structured-macro>.</p>"#;
        assert_eq!(
            blocks(storage),
            vec![DocBlock::Paragraph {
                text: vec![
                    Inline::text("That was "),
                    Inline::Issue {
                        key: "PAY-228".into(),
                        entity_id: None,
                        title: None,
                        status: None
                    },
                    Inline::text("."),
                ]
            }]
        );
    }

    #[test]
    fn a_jira_macro_with_no_key_is_a_placeholder() {
        let storage = r#"<ac:structured-macro ac:name="jira"><ac:parameter ac:name="jqlQuery">project = PAY</ac:parameter></ac:structured-macro>"#;
        assert_eq!(
            blocks(storage),
            vec![DocBlock::Placeholder {
                name: "jira".into()
            }]
        );
    }

    #[test]
    fn a_user_mention_resolves_by_name_or_by_a_key_the_record_names() {
        let mut people = People::new();
        people.insert(
            "k-priya".into(),
            Person {
                username: Some("priya".into()),
                display_name: Some("Priya Nair".into()),
            },
        );
        let storage = r#"<p><ac:link><ri:user ri:userkey="k-priya" /></ac:link> and <ac:link><ri:user ri:username="mara" /></ac:link> and <ac:link><ri:user ri:userkey="k-unknown" /></ac:link></p>"#;
        assert_eq!(
            to_blocks(storage, &people),
            vec![DocBlock::Paragraph {
                text: vec![
                    Inline::Mention {
                        user: Some("priya".into()),
                        name: Some("Priya Nair".into()),
                        me: false
                    },
                    Inline::text(" and "),
                    Inline::Mention {
                        user: Some("mara".into()),
                        name: None,
                        me: false
                    },
                    Inline::text(" and "),
                    Inline::Mention {
                        user: None,
                        name: None,
                        me: false
                    },
                ]
            }]
        );
    }

    #[test]
    fn a_page_link_names_its_title_and_space() {
        let storage = r#"<p>See <ac:link><ri:page ri:space-key="OPS" ri:content-title="Ledger reconciliation runbook" /><ac:plain-text-link-body><![CDATA[the runbook]]></ac:plain-text-link-body></ac:link>.</p>"#;
        assert_eq!(
            blocks(storage),
            vec![DocBlock::Paragraph {
                text: vec![
                    Inline::text("See "),
                    Inline::PageLink {
                        text: "the runbook".into(),
                        title: "Ledger reconciliation runbook".into(),
                        space: Some("OPS".into()),
                        entity_id: None
                    },
                    Inline::text("."),
                ]
            }]
        );
    }

    #[test]
    fn a_web_link_keeps_its_address() {
        assert_eq!(
            blocks(r#"<p><a href="https://example.com/?a=1&amp;b=2">docs</a></p>"#),
            vec![DocBlock::Paragraph {
                text: vec![Inline::Link {
                    text: "docs".into(),
                    href: "https://example.com/?a=1&b=2".into()
                }]
            }]
        );
    }

    #[test]
    fn an_unknown_macro_never_vanishes() {
        let storage = r#"<p>Before</p><ac:structured-macro ac:name="roadmap"><ac:parameter ac:name="source">x</ac:parameter></ac:structured-macro><p>After</p>"#;
        assert_eq!(
            blocks(storage),
            vec![
                para("Before"),
                DocBlock::Placeholder {
                    name: "roadmap".into()
                },
                para("After"),
            ]
        );
    }

    #[test]
    fn an_unknown_macro_inside_a_sentence_splits_it_and_is_still_there() {
        let storage = r#"<p>Status: <ac:structured-macro ac:name="status"><ac:parameter ac:name="title">DONE</ac:parameter></ac:structured-macro> since Monday.</p>"#;
        assert_eq!(
            blocks(storage),
            vec![
                para("Status:"),
                DocBlock::Placeholder {
                    name: "status".into()
                },
                para("since Monday."),
            ]
        );
    }

    /// The `html` macro is an embed of its body as the author wrote it (#2440):
    /// script, entities and a CDATA section Confluence split around `]]>`
    /// come through byte for byte, and a sentence around it splits as for any
    /// block macro.
    #[test]
    fn an_html_macro_is_an_embed_of_its_markup_byte_for_byte() {
        let storage = concat!(
            r#"<p>Try it: <ac:structured-macro ac:name="html" ac:schema-version="1" ac:macro-id="m1">"#,
            r#"<ac:plain-text-body><![CDATA[<div id="ui"></div>"#,
            "\n<script src=\"https://api.example.com/swagger-ui.js\"></script>\n",
            r#"<script>if (a ]]]]><![CDATA[> b && c < d) SwaggerUIBundle({url: "/spec.json?x=1&amp;y"});</script>]]>"#,
            r#"</ac:plain-text-body></ac:structured-macro> below.</p>"#,
        );
        assert_eq!(
            blocks(storage),
            vec![
                para("Try it:"),
                DocBlock::Embed {
                    name: "html".into(),
                    content: EmbedContent::Html {
                        html: concat!(
                            r#"<div id="ui"></div>"#,
                            "\n<script src=\"https://api.example.com/swagger-ui.js\"></script>\n",
                            r#"<script>if (a ]]> b && c < d) SwaggerUIBundle({url: "/spec.json?x=1&amp;y"});</script>"#,
                        )
                        .into()
                    },
                },
                para("below."),
            ]
        );
    }

    /// A section holding an embed is edited in Confluence: a text box would
    /// flatten the markup it runs (spec #2432, *Section editing*).
    #[test]
    fn a_section_holding_an_html_macro_refuses_as_a_macro() {
        let storage = r#"<h2>API</h2><p>Below.</p><ac:structured-macro ac:name="html"><ac:plain-text-body><![CDATA[<p>hi</p>]]></ac:plain-text-body></ac:structured-macro><h2>Plain</h2><p>Text.</p>"#;
        assert_eq!(
            sections(storage)
                .into_iter()
                .map(|s| s.refusal)
                .collect::<Vec<_>>(),
            vec![Some(SectionRefusal::Macro), None]
        );
    }

    /// An `html` macro with no body holds nothing to run: a placeholder, so
    /// the page still says something is there.
    #[test]
    fn an_empty_html_macro_is_a_placeholder() {
        assert_eq!(
            blocks(
                r#"<ac:structured-macro ac:name="html"><ac:plain-text-body><![CDATA[  ]]></ac:plain-text-body></ac:structured-macro>"#
            ),
            vec![DocBlock::Placeholder {
                name: "html".into()
            }]
        );
    }

    #[test]
    fn an_image_is_a_placeholder() {
        assert_eq!(
            blocks(r#"<ac:image><ri:attachment ri:filename="a.png" /></ac:image>"#),
            vec![DocBlock::Placeholder {
                name: "image".into()
            }]
        );
    }

    fn task(id: u32, status: &str, body: &str) -> String {
        format!(
            "<ac:task><ac:task-id>{id}</ac:task-id><ac:task-uuid>u-{id}</ac:task-uuid>\
             <ac:task-status>{status}</ac:task-status><ac:task-body>{body}</ac:task-body></ac:task>"
        )
    }

    /// Spec #2432, story 23: a task list is a list whose items carry their
    /// checked state.
    #[test]
    fn a_task_list_is_a_list_of_checked_and_unchecked_items() {
        let storage = format!(
            "<ac:task-list>{}{}</ac:task-list>",
            task(
                1,
                "complete",
                "<span class=\"placeholder-inline-tasks\">Drain the <strong>queue</strong></span>"
            ),
            task(2, "incomplete", "Restart the worker"),
        );
        assert_eq!(
            blocks(&storage),
            vec![DocBlock::List {
                ordered: false,
                items: vec![
                    ListItem::task(
                        true,
                        vec![DocBlock::Paragraph {
                            text: vec![
                                Inline::text("Drain the "),
                                Inline::Text {
                                    text: "queue".into(),
                                    strong: true,
                                    emphasis: false,
                                    code: false,
                                },
                            ],
                        }]
                    ),
                    ListItem::task(false, vec![para("Restart the worker")]),
                ]
            }]
        );
    }

    /// A nested task list lands inside the task it follows, whether the
    /// storage puts it in the task's body, in the task, or after the task in
    /// the outer list.
    #[test]
    fn a_nested_task_list_is_a_list_inside_its_task() {
        let inner = |id| {
            format!(
                "<ac:task-list>{}</ac:task-list>",
                task(id, "complete", "eu-1")
            )
        };
        let nested = ListItem::task(
            false,
            vec![
                para("Restart"),
                DocBlock::List {
                    ordered: false,
                    items: vec![ListItem::task(true, vec![para("eu-1")])],
                },
            ],
        );
        let in_body = format!(
            "<ac:task-list>{}</ac:task-list>",
            task(1, "incomplete", &format!("Restart{}", inner(2)))
        );
        let in_task = format!(
            "<ac:task-list><ac:task><ac:task-id>1</ac:task-id><ac:task-status>incomplete</ac:task-status>\
             <ac:task-body>Restart</ac:task-body>{}</ac:task></ac:task-list>",
            inner(2)
        );
        let after = format!(
            "<ac:task-list>{}{}</ac:task-list>",
            task(1, "incomplete", "Restart"),
            inner(2)
        );
        for storage in [in_body, in_task, after] {
            assert_eq!(
                blocks(&storage),
                vec![DocBlock::List {
                    ordered: false,
                    items: vec![nested.clone()]
                }],
                "{storage}"
            );
        }
    }

    #[test]
    fn a_task_list_inside_a_sentence_and_a_panel_still_reads() {
        let list = format!(
            "<ac:task-list>{}</ac:task-list>",
            task(1, "complete", "Done")
        );
        let done = DocBlock::List {
            ordered: false,
            items: vec![ListItem::task(true, vec![para("Done")])],
        };
        assert_eq!(
            blocks(&format!("<p>Before{list}after</p>")),
            vec![para("Before"), done.clone(), para("after")]
        );
        assert_eq!(
            blocks(&format!(
                "<ac:structured-macro ac:name=\"info\"><ac:rich-text-body>{list}</ac:rich-text-body></ac:structured-macro>"
            )),
            vec![DocBlock::Panel {
                kind: PanelKind::Info,
                title: None,
                blocks: vec![done]
            }]
        );
    }

    #[test]
    fn layouts_are_transparent_and_broken_markup_still_reads() {
        assert_eq!(
            blocks(
                "<ac:layout><ac:layout-section><ac:layout-cell><p>One</p></ac:layout-cell></ac:layout-section></ac:layout><p>Two</em> and <b>three"
            ),
            vec![
                para("One"),
                DocBlock::Paragraph {
                    text: vec![
                        Inline::text("Two and "),
                        Inline::Text {
                            text: "three".into(),
                            strong: true,
                            emphasis: false,
                            code: false
                        }
                    ]
                }
            ]
        );
    }

    #[test]
    fn a_page_record_projects_its_version_labels_and_comments() {
        let payload = json!({
            "id": "65601",
            "title": "On-call handbook",
            "body": {"storage": {"value": "<p>Hello <ac:link><ri:user ri:userkey=\"k-tw\" /></ac:link></p>"}},
            "version": {"number": 31, "by": {"username": "tomasz", "userKey": "k-tw", "displayName": "Tomasz Wójcik"}},
            "metadata": {"labels": {"results": [{"name": "ops"}, {"name": "on-call"}]}},
            "children": {"comment": {"results": [{
                "id": "65700",
                "body": {"storage": {"value": "<p>Rewriting the Ledger section.</p>"}},
                "history": {"createdBy": {"username": "tomasz", "displayName": "Tomasz Wójcik", "userKey": "k-tw"}, "createdDate": "2026-08-19T10:00:00.000Z"}
            }]}}
        });
        let doc = doc_content("page", &payload).expect("a page projects");
        assert_eq!(doc.version, Some(31));
        assert_eq!(doc.editor_name.as_deref(), Some("Tomasz Wójcik"));
        assert_eq!(doc.labels, vec!["ops", "on-call"]);
        assert_eq!(
            doc.blocks,
            vec![DocBlock::Paragraph {
                text: vec![
                    Inline::text("Hello "),
                    Inline::Mention {
                        user: Some("tomasz".into()),
                        name: Some("Tomasz Wójcik".into()),
                        me: false
                    }
                ]
            }]
        );
        assert_eq!(
            doc.comments,
            vec![DocComment {
                id: Some("65700".into()),
                author: Some("tomasz".into()),
                author_name: Some("Tomasz Wójcik".into()),
                at: Some("2026-08-19T10:00:00.000Z".into()),
                blocks: vec![para("Rewriting the Ledger section.")],
            }]
        );
    }

    /// #953: a key the record names nowhere resolves through the users the
    /// sync looked up and kept beside the record, in a comment as in the body.
    #[test]
    fn a_mention_resolves_through_the_users_the_sync_looked_up() {
        let payload = json!({
            "body": {"storage": {"value": "<p><ac:link><ri:user ri:userkey=\"k-mara\" /></ac:link></p>"}},
            "version": {"number": 2, "by": {"username": "tomasz", "userKey": "k-tw"}},
            "children": {"comment": {"results": [{
                "body": {"storage": {"value": "<p><ac:link><ri:user ri:userkey=\"k-mara\" /></ac:link> ok</p>"}},
                "history": {"createdBy": {"username": "tomasz", "userKey": "k-tw"}}
            }]}},
            "mentioned_users": [
                {"userKey": "k-mara", "username": "mara", "displayName": "Mara Lindqvist"}
            ]
        });
        let doc = doc_content("page", &payload).expect("a page projects");
        let mara = Inline::Mention {
            user: Some("mara".into()),
            name: Some("Mara Lindqvist".into()),
            me: false,
        };
        assert_eq!(
            doc.blocks,
            vec![DocBlock::Paragraph {
                text: vec![mara.clone()]
            }]
        );
        assert_eq!(
            doc.comments[0].blocks,
            vec![DocBlock::Paragraph {
                text: vec![mara, Inline::text(" ok")]
            }]
        );
    }

    #[test]
    fn another_kind_or_a_bodyless_record_projects_nothing() {
        assert_eq!(
            doc_content(
                "ticket",
                &json!({"body": {"storage": {"value": "<p>x</p>"}}})
            ),
            None
        );
        assert_eq!(doc_content("page", &json!({"title": "no body"})), None);
    }

    // -- sections and their edits (#862) --

    fn page(storage: &str) -> Value {
        serde_json::json!({"body": {"storage": {"value": storage}}, "version": {"number": 4}})
    }

    #[test]
    fn sections_are_numbered_by_heading_and_say_whether_knobas_edits_them() {
        let storage = "<p>Lead.</p>\
            <h2>Text</h2><p>One <br/>two.</p><ul><li>a</li><li>b</li></ul>\
            <h2>Table</h2><table><tr><td>x</td></tr></table>\
            <h2>Code</h2><ac:structured-macro ac:name=\"code\"><ac:plain-text-body><![CDATA[x]]></ac:plain-text-body></ac:structured-macro>\
            <h2>Panel</h2><ac:structured-macro ac:name=\"info\"><ac:rich-text-body><p>i</p></ac:rich-text-body></ac:structured-macro>\
            <h2>Bold</h2><p>a <strong>b</strong></p>\
            <h2>Mention</h2><p><ac:link><ri:user ri:username=\"mara\" /></ac:link></p>\
            <h2>Nested</h2><ul><li>a<ul><li>b</li></ul></li></ul>\
            <h2>Empty</h2>\
            <h2>Numbered</h2><ol><li>a</li></ol>\
            <h2>Item paragraphs</h2><ul><li><p>a</p><p>b</p></li></ul>";
        let got: Vec<(u32, Option<String>, Option<SectionRefusal>)> = sections(storage)
            .into_iter()
            .map(|s| (s.number, s.text, s.refusal))
            .collect();
        assert_eq!(
            got,
            vec![
                (1, Some("One\ntwo.\n\n- a\n- b".to_owned()), None),
                (2, None, Some(SectionRefusal::Table)),
                (3, None, Some(SectionRefusal::Code)),
                (4, Some("> **Info**\n>\n> i".to_owned()), None),
                (5, None, Some(SectionRefusal::Formatting)),
                (6, None, Some(SectionRefusal::Macro)),
                (7, None, Some(SectionRefusal::Formatting)),
                (8, Some(String::new()), None),
                (9, None, Some(SectionRefusal::Formatting)),
                (10, Some("- a b".to_owned()), None),
            ]
        );
    }

    /// Spec #2432: the editor's text has no checkbox, so a section holding a
    /// task list refuses with `formatting` rather than losing them.
    #[test]
    fn a_section_holding_a_task_list_refuses_as_formatting() {
        let list = format!(
            "<ac:task-list>{}</ac:task-list>",
            task(1, "incomplete", "Restart")
        );
        let storage = format!(
            "<h2>Tasks</h2><p>Lead.</p>{list}\
             <h2>In a panel</h2><ac:structured-macro ac:name=\"note\"><ac:rich-text-body>{list}</ac:rich-text-body></ac:structured-macro>\
             <h2>Plain</h2><ul><li>a</li></ul>"
        );
        let got: Vec<Option<SectionRefusal>> =
            sections(&storage).into_iter().map(|s| s.refusal).collect();
        assert_eq!(
            got,
            vec![
                Some(SectionRefusal::Formatting),
                Some(SectionRefusal::Formatting),
                None
            ]
        );
    }

    /// R-10.13, #975: a panel is edited as a quote that opens with its kind
    /// in bold (the Markdown export's spelling), its title beside the kind.
    #[test]
    fn a_section_with_a_panel_reads_as_a_quote_naming_its_kind() {
        let panel = |name: &str, inside: &str| {
            format!(
                "<ac:structured-macro ac:name=\"{name}\" ac:schema-version=\"1\" ac:macro-id=\"m1\">{inside}</ac:structured-macro>"
            )
        };
        let body = |inner: &str| format!("<ac:rich-text-body>{inner}</ac:rich-text-body>");
        let title = "<ac:parameter ac:name=\"title\">Before you deploy</ac:parameter>";
        let storage = [
            format!(
                "<h2>Mixed</h2><p>Lead.</p>{}<p>After.</p>",
                panel(
                    "warning",
                    &format!(
                        "{title}{}",
                        body("<p>Drain.</p><ul><li>a</li><li>b</li></ul>")
                    )
                )
            ),
            format!("<h2>Bare</h2>{}", panel("tip", "")),
            format!("<h2>Note</h2>{}", panel("note", &body("<p>n</p>"))),
            format!(
                "<h2>Icon</h2>{}",
                panel(
                    "info",
                    &format!(
                        "<ac:parameter ac:name=\"icon\">false</ac:parameter>{}",
                        body("<p>i</p>")
                    )
                )
            ),
            format!("<h2>Generic</h2>{}", panel("panel", &body("<p>p</p>"))),
            format!(
                "<h2>Nested</h2>{}",
                panel("info", &body(&panel("note", &body("<p>x</p>"))))
            ),
            format!(
                "<h2>Bold inside</h2>{}",
                panel("info", &body("<p><strong>b</strong></p>"))
            ),
            format!(
                "<h2>In a list</h2><ul><li>{}</li></ul>",
                panel("info", &body("<p>i</p>"))
            ),
        ]
        .concat();
        let got: Vec<(Option<String>, Option<SectionRefusal>)> = sections(&storage)
            .into_iter()
            .map(|s| (s.text, s.refusal))
            .collect();
        assert_eq!(
            got,
            vec![
                (
                    Some(
                        "Lead.\n\n> **Warning** Before you deploy\n>\n> Drain.\n>\n> - a\n> - b\n\nAfter."
                            .to_owned()
                    ),
                    None
                ),
                (Some("> **Tip**".to_owned()), None),
                (Some("> **Note**\n>\n> n".to_owned()), None),
                (None, Some(SectionRefusal::Macro)),
                (None, Some(SectionRefusal::Macro)),
                (None, Some(SectionRefusal::Macro)),
                (None, Some(SectionRefusal::Formatting)),
                (None, Some(SectionRefusal::Macro)),
            ]
        );
    }

    #[test]
    fn an_edit_writes_a_quote_naming_a_kind_back_as_that_panel() {
        let storage = "<h2>A</h2><p>old</p><h2>B</h2><p>b</p>";
        let text = "Lead.\n\n> **Warning** Before <you> deploy\n>\n> Drain.\n>\n> - a\n> - b\n\
                    \n> **tip**\n\n> Just a quote.\n> **Info** not first";
        assert_eq!(
            edit_section("page", &page(storage), 1, text).unwrap().body,
            "<h2>A</h2><p>Lead.</p>\
             <ac:structured-macro ac:name=\"warning\" ac:schema-version=\"1\">\
             <ac:parameter ac:name=\"title\">Before &lt;you&gt; deploy</ac:parameter>\
             <ac:rich-text-body><p>Drain.</p><ul><li>a</li><li>b</li></ul></ac:rich-text-body>\
             </ac:structured-macro>\
             <ac:structured-macro ac:name=\"tip\" ac:schema-version=\"1\">\
             <ac:rich-text-body></ac:rich-text-body></ac:structured-macro>\
             <p>&gt; Just a quote.<br/>&gt; **Info** not first</p>\
             <h2>B</h2><p>b</p>"
        );
    }

    /// Words that spell a panel's opening line are not a panel in the page,
    /// so the section refuses rather than save them back as one (#975).
    #[test]
    fn a_paragraph_spelled_like_a_panel_refuses_as_formatting() {
        let storage = "<h2>A</h2><p>&gt; **Info** not a panel</p>\
            <h2>B</h2><p>Lead.<br/><br/>&gt; **tip**</p>";
        let got: Vec<Option<SectionRefusal>> =
            sections(storage).into_iter().map(|s| s.refusal).collect();
        assert_eq!(
            got,
            vec![
                Some(SectionRefusal::Formatting),
                Some(SectionRefusal::Formatting),
            ]
        );
    }

    #[test]
    fn a_panel_section_saved_unchanged_reads_back_the_same() {
        let storage = "<h2>A</h2><p>Lead.</p><ac:structured-macro ac:name=\"info\" ac:macro-id=\"x\">\
            <ac:parameter ac:name=\"title\">T</ac:parameter><ac:rich-text-body><p>One<br/>two.</p>\
            <ul><li>a</li></ul></ac:rich-text-body></ac:structured-macro><p>After.</p>";
        let text = sections(storage)[0].text.clone().expect("editable");
        let saved = edit_section("page", &page(storage), 1, &text).unwrap().body;
        let again = sections(&saved);
        assert_eq!(again[0].text.as_deref(), Some(text.as_str()));
        assert_eq!(
            blocks(&saved),
            blocks(storage),
            "the reader draws the same page"
        );
    }

    #[test]
    fn the_numbering_is_the_readers() {
        // Every top-level heading block the projection draws is a section,
        // in its order, and nothing else is: the reader's anchors and the
        // edit address the same sections.
        let storage = "<h1>A</h1><p>x</p><ac:layout><ac:layout-section><ac:layout-cell><h2>B</h2><p>y</p></ac:layout-cell></ac:layout-section></ac:layout>\
            <ac:structured-macro ac:name=\"info\"><ac:rich-text-body><h3>Inside</h3></ac:rich-text-body></ac:structured-macro><h2>C</h2>";
        let headings = blocks(storage)
            .iter()
            .filter(|b| matches!(b, DocBlock::Heading { .. }))
            .count();
        let sections = sections(storage);
        assert_eq!(sections.len(), headings);
        assert_eq!(sections.len(), 3);
        assert_eq!(
            sections[0].refusal,
            Some(SectionRefusal::Macro),
            "A runs into a layout"
        );
        assert_eq!(
            sections[1].refusal,
            Some(SectionRefusal::Macro),
            "B is inside one"
        );
        assert_eq!(sections[2].text.as_deref(), Some(""));
    }

    #[test]
    fn an_edit_replaces_one_sections_body_and_keeps_the_rest_byte_for_byte() {
        let storage = "<p>Lead.</p><h2>Backoff</h2><p>old</p><h2>Table</h2><table><tr><td>x</td></tr></table>";
        let edit = edit_section(
            "page",
            &page(storage),
            1,
            "New <text> & more\n\n- one\n- two",
        )
        .unwrap();
        assert_eq!(edit.base_version, 4);
        assert_eq!(
            edit.body,
            "<p>Lead.</p><h2>Backoff</h2><p>New &lt;text&gt; &amp; more</p><ul><li>one</li><li>two</li></ul>\
             <h2>Table</h2><table><tr><td>x</td></tr></table>"
        );
        // The last section runs to the end of the body.
        let last = "<h2>A</h2><p>a</p><h2>B</h2><p>b</p>";
        assert_eq!(
            edit_section("page", &page(last), 2, "c").unwrap().body,
            "<h2>A</h2><p>a</p><h2>B</h2><p>c</p>"
        );
    }

    #[test]
    fn an_edit_is_refused_where_the_section_is_or_is_not_there() {
        let storage = "<h2>A</h2><table></table>";
        assert_eq!(
            edit_section("page", &page(storage), 1, "x"),
            Err(EditRefused::Section(SectionRefusal::Table))
        );
        assert_eq!(
            edit_section("page", &page(storage), 2, "x"),
            Err(EditRefused::Missing)
        );
        assert_eq!(
            edit_section("page", &page(storage), 0, "x"),
            Err(EditRefused::Missing)
        );
        assert_eq!(
            edit_section("blogpost", &page(storage), 1, "x"),
            Err(EditRefused::Missing)
        );
        let unversioned = serde_json::json!({"body": {"storage": {"value": "<h2>A</h2>"}}});
        assert_eq!(
            edit_section("page", &unversioned, 1, "x"),
            Err(EditRefused::Missing)
        );
    }

    #[test]
    fn history_is_this_version_and_the_creation() {
        let payload = serde_json::json!({
            "version": {"number": 7, "when": "2026-08-22T10:40:00Z", "message": "Cap at 5",
                        "by": {"username": "tomasz", "displayName": "Tomasz Wójcik"}},
            "history": {"createdBy": {"username": "mara", "displayName": "Mara Lindqvist"},
                        "createdDate": "2026-07-01T09:00:00Z"}
        });
        let h = history(&payload);
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].number, Some(7));
        assert_eq!(h[0].by_name.as_deref(), Some("Tomasz Wójcik"));
        assert_eq!(h[0].message.as_deref(), Some("Cap at 5"));
        assert_eq!(h[1].number, Some(1));
        assert_eq!(h[1].by.as_deref(), Some("mara"));
        assert_eq!(h[1].message, None);
    }
}
