//! The fixture's docs source (1.5.0, #2442, ADR-0040): two pages of the
//! *Harbour* space, and the projection that draws them.
//!
//! A page's payload carries its body as HTML, read with the shared reader
//! (`knobas_source::docs::html`), and the fixture's own elements, all in the
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
//! **The misbehaving projections**, by payload key, for the host's limits:
//! `"fetch": "<url>"` sends a request first and traps when it is refused,
//! `"trap": true` traps, `"spin": true` never returns, `"hog": true`
//! allocates until the memory cap stops it.

use chrono::{TimeZone, Utc};
use knobas_adapter_sdk::spi::docs::html::{self, Hook, Hooked, Node, Reading};
use knobas_adapter_sdk::spi::docs::{
    DocBlock, DocComment, DocContent, EmbedContent, Inline, ListItem, PAGE_KIND, PanelKind,
};
use knobas_adapter_sdk::spi::role::{RoleClaim, WIKI};
use knobas_adapter_sdk::spi::{EntityRef, KindInfo, KindPaths, ParentPath, PayloadPath};
use knobas_adapter_sdk::SyncItem;
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

/// The rules: a page with an excerpt.
const RULES: &str = r"<h2>Speed</h2>
<hb:excerpt><p>Five knots inside the breakwater.</p></hb:excerpt>
<p>Overtaking is forbidden.</p>";

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

/// Both pages, in sync order, for source `id`.
pub fn pages(id: &str) -> Vec<SyncItem> {
    PAGES
        .iter()
        .map(|(key, title, body, parent)| SyncItem {
            entity: EntityRef::new(id, key),
            kind: PAGE_KIND.into(),
            title: (*title).into(),
            body_text: SENT_BODY_TEXT.into(),
            author: Some("harbourmaster".into()),
            updated_at: Utc.with_ymd_and_hms(2026, 11, 2, 9, 0, 0).single(),
            payload: payload(key, title, body, *parent),
            web_url: None,
            deleted: false,
        })
        .collect()
}

/// The projection: a `page` payload drawn, `None` for any other kind or a
/// payload with no body.
pub fn project(kind: &str, payload: &Value) -> Option<DocContent> {
    if kind != PAGE_KIND {
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
        sections: Vec::new(),
        history: Vec::new(),
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
