//! The start-work flow: one reviewable sequence from a ticket (issue #44).
//!
//! **Orchestration only.** Nothing here is a new capability. Every side effect
//! is an existing `WriteOp` (#43) dispatched through the write queue (#42),
//! plus a link created through the link store -- and the orchestrator does not
//! so much as name the SPI's op enum: it moves the serialized payloads
//! `plan` composed, exactly as the `submit_write` command does, and for the
//! same reason (the enum grows per milestone, ADR-0006).
//!
//! # Where the seam is, and why it is here
//!
//! The single seam this feature adds is *a sequence of named steps with an
//! explicit outcome each* -- `knobas_core::start_work`. It is placed at the
//! **highest point**: one orchestrator that composes ops, rather than
//! start-work logic spread across the ops themselves. `WriteOp::CreateBranch`
//! knows nothing about pull requests; the write queue knows nothing about
//! sequences; the adapters know nothing about either. Everything that makes
//! four writes into one reviewable flow is in this module and in the step list
//! it reads.
//!
//! The consequence a later reader should not undo: **[`Steps`] is the whole of
//! this module's contact with the outside world.** The orchestrator is pure
//! policy over that trait plus the step store, which is what lets its real
//! behaviour -- which ops in which order, and what happens when one fails -- be
//! driven by a fake and asserted on directly.
//!
//! # What "retry this step" means
//!
//! It is the hard question here, because `knobas-http` retries a `POST` the way
//! it retries a `GET` (#122): a `create_branch` whose first attempt landed and
//! whose response was lost looks, from knobas, exactly like one that never
//! happened. Adding an idempotency key is not available -- that is frozen
//! surface and unruled -- so the answer is three rules, in order, and none of
//! them is "send it again and hope":
//!
//! 1. **A step with a write still open is retried by acting on that write, not
//!    by making a second one.** The queue row is the record of what was asked
//!    for; #42 already owns its exits (it flushes, it holds, it is amended, it
//!    is discarded). A retry reads the row's state and reports it. This is the
//!    whole of the transient case, and it cannot duplicate anything.
//! 2. **Otherwise, look before writing.** A refused write is a definite answer
//!    from the source -- and the definite answer a lost `POST` produces is
//!    "that already exists". So the retry asks the mirror whether the effect is
//!    *there*: the branch, the pull request from that head, the link. If it is,
//!    the step settles **succeeded** and nothing is sent. This is the same
//!    read story 22 needs anyway ("a ticket that already has a branch and a
//!    pull request says so rather than making a second set"), so it is one
//!    mechanism serving both.
//! 3. **When the read cannot settle it, dispatch, and let the source refuse.**
//!    The mirror may not have caught up, and a ticket's status has no
//!    adapter-independent read at all (#43 left that seam deliberately open).
//!    A duplicate the source refuses in its own words is a fact the user can
//!    act on; a duplicate created silently is not. A transition is safe here in
//!    a way a create is not -- moving a ticket to a status it is already in
//!    creates no second object.
//!
//! What none of this can promise is that a create *never* runs twice; only
//! #122 can, and it is open. What it promises is that knobas looks first, and
//! that when it cannot look, the second attempt is the source's to refuse
//! rather than knobas' to make silently.

pub mod merge;
pub mod plan;
pub mod queue;

use async_trait::async_trait;
use knobas_core::entity::EntityRef;
use knobas_core::start_work::{self as store, Advance, FlowStep, Step, StepOutcome};

pub use knobas_core::start_work::flow;
use sqlx::PgPool;

use crate::IpcError;

/// What the write queue did with a step's op.
///
/// A queue state, read as the step's business rather than as the queue's:
/// which of these a write is in is the whole difference between a step that
/// happened, one that will happen, and one that will not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Landing {
    /// Delivered to the source.
    Sent,
    /// A **pending write**: the source could not take it, so it waits and the
    /// queue will retry it on its own.
    Waiting { detail: Option<String> },
    /// A **held write**: the target changed after the write was queued. It goes
    /// nowhere until the user chooses, in the queue's own panel, where both
    /// versions are shown side by side.
    Held { detail: Option<String> },
    /// The source refused this write, in its own words.
    Refused { detail: String },
    /// The row is gone: discarded, or never readable.
    Withdrawn,
}

