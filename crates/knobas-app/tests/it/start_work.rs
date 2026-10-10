//! The start-work flow's behaviour, driven by a fake dispatcher (issue #44).
//!
//! **What is asserted here is the sequence's observable behaviour** -- which
//! ops were dispatched, in what order, and what happened when one failed --
//! never the internal shape of the step list. A test that reached into
//! `FlowStep` to check a field would be pinned to today's storage rather than
//! to the promise the stepper makes.
//!
//! # The failure paths are the load-bearing ones
//!
//! A stepper whose happy path works is close to vacuous: every macro's happy
//! path works, and a macro is what this deliberately is not. The three
//! properties worth having are that **a failed step stops the sequence**, that
//! it **leaves the earlier steps' effects intact**, and that it is
//! **resumable without repeating them**. Those are the first tests in this
//! file, and the happy path is the last.
//!
//! # The fake
//!
//! [`Fake`] records every dispatch in order and answers whatever the test set
//! up. That recording is the assertion surface: "the pull request was never
//! attempted" is a fact about the call log, which is the only place it is
//! visible -- a step's stored outcome would say the step did not succeed and
//! not whether knobas nevertheless sent something.

use std::sync::Mutex;

use async_trait::async_trait;
use knobas_app::IpcErrorCode;
use knobas_app::start_work::{self, Landing, Linked, PullRequestOnHead, Steps, queue};
use knobas_core::entity::EntityRef;
use knobas_core::start_work::{FlowStep, Step, StepOutcome};
use sqlx::PgPool;

/// The ticket every flow below is about, and the repository it works in.
const TICKET: &str = "jira:PAY-231";
const REPO: &str = "gitea:tidewater/payout-service";
const PR: &str = "gitea:tidewater/payout-service#142";
/// A head branch re-used: `#7` is finished, `#8` is the live one.
///
/// The numbers are chosen so that **`#7` sorts before `#8`**, which is what
/// makes these fixtures able to witness #359 at all: the read this replaced
/// took `order by entity_id limit 1` over every pull request the head ever
/// had, so a merged one that sorts first is the one it answered with. A pair
/// like `#7` and `#142` would hide the bug -- `#142` sorts first and happens to
/// be the open one.
const PR_MERGED: &str = "gitea:tidewater/payout-service#7";
const PR_REOPENED: &str = "gitea:tidewater/payout-service#8";

/// What the fake will answer for one op identifier.
#[derive(Clone, Debug)]
enum Answer {
    Landed(Landing),
    /// The queue refused to take the op at all -- an op the source does not
    /// declare, a source that is not configured.
    Rejected(String),
}

/// One pull request the fake's mirror holds: the repository, the head branch,
/// and the row `start_work::queue`'s statement would answer for it.
///
/// The row's `merged` and `state` are both optional because the real reads
/// **miss** rather than guess -- an undeclared flag, an absent state -- and a
/// fixture has to be able to put the mirror in that state: it is the direction
/// #359 pinned.
type MirroredPull = (String, String, queue::OnHead);

/// The row the mirror would answer for one pull request.
fn on_head(id: &str, merged: Option<bool>, state: Option<&str>) -> queue::OnHead {
    queue::OnHead {
        entity_id: id.to_owned(),
        merged,
        state: state.map(str::to_owned),
    }
}

#[derive(Default)]
struct Recorded {
    /// Every op identifier dispatched, in order. The assertion surface.
    dispatched: Vec<String>,
    /// Every `(from, to)` linked.
    linked: Vec<(String, String)>,
    /// How many times a source was asked to be re-read.
    refreshed: usize,
}

/// A dispatcher a test dictates.
struct Fake {
    /// Per op identifier: what the queue does with it.
    answers: Mutex<std::collections::HashMap<String, Answer>>,
    /// Per write id, for [`Steps::landing_of`] -- what a *retry* reads.
    landings: Mutex<std::collections::HashMap<i64, Landing>>,
    /// The branches the mirror holds, as `(repo, name)`.
    branches: Mutex<Vec<(String, String)>>,
    /// The pull requests the mirror holds.
    pulls: Mutex<Vec<MirroredPull>>,
    /// What a refresh puts into the mirror, if anything.
    on_refresh: Mutex<Option<MirroredPull>>,
    log: Mutex<Recorded>,
    next_write_id: Mutex<i64>,
}

impl Fake {
    /// Everything succeeds and the mirror is empty.
    fn new() -> Self {
        Self {
            answers: Mutex::new(std::collections::HashMap::new()),
            landings: Mutex::new(std::collections::HashMap::new()),
            branches: Mutex::new(Vec::new()),
            pulls: Mutex::new(Vec::new()),
            on_refresh: Mutex::new(None),
            log: Mutex::new(Recorded::default()),
            next_write_id: Mutex::new(1),
        }
    }

    fn answering(self, op: &str, answer: Answer) -> Self {
        self.answers.lock().unwrap().insert(op.to_owned(), answer);
        self
    }

    /// A refresh is what brings this pull request into the mirror -- the real
    /// shape, since `Source::write` answers nothing and the number is learned
    /// by reading back.
    fn revealing_on_refresh(self, head: &str) -> Self {
        *self.on_refresh.lock().unwrap() = Some((
            REPO.to_owned(),
            head.to_owned(),
            on_head(PR, Some(false), Some("open")),
        ));
        self
    }

    /// Put one pull request into the mirror this fake stands for.
    fn holding(self, head: &str, row: queue::OnHead) -> Self {
        self.pulls
            .lock()
            .unwrap()
            .push((REPO.to_owned(), head.to_owned(), row));
        self
    }

    fn ops(&self) -> Vec<String> {
        self.log.lock().unwrap().dispatched.clone()
    }

    fn links(&self) -> Vec<(String, String)> {
        self.log.lock().unwrap().linked.clone()
    }

