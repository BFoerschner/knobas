//! The docs projection crosses the boundary (#2442, ADR-0040): a component
//! built against `knobas:adapter@1.5.0` draws its pages in the docs
//! vocabulary, with no way out and under the adapter's limits, and the host
//! writes each page's search text from that drawing, never from what the
//! component sent. Install refuses a wiki claim from a component that does
//! not export the projection.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use knobas_source::SyncItem;
use knobas_source::contract::VecSink;
use knobas_source::docs::{DocBlock, DocContent, EmbedContent, Inline};
use knobas_source::instance::SourceInstance;
use knobas_wasm_host::limits::{self, Limits};
use knobas_wasm_host::{WasmAdapter, load_fixture, manifest, patched_fixture};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// What the fixture sends as a page's `body-text` (`docs::SENT_BODY_TEXT`
/// in the fixture).
const SENT: &str = "unindexed words the fixture sent for this page";

fn instance() -> SourceInstance {
    SourceInstance {
        id: "harbour".to_owned(),
        kind: "wasm-fixture".to_owned(),
        display_name: "Harbour wiki".to_owned(),
        base_url: "https://harbour.example".to_owned(),
        auth: None,
        secret: None,
        account: None,
        config: json!({}),
    }
}

async fn synced(adapter: &WasmAdapter) -> Vec<SyncItem> {
    let source = adapter.build(instance()).expect("the fixture builds");
    let mut sink = VecSink(Vec::new());
    source
        .sync(None, &mut sink)
        .await
        .expect("the fixture syncs");
    sink.0
}

async fn page(adapter: &WasmAdapter, title: &str) -> SyncItem {
    synced(adapter)
        .await
        .into_iter()
        .find(|item| item.kind == "page" && item.title == title)
        .unwrap_or_else(|| panic!("the fixture syncs a page {title:?}"))
}

/// A page payload whose body is `body`, as the fixture emits one.
fn payload(body: &str) -> Value {
    json!({ "key": "page-x", "space": { "key": "HARB" }, "body": body })
}

fn kinds(blocks: &[DocBlock], into: &mut BTreeSet<&'static str>) {
    for block in blocks {
        let name = match block {
            DocBlock::Heading { .. } => "heading",
            DocBlock::Paragraph { .. } => "paragraph",
            DocBlock::List { items, .. } => {
                for item in items {
                    if item.checked.is_some() {
                        into.insert("task");
                    }
                    kinds(&item.blocks, into);
                }
                "list"
            }
            DocBlock::Code { .. } => "code",
            DocBlock::Table { rows } => {
                for cell in rows.iter().flatten() {
                    if cell.colspan > 1 {
                        into.insert("spanning cell");
                    }
                    kinds(&cell.blocks, into);
                }
                "table"
            }
            DocBlock::Panel { blocks, .. } => {
                kinds(blocks, into);
                "panel"
            }
            DocBlock::Collapsible { blocks, .. } => {
                kinds(blocks, into);
                "collapsible"
            }
            DocBlock::Include { .. } => "include",
            DocBlock::Placeholder { .. } => "placeholder",
            DocBlock::Embed {
                content: EmbedContent::Html { .. },
                ..
            } => "html embed",
            DocBlock::Embed {
                content: EmbedContent::Url { .. },
                ..
            } => "url embed",
        };
        into.insert(name);
    }
}

fn inlines(blocks: &[DocBlock]) -> Vec<Inline> {
    let mut blocks = blocks.to_vec();
    knobas_source::docs::inlines_mut(&mut blocks)
        .into_iter()
        .map(|inline| inline.clone())
        .collect()
}

fn last_words(content: &DocContent) -> String {
    match content.blocks.last() {
        Some(DocBlock::Paragraph { text }) => match &text[..] {
            [Inline::Text { text, .. }] => text.clone(),
            other => panic!("one run of words, got {other:?}"),
        },
        other => panic!("the page ends with a paragraph, got {other:?}"),
    }
}

