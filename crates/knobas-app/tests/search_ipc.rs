//! The four §2.4 search commands, exercised through their bodies.
//!
//! A `#[tauri::command]` cannot be called without a window, so what is tested
//! here is the `*_inner` function each command is a two-line shim over -- the
//! pattern `tests/demo.rs` already uses for `demo_load_inner`. `tests/ipc.rs`
//! covers the other half (that the argument shapes decode and the handler list
//! dispatches); this file covers what the shims are *for*: the wire shape the
//! hand-written TypeScript mirrors, the error codes the frontend branches on,
//! and the one DTO that is a composition of two streams' data.
//!
//! Every test in this binary shares one database (`knobas_db::test_util`), and
//! libtest does not order them. So: nothing truncates, every corpus assertion
//! is scoped to a token this test seeded, and no assertion is made about an
//! absolute row count of anything global.

use knobas_app::IpcErrorCode;
use knobas_app::commands::search::{
    launcher_home_inner, search_inner, smart_list_items_inner, smart_lists_inner,
};
use knobas_search::{SearchFilters, SearchQuery};

/// A pool with the schema on it.
async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// A token nothing else in this binary can match.
fn token(tag: &str) -> String {
    format!("zi{tag}{}", uuid::Uuid::new_v4().simple())
}

/// One ticket and one PR carrying `tag`, plus a source to attribute them to.
///
/// The source row is what `launcher_home` reads its health out of, so it has
/// to exist before that assertion -- and it is inserted with `on conflict do
/// nothing` because another test in this binary may have got there first.
async fn seed(pool: &sqlx::PgPool, tag: &str) -> String {
    let t = token(tag);
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ('jira', 'jira', 'Jira', 'http://x', 'Pat')
         on conflict (id) do nothing",
    )
    .execute(pool)
    .await
    .unwrap();

    for (suffix, kind, title, body) in [
        (
            "PAY-231",
            "ticket",
            format!("Retry failed {t} payouts"),
            format!("The {t} batch gives up after one retry instead of backing off."),
        ),
        (
            "142",
            "pr",
            format!("Backoff for the {t} retry loop"),
            format!("Adds jittered backoff to the {t} worker."),
        ),
    ] {
        let id = format!("jira:{t}-{suffix}");
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
            .bind(&id)
            .bind(kind)
            .bind(&title)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into sync.item
               (entity_id, source_id, kind, title, body_text, author, item_updated_at,
                synced_at, payload)
             values ($1,'jira',$2,$3,$4,'mara.lindqvist', now() - interval '1 day',
                     now() - interval '4 minutes', '{}'::jsonb)",
        )
        .bind(&id)
        .bind(kind)
        .bind(&title)
        .bind(&body)
        .execute(pool)
        .await
        .unwrap();
    }
    t
}

fn query(raw: &str) -> SearchQuery {
    SearchQuery {
        raw: raw.to_owned(),
        limit: 20,
        filters: SearchFilters::default(),
    }
}

/// The field names are the contract.
///
/// `app/src/lib/ipc/search.ts` is written by hand, so a rename on this side is
/// a field the frontend reads as `undefined` -- no compile error on either
/// half. The crate's own unit tests pin the shape of a `SearchHit` against the
/// mirror's text; this pins the shape of a **live answer**, which is the only
/// place `interpreted`, `groups` and `took_ms` appear together and the only
/// place the flatten is observed on data that came out of PostgreSQL.
#[tokio::test]
async fn the_wire_shape_is_the_contract() {
    let pool = pool().await;
    let t = seed(&pool, "wire").await;

    let response = search_inner(&pool, query(&t)).await.unwrap();
    let v = serde_json::to_value(&response).unwrap();

    assert!(v["interpreted"]["unknown_tokens"].is_array(), "{v}");
    assert!(v["groups"][0]["monogram"].is_string(), "{v}");
    assert!(v["groups"][0]["plural"].is_string(), "{v}");
    assert!(v["groups"][0]["total"].is_number(), "{v}");
    // `SearchHit` flattens its `EntityRow`: the six identity fields are inline
    // and there is no `row` object. Both halves are asserted, because the
    // attribute can only be lost in the direction that adds the nesting back.
    assert!(
        v["groups"][0]["hits"][0]["entity_id"].is_string(),
        "SearchHit flattens its row: {v}"
    );
    assert!(v["groups"][0]["hits"][0]["row"].is_null(), "{v}");
    assert!(v["groups"][0]["hits"][0]["synced_at"].is_string(), "{v}");
    assert!(
        v["groups"][0]["hits"][0]["snippet"][0]["hit"].is_boolean(),
        "{v}"
    );
    assert!(
        v["groups"][0]["hits"][0]["snippet"][0]["text"].is_string(),
        "{v}"
    );
    assert!(v["took_ms"].is_number(), "{v}");
    assert!(v["total"].is_number(), "{v}");

    // And the answer is about the rows this test seeded, so the assertions
    // above are made against a populated response rather than an empty one.
    assert_eq!(response.groups.len(), 2, "a ticket and a PR: {v}");
    assert_eq!(response.total, 2);
}

/// The excerpt crosses the bridge as **data**, never as markup.
///
/// Roadmap §4 gotcha 7. The frontend renders `segment.text` as text and the
/// `hit` flag is what draws the `<mark>`; if a highlighter sentinel ever
/// reached the wire as a tag, a component author following the doc comment
/// would render it inertly and the highlight would silently disappear -- or,
/// worse, someone would reach for `{@html}` to make it work again.
#[tokio::test]
async fn no_markup_reaches_the_wire_in_a_snippet() {
    let pool = pool().await;
    let t = seed(&pool, "mkup").await;

    let response = search_inner(&pool, query(&t)).await.unwrap();
    let segments: Vec<_> = response
        .groups
        .iter()
        .flat_map(|group| group.hits.iter())
        .flat_map(|hit| hit.snippet.iter())
        .collect();

    assert!(!segments.is_empty(), "the seeded rows have excerpts");
    assert!(
        segments.iter().any(|segment| segment.hit),
        "the match itself is flagged, not marked up: {segments:?}"
    );
    for segment in &segments {
        assert!(
            !segment.text.contains('<') && !segment.text.contains('\u{1}'),
            "a segment carries markup or a raw sentinel: {segment:?}"
        );
    }
}