    fn refreshes(&self) -> usize {
        self.log.lock().unwrap().refreshed
    }

    /// Change what an op does, between runs -- a source that was down coming
    /// back, or a user fixing the branch name.
    fn now_answers(&self, op: &str, answer: Answer) {
        self.answers.lock().unwrap().insert(op.to_owned(), answer);
    }

    fn set_landing(&self, write_id: i64, landing: Landing) {
        self.landings.lock().unwrap().insert(write_id, landing);
    }
}

/// The op identifier a serialized `WriteOp` carries, read off the wire shape
/// rather than by decoding: the fake stands where the queue does, and the queue
/// receives exactly this value.
fn identifier(payload: &serde_json::Value) -> String {
    let tag = payload
        .as_object()
        .and_then(|map| map.keys().next().cloned())
        .expect("a serialized write op is a one-key object");
    match tag.as_str() {
        "CreateBranch" => "create_branch",
        "CreatePullRequest" => "create_pull_request",
        "Transition" => "transition",
        "Comment" => "comment",
        other => other,
    }
    .to_owned()
}

#[async_trait]
impl Steps for Fake {
    async fn dispatch(
        &self,
        payload: &serde_json::Value,
    ) -> Result<(i64, Landing), knobas_app::IpcError> {
        let op = identifier(payload);
        self.log.lock().unwrap().dispatched.push(op.clone());
        let answer = self
            .answers
            .lock()
            .unwrap()
            .get(&op)
            .cloned()
            .unwrap_or(Answer::Landed(Landing::Sent));
        match answer {
            Answer::Rejected(message) => Err(knobas_app::IpcError::invalid(message)),
            Answer::Landed(landing) => {
                let mut next = self.next_write_id.lock().unwrap();
                let id = *next;
                *next += 1;
                self.landings.lock().unwrap().insert(id, landing.clone());
                Ok((id, landing))
            }
        }
    }

    async fn landing_of(&self, write_id: i64) -> Result<Landing, knobas_app::IpcError> {
        Ok(self
            .landings
            .lock()
            .unwrap()
            .get(&write_id)
            .cloned()
            .unwrap_or(Landing::Withdrawn))
    }

    async fn link(
        &self,
        from: &str,
        to: &str,
        _relation: &str,
    ) -> Result<Linked, knobas_app::IpcError> {
        let mut log = self.log.lock().unwrap();
        let pair = (from.to_owned(), to.to_owned());
        let already = log.linked.contains(&pair);
        log.linked.push(pair);
        Ok(if already {
            Linked::Already
        } else {
            Linked::Made
        })
    }

    async fn branch(
        &self,
        repo: &EntityRef,
        name: &str,
    ) -> Result<Option<String>, knobas_app::IpcError> {
        Ok(self
            .branches
            .lock()
            .unwrap()
            .iter()
            .find(|(r, n)| r == &repo.to_string() && n == name)
            .map(|(r, n)| format!("{r}@refs/heads/{n}")))
    }

    async fn pull_request(
        &self,
        repo: &EntityRef,
        head: &str,
    ) -> Result<PullRequestOnHead, knobas_app::IpcError> {
        // **Classified by the real rule, not by a second one.** A fake that
        // decided for itself which of a head's pull requests counts as open
        // would let these tests pass against a rule the mirror does not have;
        // `pick_open` is the one `start_work::queue` runs over what its own
        // statement answered.
        let rows: Vec<queue::OnHead> = self
            .pulls
            .lock()
            .unwrap()
            .iter()
            .filter(|(r, h, _)| r == &repo.to_string() && h == head)
            .map(|(_, _, row)| row.clone())
            .collect();
        Ok(queue::pick_open(&rows))
    }

    async fn refresh(&self, _source: &str) {
        self.log.lock().unwrap().refreshed += 1;
        if let Some(revealed) = self.on_refresh.lock().unwrap().take() {
            self.pulls.lock().unwrap().push(revealed);
        }
    }
}

// -- the corpus ------------------------------------------------------------

/// A migrated, empty database of this test's own.
///
/// Per test rather than shared: a flow is keyed on its ticket and there is at
/// most one per ticket, so two tests starting a flow on `PAY-231` in one
/// database would decide each other's outcomes.
async fn scratch(name: &str) -> PgPool {
    knobas_db::test_util::scratch_database(name)
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database")
}

/// One live mirror item.
async fn item(pool: &PgPool, id: &str, kind: &str, title: &str, payload: serde_json::Value) {
    let entity = EntityRef::parse(id).expect("an entity id");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,$2,$3,$4,'',$5)",
    )
    .bind(id)
    .bind(&entity.namespace)
    .bind(kind)
    .bind(title)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
}

/// The ticket and the repository a flow needs to be proposed.
async fn corpus(name: &str) -> PgPool {
    let pool = scratch(name).await;
    item(
        &pool,
        TICKET,
        "ticket",
        "payout dashboard latency",
        serde_json::json!({"fields": {"status": {"name": "To Do"}}}),
    )
    .await;
    item(
        &pool,
        REPO,
        "repo",
        "tidewater/payout-service",
        serde_json::json!({"default_branch": "main"}),
    )
    .await;
    pool
}

fn ticket() -> EntityRef {
    EntityRef::parse(TICKET).unwrap()
}

fn repo() -> EntityRef {
    EntityRef::parse(REPO).unwrap()
}

/// A flow, planned but not run.
async fn planned(pool: &PgPool) -> Vec<FlowStep> {
    start_work::begin(pool, &ticket(), &repo())
        .await
        .expect("a flow is proposed")
}

/// One step of a flow, by kind.
fn step_of(flow: &[FlowStep], kind: Step) -> &FlowStep {
    flow.iter()
        .find(|step| step.step == kind)
        .unwrap_or_else(|| panic!("no {kind} step in the flow"))
}

