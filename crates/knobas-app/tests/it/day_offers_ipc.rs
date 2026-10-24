//! The Day tab's two backend halves (issues #900 and #901), over a real
//! PostgreSQL: *Dismiss* on an offered block, and the day's **open
//! stretches** -- focused time nothing claims.
//!
//! The seam is the one `time_ipc.rs` uses: beats in through
//! `time::passive::record_at`, the day out through `time::day::list`, writes
//! through `time::day`. A past is built through the store's own inserts; the
//! one SQL here moves a running timer into the past (`time_ipc.rs`'s `age`),
//! and it writes a fixture, never an assertion.

use chrono::{DateTime, Duration, SubsecRound, TimeZone, Utc};
use knobas_app::IpcErrorCode;
use knobas_app::time::week::DayWindow;
use knobas_app::time::{self, BlockKind, TimerTarget};
use sqlx::PgPool;

const TICKET: &str = "jira:PAY-231";

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

fn midnight() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 3, 0, 0, 0).unwrap()
}

fn at(hour: i64, minute: i64) -> DateTime<Utc> {
    midnight() + Duration::hours(hour) + Duration::minutes(minute)
}

/// `count` observations, thirty seconds apart, from `from`.
async fn beats(pool: &PgPool, target: Option<&TimerTarget>, from: DateTime<Utc>, count: i32) {
    for step in 0..count {
        time::passive::record_at(pool, from + Duration::seconds(30) * step, target)
            .await
            .expect("an observation in the past");
    }
}

async fn read(pool: &PgPool) -> time::day::DayRecord {
    time::day::list(pool, midnight(), midnight() + Duration::days(1))
        .await
        .expect("the day is readable")
}

/// The starts of the day's offers: a passive block's identity.
fn offered(record: &time::day::DayRecord) -> Vec<DateTime<Utc>> {
    record
        .blocks
        .iter()
        .filter(|entry| entry.block.kind == BlockKind::Passive)
        .map(|entry| entry.block.started_at)
        .collect()
}

fn open(record: &time::day::DayRecord) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    record
        .open
        .iter()
        .map(|stretch| (stretch.started_at, stretch.ended_at))
        .collect()
}

// -- Dismiss (#900) -----------------------------------------------------------

/// **A dismissed offer stays gone**, which `delete_block` never managed: the
/// day read re-derives passive blocks from the beats before it reads, so a
/// deleted offer came back on the next read. Dismissed, it does not, on the
/// next read or the one after -- and not when more beats arrive that grow the
/// same stretch, which is what today's last stretch does every thirty seconds.
#[tokio::test]
async fn a_dismissed_offer_is_gone_and_the_next_reads_do_not_offer_it_again() {
    let pool = scratch("day-dismiss").await;
    time::passive::set_enabled(&pool, true).await.unwrap();
    beats(&pool, Some(&on(TICKET)), at(9, 0), 21).await;
    beats(&pool, Some(&on(TICKET)), at(11, 0), 21).await;

    let first = read(&pool).await;
    assert_eq!(
        offered(&first),
        vec![at(9, 0), at(11, 0)],
        "the fixture offers two stretches"
    );
    let morning = first.blocks[0].block.id;

    time::passive::dismiss(&pool, morning)
        .await
        .expect("an offered block can be dismissed");
    assert_eq!(
        offered(&read(&pool).await),
        vec![at(11, 0)],
        "the dismissed stretch came back on the next read"
    );

    // The stretch goes on: more beats on the same ticket, unbroken.
    beats(
        &pool,
        Some(&on(TICKET)),
        at(9, 10) + Duration::seconds(30),
        20,
    )
    .await;
    let later = read(&pool).await;
    assert_eq!(
        offered(&later),
        vec![at(11, 0)],
        "the dismissed stretch was offered again once it grew"
    );
    // Dismissed time is not open time either: an open row offers *Assign…*,
    // which would be offering the stretch again.
    assert!(
        open(&later).iter().all(|(began, _)| *began >= at(11, 0)),
        "the dismissed morning reappeared as open time: {:?}",
        open(&later)
    );
}

/// **A dismissal in flight while the day is read still holds.** The read
/// reconciles the offers before it reads, and a reconciliation that decided
/// what to offer before the dismissal committed wrote the row straight back.
///
/// The dismissal is held open in a transaction of the test's own (the
/// statement `passive::dismiss` runs, written out, since the seam has no way
/// to pause it), the read starts beside it, and the dismissal commits once
/// the read waits on it.
#[tokio::test]
async fn a_dismissal_committing_beside_a_read_is_not_offered_back() {
    let pool = scratch("day-dismiss-race").await;
    time::passive::set_enabled(&pool, true).await.unwrap();
    beats(&pool, Some(&on(TICKET)), at(9, 0), 21).await;
    let morning = read(&pool).await.blocks[0].block.id;

    let mut dismissing = pool.begin().await.expect("a transaction");
    sqlx::query(
        "with gone as (
             delete from knobas.block where id = $1 and kind = 'passive'
             returning started_at, ended_at
         )
         insert into knobas.passive_dismissal (started_at, ended_at)
         select started_at, ended_at from gone",
    )
    .bind(morning)
    .execute(&mut *dismissing)
    .await
    .expect("the dismissal, not yet committed");

    let reader = tokio::spawn({
        let pool = pool.clone();
        async move { read(&pool).await }
    });
    // The read waits on the dismissal: the server lists its lock request on
    // the dismissals, not granted (#1927). Before, a 500 ms margin stood for
    // this, and a read slower to get there came after the commit and passed
    // without meeting the race.
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let waiting: i64 = sqlx::query_scalar(
                "select count(*) from pg_locks l join pg_class c on c.oid = l.relation
                  where c.relname = 'passive_dismissal' and not l.granted
                    and l.database = (select oid from pg_database
                                       where datname = current_database())",
            )
            .fetch_one(&mut *dismissing)
            .await
            .expect("the locks read");
            if waiting > 0 {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the read never waited on the dismissal");
    dismissing.commit().await.expect("the dismissal commits");

    let beside = reader.await.expect("the read finished");
    assert_eq!(
        offered(&beside),
        Vec::<DateTime<Utc>>::new(),
        "the read beside the dismissal offered the stretch back"
    );
    assert_eq!(
        offered(&read(&pool).await),
        Vec::<DateTime<Utc>>::new(),
        "the dismissed stretch came back on the read after"
    );
}

