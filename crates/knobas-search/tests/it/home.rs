//! The empty-query board, against a real PostgreSQL.
//!
//! Like the smart lists, the board is a whole-corpus read: "the newest twenty
//! items" is not something a per-test token can isolate, so the tests take
//! [`SERIAL`] and one of them deliberately loads the mirror up.
//!
//! [`SERIAL`] serialises but does **not** order -- libtest decides that -- so a
//! test here may run before or after the one that seeds 3 200 rows and has to
//! be right either way. The board tests get that by owning the *newest* rows in
//! the mirror, which is an invariant strong enough to be worth asserting rather
//! than assuming: see [`assert_nothing_newer`]. The database itself is fresh
//! per run (`test_util::run_nonce` is `{pid}-{nanos}`), so nothing an earlier
//! run seeded is ever in play.

use std::collections::HashSet;
use std::sync::LazyLock;

use chrono::{DateTime, Duration, Utc};
use knobas_search::{KindCatalog, Searcher, lists};
use tokio::sync::Mutex;

/// The board reads the whole mirror, so these cannot run concurrently against
/// one shared database -- `recency_reads_are_index_backed_per_kind` seeds
/// thousands of rows and would otherwise decide what the other tests see.
static SERIAL: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

fn searcher(pool: &sqlx::PgPool) -> Searcher {
    Searcher::with_kinds(pool.clone(), KindCatalog::default())
}

async fn seed(pool: &sqlx::PgPool, id: &str, kind: &str, updated: DateTime<Utc>) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(id)
        .bind(kind)
        .bind(format!("{kind} {id}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item
           (entity_id, source_id, kind, title, body_text, item_updated_at, synced_at, payload)
         values ($1,'jira',$2,$3,'body',$4, now(), '{}'::jsonb)",
    )
    .bind(id)
    .bind(kind)
    .bind(format!("{kind} {id}"))
    .bind(updated)
    .execute(pool)
    .await
    .unwrap();
}

fn token(tag: &str) -> String {
    format!("zh{tag}{}", uuid::Uuid::new_v4().simple())
}

/// A timestamp strictly newer than every row already in the mirror.
///
/// Seeding at `Utc::now()` is **not** enough to make a row the newest, and
/// assuming it was cost this file a red gate: another test in this file
/// seeds at its own `now()`, and under a mutex that serialises without ordering
/// there is no telling whether that happened a moment before or a moment after.
/// A row stamped `now() - 5s` then sits *behind* a row another test stamped
/// four seconds ago.
///
/// So the base comes from the data rather than from the clock. Everything
/// seeded above it is newer than everything already there, whoever ran first.
async fn newest_base(pool: &sqlx::PgPool) -> DateTime<Utc> {
    let max: Option<DateTime<Utc>> =
        sqlx::query_scalar("select max(item_updated_at) from sync.item")
            .fetch_one(pool)
            .await
            .unwrap();
    max.unwrap_or_else(Utc::now).max(Utc::now())
}

