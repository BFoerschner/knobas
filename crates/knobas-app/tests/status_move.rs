//! Moving a ticket to another status, end to end (#179).
//!
//! The detail's status select is optimistic by design: knobas has no read of
//! which transitions a ticket's workflow offers from where it stands (that seam
//! is M3's descriptor growth, ADR-0007), so the select offers what the source's
//! corpus has been *seen* to use and the **adapter** resolves the target at
//! write time. Which means the interesting behaviour is not in the shell at
//! all — it is what happens when a move the workflow does not allow reaches a
//! real Jira. That is what this file is about, and both directions are here:
//!
//! * a legal move lands at the source, and a sync brings the new status back
//!   into the mirror, which is what makes the board show it (story 17);
//! * an illegal one is **refused by name, with the workflow's own offered
//!   list**, and the refusal sits in the queue where every other write problem
//!   is answered.
//!
//! # Why this needs no Docker
//!
//! `tests/start_work_live.rs` is `#[ignore]`d because it needs a seeded Gitea
//! in a container. This flow touches Jira and nothing else, and `knobas-mockd`
//! serves Jira in-process on a random port — the treatment
//! `tests/adapter_to_mirror.rs` gets, and for the same reason: it runs in
//! `just check`, so the two green halves of "the shell queues it" and "the
//! source refuses it" cannot drift apart unnoticed.
//!
//! **mockd's workflow is a workflow, not a list of statuses**
//! (`MockState::jira_transitions`): from *To Do* the only move is to *In
//! Progress*, and *Done* is not reachable in one step. An adapter that assumed
//! any status was reachable would pass against a mock that offered everything
//! and fail against a real Jira, which is the whole reason the fake has a shape.

use std::sync::Arc;

use async_trait::async_trait;
use knobas_app::sources::{Registry, SourcesState};
use knobas_core::write_queue::WriteState;
use knobas_mockd::spawn_mock_jira;
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::AuthMethod;
use knobas_sync::SyncTrigger;
use knobas_sync::scheduler::{RunConnections, Scheduler, SchedulerDeps, SyncEvents};
use serde_json::json;

/// The instance id every fixture below is synced under.
const JIRA: &str = "jira";

/// The workflow-less source the read is refused by (#498).
const MOCK: &str = "mock";

/// The issue this file moves.
///
/// `PAY-240`, and the choice is load-bearing in both directions: the fixture
/// puts it in **To Do**, whose only transition is to *In Progress* — so one
/// status is legal and every other one in the corpus is not, which is what
/// makes a single fixture prove both halves.
const KEY: &str = "PAY-240";

/// Where the fixture starts it, and the one move its workflow allows.
const FROM: &str = "To Do";
const LEGAL: &str = "In Progress";

/// A status the corpus shows — so the select would offer it — that this
/// ticket's workflow will not accept from where it stands.
const ILLEGAL: &str = "Done";

/// A status the workflow **does** offer from where the ticket stands once the
/// legal move has landed (`MockState::jira_transitions`, from *In Progress*).
///
/// It is its own constant because it is what makes the refusal assertion below
/// mean anything. The message's wording — "this workflow offers …" — is a
/// format literal, so it survives an adapter that dropped the list entirely;
/// only a status the list itself supplies can witness that the list is there.
/// `LEGAL` cannot do that job: *In Progress* is where the ticket now **is**,
/// and this workflow does not offer a move from a status to itself.
const OFFERED_FROM_LEGAL: &str = "In Review";

// -- the app, wired the way the app wires it ---------------------------------

/// Connections a run gets: the scratch database's own, not the shared one's.
struct Connections(knobas_db::embedded::Connector);

#[async_trait]
impl RunConnections for Connections {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        self.0.connect().await
    }
}

/// Events nobody is listening for. The scheduler reports; there is no window.
struct Quiet;

impl SyncEvents for Quiet {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
}