/// Whether a link write drew a new edge or found one already there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Linked {
    Made,
    /// The pair already carried this relation -- which is a *success* for a
    /// step whose job is that the relationship exists, not a conflict.
    Already,
}

/// What the mirror holds from one head branch, as far as knobas can tell
/// (issue #359).
///
/// **A head branch is re-usable, so "the pull request from this head" is not a
/// single thing.** The mirror deliberately holds closed and merged pull
/// requests -- `knobas_source_gitea::client::Client::pulls` asks for
/// `state=all` and says why -- so a branch used twice leaves two records
/// behind, and a read that answered with one of them arbitrarily settled the
/// pull-request step "already open" against a pull request that was merged
/// months ago, and then linked the ticket to it.
///
/// Three answers rather than an `Option`, because the two ways of having
/// nothing to offer are different facts to the caller: a pull request somebody
/// already finished is a reason the user can act on, and no record at all is a
/// mirror that has not caught up.
///
/// # Which fact decides, and why it is not the declared one
///
/// Open-ness lives only in the verbatim `payload`, so this is an **ADR-0007
/// payload read outside an adapter** either way. There are two readings
/// available and they answer different questions:
///
/// * the **declared** merged flag (#277) answers *"was this merged"*, and
/// * the record's own `state` answers *"is this open"*.
///
/// The flow asks the second, and #359's criterion says so in as many words --
/// "distinguishes an open pull request from a merged **or closed** one". A
/// pull request closed without merging carries `merged: false`, so a rule
/// built on the declared flag alone calls it open and commits the whole bug
/// one state over. So `state` decides, read at
/// [`queue::PULL_REQUEST_BY_HEAD`](crate::start_work::queue), inside the one
/// named statement that already reads this record's head branch the same
/// interim way.
///
/// The declared flag is not discarded: it is the one reading a **source** owns
/// rather than knobas, so it can veto -- anything the declaration calls merged
/// is not open, whatever its state says -- and it is what words the refusal.
///
/// It cannot be the *deciding* fact, and `knobas-source-mock` is the shape of
/// why: it declares no merged flag at all, deliberately, because its merge is
/// a timestamp rather than a boolean. A source may decline that declaration
/// and still have pull requests knobas must be able to link, and `state` is
/// what answers for one. (The mock is an argument about shape and not a
/// corpus: its pull requests carry `from`/`to` and no `head` object, so
/// `queue::PULL_REQUEST_BY_HEAD` never reaches them either way. Gitea is the
/// only source this read reaches today.)
///
/// The **declared** route for open-ness would be `status_name` on the `pr`
/// kind, and it was left alone on purpose: `status_name` is what the mini
/// board, the room statements and the census render, so pointing it at a
/// forge's `state` changes what every one of those shows for a pull request.
/// That is a product decision, not this ticket's, and when it is made this
/// read expires into it -- which is what ADR-0007 says every interim read
/// does.
///
/// # The failure direction, pinned toward absence
///
/// ADR-0007's requirement 1 decides which way this fails, in as many words:
/// such a read is confined to derivations *"whose tolerable failure is an
/// absent result -- never to a decision where a miss becomes a wrong action"*.
/// This read feeds a decision, so the miss lands on
/// [`Unknown`](Self::Unknown): a record is [`Open`](Self::Open) only where
/// knobas positively read it as open.
///
/// What that costs, on **both** paths it gates, stated rather than left to be
/// discovered:
///
/// * the **create** step can no longer settle from the mirror, so it
///   dispatches and the source refuses a duplicate in its own words -- rule 3
///   of this module's header, already the documented answer for a mirror that
///   cannot settle the question;
/// * the **link** step has nothing to link and settles *failed*, saying so.
///   That is a step the user can retry, and it is the direction that matters:
///   a link is permanent, `merge::follow_merges` reads exactly such links, and
///   a wrong one turns into an automatic transition off somebody else's merge.
///
/// The direction pinned the other way costs the bug it was reported as.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PullRequestOnHead {
    /// A pull request from this head that knobas read as open, and that
    /// nothing it read calls merged.
    Open(String),
    /// The mirror holds pull requests from this head and knobas can tell that
    /// none of them is open. Names one, and says whether it was merged, so the
    /// step that refuses can say which word.
    Closed { id: String, merged: bool },
    /// Nothing from this head has reached the mirror, or nothing whose state
    /// knobas could read -- one answer on purpose: in both, knobas has no open
    /// pull request it can name, and it may not act as though it has.
    Unknown,
}

