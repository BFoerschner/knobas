//! The start-work step store, against a real PostgreSQL (issue #1707).
//!
//! Same shape as `write_queue.rs`: one database is shared by every test in
//! this file, so each test plans a flow on a ticket of its own.
//!
//! What these pin is the queue's transition discipline carried over to the
//! steps: an update names the outcomes it may leave, and a caller whose read
//! went stale is answered `None` and changes nothing. Each test is an ordered
//! interleaving -- the stale read, the step moving on underneath it, then the
//! stale write -- so the race is replayed in the order it happens, not hoped
//! for with a sleep.

use knobas_core::entity::EntityRef;
use knobas_core::start_work::{self as store, Advance, FlowStep, PlannedStep, Step, StepOutcome};
use uuid::Uuid;

/// A migrated pool and a planned four-step flow on a ticket unique to this
/// run.
async fn planned() -> (sqlx::PgPool, Vec<FlowStep>) {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let ticket = EntityRef::new(&format!("sw{}", Uuid::new_v4().simple()), "SW-1");
    let plan: Vec<PlannedStep> = Step::ALL
        .iter()
        .map(|step| PlannedStep {
            step: *step,
            payload: serde_json::json!({ "proposed": step.as_str() }),
        })
        .collect();
    let flow = store::plan(&pool, &ticket, &plan).await.unwrap();
    (pool, flow)
}

/// The branch step, which every test here drives.
fn branch(flow: &[FlowStep]) -> i64 {
    flow.iter()
        .find(|step| step.step == Step::CreateBranch)
        .expect("a branch step")
        .id
}

/// The branch step whose write already landed: it failed once, the stepper
/// drew it failed, and then a retry somewhere else settled it succeeded.
///
/// Answers the row as it stands after the landing.
async fn landed_behind_a_stale_read(pool: &sqlx::PgPool, id: i64) -> FlowStep {
    store::settle(
        pool,
        id,
        &[StepOutcome::Pending],
        StepOutcome::Failed,
        Some(7),
        Some("refused"),
    )
    .await
    .unwrap()
    .expect("a pending step can fail");
    let stale = store::get(pool, id).await.unwrap().unwrap();
    assert_eq!(stale.outcome, StepOutcome::Failed, "what the stepper drew");
    store::settle(
        pool,
        id,
        &[StepOutcome::Failed],
        StepOutcome::Succeeded,
        None,
        Some("already there"),
    )
    .await
    .unwrap()
    .expect("a failed step can succeed on retry")
}

/// **CORE-8.** An edit from a stepper that still draws the step failed must
/// not return a step whose write landed to pending: the next advance would
/// answer *Run* for it and a second branch would be made at the source.
#[tokio::test]
async fn a_stale_edit_leaves_a_step_that_succeeded_as_it_landed() {
    let (pool, flow) = planned().await;
    let id = branch(&flow);
    let landed = landed_behind_a_stale_read(&pool, id).await;

    let edited = store::repropose(&pool, id, serde_json::json!({ "proposed": "edited" }))
        .await
        .unwrap();

    assert_eq!(edited, None, "the stale edit was applied");
    assert_eq!(store::get(&pool, id).await.unwrap(), Some(landed));
    let ticket = EntityRef::parse(&flow[0].ticket_id).unwrap();
    let flow = store::flow(&pool, &ticket).await.unwrap();
    assert!(
        !matches!(store::advance(&flow), Advance::Run(step) if step.id == id),
        "the next advance runs the branch step a second time"
    );
}

/// A stale retry's return to pending is refused the same way: the step it
/// would leave is not one a retry may leave.
#[tokio::test]
async fn a_stale_retry_leaves_a_step_that_succeeded_as_it_landed() {
    let (pool, flow) = planned().await;
    let id = branch(&flow);
    let landed = landed_behind_a_stale_read(&pool, id).await;

    let retried = store::settle(
        &pool,
        id,
        &[StepOutcome::Pending, StepOutcome::Failed, StepOutcome::Queued],
        StepOutcome::Pending,
        None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(retried, None, "the stale retry was applied");
    assert_eq!(store::get(&pool, id).await.unwrap(), Some(landed));
}

/// The guard is the outcomes named, not "anything but succeeded": an edit is
/// refused on a queued step too, whose write is still open -- returning it to
/// pending is how a second write comes to be queued beside the first.
#[tokio::test]
async fn an_edit_of_a_queued_step_is_refused() {
    let (pool, flow) = planned().await;
    let id = branch(&flow);
    let queued = store::settle(
        &pool,
        id,
        &[StepOutcome::Pending],
        StepOutcome::Queued,
        Some(3),
        Some("the server did not answer"),
    )
    .await
    .unwrap()
    .unwrap();

    let edited = store::repropose(&pool, id, serde_json::json!({ "proposed": "edited" }))
        .await
        .unwrap();

    assert_eq!(edited, None);
    assert_eq!(store::get(&pool, id).await.unwrap(), Some(queued));
}

/// The two outcomes an edit may leave still take it: a failed step comes back
/// pending with the new proposal and keeps the write it made.
#[tokio::test]
async fn an_edit_of_a_failed_step_returns_it_to_pending_with_its_write() {
    let (pool, flow) = planned().await;
    let id = branch(&flow);
    store::settle(
        &pool,
        id,
        &[StepOutcome::Pending],
        StepOutcome::Failed,
        Some(9),
        Some("refused"),
    )
    .await
    .unwrap()
    .unwrap();

    let edited = store::repropose(&pool, id, serde_json::json!({ "proposed": "edited" }))
        .await
        .unwrap()
        .expect("a failed step's proposal is the reader's to change");

    assert_eq!(edited.outcome, StepOutcome::Pending);
    assert_eq!(edited.payload, serde_json::json!({ "proposed": "edited" }));
    assert_eq!(edited.write_id, Some(9));
    assert_eq!(edited.detail, None);
}

/// A landing that lost its race to another writer still leaves the write it
/// made on the step, so a retry acts on that write instead of making a second.
#[tokio::test]
async fn a_write_is_recorded_on_a_step_whatever_its_outcome() {
    let (pool, flow) = planned().await;
    let id = branch(&flow);
    store::settle(&pool, id, &[StepOutcome::Pending], StepOutcome::Running, None, None)
        .await
        .unwrap()
        .unwrap();
    assert!(store::interrupt(&pool, id, "stopped").await.unwrap());
    let landing = store::settle(
        &pool,
        id,
        &[StepOutcome::Running],
        StepOutcome::Succeeded,
        Some(11),
        None,
    )
    .await
    .unwrap();
    assert_eq!(landing, None, "the landing took a step that was interrupted");

    store::record_write(&pool, id, 11).await.unwrap();

    let step = store::get(&pool, id).await.unwrap().unwrap();
    assert_eq!(step.outcome, StepOutcome::Failed);
    assert_eq!(step.detail.as_deref(), Some("stopped"));
    assert_eq!(step.write_id, Some(11));
}
