//! The week timesheet's numbers, and *Log all* (issue #283).
//!
//! `tests/time_ipc.rs` has the timer and the day's blocks; `tests/worklog_ipc.rs`
//! has one day becoming one worklog. This is the week: the four numbers a cell
//! carries, and the bulk write that turns a week's unlogged blocks into one
//! worklog per day and ticket.
//!
//! The source on the other end is the same **trait-level fake** the worklog
//! tests use, and for the reason recorded there: ADR-0013 says the real
//! instance is the witness for what a *source* does, and nothing here is a
//! claim about Jira. What is claimed is knobas' own arithmetic over its own
//! tables -- which day a stretch is on, what counts as logged, what *Log all*
//! is allowed to touch -- and a source that merely answers is the honest
//! fixture for that.
//!
//! # The week the fixtures live in
//!
//! Monday 24 August 2026 to Sunday 30 August, in UTC, so a day window is a
//! plain midnight-to-midnight interval and the reader's-day arithmetic the
//! webview does (`app/src/lib/time/week.ts`) is not silently under test here
//! as well.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use knobas_app::sources::SourcesState;
use knobas_app::time::{self, week::DayWindow};
use knobas_core::write_queue::WriteState;
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::{
    AuthMethod, ConnectionInfo, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError,
    WriteOp, WriteReceipt,
};
use knobas_sync::scheduler::{AdapterRegistry, RunConnections, SchedulerDeps, SyncEvents};
use serde_json::json;
use sqlx::PgPool;

const JIRA: &str = "jira";
const TICKET: &str = "jira:PAY-231";
/// A second ticket, so "one worklog per day **and ticket**" has two tickets to
/// be wrong about.
const OTHER: &str = "jira:PAY-99";
const WORKLOG_ID: &str = "30011";

/// Monday of the fixture week.
const MONDAY: u32 = 24;

fn at(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, day, hour, minute, 0).unwrap()
}

fn date(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 8, day).unwrap()
}

/// The seven windows the webview would send for the fixture week.
fn week() -> Vec<DayWindow> {
    (0..7)
        .map(|offset| {
            let day = MONDAY + offset;
            DayWindow {
                day: date(day),
                from: at(day, 0, 0),
                to: at(day + 1, 0, 0),
            }
        })
        .collect()
}

/// The column a date sits in, so an assertion names a weekday rather than an
/// index nobody can check against a calendar.
fn column(day: u32) -> usize {
    (day - MONDAY) as usize
}

// -- the fake source, as `worklog_ipc.rs` builds it --------------------------

#[derive(Default)]
struct Wrote {
    ops: Mutex<Vec<WriteOp>>,
    fault: Mutex<Option<u16>>,
}

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
        Ok(WriteReceipt::id(WORKLOG_ID))
    }
}

fn descriptor() -> SourceDescriptor {
    SourceDescriptor {
        id: JIRA.to_owned(),
        adapter_kind: JIRA.to_owned(),
        name: "Jira".to_owned(),
        capabilities: vec![knobas_source::Capability::Write],
        adapter_version: "0".to_owned(),
        auth_methods: vec![AuthMethod::Pat],
        write_ops: vec!["comment".to_owned(), "log_work".to_owned()],
        entity_kinds: vec![KindInfo {
            id: "ticket".to_owned(),
            label: "Ticket".to_owned(),
            plural: "Tickets".to_owned(),
            monogram: "JI".to_owned(),
            full_sync_exhaustive: true,
        }],
        config_schema: json!({"type": "object", "properties": {}}),
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

struct Connections(knobas_db::embedded::Connector);

#[async_trait]
impl RunConnections for Connections {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        self.0.connect().await
    }
}

struct Quiet;

impl SyncEvents for Quiet {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
}

async fn app(name: &str) -> (SourcesState, Arc<Wrote>) {
    let connector = knobas_db::test_util::scratch_database(name).await;
    let pool = connector.pool(4).await.expect("a pool");

    let secrets = Arc::new(MemoryStore::new());
    secrets
        .put(
            JIRA,
            &Secret {
                kind: AuthMethod::Pat,
                value: "a-token".to_owned(),
            },
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
    .expect("a scheduler");

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

// -- fixtures ----------------------------------------------------------------

/// One block on an entity, of the given kind.
async fn block(
    pool: &PgPool,
    day: u32,
    from: (u32, u32),
    to: (u32, u32),
    on: &str,
    kind: &str,
) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "insert into knobas.block (started_at, ended_at, entity_id, kind)
         values ($1, $2, $3, $4) returning id",
    )
    .bind(at(day, from.0, from.1))
    .bind(at(day, to.0, to.1))
    .bind(on)
    .bind(kind)
    .fetch_one(pool)
    .await
    .expect("a block is written")
}

/// One ad-hoc label block -- work with no entity behind it.
async fn label_block(
    pool: &PgPool,
    day: u32,
    from: (u32, u32),
    to: (u32, u32),
    label: &str,
) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "insert into knobas.block (started_at, ended_at, label, kind)
         values ($1, $2, $3, 'manual') returning id",
    )
    .bind(at(day, from.0, from.1))
    .bind(at(day, to.0, to.1))
    .bind(label)
    .fetch_one(pool)
    .await
    .expect("a label block is written")
}

