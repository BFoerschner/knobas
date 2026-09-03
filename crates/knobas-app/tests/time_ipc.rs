//! The timer's lifecycle as the interface sees it, including the one thing
//! nothing else can witness: **relaunch closing a stranded timer at the last
//! heartbeat** (issue #278).
//!
//! # Why the seam is here
//!
//! `knobas_app::time` is where the decisions are, and it is reachable without
//! a window (`commands/time.rs` is shims over it, the arrangement the §10.8
//! entry ratifies). So the tests drive the store against a real database, the
//! way `tests/start_work.rs` and `tests/inbox_ipc.rs` do -- a command in, a
//! DTO out -- rather than asserting on SQL or on the row the statement wrote.
//!
//! # Why the clock is dictated and not waited for
//!
//! The relaunch rule is *"closed at the last heartbeat, not at start-up"*, and
//! those two are indistinguishable in a test whose heartbeat was a moment ago.
//! Every timer here is therefore aged by moving `started_at` and
//! `last_heartbeat` **backwards** with one `update` -- the only place these
//! tests touch SQL, and they touch it to build a past, never to assert one.
//! The gap between the last heartbeat and the sweep is **hours**, so an
//! implementation that closed at `now()` fails by a margin no scheduling
//! jitter could produce.
//!
//! # Why every test gets a database of its own
//!
//! There is at most one timer row in a database, by construction (migration
//! `0013`'s singleton primary key). Two tests sharing one would be deciding
//! each other's outcomes on the very rule the schema exists to make
//! structural. The same reasoning `tests/start_work.rs` records for its flows.

use chrono::{DateTime, Duration, Utc};
use knobas_app::IpcErrorCode;
use knobas_app::time::{self, BlockKind, TimerTarget};
use sqlx::{PgPool, Row};

const TICKET: &str = "jira:PAY-231";
const LABEL: &str = "DB config for the migration";

/// A migrated, empty database of this test's own.
async fn scratch(name: &str) -> PgPool {
    knobas_db::test_util::scratch_database(name)
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database")
}

fn on(entity_id: &str) -> TimerTarget {
    TimerTarget::Entity {
        entity_id: entity_id.to_owned(),
    }
}

fn labelled(label: &str) -> TimerTarget {
    TimerTarget::Label {
        label: label.to_owned(),
    }
}

/// Move the running timer into the past: started `ago`, last heard from
/// `silent_for` ago.
///
/// The one SQL in this file, and it writes a **fixture**, never an assertion.
/// A test that could not put a timer in yesterday could not tell "closed at
/// the last heartbeat" from "closed just now", which is the whole of what the
/// relaunch rule promises.
async fn age(pool: &PgPool, ago: Duration, silent_for: Duration) {
    let rows = sqlx::query(
        "update knobas.timer
            set started_at = now() - $1::interval,
                last_heartbeat = now() - $2::interval",
    )
    .bind(ago)
    .bind(silent_for)
    .execute(pool)
    .await
    .expect("the timer is aged")
    .rows_affected();
    assert_eq!(rows, 1, "there was no timer to age");
}

/// The activity verbs in the log, newest first.
async fn verbs(pool: &PgPool) -> Vec<(String, String)> {
    knobas_core::activity::recent(pool, 20, None)
        .await
        .expect("the activity log is readable")
        .into_iter()
        .map(|line| (line.actor, line.verb))
        .collect()
}

// -- starting ---------------------------------------------------------------

#[tokio::test]
async fn a_started_timer_is_what_the_next_read_answers_with() {
    let pool = scratch("time-start").await;
    assert_eq!(
        time::current(&pool).await.unwrap(),
        None,
        "a fresh profile has no timer"
    );

    let started = time::start(&pool, on(TICKET)).await.expect("it starts");
    assert_eq!(started.timer.target, on(TICKET));

    let read = time::current(&pool)
        .await
        .unwrap()
        .expect("the timer is running");
    assert_eq!(read, started.timer, "the read and the start disagree");
}

/// Story 9: "DB config for the migration" is as legal a target as a ticket.
#[tokio::test]
async fn an_ad_hoc_label_is_as_legal_a_target_as_a_ticket() {
    let pool = scratch("time-label").await;
    let started = time::start(&pool, labelled(LABEL)).await.expect("it starts");
    assert_eq!(started.timer.target, labelled(LABEL));

    let block = time::stop(&pool)
        .await
        .unwrap()
        .expect("a label timer closes a block like any other");
    assert_eq!(block.block.target, labelled(LABEL));
}

