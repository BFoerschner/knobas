//! Logging a day's work, from the draft to the local copy that names it
//! (issue #280).
//!
//! `tests/time_ipc.rs` has the draft's reads -- which blocks a day is made of,
//! what the candidates are. This is the half that needs a source on the other
//! end of the write queue, and the source here is a **trait-level fake**: it
//! records the op it was handed and answers a receipt, exactly as
//! `tests/inbox_ipc.rs`'s does.
//!
//! # Why a fake and not a Jira
//!
//! ADR-0013: *the real instance is the witness, a mock certifies nothing* --
//! and `knobas-mockd` is frozen and deprecated, so nothing new goes into it.
//! The claim "a worklog reaches Jira, with these seconds and this comment, and
//! comes back in the next sync's payload" is `tests/atlassian_live.rs`'s,
//! against the real product.
//!
//! What is left over is **knobas' own plumbing around that write**, which is
//! four crates of it and none of it Jira's: that the copy exists before
//! anything is sent, that the blocks it covers are spent, that the id the
//! source answers with lands on the copy, and that it lands however the flush
//! and the copy interleave. None of that is a claim about a source, so a
//! source that merely answers is the honest fixture for it -- the trait-level
//! fake ADR-0013 explicitly keeps.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use knobas_app::sources::SourcesState;
use knobas_app::time::{self, TimerTarget};
use knobas_core::write_queue::WriteState;
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::{
    AuthMethod, ConnectionInfo, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError,
    WriteOp, WriteReceipt,
};
use knobas_sync::scheduler::{AdapterRegistry, RunConnections, SchedulerDeps, SyncEvents};
use serde_json::json;
use sqlx::{PgPool, Row};

/// The instance id, which is also the namespace of everything it holds.
const JIRA: &str = "jira";

/// The ticket the time goes on -- the fixture's own story, and the one the
/// live suite logs against too.
const TICKET: &str = "jira:PAY-231";

/// The id the fake source answers with, in the shape Jira answers in: a
/// decimal string, continuing the fixture's own `30000 +` run.
const WORKLOG_ID: &str = "30007";

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

// -- a source that records what it was asked to write ------------------------

/// What the fake has been handed, and how it answers.
#[derive(Default)]
struct Wrote {
    ops: Mutex<Vec<WriteOp>>,
    /// When set, `write` fails with this status instead of accepting -- the
    /// "the source is not taking writes" fixture.
    fault: Mutex<Option<u16>>,
}

/// A `Source` whose only job is to say what it received, and to answer the way
/// a source that has just made something answers.
struct Recording {
    state: Arc<Wrote>,
}

#[async_trait]
impl Source for Recording {
    fn descriptor(&self) -> SourceDescriptor {
        descriptor()
    }

    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        Ok(ConnectionInfo::default())
    }

    async fn sync(
        &self,
        cursor: Option<Cursor>,
        _sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        Ok(cursor.unwrap_or_default())
    }

    async fn write(&self, op: WriteOp) -> Result<WriteReceipt, SourceError> {
        if let Some(status) = *self.state.fault.lock().unwrap() {
            return Err(SourceError::Unauthorized {
                status: Some(status),
            });
        }
        self.state.ops.lock().unwrap().push(op);
        // The one receipt in knobas: a worklog is not a mirrored entity, so
        // the id the source answers with is the only way the copy can name it.
        Ok(WriteReceipt::id(WORKLOG_ID))
    }

    /// No workflow: this fake declares no `transition` write op, which is the
    /// contract battery's rule for when the read is refused (#498).
    async fn reachable_transitions(
        &self,
        entity: &str,
    ) -> Result<Vec<String>, knobas_source::SourceError> {
        Err(knobas_source::SourceError::protocol(format!(
            "this fake has no workflow, so there are no reachable transitions for {entity:?}"
        )))
    }
}

/// A Jira-shaped descriptor. The `log_work` declaration is the load-bearing
/// field: it is what the draft asks about before offering itself.
fn descriptor() -> SourceDescriptor {
    SourceDescriptor {
        id: JIRA.to_owned(),
        adapter_kind: JIRA.to_owned(),
        name: "Jira".to_owned(),
        capabilities: vec![knobas_source::Capability::Write],
        adapter_version: "0".to_owned(),
        auth_methods: vec![AuthMethod::Pat],
        accepts_account: false,
        write_ops: vec!["comment".to_owned(), "log_work".to_owned()],
        entity_kinds: vec![KindInfo {
            id: "ticket".to_owned(),
            label: "Ticket".to_owned(),
            plural: "Tickets".to_owned(),
            monogram: "JI".to_owned(),
            full_sync_exhaustive: true,
        }],
        config_schema: json!({"type": "object", "properties": {}}),
        // Nothing declared: no read here goes through a payload path (#277).
        payload_paths: Vec::new(),
    }
}

