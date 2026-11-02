//! The fixture's docs source (1.5.0, #2442, ADR-0040): two pages of the
//! *Harbour* space, and the projection that draws them.
//!
//! A page's payload carries its body as HTML, read with the shared reader
//! the SDK re-exports ([`knobas_adapter_sdk::html`]), and the fixture's own elements, all in the
//! `hb:` namespace, through its hook, so that between them the two pages
//! hold every block and every inline of the docs vocabulary:
//!
//! * `hb:panel kind title`, `hb:expand title`, `hb:tasks` of `hb:task done`,
//!   `hb:include title space excerpt`, `hb:html` (its text is the markup),
//!   `hb:frame src`, `hb:excerpt` (its blocks, and the page's excerpt);
//! * `hb:page title space`, `hb:issue key`, `hb:mention user name`;
//! * any other `hb:` element is declined, so it is a named placeholder.
//!
//! What knobas fills at read time the fixture fills too when told to, as a
//! careless or hostile adapter would, so a test sees the host clear it:
//! `entity` on `hb:page`, `hb:issue` and `hb:include`, `status` on
//! `hb:issue`, `me="true"` on `hb:mention`.
//!
//! Every page ends with [`DRAWN_BY`], a sentence only the projection writes:
//! a test patches it in the component's bytes to show that a new component
//! redraws a mirrored page without a re-sync.
//!
//! **The writes** (#2443): a section is what follows a top-level heading up
//! to the next one, refused at the first part a text box cannot hold (a
//! table, code, an `hb:` element, marks or links), else edited as plain
//! text: a paragraph per block and `- ` per item, which the SDK's
//! [`html_from_text`] writes back as markup, as it makes a new page's body.
//! The section edit misbehaves on the same payload keys as the projection.
//!
//! **The docs contract's breaches** (#2444), by the instance's
//! `"docs_breach"`, which the sync writes into each page's payload as
//! `"breach"`: `draws_any_kind` draws a page's payload as whatever kind it
//! is asked for, `numbers_from_zero` numbers the sections from 0,
//! `edits_refused_sections` edits a section the projection refuses, and
//! `edits_another_version` edits against the version after the payload's.
//!
//! **The misbehaving projections**, by payload key, for the host's limits:
//! `"fetch": "<url>"` sends a request first and traps when it is refused,
//! `"trap": true` traps, `"spin": true` never returns, `"hog": true`
//! allocates until the memory cap stops it.

use chrono::{TimeZone, Utc};
use knobas_adapter_sdk::SyncItem;
use knobas_adapter_sdk::html::{self, Node};
use knobas_adapter_sdk::spi::docs::{
    DocBlock, DocComment, DocContent, DocSection, EditRefused, EmbedContent, Inline, ListItem,
    PAGE_KIND, PanelKind, SectionEdit, SectionRefusal,
};
use knobas_adapter_sdk::spi::role::{RoleClaim, WIKI};
use knobas_adapter_sdk::spi::{EntityRef, KindInfo, KindPaths, ParentPath, PayloadPath};
use knobas_adapter_sdk::{Hook, Hooked, Reading, html_from_text};
use serde_json::{Value, json};

/// The space both pages are in.
pub const SPACE: &str = "HARB";

/// The sentence the projection ends every page with.
pub const DRAWN_BY: &str = "Drawn by the quay projection.";

/// What the fixture says a page's search text is: the host ignores it
/// (ADR-0040), so a test finds none of these words in the mirror.
pub const SENT_BODY_TEXT: &str = "unindexed words the fixture sent for this page";

/// The handbook: every block but the excerpt, and an include of the rules'
/// excerpt and of the rules whole.
const HANDBOOK: &str = r#"<h2>Arrival</h2>
<p>Pilots board at <strong>buoy 3</strong>, <em>weather</em> <code>permitting</code>.<br/>Call <a href="https://harbour.example/vhf">channel 12</a>, see <hb:page title="Berthing rules"></hb:page>, ticket <hb:issue key="HARB-7"></hb:issue>, ask <hb:mention user="harbourmaster" name="Hal Master"></hb:mention>.</p>
<ul><li>Moor fore and aft</li><li>Report draught<ol><li>at the pier head</li></ol></li></ul>
<hb:tasks><hb:task done="true">Fenders checked</hb:task><hb:task done="false">Gangway rigged</hb:task></hb:tasks>
<pre>VHF 12 / 16</pre>
<table><tr><th colspan="2">Berths</th></tr><tr><td>7</td><td><ul><li>deep water</li></ul></td></tr></table>
<hb:panel kind="warning" title="Swell"><p>Keep clear of the breakwater.</p></hb:panel>
<hb:expand title="Tide tables"><p>Spring tides run at four knots.</p></hb:expand>
<hb:include title="Berthing rules" excerpt="true"></hb:include>
<hb:include title="Berthing rules" space="HARB"></hb:include>
<hb:html><![CDATA[<p>Live crane board</p><script>window.cranes = 4;</script>]]></hb:html>
<hb:frame src="https://harbour.example/webcam"></hb:frame>
<hb:frame src="javascript:alert(1)"></hb:frame>
<hb:weather></hb:weather>"#;

