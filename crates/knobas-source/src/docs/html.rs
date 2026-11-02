//! HTML in, the Docs reader's blocks out: the one tolerant reader every docs
//! source maps its markup through (spec #2432, *A shared HTML reader with a
//! hook*).
//!
//! The reader knows plain HTML and nothing else:
//!
//! | Element | Block or inline |
//! |---|---|
//! | `h1` to `h6` | heading |
//! | `p`, loose text | paragraph |
//! | `ul`, `ol` (nested too) | list, each `li` read as blocks |
//! | `table` with `tr`, `th` / `td` (through `thead`, `tbody`, `tfoot`) | table |
//! | `pre` | code, no language |
//! | `a href` | link |
//! | `strong`/`b`, `em`/`i`, `code`/`kbd`/`samp`/`tt` | text marks |
//! | `br` | line break |
//! | `script`, `style`, `head`, `template`, `noscript` | nothing |
//!
//! `div`, `section`, `blockquote`, `article`, `aside`, `header`, `footer`,
//! `main`, `nav`, `figure`, `figcaption`, `address`, `center`, `body`, `html`,
//! `dl`/`dt`/`dd` end the paragraph before them and read their children in
//! their place. `span`, `u`, `s`, `sup`, `sub`, `small`, `mark`, `abbr`,
//! `time` and the other inline wrappers in [`is_inline_wrapper`] read their
//! children in their place inside the sentence.
//!
//! # The hook
//!
//! Every other element goes to the [`Hook`] the source supplies, which
//! answers blocks, inlines, [`Hooked::Unwrap`] (read the children in its
//! place), or [`Hooked::Decline`]: a declined element is a
//! [`DocBlock::Placeholder`] named after it, so a part the reader cannot draw
//! says so rather than vanishing. The hook reads nested content back through
//! the reader with [`Reading::blocks`], so a source maps only what is its own.
//!
//! # Tolerant
//!
//! The markup is read into a small element tree ([`parse`]): an unmatched
//! close tag closes up to its opener or is ignored, an unclosed element ends
//! with the document, a `<` that never closes is text. HTML's implied end
//! tags are read as a browser reads them (`<p>a<p>b` is two paragraphs,
//! `<li>a<li>b` two items; likewise `dt`/`dd`, `tr`, `td`/`th`, `option`),
//! and elements past [`MAX_DEPTH`] read as the text they hold. It never fails: broken
//! markup reads as the text it holds. Pure, synchronous, no dependency beyond
//! `std`, so it compiles for `wasm32-wasip2` and the adapter SDK can carry it.

use super::{DocBlock, Inline, ListItem, TableCell};

// -- the tree -----------------------------------------------------------------

/// One node of the tolerant tree [`parse`] reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Element {
        /// Lowercased, namespace prefix kept (`ac:link`).
        name: String,
        /// The tag's inside, for attribute reads ([`Node::attr`]).
        tag: String,
        children: Vec<Node>,
        /// Where the element's open tag starts and where it ends (past its
        /// close tag, or where an unclosed element was cut), as byte offsets
        /// into the markup: what a section edit splices at.
        span: (usize, usize),
    },
    /// Character data, entities decoded.
    Text(String),
    /// A CDATA section's content, verbatim.
    Cdata(String),
}

impl Node {
    /// The element's name, `None` for text.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        match self {
            Node::Element { name, .. } => Some(name),
            _ => None,
        }
    }

    /// One quoted attribute's raw value (entities not decoded).
    #[must_use]
    pub fn attr(&self, key: &str) -> Option<&str> {
        match self {
            Node::Element { tag, .. } => attr(tag, key),
            _ => None,
        }
    }

    /// The element's children; none for text.
    #[must_use]
    pub fn children(&self) -> &[Node] {
        match self {
            Node::Element { children, .. } => children,
            _ => &[],
        }
    }

    /// The first child element of this name.
    #[must_use]
    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children().iter().find(|c| c.name() == Some(name))
    }

    /// All the text below this node, CDATA included.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Node::Text(t) | Node::Cdata(t) => t.clone(),
            Node::Element { children, .. } => children.iter().map(Node::text).collect(),
        }
    }
}

/// Elements that never have children.
fn is_void(name: &str) -> bool {
    matches!(
        name,
        "br" | "hr" | "img" | "col" | "input" | "meta" | "link" | "wbr" | "source" | "area"
    )
}