/// Everything the orchestrator can do to the world.
///
/// Narrow on purpose, and every method is a capability rather than a
/// convenience: the fake that stands in for it in the tests is what those tests
/// assert on -- which ops were dispatched, in what order, and what happened
/// when one failed.
#[async_trait]
pub trait Steps: Send + Sync {
    /// Queue one write op, given as the serialized payload the step carries,
    /// and answer the queue row it made and how that row settled.
    ///
    /// The payload rather than a typed op: see the module header.
    async fn dispatch(&self, payload: &serde_json::Value) -> Result<(i64, Landing), IpcError>;

    /// What became of a write that was already dispatched.
    async fn landing_of(&self, write_id: i64) -> Result<Landing, IpcError>;

    /// Draw the knobas link between two entities.
    async fn link(&self, from: &str, to: &str, relation: &str) -> Result<Linked, IpcError>;

    /// The branch `name` of `repo`, as an entity id, if the mirror holds it.
    async fn branch(&self, repo: &EntityRef, name: &str) -> Result<Option<String>, IpcError>;

    /// What the mirror holds from head branch `head` in `repo`, and whether
    /// any of it is a pull request that is still open.
    ///
    /// Not "the pull request from this head": a head branch can be re-used, so
    /// there may be several, and which of them is open is the whole question.
    /// See [`PullRequestOnHead`].
    async fn pull_request(
        &self,
        repo: &EntityRef,
        head: &str,
    ) -> Result<PullRequestOnHead, IpcError>;

    /// Re-read a source now, and wait for that run to end.
    ///
    /// The only way the mirror learns a pull request's number: `Source::write`
    /// answers nothing, so what was created is found by reading it back.
    async fn refresh(&self, source: &str);
}

/// What the queue's answer means to the step that made it.
///
/// The one place the mapping is written, and each arm is a decision:
///
/// * **Waiting is not failure.** Story 15: a source that cannot take the write
///   delays the flow rather than ending it. The step reports `queued`, which is
///   neither a success the user would act on nor a failure they would retry.
/// * **Held is not `queued`.** A held write will *never* go on its own -- it is
///   terminal until the user acts, in the write queue's own panel, which is
///   where both versions are shown side by side. Reporting it as merely waiting
///   would leave the user watching a step that is not going to move.
/// * **Refused carries the source's own sentence**, so a permanent failure is
///   reportable rather than merely counted.
#[must_use]
pub fn outcome_of(landing: &Landing) -> (StepOutcome, Option<String>) {
    match landing {
        Landing::Sent => (StepOutcome::Succeeded, None),
        Landing::Waiting { detail } => (StepOutcome::Queued, detail.clone()),
        Landing::Held { detail } => (
            StepOutcome::Failed,
            Some(match detail {
                Some(said) => format!(
                    "the target changed after this was queued, so the write is held: {said}"
                ),
                None => "the target changed after this was queued, so the write is held \
                         -- decide it in the write queue"
                    .to_owned(),
            }),
        ),
        Landing::Refused { detail } => (StepOutcome::Failed, Some(detail.clone())),
        Landing::Withdrawn => (
            StepOutcome::Failed,
            Some("the write was discarded before it went".to_owned()),
        ),
    }
}

