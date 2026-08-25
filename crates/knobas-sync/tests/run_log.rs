//! `knobas.sync_run` -- deliberately not the activity stream: §2a is the
//! user-facing record of what happened to their *work*, carries no durations,
//! and gets no line at all for a run that changed nothing. Diagnostics needs
//! exactly the runs §2a drops (the failures and the no-ops), and the scheduler
//! needs the last outcome to compute backoff.
//!
//! `reconcile_abandoned` is **not** tested here: it closes every open run in
//! the database, which would race the open-run assertions below (tests in one
//! binary share one database and run concurrently). It has its own binary,
//! `run_log_recovery.rs`, and therefore its own database.

use knobas_sync::run_log::{self, RunCounts, SyncOutcome, SyncTrigger};
use sqlx::PgPool;

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

fn unique(prefix: &str) -> String {
    format!("{prefix}-{}", uuid::Uuid::new_v4().simple())
}

fn ok_counts(upserted: i64) -> RunCounts {
    RunCounts {
        upserted,
        deleted: 0,
        swept: 0,
        cursor_after: Some(r#"{"v":1,"updated_to":"2026-08-24T09:14:00Z"}"#.to_owned()),
    }
}

async fn finish_ok(pool: &PgPool, run_id: i64, upserted: i64) {
    run_log::finish(
        pool,
        run_id,
        &run_log::RunResult {
            outcome: SyncOutcome::Ok,
            counts: ok_counts(upserted),
            error: None,
        },
    )
    .await
    .unwrap();
}

async fn finish_failed(pool: &PgPool, run_id: i64, outcome: SyncOutcome, error: &str) {
    run_log::finish(
        pool,
        run_id,
        &run_log::RunResult {
            outcome,
            counts: RunCounts::default(),
            error: Some(error.to_owned()),
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn a_run_is_open_while_it_runs_and_closed_with_its_counts() {
    let pool = pool().await;
    let id = unique("run");

    let run_id = run_log::start(&pool, &id, SyncTrigger::Manual)
        .await
        .unwrap();
    let open = run_log::list(&pool, Some(&id), 10).await.unwrap();
    assert_eq!(open.len(), 1);
    assert!(
        open[0].finished_at.is_none(),
        "a running run has no finished_at"
    );
    assert!(open[0].outcome.is_none(), "and no outcome");
    assert_eq!(open[0].trigger, SyncTrigger::Manual);
    assert_eq!(open[0].source_id, id);

    run_log::finish(
        &pool,
        run_id,
        &run_log::RunResult {
            outcome: SyncOutcome::Ok,
            counts: RunCounts {
                upserted: 42,
                deleted: 1,
                swept: 3,
                cursor_after: Some("c-9".to_owned()),
            },
            error: None,
        },
    )
    .await
    .unwrap();

    let done = run_log::list(&pool, Some(&id), 10).await.unwrap();
    assert_eq!(done[0].id, run_id);
    assert_eq!(done[0].outcome, Some(SyncOutcome::Ok));
    assert_eq!(
        (done[0].upserted, done[0].deleted, done[0].swept),
        (42, 1, 3)
    );
    assert!(done[0].finished_at.is_some());
    assert_eq!(done[0].cursor_after.as_deref(), Some("c-9"));
    assert!(done[0].error.is_none());
}

/// The scheduler reads the ladder rung off the log rather than storing a
/// counter, so "how many failures since the last success" has to be exact.
#[tokio::test]
async fn failures_are_counted_from_the_last_successful_run() {
    let pool = pool().await;
    let id = unique("run");
    assert_eq!(
        run_log::failures_since_last_ok(&pool, &id).await.unwrap(),
        0
    );

    for _ in 0..2 {
        let r = run_log::start(&pool, &id, SyncTrigger::Schedule)
            .await
            .unwrap();
        finish_failed(&pool, r, SyncOutcome::Unreachable, "refused").await;
    }
    assert_eq!(
        run_log::failures_since_last_ok(&pool, &id).await.unwrap(),
        2
    );

    let r = run_log::start(&pool, &id, SyncTrigger::Schedule)
        .await
        .unwrap();
    finish_ok(&pool, r, 1).await;
    assert_eq!(
        run_log::failures_since_last_ok(&pool, &id).await.unwrap(),
        0,
        "a success resets the ladder"
    );

    let r = run_log::start(&pool, &id, SyncTrigger::Schedule)
        .await
        .unwrap();
    finish_failed(&pool, r, SyncOutcome::Error, "boom").await;
    assert_eq!(
        run_log::failures_since_last_ok(&pool, &id).await.unwrap(),
        1
    );

    // A run still in flight is not a failure -- counting it would double the
    // backoff of a source that is merely slow.
    run_log::start(&pool, &id, SyncTrigger::Schedule)
        .await
        .unwrap();
    assert_eq!(
        run_log::failures_since_last_ok(&pool, &id).await.unwrap(),
        1,
        "an open run is not a failure"
    );

    // …and another source's failures are not this one's.
    let other = unique("run");
    let r = run_log::start(&pool, &other, SyncTrigger::Schedule)
        .await
        .unwrap();
    finish_failed(&pool, r, SyncOutcome::Error, "theirs").await;
    assert_eq!(
        run_log::failures_since_last_ok(&pool, &id).await.unwrap(),
        1,
        "one source's ladder must not count another's failures"
    );
}

/// `unauthorized` is the outcome P7 gives **no automatic retry**: only a human
/// can fix it, so it must never advance the ladder that schedules one.
#[test]
fn only_the_retryable_outcomes_back_off() {
    assert!(SyncOutcome::Unreachable.backs_off());
    assert!(SyncOutcome::Error.backs_off());
    assert!(
        !SyncOutcome::Unauthorized.backs_off(),
        "P7: a rejected credential needs a human, not a retry"
    );
    assert!(!SyncOutcome::Ok.backs_off());
    // Driven from ALL as well, so a new outcome has to be classified here
    // rather than defaulting into a retry loop.
    let backing_off: Vec<&str> = SyncOutcome::ALL
        .iter()
        .filter(|o| o.backs_off())
        .map(|o| o.as_str())
        .collect();
    assert_eq!(backing_off, ["unreachable", "error"]);
}

#[tokio::test]
async fn the_log_is_pruned_to_the_newest_runs_per_source() {
    let pool = pool().await;
    let id = unique("run");
    let other = unique("run");
    // Ids kept, because "three survived" is not the property -- *which* three
    // is. A prune that kept the oldest three would leave exactly as many rows,
    // in exactly the same newest-first order among themselves.
    let mut runs = Vec::new();
    for _ in 0..7 {
        let r = run_log::start(&pool, &id, SyncTrigger::Schedule)
            .await
            .unwrap();
        finish_ok(&pool, r, 0).await;
        runs.push(r);
    }
    let keeper = run_log::start(&pool, &other, SyncTrigger::Schedule)
        .await
        .unwrap();
    finish_ok(&pool, keeper, 0).await;

    let removed = run_log::prune(&pool, &id, 3).await.unwrap();
    assert_eq!(removed, 4);
    let left = run_log::list(&pool, Some(&id), 100).await.unwrap();
    let surviving: Vec<i64> = left.iter().map(|r| r.id).collect();
    let newest_three: Vec<i64> = runs.iter().rev().take(3).copied().collect();
    assert_eq!(
        surviving, newest_three,
        "the newest three survive, newest first -- not the oldest three, and \
         not in some other order"
    );
    assert_eq!(
        run_log::list(&pool, Some(&other), 100).await.unwrap().len(),
        1,
        "pruning one source must not touch another's history"
    );

    // Idempotent: pruning again removes nothing.
    assert_eq!(run_log::prune(&pool, &id, 3).await.unwrap(), 0);
    // And a keep larger than the history is not a delete-everything.
    assert_eq!(run_log::prune(&pool, &id, 100).await.unwrap(), 0);
    assert_eq!(run_log::list(&pool, Some(&id), 100).await.unwrap().len(), 3);
}

#[tokio::test]
async fn the_diagnostics_read_can_span_every_source_or_one() {
    let pool = pool().await;
    let a = unique("run");
    let b = unique("run");
    for id in [&a, &b] {
        let r = run_log::start(&pool, id, SyncTrigger::FirstRun)
            .await
            .unwrap();
        finish_ok(&pool, r, 5).await;
    }
    let all = run_log::list(&pool, None, 500).await.unwrap();
    assert!(all.iter().any(|r| r.source_id == a) && all.iter().any(|r| r.source_id == b));

    let just_a = run_log::list(&pool, Some(&a), 500).await.unwrap();
    assert!(
        just_a.iter().all(|r| r.source_id == a),
        "a filtered read must not leak another source's runs"
    );

    let last = run_log::last_finished(&pool, &a).await.unwrap().unwrap();
    assert_eq!(last.source_id, a);
    assert_eq!(last.trigger, SyncTrigger::FirstRun);
    assert_eq!(last.upserted, 5);
    assert!(
        run_log::last_finished(&pool, "never-ran")
            .await
            .unwrap()
            .is_none()
    );

    // `limit` is honoured, which is what keeps the diagnostics view bounded.
    assert!(run_log::list(&pool, None, 1).await.unwrap().len() <= 1);
}

/// `last_finished` is what `SourceSyncStatus.last_outcome` reads, so an open
/// run must not shadow the finished one underneath it -- the status strip
/// would go blank for the whole duration of every sync.
#[tokio::test]
async fn the_last_finished_run_ignores_one_still_in_flight() {
    let pool = pool().await;
    let id = unique("run");
    let done = run_log::start(&pool, &id, SyncTrigger::Schedule)
        .await
        .unwrap();
    finish_failed(&pool, done, SyncOutcome::Unreachable, "dns").await;

    let open = run_log::start(&pool, &id, SyncTrigger::Manual)
        .await
        .unwrap();
    assert_ne!(open, done);

    let last = run_log::last_finished(&pool, &id).await.unwrap().unwrap();
    assert_eq!(last.id, done, "the open run is not a finished one");
    assert_eq!(last.outcome, Some(SyncOutcome::Unreachable));
}

/// Every trigger and outcome this code writes must survive the round trip
/// through a `text` column -- a spelling that only one direction knows is a
/// diagnostics view showing blanks. Driven by `ALL` on both axes, so a new
/// variant reaches this test.
#[tokio::test]
async fn every_trigger_and_outcome_round_trips_through_the_column() {
    let pool = pool().await;
    for trigger in SyncTrigger::ALL {
        for outcome in SyncOutcome::ALL {
            let id = unique("run");
            let r = run_log::start(&pool, &id, *trigger).await.unwrap();
            finish_failed(&pool, r, *outcome, "x").await;
            let row = &run_log::list(&pool, Some(&id), 1).await.unwrap()[0];
            assert_eq!(row.trigger, *trigger, "trigger {trigger:?}");
            assert_eq!(row.outcome, Some(*outcome), "outcome {outcome:?}");
        }
    }
}
