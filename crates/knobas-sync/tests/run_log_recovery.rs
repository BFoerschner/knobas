//! `reconcile_abandoned`, in a binary of its own.
//!
//! It closes **every** open run in the database, so it cannot share one with
//! the tests that assert a run is still open: `test_pool()` hands out one
//! database per test *binary*, and tests within a binary run concurrently, so
//! a separate file is what buys this its own database.

use knobas_sync::run_log::{self, SyncOutcome, SyncTrigger};

fn unique(prefix: &str) -> String {
    format!("{prefix}-{}", uuid::Uuid::new_v4().simple())
}

/// A quit during a sync leaves a row with no `finished_at`. Nothing in the
/// aborted process can close it, so the next start does -- otherwise
/// `sync_status` shows a source running for ever and the diagnostics view
/// carries a run that never ended.
#[tokio::test]
async fn a_run_abandoned_by_a_quit_is_closed_at_the_next_start() {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let id = unique("run");
    let run_id = run_log::start(&pool, &id, SyncTrigger::Schedule)
        .await
        .unwrap();

    let closed = run_log::reconcile_abandoned(&pool).await.unwrap();
    assert!(closed >= 1);

    let row = &run_log::list(&pool, Some(&id), 1).await.unwrap()[0];
    assert_eq!(row.id, run_id);
    assert_eq!(row.outcome, Some(SyncOutcome::Error));
    assert!(row.finished_at.is_some());
    assert!(
        row.error.as_deref().unwrap().contains("interrupted"),
        "{:?}",
        row.error
    );

    assert_eq!(
        run_log::reconcile_abandoned(&pool).await.unwrap(),
        0,
        "idempotent: a second startup closes nothing"
    );

    // A run that finished normally keeps the outcome it finished with -- the
    // recovery must not overwrite history.
    let ok_run = run_log::start(&pool, &id, SyncTrigger::Manual)
        .await
        .unwrap();
    run_log::finish(
        &pool,
        ok_run,
        SyncOutcome::Ok,
        &run_log::RunCounts::default(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(run_log::reconcile_abandoned(&pool).await.unwrap(), 0);
    let last = run_log::last_finished(&pool, &id).await.unwrap().unwrap();
    assert_eq!(last.id, ok_run);
    assert_eq!(last.outcome, Some(SyncOutcome::Ok));
    assert!(last.error.is_none());
}
