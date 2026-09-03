//! Logging a day's work to Jira, end to end, without a container (issue #280).
//!
//! `tests/time_ipc.rs` has the draft's reads -- which blocks a day is made of,
//! what the candidates are. This is the half that needs a Jira on the other
//! end of the write queue, and mockd is one: in-process, on a random port, so
//! it runs inside `just check` and the four claims below cannot quietly stop
//! being true between licence windows.
//!
//! ADR-0013 is why this is not the certification: **the real instance is the
//! witness, and a mock certifies nothing.** `tests/atlassian_live.rs` is where
//! a worklog is asserted against a real Jira's own answer. What is asserted
//! here is knobas' own plumbing around that write -- the local copy, the block
//! pointer, the id arriving on the copy at settle -- which is knobas' code in
//! four crates and would otherwise be witnessed only inside a licence window.
//!
//! The one thing mockd *does* certify about the wire is the `started` format,
//! because it parses that field with Jira's own pattern and refuses every
//! other spelling (`knobas_mockd::jira::add_worklog`).

use std::sync::Arc;

use async_trait::async_trait;
use knobas_app::sources::{Registry, SourcesState};
use knobas_app::time::{self, TimerTarget};
use knobas_core::write_queue::WriteState;
use knobas_mockd::spawn_mock_jira;
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::AuthMethod;
use serde_json::json;
use sqlx::{PgPool, Row};

/// The instance id, which is also the namespace of everything it mirrors.
const JIRA: &str = "jira";

/// The ticket the time goes on -- the fixture's own story, and the one the
/// live suite logs against too.
const KEY: &str = "PAY-231";
const TICKET: &str = "jira:PAY-231";

/// The day every fixture below is on, and the reader's offset from UTC.
const DAY: &str = "2026-09-03";
const UTC: i32 = 0;

fn day() -> chrono::NaiveDate {
    DAY.parse().expect("a date")
}

fn at(hour: u32, minute: u32) -> chrono::DateTime<chrono::Utc> {
    use chrono::TimeZone;
    chrono::Utc
        .with_ymd_and_hms(2026, 9, 3, hour, minute, 0)
        .unwrap()
}

// -- the app, wired the way the app wires it ---------------------------------

struct Connections(knobas_db::embedded::Connector);

#[async_trait]
impl knobas_sync::scheduler::RunConnections for Connections {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        self.0.connect().await
    }
}

/// Events nobody is listening for. The scheduler reports; there is no window.
struct Quiet;

impl knobas_sync::scheduler::SyncEvents for Quiet {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
}

