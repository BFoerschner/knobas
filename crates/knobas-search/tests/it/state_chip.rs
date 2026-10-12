//! Every launcher row whose kind declares a **state** carries its chip (#666),
//! against a real PostgreSQL: a search hit, a row on the board's newest items
//! and a row of a built-in list are three statements, each expanding
//! `declared_state!` on its own, so each is asked.
//!
//! The items are shaped the way the Gitea and TeamCity adapters store them, and
//! the declarations are those adapters' own descriptors. This module has a
//! database of its own (`knobas_db::test_util` keys it by file), and its tests
//! take [`SERIAL`] because the board and the list read the whole mirror.

use std::sync::LazyLock;

use chrono::{DateTime, Utc};
use knobas_core::payload::StateChip;
use knobas_core::tone::Tone;
use knobas_search::{SearchFilters, SearchQuery, Searcher};
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

fn token() -> String {
    format!("sc{}", &uuid::Uuid::new_v4().simple().to_string()[..12])
}

async fn item(
    pool: &sqlx::PgPool,
    source: &str,
    kind: &str,
    key: &str,
    title: &str,
    payload: serde_json::Value,
    at: DateTime<Utc>,
) -> String {
    let id = format!("{source}:{key}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(&id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                item_updated_at, synced_at)
         values ($1,$2,$3,$4,'',$5,$6,$6)",
    )
    .bind(&id)
    .bind(source)
    .bind(kind)
    .bind(title)
    .bind(payload)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// A merged pull request, an open one, a failed build and a ticket, each
/// carrying `t` in its title. Answers with their ids in that order.
async fn four(pool: &sqlx::PgPool, t: &str, at: DateTime<Utc>) -> [String; 4] {
    [
        item(
            pool,
            "gitea",
            "pr",
            &format!("tidewater/payout-service#{t}1"),
            &format!("merged {t}"),
            serde_json::json!({ "state": "closed", "merged": true }),
            at,
        )
        .await,
        item(
            pool,
            "gitea",
            "pr",
            &format!("tidewater/payout-service#{t}2"),
            &format!("open {t}"),
            serde_json::json!({ "state": "open", "merged": false }),
            at,
        )
        .await,
        item(
            pool,
            "teamcity",
            "build",
            &format!("{t}3"),
            &format!("failed {t}"),
            serde_json::json!({ "state": "finished", "status": "FAILURE" }),
            at,
        )
        .await,
        item(
            pool,
            "jira",
            "ticket",
            &format!("{t}-4"),
            &format!("ticket {t}"),
            serde_json::json!({ "fields": { "status": { "name": "Open" } } }),
            at,
        )
        .await,
    ]
}

fn expected() -> [Option<StateChip>; 4] {
    let chip = |text: &str, tone: Tone| {
        Some(StateChip {
            text: text.to_owned(),
            tone,
        })
    };
    [
        chip("merged", Tone::Ok),
        chip("open", Tone::Info),
        chip("FAILURE", Tone::Err),
        None,
    ]
}

/// What each id's row carries, among the rows a read answered with.
fn states<'a>(
    rows: impl IntoIterator<Item = &'a knobas_search::EntityRow>,
    ids: &[String; 4],
) -> Vec<Option<StateChip>> {
    let rows: Vec<&knobas_search::EntityRow> = rows.into_iter().collect();
    ids.iter()
        .map(|id| {
            rows.iter()
                .find(|r| &r.entity_id == id)
                .unwrap_or_else(|| panic!("{id} is among the rows"))
                .state
                .clone()
        })
        .collect()
}

#[tokio::test]
async fn a_search_hit_carries_its_state_chip() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let t = token();
    let ids = four(&pool, &t, Utc::now() - chrono::Duration::days(400)).await;
    let response = searcher(&pool)
        .await
        .search(SearchQuery {
            raw: t.clone(),
            limit: 30,
            filters: SearchFilters::default(),
        })
        .await
        .unwrap();
    let hits = response.groups.iter().flat_map(|g| &g.hits).map(|h| &h.row);
    assert_eq!(states(hits, &ids), expected());
}

#[tokio::test]
async fn the_boards_newest_rows_carry_their_state_chip() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let t = token();
    let newest: Option<DateTime<Utc>> =
        sqlx::query_scalar("select max(item_updated_at) from sync.item")
            .fetch_one(&pool)
            .await
            .unwrap();
    let at = newest.unwrap_or_else(Utc::now).max(Utc::now()) + chrono::Duration::seconds(1);
    let ids = four(&pool, &t, at).await;
    let board = searcher(&pool).await.launcher_board().await.unwrap();
    assert_eq!(states(&board.recent, &ids), expected());
}

#[tokio::test]
async fn a_built_in_lists_rows_carry_their_state_chip() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let t = token();
    let ids = four(&pool, &t, Utc::now()).await;
    let list = searcher(&pool)
        .await
        .smart_list_items("just-synced", 500)
        .await
        .unwrap();
    let rows = list.groups.iter().flat_map(|g| &g.hits).map(|h| &h.row);
    assert_eq!(states(rows, &ids), expected());
}