fn outcome(flow: &[FlowStep], kind: Step) -> StepOutcome {
    step_of(flow, kind).outcome
}

/// The head branch the proposal chose, read off the stored proposal -- so the
/// tests follow whatever the plan proposes rather than restating it.
fn proposed_branch(flow: &[FlowStep]) -> String {
    step_of(flow, Step::CreateBranch).payload["CreateBranch"]["name"]
        .as_str()
        .expect("a branch name")
        .to_owned()
}

// -- the failure paths -----------------------------------------------------

/// **Story 12, and the property the whole stepper rests on.** A branch that
/// could not be created must not produce a pull request pointing at nothing.
///
/// Asserted on the *dispatch log*, which is the only place it is visible: the
/// step list would say the pull request did not succeed either way, and would
/// not say whether knobas nevertheless sent one.
#[tokio::test]
async fn a_failed_branch_never_produces_a_pull_request() {
    let pool = corpus("sw_failed_branch").await;
    planned(&pool).await;
    let fake = Fake::new().answering(
        "create_branch",
        Answer::Landed(Landing::Refused {
            detail: "reference does not exist".to_owned(),
        }),
    );

    let flow = start_work::run(&pool, &fake, &ticket()).await.unwrap();

    assert_eq!(
        fake.ops(),
        vec!["create_branch"],
        "the sequence went on past a step that failed"
    );
    assert!(fake.links().is_empty(), "nothing may be linked either");
    assert_eq!(outcome(&flow, Step::CreateBranch), StepOutcome::Failed);
    assert_eq!(
        outcome(&flow, Step::CreatePullRequest),
        StepOutcome::Pending
    );
    assert_eq!(outcome(&flow, Step::Transition), StepOutcome::Pending);
    assert_eq!(
        step_of(&flow, Step::CreateBranch).detail.as_deref(),
        Some("reference does not exist"),
        "a permanent failure has to be reportable, not merely counted"
    );
}

/// A failure part way through leaves what already happened intact: the earlier
/// step stays succeeded, and the write it made is still named. Story 16 -- "see
/// what the flow already did when it stopped partway" -- is this row.
#[tokio::test]
async fn a_failed_step_leaves_the_earlier_steps_effects_intact() {
    let pool = corpus("sw_intact").await;
    planned(&pool).await;
    let fake = Fake::new().answering(
        "create_pull_request",
        Answer::Landed(Landing::Refused {
            detail: "a pull request already exists for this head".to_owned(),
        }),
    );

    let flow = start_work::run(&pool, &fake, &ticket()).await.unwrap();

    assert_eq!(fake.ops(), vec!["create_branch", "create_pull_request"]);
    assert_eq!(outcome(&flow, Step::CreateBranch), StepOutcome::Succeeded);
    assert!(
        step_of(&flow, Step::CreateBranch).write_id.is_some(),
        "the write the branch step made is what a retry must not repeat"
    );
    assert_eq!(outcome(&flow, Step::CreatePullRequest), StepOutcome::Failed);
    assert_eq!(outcome(&flow, Step::Transition), StepOutcome::Pending);
}

/// **Story 13.** Retrying the step that failed does not redo the ones that
/// succeeded: `create_branch` appears once across both runs.
#[tokio::test]
async fn a_retry_resumes_without_repeating_what_already_worked() {
    let pool = corpus("sw_resume").await;
    let flow = planned(&pool).await;
    let head = proposed_branch(&flow);
    let fake = Fake::new()
        .answering(
            "create_pull_request",
            Answer::Landed(Landing::Refused {
                detail: "head and base are the same branch".to_owned(),
            }),
        )
        .revealing_on_refresh(&head);

    start_work::run(&pool, &fake, &ticket()).await.unwrap();
    let failed = step_of(
        &start_work::flow(&pool, &ticket()).await.unwrap(),
        Step::CreatePullRequest,
    )
    .id;

    // The user fixed the base and retried.
    fake.now_answers("create_pull_request", Answer::Landed(Landing::Sent));
    let flow = start_work::retry(&pool, &fake, failed).await.unwrap();

    assert_eq!(
        fake.ops(),
        vec![
            "create_branch",
            "create_pull_request",
            "create_pull_request",
            "transition"
        ],
        "the branch was created a second time, or the flow did not resume"
    );
    assert_eq!(outcome(&flow, Step::CreateBranch), StepOutcome::Succeeded);
    assert_eq!(outcome(&flow, Step::Transition), StepOutcome::Succeeded);
}

/// **The #122 case: a `POST` that landed and whose answer was lost.** The write
/// was refused with the source's duplicate message, and the branch is in fact
/// there. A retry reads before it writes, finds the effect, and sends nothing.
///
/// This is the whole of what knobas can do about a retried `POST` without an
/// idempotency key, and it is why the answer is "look first" rather than "send
/// it again".
#[tokio::test]
async fn a_retry_whose_effect_already_landed_sends_nothing() {
    let pool = corpus("sw_already_landed").await;
    let flow = planned(&pool).await;
    let head = proposed_branch(&flow);
    let fake = Fake::new().answering(
        "create_branch",
        Answer::Landed(Landing::Refused {
            detail: "branch already exists".to_owned(),
        }),
    );

    start_work::run(&pool, &fake, &ticket()).await.unwrap();
    assert_eq!(fake.ops(), vec!["create_branch"]);

    // It was there all along -- the first attempt made it and the response was
    // lost. The next sync mirrors it.
    fake.branches
        .lock()
        .unwrap()
        .push((REPO.to_owned(), head.clone()));
    fake.pulls.lock().unwrap().push((
        REPO.to_owned(),
        head,
        on_head(PR, Some(false), Some("open")),
    ));

    let failed = step_of(
        &start_work::flow(&pool, &ticket()).await.unwrap(),
        Step::CreateBranch,
    )
    .id;
    let flow = start_work::retry(&pool, &fake, failed).await.unwrap();

    assert_eq!(
        fake.ops(),
        vec!["create_branch", "transition"],
        "the retry created a second branch, or a second pull request"
    );
    assert_eq!(outcome(&flow, Step::CreateBranch), StepOutcome::Succeeded);
    assert_eq!(
        outcome(&flow, Step::CreatePullRequest),
        StepOutcome::Succeeded,
        "story 22: a pull request that is already open is reported, not duplicated"
    );
}

