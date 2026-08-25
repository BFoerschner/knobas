//! The empty-query board, against a real PostgreSQL.
//!
//! Like the smart lists, the board is a whole-corpus read: "the newest twenty
//! items" is not something a per-test token can isolate, so the tests take
//! [`SERIAL`] and one of them deliberately loads the mirror up.

use std::collections::HashSet;
use std::sync::LazyLock;

use chrono::{DateTime, Duration, Utc};
use knobas_search::{KindCatalog, Searcher, lists};
use tokio::sync::Mutex;

/// The board reads the whole mirror, so these cannot run concurrently against
/// one shared database -- `plans_are_index_backed` seeds thousands of rows and
/// would otherwise decide what the other tests see.
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

#[tokio::test]
async fn the_board_is_smart_lists_and_the_newest_of_every_kind() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let t = token("board");
    // Four kinds, interleaved in time, and the newest rows in the mirror --
    // so "every kind is on the board" is a fact about the merge and not about
    // one kind happening to hold everything.
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
            Utc::now() - Duration::seconds(n),
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

    // Every kind that has items is represented, so a source that syncs rarely
    // is still visible on the board.
    let kinds: HashSet<&str> = board.recent.iter().map(|r| r.kind.as_str()).collect();
    assert!(kinds.len() >= 3, "{kinds:?}");

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
    seed(&pool, &id, "ticket", Utc::now()).await;
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

    // And it still answers correctly at that size: bounded, newest first.
    let board = searcher(&pool).launcher_board().await.unwrap();
    assert_eq!(board.recent.len(), 20);
    let stamps: Vec<_> = board.recent.iter().map(|r| r.updated_at).collect();
    assert!(stamps.windows(2).all(|w| w[0] >= w[1]), "{stamps:?}");
}
