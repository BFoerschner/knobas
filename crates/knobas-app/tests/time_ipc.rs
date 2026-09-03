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

/// Stamp a block as logged. **The only way to reach the read-only rule until
/// #280**: `knobas.worklog` does not exist yet and nothing writes this column,
/// so the alternative is a rule with no test at all until the worklog lands.
/// Migration `0013` put the column here for exactly this reason.
async fn logged_into(pool: &PgPool, block: i64, worklog: i64) {
    let rows = sqlx::query("update knobas.block set worklog_id = $2 where id = $1")
        .bind(block)
        .bind(worklog)
        .execute(pool)
        .await
        .expect("the block is stamped")
        .rows_affected();
    assert_eq!(rows, 1, "there was no block {block} to stamp");
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
    logged_into(&pool, id, 77).await;

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
            Some(77)
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
    logged_into(&pool, locked, 77).await;
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