/// A retry of a step whose write is **still open** acts on that write rather
/// than queueing a second one. This is the transient case, and it is
/// duplicate-free by construction: the queue row is the record of what was
/// asked for, and #42 already owns its exits.
#[tokio::test]
async fn a_retry_of_a_step_whose_write_is_still_open_queues_nothing_new() {
    let pool = corpus("sw_still_open").await;
    let flow = planned(&pool).await;
    let fake = Fake::new()
        .revealing_on_refresh(&proposed_branch(&flow))
        .answering(
            "create_branch",
            Answer::Landed(Landing::Waiting {
                detail: Some("the server did not answer".to_owned()),
            }),
        );

    let flow = start_work::run(&pool, &fake, &ticket()).await.unwrap();
    assert_eq!(outcome(&flow, Step::CreateBranch), StepOutcome::Queued);
    let queued = step_of(&flow, Step::CreateBranch).id;

    // While the user was looking, the queue delivered it.
    fake.set_landing(1, Landing::Sent);
    let flow = start_work::retry(&pool, &fake, queued).await.unwrap();

    assert_eq!(
        fake.ops(),
        vec!["create_branch", "create_pull_request", "transition"],
        "the retry queued the branch a second time instead of reading the write it had"
    );
    assert_eq!(outcome(&flow, Step::CreateBranch), StepOutcome::Succeeded);
}

/// **Story 15.** A source that cannot take the write leaves the step *queued* --
/// not failed, and not succeeded -- and the sequence waits rather than running
/// the next step over an effect that does not exist yet. The flow's completion
/// and the write's delivery are different events.
#[tokio::test]
async fn a_source_that_cannot_take_the_write_queues_the_step_and_waits() {
    let pool = corpus("sw_queued").await;
    planned(&pool).await;
    let fake = Fake::new().answering(
        "create_branch",
        Answer::Landed(Landing::Waiting {
            detail: Some("the credential was refused".to_owned()),
        }),
    );

    let flow = start_work::run(&pool, &fake, &ticket()).await.unwrap();

    assert_eq!(
        fake.ops(),
        vec!["create_branch"],
        "a pull request was opened over a branch that has not been created"
    );
    assert_eq!(outcome(&flow, Step::CreateBranch), StepOutcome::Queued);
    assert_ne!(
        outcome(&flow, Step::CreateBranch),
        StepOutcome::Succeeded,
        "the flow must not report success for a write that has not gone"
    );
}

/// A **held** write is not merely waiting: it never goes on its own. The step
/// fails and says where the decision lives, rather than leaving the user
/// watching something that will not move.
#[tokio::test]
async fn a_held_write_stops_the_sequence_and_says_so() {
    let pool = corpus("sw_held").await;
    let flow = planned(&pool).await;
    let fake = Fake::new()
        .revealing_on_refresh(&proposed_branch(&flow))
        .answering(
            "transition",
            Answer::Landed(Landing::Held {
                detail: Some("somebody moved it to Done".to_owned()),
            }),
        );

    let flow = start_work::run(&pool, &fake, &ticket()).await.unwrap();

    assert_eq!(outcome(&flow, Step::Transition), StepOutcome::Failed);
    let detail = step_of(&flow, Step::Transition).detail.clone().unwrap();
    assert!(detail.contains("held"), "{detail}");
    assert!(detail.contains("somebody moved it to Done"), "{detail}");
}

/// A queue that refuses the op outright -- a source that does not declare it,
/// a target that names no configured source -- is a **failed step carrying the
/// reason**, not a failed command. The rest of the flow is still the user's to
/// run.
#[tokio::test]
async fn an_op_the_queue_will_not_take_fails_its_step_rather_than_the_call() {
    let pool = corpus("sw_rejected").await;
    planned(&pool).await;
    let fake = Fake::new().answering(
        "create_branch",
        Answer::Rejected("source \"gitea\" does not offer \"create_branch\"".to_owned()),
    );

    let flow = start_work::run(&pool, &fake, &ticket())
        .await
        .expect("a refused op is a step outcome, not an error");

    assert_eq!(outcome(&flow, Step::CreateBranch), StepOutcome::Failed);
    assert!(
        step_of(&flow, Step::CreateBranch)
            .detail
            .as_deref()
            .unwrap()
            .contains("does not offer"),
        "the step has to carry why"
    );
}

/// Story 12 again, arriving through the UI instead of through the loop: a
/// stepper that offered *Retry* on every row must not let the transition run
/// over a branch creation that failed.
#[tokio::test]
async fn a_step_the_sequence_has_not_reached_cannot_be_retried() {
    let pool = corpus("sw_out_of_order").await;
    planned(&pool).await;
    let fake = Fake::new().answering(
        "create_branch",
        Answer::Landed(Landing::Refused {
            detail: "no".to_owned(),
        }),
    );
    start_work::run(&pool, &fake, &ticket()).await.unwrap();

    let flow = start_work::flow(&pool, &ticket()).await.unwrap();
    let error = start_work::retry(&pool, &fake, step_of(&flow, Step::Transition).id)
        .await
        .expect_err("the flow is stopped two steps earlier");

    assert_eq!(error.code, IpcErrorCode::Conflict);
    assert_eq!(
        fake.ops(),
        vec!["create_branch"],
        "the transition ran over a branch that was never created"
    );
}