/// How deep [`parse`] nests elements. Past it a start tag is dropped and
/// what it holds reads as text in the innermost element, so the tree, the
/// reader's walks over it and a hook's [`Reading::blocks`] stay bounded.
/// Real pages, Confluence macros in layouts included, nest a few dozen deep.
pub const MAX_DEPTH: usize = 128;

/// One open element on the parse stack: name, tag, children, start offset.
type Open = (String, String, Vec<Node>, usize);

/// Markup as a tolerant element tree. Never fails.
#[must_use]
pub fn parse(markup: &str) -> Vec<Node> {
    // The stack of open elements; the bottom is the document.
    let mut stack: Vec<Open> = vec![(String::new(), String::new(), vec![], 0)];
    let mut rest = markup;
    let at = |rest: &str| markup.len() - rest.len();
    let push = |stack: &mut Vec<Open>, node: Node| {
        stack
            .last_mut()
            .expect("the document is never popped")
            .2
            .push(node);
    };
    let close = |stack: &mut Vec<Open>, end: usize| {
        let (name, tag, children, start) = stack.pop().expect("only called above the document");
        stack
            .last_mut()
            .expect("the document is never popped")
            .2
            .push(Node::Element {
                name,
                tag,
                children,
                span: (start, end),
            });
    };
    while !rest.is_empty() {
        let Some(open) = rest.find('<') else {
            push(&mut stack, Node::Text(decode_entities(rest)));
            rest = "";
            break;
        };
        if open > 0 {
            push(&mut stack, Node::Text(decode_entities(&rest[..open])));
        }
        rest = &rest[open..];
        let start = at(rest);
        if let Some(tail) = rest.strip_prefix("<![CDATA[") {
            let (inside, tail) = tail.split_once("]]>").unwrap_or((tail, ""));
            push(&mut stack, Node::Cdata(inside.to_owned()));
            rest = tail;
            continue;
        }
        if let Some(tail) = rest.strip_prefix("<!--") {
            rest = tail.split_once("-->").map_or("", |(_, tail)| tail);
            continue;
        }
        let Some((tag, tail)) = split_tag(rest) else {
            push(&mut stack, Node::Text(decode_entities(rest)));
            rest = "";
            break;
        };
        rest = tail;
        let name = tag_name(tag);
        if name.is_empty() || tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        if tag.starts_with('/') {
            // Close up to the matching opener; a close with no opener is
            // dropped. An element closed by an ancestor's close tag ends where
            // that tag starts.
            if let Some(depth) = stack.iter().rposition(|(open, _, _, _)| *open == name)
                && depth > 0
            {
                while stack.len() > depth + 1 {
                    close(&mut stack, start);
                }
                close(&mut stack, at(rest));
            }
            continue;
        }
        if tag.ends_with('/') || is_void(&name) {
            push(
                &mut stack,
                Node::Element {
                    name,
                    tag: tag.to_owned(),
                    children: vec![],
                    span: (start, at(rest)),
                },
            );
            continue;
        }
        if matches!(name.as_str(), "script" | "style") {
            // Raw text: a `<` in a script opens nothing and closes nothing, so
            // the contents run to the first `</script` (any case). ASCII
            // lowercasing keeps every byte offset.
            let end_tag = format!("</{name}");
            let inside = rest
                .to_ascii_lowercase()
                .find(&end_tag)
                .unwrap_or(rest.len());
            let children = if inside == 0 {
                vec![]
            } else {
                vec![Node::Text(rest[..inside].to_owned())]
            };
            rest = split_tag(&rest[inside..]).map_or("", |(_, tail)| tail);
            push(
                &mut stack,
                Node::Element {
                    name,
                    tag: tag.to_owned(),
                    children,
                    span: (start, at(rest)),
                },
            );
            continue;
        }
        // HTML leaves some end tags out: an open `p` ends where a block
        // starts, an `li` at the next `li`. The element ends where this tag
        // starts, as when an ancestor's close tag ends it.
        for rule in implied_ends(&name) {
            if let Some(depth) = implied_end(&stack, rule) {
                while stack.len() > depth {
                    close(&mut stack, start);
                }
            }
        }
        if stack.len() > MAX_DEPTH {
            // Past the depth the tag is dropped and its contents join the
            // innermost open element: every walk over the tree recurses
            // once per level, and markup anyone can author must not
            // overflow the stack. Its close tag finds no opener.
            continue;
        }
        stack.push((name, tag.to_owned(), vec![], start));
    }
    let end = at(rest);
    while stack.len() > 1 {
        close(&mut stack, end);
    }
    stack
        .pop()
        .map(|(_, _, nodes, _)| nodes)
        .unwrap_or_default()
}