/// Errors carry a code the frontend can branch on (ruling P1).
///
/// `not_found` is what makes a stale `list:` in the box say *no such list*
/// instead of *something went wrong*; the message is for humans and nobody
/// parses it.
#[tokio::test]
async fn errors_carry_a_code_the_frontend_can_branch_on() {
    let pool = pool().await;

    let unknown = smart_list_items_inner(&pool, "nope", 10).await.unwrap_err();
    assert_eq!(unknown.code, IpcErrorCode::NotFound, "{unknown:?}");
    assert!(unknown.message.contains("nope"), "{unknown:?}");

    // A query outside the engine's bounds is the caller's fault, not the
    // database's: `invalid`, so the box can say so rather than offering
    // *Retry*.
    let huge = search_inner(&pool, query(&"p".repeat(1_000)))
        .await
        .unwrap_err();
    assert_eq!(huge.code, IpcErrorCode::Invalid, "{huge:?}");

    // And a database failure is `internal`. Taken against a closed pool so
    // the mapping is observed rather than argued: `SearchError::Db` is the one
    // arm no bad argument can reach.
    let closed = knobas_db::test_util::test_pool().await;
    closed.close().await;
    let dead = search_inner(&closed, query("sepa")).await.unwrap_err();
    assert_eq!(dead.code, IpcErrorCode::Internal, "{dead:?}");
}

/// The board reports source health beside the lists (interfaces §2.4).
///
/// `LauncherHome` is the one DTO in this stream that composes two streams'
/// data: the lists and the recent rows are `knobas-search`'s, `sources` is
/// stream F's `CredentialHealth`, and `pending_writes` is stream G's write
/// queue, which does not exist in M1 and is 0 by rule.
#[tokio::test]
async fn launcher_home_reports_source_health_beside_the_lists() {
    let pool = pool().await;
    seed(&pool, "home").await;

    let home = launcher_home_inner(&pool).await.unwrap();
    let v = serde_json::to_value(&home).unwrap();

    for field in ["smart_lists", "recent", "sources", "pending_writes"] {
        assert!(!v[field].is_null(), "{field} missing: {v}");
    }
    assert_eq!(v["pending_writes"], 0, "the write queue is M2: {v}");

    // §2.2's `CredentialHealth` shape, taken from stream F's own type rather
    // than from a second copy of it -- and `unknown` because nothing has
    // tested the credential (0002's default).
    let jira = v["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["source_id"] == "jira")
        .unwrap_or_else(|| panic!("the seeded source is missing: {v}"));
    assert_eq!(jira["state"], "unknown", "{jira}");
    for field in [
        "source_id",
        "state",
        "checked_at",
        "detail",
        "secret_expires_at",
    ] {
        assert!(jira.get(field).is_some(), "{field} missing: {jira}");
    }

    // The board's own two halves are populated and shaped as the mirror
    // declares them: a smart list with a count and a badge, and recent rows
    // carrying the provenance stamp §4 renders "synced 4 min ago" from.
    assert!(!home.smart_lists.is_empty(), "M1 ships built-in lists");
    let list = serde_json::to_value(&home.smart_lists[0]).unwrap();
    for field in ["id", "label", "count", "changed", "description"] {
        assert!(list.get(field).is_some(), "{field} missing: {list}");
    }
    assert!(!home.recent.is_empty(), "this test seeded two items");
    assert!(v["recent"][0]["synced_at"].is_string(), "{v}");
}

/// `smart_lists` answers the same summaries the board embeds.
///
/// Two commands, one source of truth: the launcher refreshes the rail without
/// re-reading the whole board, and a divergence between them would show as a
/// count that changes when nothing did.
#[tokio::test]
async fn the_rail_and_the_board_report_the_same_lists() {
    let pool = pool().await;
    seed(&pool, "rail").await;

    let standalone = smart_lists_inner(&pool).await.unwrap();
    let embedded = launcher_home_inner(&pool).await.unwrap().smart_lists;

    let ids = |lists: &[knobas_search::SmartListSummary]| {
        lists.iter().map(|l| l.id.clone()).collect::<Vec<_>>()
    };
    assert_eq!(ids(&standalone), ids(&embedded));
    assert!(!standalone.is_empty());
}

/// Opening a list answers with a `SearchResponse`, so the launcher renders it
/// with the code it renders results with.
#[tokio::test]
async fn a_smart_list_answers_in_the_search_shape() {
    let pool = pool().await;
    seed(&pool, "list").await;

    let response = smart_list_items_inner(&pool, "just-synced", 20)
        .await
        .unwrap();
    let v = serde_json::to_value(&response).unwrap();

    assert_eq!(v["interpreted"]["prefix"], "list", "{v}");
    assert!(v["groups"].is_array(), "{v}");
    assert!(v["took_ms"].is_number(), "{v}");
    // A list row has no excerpt and no rank -- it was not matched against
    // anything -- and the *shape* is still a search's, which is the point.
    let hit = &v["groups"][0]["hits"][0];
    assert!(hit["entity_id"].is_string(), "{v}");
    assert_eq!(hit["snippet"], serde_json::json!([]), "{v}");
}
