//! Demo mode against a real PostgreSQL.
//!
//! `test_util` hands every test in this binary the *same* database, and the
//! binary runs them concurrently -- so the absolute row counts below are only
//! meaningful while nothing else is syncing `mock`. Every test that touches
//! that source takes [`MOCK`] for its whole duration; a test that touches only
//! an id no adapter answers to (`jira`) needs no lock.

use knobas_app::sources::demo;

/// Held by every test that syncs the `mock` source.
///
/// One shared database, one shared source: two demo loads racing would make
/// the counts below depend on which test's transaction committed first, and
/// would defeat the "not registered yet" precondition the first test opens
/// with outright. `tokio`'s mutex rather than `std`'s because it is held
/// across awaits.
static MOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn demo_load_registers_once_and_syncs_the_same_rows_every_time() {
    let _guard = MOCK.lock().await;
    let pool = knobas_db::test_util::test_pool().await;
    let pool = &pool;
    knobas_db::migrate::run(pool).await.unwrap();

    // The lock keeps other tests out, but not an earlier one that already ran:
    // the precondition below is about an *unregistered* source, so establish
    // it rather than assume it. Only the configuration row goes -- the mirror
    // rows stay, which is what makes the idempotence assertions further down
    // real work rather than a first load into an empty table.
    sqlx::query("delete from knobas.source_config where id = 'mock'")
        .execute(pool)
        .await
        .unwrap();

    // (An unregistered source being refused outright is
    // `knobas_sync`'s `cursor::a_source_with_no_configuration_row_is_refused_...`;
    // it moved there with the M0 `sync_now_inner` this file used to drive.)

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

    // The stored cursor is exactly what the scheduler picks up: an incremental
    // run from it has nothing left to do. Driven through the engine's own entry
    // point, on a connection of its own -- the shape §10.6(c) requires and the
    // shape the scheduler uses.
    let mut conn = knobas_db::test_util::test_connector()
        .await
        .connect()
        .await
        .unwrap();
    let incremental = knobas_sync::run_from_stored_cursor(
        &mut conn,
        pool,
        &knobas_source_mock::MockSource::new(),
    )
    .await
    .unwrap();
    assert_eq!(
        incremental.upserted, 0,
        "an incremental run over a frozen fixture writes nothing"
    );
    assert_eq!(
        incremental.cursor, first.cursor,
        "and it leaves the position where the full sync put it"
    );
}

/// An id no *configured source* answers to is refused as `NotFound`, and
/// nothing is written.
///
/// M0 asserted this against `"jira"` as an id **no adapter** answered to. Jira
/// is a compiled-in adapter now, so the interesting refusal moved: what makes
/// an id unusable is having no `knobas.source_config` row, whatever adapters
/// exist. The half about leaving no `sync_run` behind is the scheduler's
/// `scheduler_loop::triggering_a_source_that_does_not_exist_is_refused_without_a_log_row`.
#[tokio::test]
async fn an_unconfigured_source_cannot_be_built() {
    use std::sync::Arc;

    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let secrets: Arc<dyn knobas_secrets::SecretStore> =
        Arc::new(knobas_secrets::MemoryStore::new());
    let registry = knobas_app::sources::Registry::builtin();

    let missing = format!("nope-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
    // `Box<dyn Source>` is not `Debug`, so the success arm is matched.
    let error =
        match knobas_app::sources::crud::adapter_for(&pool, &secrets, &registry, &missing).await {
            Err(error) => error,
            Ok(_) => panic!("there is no such source, so nothing should have been built"),
        };
    assert!(
        matches!(&error, knobas_app::sources::SourcesError::NotFound(id) if *id == missing),
        "unexpected error: {error}"
    );
    assert_eq!(
        knobas_app::sources::to_ipc(&error, Some(&missing)).code,
        knobas_app::IpcErrorCode::NotFound
    );
}

/// The README's demo-mode promise, end to end: load the fixture, then find
/// `mock:PAY-231` by searching for `sepa retry`.
///
/// This is the acceptance check the contract PR's exit checklist asks a human
/// to perform by clicking *Load demo data* and typing in the search box. Every
/// step between those two gestures is here -- the fixture through the mock
/// adapter, the mirror upsert, the generated `fts` column, `sync.live_item`,
/// and `knobas_search::search` -- so what a human still has to verify is the
/// button and the rendering, not the pipeline. It also pins the item count the
/// README states, which nothing else did.
#[tokio::test]
async fn the_loaded_fixture_is_searchable_the_way_the_readme_promises() {
    let _guard = MOCK.lock().await;
    let pool = knobas_db::test_util::test_pool().await;
    let pool = &pool;
    knobas_db::migrate::run(pool).await.unwrap();

    let report = demo::demo_load_inner(pool).await.unwrap();
    assert_eq!(
        report.upserted, 21,
        "the README promises 21 items from the Tidewater fixture"
    );

    let response = knobas_search::Searcher::new(pool.clone())
        .search(knobas_search::SearchQuery {
            raw: "sepa retry".to_owned(),
            limit: 20,
            filters: knobas_search::SearchFilters::default(),
        })
        .await
        .unwrap();

    let ticket = response
        .groups
        .iter()
        .find(|group| group.kind == "ticket")
        .expect("a ticket group");
    assert!(
        ticket
            .hits
            .iter()
            .any(|hit| hit.row.entity_id == "mock:PAY-231"),
        "`sepa retry` must find mock:PAY-231 in the ticket group; got {:?}",
        response
            .groups
            .iter()
            .flat_map(|g| g.hits.iter().map(|h| h.row.entity_id.clone()))
            .collect::<Vec<_>>()
    );
}
