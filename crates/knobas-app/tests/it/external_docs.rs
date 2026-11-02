//! **An external adapter's pages in Docs** (#2442, ADR-0040): the fixture
//! component synced into the mirror and opened through the Docs reader, its
//! blocks drawn by the component at read time, its search text built by the
//! host, and its failures drawn as the source's.
//!
//! Every assertion is on a Docs read or a stored row.

use knobas_app::commands::entity::docs_page_inner;
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
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let id = format!("wiki-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ($1, 'wasm-fixture', 'Harbour wiki', '', 'none')",
    )
    .bind(&id)
    .execute(&pool)
    .await
    .unwrap();
    let source = registry
        .build(SourceInstance {
            id: id.clone(),
            kind: "wasm-fixture".to_owned(),
            display_name: "Harbour wiki".to_owned(),
            base_url: String::new(),
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
    assert_eq!(run.upserted, 7, "the surveys, the pipeline, the run, two pages");
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
            assert_eq!(excerpt, &[five_knots.clone()]);
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
    assert!(!body.contains("Five knots"), "an include's words are its page's");
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