/// How many rows in the mirror are strictly newer than `id`.
///
/// This is the invariant every page-membership assertion in this file rests
/// on. The board shows the newest `RECENT_LIMIT` items in the *whole* mirror,
/// so "my row is on the board" holds only while few enough rows are newer. That
/// was true by construction -- these tests stamp `now()` and their competitors
/// are strictly older -- but *by construction* is another way of saying
/// "unwritten, and silently broken by the first future-dated fixture".
///
/// Returned rather than asserted so that the guard itself can be given a
/// positive control: a probe that always answers "clean" is not evidence that
/// anything was clean. `the_board_is_smart_lists_and_the_newest_of_every_kind`
/// shows it counting a row it should count before relying on it counting none.
///
/// `count`, not `max`: a tie at the same microsecond is still a row that could
/// take the slot, and the point is to fail loudly rather than flake.
async fn rows_newer_than(pool: &sqlx::PgPool, id: &str) -> i64 {
    sqlx::query_scalar(
        "select count(*) from sync.item
          where item_updated_at > (select item_updated_at from sync.item where entity_id = $1)",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn the_board_is_smart_lists_and_the_newest_of_every_kind() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let t = token("board");
    // Four kinds, interleaved in time, and the newest rows in the mirror -- so
    // "every kind is on the board" is a fact about the merge and not about one
    // kind happening to hold everything. Stamped *above the mirror's current
    // maximum* rather than around `now()`, so being newest survives whatever
    // else this file has already seeded; `n` counts down from the newest.
    //
    // The hazard is seeded first, deliberately, so that "above the mirror's
    // maximum" and "around now()" are not the same thing here. A source whose
    // clock runs ahead -- or a fixture that reaches for a round future number
    // -- puts a row in the mirror that `Utc::now()` does not clear, and a test
    // stamping itself `now()` would silently stop being the newest. That is the
    // ordering bug this file already shipped once, made deterministic.
    seed(
        &pool,
        &format!("jira:{t}-hazard"),
        "ticket",
        Utc::now() + Duration::minutes(10),
    )
    .await;
    let base = newest_base(&pool).await;
    for (n, kind) in [
        (0_i64, "ticket"),
        (1, "pr"),
        (2, "build"),
        (3, "page"),
        (4, "ticket"),
        (5, "pr"),
    ] {
        seed(
            &pool,
            &format!("jira:{t}-{n}"),
            kind,
            base + Duration::seconds(6 - n),
        )
        .await;
    }

    let board = searcher(&pool).launcher_board().await.unwrap();
    assert_eq!(board.smart_lists.len(), lists::BUILTINS.len());
    assert!(board.recent.len() <= 20);
    assert!(!board.recent.is_empty());

    // Newest first, globally -- the merge of the per-kind reads must not leak
    // its per-kind ordering into the result.
    let stamps: Vec<_> = board.recent.iter().map(|r| r.updated_at).collect();
    assert!(stamps.windows(2).all(|w| w[0] >= w[1]), "{stamps:?}");

    // The six rows just seeded are the six newest in the mirror -- which is the
    // assumption everything below rests on, so it is measured rather than
    // assumed. Exactly five rows are newer than the oldest of them: the other
    // five.
    assert_eq!(
        rows_newer_than(&pool, &format!("jira:{t}-5")).await,
        5,
        "something in the mirror is newer than this test's rows -- the hazard row \
         above, if the seed base stopped consulting the mirror -- so the board \
         membership assertions below would be testing the seeding order"
    );

    // Every kind that has items is represented, so a source that syncs rarely
    // is still visible on the board. All four, not "at least three": the read
    // walks the kinds one at a time, and an off-by-one that skipped exactly one
    // of them would satisfy any weaker count.
    let kinds: HashSet<&str> = board.recent.iter().map(|r| r.kind.as_str()).collect();
    for kind in ["ticket", "pr", "build", "page"] {
        assert!(kinds.contains(kind), "{kind} is missing from {kinds:?}");
    }

    // The guard's positive control: `rows_newer_than` has to be able to *see* a
    // newer row, or the zeroes it reports elsewhere mean nothing.
    seed(
        &pool,
        &format!("jira:{t}-future"),
        "ticket",
        base + Duration::hours(1),
    )
    .await;
    assert_eq!(
        rows_newer_than(&pool, &format!("jira:{t}-0")).await,
        1,
        "the newest-row probe cannot see a newer row, so it proves nothing"
    );
    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(format!("jira:{t}-future"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("delete from sync.item where entity_id = $1")
        .bind(format!("jira:{t}-future"))
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(rows_newer_than(&pool, &format!("jira:{t}-0")).await, 0);

    // A board row carries its provenance, like every other row in this crate.
    let newest = &board.recent[0];
    assert!(newest.entity_id.contains(&t), "{}", newest.entity_id);
    assert_eq!(newest.source_id, "jira");
    assert!(!newest.title.is_empty());
    assert!(newest.synced_at <= Utc::now());
}

#[tokio::test]
async fn a_tombstoned_item_is_not_recent() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let t = token("tomb");
    let id = format!("jira:{t}");
    seed(
        &pool,
        &id,
        "ticket",
        newest_base(&pool).await + Duration::seconds(1),
    )
    .await;
    assert_eq!(rows_newer_than(&pool, &id).await, 0);
    let s = searcher(&pool);

    assert!(
        s.launcher_board()
            .await
            .unwrap()
            .recent
            .iter()
            .any(|r| r.entity_id == id)
    );

    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();

    assert!(
        s.launcher_board()
            .await
            .unwrap()
            .recent
            .iter()
            .all(|r| r.entity_id != id),
        "the board reads sync.live_item, not sync.item"
    );
}

/// The board must not sequentially scan `sync.item`.
///
/// `0002` indexes `(kind, item_updated_at desc)`, not `item_updated_at` alone,
/// so `order by item_updated_at desc limit 20` -- the obvious spelling -- has
/// no index to walk and sorts the whole mirror on every keystroke. This is the
/// test that catches a later "simplification" back to it.
///
/// Asserted at a size where the choice is real: on a handful of rows the
/// planner correctly prefers a sequential scan, so a plan assertion against a
/// small corpus would pin nothing. Rows are seeded and `analyze`d first.
#[tokio::test]
async fn recency_reads_are_index_backed_per_kind() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let t = token("plan");

    // 4 kinds x 800 rows: enough that walking one kind's index beats sorting
    // the mirror, which is the decision the board depends on.
    sqlx::query(
        "insert into knobas.entity (id, kind, title)
         select $1 || ':' || k || '-' || g, k, 'row ' || g
           from unnest($2::text[]) as k, generate_series(1, $3) g",
    )
    .bind(&t)
    .bind(vec!["ticket", "pr", "build", "page"])
    .bind(800_i32)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into sync.item
           (entity_id, source_id, kind, title, body_text, item_updated_at, synced_at, payload)
         select $1 || ':' || k || '-' || g, 'jira', k, 'row ' || g, 'body',
                now() - make_interval(mins => g), now(), '{}'::jsonb
           from unnest($2::text[]) as k, generate_series(1, $3) g",
    )
    .bind(&t)
    .bind(vec!["ticket", "pr", "build", "page"])
    .bind(800_i32)
    .execute(&pool)
    .await
    .unwrap();
    // Without statistics the planner is guessing, and a plan assertion against
    // a guess is not an assertion about the query.
    sqlx::query("analyze sync.item")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("analyze knobas.entity")
        .execute(&pool)
        .await
        .unwrap();

    let plan = searcher(&pool).explain_recent().await.unwrap();
    assert!(
        plan.contains("item_kind_updated_idx"),
        "the board must not seq-scan sync.item:\n{plan}"
    );
    assert!(!plan.contains("Seq Scan on item"), "{plan}");
    // Two `Limit` nodes: one bounding each kind's branch inside the lateral,
    // one bounding the merge. The inner one is what keeps the read off the
    // busiest kind's whole history, and its absence is visible only here --
    // the rows that come back are identical either way.
    assert!(
        plan.matches("Limit").count() >= 2,
        "the per-kind read is unbounded:\n{plan}"
    );

    // The plan covers **both** statements `recent` runs, not just the one this
    // test is named after. Asserted through nodes only each statement can
    // produce -- a `Recursive Union` for the loose index scan over the kinds, a
    // `Function Scan on unnest` for the lateral over them -- rather than
    // through the labels `explain_recent` writes, which it could equally stop
    // writing. Narrowing the probe back to the rows half is precisely how a
    // sequential scan hid here in review round 1.
    assert!(
        plan.contains("Recursive Union"),
        "the kinds read is not in this plan, so the scan assertion above does \
         not cover it:\n{plan}"
    );
    assert!(
        plan.contains("unnest"),
        "the per-kind lateral is not in this plan:\n{plan}"
    );

    // And it still answers correctly at that size: bounded, newest first.
    let board = searcher(&pool).launcher_board().await.unwrap();
    assert_eq!(board.recent.len(), 20);
    let stamps: Vec<_> = board.recent.iter().map(|r| r.updated_at).collect();
    assert!(stamps.windows(2).all(|w| w[0] >= w[1]), "{stamps:?}");
}