/// Beats every thirty seconds, focused, with nothing in the foreground -- what
/// the shell sends from a room with no detail open. Written through the
/// heartbeat's own insert (`time::passive::record_at`, #387), not fixture SQL.
async fn beats(pool: &PgPool, day: u32, hour: u32, minute: u32, count: i64) {
    for step in 0..count {
        let at = at(day, hour, minute) + chrono::Duration::seconds(step * 30);
        time::passive::record_at(pool, at, None)
            .await
            .expect("a beat is recorded");
    }
}

async fn worklog_id_of(pool: &PgPool, block: i64) -> Option<i64> {
    sqlx::query_scalar("select worklog_id from knobas.block where id = $1")
        .bind(block)
        .fetch_one(pool)
        .await
        .expect("the block is readable")
}

/// Hold the write a worklog names.
///
/// The state is set directly because it **is** the read's input: what the week
/// asserts is that a held write makes a held cell, and the two ways a write
/// comes to be held -- a target that moved on, a source the reader switched
/// off -- are `knobas_core::write_queue`'s own tests. `wait_reason` is cleared
/// with it, because `write_queue_reason_state_chk` allows a reason only on a
/// pending row: a held write is not waiting on a network, it is waiting on a
/// person.
async fn hold(pool: &PgPool, write_queue_id: i64) {
    sqlx::query("update knobas.write_queue set state = 'held', wait_reason = null where id = $1")
        .bind(write_queue_id)
        .execute(pool)
        .await
        .expect("the queue row is writable");
}

/// The row for one target, or the no-target row when `target` is `None`.
fn row_for<'a>(
    week: &'a time::week::Week,
    target: Option<&str>,
) -> Option<&'a time::week::WeekRow> {
    week.rows.iter().find(|row| match (&row.target, target) {
        (Some(time::TimerTarget::Entity { entity_id }), Some(wanted)) => entity_id == wanted,
        (None, None) => true,
        _ => false,
    })
}

// -- the criteria ------------------------------------------------------------