/// Propose a flow for a ticket and write it down, ready to be reviewed.
///
/// Nothing is dispatched here: the whole sequence is composed and stored so it
/// can be **shown before anything happens**, which is the difference between a
/// stepper and a macro.
///
/// # Errors
///
/// [`Conflict`](crate::IpcErrorCode::Conflict) if the ticket already has a flow
/// -- story 22, and the refusal is the table's unique constraint rather than a
/// check somebody could forget; otherwise as
/// [`plan::subject`](plan::subject).
pub async fn begin(
    pool: &PgPool,
    ticket: &EntityRef,
    repo: &EntityRef,
) -> Result<Vec<FlowStep>, IpcError> {
    let existing = store::flow(pool, ticket).await?;
    if !existing.is_empty() {
        return Err(IpcError::conflict(format!(
            "{ticket} already has a start-work flow -- open it rather than starting a second"
        )));
    }
    let subject = plan::subject(pool, ticket, repo).await?;
    let steps = store::plan(pool, ticket, &plan::propose(&subject)).await?;
    Ok(steps)
}

/// Run the flow as far as it will go.
///
/// One step at a time, re-reading the step list between each, so **a failed
/// step stops the sequence** rather than the loop remembering to check: the
/// only step it can be handed is the one [`store::advance`] answers with, and
/// there is no answer of that shape behind a stopped step.
///
/// Returns the flow as it stands when it stops -- done, waiting on a queued
/// write, or stopped at a step the user has to decide about (story 16).
///
/// # Errors
///
/// [`IpcError`] if the step store or the dispatcher fails. A source refusing a
/// write is **not** an error here: that is a step outcome, which is the whole
/// point.
pub async fn run(
    pool: &PgPool,
    steps: &dyn Steps,
    ticket: &EntityRef,
) -> Result<Vec<FlowStep>, IpcError> {
    let mut last: Option<i64> = None;
    loop {
        let flow = store::flow(pool, ticket).await?;
        let id = match store::advance(&flow) {
            Advance::Run(step) => step.id,
            // Waiting, stopped or done: in every case the sequence goes no
            // further without the user, and the flow as it stands is the
            // answer.
            _ => return Ok(flow),
        };

        // **Progress, or raise.** Being handed the same step twice running means
        // the sequence did not move, and a loop that kept going would spin for
        // ever against a live database and a real source. It cannot happen while
        // `advance` stops at a failed step and `perform` settles what it was
        // given, so reaching it is knobas' own bug -- and it is raised rather
        // than returned quietly, because a flow that silently stopped where it
        // should have gone on is the failure a stepper is least able to explain.
        //
        // Loud, and *not* a substitute for the rule above it: an edit that broke
        // `advance`'s stop-on-failure would fail these tests here as well as in
        // `knobas-core`, rather than hanging them.
        if last == Some(id) {
            return Err(IpcError::internal(format!(
                "the start-work flow was handed step {id} twice without it moving"
            )));
        }
        last = Some(id);

        perform(pool, steps, &flow, id).await?;
    }
}

/// Retry one step, without redoing the ones that succeeded (story 13).
///
/// The three rules the module header states, in order: act on the write that
/// already exists; otherwise look before writing; otherwise dispatch and let
/// the source refuse a duplicate in its own words.
///
/// Retrying a step that is not the one the sequence is on is refused rather
/// than obeyed -- running the third step over a first that failed is the exact
/// thing story 12 forbids.
///
/// # Errors
///
/// [`NotFound`](crate::IpcErrorCode::NotFound) if no step carries `step_id`;
/// [`Conflict`](crate::IpcErrorCode::Conflict) if an earlier step is stopping
/// the sequence.
pub async fn retry(
    pool: &PgPool,
    steps: &dyn Steps,
    step_id: i64,
) -> Result<Vec<FlowStep>, IpcError> {
    let step = step_of(pool, step_id).await?;
    let ticket = EntityRef::parse(&step.ticket_id).map_err(IpcError::invalid)?;
    guard_position(pool, &ticket, step_id).await?;

    // Rule 1: a write that is still open is the retry. Nothing is queued twice.
    if let Some(write_id) = step.write_id {
        let landing = steps.landing_of(write_id).await?;
        if !matches!(landing, Landing::Refused { .. } | Landing::Withdrawn) {
            let (outcome, detail) = outcome_of(&landing);
            store::settle(pool, step_id, outcome, None, detail.as_deref()).await?;
            return run(pool, steps, &ticket).await;
        }
    }

    // Rules 2 and 3 are `perform`'s: it reads before it writes, and dispatches
    // only what the mirror could not already account for.
    store::settle(pool, step_id, StepOutcome::Pending, None, None).await?;
    run(pool, steps, &ticket).await
}

