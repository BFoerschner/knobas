//! **An external adapter's pages in Docs** (#2442, ADR-0040): the fixture
//! component synced into the mirror and opened through the Docs reader, its
//! blocks drawn by the component at read time, its search text built by the
//! host, and its failures drawn as the source's.
//!
//! Every assertion is on a Docs read or a stored row.

use knobas_app::commands::entity::{
    docs_create_page_payload, docs_edit_section_payload, docs_page_inner,
};
use knobas_app::sources::Registry;
use knobas_source::docs::{DocBlock, Inline};
use knobas_source::instance::SourceInstance;
use knobas_sync::scheduler::AdapterRegistry;
use knobas_wasm_host::WasmAdapter;
use sqlx::PgPool;

/// What the fixture sends as a page's `body-text`.
const SENT: &str = "unindexed words the fixture sent for this page";

/// A fresh database with one fixture source synced into it through
/// `registry`; its id.
async fn synced(registry: &Registry) -> (PgPool, String) {
    synced_at(registry, "").await
}

/// [`synced`], the source's instance at `base_url`.
async fn synced_at(registry: &Registry, base_url: &str) -> (PgPool, String) {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let id = format!("wiki-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ($1, 'wasm-fixture', 'Harbour wiki', $2, 'none')",
    )
    .bind(&id)
    .bind(base_url)
    .execute(&pool)
    .await
    .unwrap();
    let source = registry
        .build(SourceInstance {
            id: id.clone(),
            kind: "wasm-fixture".to_owned(),
            display_name: "Harbour wiki".to_owned(),
            base_url: base_url.to_owned(),
            auth: None,
            secret: None,
            account: None,
            config: serde_json::json!({}),
        })
        .expect("the registry builds the fixture");
    let mut conn = knobas_db::test_util::test_connector()
        .await
        .connect()
        .await
        .unwrap();
    let run = knobas_sync::run_from_stored_cursor(&mut conn, &pool, source.as_ref())
        .await
        .unwrap();
    assert_eq!(
        run.upserted, 7,
        "the surveys, the pipeline, the run, two pages"
    );
    (pool, id)
}

fn last_words(blocks: &[DocBlock]) -> String {
    match blocks.last() {
        Some(DocBlock::Paragraph { text }) => match &text[..] {
            [Inline::Text { text, .. }] => text.clone(),
            other => panic!("one run of words, got {other:?}"),
        },
        other => panic!("the page ends with a paragraph, got {other:?}"),
    }
}

/// The handbook opens in Docs drawn by the component, its includes filled
/// from the mirror through the same projection: the rules' excerpt, and the
/// rules whole. Its stored search text is the host's, not what it sent.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_page_from_the_component_opens_in_docs_with_its_includes_filled() {
    let registry = Registry::with_external(vec![knobas_wasm_host::load_fixture()]);
    let (pool, id) = synced(&registry).await;
    let handbook = format!("{id}:page-handbook");
    let page = docs_page_inner(&pool, &registry, &handbook)
        .await
        .expect("the handbook opens");
    assert!(page.readable);
    assert_eq!(page.title, "Harbour handbook");
    assert_eq!(page.space.as_ref().map(|s| s.key.as_str()), Some("HARB"));
    assert_eq!(page.version, Some(3));
    assert_eq!(page.comments.len(), 1);
    let includes: Vec<&DocBlock> = page
        .blocks
        .iter()
        .filter(|b| matches!(b, DocBlock::Include { .. }))
        .collect();
    let rules = format!("{id}:page-rules");
    let five_knots = DocBlock::Paragraph {
        text: vec![Inline::text("Five knots inside the breakwater.")],
    };
    match &includes[..] {
        [
            DocBlock::Include {
                excerpt: true,
                entity_id: Some(excerpt_of),
                blocks: excerpt,
                unfilled: None,
                ..
            },
            DocBlock::Include {
                excerpt: false,
                entity_id: Some(whole_of),
                blocks: whole,
                unfilled: None,
                ..
            },
        ] => {
            assert_eq!((excerpt_of, whole_of), (&rules, &rules));
            assert_eq!(excerpt, std::slice::from_ref(&five_knots));
            assert!(whole.contains(&five_knots), "{whole:?}");
            assert_eq!(last_words(whole), "Drawn by the quay projection.");
        }
        other => panic!("two filled includes of the rules, got {other:?}"),
    }
    assert_eq!(last_words(&page.blocks), "Drawn by the quay projection.");

    let (body,): (String,) = sqlx::query_as("select body_text from sync.item where entity_id = $1")
        .bind(&handbook)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(body.contains("Spring tides run at four knots."), "{body}");
    assert!(!body.contains(SENT), "{body}");
    assert!(
        !body.contains("Five knots"),
        "an include's words are its page's"
    );
}

