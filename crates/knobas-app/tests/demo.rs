//! Demo mode against a real PostgreSQL.
//!
//! `test_util` hands every test in this binary the *same* database, and the
//! binary runs them concurrently -- so everything that touches the `mock`
//! source lives in one test, as consecutive steps. That is what makes the
//! absolute row counts below meaningful: nothing else is syncing `mock` while
//! they are taken.

use knobas_app::demo;

#[tokio::test]
async fn demo_load_registers_once_and_syncs_the_same_rows_every_time() {
    let pool = knobas_db::test_util::test_pool().await;
    let pool = &pool;
    knobas_db::migrate::run(pool).await.unwrap();

    let first = demo::demo_load_inner(pool).await.unwrap();
    assert_eq!(first.source_id, "mock");
    assert!(
        first.upserted > 10,
        "expected the full fixture, got {}",
        first.upserted
    );

    // Loading the demo twice is something a user can do by double-clicking the
    // button: it must not double the corpus, nor add a second configuration.
    let second = demo::demo_load_inner(pool).await.unwrap();
    assert_eq!(first.upserted, second.upserted);
    assert_eq!(first.cursor, second.cursor);

    let (sources,): (i64,) =
        sqlx::query_as("select count(*) from knobas.source_config where id = 'mock'")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(sources, 1, "the mock was registered more than once");

    let (items,): (i64,) =
        sqlx::query_as("select count(*) from sync.item where source_id = 'mock'")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(
        items as u64, first.upserted,
        "a re-run duplicated rows instead of upserting them"
    );

    // The registration has to happen *before* the run, or the engine finds no
    // row to persist the cursor into and `sync_now` has nothing to resume from.
    let (stored,): (Option<String>,) =
        sqlx::query_as("select cursor from knobas.source_config where id = 'mock'")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(stored.as_deref(), Some(first.cursor.as_str()));

    // ...which is exactly what `sync_now` picks up: an incremental run from the
    // stored cursor has nothing left to do.
    let incremental = demo::sync_now_inner(pool, "mock").await.unwrap();
    assert_eq!(incremental.upserted, 0);
    assert_eq!(incremental.cursor, first.cursor);
}

#[tokio::test]
async fn sync_now_refuses_a_source_that_is_not_configured() {
    let pool = knobas_db::test_util::test_pool().await;
    let pool = &pool;
    knobas_db::migrate::run(pool).await.unwrap();

    let error = demo::sync_now_inner(pool, "jira")
        .await
        .expect_err("M0 has no jira adapter");
    assert!(
        matches!(&error, demo::DemoError::UnknownSource(id) if id == "jira"),
        "unexpected error: {error}"
    );
}