/// Story 15, at the seam that decides it. `ctx:` is the namespace stored
/// contexts live in, and a context is a set: time on it has nowhere to go.
///
/// The context row is **real** -- created through `knobas_core::context` --
/// so the refusal is about a target a person could actually be standing in,
/// not about a string invented by the test.
#[tokio::test]
async fn a_stored_context_is_refused_as_a_target() {
    let pool = scratch("time-context").await;
    let context = knobas_core::context::create_adhoc(&pool, "SEPA migration")
        .await
        .expect("a stored context");
    assert!(
        context.id.starts_with("ctx:"),
        "this test is about the ctx namespace: {}",
        context.id
    );

    let refusal = time::start(&pool, on(&context.id))
        .await
        .expect_err("a context is a set, and time on a set has nowhere to go");
    assert_eq!(refusal.code, IpcErrorCode::Invalid);
    assert_eq!(
        time::current(&pool).await.unwrap(),
        None,
        "the refused start left a timer running"
    );
}

/// At most one timer, and the refusal says so rather than replacing the one
/// that is running -- a silent replacement is an afternoon discarded.
#[tokio::test]
async fn a_second_start_is_refused_and_leaves_the_first_running() {
    let pool = scratch("time-second").await;
    let first = time::start(&pool, on(TICKET)).await.expect("it starts");

    let refusal = time::start(&pool, labelled(LABEL))
        .await
        .expect_err("there is already a timer");
    assert_eq!(refusal.code, IpcErrorCode::Conflict);

    assert_eq!(
        time::current(&pool).await.unwrap().map(|t| t.target),
        Some(first.timer.target),
        "the refused start took the timer over anyway"
    );
}

// -- stopping ---------------------------------------------------------------

#[tokio::test]
async fn stopping_closes_a_block_over_the_time_the_timer_ran() {
    let pool = scratch("time-stop").await;
    time::start(&pool, on(TICKET)).await.expect("it starts");
    age(&pool, Duration::minutes(45), Duration::seconds(10)).await;

    let stopped = time::stop(&pool).await.unwrap().expect("a block was closed");
    let block = stopped.block;
    assert_eq!(block.target, on(TICKET));
    assert_eq!(block.kind, BlockKind::Manual);
    assert!(
        !block.ended_by_relaunch,
        "a block somebody stopped is not one knobas closed"
    );
    assert_eq!(block.worklog_id, None, "nothing in #278 logs a worklog");

    let minutes = (block.ended_at - block.started_at).num_minutes();
    assert_eq!(minutes, 45, "the block does not span the time the timer ran");

    assert_eq!(
        time::current(&pool).await.unwrap(),
        None,
        "the timer is still running after a stop"
    );
}

/// The `unlink` discipline: a second *Stop* is a success, not a message to
/// dismiss -- and it writes no second block.
#[tokio::test]
async fn stopping_a_stopped_timer_is_a_success_that_writes_nothing() {
    let pool = scratch("time-stop-twice").await;
    time::start(&pool, on(TICKET)).await.expect("it starts");
    time::stop(&pool).await.unwrap().expect("the first stop");

    assert!(
        time::stop(&pool).await.unwrap().is_none(),
        "the second stop invented a block"
    );
    assert_eq!(blocks(&pool).await, 1, "the second stop wrote a second block");
}

/// How many blocks the database holds. Counted rather than listed: what these
/// tests are about is *how many stops happened*, and the block itself is
/// asserted through the value the store handed back.
async fn blocks(pool: &PgPool) -> i64 {
    sqlx::query("select count(*) from knobas.block")
        .fetch_one(pool)
        .await
        .expect("the blocks are countable")
        .get::<i64, _>(0)
}

// -- the heartbeat ----------------------------------------------------------

#[tokio::test]
async fn a_heartbeat_moves_the_last_alive_stamp_forward() {
    let pool = scratch("time-beat").await;
    time::start(&pool, on(TICKET)).await.expect("it starts");
    age(&pool, Duration::hours(3), Duration::hours(2)).await;

    let before = time::current(&pool).await.unwrap().expect("running");
    let after = time::heartbeat(&pool, Some(on(TICKET)))
        .await
        .unwrap()
        .expect("the heartbeat answers with the timer");

    assert!(
        after.last_heartbeat > before.last_heartbeat + Duration::hours(1),
        "the heartbeat did not advance the last-alive stamp: {} -> {}",
        before.last_heartbeat,
        after.last_heartbeat
    );
    assert_eq!(
        after.started_at, before.started_at,
        "a heartbeat is not a restart"
    );
}

/// The heartbeat is sent on a schedule, not on a state, so "nothing is
/// running" is its ordinary answer -- and it is also how a shell holding a
/// stale timer learns the clock has stopped.
#[tokio::test]
async fn a_heartbeat_with_no_timer_running_is_not_a_failure() {
    let pool = scratch("time-beat-idle").await;
    assert_eq!(time::heartbeat(&pool, None).await.unwrap(), None);
}