// -- skipping, and the rest of the sequence --------------------------------

/// **Story 14.** A ticket that needs no branch still gets its status moved.
#[tokio::test]
async fn skipping_the_branch_still_lets_the_ticket_move() {
    let pool = corpus("sw_skip").await;
    let flow = planned(&pool).await;
    let fake = Fake::new().revealing_on_refresh(&proposed_branch(&flow));

    let flow = start_work::skip(&pool, &fake, step_of(&flow, Step::CreateBranch).id)
        .await
        .unwrap();

    assert_eq!(outcome(&flow, Step::CreateBranch), StepOutcome::Skipped);
    assert_eq!(outcome(&flow, Step::Transition), StepOutcome::Succeeded);
    assert_eq!(
        fake.ops(),
        vec!["create_pull_request", "transition"],
        "a skipped step must not be dispatched"
    );
}

/// Skipping the pull request leaves nothing to link, and the link step says so
/// rather than quietly reporting success or linking something else.
#[tokio::test]
async fn skipping_the_pull_request_leaves_the_link_step_with_nothing_to_do() {
    let pool = corpus("sw_skip_pr").await;
    let flow = planned(&pool).await;
    let fake = Fake::new();

    let flow = start_work::skip(&pool, &fake, step_of(&flow, Step::CreatePullRequest).id)
        .await
        .unwrap();

    assert_eq!(outcome(&flow, Step::LinkPullRequest), StepOutcome::Failed);
    assert!(fake.links().is_empty());
    assert_eq!(outcome(&flow, Step::Transition), StepOutcome::Pending);
}

// -- the happy path --------------------------------------------------------

/// The four steps, in order, with the link drawn between the pull request the
/// mirror learned about and the ticket.
///
/// The refresh is the interesting part: `Source::write` answers nothing, so the
/// pull request's number is not something knobas is told -- it is read back
/// after a sync, by the head branch the flow opened from.
#[tokio::test]
async fn a_whole_flow_creates_a_branch_a_pull_request_a_link_and_a_transition() {
    let pool = corpus("sw_happy").await;
    let flow = planned(&pool).await;
    let head = proposed_branch(&flow);
    let fake = Fake::new().revealing_on_refresh(&head);

    let flow = start_work::run(&pool, &fake, &ticket()).await.unwrap();

    assert_eq!(
        fake.ops(),
        vec!["create_branch", "create_pull_request", "transition"],
        "the link is not a write-back -- it is knobas-owned and local"
    );
    assert_eq!(fake.refreshes(), 1, "the mirror is read back exactly once");
    assert_eq!(fake.links(), vec![(PR.to_owned(), TICKET.to_owned())]);
    for step in Step::ALL {
        assert_eq!(
            outcome(&flow, *step),
            StepOutcome::Succeeded,
            "the {step} step did not finish"
        );
    }
}

/// A pull request that never reaches the mirror leaves the link step failed and
/// retryable, rather than the flow spinning or claiming a link it does not have.
#[tokio::test]
async fn a_pull_request_the_mirror_has_not_seen_leaves_the_link_step_retryable() {
    let pool = corpus("sw_no_pr_yet").await;
    planned(&pool).await;
    let fake = Fake::new();

    let flow = start_work::run(&pool, &fake, &ticket()).await.unwrap();

    assert_eq!(outcome(&flow, Step::LinkPullRequest), StepOutcome::Failed);
    assert_eq!(fake.refreshes(), 1, "it asked for a sync before giving up");
    assert!(fake.links().is_empty());
    assert_eq!(outcome(&flow, Step::Transition), StepOutcome::Pending);
}

/// **Story 22.** Running the flow again on a ticket that already has one
/// reports that state rather than proposing a second set of writes.
#[tokio::test]
async fn a_second_flow_on_one_ticket_is_refused() {
    let pool = corpus("sw_twice").await;
    planned(&pool).await;

    let error = start_work::begin(&pool, &ticket(), &repo())
        .await
        .expect_err("one flow per ticket");

    assert_eq!(error.code, IpcErrorCode::Conflict);
    assert_eq!(
        start_work::flow(&pool, &ticket()).await.unwrap().len(),
        Step::ALL.len(),
        "the refused second attempt must not have written any steps"
    );
}

/// The proposal is a value the user changes, and the changed value is what is
/// sent: a bad automatic branch name is not a commitment.
#[tokio::test]
async fn an_edited_proposal_is_what_gets_dispatched() {
    let pool = corpus("sw_edit").await;
    let flow = planned(&pool).await;
    let branch = step_of(&flow, Step::CreateBranch);
    let mut edited = branch.payload.clone();
    edited["CreateBranch"]["name"] = serde_json::json!("knobas-PAY-231");

    let flow = start_work::repropose(&pool, branch.id, edited)
        .await
        .unwrap();

    assert_eq!(proposed_branch(&flow), "knobas-PAY-231");
    assert_eq!(outcome(&flow, Step::CreateBranch), StepOutcome::Pending);
}

// -- a re-used head branch (#359) ------------------------------------------