/// **The board inherits the disabled-source filter too** (issue #202).
///
/// The ruling is "every reader", and the launcher's board is a *different
/// statement* from the search — `home::recent` orders by recency and never
/// touches the FTS index. So it is the one that shows the filter is inherited
/// from `sync.live_item` rather than re-implemented per reader, which is the
/// whole reason migration `0012` put it in the view.
///
/// Serialised and stamped newest like every test here: the board is a
/// whole-corpus read, so the row has to be the newest one to be certain of a
/// place on it.
#[tokio::test]
async fn the_board_drops_a_source_the_user_turned_off() {
    let _serial = SERIAL.lock().await;
    let pool = pool().await;
    let t = token("off");
    let source = format!("src-{t}");
    sqlx::query(
        "insert into knobas.source_config
           (id, kind, display_name, base_url, auth_kind, config)
         values ($1, 'test-kind', 'Test', 'http://x', 'Pat', '{}'::jsonb)",
    )
    .bind(&source)
    .execute(&pool)
    .await
    .unwrap();

    let id = format!("{source}:PAY-1");
    let newest = newest_base(&pool).await + Duration::seconds(60);
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'ticket',$2)")
        .bind(&id)
        .bind(&t)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item
           (entity_id, source_id, kind, title, body_text, item_updated_at, synced_at, payload)
         values ($1,$2,'ticket',$3,'body',$4, now(), '{}'::jsonb)",
    )
    .bind(&id)
    .bind(&source)
    .bind(&t)
    .bind(newest)
    .execute(&pool)
    .await
    .unwrap();

    let on = searcher(&pool).launcher_board().await.unwrap();
    assert!(
        on.recent.iter().any(|row| row.entity_id == id),
        "the newest row is not on the board, so this test cannot show it leaving"
    );

    sqlx::query("update knobas.source_config set enabled = false where id = $1")
        .bind(&source)
        .execute(&pool)
        .await
        .unwrap();

    let off = searcher(&pool).launcher_board().await.unwrap();
    assert!(
        off.recent.iter().all(|row| row.entity_id != id),
        "a source the user turned off is still on the board: {:?}",
        off.recent.iter().map(|r| &r.entity_id).collect::<Vec<_>>()
    );
}