/// **The four numbers of a cell, each witnessed by a day that has only it.**
///
/// Five days, five different states a stretch of time can be in, and every one
/// of them is a number a reasonable implementation gets wrong in a different
/// way:
///
/// * **Monday** -- logged and settled. The plain case.
/// * **Tuesday** -- a worklog whose write is **held**. Spec story 39 says a
///   held worklog shows as held rather than as logged, and this is the
///   assertion a "logged counts anything with a worklog" reading fails.
/// * **Wednesday** -- a worklog whose write is **pending** because the source
///   refused the credential. The same story says this one *is* logged: the
///   number is about what the reader did, not about sync timing. Tuesday and
///   Wednesday together are what pin the rule from both sides -- either one
///   alone is satisfied by a wrong implementation.
/// * **Thursday** -- tracked and not logged at all, so *unlogged* is a
///   difference rather than a constant zero.
/// * **Friday** -- a **passive** block. Tracked time is manual time (the day
///   review says so in words on the same screen), so this cell is zero on
///   every column, and a week that counted knobas' own guess as tracked would
///   report an afternoon nobody vouched for.
#[tokio::test(flavor = "multi_thread")]
async fn a_cell_counts_pending_as_logged_held_as_held_and_a_guess_as_neither() {
    let (state, wrote) = app("week_ipc_cells").await;

    // Monday: logged, and the write settles.
    block(&state.pool, MONDAY, (9, 0), (10, 0), TICKET, "manual").await;
    let monday = time::worklog::log(&state, TICKET, date(MONDAY), 0, at(MONDAY, 9, 0), 3_600, "")
        .await
        .expect("Monday is logged");

    // From here the source refuses the credential, so nothing settles: a
    // `held` row may not carry a settled time (`write_queue_settled_chk`), and
    // a fixture that held a sent write would be a row the schema forbids.
    *wrote.fault.lock().unwrap() = Some(401);

    // Tuesday: logged, and the write is then held.
    block(&state.pool, MONDAY + 1, (9, 0), (10, 0), TICKET, "manual").await;
    let tuesday = time::worklog::log(
        &state,
        TICKET,
        date(MONDAY + 1),
        0,
        at(MONDAY + 1, 9, 0),
        3_600,
        "",
    )
    .await
    .expect("Tuesday is logged");
    hold(
        &state.pool,
        tuesday.write_queue_id.expect("a write carries it"),
    )
    .await;

    // Wednesday: logged while the source refuses the credential, so the write
    // waits in the queue as `pending`.
    block(&state.pool, MONDAY + 2, (9, 0), (10, 0), TICKET, "manual").await;
    let wednesday = time::worklog::log(
        &state,
        TICKET,
        date(MONDAY + 2),
        0,
        at(MONDAY + 2, 9, 0),
        3_600,
        "",
    )
    .await
    .expect("logging does not fail because the source is down");
    *wrote.fault.lock().unwrap() = None;

    // Thursday: tracked, never logged.
    block(&state.pool, MONDAY + 3, (9, 0), (10, 0), TICKET, "manual").await;
    // Friday: knobas' own guess.
    block(&state.pool, MONDAY + 4, (9, 0), (10, 0), TICKET, "passive").await;

    // The states the read is being asked to distinguish, read back rather than
    // assumed -- a fixture that silently settled would make the whole test
    // agree with itself.
    for (id, expected) in [
        (monday.write_queue_id, WriteState::Sent),
        (wednesday.write_queue_id, WriteState::Pending),
    ] {
        let row = knobas_core::write_queue::get(&state.pool, id.expect("a write"))
            .await
            .expect("the queue row is readable")
            .expect("the row exists");
        assert_eq!(row.state, expected, "{:?}", row.detail);
    }

    let week = time::week::read(&state.pool, &week())
        .await
        .expect("the week reads");
    let row = row_for(&week, Some(TICKET)).expect("the ticket has a row");

    let monday_cell = row.cells[column(MONDAY)];
    assert_eq!(monday_cell.tracked_seconds, 3_600);
    assert_eq!(
        monday_cell.logged_seconds, 3_600,
        "a sent worklog is logged"
    );
    assert_eq!(monday_cell.held_seconds, 0);
    assert_eq!(monday_cell.unlogged_seconds, 0);

    let tuesday_cell = row.cells[column(MONDAY + 1)];
    assert_eq!(tuesday_cell.tracked_seconds, 3_600);
    assert_eq!(
        tuesday_cell.logged_seconds, 0,
        "a held worklog is not logged -- it shows as held (spec story 39)"
    );
    assert_eq!(tuesday_cell.held_seconds, 3_600);
    assert_eq!(
        tuesday_cell.unlogged_seconds, 0,
        "held time is spoken for: its blocks carry a worklog id and *Log all* \
         will not offer them again"
    );

    let wednesday_cell = row.cells[column(MONDAY + 2)];
    assert_eq!(
        wednesday_cell.logged_seconds, 3_600,
        "a pending worklog counts as logged: the number is about what the \
         reader did, not about sync timing"
    );
    assert_eq!(wednesday_cell.held_seconds, 0);

    let thursday_cell = row.cells[column(MONDAY + 3)];
    assert_eq!(thursday_cell.tracked_seconds, 3_600);
    assert_eq!(thursday_cell.logged_seconds, 0);
    assert_eq!(
        thursday_cell.unlogged_seconds, 3_600,
        "unlogged is the difference, and this is the day that has one"
    );

    let friday_cell = row.cells[column(MONDAY + 4)];
    assert_eq!(
        friday_cell.tracked_seconds, 0,
        "a passive block is knobas' guess at what was open, and tracked time \
         is what the reader said their day was"
    );
    assert_eq!(
        friday_cell.offered_seconds, 3_600,
        "...but it is counted beside it rather than dropped, the arrangement \
         the day strip on the same screen already uses -- a week that lost it \
         would disagree with the strip about the same afternoon"
    );
    assert_eq!(
        friday_cell.unlogged_seconds, 0,
        "a guess nobody has vouched for is not time somebody failed to log"
    );

    state.scheduler.shutdown().await;
}

/// **The "no target, app open" row tells an open window from a shut one, and
/// from a claimed afternoon.**
///
/// Three days, and the row has to say something different about each:
///
/// * **Monday** -- ten minutes of beats from a room with nothing open. The app
///   was in front of the reader and no block claims the time, so the row says
///   so. Passive attribution is **on** here, which is the only state in which
///   beats are recorded at all; the beats carry no foreground, so the
///   derivation offers no passive block and the time stays unclaimed.
/// * **Tuesday** -- the same ten minutes of beats, **inside a block**. The
///   window was just as open, and the row says nothing: this time is
///   accounted for, and a row that counted it would double the day.
/// * **Wednesday** -- no beats at all. knobas was shut, and a row claiming
///   otherwise would be inventing a working day.
#[tokio::test(flavor = "multi_thread")]
async fn the_no_target_row_tells_app_open_from_app_closed_and_from_claimed_time() {
    let (state, _wrote) = app("week_ipc_open").await;
    time::passive::set_enabled(&state.pool, true)
        .await
        .expect("passive attribution is switched on");

    beats(&state.pool, MONDAY, 9, 0, 21).await;

    beats(&state.pool, MONDAY + 1, 9, 0, 21).await;
    block(&state.pool, MONDAY + 1, (9, 0), (10, 0), TICKET, "manual").await;

    let week = time::week::read(&state.pool, &week())
        .await
        .expect("the week reads");
    let row = row_for(&week, None).expect("an open window has a row of its own");

    // Twenty-one beats thirty seconds apart: the last credits nothing forward,
    // so ten minutes of wall clock is ten minutes of focused time.
    assert_eq!(
        row.cells[column(MONDAY)].tracked_seconds,
        600,
        "the app was open with nothing claiming the time"
    );
    assert_eq!(
        row.cells[column(MONDAY)].unlogged_seconds,
        600,
        "time with nowhere to go has certainly not been logged"
    );
    assert_eq!(
        row.cells[column(MONDAY + 1)].tracked_seconds,
        0,
        "the same open window inside a block is time the block already claims"
    );
    assert_eq!(
        row.cells[column(MONDAY + 2)].tracked_seconds,
        0,
        "no observations at all is knobas shut, not knobas open and idle"
    );

    state.scheduler.shutdown().await;
}