/// A new component whose projection draws differently draws the mirrored
/// page differently the next time it is opened, with no re-sync.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_replaced_component_redraws_a_mirrored_page_with_no_re_sync() {
    let registry = Registry::with_external(vec![knobas_wasm_host::load_fixture()]);
    let (pool, id) = synced(&registry).await;
    let rules = format!("{id}:page-rules");
    let before = docs_page_inner(&pool, &registry, &rules).await.unwrap();
    assert_eq!(last_words(&before.blocks), "Drawn by the quay projection.");

    let pier = knobas_wasm_host::patched_fixture("the quay projection", "the pier projection");
    registry
        .replace(WasmAdapter::from_bytes(&pier).unwrap())
        .unwrap();
    let after = docs_page_inner(&pool, &registry, &rules).await.unwrap();
    assert_eq!(last_words(&after.blocks), "Drawn by the pier projection.");
}

/// A projection that traps, or runs past its deadline, reads as no
/// content: the page opens unreadable, the source's failure, and the next
/// page opens as before.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_projection_that_traps_or_spins_draws_the_page_as_the_source_s_failure() {
    let adapter = knobas_wasm_host::load_fixture().with_limits(knobas_wasm_host::limits::Limits {
        deadline: std::time::Duration::from_secs(1),
        ..knobas_wasm_host::limits::Limits::default()
    });
    let registry = Registry::with_external(vec![adapter]);
    let (pool, id) = synced(&registry).await;
    let rules = format!("{id}:page-rules");
    for fault in ["trap", "spin"] {
        sqlx::query(
            "update sync.item set payload = payload || jsonb_build_object($2::text, true)
              where entity_id = $1",
        )
        .bind(&rules)
        .bind(fault)
        .execute(&pool)
        .await
        .unwrap();
        let page = docs_page_inner(&pool, &registry, &rules)
            .await
            .expect("the page still opens");
        assert!(!page.readable, "{fault}");
        assert!(page.blocks.is_empty(), "{fault}");
    }
    let handbook = docs_page_inner(&pool, &registry, &format!("{id}:page-handbook"))
        .await
        .unwrap();
    assert!(handbook.readable, "the app goes on drawing");
}

// -- The docs writes (#2443) -------------------------------------------------

/// One request the fixture's instance received: method, path, JSON body.
type Received = (String, String, serde_json::Value);

/// A local HTTP server standing in for the fixture's instance: it records
/// every request and answers `200` with `answer` as the body, the id a
/// created page gets.
async fn instance_server(
    answer: &'static str,
) -> (String, std::sync::Arc<std::sync::Mutex<Vec<Received>>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let received = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let log = std::sync::Arc::clone(&received);
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let log = std::sync::Arc::clone(&log);
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                let head_end = loop {
                    if let Some(at) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        break at + 4;
                    }
                    match socket.read(&mut chunk).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                };
                let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
                let length: usize = head
                    .lines()
                    .find_map(|l| {
                        let (name, value) = l.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse().ok())?
                    })
                    .unwrap_or(0);
                while buf.len() < head_end + length {
                    match socket.read(&mut chunk).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }
                let mut start = head.split_whitespace();
                let method = start.next().unwrap_or_default().to_owned();
                let path = start.next().unwrap_or_default().to_owned();
                let body = serde_json::from_slice(&buf[head_end..head_end + length])
                    .unwrap_or(serde_json::Value::Null);
                log.lock().unwrap().push((method, path, body));
                let reply = format!(
                    "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{answer}",
                    answer.len()
                );
                let _ = socket.write_all(reply.as_bytes()).await;
                let _ = socket.shutdown().await;
            });
        }
    });
    (url, received)
}

