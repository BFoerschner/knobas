//! The launcher's own kinds (#2422, spec #2419), against a real PostgreSQL:
//! a saved ticket view is kind `view` and a link set kind `set`, each found
//! by what it is called and what it holds, with the path line the row's key
//! cell reads; a build configuration is kind `pipeline` behind `#b`, coloured
//! by its newest run; and a run, a commit and a Jira board's mirror row are
//! in neither the answer nor the board's newest rows, whatever the query.
//!
//! The mirror rows are shaped the way the TeamCity, Gitea and Jira adapters
//! store them, under those adapters' own descriptors. This module has a
//! database of its own (`knobas_db::test_util` keys it by file); every row
//! carries a fresh token, and the tests that read the board's newest rows
//! take [`SERIAL`] because that read spans the whole mirror.

use std::sync::LazyLock;

use chrono::{DateTime, Utc};
use knobas_core::payload::StateChip;
use knobas_core::tone::Tone;
use knobas_search::{SearchFilters, SearchQuery, SearchResponse, Searcher};
use tokio::sync::Mutex;

static SERIAL: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    for (id, kind) in [
        ("gitea", "gitea"),
        ("teamcity", "teamcity"),
        ("jira", "jira"),
    ] {
        sqlx::query(
            "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
             values ($1, $2, $2, 'http://x', 'Pat')
             on conflict (id) do update set kind = excluded.kind, enabled = true",
        )
        .bind(id)
        .bind(kind)
        .execute(&pool)
        .await
        .unwrap();
    }
    pool
}

async fn searcher(pool: &sqlx::PgPool) -> Searcher {
    Searcher::declaring(
        pool.clone(),
        [
            knobas_source_gitea::descriptor_template(),
            knobas_source_teamcity::descriptor_template(),
            knobas_source_jira::descriptor_template(),
        ],
    )
    .await
    .unwrap()
}

/// A word no other row holds, in letters only, so it lexes as one word.
fn token() -> String {
    let hex = uuid::Uuid::new_v4().simple().to_string();
    let letters: String = hex[..12]
        .chars()
        .map(|c| match c.to_digit(16) {
            Some(n) => char::from(b'g' + u8::try_from(n).unwrap()),
            None => c,
        })
        .collect();
    format!("lk{letters}")
}

async fn search(pool: &sqlx::PgPool, raw: &str, titles: bool) -> SearchResponse {
    searcher(pool)
        .await
        .search(SearchQuery {
            titles,
            raw: raw.to_owned(),
            limit: 50,
            filters: SearchFilters::default(),
        })
        .await
        .unwrap()
}

/// Each hit as `(kind, entity id, path)`, in the answer's order.
fn hits(response: &SearchResponse) -> Vec<(String, String, Option<String>)> {
    response
        .groups
        .iter()
        .flat_map(|group| &group.hits)
        .map(|hit| {
            (
                hit.row.kind.clone(),
                hit.row.entity_id.clone(),
                hit.row.path.clone(),
            )
        })
        .collect()
}

fn ids(response: &SearchResponse) -> Vec<String> {
    hits(response).into_iter().map(|(_, id, _)| id).collect()
}

async fn view(pool: &sqlx::PgPool, id: &str, name: &str, group: &str, query: &str) {
    sqlx::query(
        "insert into knobas.ticket_view (id, name, view_group, position, query)
         values ($1, $2, $3, 0, $4)",
    )
    .bind(id)
    .bind(name)
    .bind(group)
    .bind(query)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn a_view_is_found_by_its_name_its_group_and_its_conditions() {
    let pool = pool().await;
    let t = token();
    let id = format!("{t}-board");
    view(
        &pool,
        &id,
        &format!("Payouts {t}name"),
        &format!("Team {t}group"),
        &format!("#t status:open {t}cond"),
    )
    .await;

    for word in ["name", "group", "cond"] {
        let response = search(&pool, &format!("{t}{word}"), false).await;
        assert_eq!(
            hits(&response),
            vec![(
                "view".to_owned(),
                id.clone(),
                Some(format!("Team {t}group · #t status:open {t}cond"))
            )],
            "searching its {word}"
        );
    }
    let group = &search(&pool, &format!("{t}name"), false).await.groups[0];
    assert_eq!(
        (group.label.as_str(), group.plural.as_str(), group.monogram.as_str()),
        ("Ticket view", "Ticket views", "VW")
    );
}

#[tokio::test]
async fn a_view_with_no_conditions_has_its_group_for_a_path_line() {
    let pool = pool().await;
    let t = token();
    view(&pool, &format!("{t}-all"), &format!("All {t}"), "Mine", "").await;
    assert_eq!(
        hits(&search(&pool, &t, false).await),
        vec![("view".to_owned(), format!("{t}-all"), Some("Mine".to_owned()))]
    );
}

#[tokio::test]
async fn under_titles_a_view_is_found_by_its_name_alone() {
    let pool = pool().await;
    let t = token();
    view(&pool, &format!("{t}-v"), &format!("{t}name"), &format!("{t}group"), "").await;
    assert_eq!(
        ids(&search(&pool, &format!("{t}name"), true).await),
        vec![format!("{t}-v")]
    );
    assert!(ids(&search(&pool, &format!("{t}group"), true).await).is_empty());
}
