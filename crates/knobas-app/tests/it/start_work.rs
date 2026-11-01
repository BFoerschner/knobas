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
use knobas_app::sources::write_queue::Named;
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
/// The branch every flow below proposes its pull request into: the
/// repository's default branch, as [`corpus`] mirrors it.
const BASE: &str = "main";
/// A colleague's pull request from the flow's head into another base (#1952).
const PR_BACKPORT: &str = "gitea:tidewater/payout-service#149";

/// What the fake will answer for one op identifier.
#[derive(Clone, Debug)]
enum Answer {
    Landed(Landing),
    /// The queue refused to take the op at all -- an op the source does not
    /// declare, a source that is not configured.
    Rejected(String),
}

/// One pull request the fake's mirror holds: the repository, the head branch,
/// the base branch, and the row `start_work::queue`'s statement would answer
/// for it.
///
/// The row's `merged` and `state` are both optional because the real reads
/// **miss** rather than guess -- an undeclared flag, an absent state -- and a
/// fixture has to be able to put the mirror in that state: it is the direction
/// #359 pinned.
type MirroredPull = (String, String, String, queue::OnHead);

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
    on_refresh: Mutex<Vec<MirroredPull>>,
    /// The pull request a sent `create_pull_request`'s receipt names, if the
    /// source named one (#1949).
    names: Option<String>,
    /// Per write id, what its receipt named.
    receipts: Mutex<std::collections::HashMap<i64, String>>,
    log: Mutex<Recorded>,
    next_write_id: Mutex<i64>,
    /// When set, every dispatch waits for a permit: a send held open (#779).
    gate: Option<std::sync::Arc<tokio::sync::Semaphore>>,
    /// When set, every branch read waits for a permit: a run held while the
    /// step it is about to take is still pending (#779).
    look_gate: Option<std::sync::Arc<tokio::sync::Semaphore>>,
    /// How many branch reads have begun.
    looks: Mutex<usize>,
    /// When set, the next landing read first lets another stepper press this
    /// step's *Check again* and then its *Retry*: a step moved on between a
    /// retry's read of it and its settle (#2015).
    between: Mutex<Option<(PgPool, i64)>>,
}

impl Fake {
    /// Everything succeeds and the mirror is empty.
    fn new() -> Self {
        Self {
            answers: Mutex::new(std::collections::HashMap::new()),
            landings: Mutex::new(std::collections::HashMap::new()),
            branches: Mutex::new(Vec::new()),
            pulls: Mutex::new(Vec::new()),
            on_refresh: Mutex::new(Vec::new()),
            names: None,
            receipts: Mutex::new(std::collections::HashMap::new()),
            log: Mutex::new(Recorded::default()),
            next_write_id: Mutex::new(1),
            gate: None,
            look_gate: None,
            looks: Mutex::new(0),
            between: Mutex::new(None),
        }
    }

    /// Every branch read waits on `gate` for a permit before it answers.
    fn looking_through(mut self, gate: &std::sync::Arc<tokio::sync::Semaphore>) -> Self {
        self.look_gate = Some(std::sync::Arc::clone(gate));
        self
    }

    /// Every dispatch waits on `gate` for a permit before it answers.
    fn held_by(mut self, gate: &std::sync::Arc<tokio::sync::Semaphore>) -> Self {
        self.gate = Some(std::sync::Arc::clone(gate));
        self
    }

    fn answering(self, op: &str, answer: Answer) -> Self {
        self.answers.lock().unwrap().insert(op.to_owned(), answer);
        self
    }

    /// A refresh is what brings this pull request into the mirror -- the real
    /// shape for a source whose receipt names nothing, where the number is
    /// learned by reading back.
    fn revealing_on_refresh(self, head: &str) -> Self {
        self.revealing_on_refresh_too(head, on_head(PR, Some(false), Some("open")))
    }

    /// A refresh also brings this pull request into the mirror.
    fn revealing_on_refresh_too(self, head: &str, row: queue::OnHead) -> Self {
        self.revealing_on_refresh_into(head, BASE, row)
    }

    /// A refresh brings this pull request from `head` into `base` into the
    /// mirror.
    fn revealing_on_refresh_into(self, head: &str, base: &str, row: queue::OnHead) -> Self {
        self.on_refresh.lock().unwrap().push((
            REPO.to_owned(),
            head.to_owned(),
            base.to_owned(),
            row,
        ));
        self
    }

    /// A sent `create_pull_request` answers a receipt naming `pr` (#1949).
    fn naming(mut self, pr: &str) -> Self {
        self.names = Some(pr.to_owned());
        self
    }

    /// Put one pull request from `head` into the flow's base into the mirror
    /// this fake stands for.
    fn holding(self, head: &str, row: queue::OnHead) -> Self {
        self.holding_into(head, BASE, row)
    }

