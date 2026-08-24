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

    // Before anything registers it, the mock is an adapter knobas *has* but has
    // not been told to use. Syncing it anyway would run a full fetch whose
    // cursor the engine has no row to store, so every later sync would refetch
    // the world -- silently. This assertion has to come first: the demo loads
    // below are what create the row.
    let unconfigured = demo::sync_now_inner(pool, "mock")
        .await
        .expect_err("an unregistered source must be refused");
    assert!(
        matches!(&unconfigured, demo::DemoError::NotConfigured(id) if id == "mock"),
        "unexpected error: {unconfigured}"
    );

    let first = demo::demo_load_inner(pool).await.unwrap();
    assert_eq!(first.source_id, "mock");
    assert!(
        first.upserted > 10,
        "expected the full fixture, got {}",
        first.upserted
    );

    // *One* load has to be enough to leave the cursor behind, and that is what
    // pins the ordering inside `demo_load_inner`: registration first, then the
    // run. Reversed, the run finds no `source_config` row to persist into --
    // `run_once` updates a row and deliberately never invents one -- so the
    // cursor is dropped on the floor and the row the registration then creates
    // has `cursor` NULL. Reading this only after a *second* load would hide
    // exactly that: the first load would create the row and the second would
    // fill it in, and the assertion would hold either way.
    let (stored,): (Option<String>,) =
        sqlx::query_as("select cursor from knobas.source_config where id = 'mock'")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(
        stored.as_deref(),
        Some(first.cursor.as_str()),
        "one demo load must leave a resumable cursor -- did the run happen before the registration?"
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

    // The stored cursor is exactly what `sync_now` picks up: an incremental run
    // from it has nothing left to do.
    let incremental = demo::sync_now_inner(pool, "mock").await.unwrap();
    assert_eq!(incremental.upserted, 0);
    assert_eq!(incremental.cursor, first.cursor);
}

/// An id no adapter answers to is a different failure from one that is merely
/// unconfigured, and the two must not collapse into each other: `"jira"` is not
/// something loading the demo data would fix.
#[tokio::test]
async fn sync_now_refuses_an_adapter_that_does_not_exist() {
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