/// A `SourcesState` over a database of this test's own, with mockd's Jira
/// configured and its credential in place.
async fn app(jira_url: &str) -> SourcesState {
    let connector = knobas_db::test_util::scratch_database("status_move").await;
    let pool = connector
        .pool(4)
        .await
        .expect("a pool onto the scratch database");

    let secrets = Arc::new(MemoryStore::new());
    secrets
        .put(
            JIRA,
            &Secret::just(AuthMethod::Pat, knobas_mockd::JIRA_TOKEN),
        )
        .expect("the Jira token is stored");

    knobas_sync::config::insert(
        &pool,
        &knobas_sync::config::InsertConfig {
            id: JIRA.to_owned(),
            adapter_kind: "jira".to_owned(),
            display_name: "Tidewater Jira".to_owned(),
            base_url: jira_url.to_owned(),
            auth_kind: knobas_sync::config::AuthKind::Method(AuthMethod::Pat),
            config: json!({}),
            sync_interval_secs: 86_400,
            enabled: true,
        },
    )
    .await
    .expect("the source row is written");

    // A second source with **no workflow**, for the read's other half (#498).
    // The mock, because it is the one adapter compiled in that needs no remote
    // system and no credential -- so the refusal this file asserts is the
    // adapter's own answer over the real command path, not a fixture standing
    // in for one.
    knobas_sync::config::insert(
        &pool,
        &knobas_sync::config::InsertConfig {
            id: MOCK.to_owned(),
            adapter_kind: "mock".to_owned(),
            display_name: "Tidewater (mock)".to_owned(),
            base_url: String::new(),
            auth_kind: knobas_sync::config::AuthKind::None,
            config: json!({}),
            sync_interval_secs: 86_400,
            enabled: true,
        },
    )
    .await
    .expect("the mock source row is written");

    let scheduler = Scheduler::start(SchedulerDeps {
        pool: pool.clone(),
        connections: Arc::new(Connections(connector)),
        registry: Arc::new(Registry::builtin()),
        secrets: secrets.clone(),
        events: Arc::new(Quiet),
        timing: knobas_sync::scheduler::SchedulerTiming::default(),
    })
    .await
    .expect("a scheduler over the scratch database");

    SourcesState {
        pool,
        scheduler,
        secrets,
        registry: Arc::new(Registry::builtin()),
    }
}

/// Sync Jira and wait for the run to end.
///
/// Through a progress sink rather than a sleep, the way `start_work_live` does
/// it: a run that has not finished is a mirror that has not moved, and a test
/// that guessed at how long that takes would be flaky in exactly the direction
/// that gets it deleted.
async fn sync(state: &SourcesState) {
    let (done, wait) = tokio::sync::oneshot::channel();
    let sink = Arc::new(Ending {
        done: std::sync::Mutex::new(Some(done)),
    });
    state
        .scheduler
        .trigger(JIRA, SyncTrigger::Manual, Some(sink))
        .await
        .expect("the run starts");
    wait.await.expect("the run reports its ending");
}

struct Ending {
    done: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

impl knobas_sync::progress::ProgressSink for Ending {
    fn report(&self, progress: knobas_sync::progress::SyncProgress) {
        use knobas_sync::progress::SyncPhase;
        if !matches!(progress.phase, SyncPhase::Finished | SyncPhase::Failed) {
            return;
        }
        if let Some(sender) = self
            .done
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            let _ = sender.send(());
        }
    }
}

/// The status the **mirror** holds for `KEY`, read the way the mini board's
/// command reads it — through the granted read (#177), so this asserts the
/// thing the board would actually draw rather than a payload path of its own.
async fn mirrored_status(state: &SourcesState) -> Option<String> {
    // The declarations the running binary's adapters make, resolved against
    // the configured rows -- which is exactly what the `mini_board` command
    // does (#277). So the status this reads back is the one the real Jira
    // adapter says where to find, not a path this file spells.
    let declarations =
        knobas_app::sources::paths::declared_paths(&state.pool, state.registry.as_ref())
            .await
            .expect("what the configured sources declare");
    let board =
        knobas_core::mini_board::read(&state.pool, None, &[JIRA.to_owned()], None, &declarations)
            .await
            .expect("the mini board reads");
    let id = format!("{JIRA}:{KEY}");
    board
        .columns
        .into_iter()
        .find(|column| column.cards.iter().any(|card| card.entity_id == id))
        .and_then(|column| column.status)
}

