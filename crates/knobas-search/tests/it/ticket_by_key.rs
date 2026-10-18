//! ⌘K finds a ticket by its key (#1174), over a Jira-shaped mirror.
//!
//! Jira's adapter stores the summary as the title and the summary,
//! description and comments as the body; the key is only in the entity id
//! (`jira:PAY-231`). Before `0048` the index read the title and the body and
//! nothing else, so `PAY-231` found every row that *mentioned* the key and
//! never the ticket, and `#PAY-231` (tickets only) found nothing.
//!
//! The keys are literal, so the test takes a database of its own rather than a
//! token in the file's shared one: a token inside the key would change how it
//! lexes, which is the thing under test.

use chrono::{Duration, Utc};
use knobas_search::{SearchFilters, SearchQuery, SearchResponse, Searcher};

fn q(raw: &str) -> SearchQuery {
    SearchQuery {
        raw: raw.to_owned(),
        limit: 30,
        filters: SearchFilters::default(),
    }
}

/// One mirrored row of the source `id`'s namespace names, written the way a
/// sync run writes it.
async fn item(pool: &sqlx::PgPool, kind: &str, id: &str, title: &str, body: &str, age: i64) {
    let (source, _) = id.split_once(':').unwrap();
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                item_updated_at)
         values ($1,$2,$3,$4,$5,'{}'::jsonb,$6)",
    )
    .bind(id)
    .bind(source)
    .bind(kind)
    .bind(title)
    .bind(body)
    .bind(Utc::now() - Duration::days(age))
    .execute(pool)
    .await
    .unwrap();
}

/// A ticket as Jira's `to_sync_item` stores it: the summary as the title, and
/// the summary and the comments, blank-line joined, as the body.
async fn ticket(pool: &sqlx::PgPool, key: &str, summary: &str, comment: &str, age: i64) {
    let body = [summary, comment]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    item(pool, "ticket", &format!("jira:{key}"), summary, &body, age).await;
}

fn all_ids(response: &SearchResponse) -> Vec<(String, String)> {
    response
        .groups
        .iter()
        .flat_map(|g| {
            g.hits
                .iter()
                .map(|h| (h.row.kind.clone(), h.row.entity_id.clone()))
        })
        .collect()
}

#[tokio::test]
async fn a_jira_ticket_is_found_by_its_key_and_comes_first() {
    let pool = knobas_db::test_util::scratch_database("ticket-by-key")
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database");
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ('jira', 'jira', 'Jira', 'http://x', 'Pat'),
                ('wiki', 'confluence', 'Wiki', 'http://y', 'Pat')",
    )
    .execute(&pool)
    .await
    .unwrap();

    // The ticket itself, the oldest row, whose summary never spells its key.
    ticket(&pool, "PAY-231", "Retry failed SEPA payouts", "", 30).await;
    // A newer ticket that mentions it in a comment: found, but after it.
    ticket(
        &pool,
        "PAY-240",
        "Payout dashboard",
        "Blocked by PAY-231 until retries land.",
        1,
    )
    .await;
    // A key that holds the typed one as a prefix of its number: not it.
    ticket(&pool, "PAY-2310", "Unrelated payout report", "", 1).await;
    // A row of another kind whose *id* would match, if ids of every kind were
    // read: only a ticket's key is its name.
    item(
        &pool,
        "page",
        "wiki:PAY-231",
        "SEPA design notes",
        "Retries.",
        1,
    )
    .await;

    let searcher = Searcher::declaring(pool.clone(), [knobas_source_jira::descriptor_template()])
        .await
        .unwrap();

    let ticket_ids = |keys: &[&str]| -> Vec<(String, String)> {
        keys.iter()
            .map(|key| ("ticket".to_owned(), format!("jira:{key}")))
            .collect()
    };

    // Still typing: the last word is a prefix, so PAY-2310 matches too, at the
    // rank PAY-231 has. The ticket named what was typed comes first anyway.
    for raw in ["PAY-231", "pay-231", "#PAY-231"] {
        let response = searcher.search(q(raw)).await.unwrap();
        assert_eq!(
            all_ids(&response),
            ticket_ids(&["PAY-231", "PAY-2310", "PAY-240"]),
            "`{raw}`: the ticket first, then the newer key it is a prefix of, \
             then the ticket that only mentions it -- and never the page whose \
             id happens to end in PAY-231"
        );
    }
    // The word finished: only the ticket and the one that mentions it.
    for raw in ["PAY-231 ", "#PAY-231 "] {
        let response = searcher.search(q(raw)).await.unwrap();
        assert_eq!(
            all_ids(&response),
            ticket_ids(&["PAY-231", "PAY-240"]),
            "`{raw}`"
        );
    }
}