/// Only an offer can be dismissed. A block a person tracked is deleted, not
/// dismissed, and a block that is not there says so.
#[tokio::test]
async fn only_an_offered_block_can_be_dismissed() {
    let pool = scratch("day-dismiss-refused").await;
    let mine = time::day::create(&pool, at(9, 0), at(9, 30), on(TICKET))
        .await
        .expect("a block");

    let refused = time::passive::dismiss(&pool, mine.block.id)
        .await
        .expect_err("a manual block is not an offer");
    assert_eq!(refused.code, IpcErrorCode::Invalid);
    assert_eq!(
        read(&pool).await.blocks.len(),
        1,
        "the refused dismissal deleted the block anyway"
    );

    let missing = time::passive::dismiss(&pool, mine.block.id + 1000)
        .await
        .expect_err("there is no such block");
    assert_eq!(missing.code, IpcErrorCode::NotFound);
}

// -- Open stretches (#901) ----------------------------------------------------

/// **Focused time nothing claims, as intervals.** A morning with nothing in
/// front of the reader, a block they tracked in the middle of it, and a
/// stretch on a ticket: the open stretches are the focused time either side
/// of the block, and not the ticket's stretch, which is offered.
///
/// And they are the same measurement as the week's "no target, app open" row,
/// to the second.
#[tokio::test]
async fn the_days_open_stretches_are_focused_time_no_block_covers() {
    let pool = scratch("day-open").await;
    time::passive::set_enabled(&pool, true).await.unwrap();
    beats(&pool, None, at(9, 0), 21).await;
    time::day::create(&pool, at(9, 2), at(9, 4), on(TICKET))
        .await
        .expect("a block");
    beats(&pool, Some(&on(TICKET)), at(10, 0), 21).await;

    let record = read(&pool).await;
    assert_eq!(
        open(&record),
        // The last idle beat at 09:10 credits one window forward, because a
        // beat follows it (at 10:00): focused time is the gap to the next
        // beat, clamped to the window.
        vec![
            (at(9, 0), at(9, 2)),
            (at(9, 4), at(9, 10) + Duration::seconds(30)),
        ],
        "the open stretches are the focused morning minus the block"
    );

    let window = DayWindow {
        day: midnight().date_naive(),
        from: midnight(),
        to: midnight() + Duration::days(1),
    };
    let week = time::week::read(&pool, &[window]).await.expect("the week");
    let no_target = week
        .rows
        .iter()
        .find(|row| row.target.is_none())
        .expect("the week has a no-target row");
    assert_eq!(
        no_target.cells[0].tracked_seconds,
        2 * 60 + 6 * 60 + 30,
        "the week's open time and the day's open stretches disagree"
    );
}

/// **The running timer claims its time.** It is not a block until it stops,
/// so without this the Day tab drew an open stretch under the yellow one, and
/// the copy says *with no timer*.
#[tokio::test]
async fn the_running_timer_claims_its_time_from_the_open_stretches() {
    let pool = scratch("day-open-timer").await;
    time::passive::set_enabled(&pool, true).await.unwrap();
    // Whole seconds, so the instants round-trip through PostgreSQL unchanged.
    let now = Utc::now().trunc_subsecs(0);
    // Focused from ninety minutes ago to thirty minutes ago; the timer has
    // run for the last hour of it.
    beats(&pool, None, now - Duration::minutes(90), 121).await;
    time::start(&pool, on(TICKET), None).await.expect("a timer");
    sqlx::query("update knobas.timer set started_at = $1, last_heartbeat = now()")
        .bind(now - Duration::minutes(60))
        .execute(&pool)
        .await
        .expect("the timer is aged");

    let record = time::day::list(&pool, now - Duration::hours(3), now + Duration::minutes(1))
        .await
        .expect("the day is readable");
    assert_eq!(
        open(&record),
        vec![(now - Duration::minutes(90), now - Duration::minutes(60))],
        "the open stretch ran on under the running timer"
    );
}

/// Passive attribution off means nothing was observed, and nothing is open.
#[tokio::test]
async fn with_nothing_observed_nothing_is_open() {
    let pool = scratch("day-open-none").await;
    time::day::create(&pool, at(9, 0), at(9, 30), on(TICKET))
        .await
        .expect("a block");
    assert!(read(&pool).await.open.is_empty());
}