/// **The read, against a real mirror.** A head branch used twice leaves two
/// records behind -- the mirror holds merged pull requests deliberately
/// (`state=all`) -- and the flow must find the open one.
///
/// This is the half the [`Fake`] cannot witness: it stands where the queue
/// does, so the statement itself is only under test here.
#[tokio::test]
async fn a_merged_pull_request_on_a_re_used_head_is_not_the_open_one() {
    let pool = corpus("sw_head_reused").await;
    let head = "feature/PAY-231";
    item(
        &pool,
        PR_MERGED,
        "pr",
        "payout dashboard latency",
        serde_json::json!({"merged": true, "state": "closed", "head": {"ref": head}}),
    )
    .await;
    item(
        &pool,
        PR_REOPENED,
        "pr",
        "payout dashboard latency, part two",
        serde_json::json!({"merged": false, "state": "open", "head": {"ref": head}}),
    )
    .await;

    let found = queue::pull_request_on_head(&pool, &declarations(), &repo(), head)
        .await
        .unwrap();

    assert_eq!(
        found,
        PullRequestOnHead::Open(PR_REOPENED.to_owned()),
        "the read answered with a pull request that was merged, which is what \
         settles the create step \"already open\" against a merge and then links \
         the ticket to it"
    );
}

/// A head whose only pull request is merged offers no open one, and names it
/// so the step that refuses can say which.
#[tokio::test]
async fn a_head_whose_only_pull_request_is_merged_offers_no_open_one() {
    let pool = corpus("sw_head_only_merged").await;
    let head = "feature/PAY-231";
    item(
        &pool,
        PR_MERGED,
        "pr",
        "payout dashboard latency",
        serde_json::json!({"merged": true, "state": "closed", "head": {"ref": head}}),
    )
    .await;

    let found = queue::pull_request_on_head(&pool, &declarations(), &repo(), head)
        .await
        .unwrap();

    assert_eq!(
        found,
        PullRequestOnHead::Closed {
            id: PR_MERGED.to_owned(),
            merged: true,
        }
    );
}

/// **The bug one state over, against a real mirror.** A pull request closed
/// *without* merging carries `merged: false`, so a rule built on the declared
/// flag alone would call it open, settle the create step against it and link
/// the ticket to it. `state` is what says otherwise.
#[tokio::test]
async fn a_pull_request_closed_without_merging_is_not_an_open_one() {
    let pool = corpus("sw_head_closed").await;
    let head = "feature/PAY-231";
    item(
        &pool,
        PR_MERGED,
        "pr",
        "payout dashboard latency, abandoned",
        serde_json::json!({"merged": false, "state": "closed", "head": {"ref": head}}),
    )
    .await;

    let found = queue::pull_request_on_head(&pool, &declarations(), &repo(), head)
        .await
        .unwrap();

    assert_eq!(
        found,
        PullRequestOnHead::Closed {
            id: PR_MERGED.to_owned(),
            merged: false,
        },
        "somebody closed this one; `merged: false` does not make it open"
    );
}

/// **The pinned failure direction, end to end** (ADR-0007 requirement 3).
/// Open is a thing knobas has to have read: a record with no readable state is
/// `Unknown`, so the flow dispatches and refuses to link rather than settling
/// against a record it cannot read.
#[tokio::test]
async fn a_pull_request_whose_state_cannot_be_read_is_not_an_open_one() {
    let pool = corpus("sw_head_stateless").await;
    let head = "feature/PAY-231";
    item(
        &pool,
        PR_REOPENED,
        "pr",
        "payout dashboard latency, part two",
        // Every hint short of the fact: not merged, not closed, just no state.
        serde_json::json!({"merged": false, "head": {"ref": head}}),
    )
    .await;

    let found = queue::pull_request_on_head(&pool, &declarations(), &repo(), head)
        .await
        .unwrap();

    assert_eq!(
        found,
        PullRequestOnHead::Unknown,
        "a payload read outside an adapter misses; a declaration saying `not \
         merged` is not a record saying `open`"
    );
}

/// A source that declares no merged flag still resolves through the record's
/// own state, which is why the deciding fact is that one: a source may decline
/// that declaration -- `knobas_source_mock` does, its merge being a timestamp
/// -- and its pull requests still have to be linkable. The empty
/// [`Declarations`] is that source's shape, not that source's corpus; the
/// mock's own pull requests carry no `head` object and never reach this
/// statement.
///
/// [`Declarations`]: knobas_core::payload::Declarations
#[tokio::test]
async fn a_source_that_declares_no_merged_flag_still_reads_the_state() {
    let pool = corpus("sw_head_undeclared").await;
    let head = "feature/PAY-231";
    item(
        &pool,
        PR_REOPENED,
        "pr",
        "payout dashboard latency, part two",
        serde_json::json!({"merged": false, "state": "open", "head": {"ref": head}}),
    )
    .await;

    let found = queue::pull_request_on_head(
        &pool,
        &knobas_core::payload::Declarations::empty(),
        &repo(),
        head,
    )
    .await
    .unwrap();

    assert_eq!(found, PullRequestOnHead::Open(PR_REOPENED.to_owned()));
}

/// **The seam.** With both pull requests on the head, the flow settles the
/// create step against the open one and draws the ticket's link to it -- never
/// to the merge.
#[tokio::test]
async fn a_re_used_head_links_the_open_pull_request_and_not_the_merged_one() {
    let pool = corpus("sw_reused_flow").await;
    let flow = planned(&pool).await;
    let head = proposed_branch(&flow);
    let fake = Fake::new()
        .holding(&head, on_head(PR_MERGED, Some(true), Some("closed")))
        .holding(&head, on_head(PR_REOPENED, Some(false), Some("open")));

    let flow = start_work::run(&pool, &fake, &ticket()).await.unwrap();

    assert_eq!(
        fake.ops(),
        vec!["create_branch", "transition"],
        "the create step sent a second pull request over one already open from \
         this head"
    );
    assert_eq!(
        fake.links(),
        vec![(PR_REOPENED.to_owned(), TICKET.to_owned())],
        "the ticket was linked to the merged pull request"
    );
    assert_eq!(
        step_of(&flow, Step::CreatePullRequest).detail.as_deref(),
        Some(&format!("{PR_REOPENED} is already open")[..])
    );
}

