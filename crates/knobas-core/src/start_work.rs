//! The start-work flow's progress: a sequence of named steps, each with an
//! explicit outcome (issue #44).
//!
//! Starting work on a ticket is the same four steps every time -- a branch, a
//! draft pull request, a link back to the ticket, and the ticket moved to In
//! Progress -- across three systems. knobas proposes the whole sequence,
//! **shows it before anything happens**, and then performs it step by step with
//! each outcome visible. This module owns the rows that make that possible.
//!
//! ## What is here and what is not
//!
//! The rows, and **the rule that decides what runs next** ([`advance`]). Not
//! the running: composing an op and handing it to the write queue needs the
//! SPI, and `knobas-source` depends on this crate rather than the other way
//! round -- so the orchestrator lives in `knobas_app::start_work`, and this is
//! the state it reads and writes.
//!
//! That split is why a step's proposal is stored as a jsonb payload rather
//! than as a typed `WriteOp`: this module carries the value the user edited
//! without decoding it, and the enum grows per milestone (ADR-0006). It is the
//! same treatment `crate::write_queue` gives a queued write, for the same
//! reason.
//!
//! ## The one rule the sequence rests on
//!
//! **A failed step never advances the sequence** (story 12). It is [`advance`],
//! it is pure, and it is a property of the *step list* rather than a check the
//! orchestrator has to remember: a caller that asks what to run next cannot be
//! handed a step behind a stopped one, because there is no answer of that shape
//! to give.

use serde::Serialize;
use sqlx::PgPool;

use crate::CoreError;
use crate::entity::EntityRef;

crate::closed_vocabulary! {
    /// The steps a start-work flow is made of.
    ///
    /// Stored as lowercase text in `knobas.start_work_step.step`, whose
    /// `start_work_step_chk` (migration 0008) allows exactly these spellings --
    /// and `ALL` is what the test that pins the two together walks.
    ///
    /// Three of the four are `knobas_source::WriteOp` identifiers and one is
    /// not: [`LinkPullRequest`](Step::LinkPullRequest) is a **knobas link**,
    /// created through the link store and never written to either source,
    /// which is the whole reason the relationship survives regardless of what
    /// Jira or Gitea happens to record (story 9).
    ///
    /// - `create_branch`: The branch, in the repository the user chose.
    /// - `create_pull_request`: The draft pull request, from that branch.
    /// - `link_pull_request`: The knobas link from the pull request back to the
    ///   ticket.
    /// - `transition`: The ticket, to In Progress.
    #[derive(ts_rs::TS)]
    #[ts(export_to = "entity.ts", rename = "StartWorkStepKind")]
    pub enum Step {
        CreateBranch => "create_branch",
        CreatePullRequest => "create_pull_request",
        LinkPullRequest => "link_pull_request",
        Transition => "transition",
    }
}

crate::closed_vocabulary! {
    /// What happened to one step.
    ///
    /// Stored as lowercase text in `knobas.start_work_step.outcome`, whose
    /// `start_work_outcome_chk` (migration 0008) allows exactly these
    /// spellings.
    ///
    /// - `pending`: Planned; nothing has been attempted.
    /// - `running`: Dispatched, no answer yet -- what tells a slow step from a
    ///   stuck one (story 11).
    /// - `succeeded`: The effect exists at the source.
    /// - `queued`: The source could not take the write, so it is a **pending
    ///   write** and will go when the source can (story 15).
    ///
    ///   Deliberately neither a success nor a failure. The flow's completion
    ///   and the write's delivery are different events and the two must not be
    ///   conflated: a queued step has changed nothing at the source yet, so the
    ///   sequence waits rather than reporting a step that has not happened.
    /// - `failed`: The source refused, or knobas could not go on.
    /// - `skipped`: The user chose not to run it (story 14).
    #[derive(ts_rs::TS)]
    #[ts(export_to = "entity.ts", rename = "StartWorkOutcome")]
    pub enum StepOutcome {
        Pending => "pending",
        Running => "running",
        Succeeded => "succeeded",
        Queued => "queued",
        Failed => "failed",
        Skipped => "skipped",
    }
}