/// **A week of two days on one ticket is two worklogs, not one** -- the whole
/// reason *Log all* exists (spec story 43: "so that a whole week logs
/// correctly rather than as one lump on Friday").
///
/// Two days on `PAY-231` with different totals, plus one day on a second
/// ticket, so "one worklog per day **and ticket**" is wrong in both directions
/// if either half is dropped: a per-ticket-only reading makes one worklog of
/// three hours, and a per-day-only reading merges Wednesday's two tickets.
///
/// The plan is asserted **before** anything is sent, which is the criterion's
/// own wording -- the confirmation lists what it will send.
#[tokio::test(flavor = "multi_thread")]
async fn log_all_makes_one_worklog_per_day_and_ticket() {
    let (state, wrote) = app("week_ipc_split").await;

    block(&state.pool, MONDAY, (9, 0), (10, 0), TICKET, "manual").await;
    block(&state.pool, MONDAY + 1, (9, 0), (11, 0), TICKET, "manual").await;
    // Wednesday: two tickets on one day, and one of them in two sittings with
    // a lunch between -- so the seconds are the blocks added up rather than
    // the window they sat in.
    block(&state.pool, MONDAY + 2, (9, 0), (10, 0), TICKET, "manual").await;
    block(&state.pool, MONDAY + 2, (13, 0), (13, 30), TICKET, "manual").await;
    block(&state.pool, MONDAY + 2, (11, 0), (11, 30), OTHER, "manual").await;

    let planned = time::week::plan(&state.pool, state.registry.as_ref(), &week())
        .await
        .expect("the plan reads");
    assert!(
        wrote.ops.lock().unwrap().is_empty(),
        "the confirmation is drawn before anything is sent"
    );
    let listed: Vec<(NaiveDate, &str, i64, i64)> = planned
        .iter()
        .map(|entry| {
            (
                entry.day,
                entry.entity_id.as_str(),
                entry.seconds,
                entry.blocks,
            )
        })
        .collect();
    assert_eq!(
        listed,
        vec![
            (date(MONDAY), TICKET, 3_600, 1),
            (date(MONDAY + 1), TICKET, 7_200, 1),
            (date(MONDAY + 2), TICKET, 5_400, 2),
            (date(MONDAY + 2), OTHER, 1_800, 1),
        ],
        "four worklogs: one per day and ticket, and Wednesday's two sittings \
         on PAY-231 are one of them"
    );

    let made = time::week::log_all(&state, &week())
        .await
        .expect("the week logs");
    assert_eq!(made.len(), 4, "four worklogs, not one lump: {made:?}");

    let sent: Vec<(String, DateTime<Utc>, i64)> = wrote
        .ops
        .lock()
        .unwrap()
        .iter()
        .map(|op| match op {
            WriteOp::LogWork {
                entity,
                started,
                seconds,
                ..
            } => (entity.clone(), *started, *seconds),
            other => panic!("the source was handed {other:?}, not a worklog"),
        })
        .collect();
    assert_eq!(
        sent,
        vec![
            (TICKET.to_owned(), at(MONDAY, 9, 0), 3_600),
            (TICKET.to_owned(), at(MONDAY + 1, 9, 0), 7_200),
            (TICKET.to_owned(), at(MONDAY + 2, 9, 0), 5_400),
            (OTHER.to_owned(), at(MONDAY + 2, 11, 0), 1_800),
        ],
        "each worklog starts when its own day's first block did, and its \
         seconds are that day's blocks added up"
    );

    // Idempotent: the blocks are spent, so a second run has nothing to send.
    let again = time::week::plan(&state.pool, state.registry.as_ref(), &week())
        .await
        .expect("the plan reads again");
    assert!(again.is_empty(), "every block is spoken for: {again:?}");

    state.scheduler.shutdown().await;
}

