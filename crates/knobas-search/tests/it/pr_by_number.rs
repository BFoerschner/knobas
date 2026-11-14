//! ⌘K finds a pull request by its number (migration `0072`).
//!
//! A forge's pull request is `gitea:<owner>/<repo>#<n>`, and before `0072`
//! its number was in its id and nowhere the index read: `#142`, `PR 142`
//! (which the launcher sends as `#p 142`) and `142` found nothing, while its
//! title's words found it. On the live products four pull requests were #142,
//! one per repository and forge, and none was reachable by the number.
//!
//! The ids are literal, so the test takes a database of its own, as
//! `ticket_by_key` does.

use chrono::{Duration, Utc};
use knobas_search::{SearchFilters, SearchQuery, SearchResponse, Searcher};

fn q(raw: &str) -> SearchQuery {
    SearchQuery {
        titles: false,
        raw: raw.to_owned(),
        limit: 30,
        filters: SearchFilters::default(),
    }
}

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

/// Every hit's id, group by group, in the order drawn.
fn ids(response: &SearchResponse) -> Vec<String> {
    response
        .groups
        .iter()
        .flat_map(|g| g.hits.iter().map(|h| h.row.entity_id.clone()))
        .collect()
}

/// The pull requests' hits alone, in the order drawn.
fn prs(response: &SearchResponse) -> Vec<String> {
    response
        .groups
        .iter()
        .flat_map(|g| g.hits.iter())
        .filter(|h| h.row.kind == "pr")
        .map(|h| h.row.entity_id.clone())
        .collect()
}

#[tokio::test]
async fn a_pull_request_is_found_by_its_number_on_every_forge() {
    let pool = knobas_db::test_util::scratch_database("pr-by-number")
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database");
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ('gitea', 'gitea', 'Gitea', 'http://x', 'Pat'),
                ('forgejo', 'gitea', 'Forgejo', 'http://y', 'Pat'),
                ('jira', 'jira', 'Jira', 'http://z', 'Pat')",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Four #142s whose titles never say the number, two repositories on two forges.
    item(
        &pool,
        "pr",
        "gitea:tidewater/payout-service#142",
        "Retry SEPA batch",
        "",
        9,
    )
    .await;
    item(
        &pool,
        "pr",
        "gitea:tidewater/ledger-api#142",
        "Ledger rounding",
        "",
        8,
    )
    .await;
    item(
        &pool,
        "pr",
        "forgejo:tidewater/payout-service#142",
        "Retry SEPA batch",
        "",
        7,
    )
    .await;
    item(
        &pool,
        "pr",
        "forgejo:tidewater/ledger-api#142",
        "Ledger rounding",
        "",
        6,
    )
    .await;
    // A number the typed one is a prefix of, a pull request that only mentions
    // #142 in its body, and a ticket whose comment does.
    item(
        &pool,
        "pr",
        "gitea:tidewater/payout-service#1420",
        "Unrelated",
        "",
        1,
    )
    .await;
    item(
        &pool,
        "pr",
        "gitea:tidewater/payout-service#150",
        "Follow-up",
        "Builds on #142.",
        1,
    )
    .await;
    item(
        &pool,
        "ticket",
        "jira:PAY-9",
        "Payout notes",
        "See PR 142.",
        1,
    )
    .await;

    let searcher = Searcher::declaring(pool.clone(), [knobas_source_jira::descriptor_template()])
        .await
        .unwrap();

    let fours = [
        "gitea:tidewater/payout-service#142",
        "gitea:tidewater/ledger-api#142",
        "forgejo:tidewater/payout-service#142",
        "forgejo:tidewater/ledger-api#142",
    ];
    // `PR 142` is sent as `#p 142` by the launcher (`askedAs`).
    for raw in ["#142 ", "142 ", "#p 142 ", "#p #142 "] {
        let found = prs(&searcher.search(q(raw)).await.unwrap());
        let mut head: Vec<&str> = found.iter().take(4).map(String::as_str).collect();
        head.sort_unstable();
        let mut want = fours.to_vec();
        want.sort_unstable();
        assert_eq!(head, want, "`{raw}` puts the four #142s first: {found:?}");
        assert!(
            !found.iter().any(|id| id.ends_with("#1420")),
            "`{raw}` finished is not a prefix of #1420: {found:?}"
        );
    }
    // Narrowed to pull requests, the ticket that mentions it goes.
    let narrowed = ids(&searcher.search(q("#p 142 ")).await.unwrap());
    assert!(!narrowed.contains(&"jira:PAY-9".to_owned()), "{narrowed:?}");
    // Still typing, `14` is a prefix of the number.
    let typing = ids(&searcher.search(q("#p 14")).await.unwrap());
    for id in fours {
        assert!(
            typing.contains(&id.to_owned()),
            "`#p 14` reaches {id}: {typing:?}"
        );
    }
}