/// The rules: a page with an excerpt, and a section the editor edits.
const RULES: &str = r"<h2>Speed</h2>
<hb:excerpt><p>Five knots inside the breakwater.</p></hb:excerpt>
<p>Overtaking is forbidden.</p>
<h2>Mooring</h2>
<p>Two lines, fore and aft.</p>";

/// `(key, title, body, parent title)` per page, in sync order.
const PAGES: [(&str, &str, &str, Option<&str>); 2] = [
    ("page-handbook", "Harbour handbook", HANDBOOK, None),
    (
        "page-rules",
        "Berthing rules",
        RULES,
        Some("Harbour handbook"),
    ),
];

/// The `page` kind.
pub fn kind() -> KindInfo {
    KindInfo {
        id: PAGE_KIND.into(),
        label: "Page".into(),
        plural: "Pages".into(),
        monogram: "PG".into(),
        full_sync_exhaustive: true,
        refreshes: true,
    }
}

/// Where a page keeps its space and its parent, as the wiki role needs.
pub fn payload_paths() -> KindPaths {
    KindPaths {
        kind: PAGE_KIND.into(),
        project_key: vec![PayloadPath::of(["space", "key"])],
        project_name: vec![PayloadPath::of(["space", "name"])],
        parent: vec![ParentPath {
            kind: PAGE_KIND.into(),
            at: PayloadPath::of(["ancestors"]),
            names: vec![PayloadPath::of(["title"])],
        }],
        ..KindPaths::default()
    }
}

/// The wiki claim.
pub fn role() -> RoleClaim {
    RoleClaim::new(WIKI, ["doc_content"])
}

/// A page's payload, as the sync emits it.
fn payload(key: &str, title: &str, body: &str, parent: Option<&str>) -> Value {
    let ancestors: Vec<Value> = parent.map(|t| json!({ "title": t })).into_iter().collect();
    json!({
        "key": key,
        "title": title,
        "space": { "key": SPACE, "name": "Harbour" },
        "ancestors": ancestors,
        "version": 3,
        "editor": "Hal Master",
        "labels": ["harbour"],
        "body": body,
        "comments": [
            { "id": "c1", "author": "pilot", "name": "Pia Lot", "body": "<p>Buoy 3 is lit again.</p>" },
        ],
    })
}

/// Both pages, in sync order, for source `id`, breaking the docs contract
/// the `breach` way if one is named.
pub fn pages(id: &str, breach: Option<&str>) -> Vec<SyncItem> {
    PAGES
        .iter()
        .map(|(key, title, body, parent)| SyncItem {
            entity: EntityRef::new(id, key),
            kind: PAGE_KIND.into(),
            title: (*title).into(),
            body_text: SENT_BODY_TEXT.into(),
            author: Some("harbourmaster".into()),
            updated_at: Utc.with_ymd_and_hms(2026, 11, 2, 9, 0, 0).single(),
            payload: {
                let mut payload = payload(key, title, body, *parent);
                if let Some(breach) = breach {
                    payload["breach"] = breach.into();
                }
                payload
            },
            web_url: None,
            deleted: false,
        })
        .collect()
}

/// The projection: a `page` payload drawn, `None` for any other kind or a
/// payload with no body.
pub fn project(kind: &str, payload: &Value) -> Option<DocContent> {
    if kind != PAGE_KIND && !breaks(payload, "draws_any_kind") {
        return None;
    }
    misbehave(payload);
    let body = payload["body"].as_str()?;
    let nodes = html::parse(body);
    let mut blocks = html::blocks(&nodes, &Harbour);
    blocks.push(DocBlock::Paragraph {
        text: vec![Inline::text(DRAWN_BY)],
    });
    let excerpt = find(&nodes, "hb:excerpt").map(|node| html::blocks(node.children(), &Harbour));
    let comments = payload["comments"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| DocComment {
            id: c["id"].as_str().map(str::to_owned),
            author: c["author"].as_str().map(str::to_owned),
            author_name: c["name"].as_str().map(str::to_owned),
            at: Some("2026-11-02T09:30:00Z".into()),
            blocks: html::to_blocks(c["body"].as_str().unwrap_or_default(), &Harbour),
        })
        .collect();
    Some(DocContent {
        version: payload["version"].as_u64(),
        editor_name: payload["editor"].as_str().map(str::to_owned),
        labels: payload["labels"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|l| l.as_str().map(str::to_owned))
            .collect(),
        blocks,
        comments,
        excerpt,
        sections: sections(body, breaks(payload, "numbers_from_zero")),
        history: Vec::new(),
    })
}