/// A page synced from the component opens in its blocks: every block kind
/// of the vocabulary, every inline, its comments, labels, version and the
/// rules page's excerpt.
#[tokio::test]
async fn a_synced_page_is_drawn_by_the_component_in_every_block_of_the_vocabulary() {
    let adapter = load_fixture();
    assert!(adapter.projects());
    let handbook = page(&adapter, "Harbour handbook").await;
    let content = adapter
        .project("page", &handbook.payload)
        .expect("the handbook is drawn");
    let mut drawn = BTreeSet::new();
    kinds(&content.blocks, &mut drawn);
    assert_eq!(
        drawn,
        BTreeSet::from([
            "code",
            "collapsible",
            "heading",
            "html embed",
            "include",
            "list",
            "panel",
            "paragraph",
            "placeholder",
            "spanning cell",
            "table",
            "task",
            "url embed",
        ])
    );
    let inline_kinds: BTreeSet<&str> = inlines(&content.blocks)
        .iter()
        .map(|inline| match inline {
            Inline::Text { strong: true, .. } => "strong",
            Inline::Text { emphasis: true, .. } => "emphasis",
            Inline::Text { code: true, .. } => "code",
            Inline::Text { .. } => "text",
            Inline::Link { .. } => "link",
            Inline::PageLink { .. } => "page link",
            Inline::Issue { .. } => "issue",
            Inline::Mention { .. } => "mention",
            Inline::Break => "break",
        })
        .collect();
    assert_eq!(
        inline_kinds,
        BTreeSet::from([
            "break",
            "code",
            "emphasis",
            "issue",
            "link",
            "mention",
            "page link",
            "strong",
            "text"
        ])
    );
    assert_eq!(content.version, Some(3));
    assert_eq!(content.labels, ["harbour"]);
    assert_eq!(content.comments.len(), 1);
    assert_eq!(content.comments[0].author.as_deref(), Some("pilot"));
    assert_eq!(content.excerpt, None, "the handbook marks no excerpt");

    let rules = page(&adapter, "Berthing rules").await;
    let rules = adapter.project("page", &rules.payload).unwrap();
    assert_eq!(
        rules.excerpt,
        Some(vec![DocBlock::Paragraph {
            text: vec![Inline::text("Five knots inside the breakwater.")]
        }])
    );
    assert_eq!(
        adapter.project("survey", &json!({ "key": "survey-1" })),
        None
    );
}

/// What knobas fills when a page is read is the host's to fill: a
/// component that fills it is cleared. A URL embed of a scheme the shell
/// does not frame is a placeholder; an `http(s)` one is kept.
#[test]
fn what_knobas_fills_is_cleared_and_only_an_http_url_is_framed() {
    let content = load_fixture()
        .project(
            "page",
            &payload(
                r#"<p><hb:page title="Rules" entity="harbour:page-rules"></hb:page>
                <hb:issue key="HARB-7" entity="jira:HARB-7" status="Done"></hb:issue>
                <hb:mention user="pilot" me="true"></hb:mention></p>
                <hb:include title="Rules" entity="harbour:page-rules"></hb:include>
                <hb:frame src="javascript:alert(1)"></hb:frame>
                <hb:frame src=" https://harbour.example/cam "></hb:frame>"#,
            ),
        )
        .expect("drawn");
    for inline in inlines(&content.blocks) {
        match inline {
            Inline::PageLink { entity_id, .. } => assert_eq!(entity_id, None),
            Inline::Issue {
                entity_id,
                title,
                status,
                ..
            } => assert_eq!((entity_id, title, status), (None, None, None)),
            Inline::Mention { me, .. } => assert!(!me),
            _ => {}
        }
    }
    assert_eq!(
        content.blocks[1],
        DocBlock::include("Rules", None, false),
        "nothing filled"
    );
    assert_eq!(
        content.blocks[2],
        DocBlock::Placeholder {
            name: "frame".into()
        }
    );
    assert_eq!(
        content.blocks[3],
        DocBlock::Embed {
            name: "frame".into(),
            content: EmbedContent::Url {
                url: "https://harbour.example/cam".into()
            }
        }
    );
}