/// **Neither a passive block nor a label block is ever touched** (spec: "*Log
/// all* … never touches passive or label blocks").
///
/// Both exclusions on one day, beside one manual block that *is* logged, so
/// the test cannot pass by *Log all* doing nothing at all. Passive attribution
/// is left off, which is its default, so nothing reconciles the passive block
/// away underneath the assertion.
#[tokio::test(flavor = "multi_thread")]
async fn log_all_touches_neither_a_passive_block_nor_a_label_one() {
    let (state, wrote) = app("week_ipc_excludes").await;

    let manual = block(&state.pool, MONDAY, (9, 0), (10, 0), TICKET, "manual").await;
    let guessed = block(&state.pool, MONDAY, (11, 0), (12, 0), TICKET, "passive").await;
    let ad_hoc = label_block(&state.pool, MONDAY, (14, 0), (15, 0), "DB config").await;

    let planned = time::week::plan(&state.pool, state.registry.as_ref(), &week())
        .await
        .expect("the plan reads");
    assert_eq!(
        planned.len(),
        1,
        "only the manual block is offered: {planned:?}"
    );
    assert_eq!(
        planned[0].seconds, 3_600,
        "the guess and the label are not in the hours either"
    );

    time::week::log_all(&state, &week()).await.expect("it logs");

    assert_eq!(
        wrote.ops.lock().unwrap().len(),
        1,
        "one worklog, for the one block a person vouched for"
    );
    assert!(
        worklog_id_of(&state.pool, manual).await.is_some(),
        "the manual block was logged"
    );
    assert_eq!(
        worklog_id_of(&state.pool, guessed).await,
        None,
        "no passive block is ever logged without a person saying so"
    );
    assert_eq!(
        worklog_id_of(&state.pool, ad_hoc).await,
        None,
        "an ad-hoc label is not somewhere a worklog can go -- the day review's \
         *Log to a ticket…* is that path, and it is a person choosing"
    );

    state.scheduler.shutdown().await;
}

/// **A block that ran through midnight, under both of this file's day rules
/// at once** -- the asymmetry `time::week`'s module docs call deliberate, and
/// the one thing in the week that two reasonable readings get differently.
///
/// One stretch, Monday 22:00 to Tuesday 01:00:
///
/// * **Bookkeeping counts it on the day it started, whole.** Monday's
///   *tracked* is all three hours and Tuesday's is zero -- the rule
///   `worklog::UNLOGGED_BLOCKS` keys on, and it has to be the same rule here
///   because *Log all* logs by it. A week that split the block at midnight
///   would draw an hour on Tuesday that *Log all* then sent on Monday, and the
///   two would never reconcile. So the third assertion is not decoration: the
///   plan is checked against the columns.
/// * **Coverage asks about an instant, so it clips to the day.** Tuesday's
///   beats are in two batches -- ten minutes from 00:00, inside the block's
///   Tuesday half, and ten from 09:00, outside every block. The "no target"
///   row must be **600**, not 1200: the block covers the small hours it ran
///   through even though it is booked on Monday.
///
/// The two rules disagree about the same three hours on purpose, and a mutant
/// that makes either one agree with the other dies here.
#[tokio::test(flavor = "multi_thread")]
async fn a_block_through_midnight_is_booked_on_monday_and_covers_tuesdays_small_hours() {
    let (state, wrote) = app("week_ipc_midnight").await;
    let days = week();
    time::passive::set_enabled(&state.pool, true)
        .await
        .expect("passive attribution is switched on");

    sqlx::query(
        "insert into knobas.block (started_at, ended_at, entity_id, kind)
         values ($1, $2, $3, 'manual')",
    )
    .bind(at(MONDAY, 22, 0))
    .bind(at(MONDAY + 1, 1, 0))
    .bind(TICKET)
    .execute(&state.pool)
    .await
    .expect("the overnight block is written");

    // Ten minutes inside the block's Tuesday half, and ten outside every
    // block. Nothing is in the foreground, so no passive block is derived
    // from either batch and the coverage under test is the manual one.
    beats(&state.pool, MONDAY + 1, 0, 0, 21).await;
    beats(&state.pool, MONDAY + 1, 9, 0, 21).await;

    let sheet = time::week::read(&state.pool, &days)
        .await
        .expect("the week reads");
    let ticket = row_for(&sheet, Some(TICKET)).expect("the ticket has a row");
    assert_eq!(
        ticket.cells[column(MONDAY)].tracked_seconds,
        3 * 3_600,
        "a block belongs to the day it started on, whole -- all three hours \
         are Monday's"
    );
    assert_eq!(
        ticket.cells[column(MONDAY + 1)].tracked_seconds,
        0,
        "...and none of them are Tuesday's, because splitting it at midnight \
         would invent a boundary nobody made"
    );

    let open = row_for(&sheet, None).expect("an open window has a row of its own");
    assert_eq!(
        open.cells[column(MONDAY + 1)].tracked_seconds,
        600,
        "coverage is the other rule: the block covers the small hours it ran \
         through, so only the ten minutes at 09:00 have no target"
    );

    // And the columns reconcile with what *Log all* sends: one worklog, on
    // Monday, of the whole stretch.
    let planned = time::week::plan(&state.pool, state.registry.as_ref(), &days)
        .await
        .expect("the plan reads");
    assert_eq!(
        planned
            .iter()
            .map(|entry| (entry.day, entry.seconds))
            .collect::<Vec<_>>(),
        vec![(date(MONDAY), 3 * 3_600)],
        "Monday's column is what Monday sends: {planned:?}"
    );

    time::week::log_all(&state, &days).await.expect("it logs");
    let sent: Vec<(DateTime<Utc>, i64)> = wrote
        .ops
        .lock()
        .unwrap()
        .iter()
        .map(|op| match op {
            WriteOp::LogWork {
                started, seconds, ..
            } => (*started, *seconds),
            other => panic!("the source was handed {other:?}, not a worklog"),
        })
        .collect();
    assert_eq!(
        sent,
        vec![(at(MONDAY, 22, 0), 3 * 3_600)],
        "the worklog starts when the block did, on the evening it started"
    );

    state.scheduler.shutdown().await;
}

