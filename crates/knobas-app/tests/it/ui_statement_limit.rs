//! The two app-pool commands that may legitimately run past the UI pool's
//! statement limit lift it for themselves, and only for themselves (#745).
//!
//! `knobas_db::embedded::UI_STATEMENT_TIMEOUT` cancels any statement on the
//! app's pool after thirty seconds. Two callers on that pool do work that
//! grows with the mirror: the diagnostics view's *Re-index* (a `REINDEX
//! CONCURRENTLY`, which leaves an invalid index behind when cancelled) and the
//! adapters' declarations, whose change recomputes every item's parents (7.6 s
//! at 100,000 items in the #745 review). Witnessed on a one-connection pool
//! whose limit is a few milliseconds: each finishes anyway, and the connection
//! the pool holds afterwards still carries the limit.

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

const LIMIT: &str = "10ms";

/// A one-connection pool onto a fresh database whose statements are cancelled
/// after [`LIMIT`], and a second pool without one for the setup.
async fn limited(label: &str) -> (PgPool, PgPool) {
    let connector = knobas_db::test_util::scratch_database(label).await;
    let free = connector.pool(2).await.unwrap();
    knobas_db::migrate::run(&free).await.unwrap();
    let options = (*free.connect_options())
        .clone()
        .options([("statement_timeout", LIMIT)]);
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    // 20,000 tickets: enough that neither statement fits in ten milliseconds.
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ('jira', 'jira', 'Jira', 'http://x', 'Pat')",
    )
    .execute(&free)
    .await
    .unwrap();
    sqlx::query(
        "insert into knobas.entity (id, kind, title)
         select 'jira:PAY-' || g, 'ticket', 'payout retry storm ' || g
           from generate_series(1, 20000) g",
    )
    .execute(&free)
    .await
    .unwrap();
    sqlx::query(
        r#"insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         select id, 'jira', 'ticket', title, 'the payout retry storm floods the ledger',
                jsonb_build_object('fields', jsonb_build_object(
                    'parent', jsonb_build_object('key', 'PAY-1'),
                    'project', jsonb_build_object('key', 'PAY')))
           from knobas.entity"#,
    )
    .execute(&free)
    .await
    .unwrap();
    (free, pool)
}

async fn limit_of(pool: &PgPool) -> String {
    sqlx::query_scalar("show statement_timeout")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn reindex_runs_past_the_ui_limit_and_leaves_it_in_place() {
    let (_free, pool) = limited("limit_reindex").await;
    assert_eq!(limit_of(&pool).await, LIMIT);

    knobas_sync::stats::reindex_fts(&pool)
        .await
        .expect("a rebuild longer than the pool's limit still finishes");
    assert_eq!(
        limit_of(&pool).await,
        LIMIT,
        "the lifted limit must not go back into the pool"
    );
}

#[tokio::test]
async fn a_declaration_recompute_runs_past_the_ui_limit_and_leaves_it_in_place() {
    let (free, pool) = limited("limit_declare").await;
    sqlx::query("delete from sync.parent_declaration")
        .execute(&free)
        .await
        .unwrap();

    knobas_app::commands::search::searcher(&pool)
        .await
        .expect("a recompute longer than the pool's limit still finishes");
    let parents: i64 = sqlx::query_scalar("select count(*) from sync.item_parent")
        .fetch_one(&free)
        .await
        .unwrap();
    assert!(
        parents >= 20_000,
        "the recompute ran: {parents} parent rows"
    );
    assert_eq!(
        limit_of(&pool).await,
        LIMIT,
        "the lifted limit must not outlive its transaction"
    );
}