    /// Put one pull request from `head` into `base` into the mirror.
    fn holding_into(self, head: &str, base: &str, row: queue::OnHead) -> Self {
        self.pulls
            .lock()
            .unwrap()
            .push((REPO.to_owned(), head.to_owned(), base.to_owned(), row));
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
        if let Some(gate) = &self.gate {
            gate.acquire().await.expect("the gate stays open").forget();
        }
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
                if let (Landing::Sent, "create_pull_request", Some(named)) =
                    (&landing, op.as_str(), &self.names)
                {
                    self.receipts.lock().unwrap().insert(id, named.clone());
                }
                Ok((id, landing))
            }
        }
    }

    async fn landing_of(&self, write_id: i64) -> Result<Landing, knobas_app::IpcError> {
        let between = self.between.lock().unwrap().take();
        if let Some((pool, step_id)) = between {
            start_work::ready_retry(&pool, self, step_id)
                .await
                .expect("the other stepper's Check again");
            start_work::ready_retry(&pool, self, step_id)
                .await
                .expect("the other stepper's Retry");
        }
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
        *self.looks.lock().unwrap() += 1;
        if let Some(gate) = &self.look_gate {
            gate.acquire().await.expect("the gate stays open").forget();
        }
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
        base: &str,
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
            .filter(|(r, h, b, _)| r == &repo.to_string() && h == head && b == base)
            .map(|(_, _, _, row)| row.clone())
            .collect();
        Ok(queue::pick_open(&rows))
    }

    async fn pull_request_named(&self, write_id: i64) -> Result<Named, knobas_app::IpcError> {
        // The real read's rule: the named one, once the mirror holds it.
        let Some(named) = self.receipts.lock().unwrap().get(&write_id).cloned() else {
            return Ok(Named::Nothing);
        };
        let held = self
            .pulls
            .lock()
            .unwrap()
            .iter()
            .any(|(_, _, _, row)| row.entity_id == named);
        Ok(Named::Made(held.then_some(named)))
    }

    async fn refresh(&self, _source: &str) {
        self.log.lock().unwrap().refreshed += 1;
        let revealed: Vec<MirroredPull> = self.on_refresh.lock().unwrap().drain(..).collect();
        self.pulls.lock().unwrap().extend(revealed);
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
        BASE.to_owned(),
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

/// A transition **sent and never answered** (#1125) may already be at Jira, so
/// the step is on its way, not failed (Björn's #1966 ruling; #1973): it reads
/// *queued*, says to check Jira, and a retry re-reads the write and sends no
/// second transition. Once the user sends it again from the queue, the retry
/// is what moves the step on.
#[tokio::test]
async fn a_transition_never_answered_is_on_its_way_and_its_retry_sends_nothing() {
    let pool = corpus("sw_unconfirmed").await;
    let flow = planned(&pool).await;
    let fake = Fake::new()
        .revealing_on_refresh(&proposed_branch(&flow))
        .answering(
            "transition",
            Answer::Landed(Landing::Unconfirmed {
                source: "Jira".to_owned(),
                detail: None,
            }),
        );

    let flow = start_work::run(&pool, &fake, &ticket()).await.unwrap();

    assert_eq!(outcome(&flow, Step::Transition), StepOutcome::Queued);
    let step = step_of(&flow, Step::Transition);
    let detail = step.detail.clone().unwrap();
    assert!(detail.contains("check Jira"), "{detail}");
    assert!(!detail.contains("changed"), "{detail}");
    let write = step.write_id.expect("the transition was dispatched");

    let flow = start_work::retry(&pool, &fake, step.id).await.unwrap();
    assert_eq!(outcome(&flow, Step::Transition), StepOutcome::Queued);
    assert_eq!(
        fake.ops(),
        vec!["create_branch", "create_pull_request", "transition"],
        "a retry sent the transition a second time over one that may have landed"
    );

    // The user checked Jira and sent it again from the write queue.
    fake.set_landing(write, Landing::Sent);
    let flow = start_work::retry(&pool, &fake, step.id).await.unwrap();
    assert_eq!(outcome(&flow, Step::Transition), StepOutcome::Succeeded);
    assert_eq!(fake.ops().len(), 3, "{:?}", fake.ops());
}

/// A queued step's button is *Check again* (#1998): it re-reads the write. When
/// the user checked Jira, found the transition there and **discarded** the
/// write, the re-read settles the step failed with the discard's sentence and
/// sends nothing (#2015). The failed step's *Retry* is then the retry, and it
/// runs rules 2 and 3 as any failed step's does.
#[tokio::test]
async fn checking_again_on_a_queued_step_whose_write_was_discarded_settles_it_failed_and_sends_nothing()
 {
    let pool = corpus("sw_check_discarded").await;
    let flow = planned(&pool).await;
    let fake = Fake::new()
        .revealing_on_refresh(&proposed_branch(&flow))
        .answering(
            "transition",
            Answer::Landed(Landing::Unconfirmed {
                source: "Jira".to_owned(),
                detail: None,
            }),
        );
    let flow = start_work::run(&pool, &fake, &ticket()).await.unwrap();
    let step = step_of(&flow, Step::Transition).clone();
    assert_eq!(step.outcome, StepOutcome::Queued);

    // The user found it in Jira and discarded the write in the write queue.
    fake.set_landing(step.write_id.expect("dispatched"), Landing::Withdrawn);
    let flow = start_work::retry(&pool, &fake, step.id).await.unwrap();

    assert_eq!(outcome(&flow, Step::Transition), StepOutcome::Failed);
    assert_eq!(
        step_of(&flow, Step::Transition).detail.as_deref(),
        Some("the write was discarded before it went")
    );
    assert_eq!(
        fake.ops(),
        vec!["create_branch", "create_pull_request", "transition"],
        "checking again sent the transition a second time"
    );

    // The step reads failed now, and its *Retry* runs it again (rule 3).
    fake.now_answers("transition", Answer::Landed(Landing::Sent));
    let flow = start_work::retry(&pool, &fake, step.id).await.unwrap();
    assert_eq!(outcome(&flow, Step::Transition), StepOutcome::Succeeded);
    assert_eq!(
        fake.ops(),
        vec![
            "create_branch",
            "create_pull_request",
            "transition",
            "transition"
        ]
    );
}

/// The same for a queued write the source has since **refused**: the re-read
/// settles the step failed with the source's own sentence and dispatches
/// nothing, and the failed step's *Retry* looks before it writes (rule 2).
#[tokio::test]
async fn checking_again_on_a_queued_step_whose_write_was_refused_settles_it_failed_and_sends_nothing()
 {
    let pool = corpus("sw_check_refused").await;
    let flow = planned(&pool).await;
    let head = proposed_branch(&flow);
    let fake = Fake::new().answering(
        "create_branch",
        Answer::Landed(Landing::Waiting {
            detail: Some("the server did not answer".to_owned()),
        }),
    );
    let flow = start_work::run(&pool, &fake, &ticket()).await.unwrap();
    let step = step_of(&flow, Step::CreateBranch).clone();
    assert_eq!(step.outcome, StepOutcome::Queued);

    fake.set_landing(
        step.write_id.expect("dispatched"),
        Landing::Refused {
            detail: "branch already exists".to_owned(),
        },
    );
    let looks = *fake.looks.lock().unwrap();
    let flow = start_work::retry(&pool, &fake, step.id).await.unwrap();

    assert_eq!(outcome(&flow, Step::CreateBranch), StepOutcome::Failed);
    assert_eq!(
        step_of(&flow, Step::CreateBranch).detail.as_deref(),
        Some("branch already exists")
    );
    assert_eq!(
        fake.ops(),
        vec!["create_branch"],
        "checking again re-sent the branch"
    );
    assert_eq!(
        *fake.looks.lock().unwrap(),
        looks,
        "checking again ran the step's read: it is the failed step's Retry that looks"
    );

    // It was there all along. The failed step's *Retry* reads the mirror,
    // finds it, and sends nothing for it.
    fake.branches
        .lock()
        .unwrap()
        .push((REPO.to_owned(), head.clone()));
    fake.pulls.lock().unwrap().push((
        REPO.to_owned(),
        head,
        BASE.to_owned(),
        on_head(PR, Some(false), Some("open")),
    ));
    let flow = start_work::retry(&pool, &fake, step.id).await.unwrap();
    assert_eq!(outcome(&flow, Step::CreateBranch), StepOutcome::Succeeded);
    assert_eq!(fake.ops(), vec!["create_branch", "transition"]);
}

/// A *Check again* that read the step `queued` settles it failed only if it is
/// still `queued`. Here a second stepper pressed *Check again* and then *Retry*
/// while the first one's re-read was out, so the step is `pending` again and
/// the run's to take. Settling it failed would undo that retry, so the first
/// re-read is refused and sends nothing (#2015, #1707).
#[tokio::test]
async fn checking_again_on_a_step_another_stepper_moved_on_is_refused_and_sends_nothing() {
    let pool = corpus("sw_check_moved_on").await;
    let flow = planned(&pool).await;
    let fake = Fake::new()
        .revealing_on_refresh(&proposed_branch(&flow))
        .answering(
            "transition",
            Answer::Landed(Landing::Unconfirmed {
                source: "Jira".to_owned(),
                detail: None,
            }),
        );
    let flow = start_work::run(&pool, &fake, &ticket()).await.unwrap();
    let step = step_of(&flow, Step::Transition).clone();
    assert_eq!(step.outcome, StepOutcome::Queued);
    fake.set_landing(step.write_id.expect("dispatched"), Landing::Withdrawn);

    *fake.between.lock().unwrap() = Some((pool.clone(), step.id));
    let refusal = start_work::ready_retry(&pool, &fake, step.id)
        .await
        .expect_err("the step is no longer the queued one this re-read read");

    assert_eq!(refusal.code, IpcErrorCode::Conflict);
    let flow = start_work::flow(&pool, &ticket()).await.unwrap();
    assert_eq!(
        outcome(&flow, Step::Transition),
        StepOutcome::Pending,
        "the re-read settled over the other stepper's retry"
    );
    assert_eq!(
        fake.ops(),
        vec!["create_branch", "create_pull_request", "transition"]
    );
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
/// The refresh is the interesting part: this fake's receipt names nothing, so
/// the pull request's number is not something knobas is told -- it is read
/// back after a sync, by the head branch the flow opened from.
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

/// The link goes to the pull request the create's receipt named (#1949), not
/// to another one open on the same head: a colleague's from that head into a
/// release branch is open there too, and comes first in the mirror.
#[tokio::test]
async fn the_link_goes_to_the_pull_request_the_create_named_not_another_open_on_its_head() {
    let pool = corpus("sw_named_pr").await;
    let flow = planned(&pool).await;
    let head = proposed_branch(&flow);
    let fake = Fake::new()
        .naming(PR)
        .revealing_on_refresh_too(&head, on_head(PR_REOPENED, Some(false), Some("open")))
        .revealing_on_refresh(&head);

    let flow = start_work::run(&pool, &fake, &ticket()).await.unwrap();

    assert_eq!(
        fake.ops(),
        vec!["create_branch", "create_pull_request", "transition"]
    );
    assert_eq!(
        fake.links(),
        vec![(PR.to_owned(), TICKET.to_owned())],
        "the ticket was linked to a pull request this flow did not open"
    );
    assert_eq!(
        outcome(&flow, Step::LinkPullRequest),
        StepOutcome::Succeeded
    );
}

/// A named pull request the mirror has not seen after the one refresh fails
/// the link step, retryable, and is never stood in for by another one open on
/// its head (#1949).
#[tokio::test]
async fn a_named_pull_request_not_yet_mirrored_is_not_stood_in_for_by_another_on_its_head() {
    let pool = corpus("sw_named_pr_unmirrored").await;
    let flow = planned(&pool).await;
    let head = proposed_branch(&flow);
    let fake = Fake::new()
        .naming(PR)
        .revealing_on_refresh_too(&head, on_head(PR_REOPENED, Some(false), Some("open")));

    let flow = start_work::run(&pool, &fake, &ticket()).await.unwrap();

    assert_eq!(outcome(&flow, Step::LinkPullRequest), StepOutcome::Failed);
    assert!(fake.links().is_empty(), "linked to another pull request");
    assert_eq!(fake.refreshes(), 1, "it asked for a sync before giving up");
    let detail = step_of(&flow, Step::LinkPullRequest)
        .detail
        .clone()
        .expect("the step has to carry why");
    assert!(detail.contains("not reached the mirror"), "{detail}");
    assert_eq!(outcome(&flow, Step::Transition), StepOutcome::Pending);
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

/// The repository dropdown repoints the branch and pull request steps through
/// `start_work_amend` (PAGE-10): the amendment of `entity` is accepted and read
/// back, on both steps.
#[tokio::test]
async fn the_repository_of_a_branch_and_a_pull_request_step_can_be_amended() {
    let pool = corpus("sw_edit_entity").await;
    let flow = planned(&pool).await;
    let other = "gitea:tidewater/other-service";

    for (kind, tag) in [
        (Step::CreateBranch, "CreateBranch"),
        (Step::CreatePullRequest, "CreatePullRequest"),
    ] {
        let step = step_of(&flow, kind);
        let mut edited = step.payload.clone();
        edited[tag]["entity"] = serde_json::json!(other);
        let after = start_work::repropose(&pool, step.id, edited).await.unwrap();
        assert_eq!(step_of(&after, kind).payload[tag]["entity"], other);
    }
}

// -- a step only leaves the outcomes it may leave (#1707) ------------------

/// A queued step's write is still open in the queue. Returning the step to
/// pending would have the next run dispatch a second branch beside the first,
/// so the edit is refused; the write is amended in the write queue instead.
#[tokio::test]
async fn an_edit_of_a_queued_step_is_refused_and_queues_nothing_new() {
    let pool = corpus("sw_edit_queued").await;
    planned(&pool).await;
    let fake = Fake::new().answering(
        "create_branch",
        Answer::Landed(Landing::Waiting {
            detail: Some("the server did not answer".to_owned()),
        }),
    );
    let flow = start_work::run(&pool, &fake, &ticket()).await.unwrap();
    let branch = step_of(&flow, Step::CreateBranch);
    assert_eq!(branch.outcome, StepOutcome::Queued);
    let mut edited = branch.payload.clone();
    edited["CreateBranch"]["name"] = serde_json::json!("knobas-PAY-231");

    let refusal = start_work::repropose(&pool, branch.id, edited)
        .await
        .expect_err("a queued step's write is still open");
    start_work::run(&pool, &fake, &ticket()).await.unwrap();

    assert_eq!(refusal.code, IpcErrorCode::Conflict);
    assert_eq!(
        fake.ops(),
        vec!["create_branch"],
        "the edit queued a second branch beside the open one"
    );
}

/// A stale stepper's *Retry* on a step that has since succeeded changes
/// nothing, whatever its write's row now says: the effect exists, and running
/// the step again is how a second branch gets made.
#[tokio::test]
async fn a_retry_of_a_step_that_already_succeeded_is_refused_and_sends_nothing() {
    let pool = corpus("sw_retry_done").await;
    let flow = planned(&pool).await;
    let fake = Fake::new().revealing_on_refresh(&proposed_branch(&flow));
    let flow = start_work::run(&pool, &fake, &ticket()).await.unwrap();
    let branch = step_of(&flow, Step::CreateBranch);
    assert_eq!(branch.outcome, StepOutcome::Succeeded);
    // The delivered write's row is no longer readable.
    fake.set_landing(
        branch.write_id.expect("the branch was dispatched"),
        Landing::Withdrawn,
    );

    let refusal = start_work::retry(&pool, &fake, branch.id)
        .await
        .expect_err("a succeeded step is not retried");

    assert_eq!(refusal.code, IpcErrorCode::Conflict);
    assert_eq!(
        fake.ops(),
        ["create_branch", "create_pull_request", "transition"],
        "the stale retry ran the branch step a second time"
    );
    let flow = start_work::flow(&pool, &ticket()).await.unwrap();
    assert_eq!(outcome(&flow, Step::CreateBranch), StepOutcome::Succeeded);
}

/// The run's branch read is held while the step is skipped underneath it:
/// the run has read the flow and is looking at the source. The run's move to
/// running may only leave a pending step, so it finds the step taken and does
/// not send it.
#[tokio::test]
async fn a_step_skipped_while_the_run_looks_is_not_sent() {
    let pool = corpus("sw_skip_under_run").await;
    let flow = planned(&pool).await;
    let branch = step_of(&flow, Step::CreateBranch).id;
    let look = std::sync::Arc::new(tokio::sync::Semaphore::new(0));
    let fake = std::sync::Arc::new(
        Fake::new()
            .looking_through(&look)
            .revealing_on_refresh(&proposed_branch(&flow)),
    );

    let run = tokio::spawn({
        let (pool, fake) = (pool.clone(), std::sync::Arc::clone(&fake));
        async move { start_work::run(&pool, &*fake, &ticket()).await }
    });
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    while *fake.looks.lock().unwrap() == 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the run never looked"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    knobas_core::start_work::settle(
        &pool,
        branch,
        &[StepOutcome::Pending],
        StepOutcome::Skipped,
        None,
        Some("skipped"),
    )
    .await
    .unwrap()
    .expect("the step is still pending while the run looks");
    look.add_permits(10);
    let flow = run.await.unwrap().unwrap();

    assert!(
        !fake.ops().contains(&"create_branch".to_owned()),
        "the run sent a step that was skipped underneath it: {:?}",
        fake.ops()
    );
    assert_eq!(outcome(&flow, Step::CreateBranch), StepOutcome::Skipped);
}

/// A send is held open, the step is interrupted underneath it,
/// then the send lands. The landing may not leave a step that is no longer
/// running, so the interruption stands -- but the write it made is kept on the
/// step, and the retry adopts that write rather than sending a second.
#[tokio::test]
async fn a_landing_that_loses_to_an_interrupt_keeps_its_write_for_the_retry() {
    let pool = corpus("sw_land_after_intr").await;
    let flow = planned(&pool).await;
    let branch = step_of(&flow, Step::CreateBranch).id;
    let gate = std::sync::Arc::new(tokio::sync::Semaphore::new(0));
    let fake = std::sync::Arc::new(
        Fake::new()
            .held_by(&gate)
            .revealing_on_refresh(&proposed_branch(&flow)),
    );

    let run = tokio::spawn({
        let (pool, fake) = (pool.clone(), std::sync::Arc::clone(&fake));
        async move { start_work::run(&pool, &*fake, &ticket()).await }
    });
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    while fake.ops().is_empty() {
        assert!(tokio::time::Instant::now() < deadline, "the run never sent");
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        knobas_core::start_work::interrupt(&pool, branch, start_work::INTERRUPTED)
            .await
            .unwrap(),
        "the send is out, so the step is running"
    );
    gate.add_permits(10);
    let flow = run.await.unwrap().unwrap();

    let step = step_of(&flow, Step::CreateBranch);
    assert_eq!(
        step.outcome,
        StepOutcome::Failed,
        "the landing overwrote the interruption"
    );
    assert_eq!(step.detail.as_deref(), Some(start_work::INTERRUPTED));
    assert_eq!(step.write_id, Some(1), "the write the send made was lost");
    assert_eq!(fake.ops(), ["create_branch"]);

    let flow = start_work::retry(&pool, &*fake, branch).await.unwrap();
    assert_eq!(
        fake.ops(),
        ["create_branch", "create_pull_request", "transition"],
        "the retry sent the branch a second time instead of adopting its write"
    );
    assert_eq!(outcome(&flow, Step::CreateBranch), StepOutcome::Succeeded);
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
        serde_json::json!({"merged": true, "state": "closed", "head": {"ref": head}, "base": {"ref": BASE}}),
    )
    .await;
    item(
        &pool,
        PR_REOPENED,
        "pr",
        "payout dashboard latency, part two",
        serde_json::json!({"merged": false, "state": "open", "head": {"ref": head}, "base": {"ref": BASE}}),
    )
    .await;

    let found = queue::pull_request_on_head(&pool, &declarations(), &repo(), head, BASE)
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

/// **A pull request is the pair, head and base (#1952).** A colleague's open
/// pull request from the flow's head into a release branch is not the one the
/// flow would open into `main`: Gitea opens both side by side, and refuses a
/// second one only for the same head *and* base. So the read does not find it,
/// nor a record whose base it cannot read, and the create step goes on to open
/// the flow's own.
#[tokio::test]
async fn an_open_pull_request_from_the_head_into_another_base_is_not_found() {
    let pool = corpus("sw_head_other_base").await;
    let head = "feature/PAY-231";
    item(
        &pool,
        PR_BACKPORT,
        "pr",
        "PAY-231: backport to release/2026.08",
        serde_json::json!({
            "merged": false, "state": "open",
            "head": {"ref": head}, "base": {"ref": "release/2026.08"}
        }),
    )
    .await;
    // ADR-0007's miss: a record that carries no base is not one into `main`.
    item(
        &pool,
        PR_REOPENED,
        "pr",
        "payout dashboard latency, part two",
        serde_json::json!({"merged": false, "state": "open", "head": {"ref": head}}),
    )
    .await;

    let found = queue::pull_request_on_head(&pool, &declarations(), &repo(), head, BASE)
        .await
        .unwrap();
    assert_eq!(
        found,
        PullRequestOnHead::Unknown,
        "a pull request into another base settled the create step \"already open\""
    );

    let found =
        queue::pull_request_on_head(&pool, &declarations(), &repo(), head, "release/2026.08")
            .await
            .unwrap();
    assert_eq!(found, PullRequestOnHead::Open(PR_BACKPORT.to_owned()));
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
        serde_json::json!({"merged": true, "state": "closed", "head": {"ref": head}, "base": {"ref": BASE}}),
    )
    .await;

    let found = queue::pull_request_on_head(&pool, &declarations(), &repo(), head, BASE)
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
        serde_json::json!({"merged": false, "state": "closed", "head": {"ref": head}, "base": {"ref": BASE}}),
    )
    .await;

    let found = queue::pull_request_on_head(&pool, &declarations(), &repo(), head, BASE)
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
        serde_json::json!({"merged": false, "head": {"ref": head}, "base": {"ref": BASE}}),
    )
    .await;

    let found = queue::pull_request_on_head(&pool, &declarations(), &repo(), head, BASE)
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
/// -- and its pull requests still have to be linkable. Gitea's spelling with
/// the merged flag left out is that source's shape, not its corpus.
#[tokio::test]
async fn a_source_that_declares_no_merged_flag_still_reads_the_state() {
    let pool = corpus("sw_head_undeclared").await;
    let head = "feature/PAY-231";
    item(
        &pool,
        PR_REOPENED,
        "pr",
        "payout dashboard latency, part two",
        serde_json::json!({"merged": false, "state": "open", "head": {"ref": head}, "base": {"ref": BASE}}),
    )
    .await;

    let found = queue::pull_request_on_head(&pool, &declared(false), &repo(), head, BASE)
        .await
        .unwrap();

    assert_eq!(found, PullRequestOnHead::Open(PR_REOPENED.to_owned()));
}