/// Skip a step (story 14), so a ticket that needs no branch can still get its
/// status moved.
///
/// A skipped step is settled, so the sequence carries on past it.
///
/// # Errors
///
/// [`NotFound`](crate::IpcErrorCode::NotFound) if no step carries `step_id`;
/// [`Conflict`](crate::IpcErrorCode::Conflict) if the step already succeeded
/// -- its effect exists at the source, and marking it skipped would only
/// rewrite the record of what happened.
pub async fn skip(
    pool: &PgPool,
    steps: &dyn Steps,
    step_id: i64,
) -> Result<Vec<FlowStep>, IpcError> {
    let step = step_of(pool, step_id).await?;
    if step.outcome == StepOutcome::Succeeded {
        return Err(IpcError::conflict(format!(
            "the {} step has already happened -- skipping it now would only rewrite the record",
            step.step
        )));
    }
    let ticket = EntityRef::parse(&step.ticket_id).map_err(IpcError::invalid)?;
    store::settle(pool, step_id, StepOutcome::Skipped, None, Some("skipped")).await?;
    note(pool, &step, "skipped", "the user chose not to run it").await;
    run(pool, steps, &ticket).await
}

/// Replace a step's proposal with the one the user edited, and return it to the
/// front of the queue.
///
/// The whole reason the proposal is a value the user can change: a bad
/// automatic branch name is not a commitment, and a source that refused one
/// will refuse it again unchanged.
///
/// # Errors
///
/// [`NotFound`](crate::IpcErrorCode::NotFound) if no step carries `step_id`;
/// [`Conflict`](crate::IpcErrorCode::Conflict) if the step has already run.
pub async fn repropose(
    pool: &PgPool,
    step_id: i64,
    payload: serde_json::Value,
) -> Result<Vec<FlowStep>, IpcError> {
    let step = step_of(pool, step_id).await?;
    if step.outcome == StepOutcome::Succeeded {
        return Err(IpcError::conflict(format!(
            "the {} step has already happened -- editing it now would change nothing at the source",
            step.step
        )));
    }
    let ticket = EntityRef::parse(&step.ticket_id).map_err(IpcError::invalid)?;
    store::repropose(pool, step_id, payload).await?;
    Ok(store::flow(pool, &ticket).await?)
}

/// One step, or `not_found`.
async fn step_of(pool: &PgPool, step_id: i64) -> Result<FlowStep, IpcError> {
    store::get(pool, step_id)
        .await?
        .ok_or_else(|| IpcError::not_found(format!("no start-work step with id {step_id}")))
}

/// Refuse to touch a step the sequence has not reached.
///
/// Without this, a stepper that offered *Retry* on every row would let the user
/// run the transition over a branch creation that failed -- the exact outcome
/// story 12 exists to prevent, arriving through the UI instead of through the
/// loop.
async fn guard_position(pool: &PgPool, ticket: &EntityRef, step_id: i64) -> Result<(), IpcError> {
    let flow = store::flow(pool, ticket).await?;
    let blocking = flow
        .iter()
        .find(|step| !step.outcome.is_settled() && step.id != step_id);
    match blocking {
        Some(step) if step.position < position_of(&flow, step_id) => {
            Err(IpcError::conflict(format!(
                "the {} step is where this flow stopped -- deal with it first",
                step.step
            )))
        }
        _ => Ok(()),
    }
}