async fn app(name: &str, jira_url: &str) -> SourcesState {
    let connector = knobas_db::test_util::scratch_database(name).await;
    let pool = connector
        .pool(4)
        .await
        .expect("a pool onto the scratch database");

    let secrets = Arc::new(MemoryStore::new());
    secrets
        .put(
            JIRA,
            &Secret {
                kind: AuthMethod::Pat,
                value: knobas_mockd::JIRA_TOKEN.to_owned(),
            },
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
            config: json!({ "username": "mara.lindqvist" }),
            sync_interval_secs: 86_400,
            enabled: true,
        },
    )
    .await
    .expect("the source row is written");

    let scheduler =
        knobas_sync::scheduler::Scheduler::start(knobas_sync::scheduler::SchedulerDeps {
            pool: pool.clone(),
            connections: Arc::new(Connections(connector)),
            registry: Arc::new(Registry::builtin()),
            secrets: secrets.clone(),
            events: Arc::new(Quiet),
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

/// Sync Jira and wait for the run to end -- the mirror has to hold the ticket
/// before a write against it can be queued without being held.
async fn sync(state: &SourcesState) {
    let (done, wait) = tokio::sync::oneshot::channel();
    let sink = Arc::new(Ending {
        done: std::sync::Mutex::new(Some(done)),
    });
    state
        .scheduler
        .trigger(JIRA, knobas_sync::SyncTrigger::Manual, Some(sink))
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

/// One block on the ticket.
async fn block(pool: &PgPool, from: (u32, u32), to: (u32, u32)) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "insert into knobas.block (started_at, ended_at, entity_id, kind)
         values ($1, $2, $3, 'manual') returning id",
    )
    .bind(at(from.0, from.1))
    .bind(at(to.0, to.1))
    .bind(TICKET)
    .fetch_one(pool)
    .await
    .expect("a block is written")
}

/// The `worklog_id` each of `ids` carries -- `None` while a block is still
/// free to be logged.
async fn pointers(pool: &PgPool, ids: &[i64]) -> Vec<Option<i64>> {
    sqlx::query("select id, worklog_id from knobas.block where id = any($1) order by id")
        .bind(ids)
        .fetch_all(pool)
        .await
        .expect("the blocks are readable")
        .iter()
        .map(|row| row.try_get("worklog_id").expect("the column"))
        .collect()
}

async fn draft(state: &SourcesState) -> Option<time::worklog::Draft> {
    draft_on(state, day()).await
}

/// The draft for a day this test names -- which the timer test has to do,
/// because a timer stopped now closes its block on **today**, whatever the
/// fixtures below call a day.
async fn draft_on(state: &SourcesState, day: chrono::NaiveDate) -> Option<time::worklog::Draft> {
    time::worklog::draft(&state.pool, state.registry.as_ref(), TICKET, day, UTC)
        .await
        .expect("the draft is readable")
}

// -- the criteria ------------------------------------------------------------

/// **The whole of logging a day**, in the order a person does it: the timer
/// stops, the draft opens, *Log 2h 30m to PAY-231*, and the write goes.
///
/// Five claims, and each one is a different way this could report success
/// while being wrong:
///
/// 1. the worklog is **at Jira**, with the seconds and the comment the reader
///    settled on -- read out of the server's own state, not out of the queue;
/// 2. the write went through the **queue**, and settled `sent`, so a worklog
///    behaves like every other write-back and shows up in the same panel;
/// 3. there is a **local copy at once**, carrying the queue row's id;
/// 4. the copy carries **Jira's worklog id** once Jira has answered. This is
///    the one that cannot be recovered later: the id exists only in the answer
///    to the POST, and a settle that dropped it would leave a copy that can
///    never name the row it stands for;
/// 5. the covered blocks are **read-only** -- they carry the worklog's id, and
///    the next draft for the same day offers nothing, so the same afternoon
///    cannot be logged twice.
#[tokio::test(flavor = "multi_thread")]
async fn a_days_blocks_become_one_worklog_at_jira_and_a_copy_that_names_it() {
    let jira = spawn_mock_jira().await;
    let state = app("worklog_ipc_logs", &jira.base_url()).await;
    sync(&state).await;

    let morning = block(&state.pool, (9, 0), (10, 30)).await;
    let afternoon = block(&state.pool, (13, 0), (14, 0)).await;
    let before = jira.state().issue(KEY).expect("in the fixture");

    let opened = draft(&state).await.expect("there is time to log");
    assert_eq!(opened.seconds, 150 * 60, "two and a half hours were worked");
    assert_eq!(opened.block_ids, vec![morning, afternoon]);

    // What the reader settled on: the draft's interval, and a comment they
    // edited down to one line.
    let comment = "- Retry SEPA payouts";
    let logged = time::worklog::log(
        &state,
        TICKET,
        day(),
        UTC,
        opened.started_at,
        opened.seconds,
        comment,
    )
    .await
    .expect("the day is logged");

    // 1. At Jira.
    let after = jira.state().issue(KEY).expect("in the fixture");
    assert_eq!(
        after.worklogs.len(),
        before.worklogs.len() + 1,
        "the worklog is on the ticket at Jira"
    );
    let at_jira = after.worklogs.last().expect("the worklog just added");
    assert_eq!(at_jira.time_spent_seconds, 150 * 60);
    assert_eq!(at_jira.comment, comment);
    assert_eq!(at_jira.started, at(9, 0));

    // 2. Through the queue, settled.
    let write_id = logged
        .write_queue_id
        .expect("a logged worklog names the write that carries it");
    let row = knobas_core::write_queue::get(&state.pool, write_id)
        .await
        .expect("the queue row is readable")
        .expect("the row `log` queued");
    assert_eq!(row.state, WriteState::Sent, "{:?}", row.detail);
    assert_eq!(row.op, "log_work");
    assert_eq!(row.entity_id, TICKET);

    // 3 and 4. A copy at once, and Jira's id on it.
    assert_eq!(logged.entity_id, TICKET);
    assert_eq!(logged.seconds, 150 * 60);
    assert_eq!(logged.comment, comment);
    assert_eq!(logged.block_ids, vec![morning, afternoon]);
    assert_eq!(
        logged.remote_id.as_deref(),
        Some(at_jira.id.to_string().as_str()),
        "the settle is what puts Jira's worklog id on the copy, and it is the \
         only moment that id exists"
    );

    // 5. The blocks are spoken for, and the day has nothing left to offer.
    assert_eq!(
        pointers(&state.pool, &[morning, afternoon]).await,
        vec![Some(logged.id), Some(logged.id)],
        "every block the worklog covers points back at it"
    );
    assert!(
        draft(&state).await.is_none(),
        "the same afternoon must not come back up for logging"
    );

    jira.assert_no_violations();
    state.scheduler.shutdown().await;
}

/// A worklog Jira never took still leaves a copy and still spends its blocks
/// -- and carries **no** remote id.
///
/// The fault is a Jira that does not answer, which is the ordinary way this
/// goes wrong: the write waits in the queue, and the day is not offered for
/// logging a second time while it does. A copy written only on success would
/// mean the reader logged their afternoon, saw it fail, and found the blocks
/// back on the pile with nothing to say a worklog was already on its way --
/// which is how a duplicate gets sent by hand.
///
/// `remote_id` being `null` is the other half: it is what tells the copy apart
/// from one Jira has taken, and a settle path that stamped something anyway
/// would make every refused worklog look landed.
#[tokio::test(flavor = "multi_thread")]
async fn a_worklog_jira_never_took_still_has_a_copy_and_still_spends_its_blocks() {
    let jira = spawn_mock_jira().await;
    let state = app("worklog_ipc_waits", &jira.base_url()).await;
    sync(&state).await;
    let morning = block(&state.pool, (9, 0), (10, 0)).await;

    // From here Jira answers nothing at all.
    jira.state()
        .set_fault(knobas_mockd::MockFault::Unauthorized);

    let logged = time::worklog::log(&state, TICKET, day(), UTC, at(9, 0), 3_600, "")
        .await
        .expect("logging a day does not fail because Jira is down");

    let row = knobas_core::write_queue::get(
        &state.pool,
        logged.write_queue_id.expect("the write it queued"),
    )
    .await
    .expect("the queue row is readable")
    .expect("the row `log` queued");
    assert_eq!(
        row.state,
        WriteState::Pending,
        "a credential Jira refuses is something a person fixes, so the write \
         waits rather than dying: {:?}",
        row.detail
    );
    assert_eq!(
        logged.remote_id, None,
        "nothing answered, so there is no id to carry"
    );
    assert_eq!(
        pointers(&state.pool, &[morning]).await,
        vec![Some(logged.id)],
        "the block is spent whether or not Jira has taken the worklog yet"
    );
    assert!(
        draft(&state).await.is_none(),
        "an afternoon already on its way to Jira must not be offered again"
    );

    state.scheduler.shutdown().await;
}

/// Stopping the timer on a ticket is what the draft opens on, and the block
/// the stop closed is the one it offers.
///
/// The seam the shell actually uses: `stop_timer` answers a `Block`, and the
/// shell asks for a draft on its target. Without this the two halves are
/// tested apart -- a stop that wrote a block, and a draft that read one
/// somebody inserted.
#[tokio::test(flavor = "multi_thread")]
async fn stopping_the_timer_on_a_ticket_is_what_there_is_to_draft() {
    let jira = spawn_mock_jira().await;
    let state = app("worklog_ipc_stop", &jira.base_url()).await;
    sync(&state).await;

    let today = chrono::Utc::now().date_naive();
    assert!(
        draft_on(&state, today).await.is_none(),
        "nothing has been worked on yet"
    );
    time::start(
        &state.pool,
        TimerTarget::Entity {
            entity_id: TICKET.to_owned(),
        },
    )
    .await
    .expect("the timer starts");
    let stopped = time::stop(&state.pool)
        .await
        .expect("the timer stops")
        .expect("something was running");
    assert_eq!(
        stopped.block.worklog_id, None,
        "a block is unlogged the moment it is closed"
    );

    let opened = draft_on(&state, today)
        .await
        .expect("a stop on a ticket is something to draft");
    assert_eq!(opened.block_ids, vec![stopped.block.id]);
    assert_eq!(opened.entity_id, TICKET);

    state.scheduler.shutdown().await;
}
