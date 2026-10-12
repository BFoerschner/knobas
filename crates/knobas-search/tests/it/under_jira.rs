//! `under:<epic key>` returns the epic's tickets, against a real PostgreSQL
//! (ADR-0018, #598).
//!
//! Jira declares a ticket's epic or parent issue as its parent, read off the
//! `fields.parent` object an issue record nests, answered to by its `key`.
//! These items are shaped the way the real adapter stores them -- the issue
//! record verbatim, `key` on its root and the parent's `{id, key, fields}`
//! under `fields.parent` -- and the declaration is that adapter's own
//! descriptor, not a copy of it. One database for the file (one per test
//! module, #572), a unique token per test, nothing truncated, as in `under.rs`.

use knobas_search::{SearchFilters, SearchQuery, SearchResponse, Searcher};

fn q(raw: &str) -> SearchQuery {
    SearchQuery {
        raw: raw.to_owned(),
        limit: 30,
        filters: SearchFilters::default(),
    }
}

/// A project key unique to one test: Jira keys are `[A-Z][A-Z0-9_]*`, so the
/// token is letters only, and every key in a test is `<project>-<n>`.
fn project() -> String {
    let hex = uuid::Uuid::new_v4().simple().to_string();
    let letters: String = hex[..10]
        .chars()
        .map(|c| match c.to_digit(16) {
            Some(d) => char::from(b'A' + u8::try_from(d).unwrap()),
            None => unreachable!(),
        })
        .collect();
    format!("P{letters}")
}

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ('jira', 'jira', 'Jira', 'http://x', 'Pat')
         on conflict (id) do update set kind = excluded.kind, enabled = true",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

/// The searcher the app builds, with Jira's declared payload paths -- written
/// to `sync.parent_declaration` too, which `under:` reads through
/// `sync.item_parent` since #627. This file has its own database (#572), so
/// declaring Jira alone removes nobody else's adapter kind.
async fn searcher(pool: &sqlx::PgPool) -> Searcher {
    Searcher::declaring(pool.clone(), [knobas_source_jira::descriptor_template()])
        .await
        .unwrap()
}

/// One issue, stored the way the adapter stores it: the record verbatim as
/// payload, the summary as title. `parent` is the parent issue's key, nested
/// the way Jira nests it.
async fn ticket(pool: &sqlx::PgPool, key: &str, summary: &str, parent: Option<&str>) -> String {
    let mut fields = serde_json::json!({
        "summary": summary,
        "issuetype": { "name": if parent.is_some() { "Story" } else { "Epic" } },
        "status": { "name": "In Progress" },
    });
    if let Some(parent) = parent {
        fields["parent"] = serde_json::json!({
            "id": "10200", "key": parent,
            "fields": { "summary": "the epic's summary, which nobody types",
                        "issuetype": { "name": "Epic" } },
        });
    }
    let id = format!("jira:{key}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'ticket',$2)")
        .bind(&id)
        .bind(summary)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                item_updated_at)
         values ($1,'jira','ticket',$2,$2,$3, now())",
    )
    .bind(&id)
    .bind(summary)
    .bind(serde_json::json!({ "id": "1", "key": key, "fields": fields }))
    .execute(pool)
    .await
    .unwrap();
    id
}

fn ids(response: &SearchResponse) -> Vec<String> {
    let mut out: Vec<String> = response
        .groups
        .iter()
        .flat_map(|g| g.hits.iter().map(|h| h.row.entity_id.clone()))
        .collect();
    out.sort();
    out
}

fn sorted(mut ids: Vec<String>) -> Vec<String> {
    ids.sort();
    ids
}

/// `under:<key>`, matched ignoring case, returns the epic's children **and**
/// the epic itself, and nothing of the epic beside it in the same project.
#[tokio::test]
async fn under_an_epic_key_returns_its_tickets_and_the_epic() {
    let pool = pool().await;
    let p = project();
    let epic = format!("{p}-100");
    let other = format!("{p}-200");
    let mut ours = vec![ticket(&pool, &epic, "Payout reliability", None).await];
    for n in [101, 102] {
        ours.push(ticket(&pool, &format!("{p}-{n}"), "Retry payouts", Some(&epic)).await);
    }
    ticket(&pool, &other, "Ledger export", None).await;
    ticket(&pool, &format!("{p}-201"), "Retry exports", Some(&other)).await;

    for raw in [format!("under:{epic}"), format!("under:{}", epic.to_lowercase())] {
        let response = searcher(&pool).await.search(q(&raw)).await.unwrap();
        assert_eq!(ids(&response), sorted(ours.clone()), "{raw}");
        assert!(
            response.interpreted.unknown_tokens.is_empty(),
            "{raw} names an epic and is not unknown: {:?}",
            response.interpreted.unknown_tokens
        );
    }
}

/// An epic outside the synced JQL is still the parent its children name
/// (`CONTEXT.md` **Parent**): `under:` resolves against what the children
/// declare, so they come back though the epic has no row.
#[tokio::test]
async fn under_an_epic_that_is_not_mirrored_still_returns_its_tickets() {
    let pool = pool().await;
    let p = project();
    let epic = format!("{p}-100");
    let children = vec![
        ticket(&pool, &format!("{p}-101"), "Retry payouts", Some(&epic)).await,
        ticket(&pool, &format!("{p}-102"), "Cap retries", Some(&epic)).await,
    ];
    ticket(&pool, &format!("{p}-103"), "Unrelated", None).await;

    let response = searcher(&pool)
        .await
        .search(q(&format!("under:{epic}")))
        .await
        .unwrap();
    assert_eq!(ids(&response), sorted(children));
    assert!(
        response.interpreted.unknown_tokens.is_empty(),
        "an epic its children name is known though unmirrored: {:?}",
        response.interpreted.unknown_tokens
    );
}

/// A ticket hit carries its epic's key on its path line and keeps its own
/// summary as title; the epic declares no parent and carries no path.
#[tokio::test]
async fn a_ticket_carries_its_epic_as_path() {
    let pool = pool().await;
    let p = project();
    let epic = format!("{p}-100");
    let child = format!("{p}-101");
    let epic_id = ticket(&pool, &epic, "Payout reliability", None).await;
    let child_id = ticket(&pool, &child, "Retry payouts", Some(&epic)).await;

    let response = searcher(&pool)
        .await
        .search(q(&format!("under:{epic}")))
        .await
        .unwrap();
    let mut rows: Vec<(String, String, Option<String>)> = response
        .groups
        .iter()
        .flat_map(|g| {
            g.hits.iter().map(|h| {
                (
                    h.row.entity_id.clone(),
                    h.row.title.clone(),
                    h.row.path.clone(),
                )
            })
        })
        .collect();
    rows.sort();
    let mut expected = vec![
        (epic_id, "Payout reliability".to_owned(), None),
        (child_id, "Retry payouts".to_owned(), Some(epic.clone())),
    ];
    expected.sort();
    assert_eq!(rows, expected);
}