// -- discarding a queued worklog (#328) --------------------------------------

/// Is there still a `knobas.worklog` row with this id?
async fn copy_exists(pool: &PgPool, worklog: i64) -> bool {
    sqlx::query_scalar::<_, i64>("select count(*) from knobas.worklog where id = $1")
        .bind(worklog)
        .fetch_one(pool)
        .await
        .expect("the worklog table is readable")
        == 1
}

/// What Jira called this copy, as the copy itself has it.
async fn remote_id_of(pool: &PgPool, worklog: i64) -> Option<String> {
    sqlx::query_scalar("select remote_id from knobas.worklog where id = $1")
        .bind(worklog)
        .fetch_one(pool)
        .await
        .expect("the worklog is readable")
}

/// **Withdrawing a queued worklog gives the afternoon back** (issue #328).
///
/// The four consequences the ticket names, each of which a discard used to
/// leave wrong for good, asserted together rather than split -- because an
/// implementation that fixed one and not the rest is exactly the failure that
/// shipped:
///
/// 1. the week draws the time as **held**, permanently;
/// 2. `unlogged` stays **zero**, so story 41's honest total understates;
/// 3. ***Log all*** never offers the blocks again, and neither does the draft
///    (`week::LOGGABLE` and `worklog::UNLOGGED_BLOCKS` are the two reads);
/// 4. the **day review** will not let them be edited or deleted by hand
///    either -- the `worklog_id is null` guard on `day::update` and
///    `day::remove` is the read-only rule.
///
/// **They are four consequences but not four independent witnesses**, and
/// saying so is worth more than the appearance of coverage: 1 and 2 are two
/// halves of one subtraction (`unlogged = tracked - logged - held`), so no
/// regression can fail one and pass the other. What is separately witnessed
/// is the week cell, the two loggable reads, and the two day-review writes.
///
/// **Tuesday is the control.** A second day is logged and *not* discarded, so
/// the release has something it must leave alone: a statement that deleted
/// every worklog, or released every block, would pass every Monday assertion
/// below and fail here.
///
/// The source refuses the credential throughout, which is what keeps the write
/// `pending` rather than `sent` -- a settled write is not discardable at all
/// (`discard` narrows on `state in ('pending','held','refused')`), so a fixture
/// that let it settle would be testing nothing.
#[tokio::test(flavor = "multi_thread")]
async fn discarding_a_queued_worklog_gives_the_afternoon_back() {
    let (state, wrote) = app("week_ipc_discard").await;
    *wrote.fault.lock().unwrap() = Some(401);

    let monday_block = block(&state.pool, MONDAY, (9, 0), (10, 0), TICKET, "manual").await;
    let monday = time::worklog::log(&state, TICKET, date(MONDAY), 0, at(MONDAY, 9, 0), 3_600, "")
        .await
        .expect("Monday is logged");

    let tuesday_block = block(&state.pool, MONDAY + 1, (9, 0), (10, 0), TICKET, "manual").await;
    let tuesday = time::worklog::log(
        &state,
        TICKET,
        date(MONDAY + 1),
        0,
        at(MONDAY + 1, 9, 0),
        3_600,
        "",
    )
    .await
    .expect("Tuesday is logged");

    // The state the discard is about to act on, read back rather than assumed.
    let queued = monday.write_queue_id.expect("a write carries the worklog");
    assert_eq!(
        knobas_core::write_queue::get(&state.pool, queued)
            .await
            .expect("the queue row is readable")
            .expect("the row exists")
            .state,
        WriteState::Pending,
        "the fixture needs an open write -- a settled one cannot be discarded"
    );
    assert_eq!(
        worklog_id_of(&state.pool, monday_block).await,
        Some(monday.id)
    );

    // The withdrawal itself, through the seam `discard_write` shims.
    let discarded = knobas_sync::write_queue::discard(state.scheduler.deps(), queued)
        .await
        .expect("the discard runs")
        .expect("there was an open write to withdraw");
    assert_eq!(discarded.state, WriteState::Discarded);

    // 1 and 2: the week says unlogged, not held.
    let sheet = time::week::read(&state.pool, &week())
        .await
        .expect("the week reads");
    let row = row_for(&sheet, Some(TICKET)).expect("the ticket has a row");
    let cell = row.cells[column(MONDAY)];
    assert_eq!(cell.tracked_seconds, 3_600);
    assert_eq!(cell.logged_seconds, 0);
    assert_eq!(
        cell.held_seconds, 0,
        "a withdrawn write holds nothing -- there is no worklog left to hold it"
    );
    assert_eq!(
        cell.unlogged_seconds, 3_600,
        "the hour is unlogged again, which is what story 41's honest total needs"
    );

    // 3: *Log all* offers the afternoon again.
    let planned = time::week::plan(&state.pool, state.registry.as_ref(), &week())
        .await
        .expect("the plan reads");
    assert_eq!(
        planned
            .iter()
            .map(|entry| (entry.day, entry.entity_id.clone(), entry.seconds))
            .collect::<Vec<_>>(),
        vec![(date(MONDAY), TICKET.to_owned(), 3_600)],
        "Monday is loggable again and Tuesday is still spoken for: {planned:?}"
    );

    // ...and so does the draft, which is *Log all*'s single-day counterpart
    // and narrows on the same column through its own read
    // (`worklog::UNLOGGED_BLOCKS`). The ticket names both, so both are asked.
    let draft = time::worklog::draft(
        &state.pool,
        state.registry.as_ref(),
        TICKET,
        date(MONDAY),
        0,
    )
    .await
    .expect("the draft reads")
    .expect("Monday has unlogged time on the ticket again");
    assert_eq!(draft.block_ids, vec![monday_block]);
    assert_eq!(draft.seconds, 3_600);

    // 4: and so does the day review -- the read-only rule is off this block.
    time::day::update(
        &state.pool,
        monday_block,
        at(MONDAY, 9, 0),
        at(MONDAY, 9, 30),
        time::TimerTarget::Entity {
            entity_id: TICKET.to_owned(),
        },
    )
    .await
    .expect("a released block is editable again");

    // And the mechanism under all four: the block is knobas' own again, and
    // the copy of a record Jira never took is gone with the write that was
    // going to make it.
    assert_eq!(
        worklog_id_of(&state.pool, monday_block).await,
        None,
        "a discarded worklog gives its blocks back"
    );
    assert!(
        !copy_exists(&state.pool, monday.id).await,
        "nothing was sent, so there is no record to keep a copy of"
    );

    // Tuesday, untouched: the release is keyed on the write that was
    // withdrawn, not on worklogs in general.
    assert_eq!(
        worklog_id_of(&state.pool, tuesday_block).await,
        Some(tuesday.id),
        "the day nobody withdrew keeps its worklog"
    );
    assert!(copy_exists(&state.pool, tuesday.id).await);
    assert_eq!(
        row.cells[column(MONDAY + 1)].logged_seconds,
        3_600,
        "Tuesday's write is still pending, which the week counts as logged"
    );
    assert_eq!(row.cells[column(MONDAY + 1)].unlogged_seconds, 0);

    // The other half of the read-only rule, and last because it takes the row
    // every assertion above reads. `DELETE` carries the same `worklog_id is
    // null` clause `UPDATE` does, so a release that reached one and not the
    // other would leave a block editable but undeletable.
    time::day::remove(&state.pool, monday_block)
        .await
        .expect("a released block can be deleted again");

    state.scheduler.shutdown().await;
}