/// The host writes a page's `body_text` with the shared function from the
/// component's drawing, whatever the component sent: the words of its
/// blocks, cells, expands and HTML text, not script, not an include's
/// target, and not one word of the sent text. Every other item keeps its
/// own.
#[tokio::test]
async fn a_pages_search_text_is_built_by_the_host_from_the_projection() {
    let adapter = load_fixture();
    let items = synced(&adapter).await;
    let handbook = items
        .iter()
        .find(|item| item.title == "Harbour handbook")
        .unwrap();
    let body = &handbook.body_text;
    for found in [
        "Pilots board at buoy 3",
        "deep water",
        "Keep clear of the breakwater.",
        "Tide tables",
        "Spring tides run at four knots.",
        "Live crane board",
        "Fenders checked",
        "Drawn by the quay projection.",
    ] {
        assert!(body.contains(found), "{found:?} in {body:?}");
    }
    for missing in ["window.cranes", "Five knots", "Overtaking", "unindexed"] {
        assert!(!body.contains(missing), "{missing:?} in {body:?}");
    }
    let content = adapter.project("page", &handbook.payload).unwrap();
    assert_eq!(
        *body,
        knobas_source::docs::search_text("Harbour handbook", "HARB", &content)
    );
    let rules = items
        .iter()
        .find(|item| item.title == "Berthing rules")
        .unwrap();
    assert!(
        rules
            .body_text
            .contains("Five knots inside the breakwater.")
    );
    assert!(items.iter().all(|item| !item.body_text.contains(SENT)));
    let survey = items.iter().find(|item| item.kind == "survey").unwrap();
    assert!(survey.body_text.contains("quaywright"), "kept as sent");
}

/// A local server that answers every request `200` and counts the
/// connections it accepted.
async fn counting_server() -> (String, Arc<AtomicUsize>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&accepted);
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            count.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 1024];
                while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    match socket.read(&mut chunk).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }
                let _ = socket
                    .write_all(
                        b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok",
                    )
                    .await;
                let _ = socket.shutdown().await;
            });
        }
    });
    (address.to_string(), accepted)
}

/// The projection has no HTTP capability: a projection that sends a request
/// to a host the adapter's manifest declares, and which the adapter's own
/// calls reach, is refused before it leaves, so it traps and draws nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_projection_reaches_no_host_even_one_its_manifest_declares() {
    let (address, accepted) = counting_server().await;
    let adapter = load_fixture().with_hosts(vec![
        knobas_wasm_host::allow::HostPattern::parse(&address).unwrap(),
    ]);
    let url = format!("http://{address}/page");
    // The adapter's own call reaches the host: the direction a refusal
    // could otherwise hide.
    let mut fetching = instance();
    fetching.config = json!({ "fetch": url });
    let reached = adapter
        .build(fetching)
        .unwrap()
        .test_connection()
        .await
        .expect("the declared host answers the adapter's own call");
    assert!(reached.detail.unwrap().contains("200"));
    assert_eq!(accepted.load(Ordering::SeqCst), 1);

    let mut asks = payload("<p>words</p>");
    asks["fetch"] = url.into();
    let projecting = adapter.clone();
    let drawn = tokio::task::spawn_blocking(move || projecting.project("page", &asks))
        .await
        .unwrap();
    assert_eq!(drawn, None, "the refused request trapped the projection");
    assert_eq!(
        accepted.load(Ordering::SeqCst),
        1,
        "nothing reached the host"
    );
}

