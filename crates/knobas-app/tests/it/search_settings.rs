//! Settings › Search (the everything-search decisions, 2026-11-20, D13)
//! against a real `knobas.setting`: every box on until set, what was stored
//! after a write, the notes switch read beside it in one statement, and what
//! each box covers counted from the mirror.

use knobas_app::search_settings;
use knobas_search::EverythingMatches;
use sqlx::PgPool;

async fn scratch(name: &str) -> PgPool {
    knobas_db::test_util::scratch_database(name)
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database")
}

#[tokio::test]
async fn every_box_is_on_until_set_and_then_what_was_stored() {
    let pool = scratch("search_settings_round_trip").await;
    assert_eq!(
        search_settings::everything(&pool).await.unwrap(),
        EverythingMatches::default()
    );
    assert_eq!(
        search_settings::for_searcher(&pool).await.unwrap(),
        (false, EverythingMatches::default())
    );

    let cleared = EverythingMatches {
        ticket_text: false,
        build_logs: false,
        ..EverythingMatches::default()
    };
    search_settings::set_everything(&pool, cleared)
        .await
        .unwrap();
    assert_eq!(search_settings::everything(&pool).await.unwrap(), cleared);
    knobas_app::settings::write(&pool, knobas_app::capture::SHOWN_KEY, Some("true"))
        .await
        .unwrap();
    assert_eq!(
        search_settings::for_searcher(&pool).await.unwrap(),
        (true, cleared)
    );
    assert_eq!(
        search_settings::settings(&pool).await.unwrap().everything,
        cleared
    );
}

#[tokio::test]
async fn each_box_counts_what_it_covers() {
    let pool = scratch("search_settings_counts").await;
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ('jira', 'jira', 'Jira', 'http://x', 'Pat')",
    )
    .execute(&pool)
    .await
    .unwrap();
    for (id, kind, deleted) in [
        ("jira:A-1", "ticket", false),
        ("jira:A-2", "ticket", false),
        ("jira:A-3", "ticket", true),
        ("jira:P-1", "pr", false),
        ("jira:G-1", "page", false),
        ("jira:B-1", "build", false),
    ] {
        sqlx::query("insert into knobas.entity (id, kind, title, deleted_at) values ($1, $2, $1, case when $3 then now() end)")
            .bind(id)
            .bind(kind)
            .bind(deleted)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at, synced_at)
             values ($1, 'jira', $2, $1, '', '{}', now(), now())",
        )
        .bind(id)
        .bind(kind)
        .execute(&pool)
        .await
        .unwrap();
    }
    let read = search_settings::settings(&pool).await.unwrap();
    assert_eq!(
        (read.tickets, read.pull_requests, read.pages, read.pipelines),
        (2, 1, 1, 0)
    );
}