/// **A worklog Jira answered for is not undone by a discard** (issue #328).
///
/// `remote_id` on the copy is knobas' record that the source made a worklog,
/// and a release that removed one would be knobas forgetting a worklog that
/// exists in Jira -- and then offering the same hour to *Log all* again, which
/// bills it twice. So the delete carries `remote_id is null` as its own
/// condition, and this is the direction that pins it.
///
/// **The fixture stamps the copy directly**, the way `hold` above sets a state
/// directly, and for the same reason: this is the read's input rather than
/// something under test. The state machine does not currently produce it --
/// `sent` writes the id and `state = 'sent'` in one statement, and nothing
/// moves a row out of `sent` -- so the guard is what keeps the delete safe on
/// its own terms rather than by an argument about the rest of the file.
#[tokio::test(flavor = "multi_thread")]
async fn a_discard_leaves_a_worklog_jira_answered_for_alone() {
    let (state, wrote) = app("week_ipc_discard_sent").await;
    *wrote.fault.lock().unwrap() = Some(401);

    let logged = block(&state.pool, MONDAY, (9, 0), (10, 0), TICKET, "manual").await;
    let worklog = time::worklog::log(&state, TICKET, date(MONDAY), 0, at(MONDAY, 9, 0), 3_600, "")
        .await
        .expect("Monday is logged");

    sqlx::query("update knobas.worklog set remote_id = $2 where id = $1")
        .bind(worklog.id)
        .bind(WORKLOG_ID)
        .execute(&state.pool)
        .await
        .expect("the copy is stamped");

    knobas_sync::write_queue::discard(
        state.scheduler.deps(),
        worklog.write_queue_id.expect("a write carries the worklog"),
    )
    .await
    .expect("the discard runs")
    .expect("there was an open write to withdraw");

    assert!(
        copy_exists(&state.pool, worklog.id).await,
        "Jira holds this worklog -- knobas' copy of it is not the queue's to delete"
    );
    assert_eq!(
        remote_id_of(&state.pool, worklog.id).await.as_deref(),
        Some(WORKLOG_ID)
    );
    assert_eq!(
        worklog_id_of(&state.pool, logged).await,
        Some(worklog.id),
        "the blocks stay spoken for: the hour is at Jira"
    );

    let planned = time::week::plan(&state.pool, state.registry.as_ref(), &week())
        .await
        .expect("the plan reads");
    assert!(
        planned.is_empty(),
        "*Log all* must not offer an hour Jira already has: {planned:?}"
    );

    // And the cell, which is the only place `is_logged`'s answer for
    // `discarded` is still reachable from: a copy that outlived the write that
    // was going to carry it. `time/week.rs` and contract.md both say it reads
    // as *held*, and this is the fixture that can say whether they are right.
    let sheet = time::week::read(&state.pool, &week())
        .await
        .expect("the week reads");
    let cell = row_for(&sheet, Some(TICKET))
        .expect("the ticket has a row")
        .cells[column(MONDAY)];
    assert_eq!(
        cell.held_seconds, 3_600,
        "a discarded write over a copy Jira answered for is held, not logged and not unlogged"
    );
    assert_eq!(cell.logged_seconds, 0);
    assert_eq!(cell.unlogged_seconds, 0);

    state.scheduler.shutdown().await;
}