/// Start tags that end an open `p` (HTML's *in body* rules).
fn ends_paragraph(name: &str) -> bool {
    matches!(
        name,
        "address"
            | "article"
            | "aside"
            | "blockquote"
            | "center"
            | "details"
            | "dialog"
            | "dir"
            | "div"
            | "dl"
            | "dd"
            | "dt"
            | "fieldset"
            | "figcaption"
            | "figure"
            | "footer"
            | "form"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "header"
            | "hgroup"
            | "hr"
            | "li"
            | "listing"
            | "main"
            | "menu"
            | "nav"
            | "ol"
            | "p"
            | "pre"
            | "search"
            | "section"
            | "summary"
            | "table"
            | "ul"
            | "xmp"
    )
}

/// Which open elements a start tag ends, and the open elements its search
/// stops at: `(ends, stops)`.
type Rule = (&'static [&'static str], &'static [&'static str]);

/// The scope HTML searches for an open `p`: a table, a cell or a widget is
/// a fresh start.
const SCOPE: &[&str] = &[
    "table", "td", "th", "caption", "template", "button", "object", "select",
];

/// Where HTML's search for an open `li`, `dt` or `dd` stops: any of its
/// *special* elements but `address`, `div` and `p` (the void, raw-text and
/// never-open ones left out), so `<li>a<blockquote><li>b` nests the second
/// item in the quote, as a browser does.
const ITEM_STOPS: &[&str] = &[
    "applet", "article", "aside", "blockquote", "body", "button", "caption", "center",
    "colgroup", "details", "dir", "dl", "fieldset", "figcaption", "figure", "footer", "form",
    "frameset", "h1", "h2", "h3", "h4", "h5", "h6", "head", "header", "hgroup", "html",
    "iframe", "listing", "main", "marquee", "menu", "nav", "noembed", "noframes", "noscript",
    "object", "ol", "plaintext", "pre", "search", "section", "select", "summary", "table",
    "tbody", "td", "template", "textarea", "tfoot", "th", "thead", "title", "tr", "ul", "xmp",
    "li", "dt", "dd",
];

/// The implied-end rules a start tag `name` applies, in order: an `li`
/// first ends an open `p`, then the `li` before it.
fn implied_ends(name: &str) -> impl Iterator<Item = Rule> {
    let paragraph: Rule = (&["p"], SCOPE);
    let own: Option<Rule> = match name {
        "li" => Some((&["li"], ITEM_STOPS)),
        "dt" | "dd" => Some((&["dt", "dd"], ITEM_STOPS)),
        "tr" => Some((&["tr"], &["table", "template"])),
        "td" | "th" => Some((&["td", "th"], &["tr", "table", "template"])),
        "option" => Some((&["option"], &["select", "datalist", "optgroup"])),
        _ => None,
    };
    ends_paragraph(name)
        .then_some(paragraph)
        .into_iter()
        .chain(own)
}

/// The stack index of the open element `rule` ends, `None` when there is
/// none in scope.
///
/// The search runs down from the innermost open element and stops at the
/// rule's stops (an outer list's `li` is not ended by an inner list's `li`,
/// an outer table's cell not by an inner table's) and at any namespaced
/// element, so a Confluence macro body (`ac:rich-text-body`) inside a
/// paragraph keeps its own paragraphs.
fn implied_end(stack: &[Open], (ends, stops): Rule) -> Option<usize> {
    for (depth, (open, ..)) in stack.iter().enumerate().skip(1).rev() {
        if ends.contains(&open.as_str()) {
            return Some(depth);
        }
        if stops.contains(&open.as_str()) || open.contains(':') {
            return None;
        }
    }
    None
}

// -- tags and entities ----------------------------------------------------------

/// Split `<…>` off the front of `rest`, respecting quoted attribute values.
///
/// `<img alt="a > b"/>` is one tag, and a scan to the first `>` would cut it
/// in half and spill `b"/>` into the text. Returns the tag's inside and what
/// follows it, or `None` for a `<` that never closes.
#[must_use]
pub fn split_tag(rest: &str) -> Option<(&str, &str)> {
    let mut quote: Option<char> = None;
    for (i, ch) in rest.char_indices().skip(1) {
        match quote {
            Some(open) if ch == open => quote = None,
            Some(_) => {}
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch == '>' => return Some((&rest[1..i], &rest[i + ch.len_utf8()..])),
            None => {}
        }
    }
    None
}