/// Queue a move the way the detail's select does — through the real command
/// path, so a payload the shell could not actually send would fail here.
///
/// **This attempts the write, it does not merely enqueue it.**
/// `knobas_sync::write_queue::submit` flushes the source before it returns
/// ("try it now: a failure here is the queue working, not the call failing"),
/// so what comes back below is already settled. An explicit `flush_source`
/// after this would be a line no mutation could kill — which is how it was
/// found.
async fn queue_move(state: &SourcesState, status: &str) -> i64 {
    let queued = knobas_app::sources::write_queue::submit(
        state,
        json!({ "Transition": { "entity": format!("{JIRA}:{KEY}"), "status": status } }),
    )
    .await
    .expect("the move is queued");
    queued.id
}

/// The write with this id, as the pending/held UI would list it.
async fn in_queue(state: &SourcesState, id: i64) -> knobas_core::write_queue::QueuedWrite {
    knobas_core::write_queue::get(&state.pool, id)
        .await
        .expect("the queue reads")
        .unwrap_or_else(|| panic!("write {id} is in the queue"))
}

// -- the round trip ----------------------------------------------------------

/// **Both directions, over one fixture.**
///
/// One test rather than two because the second half depends on the first: the
/// refusal is only meaningful while the ticket is somewhere the illegal move is
/// genuinely illegal *from*, and re-establishing that state for a second test
/// would mean either a second mockd or an assertion about ordering. The two
/// halves are separated by a comment and assert independently.
#[tokio::test]
async fn a_legal_move_lands_and_an_illegal_one_is_refused_by_name() {
    let jira = spawn_mock_jira().await;
    let state = app(&jira.base_url()).await;

    sync(&state).await;
    assert_eq!(
        mirrored_status(&state).await.as_deref(),
        Some(FROM),
        "the fixture has to start where the workflow's one legal move begins, \
         or neither half below proves anything"
    );

    // 1. The legal move: queued, sent, and *mirrored* -- the criterion is the
    //    round trip, not the dispatch. The board shows the new column because
    //    the source says so, never because knobas queued something.
    let legal = queue_move(&state, LEGAL).await;
    let sent = in_queue(&state, legal).await;
    assert_eq!(sent.state, WriteState::Sent, "the move was delivered");

    sync(&state).await;
    assert_eq!(
        mirrored_status(&state).await.as_deref(),
        Some(LEGAL),
        "a landed move reaches the board through the mirror"
    );

    // 2. The illegal move. `Done` is a status this corpus shows -- so the
    //    select offers it -- and one this ticket cannot reach from `In
    //    Progress` in a single transition. The adapter is what discovers that,
    //    at write time, against the source's own answer.
    let illegal = queue_move(&state, ILLEGAL).await;
    let refused = in_queue(&state, illegal).await;
    assert_eq!(
        refused.state,
        WriteState::Refused,
        "a workflow's refusal is a decision, not a blip: it must not be retried"
    );
    assert!(
        refused.state.is_open(),
        "and it stays in the list the pending/held UI draws, where it can be \
         answered -- a refusal nobody can see is a write that vanished"
    );

    let said = refused.detail.clone().unwrap_or_default();
    assert!(
        said.contains(ILLEGAL),
        "the refusal names the move that was asked for: {said}"
    );
    assert!(
        said.contains(OFFERED_FROM_LEGAL),
        "and it carries the workflow's own offered list, which is what the \
         reader learns their workflow from — and which the word \"offers\" on \
         its own does not witness: {said}"
    );

    // The mirror is untouched by a refused write, which is the other half of
    // "the board reflects the mirror only".
    sync(&state).await;
    assert_eq!(
        mirrored_status(&state).await.as_deref(),
        Some(LEGAL),
        "a refused move changes nothing at the source and nothing on the board"
    );

    jira.assert_no_violations();
}