/// Where a step sits, or `i32::MAX` for one that is not in the flow at all --
/// which cannot happen, and which must not read as "at the front".
fn position_of(flow: &[FlowStep], step_id: i64) -> i32 {
    flow.iter()
        .find(|step| step.id == step_id)
        .map_or(i32::MAX, |step| step.position)
}

/// Run one step and record what happened to it.
///
/// Every arm settles the step to something that is not `pending`, which is what
/// makes [`run`]'s loop terminate.
async fn perform(
    pool: &PgPool,
    steps: &dyn Steps,
    flow: &[FlowStep],
    step_id: i64,
) -> Result<(), IpcError> {
    let step = flow
        .iter()
        .find(|step| step.id == step_id)
        .ok_or_else(|| IpcError::internal("the step the flow answered with is not in it"))?;

    match step.step {
        Step::CreateBranch => {
            let (repo, name) = branch_of(step)?;
            // Rule 2: look before writing. A branch that is already there is a
            // step that has already happened -- whether an earlier attempt made
            // it (#122's lost `POST`) or somebody made it by hand (story 22).
            if let Some(existing) = steps.branch(&repo, &name).await? {
                return settle_ok(pool, step, &format!("{existing} is already there")).await;
            }
            dispatch(pool, steps, step, &step.payload).await
        }
        Step::CreatePullRequest => {
            let (repo, head) = pull_request_of(step)?;
            match steps.pull_request(&repo, &head).await? {
                // Rule 2 again, and it asks the *right* question now (#359): a
                // pull request that is open from this head is this step's
                // effect, whoever made it.
                PullRequestOnHead::Open(existing) => {
                    settle_ok(pool, step, &format!("{existing} is already open")).await
                }
                // **A finished pull request on this head opens a new one.**
                // The alternative -- refusing, and telling the user to pick
                // another branch name -- would make knobas the one saying no,
                // on the strength of a payload read, about the ordinary case: a
                // head re-used for the next piece of work on the same ticket. A
                // forge does not refuse a pull request from a head whose last
                // one was merged or closed, because there is nothing wrong with
                // it; the new commits on that head are exactly what wants
                // reviewing. If the forge disagrees it refuses in its own
                // words, which is rule 3 and is a fact the user can act on.
                //
                // `Unknown` takes the same road for the reason
                // `PullRequestOnHead` states: an unsettled read dispatches and
                // lets the source refuse a duplicate, rather than knobas
                // guessing that the thing it cannot see is there.
                PullRequestOnHead::Closed { .. } | PullRequestOnHead::Unknown => {
                    dispatch(pool, steps, step, &step.payload).await
                }
            }
        }
        Step::LinkPullRequest => link_step(pool, steps, flow, step).await,
        // A transition has no adapter-independent read (#43 left that seam
        // open, and this is the flow that meets it): there is no way to ask
        // whether the ticket is already in the status. Dispatching is safe in a
        // way a create is not -- a status a ticket is already in creates no
        // second object -- so it goes, and the adapter resolves the name
        // against what the source says is reachable right now.
        Step::Transition => dispatch(pool, steps, step, &step.payload).await,
    }
}

/// Queue a step's op and record how the queue settled it.
///
/// The step is marked **running** for the duration (story 11): the row is
/// what the stepper polls while the flow runs, and it is also what an
/// interrupted session reads back -- a step found `running` with no landing
/// was cut off mid-dispatch, which is honest in a way `pending` is not.
async fn dispatch(
    pool: &PgPool,
    steps: &dyn Steps,
    step: &FlowStep,
    payload: &serde_json::Value,
) -> Result<(), IpcError> {
    store::settle(pool, step.id, StepOutcome::Running, None, None).await?;
    match steps.dispatch(payload).await {
        Ok((write_id, landing)) => {
            let (outcome, detail) = outcome_of(&landing);
            store::settle(pool, step.id, outcome, Some(write_id), detail.as_deref()).await?;
            Ok(())
        }
        Err(error) => {
            // The queue would not even take it -- an op the source does not
            // declare, a target that is not an entity id, a source that is not
            // configured. That is a failed step carrying the reason, not a
            // failed command: the rest of the flow is still the user's to run.
            store::settle(
                pool,
                step.id,
                StepOutcome::Failed,
                None,
                Some(&error.message),
            )
            .await?;
            // The queue never saw this write, so nothing else logs it
            // (story 20).
            note(pool, step, "failed", &error.message).await;
            Ok(())
        }
    }
}

