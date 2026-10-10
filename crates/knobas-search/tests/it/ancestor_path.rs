//! A search returns a page **with its ancestor path** -- the whole chain, read
//! off a [`SearchHit`], against a real PostgreSQL (#388).
//!
//! M3.2's exit criterion is "the launcher finds *SEPA payout retry design*
//! with its ancestor path", and before this file every link of that chain was
//! witnessed on its own: `knobas-core/tests/it/ancestor_path.rs` runs the read
//! over a bound payload, `map.rs` asserts the adapter keeps `ancestors` in the
//! payload, and the launcher row draws whatever path it is handed. What none
//! of them asserts is that a **search** hands the row one -- that the corpus
//! in `corpus.rs` actually wires `ancestor_path_read!` over `i.payload` into
//! the statement the [`Searcher`] runs, and that the value comes out of the
//! same `FromRow` as the title. This is that assertion.
//!
//! The payload is the shape the Confluence adapter writes -- `map.rs`'s golden
//! page, `ancestors` outermost first, each an object with `id`, `title` and
//! `type` -- and not a minimal `{"ancestors": [...]}`, because the read has to
//! find the key on a real record with everything else around it.
//!
//! Same database discipline as `search.rs`: one shared database for the file,
//! a token unique to this test, and nothing truncated.
//!
//! [`SearchHit`]: knobas_search::SearchHit

use chrono::Utc;
use knobas_search::{SearchFilters, SearchQuery, Searcher};

/// A query object with the defaults the launcher sends.
fn q(raw: &str) -> SearchQuery {
    SearchQuery {
        raw: raw.to_owned(),
        limit: 30,
        filters: SearchFilters::default(),
    }
}

fn token(tag: &str) -> String {
    format!("zt{tag}{}", uuid::Uuid::new_v4().simple())
}

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// One mirrored item, with the payload the adapter would have written.
async fn seed(
    pool: &sqlx::PgPool,
    id: &str,
    kind: &str,
    source_id: &str,
    title: &str,
    body: &str,
    payload: serde_json::Value,
) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item
           (entity_id, source_id, kind, title, body_text, author, item_updated_at,
            synced_at, payload)
         values ($1,$2,$3,$4,$5,null,$6, now(), $7)",
    )
    .bind(id)
    .bind(source_id)
    .bind(kind)
    .bind(title)
    .bind(body)
    .bind(Utc::now())
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
}

/// The exit criterion's page, in the shape `map.rs`'s golden record has:
/// `ancestors` outermost first, each with `id`, `title` and `type`, and the
/// rest of a real page around them.
///
/// A transcription of that record rather than a value derived from it -- the
/// two crates share no fixture, and `knobas-source-confluence` is not a
/// dependency of this one. What keeps them in step is that the adapter's own
/// live suite reads `ancestors` off the real server (`the_space_and_the_
/// ancestors_are_where_their_readers_look`), so a shape that drifted from
/// Confluence would fail there rather than silently here.
fn confluence_page(title: &str) -> serde_json::Value {
    serde_json::json!({
        "id": "98307",
        "type": "page",
        "status": "current",
        "title": title,
        "space": { "key": "ENG", "name": "Engineering", "type": "global" },
        "body": {
            "storage": {
                "value": "<h2>Backoff policy</h2><p>base 30 s, factor 2, max 5 attempts.</p>",
                "representation": "storage"
            }
        },
        "version": {
            "number": 3,
            "when": "2026-08-22T12:40:00.000+02:00",
            "by": { "username": "knobas", "displayName": "knobas" }
        },
        "history": { "createdBy": { "username": "mara.lindqvist" } },
        "ancestors": [
            { "id": "65537", "title": "Engineering", "type": "page" },
            { "id": "65540", "title": "Payments", "type": "page" }
        ],
        "children": {
            "comment": {
                "results": [{
                    "id": "98320",
                    "type": "comment",
                    "body": { "storage": { "value": "<p>@Mara can you add the SLA?</p>" } }
                }],
                "size": 1,
                "_links": {}
            }
        },
        "_links": { "webui": "/display/ENG/SEPA+payout+retry+design" }
    })
}

/// **The chain, end to end**: a page whose payload carries `ancestors` is
/// searched by its title, and the hit's `path` is the joined ancestor path --
/// outermost first, on the one separator. A ticket found by the same search
/// carries **no** path, which is the ADR-0007 miss the corpus promises for
/// every kind whose record has no `ancestors`; both directions from one
/// statement, so a corpus that answered every row alike could not pass.
#[tokio::test]
async fn a_search_returns_a_page_with_its_ancestor_path_and_a_ticket_with_none() {
    let pool = pool().await;
    let t = token("path");
    let title = format!("{t} payout retry design");
    seed(
        &pool,
        &format!("confluence:{t}"),
        "page",
        "confluence",
        &title,
        "base 30 s, factor 2, max 5 attempts.",
        confluence_page(&title),
    )
    .await;
    seed(
        &pool,
        &format!("jira:{t}-PAY-231"),
        "ticket",
        "jira",
        &format!("Retry failed {t} payouts"),
        "The batch gives up after one retry.",
        serde_json::json!({ "key": "PAY-231", "fields": { "summary": "Retry failed payouts" } }),
    )
    .await;

    let r = Searcher::new(pool.clone()).search(q(&t)).await.unwrap();
    let hits: Vec<_> = r.groups.iter().flat_map(|g| g.hits.iter()).collect();

    let page = hits
        .iter()
        .find(|h| h.row.entity_id == format!("confluence:{t}"))
        .unwrap_or_else(|| panic!("the page is not among the hits: {hits:?}"));
    assert_eq!(page.row.title, title);
    // The literal, not `ANCESTOR_SEPARATOR` -- an independent statement of
    // what a person reads under the row, so a separator that changed on both
    // sides at once would still fail here.
    assert_eq!(
        page.row.path.as_deref(),
        Some("Engineering \u{203a} Payments"),
        "the hit's path is the joined ancestor path, outermost first: {:?}",
        page.row
    );

    let ticket = hits
        .iter()
        .find(|h| h.row.entity_id == format!("jira:{t}-PAY-231"))
        .unwrap_or_else(|| panic!("the ticket is not among the hits: {hits:?}"));
    assert_eq!(
        ticket.row.path, None,
        "a record with no `ancestors` sits nowhere: {:?}",
        ticket.row
    );
}