/// **The other half of the ruling.** When the only pull request on the head is
/// merged, the flow **opens a new one** rather than settling "already open"
/// against the merge: a forge does not refuse a pull request from a head whose
/// last one was merged, and the new commits on that head are what wants
/// reviewing. The link then goes to what the refresh brought back.
#[tokio::test]
async fn a_head_whose_only_pull_request_is_merged_gets_a_new_one() {
    let pool = corpus("sw_merged_head_flow").await;
    let flow = planned(&pool).await;
    let head = proposed_branch(&flow);
    let fake = Fake::new()
        .holding(&head, on_head(PR_MERGED, Some(true), Some("closed")))
        .revealing_on_refresh(&head);

    let flow = start_work::run(&pool, &fake, &ticket()).await.unwrap();

    assert_eq!(
        fake.ops(),
        vec!["create_branch", "create_pull_request", "transition"],
        "the merged pull request settled the create step, so no new one was opened"
    );
    assert_eq!(
        fake.links(),
        vec![(PR.to_owned(), TICKET.to_owned())],
        "the link must go to the pull request this flow made"
    );
    assert_eq!(
        outcome(&flow, Step::LinkPullRequest),
        StepOutcome::Succeeded
    );
}

/// And when nothing new reaches the mirror, the link step **refuses with a
/// reason the user can act on**: it names the merged pull request it declined
/// to link to. A wrong link is permanent and is what the reverse direction
/// reads; a failed step is retryable.
#[tokio::test]
async fn a_link_step_refuses_a_merged_pull_request_and_names_it() {
    let pool = corpus("sw_merged_head_no_new").await;
    let flow = planned(&pool).await;
    let head = proposed_branch(&flow);
    let fake = Fake::new().holding(&head, on_head(PR_MERGED, Some(true), Some("closed")));

    let flow = start_work::run(&pool, &fake, &ticket()).await.unwrap();

    assert_eq!(outcome(&flow, Step::LinkPullRequest), StepOutcome::Failed);
    assert!(fake.links().is_empty(), "the ticket was linked to a merge");
    assert_eq!(fake.refreshes(), 1, "it asked for a sync before giving up");
    let detail = step_of(&flow, Step::LinkPullRequest)
        .detail
        .clone()
        .expect("the step has to carry why");
    assert!(
        detail.contains(PR_MERGED) && detail.contains("merged"),
        "the refusal must name what it found: {detail}"
    );
    assert_eq!(outcome(&flow, Step::Transition), StepOutcome::Pending);
}

/// And the **other word**: a pull request somebody closed without merging is
/// refused as *closed*, not as merged. The declared flag is what chooses
/// between the two words, so the branch that reads it as `false` is a
/// user-facing string nothing else here would notice getting wrong -- the
/// classification is the same either way, and only the sentence differs.
#[tokio::test]
async fn a_link_step_refuses_a_closed_pull_request_as_closed_and_not_as_merged() {
    let pool = corpus("sw_closed_head_no_new").await;
    let flow = planned(&pool).await;
    let head = proposed_branch(&flow);
    let fake = Fake::new().holding(&head, on_head(PR_MERGED, Some(false), Some("closed")));

    let flow = start_work::run(&pool, &fake, &ticket()).await.unwrap();

    assert_eq!(outcome(&flow, Step::LinkPullRequest), StepOutcome::Failed);
    assert!(
        fake.links().is_empty(),
        "the ticket was linked to a closed one"
    );
    let detail = step_of(&flow, Step::LinkPullRequest)
        .detail
        .clone()
        .expect("the step has to carry why");
    assert!(
        detail.contains(PR_MERGED) && detail.contains("closed") && !detail.contains("merged"),
        "nothing knobas read says this one was merged, and the refusal must not \
         say so either: {detail}"
    );
}

// -- the reverse direction -------------------------------------------------

/// What the pull request's source declares about where it keeps its merged
/// flag (#277). Gitea's boolean `merged`, which is `knobas_source_gitea`'s own
/// declaration -- the pass reads through it now, so a fixture's source has to
/// have said where its own spelling is.
fn declarations() -> knobas_core::payload::Declarations {
    use knobas_core::payload::{Declarations, KindPaths, PayloadPath};
    Declarations::empty().with(
        "gitea",
        vec![KindPaths {
            kind: "pr".to_owned(),
            merged: vec![PayloadPath::of(["merged"])],
            ..KindPaths::default()
        }],
    )
}

/// **Story 17.** A merged pull request moves the ticket a knobas link joins it
/// to.
#[tokio::test]
async fn a_merged_pull_request_moves_the_ticket_it_is_linked_to() {
    let pool = corpus("sw_merged").await;
    item(
        &pool,
        PR,
        "pr",
        "WIP: payout dashboard latency",
        serde_json::json!({"merged": true, "state": "closed", "head": {"ref": "feature/PAY-231"}}),
    )
    .await;
    knobas_app::commands::entity::create_link_inner(&pool, PR, TICKET, Some("implements"), None)
        .await
        .unwrap();
    let fake = Fake::new();

    let moved = start_work::merge::follow_merges(&pool, &fake, "In Review", &declarations())
        .await
        .unwrap();

    assert_eq!(moved, 1);
    assert_eq!(fake.ops(), vec!["transition"]);
}

/// **Story 18, and the half that matters most.** A merged pull request nobody
/// linked moves nothing: an automatic status change may never reach a ticket
/// the user did not connect.
#[tokio::test]
async fn a_merged_pull_request_nobody_linked_moves_nothing() {
    let pool = corpus("sw_unlinked").await;
    item(
        &pool,
        PR,
        "pr",
        "WIP: payout dashboard latency",
        serde_json::json!({"merged": true, "head": {"ref": "feature/PAY-231"}}),
    )
    .await;
    let fake = Fake::new();

    let moved = start_work::merge::follow_merges(&pool, &fake, "In Review", &declarations())
        .await
        .unwrap();

    assert_eq!(moved, 0);
    assert!(
        fake.ops().is_empty(),
        "knobas touched a ticket the user never connected to that pull request"
    );
}