impl StepOutcome {
    /// Whether this step is finished with, so the sequence may move past it.
    ///
    /// Two outcomes and only two. `Succeeded` is the step having happened;
    /// `Skipped` is the user saying it does not have to. Everything else is a
    /// step the sequence is still on -- including [`Queued`](Self::Queued),
    /// because a queued write has changed nothing at the source and a
    /// pull request opened from a branch that does not exist yet points at
    /// nothing (story 12).
    #[must_use]
    pub fn is_settled(self) -> bool {
        matches!(self, Self::Succeeded | Self::Skipped)
    }
}

/// Every column of `knobas.start_work_step` that leaves this module, in one
/// place.
///
/// The same device, for the same reason, as `link_columns!` and
/// `queue_columns!`: [`FlowStep`] is a `FromRow`, so a column this list forgets
/// is a decode failure at run time rather than a compile error.
macro_rules! step_columns {
    () => {
        "id, ticket_id, step, position, outcome, payload, write_id, detail, updated_at"
    };
}

/// One step of one flow.
///
/// Issue #44. `payload` is untyped on the wire for `submit_write`'s reason:
/// `WriteOp` grows per milestone.
#[derive(Clone, Debug, PartialEq, Serialize, sqlx::FromRow, ts_rs::TS)]
#[ts(export_to = "entity.ts", rename = "StartWorkStep")]
pub struct FlowStep {
    pub id: i64,
    /// The ticket the flow is about -- and the flow's identity.
    pub ticket_id: String,
    pub step: Step,
    /// Where the step sits in the sequence.
    ///
    /// The stepper renders in this order.
    pub position: i32,
    pub outcome: StepOutcome,
    /// **The proposal**, as the user last left it: the serialized
    /// `knobas_source::WriteOp` a dispatching step will submit, or the link's
    /// relation.
    #[ts(type = "unknown")]
    pub payload: serde_json::Value,
    /// The `knobas.write_queue` row this step dispatched, once it has one.
    ///
    /// What makes a retry safe: a step whose write is still open is retried by
    /// acting on *that* write rather than by queueing a second one.
    pub write_id: Option<i64>,
    /// What happened, in whoever's words. Untrusted source text.
    pub detail: Option<String>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// One step of a plan, before it is written.
///
/// A plan is proposed and *shown* before anything happens, so it exists as a
/// value before it exists as rows.
#[derive(Clone, Debug)]
pub struct PlannedStep {
    pub step: Step,
    pub payload: serde_json::Value,
}

/// What the sequence does next.
///
/// The four answers are exhaustive over a step list, which is what makes
/// "a failed step never advances the sequence" a property rather than a rule
/// somebody has to remember: there is no variant that hands back a step behind
/// a stopped one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Advance<'a> {
    /// Run this step: it is the first that has not been settled, and nothing
    /// before it is stopping the sequence.
    Run(&'a FlowStep),
    /// Wait: this step is dispatched and has not answered, or its write is
    /// queued behind a source that cannot take it yet. Nothing after it may
    /// start.
    Waiting(&'a FlowStep),
    /// Stopped here. This step failed, and the sequence goes no further until
    /// the user retries it or skips it.
    Stopped(&'a FlowStep),
    /// Every step is settled -- succeeded or deliberately skipped.
    Done,
}

/// What the sequence does next, given every step of it.
///
/// **The rule this whole feature rests on.** Walk in position order; the first
/// step that is not [settled](StepOutcome::is_settled) decides, and every step
/// behind it waits. A failed branch creation therefore cannot produce a pull
/// request pointing at nothing (story 12) -- not because the orchestrator
/// checks, but because asking what to run next answers
/// [`Stopped`](Advance::Stopped) and there is no way to ask for the step after
/// it.
///
/// Pure, and takes the whole list rather than a database handle, so the rule
/// can be checked against sequences that never existed -- which is where a
/// stepper's real behaviour is.
///
/// `steps` is expected in position order; [`flow`] reads it that way.
#[must_use]
pub fn advance(steps: &[FlowStep]) -> Advance<'_> {
    for step in steps {
        if step.outcome.is_settled() {
            continue;
        }
        return match step.outcome {
            StepOutcome::Pending => Advance::Run(step),
            StepOutcome::Failed => Advance::Stopped(step),
            // Running and Queued both mean "dispatched, no landing yet". The
            // sequence waits for the same reason in both cases: nothing after
            // this step may assume its effect exists.
            StepOutcome::Running | StepOutcome::Queued => Advance::Waiting(step),
            // Unreachable by `is_settled` above, and spelled rather than
            // wildcarded so a sixth outcome stops this compiling.
            StepOutcome::Succeeded | StepOutcome::Skipped => continue,
        };
    }
    Advance::Done
}

/// Write a flow's plan, in the order given.
///
/// Refuses if the ticket already has one: **there is at most one start-work
/// flow per ticket**, so running the flow again on a ticket that already has a
/// branch and a pull request reports that state rather than making a second
/// set (story 22). The refusal is `start_work_step_unq`'s, not a
/// check-then-insert, so two callers arriving together cannot both decide the
/// ticket is free -- and the whole plan is written in one transaction, so a
/// flow is never half-planned.
///
/// # Errors
///
/// [`CoreError::Duplicate`] if the ticket already has a flow -- the crate-wide
/// classifier lifts the unique violation out for us; [`CoreError::Db`] if the
/// insert fails for any other reason.
pub async fn plan(
    pool: &PgPool,
    ticket: &EntityRef,
    steps: &[PlannedStep],
) -> Result<Vec<FlowStep>, CoreError> {
    let mut tx = pool.begin().await?;
    let mut written = Vec::with_capacity(steps.len());
    for (position, planned) in steps.iter().enumerate() {
        let row = sqlx::query_as::<_, FlowStep>(concat!(
            "insert into knobas.start_work_step (ticket_id, step, position, payload)
             values ($1, $2, $3, $4)
             returning ",
            step_columns!()
        ))
        .bind(ticket.to_string())
        .bind(planned.step.as_str())
        .bind(i32::try_from(position).unwrap_or(i32::MAX))
        .bind(&planned.payload)
        .fetch_one(&mut *tx)
        .await?;
        written.push(row);
    }
    tx.commit().await?;
    Ok(written)
}

/// Every step of one ticket's flow, in the order it runs.
///
/// Empty if the ticket has no flow, which is how a caller asks "has this been
/// started?".
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn flow(pool: &PgPool, ticket: &EntityRef) -> Result<Vec<FlowStep>, CoreError> {
    let rows = sqlx::query_as::<_, FlowStep>(concat!(
        "select ",
        step_columns!(),
        " from knobas.start_work_step where ticket_id = $1 order by position"
    ))
    .bind(ticket.to_string())
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// One step by id, or `None` if nothing carries it.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn get(pool: &PgPool, id: i64) -> Result<Option<FlowStep>, CoreError> {
    let row = sqlx::query_as::<_, FlowStep>(concat!(
        "select ",
        step_columns!(),
        " from knobas.start_work_step where id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Record what became of a step, if it is still in one of the outcomes `from`
/// names.
///
/// **`from` is the guard** (#1707), the write queue's transition discipline
/// carried over: each caller names the outcomes its move may leave, and a step
/// that moved on between the caller's read and this write is left as it landed.
/// Without it a stale caller -- a retry from a stepper still drawing the step
/// failed -- returned a succeeded step to pending, and the next advance ran it
/// again at the source.
///
/// `None` if no step carries `id` or the step is in none of the outcomes in
/// `from`.
///
/// `write_id` is the `knobas.write_queue` row the step dispatched. It is
/// **written once and never cleared**: `coalesce` keeps the id a later
/// settlement does not carry, because the write a step made is a fact about
/// that step whatever its outcome ends up being, and it is what a retry reads
/// to decide whether a second write is needed at all. A settlement the guard
/// refuses records no write; [`record_write`] is how its caller keeps it.
///
/// # Errors
///
/// [`CoreError::Db`] if the statement fails.
pub async fn settle(
    pool: &PgPool,
    id: i64,
    from: &[StepOutcome],
    outcome: StepOutcome,
    write_id: Option<i64>,
    detail: Option<&str>,
) -> Result<Option<FlowStep>, CoreError> {
    let from: Vec<&str> = from.iter().map(|outcome| outcome.as_str()).collect();
    let row = sqlx::query_as::<_, FlowStep>(concat!(
        "update knobas.start_work_step
            set outcome = $2, write_id = coalesce($3, write_id),
                detail = $4, updated_at = now()
          where id = $1 and outcome = any($5)
         returning ",
        step_columns!()
    ))
    .bind(id)
    .bind(outcome.as_str())
    .bind(write_id)
    .bind(detail)
    .bind(from)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Keep the write a step dispatched, whatever outcome the step is in now.
///
/// For a landing [`settle`] refused because the step moved on while the write
/// was out -- interrupted underneath a send. The outcome is the other writer's;
/// the write is still this step's, and forgetting it is how a retry comes to
/// send a second one.
///
/// # Errors
///
/// [`CoreError::Db`] if the statement fails.
pub async fn record_write(pool: &PgPool, id: i64, write_id: i64) -> Result<(), CoreError> {
    sqlx::query(
        "update knobas.start_work_step set write_id = $2, updated_at = now() where id = $1",
    )
    .bind(id)
    .bind(write_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Every step stored as [`Running`](StepOutcome::Running), across all tickets.
///
/// Read by the start-up pass that finds steps a quit left running (#790).
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn running(pool: &PgPool) -> Result<Vec<FlowStep>, CoreError> {
    let rows = sqlx::query_as::<_, FlowStep>(concat!(
        "select ",
        step_columns!(),
        " from knobas.start_work_step where outcome = 'running' order by id"
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Mark a step that is still `running` as [`Failed`](StepOutcome::Failed) with
/// `detail`, and say whether it was.
///
/// The `where outcome = 'running'` is the guard: a step that settled between
/// the caller's read and this write is left as it landed.
///
/// # Errors
///
/// [`CoreError::Db`] if the statement fails.
pub async fn interrupt(pool: &PgPool, id: i64, detail: &str) -> Result<bool, CoreError> {
    let done = sqlx::query(
        "update knobas.start_work_step
            set outcome = 'failed', detail = $2, updated_at = now()
          where id = $1 and outcome = 'running'",
    )
    .bind(id)
    .bind(detail)
    .execute(pool)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Replace a step's proposal with the one the user edited, and return it to
/// [`Pending`](StepOutcome::Pending).
///
/// The edit is what a *retry* of a refused step is for: a source that rejected
/// a branch name will reject it again unchanged, so changing it is the exit.
/// The `write_id` is deliberately **kept** -- the earlier attempt still
/// happened, and forgetting it is how a retry comes to make a second object at
/// the source.
///
/// Only a [`Pending`](StepOutcome::Pending) or [`Failed`](StepOutcome::Failed)
/// step is re-proposed (#1707). Every other outcome has a write that landed or
/// is still open, or is a decision already made, and returning it to pending
/// would have the next advance run it at the source a second time.
///
/// `None` if no step carries `id`, or the step is in any other outcome -- the
/// answer a caller whose read went stale is given.
///
/// # Errors
///
/// [`CoreError::Db`] if the statement fails.
pub async fn repropose(
    pool: &PgPool,
    id: i64,
    payload: serde_json::Value,
) -> Result<Option<FlowStep>, CoreError> {
    let row = sqlx::query_as::<_, FlowStep>(concat!(
        "update knobas.start_work_step
            set payload = $2, outcome = 'pending', detail = null, updated_at = now()
          where id = $1 and outcome in ('pending', 'failed')
         returning ",
        step_columns!()
    ))
    .bind(id)
    .bind(payload)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(step: Step, position: i32, outcome: StepOutcome) -> FlowStep {
        FlowStep {
            id: i64::from(position) + 1,
            ticket_id: "jira:PAY-231".to_owned(),
            step,
            position,
            outcome,
            payload: serde_json::json!({}),
            write_id: None,
            detail: None,
            updated_at: chrono::Utc::now(),
        }
    }

    /// The four steps in the order the flow runs them, each with the outcome
    /// given.
    fn sequence(outcomes: [StepOutcome; 4]) -> Vec<FlowStep> {
        Step::ALL
            .iter()
            .zip(outcomes)
            .enumerate()
            .map(|(position, (kind, outcome))| {
                step(*kind, i32::try_from(position).expect("four steps"), outcome)
            })
            .collect()
    }

    /// The two vocabularies and migration 0008's CHECK constraints are one
    /// list written in two places, and neither may grow without the other: a
    /// variant the constraint does not allow is an `INSERT` that fails at
    /// runtime, and a spelling the enum does not know is a step this module's
    /// decoder *refuses*, so the flow can never be read back at all.
    #[test]
    fn the_steps_and_outcomes_are_exactly_what_the_migration_allows() {
        let migration = include_str!("../../knobas-db/migrations/0008_start_work.sql");
        for (marker, spellings, len) in [
            (
                "check (step in (",
                Step::ALL.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                Step::ALL.len(),
            ),
            (
                "check (outcome in (",
                StepOutcome::ALL
                    .iter()
                    .map(|o| o.as_str())
                    .collect::<Vec<_>>(),
                StepOutcome::ALL.len(),
            ),
        ] {
            let line = migration
                .lines()
                .find(|line| line.contains(marker))
                .unwrap_or_else(|| panic!("0008 has no line containing {marker:?}"));
            for spelling in &spellings {
                assert!(
                    line.contains(&format!("'{spelling}'")),
                    "{spelling:?} is a variant the constraint does not allow: {line}"
                );
            }
            assert_eq!(
                line.matches('\'').count() / 2,
                len,
                "the constraint and the enum list different numbers of values: {line}"
            );
        }
    }

    #[test]
    fn a_step_and_an_outcome_roundtrip_through_their_column_values() {
        for kind in Step::ALL {
            assert_eq!(kind.as_str().parse(), Ok(*kind));
        }
        for outcome in StepOutcome::ALL {
            assert_eq!(outcome.as_str().parse(), Ok(*outcome));
        }
        assert!("abandoned".parse::<StepOutcome>().is_err());
    }

    /// **The load-bearing rule.** A failed step stops the sequence, and the
    /// steps behind it are not offered -- which is what stops a pull request
    /// being opened from a branch that was never created.
    #[test]
    fn a_failed_step_stops_the_sequence_rather_than_advancing_past_it() {
        use StepOutcome::{Failed, Pending};
        let steps = sequence([Failed, Pending, Pending, Pending]);
        assert_eq!(advance(&steps), Advance::Stopped(&steps[0]));
    }

    /// The same, from the middle: a failure two steps in does not let the
    /// remaining two run, and the answer names the step that stopped it so the
    /// stepper can say which one.
    #[test]
    fn a_failure_part_way_through_stops_everything_after_it() {
        use StepOutcome::{Failed, Pending, Succeeded};
        let steps = sequence([Succeeded, Failed, Pending, Pending]);
        match advance(&steps) {
            Advance::Stopped(stopped) => assert_eq!(stopped.step, Step::CreatePullRequest),
            other => panic!("a failed step must stop the sequence, not {other:?}"),
        }
    }

    /// Story 15: a step whose source could not take the write is **queued**,
    /// and the sequence waits for it rather than either reporting success or
    /// running the next step over an effect that does not exist yet.
    #[test]
    fn a_queued_step_holds_the_sequence_without_failing_it() {
        use StepOutcome::{Pending, Queued, Succeeded};
        let steps = sequence([Succeeded, Queued, Pending, Pending]);
        match advance(&steps) {
            Advance::Waiting(waiting) => assert_eq!(waiting.step, Step::CreatePullRequest),
            other => panic!("a queued step must hold the sequence, not {other:?}"),
        }
    }

    /// Story 14: skipping is how a ticket that needs no branch still gets its
    /// status moved. A skipped step is settled, so the sequence goes on.
    #[test]
    fn a_skipped_step_lets_the_sequence_carry_on() {
        use StepOutcome::{Pending, Skipped};
        let steps = sequence([Skipped, Skipped, Skipped, Pending]);
        match advance(&steps) {
            Advance::Run(next) => assert_eq!(next.step, Step::Transition),
            other => panic!("a skipped step must not stop the flow, not {other:?}"),
        }
    }

    /// A flow nobody has run yet starts at its first step, and one whose steps
    /// are all settled is over.
    #[test]
    fn a_flow_runs_from_the_front_and_ends_when_every_step_is_settled() {
        use StepOutcome::{Pending, Skipped, Succeeded};
        let fresh = sequence([Pending, Pending, Pending, Pending]);
        assert_eq!(advance(&fresh), Advance::Run(&fresh[0]));
        assert_eq!(
            advance(&sequence([Succeeded, Succeeded, Skipped, Succeeded])),
            Advance::Done
        );
        assert_eq!(advance(&[]), Advance::Done);
    }

    /// The two settled outcomes are exactly the two that mean the step is
    /// finished with -- the distinction [`advance`] walks.
    #[test]
    fn only_a_succeeded_or_skipped_step_is_settled() {
        let settled: Vec<_> = StepOutcome::ALL
            .iter()
            .filter(|o| o.is_settled())
            .copied()
            .collect();
        assert_eq!(
            settled,
            vec![StepOutcome::Succeeded, StepOutcome::Skipped],
            "a queued or running step has changed nothing at the source yet"
        );
    }
}