struct Runs(knobas_db::embedded::Connector);

#[async_trait::async_trait]
impl knobas_sync::scheduler::RunConnections for Runs {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        self.0.connect().await
    }
}

struct Quiet;

impl knobas_sync::scheduler::SyncEvents for Quiet {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
}

/// The write queue's dependencies over `pool` and `registry`.
async fn deps(
    pool: &PgPool,
    registry: Registry,
) -> knobas_sync::scheduler::SchedulerDeps {
    knobas_sync::scheduler::SchedulerDeps {
        pool: pool.clone(),
        connections: std::sync::Arc::new(Runs(knobas_db::test_util::test_connector().await)),
        registry: std::sync::Arc::new(registry),
        secrets: std::sync::Arc::new(knobas_secrets::MemoryStore::new()),
        events: std::sync::Arc::new(Quiet),
        timing: knobas_sync::scheduler::SchedulerTiming::default(),
        quit: knobas_sync::scheduler::Quit::default(),
    }
}

/// The fixture, allowed to reach its instance.
fn reaching_its_instance() -> Registry {
    Registry::with_external(vec![knobas_wasm_host::load_fixture().with_hosts(vec![
        knobas_wasm_host::allow::HostPattern::parse("{base_url}").unwrap(),
    ])])
}

/// The rules page's mirrored body, replaced: an edited section, then one
/// for each reason a section is refused.
const SECTIONS: &str = "<h2>Speed</h2><p>Five knots.</p>\
<h2>Berths</h2><table><tr><td>7</td></tr></table>\
<h2>Radio</h2><pre>VHF 12</pre>\
<h2>Rules</h2><p>See <hb:page title=\"Harbour handbook\"></hb:page>.</p>\
<h2>Tides</h2><p>Spring tides run <strong>fast</strong>.</p>";

async fn with_body(pool: &PgPool, entity: &str, body: &str) {
    sqlx::query(
        "update sync.item set payload = jsonb_set(payload, '{body}', to_jsonb($2::text))
          where entity_id = $1",
    )
    .bind(entity)
    .bind(body)
    .execute(pool)
    .await
    .unwrap();
}

/// The queued write's row once the queue has tried it.
async fn submitted(
    deps: &knobas_sync::scheduler::SchedulerDeps,
    source: &str,
    payload: serde_json::Value,
) -> knobas_core::write_queue::QueuedWrite {
    let op: knobas_source::WriteOp = serde_json::from_value(payload).unwrap();
    let queued = knobas_sync::write_queue::submit(deps, source, op)
        .await
        .expect("the write is queued");
    knobas_core::write_queue::get(&deps.pool, queued.id)
        .await
        .unwrap()
        .expect("the row is there")
}

