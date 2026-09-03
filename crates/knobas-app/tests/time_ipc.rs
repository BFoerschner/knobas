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

use chrono::{DateTime, Duration, TimeZone, Utc};
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

/// An instant on [`DAY`], the day every worklog fixture below is on.
fn at(hour: u32, minute: u32) -> DateTime<Utc> {
    use chrono::TimeZone;
    Utc.with_ymd_and_hms(2026, 9, 3, hour, minute, 0).unwrap()
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
    let started = time::start(&pool, labelled(LABEL))
        .await
        .expect("it starts");
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

    let stopped = time::stop(&pool)
        .await
        .unwrap()
        .expect("a block was closed");
    let block = stopped.block;
    assert_eq!(block.target, on(TICKET));
    assert_eq!(block.kind, BlockKind::Manual);
    assert!(
        !block.ended_by_relaunch,
        "a block somebody stopped is not one knobas closed"
    );
    assert_eq!(block.worklog_id, None, "nothing in #278 logs a worklog");

    let minutes = (block.ended_at - block.started_at).num_minutes();
    assert_eq!(
        minutes, 45,
        "the block does not span the time the timer ran"
    );

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
    assert_eq!(
        blocks(&pool).await,
        1,
        "the second stop wrote a second block"
    );
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

/// **A foreground the backend dislikes must not cost the beat.**
///
/// The stamp is a statement about knobas being alive, not about what the
/// reader was looking at. A heartbeat refused because its foreground was
/// malformed would leave `last_heartbeat` frozen, and the next relaunch would
/// close the block there -- silently discarding every hour since, which is the
/// one failure the relaunch rule exists to prevent. So the observation is
/// dropped and the stamp lands.
#[tokio::test]
async fn a_foreground_the_timer_could_never_run_on_does_not_cost_the_beat() {
    let pool = scratch("time-beat-ctx").await;
    let context = knobas_core::context::create_adhoc(&pool, "SEPA migration")
        .await
        .expect("a stored context");
    time::start(&pool, on(TICKET)).await.expect("it starts");
    age(&pool, Duration::hours(3), Duration::hours(2)).await;
    let before = time::current(&pool).await.unwrap().expect("running");

    let after = time::heartbeat(&pool, Some(on(&context.id)))
        .await
        .expect("a context foreground is not a reason to lose the stamp")
        .expect("the timer is still running");

    assert!(
        after.last_heartbeat > before.last_heartbeat + Duration::hours(1),
        "the stamp did not move, so a relaunch would close this block two \
         hours early: {} -> {}",
        before.last_heartbeat,
        after.last_heartbeat
    );
    // ...and the observation itself went nowhere: the target is untouched.
    assert_eq!(after.target, on(TICKET));
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
    time::start(&pool, labelled(LABEL))
        .await
        .expect("it starts");
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
    let started = time::start(&pool, labelled(LABEL))
        .await
        .expect("it starts");

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
/// block editor, #282's passive derivation, a restored backup -- and a
/// backstop nothing exercises is a backstop nobody knows is there. So these
/// two go through raw `insert`s, which is the only way to reach it.
#[tokio::test]
async fn the_schema_refuses_a_target_that_is_both_halves_or_neither() {
    let pool = scratch("time-target-check").await;

    for (entity_id, label, what) in [
        (Some(TICKET), Some(LABEL), "both halves"),
        (None, None, "neither half"),
    ] {
        let refused = sqlx::query("insert into knobas.timer (entity_id, label) values ($1, $2)")
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

    // ...and the same refusal by the other door. The insert above collides on
    // the primary key because `only_one` defaults to `true`; a writer that
    // spells the column out can ask for `false`, and only
    // `timer_only_one_chk` stands between that and a second legal row. The
    // migration's own comment says the check is what makes the key a
    // singleton "rather than merely a boolean key with two legal rows" --
    // this is the assertion that makes the sentence true. Without it the
    // check can be deleted and every test here stays green.
    let refused = sqlx::query("insert into knobas.timer (only_one, label) values (false, $1)")
        .bind(LABEL)
        .execute(&pool)
        .await;
    assert!(
        refused.is_err(),
        "knobas.timer took a second row with `only_one = false`, so the \
         primary key is a boolean key with two legal rows and the timer is \
         no longer a singleton"
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

// -- the day review: reading a day ------------------------------------------

/// Put a block in the past directly. The day review's fixtures are *finished*
/// blocks, and the only writer of one until now was a timer being stopped --
/// which can only ever produce a block ending at `now()`. A day with a
/// morning, an afternoon and a gap between them cannot be built by stopping a
/// clock three times.
///
/// SQL, and the same licence `age` above takes: it writes a **past**, never an
/// assertion. Everything asserted goes through `time::day`.
async fn block_at(
    pool: &PgPool,
    started_at: DateTime<Utc>,
    ended_at: DateTime<Utc>,
    target: &TimerTarget,
) -> i64 {
    let (entity_id, label) = match target {
        TimerTarget::Entity { entity_id } => (Some(entity_id.as_str()), None),
        TimerTarget::Label { label } => (None, Some(label.as_str())),
    };
    sqlx::query(
        "insert into knobas.block (started_at, ended_at, entity_id, label, kind)
         values ($1, $2, $3, $4, 'manual') returning id",
    )
    .bind(started_at)
    .bind(ended_at)
    .bind(entity_id)
    .bind(label)
    .fetch_one(pool)
    .await
    .expect("a block in the past")
    .get::<i64, _>("id")
}

#[tokio::test]
async fn the_days_blocks_come_back_in_time_order_whatever_order_they_were_written_in() {
    let pool = scratch("time-day-list").await;
    let day = Utc.with_ymd_and_hms(2026, 9, 3, 0, 0, 0).unwrap();
    let at = |h, m| day + Duration::hours(h) + Duration::minutes(m);

    // Written out of order on purpose, and out of *both* orders: three blocks
    // whose insertion order is neither the time order nor its reverse, so
    // "ordered by id" and "ordered by id backwards" are each wrong here. A
    // two-block fixture cannot separate those -- the reverse of a two-item
    // list written backwards is the right answer by accident.
    block_at(&pool, at(11, 0), at(12, 0), &on(TICKET)).await;
    block_at(&pool, at(9, 0), at(10, 0), &labelled(LABEL)).await;
    block_at(&pool, at(13, 0), at(14, 30), &on(TICKET)).await;

    let listed = time::day::list(&pool, day, day + Duration::days(1))
        .await
        .expect("the day is readable");

    assert_eq!(
        listed
            .iter()
            .map(|d| d.block.started_at)
            .collect::<Vec<_>>(),
        vec![at(9, 0), at(11, 0), at(13, 0)],
        "the day review draws its strip in the order this list comes in"
    );
}

/// Put a row in the mirror, so a block's target has a name.
async fn mirrored(pool: &PgPool, entity_id: &str, title: &str) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', $2)")
        .bind(entity_id)
        .bind(title)
        .execute(pool)
        .await
        .expect("a mirrored ticket");
}

/// Stamp a block as logged, by writing the worklog it was logged into, and
/// answer that worklog's id.
///
/// **A real `knobas.worklog` row, since #280** -- which is the shape this
/// helper's own note said it would take once the worklog landed. Migration
/// `0014` puts a foreign key on `knobas.block.worklog_id`, so a fabricated id
/// is now a state the schema refuses; that is what the key is for, and it is
/// what makes deleting a worklog give its blocks back rather than leave them
/// locked for ever.
async fn logged_into(pool: &PgPool, block: i64) -> i64 {
    let worklog = sqlx::query_scalar::<_, i64>(
        "insert into knobas.worklog
           (entity_id, started_at, seconds, comment, block_ids)
         values ($1, now(), 3600, '', array[$2::bigint]) returning id",
    )
    .bind(TICKET)
    .bind(block)
    .fetch_one(pool)
    .await
    .expect("a worklog to log the block into");
    let rows = sqlx::query("update knobas.block set worklog_id = $2 where id = $1")
        .bind(block)
        .bind(worklog)
        .execute(pool)
        .await
        .expect("the block is stamped")
        .rows_affected();
    assert_eq!(rows, 1, "there was no block {block} to stamp");
    worklog
}

/// The strip labels an entity block with the title the mirror holds, and falls
/// back to nothing -- so the view shows the id -- where it holds none.
///
/// Both directions in one test, because a `title` that is always `None` and a
/// `title` that is always the first row's would each pass half of it.
#[tokio::test]
async fn an_entity_block_carries_the_mirrors_title_and_a_blank_one_carries_none() {
    let pool = scratch("time-day-title").await;
    let day = Utc.with_ymd_and_hms(2026, 9, 3, 0, 0, 0).unwrap();
    let at = |h: i64| day + Duration::hours(h);

    mirrored(&pool, TICKET, "Retry failed SEPA payouts").await;
    // Synced before its adapter could name it: the column defaults to `''`.
    mirrored(&pool, "jira:PAY-9", "   ").await;

    block_at(&pool, at(9), at(10), &on(TICKET)).await;
    block_at(&pool, at(11), at(12), &on("jira:PAY-9")).await;
    block_at(&pool, at(13), at(14), &on("jira:PAY-404")).await;

    let listed = time::day::list(&pool, day, day + Duration::days(1))
        .await
        .expect("the day is readable");
    assert_eq!(
        listed
            .iter()
            .map(|d| d.title.clone())
            .collect::<Vec<Option<String>>>(),
        vec![Some("Retry failed SEPA payouts".to_owned()), None, None],
        "a blank title and a purged entity both have to read as `no name`, and \
         a mirrored one has to read as its name"
    );
}

/// A block that ran through midnight belongs to **both** days it touched: the
/// day review a person most wants to fix is the one with the block they left
/// running, and a read that dropped it would be a strip with nothing to edit.
#[tokio::test]
async fn a_block_that_ran_through_midnight_is_on_both_days() {
    let pool = scratch("time-day-midnight").await;
    let midnight = Utc.with_ymd_and_hms(2026, 9, 3, 0, 0, 0).unwrap();
    let overnight = block_at(
        &pool,
        midnight - Duration::hours(2),
        midnight + Duration::hours(1),
        &labelled(LABEL),
    )
    .await;
    // ...and one wholly on the day before, which must *not* reach the 3rd.
    block_at(
        &pool,
        midnight - Duration::hours(6),
        midnight - Duration::hours(5),
        &on(TICKET),
    )
    .await;

    let third = time::day::list(&pool, midnight, midnight + Duration::days(1))
        .await
        .expect("the 3rd is readable");
    assert_eq!(
        third.iter().map(|d| d.block.id).collect::<Vec<_>>(),
        vec![overnight],
        "the day review reads the blocks that overlap the day, and only those"
    );

    let second = time::day::list(&pool, midnight - Duration::days(1), midnight)
        .await
        .expect("the 2nd is readable");
    assert!(
        second.iter().any(|d| d.block.id == overnight),
        "the same block has to be on the day it started too"
    );
}

// -- the day review: editing a block ----------------------------------------

/// Story 19: a mistake at the keyboard is a mistake I can fix. All three
/// fields move, and what comes back is what the next read says.
#[tokio::test]
async fn a_blocks_start_end_and_target_can_all_be_moved() {
    let pool = scratch("time-day-edit").await;
    let day = Utc.with_ymd_and_hms(2026, 9, 3, 0, 0, 0).unwrap();
    let at = |h: i64, m: i64| day + Duration::hours(h) + Duration::minutes(m);
    mirrored(&pool, TICKET, "Retry failed SEPA payouts").await;
    let id = block_at(&pool, at(9, 0), at(10, 0), &labelled(LABEL)).await;

    let edited = time::day::update(&pool, id, at(9, 30), at(11, 15), on(TICKET))
        .await
        .expect("a manual block is editable");

    assert_eq!(edited.block.started_at, at(9, 30));
    assert_eq!(edited.block.ended_at, at(11, 15));
    assert_eq!(edited.block.target, on(TICKET));
    assert_eq!(
        edited.title.as_deref(),
        Some("Retry failed SEPA payouts"),
        "the answer has to carry the new target's name, or the strip draws \
         the old one until the next read"
    );

    let listed = time::day::list(&pool, day, day + Duration::days(1))
        .await
        .expect("the day is readable");
    assert_eq!(listed, vec![edited], "the read and the write disagree");
}

/// *Extend to now*, at the seam that decides it (#272, story 13; #279's fourth
/// criterion). **Both halves**: the end moves to the moment asked for, and the
/// block stops claiming knobas chose its end.
#[tokio::test]
async fn extending_a_relaunch_ended_block_moves_its_end_and_drops_the_marker() {
    let pool = scratch("time-day-extend").await;
    // A real stranded block, closed by the sweep, rather than one this test
    // flagged by hand: the marker is what the sweep writes, and a fixture that
    // set the column itself would not witness that the two agree.
    time::start(&pool, on(TICKET)).await.expect("it starts");
    age(&pool, Duration::hours(9), Duration::hours(6)).await;
    let closed = time::close_stranded(&pool)
        .await
        .unwrap()
        .expect("the sweep closed it");
    assert!(closed.ended_by_relaunch, "this test needs a marked block");

    let now = Utc::now();
    let extended = time::day::update(&pool, closed.id, closed.started_at, now, closed.target)
        .await
        .expect("a relaunch-ended block is editable");

    assert_eq!(
        extended.block.ended_at, now,
        "*Extend to now* did not move the end, so the block still stops where \
         knobas stopped being alive"
    );
    assert!(
        !extended.block.ended_by_relaunch,
        "the block still says knobas closed it, so the strip goes on offering \
         *Extend to now* on a block the reader has already vouched for"
    );
    assert_eq!(
        extended.block.started_at, closed.started_at,
        "extending the end moved the start too"
    );
}

/// A block that ends before it starts is refused **with a sentence**.
/// `block_span_chk` refuses it too, and that is the backstop: a check
/// violation reaches the reader as an internal error, which is not something
/// anybody can act on.
#[tokio::test]
async fn an_end_before_its_start_is_refused_in_words_the_reader_can_act_on() {
    let pool = scratch("time-day-backwards").await;
    let day = Utc.with_ymd_and_hms(2026, 9, 3, 0, 0, 0).unwrap();
    let id = block_at(
        &pool,
        day + Duration::hours(9),
        day + Duration::hours(10),
        &on(TICKET),
    )
    .await;

    let refusal = time::day::update(
        &pool,
        id,
        day + Duration::hours(10),
        day + Duration::hours(9),
        on(TICKET),
    )
    .await
    .expect_err("a block cannot run backwards");
    assert_eq!(refusal.code, IpcErrorCode::Invalid);
    assert!(
        refusal.message.contains("end before it starts"),
        "the refusal has to say what is wrong: {}",
        refusal.message
    );

    let listed = time::day::list(&pool, day, day + Duration::days(1))
        .await
        .unwrap();
    assert_eq!(
        listed[0].block.started_at,
        day + Duration::hours(9),
        "the refused edit landed anyway"
    );
}

/// A stored context is no more a block's target than a timer's (story 15) --
/// the same `vet`, so the rule cannot be spelled two ways.
#[tokio::test]
async fn a_block_cannot_be_retargeted_onto_a_stored_context() {
    let pool = scratch("time-day-ctx").await;
    let day = Utc.with_ymd_and_hms(2026, 9, 3, 0, 0, 0).unwrap();
    let context = knobas_core::context::create_adhoc(&pool, "SEPA migration")
        .await
        .expect("a stored context");
    let id = block_at(
        &pool,
        day + Duration::hours(9),
        day + Duration::hours(10),
        &on(TICKET),
    )
    .await;

    let refusal = time::day::update(
        &pool,
        id,
        day + Duration::hours(9),
        day + Duration::hours(10),
        on(&context.id),
    )
    .await
    .expect_err("a context is a set, and time on a set has nowhere to go");
    assert_eq!(refusal.code, IpcErrorCode::Invalid);
    assert!(refusal.message.contains("stored context"));
}

#[tokio::test]
async fn a_block_can_be_deleted_and_the_day_stops_listing_it() {
    let pool = scratch("time-day-delete").await;
    let day = Utc.with_ymd_and_hms(2026, 9, 3, 0, 0, 0).unwrap();
    let kept = block_at(
        &pool,
        day + Duration::hours(9),
        day + Duration::hours(10),
        &on(TICKET),
    )
    .await;
    let gone = block_at(
        &pool,
        day + Duration::hours(11),
        day + Duration::hours(12),
        &labelled(LABEL),
    )
    .await;

    time::day::remove(&pool, gone).await.expect("it deletes");

    let listed = time::day::list(&pool, day, day + Duration::days(1))
        .await
        .unwrap();
    assert_eq!(
        listed.iter().map(|d| d.block.id).collect::<Vec<_>>(),
        vec![kept],
        "delete took the wrong block, or none"
    );
}

/// A block that is not there is `not_found`, not a silent success -- the day
/// review may be looking at a day another window has already edited.
#[tokio::test]
async fn editing_or_deleting_a_block_that_is_not_there_says_so() {
    let pool = scratch("time-day-missing").await;
    let at = Utc.with_ymd_and_hms(2026, 9, 3, 9, 0, 0).unwrap();

    let refusal = time::day::update(&pool, 4242, at, at + Duration::hours(1), on(TICKET))
        .await
        .expect_err("there is no block 4242");
    assert_eq!(refusal.code, IpcErrorCode::NotFound);

    let refusal = time::day::remove(&pool, 4242)
        .await
        .expect_err("there is no block 4242");
    assert_eq!(refusal.code, IpcErrorCode::NotFound);
}

// -- the day review: a logged block is read-only -----------------------------

/// **Story 20**, in both directions and on both writers.
///
/// The `worklog_id` is set directly, because that is the only way to reach
/// this rule until #280 brings the table that writes it -- migration `0013`
/// put the column here so the rule did not have to wait. What is asserted is
/// what the view shows: the code the shell branches on, the sentence it
/// prints, and the block still being exactly as it was afterwards.
#[tokio::test]
async fn a_logged_block_refuses_both_edits_and_says_why() {
    let pool = scratch("time-day-logged").await;
    let day = Utc.with_ymd_and_hms(2026, 9, 3, 0, 0, 0).unwrap();
    let id = block_at(
        &pool,
        day + Duration::hours(9),
        day + Duration::hours(10),
        &on(TICKET),
    )
    .await;
    let worklog = logged_into(&pool, id).await;

    let refusal = time::day::update(
        &pool,
        id,
        day + Duration::hours(8),
        day + Duration::hours(12),
        labelled(LABEL),
    )
    .await
    .expect_err("a logged block is read-only");
    assert_eq!(refusal.code, IpcErrorCode::Invalid);
    assert!(
        refusal.message.contains("worklog") && refusal.message.contains("read-only"),
        "the reader has to be told the block is logged, not merely that \
         nothing happened: {}",
        refusal.message
    );

    let refusal = time::day::remove(&pool, id)
        .await
        .expect_err("a logged block cannot be deleted either");
    assert_eq!(refusal.code, IpcErrorCode::Invalid);
    assert!(refusal.message.contains("worklog"));

    let listed = time::day::list(&pool, day, day + Duration::days(1))
        .await
        .unwrap();
    assert_eq!(
        listed
            .iter()
            .map(|d| (d.block.started_at, d.block.ended_at, d.block.worklog_id))
            .collect::<Vec<_>>(),
        vec![(
            day + Duration::hours(9),
            day + Duration::hours(10),
            Some(worklog)
        )],
        "the refused edits changed the block anyway, so what knobas shows now \
         disagrees with what the worklog holds"
    );
}

/// ...and the direction that shows the rule is not simply refusing every
/// write: the same two commands on the same day's *unlogged* block go through.
#[tokio::test]
async fn an_unlogged_block_beside_a_logged_one_is_still_editable() {
    let pool = scratch("time-day-unlogged").await;
    let day = Utc.with_ymd_and_hms(2026, 9, 3, 0, 0, 0).unwrap();
    let locked = block_at(
        &pool,
        day + Duration::hours(9),
        day + Duration::hours(10),
        &on(TICKET),
    )
    .await;
    logged_into(&pool, locked).await;
    let free = block_at(
        &pool,
        day + Duration::hours(11),
        day + Duration::hours(12),
        &labelled(LABEL),
    )
    .await;

    time::day::update(
        &pool,
        free,
        day + Duration::hours(11),
        day + Duration::hours(13),
        labelled(LABEL),
    )
    .await
    .expect("an unlogged block is editable");
    time::day::remove(&pool, free)
        .await
        .expect("an unlogged block is deletable");
}

/// The boundary the overlap rule turns on, in both directions.
///
/// A block that ends **exactly** at a day's first instant does not overlap
/// that day at all -- it is the previous evening's, and it stops where the day
/// begins. Returning it puts a zero-width sliver at the head of the strip and
/// the day review then draws the whole night before the first real block as
/// unaccounted time.
///
/// A block that *starts* exactly there is the opposite: it belongs to this day
/// even when it has no length, because a block of no length is still a block
/// (the relaunch sweep writes one for a timer that died before its first
/// heartbeat). So the two cases are asserted together -- a predicate that
/// admitted both, or refused both, fails here.
#[tokio::test]
async fn a_block_ending_at_midnight_belongs_to_the_day_it_ran_in() {
    let pool = scratch("time-day-boundary").await;
    let midnight = Utc.with_ymd_and_hms(2026, 9, 3, 0, 0, 0).unwrap();

    let evening = block_at(&pool, midnight - Duration::hours(2), midnight, &on(TICKET)).await;
    let sliver = block_at(&pool, midnight, midnight, &labelled(LABEL)).await;

    let third = time::day::list(&pool, midnight, midnight + Duration::days(1))
        .await
        .expect("the 3rd is readable");
    assert_eq!(
        third.iter().map(|d| d.block.id).collect::<Vec<_>>(),
        vec![sliver],
        "a block that stops where the day starts is the day before's, and a \
         block of no length that starts here is this day's"
    );

    let second = time::day::list(&pool, midnight - Duration::days(1), midnight)
        .await
        .expect("the 2nd is readable");
    assert_eq!(
        second.iter().map(|d| d.block.id).collect::<Vec<_>>(),
        vec![evening],
        "the evening's block has to be on the evening's day, or it is on no \
         day at all"
    );
}

// -- passive attribution ----------------------------------------------------
//
// The seam is the same one the rest of this file uses: beats in through the
// store, blocks out through `time::day::list`, and the only SQL is fixture SQL
// that writes a **past** -- the heartbeat command can only ever record `now()`,
// and a day with a morning of beats in it cannot be built by waiting.
//
// The arithmetic these tests assert on is `time::passive::derive`'s, which has
// its own unit tests without a database. What is witnessed here is that the
// derivation is reached at all, that the setting gates it, and that what comes
// out of it is a row the day review can draw and a person can assign.

/// Insert `count` observations, `every` apart, starting at `from`.
///
/// `None` is *the reader had nothing in front of them*, which is a legal
/// observation. Fixture SQL, like `age` and `block_at` above.
async fn beats(
    pool: &PgPool,
    target: Option<&TimerTarget>,
    from: DateTime<Utc>,
    count: i64,
    every: Duration,
) {
    let (entity_id, label) = match target {
        Some(TimerTarget::Entity { entity_id }) => (Some(entity_id.as_str()), None),
        Some(TimerTarget::Label { label }) => (None, Some(label.as_str())),
        None => (None, None),
    };
    for step in 0..count {
        sqlx::query("insert into knobas.heartbeat (at, entity_id, label) values ($1, $2, $3)")
            .bind(from + every * i32::try_from(step).expect("a test fixture is small"))
            .bind(entity_id)
            .bind(label)
            .execute(pool)
            .await
            .expect("an observation in the past");
    }
}

/// How many observations knobas has kept.
async fn observations(pool: &PgPool) -> i64 {
    sqlx::query_scalar("select count(*) from knobas.heartbeat")
        .fetch_one(pool)
        .await
        .expect("the observations are countable")
}

/// The day's blocks, as `(kind, target, minutes)`, in the order the strip
/// draws them.
async fn day(pool: &PgPool, on_day: DateTime<Utc>) -> Vec<(BlockKind, TimerTarget, i64)> {
    time::day::list(pool, on_day, on_day + Duration::days(1))
        .await
        .expect("the day is readable")
        .into_iter()
        .map(|entry| {
            (
                entry.block.kind,
                entry.block.target,
                (entry.block.ended_at - entry.block.started_at).num_seconds(),
            )
        })
        .collect()
}

const OTHER: &str = "jira:PAY-99";

/// **The off state, and both halves of it.** Passive attribution is opt-in, so
/// a profile nobody has switched it on in records nothing at all and offers no
/// blocks -- and the heartbeat goes on doing the job it did before #282, which
/// is the half that would otherwise be lost silently.
///
/// The stamp is asserted as a *movement*, from a `last_heartbeat` an hour in
/// the past: "the timer still has a stamp" would pass against a heartbeat that
/// stopped writing one altogether.
#[tokio::test]
async fn with_passive_attribution_off_the_beat_still_stamps_and_nothing_is_recorded() {
    let pool = scratch("time-passive-off").await;
    assert!(
        !time::passive::enabled(&pool).await.unwrap(),
        "passive attribution is off until somebody switches it on"
    );

    time::start(&pool, on(TICKET)).await.expect("it starts");
    age(&pool, Duration::hours(2), Duration::hours(1)).await;
    let stale = time::current(&pool).await.unwrap().unwrap().last_heartbeat;

    let beaten = time::heartbeat(&pool, Some(on(OTHER)))
        .await
        .expect("a beat with the setting off is still a beat")
        .expect("the timer is running");

    assert!(
        beaten.last_heartbeat > stale + Duration::minutes(50),
        "the beat did not move `last_heartbeat`, so a relaunch would close \
         this block an hour early: {stale} -> {}",
        beaten.last_heartbeat
    );
    assert_eq!(
        observations(&pool).await,
        0,
        "off means knobas records nothing, not that it records and declines \
         to look"
    );
}

/// Switched on, a beat is an observation -- and a foreground the timer could
/// never run on costs the attribution rather than the row: the beat happened
/// and the window was focused, and a hole in the timeline is what the focused
/// budget would be measured wrong from.
#[tokio::test]
async fn switched_on_every_beat_is_an_observation_even_a_malformed_one() {
    let pool = scratch("time-passive-record").await;
    assert!(time::passive::set_enabled(&pool, true).await.unwrap());

    time::heartbeat(&pool, Some(on(TICKET))).await.unwrap();
    time::heartbeat(&pool, None).await.unwrap();
    let context = knobas_core::context::create_adhoc(&pool, "SEPA migration")
        .await
        .expect("a stored context");
    time::heartbeat(&pool, Some(on(&context.id))).await.unwrap();

    assert_eq!(observations(&pool).await, 3);
    let attributed: i64 =
        sqlx::query_scalar("select count(*) from knobas.heartbeat where entity_id = $1")
            .bind(TICKET)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(attributed, 1, "one of those three was on the ticket");
    let unattributed: i64 = sqlx::query_scalar(
        "select count(*) from knobas.heartbeat where entity_id is null and label is null",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        unattributed, 2,
        "an empty foreground and a foreground that is a stored context are \
         both observations with nothing to attribute"
    );
}

/// The floor and the merge, through the day read.
///
/// Twenty-one beats on one ticket, then ninety seconds on another, then
/// twenty-one more on the first. What the strip gets is **two** blocks, not
/// forty-two and not one: the merge makes each run a stretch, the floor drops
/// the ninety-second glance, and the interruption keeps the two runs apart.
#[tokio::test]
async fn the_day_read_offers_the_blocks_the_beats_support() {
    let pool = scratch("time-passive-day").await;
    time::passive::set_enabled(&pool, true).await.unwrap();
    let midnight = Utc.with_ymd_and_hms(2026, 9, 3, 0, 0, 0).unwrap();
    let at = |h, m, s| midnight + Duration::hours(h) + Duration::minutes(m) + Duration::seconds(s);

    beats(
        &pool,
        Some(&on(TICKET)),
        at(9, 0, 0),
        21,
        Duration::seconds(30),
    )
    .await;
    beats(
        &pool,
        Some(&on(OTHER)),
        at(9, 10, 30),
        3,
        Duration::seconds(30),
    )
    .await;
    beats(
        &pool,
        Some(&on(TICKET)),
        at(9, 12, 0),
        21,
        Duration::seconds(30),
    )
    .await;

    assert_eq!(
        day(&pool, midnight).await,
        vec![
            (BlockKind::Passive, on(TICKET), 630),
            (BlockKind::Passive, on(TICKET), 630),
        ],
        "a ninety-second glance at another ticket is not a block, and the two \
         runs either side of it are not one"
    );

    // Reading the day again says the same thing, with the same rows: the
    // reconciliation is idempotent, and an id the strip drew is an id
    // *Assign…* can still be sent.
    let first: Vec<i64> = time::day::list(&pool, midnight, midnight + Duration::days(1))
        .await
        .unwrap()
        .iter()
        .map(|entry| entry.block.id)
        .collect();
    let again: Vec<i64> = time::day::list(&pool, midnight, midnight + Duration::days(1))
        .await
        .unwrap()
        .iter()
        .map(|entry| entry.block.id)
        .collect();
    assert_eq!(
        first, again,
        "a second read rewrote the day's passive blocks"
    );
}

/// The cap, through the day read, on the numbers that separate it from the
/// merge: beats arriving six times as often as they are sent.
///
/// Five minutes of focused time, claims that add up to five and a half, and a
/// block of exactly five. Without the cap the merge still gives **one** block,
/// of 330 seconds -- so this fails on its arithmetic rather than its length,
/// which is what tells "capped" from "merged".
#[tokio::test]
async fn the_cap_binds_through_the_day_read() {
    let pool = scratch("time-passive-cap").await;
    time::passive::set_enabled(&pool, true).await.unwrap();
    let midnight = Utc.with_ymd_and_hms(2026, 9, 3, 0, 0, 0).unwrap();

    beats(
        &pool,
        Some(&on(TICKET)),
        midnight + Duration::hours(9),
        61,
        Duration::seconds(5),
    )
    .await;

    assert_eq!(
        day(&pool, midnight).await,
        vec![(BlockKind::Passive, on(TICKET), 300)],
        "sixty-one beats five seconds apart are five minutes of focused time"
    );
}

/// **Assigning a passive block makes it manual** -- the one direction of the
/// kind write there is a way to ask for.
///
/// And the second half, which is the reason the write is there at all: the
/// next day read does not grow a passive twin over the stretch. The beats that
/// produced the block are still in the table and still derive the same span;
/// what stops it coming back is that a block the person owns now covers it.
#[tokio::test]
async fn assigning_a_passive_block_makes_it_manual_and_it_stays_assigned() {
    let pool = scratch("time-passive-assign").await;
    time::passive::set_enabled(&pool, true).await.unwrap();
    let midnight = Utc.with_ymd_and_hms(2026, 9, 3, 0, 0, 0).unwrap();
    beats(
        &pool,
        Some(&on(TICKET)),
        midnight + Duration::hours(9),
        21,
        Duration::seconds(30),
    )
    .await;

    let offered = time::day::list(&pool, midnight, midnight + Duration::days(1))
        .await
        .unwrap();
    let [offered] = offered.as_slice() else {
        panic!("one passive block was offered, not {}", offered.len())
    };
    assert_eq!(offered.block.kind, BlockKind::Passive);

    let assigned = time::day::update(
        &pool,
        offered.block.id,
        offered.block.started_at,
        offered.block.ended_at,
        labelled(LABEL),
    )
    .await
    .expect("a passive block can be assigned");
    assert_eq!(
        assigned.block.kind,
        BlockKind::Manual,
        "a block a person has named is theirs, not knobas' guess"
    );
    assert_eq!(
        assigned.block.id, offered.block.id,
        "assigning wrote a new row"
    );

    assert_eq!(
        day(&pool, midnight).await,
        vec![(BlockKind::Manual, labelled(LABEL), 600)],
        "the beats still derive this stretch, and the next read offered it \
         again beside the block it had already become"
    );
}

/// The other direction of the kind write, which is not a thing a caller can
/// ask for: editing a manual block leaves it manual.
///
/// It is witnessed as a *round trip through the one command that writes the
/// column* rather than as an absence, because the failure it stands against is
/// an `update` that copied whatever kind it found -- which would turn every
/// edit of an assigned block back into a passive one on the next read.
#[tokio::test]
async fn a_manual_block_is_never_turned_passive() {
    let pool = scratch("time-passive-direction").await;
    let midnight = Utc.with_ymd_and_hms(2026, 9, 3, 0, 0, 0).unwrap();
    let at = |h| midnight + Duration::hours(h);
    let id = block_at(&pool, at(9), at(10), &on(TICKET)).await;

    let edited = time::day::update(&pool, id, at(9), at(11), on(TICKET))
        .await
        .expect("a manual block is editable");

    assert_eq!(edited.block.kind, BlockKind::Manual);
}

/// *Assign…* on a gap: a manual block spanning it, and nothing else about the
/// day changes.
#[tokio::test]
async fn assigning_a_gap_writes_a_manual_block_over_it() {
    let pool = scratch("time-passive-gap").await;
    let midnight = Utc.with_ymd_and_hms(2026, 9, 3, 0, 0, 0).unwrap();
    let at = |h, m| midnight + Duration::hours(h) + Duration::minutes(m);
    block_at(&pool, at(9, 0), at(10, 0), &on(TICKET)).await;
    block_at(&pool, at(11, 0), at(12, 0), &on(TICKET)).await;

    let made = time::day::create(&pool, at(10, 0), at(11, 0), labelled(LABEL))
        .await
        .expect("an unaccounted hour can be claimed");
    assert_eq!(made.block.kind, BlockKind::Manual);
    assert_eq!(made.block.target, labelled(LABEL));

    assert_eq!(
        day(&pool, midnight).await,
        vec![
            (BlockKind::Manual, on(TICKET), 3600),
            (BlockKind::Manual, labelled(LABEL), 3600),
            (BlockKind::Manual, on(TICKET), 3600),
        ],
        "the gap is gone and the blocks either side of it are untouched"
    );

    let refusal = time::day::create(&pool, at(11, 0), at(10, 0), labelled(LABEL))
        .await
        .expect_err("a block cannot end before it starts");
    assert_eq!(refusal.code, IpcErrorCode::Invalid);
}

/// Passive attribution never draws over time a block already claims.
///
/// The timer ran through the whole of these beats, so the derivation has
/// something to say about the stretch and no business saying it: where they
/// overlap the block wins and the span is dropped whole, the direction that
/// can only lose a suggestion and never invent one.
#[tokio::test]
async fn a_stretch_a_block_already_covers_is_not_offered_passively() {
    let pool = scratch("time-passive-covered").await;
    time::passive::set_enabled(&pool, true).await.unwrap();
    let midnight = Utc.with_ymd_and_hms(2026, 9, 3, 0, 0, 0).unwrap();
    let at = |h, m| midnight + Duration::hours(h) + Duration::minutes(m);
    block_at(&pool, at(9, 0), at(9, 30), &labelled(LABEL)).await;

    beats(
        &pool,
        Some(&on(TICKET)),
        at(9, 5),
        21,
        Duration::seconds(30),
    )
    .await;
    beats(
        &pool,
        Some(&on(TICKET)),
        at(10, 0),
        21,
        Duration::seconds(30),
    )
    .await;

    assert_eq!(
        day(&pool, midnight).await,
        vec![
            (BlockKind::Manual, labelled(LABEL), 1800),
            (BlockKind::Passive, on(TICKET), 600),
        ],
        "the morning's beats fell inside a block the person owns, and the ten \
         o'clock ones did not"
    );
}

/// Switching it off stops the recording and the derivation; it does not take
/// back what has already been offered.
///
/// A passive block on a day nobody has reviewed yet is knobas' answer to "what
/// was I doing", and nothing passive has ever reached a source, so there is
/// nothing to withdraw.
#[tokio::test]
async fn switching_it_off_leaves_the_blocks_already_offered_alone() {
    let pool = scratch("time-passive-off-later").await;
    time::passive::set_enabled(&pool, true).await.unwrap();
    let midnight = Utc.with_ymd_and_hms(2026, 9, 3, 0, 0, 0).unwrap();
    beats(
        &pool,
        Some(&on(TICKET)),
        midnight + Duration::hours(9),
        21,
        Duration::seconds(30),
    )
    .await;
    assert_eq!(day(&pool, midnight).await.len(), 1, "a block was offered");

    assert!(!time::passive::set_enabled(&pool, false).await.unwrap());

    assert_eq!(
        day(&pool, midnight).await,
        vec![(BlockKind::Passive, on(TICKET), 600)],
        "switching the setting off deleted a block the reader had not answered yet"
    );
}

/// A day the beats say nothing about is a day this reconciliation says nothing
/// about -- every day before #282 existed is such a day, and one that spoke
/// would delete passive blocks it has no evidence either way for.
#[tokio::test]
async fn a_day_with_no_observations_is_left_exactly_as_it_was() {
    let pool = scratch("time-passive-silent-day").await;
    time::passive::set_enabled(&pool, true).await.unwrap();
    let midnight = Utc.with_ymd_and_hms(2026, 9, 3, 0, 0, 0).unwrap();
    let at = |h| midnight + Duration::hours(h);
    sqlx::query(
        "insert into knobas.block (started_at, ended_at, entity_id, kind)
         values ($1, $2, $3, 'passive')",
    )
    .bind(at(9))
    .bind(at(10))
    .bind(TICKET)
    .execute(&pool)
    .await
    .expect("a passive block from a knobas that still had the beats");

    assert_eq!(
        day(&pool, midnight).await,
        vec![(BlockKind::Passive, on(TICKET), 3600)],
        "a day with no observations had its passive blocks reconciled away"
    );
}

/// A passive block the day no longer supports is **taken back**, and the
/// commonest way that happens is a person claiming the stretch themselves.
///
/// This is the half of the reconciliation the other tests do not reach: they
/// witness rows being offered and rows being left alone, and this witnesses one
/// being forgotten. Without it a `materialize` that only ever inserted would
/// leave the reader looking at knobas' guess underneath the block they had just
/// written over it.
///
/// It is also where the overlap rule is witnessed at its **sharpest edge**:
/// the block the reader writes covers only half of the passive one, and the
/// passive one goes **whole**. That is the documented direction -- a
/// suggestion can be lost, never invented -- and it is the shape a reviewer
/// should argue with if they are going to argue with anything here.
#[tokio::test]
async fn a_passive_block_a_new_manual_one_overlaps_is_taken_back_whole() {
    let pool = scratch("time-passive-forget").await;
    time::passive::set_enabled(&pool, true).await.unwrap();
    let midnight = Utc.with_ymd_and_hms(2026, 9, 3, 0, 0, 0).unwrap();
    let at = |h, m| midnight + Duration::hours(h) + Duration::minutes(m);
    beats(
        &pool,
        Some(&on(TICKET)),
        at(9, 0),
        21,
        Duration::seconds(30),
    )
    .await;
    assert_eq!(
        day(&pool, midnight).await,
        vec![(BlockKind::Passive, on(TICKET), 600)],
        "a block was offered to take back"
    );

    // The reader says what the morning was, over part of the same stretch.
    time::day::create(&pool, at(9, 5), at(9, 40), labelled(LABEL))
        .await
        .expect("an unaccounted stretch can be claimed");

    assert_eq!(
        day(&pool, midnight).await,
        vec![(BlockKind::Manual, labelled(LABEL), 2100)],
        "the passive block knobas had guessed is still there underneath the \
         one the reader wrote over it"
    );
// -- the worklog draft (#280) -----------------------------------------------
//
// The draft's *reads*, against a real database: which blocks a day's interval
// is made of, what the candidates are, and what the comment says. The other
// half of the worklog -- queueing the write, the local copy, Jira's id landing
// on it -- needs a source that answers, and lives in `tests/worklog_ipc.rs`
// behind a trait-level fake (the layer ADR-0013 keeps).

/// The reader's day, and the offset their machine sends with it. UTC here, so
/// the fixtures below read as the instants they are.
const DAY: &str = "2026-09-03";

fn day() -> chrono::NaiveDate {
    DAY.parse().expect("a date")
}

/// The Jira source every draft below is drawn against.
///
/// A configuration row is what makes `jira:` a namespace that takes worklogs:
/// `worklog::draft` asks the **adapter** whether it declares `log_work`, and
/// the row is the hop from the instance id to the adapter kind.
async fn configure_jira(pool: &PgPool, username: &str) {
    knobas_sync::config::insert(
        pool,
        &knobas_sync::config::InsertConfig {
            id: "jira".to_owned(),
            adapter_kind: "jira".to_owned(),
            display_name: "Tidewater Jira".to_owned(),
            base_url: "https://jira.example".to_owned(),
            auth_kind: knobas_sync::config::AuthKind::Method(knobas_source::AuthMethod::Pat),
            config: serde_json::json!({ "username": username }),
            sync_interval_secs: 86_400,
            enabled: true,
        },
    )
    .await
    .expect("the source row is written");
}

/// One block on `entity`, from `from` to `to` on [`DAY`].
async fn block(pool: &PgPool, entity: &str, from: (u32, u32), to: (u32, u32)) -> i64 {
    let at = |(h, m): (u32, u32)| {
        use chrono::TimeZone;
        Utc.with_ymd_and_hms(2026, 9, 3, h, m, 0).unwrap()
    };
    sqlx::query_scalar::<_, i64>(
        "insert into knobas.block (started_at, ended_at, entity_id, kind)
         values ($1, $2, $3, 'manual') returning id",
    )
    .bind(at(from))
    .bind(at(to))
    .bind(entity)
    .fetch_one(pool)
    .await
    .expect("a block is written")
}

/// A mirrored item **authored by `author`**, updated at `at`.
///
/// Not the day review's `mirrored` above: that one writes a title for a block
/// to read back, and the two fields this one exists for -- who wrote it and
/// when -- are exactly what the candidate list narrows on.
async fn authored(pool: &PgPool, id: &str, title: &str, author: &str, at: DateTime<Utc>) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'commit', $2)")
        .bind(id)
        .bind(title)
        .execute(pool)
        .await
        .expect("the entity row");
    sqlx::query(
        "insert into sync.item
           (entity_id, source_id, kind, title, body_text, author, item_updated_at, payload)
         values ($1, 'jira', 'commit', $2, '', $3, $4, '{}'::jsonb)",
    )
    .bind(id)
    .bind(title)
    .bind(author)
    .bind(at)
    .execute(pool)
    .await
    .expect("the mirror row");
}

/// One activity line of the reader's own, **at a moment this test dictates**.
///
/// `activity::record` stamps `now()`, which would put the line inside or
/// outside a fixture interval depending on what time of day the suite runs --
/// a test that passes over lunch and fails after it. The stamp is moved with
/// one `update`, which writes a fixture and never an assertion, the discipline
/// this file's `age` records for the timer.
async fn line(pool: &PgPool, verb: &str, at: DateTime<Utc>) {
    let ticket = knobas_core::entity::EntityRef::parse(TICKET).expect("an entity id");
    let row =
        knobas_core::activity::record(pool, "user", verb, Some(&ticket), serde_json::json!({}))
            .await
            .expect("an activity line");
    sqlx::query("update knobas.activity set at = $1 where id = $2")
        .bind(at)
        .bind(row.id)
        .execute(pool)
        .await
        .expect("the line is moved into the interval");
}

fn registry() -> knobas_app::sources::Registry {
    knobas_app::sources::Registry::builtin()
}

async fn draft_of(pool: &PgPool, entity: &str) -> Option<knobas_app::time::worklog::Draft> {
    time::worklog::draft(pool, &registry(), entity, day(), 0)
        .await
        .expect("the draft is readable")
}

/// **The one number that can be wrong invisibly**, against a real database: an
/// afternoon with a lunch in it.
///
/// 09:00--10:30 and 13:00--14:00 is two and a half hours of work inside a five
/// hour window, and the draft logs the first. The unit test beside
/// `concatenate` pins the arithmetic; this pins that the *read* feeding it
/// picks up both blocks and no others.
#[tokio::test]
async fn a_days_blocks_concatenate_into_one_interval_that_bills_no_lunch() {
    let pool = scratch("worklog-interval").await;
    configure_jira(&pool, "mara.lindqvist").await;
    let morning = block(&pool, TICKET, (9, 0), (10, 30)).await;
    let afternoon = block(&pool, TICKET, (13, 0), (14, 0)).await;

    let draft = draft_of(&pool, TICKET).await.expect("there is time to log");
    assert_eq!(draft.started_at, at(9, 0));
    assert_eq!(draft.ended_at, at(14, 0));
    assert_eq!(draft.seconds, 150 * 60, "two and a half hours were worked");
    assert_eq!(draft.block_ids, vec![morning, afternoon]);
}

/// Another ticket's afternoon is not this ticket's worklog.
///
/// The mutation this exists for is a `where` clause that lost its entity: the
/// draft would then read as a longer day, silently, and log somebody else's
/// hours against PAY-231.
#[tokio::test]
async fn another_tickets_blocks_are_not_in_this_tickets_interval() {
    let pool = scratch("worklog-other-ticket").await;
    configure_jira(&pool, "mara.lindqvist").await;
    let mine = block(&pool, TICKET, (9, 0), (10, 0)).await;
    block(&pool, "jira:PAY-240", (10, 0), (12, 0)).await;
    // ...and an ad-hoc label's block, which has no ticket at all.
    sqlx::query(
        "insert into knobas.block (started_at, ended_at, label, kind)
         values ($1, $2, $3, 'manual')",
    )
    .bind(at(13, 0))
    .bind(at(15, 0))
    .bind(LABEL)
    .execute(&pool)
    .await
    .expect("a labelled block");

    let draft = draft_of(&pool, TICKET).await.expect("there is time to log");
    assert_eq!(draft.block_ids, vec![mine], "{:?}", draft.block_ids);
    assert_eq!(draft.seconds, 60 * 60);
}

/// Yesterday's blocks are not today's draft, and the day is **the reader's**.
#[tokio::test]
async fn a_day_is_the_readers_day() {
    let pool = scratch("worklog-day").await;
    configure_jira(&pool, "mara.lindqvist").await;
    let today = block(&pool, TICKET, (9, 0), (10, 0)).await;
    sqlx::query(
        "insert into knobas.block (started_at, ended_at, entity_id, kind)
         values ($1, $2, $3, 'manual')",
    )
    .bind(at(9, 0) - Duration::days(1))
    .bind(at(10, 0) - Duration::days(1))
    .bind(TICKET)
    .execute(&pool)
    .await
    .expect("yesterday's block");

    let draft = draft_of(&pool, TICKET).await.expect("there is time to log");
    assert_eq!(draft.block_ids, vec![today]);

    // The same blocks, read by somebody four hours east: 09:00 UTC is 13:00 to
    // them, still today -- but a block at 22:00 UTC is tomorrow, and the
    // server's own date would have said otherwise.
    let evening = block(&pool, TICKET, (22, 0), (23, 0)).await;
    let theirs = time::worklog::draft(&pool, &registry(), TICKET, day(), 240)
        .await
        .expect("the draft is readable")
        .expect("there is time to log");
    assert!(
        !theirs.block_ids.contains(&evening),
        "22:00 UTC is tomorrow for a reader at +04:00, and their day must not \
         carry it: {:?}",
        theirs.block_ids
    );
}

/// A source that does not declare `log_work` gets no draft at all -- which is
/// how a stop on a Gitea commit or a note passes without an apology.
#[tokio::test]
async fn a_source_that_takes_no_worklogs_has_no_draft() {
    let pool = scratch("worklog-no-source").await;
    configure_jira(&pool, "mara.lindqvist").await;
    block(&pool, "gitea:tidewater/payout-service#142", (9, 0), (10, 0)).await;
    block(&pool, "note:5b1c0f1e", (10, 0), (11, 0)).await;

    for id in ["gitea:tidewater/payout-service#142", "note:5b1c0f1e"] {
        assert!(
            draft_of(&pool, id).await.is_none(),
            "{id} is not somewhere a worklog can go, so there is nothing to draft"
        );
    }
    // ...and the direction that shows the refusal is not simply refusing
    // everything.
    block(&pool, TICKET, (11, 0), (12, 0)).await;
    assert!(draft_of(&pool, TICKET).await.is_some());
}

/// A ticket with nothing left to log has no draft either -- the same `null`,
/// for the reason the command's own documentation gives: the shell asks on
/// every stop and opens the draft only if it got one.
#[tokio::test]
async fn a_ticket_with_no_unlogged_time_has_no_draft() {
    let pool = scratch("worklog-nothing").await;
    configure_jira(&pool, "mara.lindqvist").await;
    assert!(draft_of(&pool, TICKET).await.is_none());
}

/// **A passive block is not something to log.**
///
/// `0013`'s block vocabulary has two kinds and #282 writes the second one:
/// `passive` is knobas' guess at what was open on screen, not a person's
/// account of an afternoon. Drafting one would put minutes nobody vouched for
/// into a worklog that bills a client, so the draft narrows to `manual` -- and
/// so does `log`, which re-derives its covered blocks from the same read.
///
/// The row is inserted directly rather than through the timer, because the
/// timer only makes `manual` ones: #282's derivation is the writer of the
/// other kind and it is not here yet. That is the point -- the guard has to be
/// in place before its writer arrives, or the first passive afternoon knobas
/// records is one it silently offers to bill.
#[tokio::test]
async fn a_passive_block_is_not_drafted() {
    let pool = scratch("worklog-passive").await;
    configure_jira(&pool, "mara.lindqvist").await;
    sqlx::query(
        "insert into knobas.block (started_at, ended_at, entity_id, kind)
         values ($1, $2, $3, 'passive')",
    )
    .bind(Utc.with_ymd_and_hms(2026, 9, 3, 9, 0, 0).unwrap())
    .bind(Utc.with_ymd_and_hms(2026, 9, 3, 10, 30, 0).unwrap())
    .bind(TICKET)
    .execute(&pool)
    .await
    .expect("a passive block is written");

    assert!(
        draft_of(&pool, TICKET).await.is_none(),
        "the day's only block was passive -- knobas guessed at it, and a guess \
         is not an afternoon anybody has claimed"
    );

    // ...and the direction that shows the narrowing is `kind` and not the
    // fixture failing to insert: a manual block beside it *is* drafted, and
    // the interval is that block alone.
    let claimed = block(&pool, TICKET, (11, 0), (12, 0)).await;
    let drafted = draft_of(&pool, TICKET)
        .await
        .expect("the manual block is something to draft");
    assert_eq!(
        drafted.block_ids,
        vec![claimed],
        "the passive block was drafted alongside the manual one"
    );
    assert_eq!(
        drafted.seconds, 3_600,
        "the passive block's ninety minutes are in the interval"
    );
}

/// The candidates: what the mirror says the reader did in the interval, and
/// what their own activity lines say -- and **one bullet each**.
///
/// Four fixtures, and each is a direction the read can be wrong in:
///
/// * a commit of theirs inside the window is offered;
/// * a colleague's commit, in the same window, is not -- "authored by me" is
///   the whole of what this list means;
/// * a commit of theirs *outside* the window is not, so the interval is doing
///   the narrowing rather than the day;
/// * their own activity line is offered, but the timer's own `stopped` is not:
///   a worklog comment that says "stopped the timer" is knobas talking about
///   itself.
#[tokio::test]
async fn the_candidates_are_the_readers_own_work_inside_the_interval() {
    let pool = scratch("worklog-candidates").await;
    configure_jira(&pool, "mara.lindqvist").await;
    block(&pool, TICKET, (9, 0), (11, 0)).await;

    authored(
        &pool,
        "jira:c1",
        "Retry SEPA payouts",
        "mara.lindqvist",
        at(9, 30),
    )
    .await;
    authored(
        &pool,
        "jira:c2",
        "Someone else's work",
        "jonas.k",
        at(9, 40),
    )
    .await;
    authored(
        &pool,
        "jira:c3",
        "Yesterday's commit",
        "mara.lindqvist",
        at(9, 30) - Duration::days(1),
    )
    .await;

    line(&pool, "linked", at(9, 30)).await;
    line(&pool, "stopped", at(9, 40)).await;

    let draft = draft_of(&pool, TICKET).await.expect("there is time to log");
    let ids: Vec<&str> = draft.candidates.iter().map(|c| c.id.as_str()).collect();
    assert!(
        ids.contains(&"item:jira:c1"),
        "the reader's own commit is a candidate: {ids:?}"
    );
    assert!(
        !ids.contains(&"item:jira:c2"),
        "somebody else's commit is not the reader's afternoon: {ids:?}"
    );
    assert!(
        !ids.contains(&"item:jira:c3"),
        "a commit outside the interval is not in it: {ids:?}"
    );
    assert!(
        draft
            .candidates
            .iter()
            .any(|c| c.bullet == "- linked PAY-231"),
        "the reader's own activity line is a candidate: {:?}",
        draft.candidates
    );
    assert!(
        !draft
            .candidates
            .iter()
            .any(|c| c.bullet.contains("stopped")),
        "the timer's own bookkeeping is not work: {:?}",
        draft.candidates
    );

    // The comment the draft opens with is those bullets, in order, and
    // nothing else -- which is what the frontend re-joins when a box is
    // unticked.
    assert_eq!(
        draft.comment,
        draft
            .candidates
            .iter()
            .map(|c| c.bullet.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        draft.comment.contains("- Retry SEPA payouts"),
        "{}",
        draft.comment
    );
}