/// One attribute's value out of a tag body, or `None` for an attribute that is
/// absent or unquoted.
///
/// The name must be **preceded by whitespace and followed by `=`**, because
/// `ri:username` contains `ri:user` and a substring search for one would find
/// the other. Whitespace and not "or the start of the tag": the first token in
/// a tag body is the element name, never an attribute. Deliberately total: a
/// malformed tag yields no attribute rather than an error.
#[must_use]
pub fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    if name.is_empty() {
        // `find("")` matches at 0 forever: no name, no attribute.
        return None;
    }
    let mut rest = tag;
    loop {
        let at = rest.find(name)?;
        let before_ok = at > 0
            && rest[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_whitespace());
        let after = &rest[at + name.len()..];
        let value = after.trim_start();
        if before_ok && value.starts_with('=') {
            let value = value[1..].trim_start();
            let quote = value.chars().next().filter(|c| *c == '"' || *c == '\'')?;
            let value = &value[quote.len_utf8()..];
            return value.find(quote).map(|end| &value[..end]);
        }
        rest = after;
    }
}

/// The element name inside a tag body, lowercased and without its `/`.
#[must_use]
pub fn tag_name(tag: &str) -> String {
    tag.trim_start_matches('/')
        .split(|c: char| c.is_ascii_whitespace() || c == '/' || c == '>')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// The XML entities markup carries, plus numeric references.
///
/// `&nbsp;` becomes an ordinary space rather than U+00A0: a non-breaking space
/// in search text or a reader's paragraph is a character no search will ever
/// be typed with. An entity the reader does not know stays as written.
#[must_use]
pub fn decode_entities(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some((entity, tail)) = rest[1..]
            .find(';')
            .filter(|end| *end <= 12)
            .map(|end| (&rest[1..=end], &rest[end + 2..]))
        else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        match entity {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" | "#39" => out.push('\''),
            "nbsp" | "#160" => out.push(' '),
            _ => match numeric(entity) {
                Some(ch) => out.push(ch),
                None => {
                    out.push('&');
                    out.push_str(entity);
                    out.push(';');
                }
            },
        }
        rest = tail;
    }
    out.push_str(rest);
    out
}

/// `&#8212;` / `&#x2014;`: a dash, a quote or an ellipsis by number.
fn numeric(entity: &str) -> Option<char> {
    let digits = entity.strip_prefix('#')?;
    let code = match digits.strip_prefix(['x', 'X']) {
        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
        None => digits.parse::<u32>().ok()?,
    };
    char::from_u32(code)
}

// -- the hook -----------------------------------------------------------------

/// What a [`Hook`] answers for an element the reader does not know.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hooked {
    /// Blocks drawn where the element stood. Inside a sentence, the sentence
    /// stops there and goes on after them.
    Blocks(Vec<DocBlock>),
    /// Inlines inside the sentence the element sits in. Between blocks, they
    /// start or join a paragraph.
    Inlines(Vec<Inline>),
    /// Read the element's children in its place, as though the tag were not
    /// there.
    Unwrap,
    /// The reader draws a [`DocBlock::Placeholder`] named after the element.
    Decline,
}

/// The source's mapping for the elements plain HTML does not have (a
/// Confluence `ac:` macro, an XWiki macro marker).
pub trait Hook {
    /// Answer for `element`, which the reader does not know. `reading` reads
    /// nested content back through the reader, under the marks the element
    /// sits in.
    fn element(&self, element: &Node, reading: &Reading<'_>) -> Hooked;
}

/// No mapping of its own: every element the reader does not know is declined.
#[derive(Debug, Clone, Copy, Default)]
pub struct Plain;

impl Hook for Plain {
    fn element(&self, _element: &Node, _reading: &Reading<'_>) -> Hooked {
        Hooked::Decline
    }
}

/// The text marks in force where an element sits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Marks {
    pub strong: bool,
    pub emphasis: bool,
    pub code: bool,
}

/// The reader as a [`Hook`] sees it: nested content read back through it,
/// and the marks in force.
pub struct Reading<'a> {
    hook: &'a dyn Hook,
    marks: Marks,
}