/// A pull request that is merely *open* moves nothing, however well linked.
#[tokio::test]
async fn an_open_pull_request_moves_nothing() {
    let pool = corpus("sw_open_pr").await;
    item(
        &pool,
        PR,
        "pr",
        "WIP: payout dashboard latency",
        serde_json::json!({"merged": false, "state": "open", "head": {"ref": "feature/PAY-231"}}),
    )
    .await;
    knobas_app::commands::entity::create_link_inner(&pool, PR, TICKET, Some("implements"), None)
        .await
        .unwrap();
    let fake = Fake::new();

    assert_eq!(
        start_work::merge::follow_merges(&pool, &fake, "In Review", &declarations())
            .await
            .unwrap(),
        0
    );
    assert!(fake.ops().is_empty());
}

/// A merge is followed **once**. Without this the pass would re-queue the same
/// transition every time it ran, for as long as the pull request stayed merged
/// -- which is for ever.
///
/// The memory is the write queue's own row, so this needs the real queue rather
/// than the fake: `due` is what reads it.
#[tokio::test]
async fn a_merge_already_followed_is_not_followed_twice() {
    let pool = corpus("sw_once").await;
    item(
        &pool,
        PR,
        "pr",
        "WIP: payout dashboard latency",
        serde_json::json!({"merged": true, "head": {"ref": "feature/PAY-231"}}),
    )
    .await;
    knobas_app::commands::entity::create_link_inner(&pool, PR, TICKET, Some("implements"), None)
        .await
        .unwrap();

    assert_eq!(
        start_work::merge::due(&pool, "In Review", &declarations())
            .await
            .unwrap()
            .len(),
        1,
        "the pass has something to do before the transition is queued"
    );

    // The transition knobas would make, queued -- and then *discarded* by the
    // user, which is the sharpest case: a decision has been made, and re-queuing
    // over it would be knobas arguing with them.
    let queued = knobas_core::write_queue::queue(
        &pool,
        "jira",
        &ticket(),
        "transition",
        start_work::plan::transition(&ticket(), "In Review"),
        serde_json::json!({}),
    )
    .await
    .unwrap();
    knobas_core::write_queue::discard(&pool, queued.id)
        .await
        .unwrap();

    assert!(
        start_work::merge::due(&pool, "In Review", &declarations())
            .await
            .unwrap()
            .is_empty(),
        "the same merge would be followed again on the next pass, for ever"
    );
}

// -- what the round-1 review added ------------------------------------------

/// A step that already happened cannot be skipped: its effect exists at the
/// source, and marking it skipped would rewrite the record of what the flow
/// did -- the record stories 16 and 20 exist for.
#[tokio::test]
async fn skipping_a_step_that_already_happened_is_refused() {
    let pool = corpus("sw_skip_done").await;
    let flow = planned(&pool).await;
    let fake = Fake::new().revealing_on_refresh(&proposed_branch(&flow));

    let flow = start_work::run(&pool, &fake, &ticket()).await.unwrap();
    let branch = step_of(&flow, Step::CreateBranch);
    assert_eq!(branch.outcome, StepOutcome::Succeeded);

    let refusal = start_work::skip(&pool, &fake, branch.id)
        .await
        .expect_err("a succeeded step must not be skippable");
    assert_eq!(refusal.code, IpcErrorCode::Conflict);
}

/// **Story 20.** A skipped step is a decision with no side effect anywhere
/// else, so nothing but the flow itself would remember it -- the activity
/// stream has to.
#[tokio::test]
async fn a_skipped_step_lands_in_the_activity_stream() {
    let pool = corpus("sw_skip_audit").await;
    let flow = planned(&pool).await;
    let fake = Fake::new().revealing_on_refresh(&proposed_branch(&flow));

    start_work::skip(&pool, &fake, step_of(&flow, Step::CreateBranch).id)
        .await
        .unwrap();

    let (entity, step): (Option<String>, Option<String>) = sqlx::query_as(
        "select entity_id, detail->>'step' from knobas.activity where verb = 'skipped'",
    )
    .fetch_one(&pool)
    .await
    .expect("the skip wrote an activity line");
    assert_eq!(entity.as_deref(), Some(TICKET));
    assert_eq!(step.as_deref(), Some("create_branch"));
}

/// **Stories 19 and 20.** The queue's own activity line says a transition was
/// queued; the reverse direction has to say *why* -- the merged pull request
/// -- and may not claim the user asked for it.
#[tokio::test]
async fn the_reverse_direction_names_its_pull_request_in_the_activity_stream() {
    let pool = corpus("sw_merged_audit").await;
    item(
        &pool,
        PR,
        "pr",
        "WIP: payout dashboard latency",
        serde_json::json!({"merged": true, "head": {"ref": "feature/PAY-231"}}),
    )
    .await;
    knobas_app::commands::entity::create_link_inner(&pool, PR, TICKET, Some("implements"), None)
        .await
        .unwrap();

    let moved = start_work::merge::follow_merges(&pool, &Fake::new(), "In Review", &declarations())
        .await
        .unwrap();
    assert_eq!(moved, 1);

    let (actor, entity, pr): (String, Option<String>, Option<String>) = sqlx::query_as(
        "select actor, entity_id, detail->>'pr' from knobas.activity where verb = 'followed'",
    )
    .fetch_one(&pool)
    .await
    .expect("the follow wrote an activity line");
    assert_eq!(entity.as_deref(), Some(TICKET));
    assert_eq!(pr.as_deref(), Some(PR));
    assert_eq!(
        actor, "knobas",
        "nobody asked for this write, and the log may not claim they did"
    );
}