struct Registry {
    state: Arc<Wrote>,
}

impl AdapterRegistry for Registry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        vec![descriptor()]
    }

    fn build(
        &self,
        _instance: knobas_source::instance::SourceInstance,
    ) -> Result<Box<dyn Source>, SourceError> {
        Ok(Box::new(Recording {
            state: Arc::clone(&self.state),
        }))
    }
}

// -- the app, wired the way the app wires it ---------------------------------

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

async fn app(name: &str) -> (SourcesState, Arc<Wrote>) {
    let connector = knobas_db::test_util::scratch_database(name).await;
    let pool = connector
        .pool(4)
        .await
        .expect("a pool onto the scratch database");

    let secrets = Arc::new(MemoryStore::new());
    secrets
        .put(
            &knobas_secrets::KeychainAccount::source(JIRA),
            &Secret::just(AuthMethod::Pat, "a-token"),
        )
        .expect("the credential is stored");

    knobas_sync::config::insert(
        &pool,
        &knobas_sync::config::InsertConfig {
            id: JIRA.to_owned(),
            adapter_kind: JIRA.to_owned(),
            display_name: "Tidewater Jira".to_owned(),
            base_url: "https://jira.example".to_owned(),
            auth_kind: knobas_sync::config::AuthKind::Method(AuthMethod::Pat),
            config: json!({ "username": "mara.lindqvist" }),
            sync_interval_secs: 86_400,
            enabled: true,
        },
    )
    .await
    .expect("the source row is written");

    let wrote = Arc::new(Wrote::default());
    let registry: Arc<dyn AdapterRegistry> = Arc::new(Registry {
        state: Arc::clone(&wrote),
    });
    let scheduler = knobas_sync::scheduler::Scheduler::start(SchedulerDeps {
        pool: pool.clone(),
        connections: Arc::new(Connections(connector)),
        registry: Arc::clone(&registry),
        secrets: secrets.clone(),
        events: Arc::new(Quiet),
        timing: knobas_sync::scheduler::SchedulerTiming::default(),
    })
    .await
    .expect("a scheduler over the scratch database");

    (
        SourcesState {
            pool,
            scheduler,
            secrets,
            registry,
        },
        wrote,
    )
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
/// fixtures here call a day.
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
/// 1. the source was asked to log **the interval the reader settled on**, with
///    those seconds and that comment -- read off the op it was handed;
/// 2. the write went through the **queue** and settled `sent`, so a worklog
///    behaves like every other write-back and shows up in the same panel;
/// 3. there is a **local copy at once**, carrying the queue row's id;
/// 4. the copy carries the **id the source answered with**. This is the one
///    that cannot be recovered later: the id exists only in that answer, and a
///    settle that dropped it would leave a copy that can never name the row it
///    stands for;
/// 5. the covered blocks are **read-only** -- they carry the worklog's id, and
///    the next draft for the same day offers nothing, so the same afternoon
///    cannot be logged twice.
#[tokio::test(flavor = "multi_thread")]
async fn a_days_blocks_become_one_worklog_and_a_copy_that_names_it() {
    let (state, wrote) = app("worklog_ipc_logs").await;
    let morning = block(&state.pool, (9, 0), (10, 30)).await;
    let afternoon = block(&state.pool, (13, 0), (14, 0)).await;

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

    // 1. What the source was asked to do.
    {
        let ops = wrote.ops.lock().unwrap();
        let [
            WriteOp::LogWork {
                entity,
                started,
                seconds,
                comment: sent,
            },
        ] = ops.as_slice()
        else {
            panic!("the source was handed {ops:?}, not one worklog");
        };
        assert_eq!(entity, TICKET);
        assert_eq!(
            *started,
            at(9, 0),
            "the work began when the first block did"
        );
        assert_eq!(*seconds, 150 * 60, "and the gap between them is not logged");
        assert_eq!(sent, comment);
    }

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

    // 3 and 4. A copy at once, and the source's id on it.
    assert_eq!(logged.entity_id, TICKET);
    assert_eq!(logged.seconds, 150 * 60);
    assert_eq!(logged.comment, comment);
    assert_eq!(logged.block_ids, vec![morning, afternoon]);
    assert_eq!(
        logged.remote_id.as_deref(),
        Some(WORKLOG_ID),
        "the settle is what puts the source's worklog id on the copy, and it is \
         the only moment that id exists"
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

    state.scheduler.shutdown().await;
}

/// A worklog the source never took still leaves a copy and still spends its
/// blocks -- and carries **no** remote id.
///
/// The fault is a source that refuses the credential, which is the ordinary
/// way this goes wrong: the write waits in the queue, and the day is not
/// offered for logging a second time while it does. A copy written only on
/// success would mean the reader logged their afternoon, saw it fail, and
/// found the blocks back on the pile with nothing to say a worklog was already
/// on its way -- which is how a duplicate gets sent by hand.
///
/// `remote_id` staying `null` is the other half: it is what tells this copy
/// apart from one the source has taken, and a settle path that stamped
/// something anyway would make every refused worklog look landed.
#[tokio::test(flavor = "multi_thread")]
async fn a_worklog_the_source_never_took_still_has_a_copy_and_still_spends_its_blocks() {
    let (state, wrote) = app("worklog_ipc_waits").await;
    let morning = block(&state.pool, (9, 0), (10, 0)).await;
    *wrote.fault.lock().unwrap() = Some(401);

    let logged = time::worklog::log(&state, TICKET, day(), UTC, at(9, 0), 3_600, "")
        .await
        .expect("logging a day does not fail because the source is down");

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
        "a credential the source refuses is something a person fixes, so the \
         write waits rather than dying: {:?}",
        row.detail
    );
    assert_eq!(
        logged.remote_id, None,
        "nothing answered, so there is no id to carry"
    );
    assert_eq!(
        pointers(&state.pool, &[morning]).await,
        vec![Some(logged.id)],
        "the block is spent whether or not the source has taken the worklog yet"
    );
    assert!(
        draft(&state).await.is_none(),
        "an afternoon already on its way must not be offered again"
    );

    state.scheduler.shutdown().await;
}

/// **A settle that ran before the copy existed does not lose the id.**
///
/// `log` queues, writes the copy, then flushes, so ordinarily the copy is
/// there when the settle stamps it. That is not the only order that can
/// happen: the scheduler flushes every source on its own tick, and a tick
/// landing in the gap between the queue row and the copy settles the write
/// while nothing names it. The stamp then matches no row, and the id -- which
/// exists only in the answer to that one call -- would be gone for good.
///
/// So the settle records the id **on the queue row** as well, and the copy
/// adopts it from there. Two writers, one value, no ordering required.
///
/// The interleaving is reproduced rather than raced for: the write is queued
/// and sent with no `knobas.worklog` row in existence at all, which is the
/// worst case of that tick, and the copy is written afterwards exactly as
/// `log` writes it.
#[tokio::test(flavor = "multi_thread")]
async fn a_settle_that_beat_the_copy_still_gives_it_the_id() {
    let (state, _wrote) = app("worklog_ipc_race").await;
    let morning = block(&state.pool, (9, 0), (10, 0)).await;

    let payload = serde_json::to_value(WriteOp::LogWork {
        entity: TICKET.to_owned(),
        started: at(9, 0),
        seconds: 3_600,
        comment: String::new(),
    })
    .expect("the op serialises");
    let queued = knobas_app::sources::write_queue::submit(&state, payload)
        .await
        .expect("the write is queued");
    let settled = knobas_core::write_queue::get(&state.pool, queued.id)
        .await
        .expect("the queue row is readable")
        .expect("the row just queued");
    assert_eq!(
        settled.state,
        WriteState::Sent,
        "this fixture needs the write to have settled before any copy exists: {:?}",
        settled.detail
    );

    // ...and now the copy, naming a write that has already been answered.
    let adopted = time::worklog::keep(
        &state.pool,
        TICKET,
        at(9, 0),
        3_600,
        "",
        &[morning],
        queued.id,
    )
    .await
    .expect("the copy is written");

    assert_eq!(
        adopted.remote_id.as_deref(),
        Some(WORKLOG_ID),
        "the settle had already run, so the copy takes the id off the queue row \
         it names -- the alternative is a worklog the source holds and knobas \
         can never name"
    );
    assert_eq!(
        pointers(&state.pool, &[morning]).await,
        vec![Some(adopted.id)]
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
    let (state, _wrote) = app("worklog_ipc_stop").await;

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
        None,
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