impl Reading<'_> {
    /// `nodes` read as blocks, the hook asked again for what it maps.
    #[must_use]
    pub fn blocks(&self, nodes: &[Node]) -> Vec<DocBlock> {
        blocks(nodes, self.hook)
    }

    /// The marks the element sits under.
    #[must_use]
    pub fn marks(&self) -> Marks {
        self.marks
    }

    /// Words under the marks the element sits in.
    #[must_use]
    pub fn text(&self, text: impl Into<String>) -> Inline {
        Inline::Text {
            text: text.into(),
            strong: self.marks.strong,
            emphasis: self.marks.emphasis,
            code: self.marks.code,
        }
    }
}

/// A placeholder for the source's own name of a construct; `unnamed` for an
/// empty name, so a placeholder always says something.
#[must_use]
pub fn placeholder(name: &str) -> DocBlock {
    DocBlock::Placeholder {
        name: if name.is_empty() {
            "unnamed".into()
        } else {
            name.to_owned()
        },
    }
}

// -- the reader -----------------------------------------------------------------

/// Markup as the reader's blocks, `hook` asked for every element plain HTML
/// does not have. Never fails.
#[must_use]
pub fn to_blocks(markup: &str, hook: &dyn Hook) -> Vec<DocBlock> {
    blocks(&parse(markup), hook)
}

/// An already parsed tree as the reader's blocks.
#[must_use]
pub fn blocks(nodes: &[Node], hook: &dyn Hook) -> Vec<DocBlock> {
    let mut out = Blocks::new(hook);
    out.flow(nodes);
    out.finish()
}

/// Wrappers between blocks: the paragraph before them ends, their children
/// are read in their place.
fn is_block_wrapper(name: &str) -> bool {
    matches!(
        name,
        "div"
            | "section"
            | "blockquote"
            | "article"
            | "aside"
            | "header"
            | "footer"
            | "main"
            | "nav"
            | "figure"
            | "figcaption"
            | "address"
            | "center"
            | "body"
            | "html"
            | "dl"
            | "dt"
            | "dd"
            | "details"
            | "summary"
    )
}

/// Wrappers inside a sentence: their children are read in their place.
fn is_inline_wrapper(name: &str) -> bool {
    matches!(
        name,
        "span"
            | "u"
            | "s"
            | "strike"
            | "del"
            | "ins"
            | "sup"
            | "sub"
            | "small"
            | "big"
            | "mark"
            | "abbr"
            | "acronym"
            | "cite"
            | "dfn"
            | "q"
            | "time"
            | "font"
            | "label"
            | "bdi"
            | "bdo"
            | "var"
            | "nobr"
    )
}

/// Elements whose contents are never words on the page.
fn is_silent(name: &str) -> bool {
    matches!(
        name,
        "script" | "style" | "head" | "title" | "template" | "noscript"
    )
}

/// Every element the reader handles itself; the rest go to the hook.
fn is_known(name: &str) -> bool {
    is_block_wrapper(name)
        || is_inline_wrapper(name)
        || is_silent(name)
        || matches!(
            name,
            "h1" | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "p"
                | "ul"
                | "ol"
                | "li"
                | "table"
                | "thead"
                | "tbody"
                | "tfoot"
                | "tr"
                | "th"
                | "td"
                | "caption"
                | "colgroup"
                | "col"
                | "pre"
                | "hr"
                | "br"
                | "a"
                | "strong"
                | "b"
                | "em"
                | "i"
                | "code"
                | "kbd"
                | "samp"
                | "tt"
        )
}

/// The block walk, with a paragraph collecting loose inline content.
struct Blocks<'a> {
    hook: &'a dyn Hook,
    out: Vec<DocBlock>,
    loose: Vec<Inline>,
}

impl<'a> Blocks<'a> {
    fn new(hook: &'a dyn Hook) -> Self {
        Blocks {
            hook,
            out: Vec::new(),
            loose: Vec::new(),
        }
    }

    fn finish(mut self) -> Vec<DocBlock> {
        self.flush();
        self.out
    }

    fn flush(&mut self) {
        let text = tidy_inlines(std::mem::take(&mut self.loose));
        if !text.is_empty() {
            self.out.push(DocBlock::Paragraph { text });
        }
    }

    fn block(&mut self, block: DocBlock) {
        self.flush();
        self.out.push(block);
    }

    fn flow(&mut self, nodes: &[Node]) {
        for node in nodes {
            self.node(node);
        }
    }