/// The link back to the ticket -- knobas-owned, local, and never written to
/// either source.
///
/// The pull request has to be *found* first: `Source::write` answers nothing,
/// so knobas does not learn the number it created and reads it back out of the
/// mirror by the head branch it opened from. A refresh is asked for once, and
/// only once, before giving up: the step is retryable, and a flow that spun
/// waiting for a sync would be the stuck step story 11 wants distinguishable.
async fn link_step(
    pool: &PgPool,
    steps: &dyn Steps,
    flow: &[FlowStep],
    step: &FlowStep,
) -> Result<(), IpcError> {
    let Some(pr_step) = flow.iter().find(|s| s.step == Step::CreatePullRequest) else {
        return settle_failed(pool, step, "this flow has no pull request step").await;
    };
    if pr_step.outcome == StepOutcome::Skipped {
        return settle_failed(
            pool,
            step,
            "the pull request step was skipped, so there is nothing to link",
        )
        .await;
    }
    let (repo, head) = pull_request_of(pr_step)?;

    let mut found = steps.pull_request(&repo, &head).await?;
    // Anything but an open one is a reason to look again, not only nothing at
    // all (#359): where the head was re-used, the merged pull request is
    // precisely what the mirror already holds, and the one this flow just made
    // is the one the refresh fetches.
    if !matches!(found, PullRequestOnHead::Open(_)) {
        steps.refresh(&repo.namespace).await;
        found = steps.pull_request(&repo, &head).await?;
    }
    let pr = match found {
        PullRequestOnHead::Open(pr) => pr,
        // **The bug this step must not commit.** Drawing the ticket's link to
        // a merged pull request would record, permanently and in the panel the
        // user trusts, that this flow's work is that pull request's -- and the
        // reverse direction reads exactly such links, so it would then move the
        // ticket to In Review off a merge that happened before the flow began.
        // A failed step is recoverable and says what it saw; a wrong link is
        // neither.
        PullRequestOnHead::Closed { id, merged } => {
            let word = if merged { "merged" } else { "closed" };
            return settle_failed(
                pool,
                step,
                &format!(
                    "no pull request from {head} is open -- {id} is {word}, and linking to it \
                     would claim work this flow did not do"
                ),
            )
            .await;
        }
        PullRequestOnHead::Unknown => {
            return settle_failed(
                pool,
                step,
                &format!("no open pull request from {head} has reached the mirror yet"),
            )
            .await;
        }
    };

    let relation = step
        .payload
        .get("relation")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(plan::RELATION);
    match steps.link(&pr, &step.ticket_id, relation).await {
        // Already linked is a success: the step's job is that the relationship
        // exists, not that this attempt is the one that made it. It is also
        // what makes the step idempotent under retry with no read of its own.
        Ok(Linked::Made | Linked::Already) => {
            settle_ok(pool, step, &format!("linked to {pr}")).await
        }
        Err(error) => settle_failed(pool, step, &error.message).await,
    }
}

/// The repository and branch name a `create_branch` step will use.
fn branch_of(step: &FlowStep) -> Result<(EntityRef, String), IpcError> {
    let op = &step.payload["CreateBranch"];
    let entity = field(op, "entity")?;
    Ok((
        EntityRef::parse(&entity).map_err(IpcError::invalid)?,
        field(op, "name")?,
    ))
}

/// The repository and head branch a `create_pull_request` step will use.
fn pull_request_of(step: &FlowStep) -> Result<(EntityRef, String), IpcError> {
    let op = &step.payload["CreatePullRequest"];
    let entity = field(op, "entity")?;
    Ok((
        EntityRef::parse(&entity).map_err(IpcError::invalid)?,
        field(op, "head")?,
    ))
}