// -- the observation horizon, as the timesheet meets it (#337) --------------

/// **A week that straddles the horizon draws both readings at once.**
///
/// The sharp case, and the one the fixture week is shaped for: the sweep's
/// horizon falls on Thursday's midnight, so Monday to Wednesday are days
/// knobas has no observations for and Thursday to Sunday are days it has them.
/// The "no target, app open" row reads **zero** for every one of the seven --
/// Monday's observations are gone and the rest never had any -- so without the flag
/// the timesheet says the same thing about a day it swept and a day the reader
/// had the app shut on.
///
/// The row is asserted absent as well, deliberately: a week entirely past the
/// horizon loses it altogether (`week::read` drops a row with nothing in it),
/// which is why the reading cannot be hung on that row and rides on the week.
#[tokio::test(flavor = "multi_thread")]
async fn a_week_straddling_the_horizon_says_which_of_its_days_have_no_observations() {
    let (state, _wrote) = app("week_ipc_horizon").await;
    time::passive::set_enabled(&state.pool, true)
        .await
        .expect("passive attribution is switched on");

    // Ten minutes of Monday, which is what gives the sweep something to take
    // and therefore a stamp to write.
    beats(&state.pool, MONDAY, 9, 0, 21).await;

    // A horizon on Thursday's midnight: three days behind it, four in front.
    let thursday = at(MONDAY + 3, 0, 0);
    let taken = time::passive::prune(
        &state.pool,
        thursday + chrono::Duration::days(time::passive::RETENTION_DAYS),
    )
    .await
    .expect("the sweep runs");
    assert_eq!(
        taken, 21,
        "Monday's observations are what the horizon is past"
    );

    let sheet = time::week::read(&state.pool, &week())
        .await
        .expect("the week reads");

    assert_eq!(
        sheet.past_horizon,
        vec![true, true, true, false, false, false, false],
        "the week has to name the days knobas has no observations for, \
         and only those"
    );
    assert!(
        row_for(&sheet, None).is_none(),
        "the fixture is only sharp while the no-target row is absent: with a \
         row of zeros on screen the reader would have something else to read"
    );
}

/// **A week with the app shut all through it is not a week past the horizon.**
///
/// The other direction of the same question, on a database the sweep has never
/// taken anything out of. Every cell reads zero and the no-target row is
/// missing, exactly as in the straddling week above -- and the flag is the
/// only thing that disagrees, which is the whole of what it is for.
#[tokio::test(flavor = "multi_thread")]
async fn a_week_nobody_had_the_app_open_in_is_not_past_the_horizon() {
    let (state, _wrote) = app("week_ipc_horizon_shut").await;
    time::passive::set_enabled(&state.pool, true)
        .await
        .expect("passive attribution is switched on");

    let sheet = time::week::read(&state.pool, &week())
        .await
        .expect("the week reads");

    assert_eq!(
        sheet.past_horizon,
        vec![false; 7],
        "nothing has been swept, so knobas still has every observation these \
         days never had"
    );
    assert!(row_for(&sheet, None).is_none(), "the app was shut all week");
}