    fn node(&mut self, node: &Node) {
        let Node::Element { name, children, .. } = node else {
            // Loose text between blocks is a paragraph's.
            self.inline(node);
            return;
        };
        match name.as_str() {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level = name[1..].parse().unwrap_or(2);
                let mut inl = Inlines::new(self.hook);
                inl.flow(children, Marks::default());
                let (text, escaped) = inl.finish();
                self.block(DocBlock::Heading {
                    level,
                    text: tidy_inlines(text),
                });
                for block in escaped {
                    self.block(block);
                }
            }
            "p" => {
                self.flush();
                let mut inl = Inlines::new(self.hook);
                inl.flow(children, Marks::default());
                self.take(inl);
                self.flush();
            }
            "ul" | "ol" => {
                let items = children
                    .iter()
                    .filter(|c| c.name() == Some("li"))
                    .map(|li| ListItem::new(blocks(li.children(), self.hook)))
                    .collect();
                self.block(DocBlock::List {
                    ordered: name == "ol",
                    items,
                });
            }
            "table" => self.block(DocBlock::Table {
                rows: self.rows(children),
            }),
            "pre" => self.block(DocBlock::Code {
                code: node.text(),
                language: None,
            }),
            "hr" => self.flush(),
            n if is_silent(n) => {}
            n if is_block_wrapper(n) => {
                self.flush();
                self.flow(children);
                self.flush();
            }
            n if is_known(n) => self.inline(node),
            _ => match self.hook.element(
                node,
                &Reading {
                    hook: self.hook,
                    marks: Marks::default(),
                },
            ) {
                Hooked::Blocks(blocks) => {
                    for block in blocks {
                        self.block(block);
                    }
                }
                Hooked::Inlines(inlines) => self.loose.extend(inlines),
                Hooked::Unwrap => self.flow(children),
                Hooked::Decline => self.block(placeholder(name)),
            },
        }
    }

    /// One node read as inline content of the loose paragraph.
    fn inline(&mut self, node: &Node) {
        let mut inl = Inlines::new(self.hook);
        inl.node(node, Marks::default());
        self.take(inl);
    }

    /// Fold an inline walk into the loose paragraph; a block it had to step
    /// out for (a hooked block inside a sentence) ends the paragraph there.
    fn take(&mut self, inl: Inlines<'_>) {
        for piece in inl.pieces {
            match piece {
                Piece::Inline(i) => self.loose.push(i),
                Piece::Block(b) => self.block(b),
            }
        }
    }

    fn rows(&self, nodes: &[Node]) -> Vec<Vec<TableCell>> {
        let mut rows = Vec::new();
        for node in nodes {
            match node.name() {
                Some("tr") => rows.push(
                    node.children()
                        .iter()
                        .filter_map(|cell| {
                            let header = match cell.name() {
                                Some("th") => true,
                                Some("td") => false,
                                _ => return None,
                            };
                            Some(TableCell {
                                header,
                                blocks: blocks(cell.children(), self.hook),
                                colspan: span(cell, "colspan"),
                                rowspan: span(cell, "rowspan"),
                            })
                        })
                        .collect(),
                ),
                Some("thead" | "tbody" | "tfoot") => rows.extend(self.rows(node.children())),
                _ => {}
            }
        }
        rows
    }
}

/// A cell's `colspan` or `rowspan`: a positive whole number, else 1 (HTML's
/// `rowspan="0"`, "to the end of the group", reads as 1 here), at most what
/// HTML's own table model takes (1,000 columns, 65,534 rows).
fn span(cell: &Node, key: &str) -> u32 {
    let most = if key == "colspan" { 1_000 } else { 65_534 };
    cell.attr(key)
        .and_then(|v| v.trim().parse::<u32>().ok())
        .filter(|n| *n > 0)
        .map_or(1, |n| n.min(most))
}

// -- inlines ------------------------------------------------------------------

enum Piece {
    Inline(Inline),
    Block(DocBlock),
}

struct Inlines<'a> {
    hook: &'a dyn Hook,
    pieces: Vec<Piece>,
}

impl<'a> Inlines<'a> {
    fn new(hook: &'a dyn Hook) -> Self {
        Inlines {
            hook,
            pieces: Vec::new(),
        }
    }

    fn finish(self) -> (Vec<Inline>, Vec<DocBlock>) {
        let mut text = Vec::new();
        let mut blocks = Vec::new();
        for piece in self.pieces {
            match piece {
                Piece::Inline(i) => text.push(i),
                Piece::Block(b) => blocks.push(b),
            }
        }
        (text, blocks)
    }