// -- The docs writes (#2443) -------------------------------------------------

/// One section of a body: the byte span of what follows its top-level
/// heading up to the next one, and the text the editor opens it with or why
/// it is refused.
struct Span {
    from: usize,
    to: usize,
    verdict: Result<String, SectionRefusal>,
}

fn is_heading(node: &Node) -> bool {
    matches!(node.name(), Some("h1" | "h2" | "h3" | "h4" | "h5" | "h6"))
}

/// The body's sections, in the order of its top-level headings.
fn spans(body: &str) -> Vec<Span> {
    let nodes = html::parse(body);
    let starts: Vec<(usize, usize)> = nodes
        .iter()
        .filter(|n| is_heading(n))
        .filter_map(|n| match n {
            Node::Element { span, .. } => Some(*span),
            _ => None,
        })
        .collect();
    starts
        .iter()
        .enumerate()
        .map(|(i, (_, from))| {
            let to = starts.get(i + 1).map_or(body.len(), |(next, _)| *next);
            Span {
                from: *from,
                to,
                verdict: section_text(&html::parse(&body[*from..to])),
            }
        })
        .collect()
}

/// A section's nodes as the editor's plain text: a paragraph per block, a
/// blank line between them, `- ` per item of a flat list. The first part a
/// text box cannot hold refuses it: a table, code, one of the fixture's
/// own elements (a macro), or marks, links, an ordered or a nested list.
fn section_text(nodes: &[Node]) -> Result<String, SectionRefusal> {
    let mut blocks = Vec::new();
    for node in nodes {
        match node {
            Node::Text(text) | Node::Cdata(text) => {
                if !text.trim().is_empty() {
                    blocks.push(html::collapse(text));
                }
            }
            Node::Element { name, children, .. } => match name.as_str() {
                "p" => blocks.push(inline_text(children)?),
                "ul" => {
                    let items = children
                        .iter()
                        .filter(|c| c.name().is_some())
                        .map(|item| match item.name() {
                            Some("li") => inline_text(item.children()).map(|t| format!("- {t}")),
                            _ => Err(SectionRefusal::Formatting),
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    blocks.push(items.join("\n"));
                }
                other => return Err(refusal(other)),
            },
        }
    }
    Ok(blocks.join("\n\n"))
}

/// A paragraph's or an item's words, refused at the first element.
fn inline_text(nodes: &[Node]) -> Result<String, SectionRefusal> {
    let mut words = String::new();
    for node in nodes {
        match node {
            Node::Text(text) | Node::Cdata(text) => words.push_str(text),
            Node::Element { name, .. } => return Err(refusal(name)),
        }
    }
    Ok(html::collapse(&words))
}

/// Why an element in a section refuses it.
fn refusal(name: &str) -> SectionRefusal {
    match name {
        "table" => SectionRefusal::Table,
        "pre" | "code" => SectionRefusal::Code,
        n if n.starts_with("hb:") => SectionRefusal::Macro,
        _ => SectionRefusal::Formatting,
    }
}

/// Whether the page's payload says to break the docs contract `how`.
fn breaks(payload: &Value, how: &str) -> bool {
    payload["breach"] == how
}

/// The sections the projection names, from 1 (or from 0, to break the
/// contract).
fn sections(body: &str, from_zero: bool) -> Vec<DocSection> {
    spans(body)
        .into_iter()
        .zip(u32::from(!from_zero)..)
        .map(|(span, number)| DocSection {
            number,
            refusal: span.verdict.as_ref().err().copied(),
            text: span.verdict.ok(),
        })
        .collect()
}

/// The section edit: section `number`'s body replaced with `text` in the
/// fixture's markup, every byte around it kept, against the payload's
/// version.
pub fn edit_section(
    kind: &str,
    payload: &Value,
    number: u32,
    text: &str,
) -> Result<SectionEdit, EditRefused> {
    if kind != PAGE_KIND {
        return Err(EditRefused::Missing);
    }
    misbehave(payload);
    let body = payload["body"].as_str().ok_or(EditRefused::Missing)?;
    let base_version = payload["version"].as_i64().ok_or(EditRefused::Missing)?;
    let index = usize::try_from(number)
        .ok()
        .and_then(|n| n.checked_sub(1))
        .ok_or(EditRefused::Missing)?;
    let span = spans(body)
        .into_iter()
        .nth(index)
        .ok_or(EditRefused::Missing)?;
    if !breaks(payload, "edits_refused_sections") {
        span.verdict.map_err(EditRefused::Section)?;
    }
    let base_version = base_version + i64::from(breaks(payload, "edits_another_version"));
    Ok(SectionEdit {
        body: format!(
            "{}{}{}",
            &body[..span.from],
            html_from_text(text),
            &body[span.to..]
        ),
        base_version,
    })
}

/// What a payload's misbehaving keys ask for, before anything is drawn.
fn misbehave(payload: &Value) {
    if let Some(url) = payload["fetch"].as_str() {
        let trace = crate::fetch::get(url, crate::fetch::Options::default());
        if let Some(error) = trace.error {
            panic!("the projection's request was refused: {error}");
        }
    }
    if payload["trap"] == true {
        panic!("the projection was told to trap");
    }
    if payload["spin"] == true {
        crate::spin();
    }
    if payload["hog"] == true {
        crate::hog();
    }
}

/// The first element named `name`, depth first.
fn find<'a>(nodes: &'a [Node], name: &str) -> Option<&'a Node> {
    nodes.iter().find_map(|node| {
        if node.name() == Some(name) {
            Some(node)
        } else {
            find(node.children(), name)
        }
    })
}