/// A projection that traps, runs past its deadline or past its memory cap
/// is no content, each within its limit, and the adapter draws the next page
/// as before.
#[test]
fn a_projection_that_traps_spins_or_hogs_is_no_content_and_the_adapter_goes_on() {
    let adapter = load_fixture().with_limits(Limits {
        deadline: Duration::from_secs(1),
        memory_mib: 32,
    });
    for key in ["trap", "spin", "hog"] {
        let mut asks = payload("<p>words</p>");
        asks[key] = true.into();
        let started = Instant::now();
        assert_eq!(adapter.project("page", &asks), None, "{key}");
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{key} stopped at its limit: {:?}",
            started.elapsed()
        );
    }
    assert!(adapter.project("page", &payload("<p>words</p>")).is_some());
}

/// A projection that spins under the adapter's default deadline of ten
/// minutes is stopped at the projection's own, so opening a page never
/// waits on it for longer (spec #2432 story 48).
#[test]
fn a_spinning_projection_stops_at_its_own_deadline_not_the_adapters() {
    let adapter = load_fixture();
    assert_eq!(adapter.limits().deadline, limits::DEFAULT_DEADLINE);
    let mut spins = payload("<p>words</p>");
    spins["spin"] = true.into();
    let started = Instant::now();
    let (sent, answer) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sent.send(adapter.project("page", &spins).is_none());
    });
    let none = answer
        .recv_timeout(limits::PROJECTION_DEADLINE * 3)
        .expect("the projection is stopped well before the adapter's deadline");
    assert!(none, "a stopped projection is no content");
    let took = started.elapsed();
    assert!(
        took >= limits::PROJECTION_DEADLINE,
        "stopped at the projection's deadline: {took:?}"
    );
}

/// A new component whose projection draws differently draws the same
/// mirrored payload differently: nothing projected was kept.
#[tokio::test]
async fn a_component_that_draws_differently_redraws_the_same_payload() {
    let handbook = page(&load_fixture(), "Harbour handbook").await;
    let before = load_fixture().project("page", &handbook.payload).unwrap();
    let redrawn = WasmAdapter::from_bytes(&patched_fixture(
        "the quay projection",
        "the pier projection",
    ))
    .unwrap()
    .project("page", &handbook.payload)
    .unwrap();
    assert_eq!(last_words(&before), "Drawn by the quay projection.");
    assert_eq!(last_words(&redrawn), "Drawn by the pier projection.");
}

fn install(artifact: Vec<u8>) -> Result<manifest::Checked, manifest::Refusal> {
    let manifest = manifest::Manifest::parse(
        &json!({
            "adapter_kind": "wasm-fixture",
            "adapter_version": "0.1.0",
            "interface_version": "1.5.0",
            "artifact": { "file": "fixture.wasm", "sha256": manifest::sha256(&artifact) },
            "hosts": ["{base_url}"],
        })
        .to_string(),
    )
    .expect("the manifest is well formed");
    manifest::check(manifest, artifact)
}

/// Install accepts the fixture's wiki claim, and refuses the same component
/// with its `docs` export renamed away: a wiki claim needs the projection.
#[test]
fn a_wiki_claim_from_a_component_without_the_projection_is_refused_at_install() {
    let fixture = std::fs::read(knobas_wasm_host::fixture_component()).unwrap();
    let installed = install(fixture).expect("the fixture installs");
    assert!(installed.adapter.projects());

    let unexported = patched_fixture("knobas:adapter/docs@1.5.0", "knobas:adapter/doxs@1.5.0");
    let loaded = WasmAdapter::from_bytes(&unexported).expect("it still loads");
    assert!(!loaded.projects());
    assert_eq!(loaded.project("page", &payload("<p>words</p>")), None);
    let refusal = install(unexported).expect_err("a wiki claim without the projection");
    assert_eq!(
        refusal.to_string(),
        "the adapter claims the wiki role, and its component exports no docs projection \
         (knobas:adapter/docs, 1.5.0), so knobas could draw none of its pages"
    );
}