/// One string out of a stored op payload.
///
/// The payload is a value the webview has been holding, so a missing field is
/// something to refuse rather than to unwrap.
fn field(op: &serde_json::Value, name: &str) -> Result<String, IpcError> {
    op.get(name)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| IpcError::invalid(format!("this step's proposal has no {name:?}")))
}

async fn settle_ok(pool: &PgPool, step: &FlowStep, detail: &str) -> Result<(), IpcError> {
    store::settle(pool, step.id, StepOutcome::Succeeded, None, Some(detail)).await?;
    note(pool, step, "succeeded", detail).await;
    Ok(())
}

async fn settle_failed(pool: &PgPool, step: &FlowStep, detail: &str) -> Result<(), IpcError> {
    store::settle(pool, step.id, StepOutcome::Failed, None, Some(detail)).await?;
    note(pool, step, "failed", detail).await;
    Ok(())
}

/// Put a step's settlement in the activity log (story 20).
///
/// Only for the settlements the queue never sees: a step that succeeded
/// against the mirror, failed before a write existed, or was skipped. A
/// dispatched write already gets its line from the queue itself, and a second
/// one here would say the same thing twice. Best-effort, like the queue's own:
/// a step that settled is settled, and a lost log line is not a reason to
/// fail it.
async fn note(pool: &PgPool, step: &FlowStep, verb: &str, detail: &str) {
    let entity = EntityRef::parse(&step.ticket_id).ok();
    let body =
        serde_json::json!({ "flow": "start-work", "step": step.step.as_str(), "detail": detail });
    if let Err(error) =
        knobas_core::activity::record(pool, "user", verb, entity.as_ref(), body).await
    {
        tracing::warn!(%error, step = %step.step, "a start-work activity line failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Story 15's distinction, pinned: a source that cannot take a write yet
    /// leaves the step **queued**, which is neither a success the user acts on
    /// nor a failure they retry. Reporting it as succeeded is the bug this
    /// guards -- the flow's completion and the write's delivery are different
    /// events.
    #[test]
    fn a_write_the_source_could_not_take_leaves_the_step_queued() {
        let (outcome, detail) = outcome_of(&Landing::Waiting {
            detail: Some("the server did not answer".to_owned()),
        });
        assert_eq!(outcome, StepOutcome::Queued);
        assert_eq!(detail.as_deref(), Some("the server did not answer"));
    }

    /// A **held** write is not merely waiting: it will never go on its own, so
    /// a step reporting it as queued would leave the user watching something
    /// that is not going to move. It fails, and says where the decision lives.
    #[test]
    fn a_held_write_fails_its_step_rather_than_leaving_it_waiting() {
        let (outcome, detail) = outcome_of(&Landing::Held { detail: None });
        assert_eq!(
            outcome,
            StepOutcome::Failed,
            "a held write waits for the user and for nothing else"
        );
        assert!(
            detail.expect("a reason").contains("held"),
            "the step has to say what happened"
        );
    }

    /// A refusal carries the source's own sentence up to the stepper, so a
    /// permanent failure is reportable rather than merely counted.
    #[test]
    fn a_refusal_carries_what_the_source_said() {
        let (outcome, detail) = outcome_of(&Landing::Refused {
            detail: "branch already exists".to_owned(),
        });
        assert_eq!(outcome, StepOutcome::Failed);
        assert_eq!(detail.as_deref(), Some("branch already exists"));
    }

    /// Delivered is the only landing that succeeds a step.
    #[test]
    fn only_a_delivered_write_succeeds_its_step() {
        let succeeded: Vec<_> = [
            Landing::Sent,
            Landing::Waiting { detail: None },
            Landing::Held { detail: None },
            Landing::Refused {
                detail: String::new(),
            },
            Landing::Withdrawn,
        ]
        .into_iter()
        .filter(|landing| outcome_of(landing).0 == StepOutcome::Succeeded)
        .collect();
        assert_eq!(succeeded, vec![Landing::Sent]);
    }
}