/// The fixture's hook: its `hb:` elements.
struct Harbour;

impl Hook for Harbour {
    fn element(&self, element: &Node, reading: &Reading<'_>) -> Hooked {
        let attr = |key: &str| element.attr(key).map(html::decode_entities);
        let children = || reading.blocks(element.children());
        match element.name().unwrap_or_default() {
            "hb:panel" => Hooked::Blocks(vec![DocBlock::Panel {
                kind: match attr("kind").as_deref() {
                    Some("warning") => PanelKind::Warning,
                    Some("tip") => PanelKind::Tip,
                    Some("note") => PanelKind::Note,
                    _ => PanelKind::Info,
                },
                title: attr("title"),
                blocks: children(),
            }]),
            "hb:expand" => Hooked::Blocks(vec![DocBlock::Collapsible {
                title: attr("title").unwrap_or_else(|| "More".into()),
                blocks: children(),
            }]),
            "hb:tasks" => Hooked::Blocks(vec![DocBlock::List {
                ordered: false,
                items: element
                    .children()
                    .iter()
                    .filter(|c| c.name() == Some("hb:task"))
                    .map(|task| {
                        ListItem::task(
                            task.attr("done") == Some("true"),
                            reading.blocks(task.children()),
                        )
                    })
                    .collect(),
            }]),
            "hb:include" => {
                let mut include = DocBlock::include(
                    attr("title").unwrap_or_default(),
                    attr("space"),
                    element.attr("excerpt") == Some("true"),
                );
                if let DocBlock::Include {
                    entity_id, blocks, ..
                } = &mut include
                    && let Some(entity) = attr("entity")
                {
                    *entity_id = Some(entity);
                    *blocks = vec![DocBlock::Placeholder {
                        name: "smuggled".into(),
                    }];
                }
                Hooked::Blocks(vec![include])
            }
            "hb:html" => Hooked::Blocks(vec![DocBlock::Embed {
                name: "html".into(),
                content: EmbedContent::Html {
                    html: element.text(),
                },
            }]),
            // The fixture does not check the scheme: the host does (#2442).
            "hb:frame" => Hooked::Blocks(vec![DocBlock::Embed {
                name: "frame".into(),
                content: EmbedContent::Url {
                    url: attr("src").unwrap_or_default(),
                },
            }]),
            "hb:excerpt" => Hooked::Blocks(children()),
            "hb:page" => {
                let title = attr("title").unwrap_or_default();
                Hooked::Inlines(vec![Inline::PageLink {
                    text: title.clone(),
                    title,
                    space: attr("space"),
                    entity_id: attr("entity"),
                }])
            }
            "hb:issue" => Hooked::Inlines(vec![Inline::Issue {
                key: attr("key").unwrap_or_default(),
                entity_id: attr("entity"),
                title: attr("status").map(|_| "smuggled".into()),
                status: attr("status"),
            }]),
            "hb:mention" => Hooked::Inlines(vec![Inline::Mention {
                user: attr("user"),
                name: attr("name"),
                me: element.attr("me") == Some("true"),
            }]),
            _ => Hooked::Decline,
        }
    }
}