/// A foreground the timer could never run on is a frontend bug, and refusing
/// it here is what stops passive attribution (#281) inheriting one.
#[tokio::test]
async fn a_heartbeat_carrying_a_context_as_its_foreground_is_refused() {
    let pool = scratch("time-beat-ctx").await;
    let context = knobas_core::context::create_adhoc(&pool, "SEPA migration")
        .await
        .expect("a stored context");

    let refusal = time::heartbeat(&pool, Some(on(&context.id)))
        .await
        .expect_err("a context is not a thing time can be attributed to");
    assert_eq!(refusal.code, IpcErrorCode::Invalid);
}

// -- relaunch ---------------------------------------------------------------

/// **Acceptance criterion 4.** knobas was killed at 09:15 with a timer that
/// started at 06:00; it opens again now. The block ends where knobas stopped
/// being alive, not where it came back.
///
/// The two candidate answers are hours apart, deliberately: `now()` and "the
/// last heartbeat" would be the same value within a tolerance if the fixture
/// were minutes old, and the test would pass against the bug it exists to
/// catch.
#[tokio::test]
async fn relaunch_closes_a_stranded_timer_at_its_last_heartbeat() {
    let pool = scratch("time-relaunch").await;
    time::start(&pool, on(TICKET)).await.expect("it starts");
    let started = Duration::hours(9);
    let died = Duration::hours(6);
    age(&pool, started, died).await;
    let stranded = time::current(&pool).await.unwrap().expect("running");

    let block = time::close_stranded(&pool)
        .await
        .unwrap()
        .expect("the stranded timer was closed");

    assert_eq!(block.target, on(TICKET));
    assert!(
        block.ended_by_relaunch,
        "the block does not say knobas closed it, so the day review cannot \
         offer *Extend to now*"
    );
    assert_eq!(
        block.ended_at, stranded.last_heartbeat,
        "the block was closed somewhere other than the last heartbeat"
    );
    assert_eq!(block.started_at, stranded.started_at);

    // ...and the same fact stated as the difference that matters: the block is
    // three hours long, not nine, and it does not reach the present.
    assert_eq!((block.ended_at - block.started_at).num_hours(), 3);
    assert!(
        Utc::now() - block.ended_at > Duration::hours(5),
        "the block reaches the present, which is what closing at now() looks \
         like: it ends at {}",
        block.ended_at
    );

    assert_eq!(
        time::current(&pool).await.unwrap(),
        None,
        "the timer row survived the sweep, so the strip still draws a clock \
         that has been running all night"
    );
}

/// The ordinary launch. A sweep that closed something here would be inventing
/// a block on every start-up.
#[tokio::test]
async fn relaunch_with_no_timer_closes_nothing() {
    let pool = scratch("time-relaunch-idle").await;
    assert!(time::close_stranded(&pool).await.unwrap().is_none());
    assert_eq!(blocks(&pool).await, 0);
}

/// A timer that died before its first heartbeat closes at zero length rather
/// than not at all: "a timer was running and knobas stopped" is a fact the
/// day review can act on.
#[tokio::test]
async fn a_timer_that_never_saw_a_heartbeat_still_closes() {
    let pool = scratch("time-relaunch-instant").await;
    time::start(&pool, labelled(LABEL)).await.expect("it starts");
    age(&pool, Duration::hours(4), Duration::hours(4)).await;

    let block = time::close_stranded(&pool)
        .await
        .unwrap()
        .expect("a zero-length block is still a block");
    assert_eq!(block.started_at, block.ended_at);
    assert!(block.ended_by_relaunch);
}

// -- the activity signal ----------------------------------------------------

/// Acceptance criterion 3: the shell learns through the signal it already
/// watches, so every one of these has to actually write a line.
#[tokio::test]
async fn starting_and_stopping_write_the_activity_lines_the_shell_reads() {
    let pool = scratch("time-activity").await;
    time::start(&pool, on(TICKET)).await.expect("it starts");
    time::stop(&pool).await.unwrap().expect("it stops");

    assert_eq!(
        verbs(&pool).await,
        vec![
            ("user".to_owned(), "stopped".to_owned()),
            ("user".to_owned(), "started".to_owned()),
        ],
        "the status bar's latest-change line has nothing to show"
    );
}

/// ...and the relaunch sweep is **not** the user. A person reading their own
/// name against a stop they did not make is knobas lying about who did what.
#[tokio::test]
async fn the_relaunch_sweep_signs_its_line_as_knobas() {
    let pool = scratch("time-activity-sweep").await;
    time::start(&pool, on(TICKET)).await.expect("it starts");
    age(&pool, Duration::hours(9), Duration::hours(6)).await;
    time::close_stranded(&pool).await.unwrap().expect("swept");

    let lines = verbs(&pool).await;
    assert_eq!(lines[0], ("knobas".to_owned(), "closed".to_owned()));
}

