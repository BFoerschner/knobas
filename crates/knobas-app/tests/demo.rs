//! Demo mode against a real PostgreSQL.
//!
//! `test_util` hands every test in this binary the *same* database, and the
//! binary runs them concurrently -- so the absolute row counts below are only
//! meaningful while nothing else is syncing `mock`. Every test that touches
//! that source takes [`MOCK`] for its whole duration; a test that touches only
//! an id no adapter answers to (`jira`) needs no lock.

use knobas_app::demo;

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

    // Before anything registers it, the mock is an adapter knobas *has* but has
    // not been told to use. Syncing it anyway would run a full fetch whose
    // cursor the engine has no row to store, so every later sync would refetch
    // the world -- silently. This assertion has to come first: the demo loads
    // below are what create the row.
    let unconfigured = demo::sync_now_inner(pool, "mock", None)
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

    // The stored cursor is exactly what `sync_now` picks up: an incremental
    // run from it has nothing left to do -- and P3 means the command hands
    // back the run's id, so what it did is read from the log.
    let run_id = demo::sync_now_inner(pool, "mock", None).await.unwrap();
    let (outcome, upserted, cursor_after, finished_at): (
        Option<String>,
        i64,
        Option<String>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "select outcome, upserted, cursor_after, finished_at from knobas.sync_run where id = $1",
    )
    .bind(run_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(outcome.as_deref(), Some("ok"));
    assert_eq!(
        upserted, 0,
        "an incremental run over a frozen fixture writes nothing"
    );
    assert_eq!(cursor_after.as_deref(), Some(first.cursor.as_str()));
    assert!(
        finished_at.is_some(),
        "a finished run must not look like a running one"
    );
}

/// A run that never started leaves no log line: the diagnostics view must not
/// show a phantom run for a source the caller got wrong.
#[tokio::test]
async fn a_refused_sync_writes_no_run() {
    let pool = knobas_db::test_util::test_pool().await;
    let pool = &pool;
    knobas_db::migrate::run(pool).await.unwrap();

    let before: (i64,) =
        sqlx::query_as("select count(*) from knobas.sync_run where source_id = 'jira'")
            .fetch_one(pool)
            .await
            .unwrap();
    demo::sync_now_inner(pool, "jira", None)
        .await
        .expect_err("M0 has no jira adapter");
    let after: (i64,) =
        sqlx::query_as("select count(*) from knobas.sync_run where source_id = 'jira'")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(before, after);
}

/// Ruling P3: per-item progress goes on the sink and nowhere else, and the run
/// id is on every message so a UI listening to two runs can tell them apart.
#[tokio::test]
async fn a_run_reports_its_phases_to_the_progress_sink() {
    use knobas_sync::{ProgressSink, SyncPhase, SyncProgress};

    #[derive(Default)]
    struct Recorder(std::sync::Mutex<Vec<SyncProgress>>);
    impl ProgressSink for Recorder {
        fn report(&self, progress: SyncProgress) {
            self.0.lock().expect("recorder").push(progress);
        }
    }

    let _guard = MOCK.lock().await;
    let pool = knobas_db::test_util::test_pool().await;
    let pool = &pool;
    knobas_db::migrate::run(pool).await.unwrap();
    demo::demo_load_inner(pool).await.unwrap();

    let recorder = Recorder::default();
    let run_id = demo::sync_now_inner(pool, "mock", Some(&recorder))
        .await
        .unwrap();

    let seen = recorder.0.lock().expect("recorder").clone();
    let phases: Vec<SyncPhase> = seen.iter().map(|p| p.phase).collect();
    assert_eq!(
        phases,
        [SyncPhase::Started, SyncPhase::Finished],
        "{seen:?}"
    );
    assert!(
        seen.iter()
            .all(|p| p.run_id == run_id && p.source_id == "mock")
    );
}

/// An id no adapter answers to is a different failure from one that is merely
/// unconfigured, and the two must not collapse into each other: `"jira"` is not
/// something loading the demo data would fix.
#[tokio::test]
async fn sync_now_refuses_an_adapter_that_does_not_exist() {
    let pool = knobas_db::test_util::test_pool().await;
    let pool = &pool;
    knobas_db::migrate::run(pool).await.unwrap();

    let error = demo::sync_now_inner(pool, "jira", None)
        .await
        .expect_err("M0 has no jira adapter");
    assert!(
        matches!(&error, demo::DemoError::UnknownSource(id) if id == "jira"),
        "unexpected error: {error}"
    );
}