    fn push(&mut self, inline: Inline) {
        self.pieces.push(Piece::Inline(inline));
    }

    fn flow(&mut self, nodes: &[Node], marks: Marks) {
        for node in nodes {
            self.node(node, marks);
        }
    }

    fn node(&mut self, node: &Node, marks: Marks) {
        let (name, children) = match node {
            Node::Text(text) | Node::Cdata(text) => {
                self.push(Inline::Text {
                    text: text.clone(),
                    strong: marks.strong,
                    emphasis: marks.emphasis,
                    code: marks.code,
                });
                return;
            }
            Node::Element { name, children, .. } => (name.as_str(), children),
        };
        match name {
            "strong" | "b" => self.flow(
                children,
                Marks {
                    strong: true,
                    ..marks
                },
            ),
            "em" | "i" => self.flow(
                children,
                Marks {
                    emphasis: true,
                    ..marks
                },
            ),
            "code" | "kbd" | "samp" | "tt" => self.flow(
                children,
                Marks {
                    code: true,
                    ..marks
                },
            ),
            "br" => self.push(Inline::Break),
            "a" => {
                let text = collapse(&node.text());
                match node.attr("href").filter(|h| !h.trim().is_empty()) {
                    Some(href) => self.push(Inline::Link {
                        text: if text.is_empty() {
                            href.to_owned()
                        } else {
                            text
                        },
                        href: decode_entities(href),
                    }),
                    None => self.flow(children, marks),
                }
            }
            n if is_silent(n) => {}
            // Anything else plain HTML has (a wrapper, or a block met inside a
            // sentence) reads as its words.
            n if is_known(n) => self.flow(children, marks),
            _ => match self.hook.element(
                node,
                &Reading {
                    hook: self.hook,
                    marks,
                },
            ) {
                Hooked::Blocks(blocks) => {
                    // A block inside a sentence: the sentence stops and the
                    // block is drawn where it stood.
                    self.pieces.extend(blocks.into_iter().map(Piece::Block));
                }
                Hooked::Inlines(inlines) => {
                    self.pieces.extend(inlines.into_iter().map(Piece::Inline));
                }
                Hooked::Unwrap => self.flow(children, marks),
                Hooked::Decline => self.pieces.push(Piece::Block(placeholder(name))),
            },
        }
    }
}

/// Whitespace runs as one space, the ends trimmed.
#[must_use]
pub fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Collapse whitespace runs across a paragraph's text runs, drop empty runs
/// and trim the ends, so source indentation never reaches the reader.
fn tidy_inlines(inlines: Vec<Inline>) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::new();
    let mut space_before = true; // at the start, leading space is dropped
    for inline in inlines {
        match inline {
            Inline::Text {
                text,
                strong,
                emphasis,
                code,
            } => {
                let mut squeezed = String::with_capacity(text.len());
                for ch in text.chars() {
                    if ch.is_whitespace() {
                        if !space_before {
                            squeezed.push(' ');
                            space_before = true;
                        }
                    } else {
                        squeezed.push(ch);
                        space_before = false;
                    }
                }
                if squeezed.is_empty() {
                    continue;
                }
                // Merge with a neighbour carrying the same marks.
                if let Some(Inline::Text {
                    text: prev,
                    strong: s,
                    emphasis: e,
                    code: c,
                }) = out.last_mut()
                    && *s == strong
                    && *e == emphasis
                    && *c == code
                {
                    prev.push_str(&squeezed);
                    continue;
                }
                out.push(Inline::Text {
                    text: squeezed,
                    strong,
                    emphasis,
                    code,
                });
            }
            Inline::Break => {
                trim_end(&mut out);
                out.push(Inline::Break);
                space_before = true;
            }
            other => {
                out.push(other);
                space_before = false;
            }
        }
    }
    trim_end(&mut out);
    while matches!(out.last(), Some(Inline::Break)) {
        out.pop();
        trim_end(&mut out);
    }
    out
}

fn trim_end(out: &mut Vec<Inline>) {
    if let Some(Inline::Text { text, .. }) = out.last_mut() {
        let trimmed = text.trim_end().len();
        text.truncate(trimmed);
        if text.is_empty() {
            out.pop();
        }
    }
}

#[cfg(test)]
mod tests;