// -- the read the select is offered (#498) -----------------------------------

/// Ask the way the detail asks: through `crud::reachable_transitions`, which is
/// the whole of what the `reachable_transitions` command does once its state is
/// resolved.
async fn reachable(state: &SourcesState, entity: &str) -> Result<Vec<String>, String> {
    knobas_app::sources::crud::reachable_transitions(
        &state.pool,
        &state.secrets,
        state.registry.as_ref(),
        entity,
    )
    .await
    .map_err(|error| error.to_string())
}

/// **The read is the workflow's, and the workflow has a shape.**
///
/// `MockState::jira_transitions` is not a list of statuses: from *To Do* the
/// only move is to *In Progress*, and from *In Progress* there are two, neither
/// of them *Done*. So an adapter that answered "every status this corpus shows"
/// -- which is exactly what the select offered before this read existed -- fails
/// the first assertion here, and one that answered the transitions' own *names*
/// rather than the statuses they land on fails it too: mockd's names are *Start
/// Progress*, *Send to Review*, *Stop Progress*, and no status is called that.
///
/// Both states are read over **one** fixture and the second after a real move,
/// because the point of the read is that the answer depends on where the ticket
/// stands: two assertions against one state could both pass on an adapter that
/// ignored the ticket entirely.
#[tokio::test]
async fn the_read_offers_the_workflow_and_a_source_without_one_refuses_by_name() {
    let jira = spawn_mock_jira().await;
    let state = app(&jira.base_url()).await;
    sync(&state).await;

    let target = format!("{JIRA}:{KEY}");
    assert_eq!(
        mirrored_status(&state).await.as_deref(),
        Some(FROM),
        "the fixture has to start where the workflow's one legal move begins"
    );
    assert_eq!(
        reachable(&state, &target).await.expect("the read answers"),
        vec![LEGAL.to_owned()],
        "from {FROM:?} this workflow reaches exactly one status, and the read says which"
    );

    // Move it, and ask again: the answer follows the ticket.
    let legal = queue_move(&state, LEGAL).await;
    assert_eq!(in_queue(&state, legal).await.state, WriteState::Sent);
    let now = reachable(&state, &target).await.expect("the read answers");
    assert!(
        now.contains(&OFFERED_FROM_LEGAL.to_owned()) && now.contains(&FROM.to_owned()),
        "from {LEGAL:?} the workflow offers {OFFERED_FROM_LEGAL:?} and a way back to {FROM:?}: {now:?}"
    );
    assert!(
        !now.contains(&ILLEGAL.to_owned()),
        "{ILLEGAL:?} is a status this corpus shows and this workflow does not reach from \
         {LEGAL:?} -- offering it is the guess this read exists to stop: {now:?}"
    );

    // The other half: a source with no workflow refuses, and names itself. Not
    // an empty list -- an empty answer is a workflow with nowhere left to go,
    // and the shell draws those two differently.
    let refused = reachable(&state, &format!("{MOCK}:PAY-231"))
        .await
        .expect_err("a source with no workflow refuses the read");
    assert!(
        refused.to_lowercase().contains("mock"),
        "the refusal names the adapter that has no workflow: {refused}"
    );

    // And an entity whose namespace is no configured source is `not_found`
    // rather than a refusal from some adapter that happened to be built.
    let unknown = reachable(&state, "nowhere:PAY-231")
        .await
        .expect_err("an unconfigured source has nothing to ask");
    assert!(unknown.contains("nowhere"), "{unknown}");

    jira.assert_no_violations();
}