/// A source that spells a pull request's two branches and its state its own
/// way is read where it **declares** them (#2358), not at Gitea's
/// `head.ref` / `base.ref`. The declarations are `knobas_source_mock`'s
/// spelling (`from`, `to`, `state` mapping `open | merged`); the corpus is a
/// record in that shape, and nothing about it is Gitea's.
#[tokio::test]
async fn a_source_that_spells_head_and_base_its_own_way_is_read_where_it_declares_them() {
    use knobas_core::payload::{Declarations, KindPaths, PayloadPath, StatePath, StateValue};
    use knobas_core::tone::Tone;
    let pool = corpus("sw_head_declared").await;
    let head = "feature/PAY-231";
    item(
        &pool,
        PR_REOPENED,
        "pr",
        "payout dashboard latency, part two",
        serde_json::json!({"state": "open", "from": head, "to": BASE}),
    )
    .await;
    item(
        &pool,
        PR_MERGED,
        "pr",
        "payout dashboard latency",
        serde_json::json!({"state": "merged", "from": head, "to": "release/2026.08"}),
    )
    .await;
    let declarations = Declarations::empty().with(
        "gitea",
        vec![KindPaths {
            kind: "pr".to_owned(),
            state: vec![StatePath {
                at: PayloadPath::of(["state"]),
                values: vec![
                    StateValue::of("open", Tone::Info),
                    StateValue::of("merged", Tone::Ok),
                ],
            }],
            head: vec![PayloadPath::of(["from"])],
            base: vec![PayloadPath::of(["to"])],
            ..KindPaths::default()
        }],
    );

    let found = queue::pull_request_on_head(&pool, &declarations, &repo(), head, BASE)
        .await
        .unwrap();
    assert_eq!(found, PullRequestOnHead::Open(PR_REOPENED.to_owned()));

    let found =
        queue::pull_request_on_head(&pool, &declarations, &repo(), head, "release/2026.08")
            .await
            .unwrap();
    assert_eq!(
        found,
        PullRequestOnHead::Closed {
            id: PR_MERGED.to_owned(),
            merged: false
        },
        "a state the declaration names and that is not open is finished"
    );
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

/// **The seam, for the pair (#1952).** A colleague's pull request open from
/// the flow's head into a release branch does not settle the create step:
/// the flow opens its own into `main`, which Gitea allows beside it, and the
/// link goes to that one, read back by head and base where the receipt named
/// nothing -- never to the colleague's.
#[tokio::test]
async fn an_open_pull_request_from_the_head_into_another_base_gets_the_flow_its_own() {
    let pool = corpus("sw_other_base_flow").await;
    let flow = planned(&pool).await;
    let head = proposed_branch(&flow);
    let fake = Fake::new()
        .holding_into(
            &head,
            "release/2026.08",
            on_head(PR_BACKPORT, Some(false), Some("open")),
        )
        .revealing_on_refresh(&head);

    let flow = start_work::run(&pool, &fake, &ticket()).await.unwrap();

    assert_eq!(
        fake.ops(),
        vec!["create_branch", "create_pull_request", "transition"],
        "a pull request into another base settled the create step \"already open\""
    );
    assert_eq!(
        fake.links(),
        vec![(PR.to_owned(), TICKET.to_owned())],
        "the ticket was linked to a pull request into another base"
    );
    assert_eq!(
        outcome(&flow, Step::LinkPullRequest),
        StepOutcome::Succeeded
    );
}

/// The base is **the step's**, the one the reader chose in the review step, not
/// the repository's default (#1952): with the flow pointed at a release branch,
/// a pull request open from its head into `main` does not settle the create
/// step, and the one the refresh brings in from the head into the release
/// branch is the one linked.
#[tokio::test]
async fn the_pull_request_is_looked_for_into_the_base_the_reader_chose() {
    let pool = corpus("sw_reproposed_base").await;
    let flow = planned(&pool).await;
    let head = proposed_branch(&flow);
    let step = step_of(&flow, Step::CreatePullRequest);
    let mut payload = step.payload.clone();
    payload["CreatePullRequest"]["base"] = serde_json::json!("release/2026.08");
    start_work::repropose(&pool, step.id, payload)
        .await
        .expect("the proposal is the reader's to change");
    let fake = Fake::new()
        .holding(&head, on_head(PR_REOPENED, Some(false), Some("open")))
        .revealing_on_refresh_into(
            &head,
            "release/2026.08",
            on_head(PR_BACKPORT, Some(false), Some("open")),
        );

    let flow = start_work::run(&pool, &fake, &ticket()).await.unwrap();

    assert_eq!(
        fake.ops(),
        vec!["create_branch", "create_pull_request", "transition"],
        "a pull request into main settled a step the reader pointed at a release branch"
    );
    assert_eq!(
        fake.links(),
        vec![(PR_BACKPORT.to_owned(), TICKET.to_owned())],
        "the ticket was linked to a pull request into a base the reader did not choose"
    );
    assert_eq!(
        outcome(&flow, Step::LinkPullRequest),
        StepOutcome::Succeeded
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

/// What the pull request's source declares (#277, #2358): Gitea's own
/// spelling, as `knobas_source_gitea` declares it -- the boolean `merged`,
/// the state that flag leads and the `open | closed` word behind it, and the
/// two branches at `head.ref` and `base.ref`. The reads follow these
/// declarations, so a fixture's source has to have said where its own
/// spelling is.
fn declarations() -> knobas_core::payload::Declarations {
    declared(true)
}

/// [`declarations`], with or without the merged flag.
fn declared(merged: bool) -> knobas_core::payload::Declarations {
    use knobas_core::payload::{Declarations, KindPaths, PayloadPath, StatePath, StateValue};
    use knobas_core::tone::Tone;
    let mut state = vec![StatePath {
        at: PayloadPath::of(["state"]),
        values: vec![
            StateValue::of("open", Tone::Info),
            StateValue::of("closed", Tone::Muted),
        ],
    }];
    if merged {
        state.insert(
            0,
            StatePath {
                at: PayloadPath::of(["merged"]),
                values: vec![StateValue::of(true, Tone::Ok).shown_as("merged")],
            },
        );
    }
    Declarations::empty().with(
        "gitea",
        vec![KindPaths {
            kind: "pr".to_owned(),
            merged: if merged {
                vec![PayloadPath::of(["merged"])]
            } else {
                Vec::new()
            },
            state,
            head: vec![PayloadPath::of(["head", "ref"])],
            base: vec![PayloadPath::of(["base", "ref"])],
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

/// A merge's transition sent and never answered may have landed: it counts as
/// followed, as a queued one does (#1966, #1973), and is never sent twice --
/// the queued row is `due`'s memory either way.
#[tokio::test]
async fn a_merge_transition_never_answered_counts_as_followed() {
    let pool = corpus("sw_merged_unconfirmed").await;
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
    let fake = Fake::new().answering(
        "transition",
        Answer::Landed(Landing::Unconfirmed {
            source: "Jira".to_owned(),
            detail: None,
        }),
    );

    let moved = start_work::merge::follow_merges(&pool, &fake, "In Review", &declarations())
        .await
        .unwrap();

    assert_eq!(moved, 1);
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

// -- behind the answer (#779) ----------------------------------------------

/// The flow as it stands, read until `done` says so or ten seconds pass.
async fn read_until(pool: &PgPool, done: impl Fn(&[FlowStep]) -> bool) -> Vec<FlowStep> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let flow = start_work::flow(pool, &ticket())
            .await
            .expect("the flow reads");
        if done(&flow) {
            return flow;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the flow never got there: {:?}",
            flow.iter().map(|s| (s.step, s.outcome)).collect::<Vec<_>>()
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

/// **#779: *Run* answers before the first send has, and the steps land behind
/// it in their order.** The fake's dispatch is held open on a semaphore, so an
/// answer that waited for the send would not come back at all. A second *Run*
/// while the first is still out starts nothing: each op is dispatched once.
#[tokio::test(flavor = "multi_thread")]
async fn run_answers_before_the_first_send_and_the_steps_land_behind_it() {
    let pool = corpus("start_work_behind").await;
    let background = knobas_sync::background::Background::new();
    let flow = planned(&pool).await;
    let branch = proposed_branch(&flow);
    let gate = std::sync::Arc::new(tokio::sync::Semaphore::new(0));
    let fake = std::sync::Arc::new(Fake::new().revealing_on_refresh(&branch).held_by(&gate));

    let answered = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        start_work::run_in_background(&pool, std::sync::Arc::clone(&fake), &ticket(), &background),
    )
    .await
    .expect("the run answers while its first send is held open")
    .expect("the run starts");
    assert!(
        answered
            .iter()
            .all(|step| step.outcome != StepOutcome::Succeeded),
        "nothing has landed yet"
    );

    // The branch's send is out: the step reads as running, and a second ask
    // while it is out starts no second loop.
    read_until(&pool, |f| {
        outcome(f, Step::CreateBranch) == StepOutcome::Running
    })
    .await;
    tokio::time::timeout(
        std::time::Duration::from_secs(20),
        start_work::run_in_background(&pool, std::sync::Arc::clone(&fake), &ticket(), &background),
    )
    .await
    .expect("a second ask answers, and does not wait on the first")
    .expect("a second ask reads the flow");

    gate.add_permits(10);
    let flow = read_until(&pool, |f| {
        f.iter().all(|step| step.outcome == StepOutcome::Succeeded)
    })
    .await;
    assert_eq!(outcome(&flow, Step::Transition), StepOutcome::Succeeded);
    assert_eq!(
        fake.ops(),
        ["create_branch", "create_pull_request", "transition"],
        "each op once, in the flow's order"
    );
    assert_eq!(fake.links(), [(PR.to_owned(), TICKET.to_owned())]);
}

/// A step the background run stops at stops it, as an awaited run would: a
/// refused branch never produces a pull request, and the flow ends stopped
/// on the branch step for the reader to retry.
#[tokio::test(flavor = "multi_thread")]
async fn a_refusal_behind_the_answer_stops_the_flow_where_it_happened() {
    let pool = corpus("start_work_behind_refused").await;
    let background = knobas_sync::background::Background::new();
    planned(&pool).await;
    let fake = std::sync::Arc::new(Fake::new().answering(
        "create_branch",
        Answer::Landed(Landing::Refused {
            detail: "branch name taken".to_owned(),
        }),
    ));
    start_work::run_in_background(&pool, std::sync::Arc::clone(&fake), &ticket(), &background)
        .await
        .expect("the run starts");
    let flow = read_until(&pool, |f| {
        outcome(f, Step::CreateBranch) == StepOutcome::Failed
    })
    .await;
    assert_eq!(
        step_of(&flow, Step::CreateBranch).detail.as_deref(),
        Some("branch name taken")
    );
    assert_eq!(fake.ops(), ["create_branch"]);
    // Retried at once, asked once: if the stopped run still holds the claim,
    // it finds the pending step on its way out and takes it; if not, this
    // ask starts a run. Either way the flow moves on.
    fake.now_answers("create_branch", Answer::Landed(Landing::Sent));
    let id = step_of(&flow, Step::CreateBranch).id;
    let ticket = start_work::ready_retry(&pool, fake.as_ref(), id)
        .await
        .expect("a retry");
    start_work::run_in_background(&pool, std::sync::Arc::clone(&fake), &ticket, &background)
        .await
        .expect("the retry runs");
    read_until(&pool, |f| {
        outcome(f, Step::CreatePullRequest) != StepOutcome::Pending
    })
    .await;
    assert_eq!(&fake.ops()[..2], ["create_branch", "create_branch"]);
}

/// **One run per ticket behind the answer.** The first run is held while the
/// branch step it is about to take is still pending -- the one moment a second
/// loop over the same flow would be handed the same step. A second *Run* then
/// must start nothing: the branch is dispatched once, not twice.
#[tokio::test(flavor = "multi_thread")]
async fn a_second_run_while_one_is_behind_starts_no_second_loop() {
    let pool = corpus("start_work_behind_once").await;
    let background = knobas_sync::background::Background::new();
    let flow = planned(&pool).await;
    let branch = proposed_branch(&flow);
    let look = std::sync::Arc::new(tokio::sync::Semaphore::new(0));
    let fake = std::sync::Arc::new(
        Fake::new()
            .revealing_on_refresh(&branch)
            .looking_through(&look),
    );

    start_work::run_in_background(&pool, std::sync::Arc::clone(&fake), &ticket(), &background)
        .await
        .expect("the run starts");
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    while *fake.looks.lock().unwrap() == 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the run never looked"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let held = start_work::flow(&pool, &ticket())
        .await
        .expect("the flow reads");
    assert_eq!(
        outcome(&held, Step::CreateBranch),
        StepOutcome::Pending,
        "held before it ran"
    );

    tokio::time::timeout(
        std::time::Duration::from_secs(20),
        start_work::run_in_background(&pool, std::sync::Arc::clone(&fake), &ticket(), &background),
    )
    .await
    .expect("a second ask answers at once")
    .expect("a second ask reads the flow");
    // The second ask found the flow's claim held and spawned nothing: the set
    // holds the first run alone (#1927). A loop is in the set from the ask
    // that spawns it, so one started now would be counted here, however late
    // it reached the look. Before, a 200 ms wait for it to reach the look
    // stood for this.
    assert_eq!(
        background.running(),
        1,
        "a second ask started a second loop"
    );
    look.add_permits(10);
    read_until(&pool, |f| {
        f.iter().all(|step| step.outcome == StepOutcome::Succeeded)
    })
    .await;
    // And nothing is left running to dispatch anything after the look below.
    background
        .close(tokio::time::Instant::now() + std::time::Duration::from_secs(20))
        .await;
    assert_eq!(
        fake.ops(),
        ["create_branch", "create_pull_request", "transition"],
        "the branch once: one loop took the flow"
    );
}

/// **A step a run behind the answer may be taking is not the reader's to skip
/// or edit (#779).** The run is held at the branch read, before it marks the
/// step running, so the step still reads as pending and the stepper would
/// offer *Skip* and *Edit* on it. Both are refused while the run is behind;
/// the branch goes once, with the proposal the run read, and the flow
/// finishes.
#[tokio::test(flavor = "multi_thread")]
async fn a_step_a_run_behind_may_be_taking_cannot_be_skipped_or_edited() {
    let pool = corpus("start_work_behind_guarded").await;
    let background = knobas_sync::background::Background::new();
    let flow = planned(&pool).await;
    let branch = proposed_branch(&flow);
    let branch_step = step_of(&flow, Step::CreateBranch).clone();
    let look = std::sync::Arc::new(tokio::sync::Semaphore::new(0));
    let fake = std::sync::Arc::new(
        Fake::new()
            .revealing_on_refresh(&branch)
            .looking_through(&look),
    );

    start_work::run_in_background(&pool, std::sync::Arc::clone(&fake), &ticket(), &background)
        .await
        .expect("the run starts");
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    while *fake.looks.lock().unwrap() == 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the run never looked"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    let skipped = start_work::ready_skip(&pool, branch_step.id)
        .await
        .expect_err("a skip of the step the run is taking is refused");
    assert_eq!(skipped.code, IpcErrorCode::Conflict);
    let edited = start_work::repropose(&pool, branch_step.id, branch_step.payload.clone())
        .await
        .expect_err("an edit of the step the run is taking is refused");
    assert_eq!(edited.code, IpcErrorCode::Conflict);

    look.add_permits(10);
    let flow = read_until(&pool, |f| {
        f.iter().all(|step| step.outcome == StepOutcome::Succeeded)
    })
    .await;
    assert_eq!(outcome(&flow, Step::CreateBranch), StepOutcome::Succeeded);
    assert_eq!(
        fake.ops(),
        ["create_branch", "create_pull_request", "transition"]
    );
}

/// **#790.** A quit while a step was being sent leaves it `running`. The
/// start-up pass reads it as interrupted -- failed, with the reason -- and
/// *Retry* and *Skip* both work on it.
#[tokio::test]
async fn a_step_left_running_by_a_quit_reads_as_interrupted_and_can_be_retried_or_skipped() {
    for skip in [false, true] {
        let pool = corpus(if skip {
            "sw_intr_skip"
        } else {
            "sw_intr_retry"
        })
        .await;
        let flow = planned(&pool).await;
        let branch = step_of(&flow, Step::CreateBranch).id;
        knobas_core::start_work::settle(
            &pool,
            branch,
            &[StepOutcome::Pending],
            StepOutcome::Running,
            None,
            None,
        )
        .await
        .unwrap();

        let count = start_work::interrupt_stranded(&pool).await.unwrap();
        assert_eq!(count, 1);
        let flow = start_work::flow(&pool, &ticket()).await.unwrap();
        let step = step_of(&flow, Step::CreateBranch);
        assert_eq!(step.outcome, StepOutcome::Failed);
        assert_eq!(step.detail.as_deref(), Some(start_work::INTERRUPTED));

        let fake = Fake::new();
        let flow = if skip {
            start_work::skip(&pool, &fake, branch).await.unwrap()
        } else {
            start_work::retry(&pool, &fake, branch).await.unwrap()
        };
        let expected = if skip {
            StepOutcome::Skipped
        } else {
            StepOutcome::Succeeded
        };
        assert_eq!(outcome(&flow, Step::CreateBranch), expected);
    }
}

/// A step that settled, or one with a live run behind it, is left alone.
#[tokio::test]
async fn the_start_up_pass_leaves_settled_steps_alone() {
    let pool = corpus("sw_intr_settled").await;
    let flow = planned(&pool).await;
    let branch = step_of(&flow, Step::CreateBranch).id;
    knobas_core::start_work::settle(
        &pool,
        branch,
        &[StepOutcome::Pending],
        StepOutcome::Succeeded,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(start_work::interrupt_stranded(&pool).await.unwrap(), 0);
    let flow = start_work::flow(&pool, &ticket()).await.unwrap();
    assert_eq!(outcome(&flow, Step::CreateBranch), StepOutcome::Succeeded);
}

/// The interrupted step's write may have reached the source: a retry looks
/// first, and a branch that is already there is adopted, not created twice.
#[tokio::test]
async fn a_retry_of_an_interrupted_step_whose_branch_exists_adopts_it() {
    let pool = corpus("sw_intr_adopt").await;
    let flow = planned(&pool).await;
    let head = proposed_branch(&flow);
    let branch = step_of(&flow, Step::CreateBranch).id;
    knobas_core::start_work::settle(
        &pool,
        branch,
        &[StepOutcome::Pending],
        StepOutcome::Running,
        None,
        None,
    )
    .await
    .unwrap();
    start_work::interrupt_stranded(&pool).await.unwrap();

    let fake = Fake::new();
    fake.branches
        .lock()
        .unwrap()
        .push((REPO.to_owned(), head.clone()));
    fake.pulls.lock().unwrap().push((
        REPO.to_owned(),
        head,
        BASE.to_owned(),
        on_head(PR, Some(false), Some("open")),
    ));
    let flow = start_work::retry(&pool, &fake, branch).await.unwrap();

    assert_eq!(
        fake.ops(),
        vec!["transition"],
        "the retry created the branch or the pull request a second time"
    );
    assert_eq!(outcome(&flow, Step::CreateBranch), StepOutcome::Succeeded);
}