/// A section edit on a page from the component goes through the write
/// queue and lands at the fixture's instance with the whole body the
/// component made: the edited section in its markup, every other section
/// as it was, against the version read.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_section_edit_on_a_component_page_lands_at_its_source_with_the_whole_body() {
    let (url, received) = instance_server("").await;
    let (pool, id) = synced_at(&reaching_its_instance(), &url).await;
    let rules = format!("{id}:page-rules");
    with_body(&pool, &rules, SECTIONS).await;
    let registry = reaching_its_instance();
    let page = docs_page_inner(&pool, &registry, &rules).await.unwrap();
    assert_eq!(page.writes, ["update_page", "create_page"]);
    assert_eq!(page.sections[0].text.as_deref(), Some("Five knots."));

    let payload =
        docs_edit_section_payload(&pool, &registry, &rules, 1, Some(3), "Six knots.\n- in fog")
            .await
            .expect("the component makes the edit");
    let row = submitted(&deps(&pool, registry).await, &id, payload).await;
    assert_eq!(row.state, knobas_core::write_queue::WriteState::Sent, "{row:?}");

    let whole = SECTIONS.replacen(
        "<p>Five knots.</p>",
        "<p>Six knots.</p><ul><li>in fog</li></ul>",
        1,
    );
    let received = received.lock().unwrap().clone();
    assert_eq!(
        received,
        [(
            "PUT".to_owned(),
            "/pages/page-rules".to_owned(),
            serde_json::json!({ "version": 4, "body": whole }),
        )]
    );
}

/// A section the component refuses is refused in the shell as a
/// Confluence one is: each reason with the same words, and nothing queued.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_section_from_the_component_is_refused_as_a_confluence_one_is() {
    use knobas_source::docs::SectionRefusal;
    let registry = reaching_its_instance();
    let (pool, id) = synced(&registry).await;
    let rules = format!("{id}:page-rules");
    with_body(&pool, &rules, SECTIONS).await;
    let page = docs_page_inner(&pool, &registry, &rules).await.unwrap();
    let refusals: Vec<_> = page.sections.iter().map(|s| s.refusal).collect();
    assert_eq!(
        refusals,
        [
            None,
            Some(SectionRefusal::Table),
            Some(SectionRefusal::Code),
            Some(SectionRefusal::Macro),
            Some(SectionRefusal::Formatting),
        ]
    );
    for (number, words) in [
        (2, "This section has a table; edit it in the source."),
        (3, "This section has code; edit it in the source."),
        (4, "This section has a macro; edit it in the source."),
        (
            5,
            "This section has formatting or links that a text edit would lose; edit it in the source.",
        ),
        (6, "The page has no such section any more. Reopen it and try again."),
    ] {
        let refused = docs_edit_section_payload(&pool, &registry, &rules, number, Some(3), "x")
            .await
            .unwrap_err();
        assert_eq!(refused.message, words, "section {number}");
    }
}

/// *New page* on the fixture's space creates a page from the text: the
/// component's markup of it, in the parent's space, at the instance, and
/// the id the instance answers is the page the write made.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_page_on_the_component_s_space_is_created_from_the_text() {
    let (url, received) = instance_server("page-tides").await;
    let (pool, id) = synced_at(&reaching_its_instance(), &url).await;
    let registry = reaching_its_instance();
    let new = knobas_app::docs::NewPage {
        parent: format!("{id}:page-handbook"),
        title: "Tide tables".to_owned(),
        text: "Springs run fast.\n\n## Neaps\n- slack at noon".to_owned(),
    };
    let (payload, source) = docs_create_page_payload(&pool, &registry, &new)
        .await
        .expect("the component makes the body");
    assert_eq!(source, id);
    let row = submitted(&deps(&pool, registry).await, &id, payload).await;
    assert_eq!(row.state, knobas_core::write_queue::WriteState::Sent, "{row:?}");
    let (made,): (Option<String>,) =
        sqlx::query_as("select remote_id from knobas.write_queue where id = $1")
            .bind(row.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(made.as_deref(), Some("page-tides"));
    let received = received.lock().unwrap().clone();
    assert_eq!(
        received,
        [(
            "POST".to_owned(),
            "/pages".to_owned(),
            serde_json::json!({
                "parent": "page-handbook",
                "space": "HARB",
                "title": "Tide tables",
                "body": "<p>Springs run fast.</p><h2>Neaps</h2><ul><li>slack at noon</li></ul>",
            }),
        )]
    );
}