/// A label timer's line carries no entity id, because there is no entity --
/// and the detail is what says what the time was on.
#[tokio::test]
async fn a_label_timers_line_carries_the_label_and_no_entity() {
    let pool = scratch("time-activity-label").await;
    let started = time::start(&pool, labelled(LABEL)).await.expect("it starts");

    assert_eq!(started.activity.entity_id, None);
    assert_eq!(
        started.activity.detail["timer"],
        serde_json::json!({"kind": "label", "label": LABEL})
    );
}

/// ...and an entity timer's line does carry it, so the detail view's history
/// panel and the digest both find it under the ticket.
#[tokio::test]
async fn an_entity_timers_line_is_filed_under_that_entity() {
    let pool = scratch("time-activity-entity").await;
    let started = time::start(&pool, on(TICKET)).await.expect("it starts");
    assert_eq!(started.activity.entity_id.as_deref(), Some(TICKET));

    let stopped = time::stop(&pool).await.unwrap().expect("it stops");
    assert_eq!(stopped.activity.entity_id.as_deref(), Some(TICKET));
    assert_eq!(stopped.activity.detail["block_id"], stopped.block.id);
}

// -- what the schema refuses ------------------------------------------------

/// The exactly-one-target constraint, in **both** directions.
///
/// Neither is reachable through `time::start`: `TimerTarget` is a tagged enum,
/// so "both halves" and "neither half" have no spelling in Rust at all. The
/// check constraint is therefore a backstop against a future writer -- #279's
/// block editor, #281's passive derivation, a restored backup -- and a
/// backstop nothing exercises is a backstop nobody knows is there. So these
/// two go through raw `insert`s, which is the only way to reach it.
#[tokio::test]
async fn the_schema_refuses_a_target_that_is_both_halves_or_neither() {
    let pool = scratch("time-target-check").await;

    for (entity_id, label, what) in [
        (Some(TICKET), Some(LABEL), "both halves"),
        (None, None, "neither half"),
    ] {
        let refused = sqlx::query(
            "insert into knobas.timer (entity_id, label) values ($1, $2)",
        )
        .bind(entity_id)
        .bind(label)
        .execute(&pool)
        .await;
        assert!(
            refused.is_err(),
            "knobas.timer accepted a target that is {what}"
        );

        let refused = sqlx::query(
            "insert into knobas.block (started_at, ended_at, entity_id, label, kind)
             values (now(), now(), $1, $2, 'manual')",
        )
        .bind(entity_id)
        .bind(label)
        .execute(&pool)
        .await;
        assert!(
            refused.is_err(),
            "knobas.block accepted a target that is {what}"
        );
    }
}

/// The singleton, at the level it is enforced. `time::start`'s `conflict` is
/// the message; this is the reason the message can be trusted -- there is no
/// state in which two timers exist, whatever writes them.
#[tokio::test]
async fn the_schema_refuses_a_second_timer_row() {
    let pool = scratch("time-singleton").await;
    time::start(&pool, on(TICKET)).await.expect("it starts");

    let refused = sqlx::query("insert into knobas.timer (label) values ($1)")
        .bind(LABEL)
        .execute(&pool)
        .await;
    assert!(
        refused.is_err(),
        "a second timer row was accepted, so two surfaces can disagree about \
         what the clock is on"
    );
}

/// A block that ends before it starts is not a short block, it is a bad write.
#[tokio::test]
async fn the_schema_refuses_a_block_that_ends_before_it_starts() {
    let pool = scratch("time-span-check").await;
    let refused = sqlx::query(
        "insert into knobas.block (started_at, ended_at, label, kind)
         values (now(), now() - interval '1 hour', $1, 'manual')",
    )
    .bind(LABEL)
    .execute(&pool)
    .await;
    assert!(refused.is_err(), "a block ran backwards and was accepted");
}

/// And the direction that shows the check above is not simply refusing
/// everything: equal ends are legal, and are exactly what a timer stopped
/// inside one second -- and what relaunch on a heartbeat-less timer --
/// produce.
#[tokio::test]
async fn the_schema_accepts_a_block_of_no_length() {
    let pool = scratch("time-span-zero").await;
    let at: DateTime<Utc> = Utc::now();
    sqlx::query(
        "insert into knobas.block (started_at, ended_at, label, kind)
         values ($1, $1, $2, 'manual')",
    )
    .bind(at)
    .bind(LABEL)
    .execute(&pool)
    .await
    .expect("a block of no length is a block");
}
