//! The launcher's reads made ready after the app starts (the ux30 end gate,
//! 2026-11-23; `docs/decisions/2026-11-23-the-launcher-is-ready-before-the-first-key.md`):
//! `knobas_search::warm::warm`, which bring-up runs behind the app.
//!
//! Each test takes a database of its own: what it evicts from the buffers and
//! what it analyzes would reach every other test of a shared one.

use knobas_search::sql::{explain_kept, probe_sql};
use knobas_search::warm::{WARMED, warm};

use super::item_search::seed_scale_words;
use super::one_letter::filters;

async fn scratch(label: &str) -> (knobas_db::embedded::Connector, sqlx::PgPool) {
    let connector = knobas_db::test_util::scratch_database(label).await;
    let pool = connector
        .pool(4)
        .await
        .expect("a pool on the scratch database");
    (connector, pool)
}

/// Every page of the search copies out of PostgreSQL's buffers, as after a
/// start: what the first broad word met.
async fn evict(pool: &sqlx::PgPool) {
    sqlx::query("create extension if not exists pg_buffercache")
        .execute(pool)
        .await
        .unwrap();
    for name in WARMED {
        sqlx::query("select pg_buffercache_evict_relation(to_regclass($1))")
            .bind(name)
            .execute(pool)
            .await
            .unwrap();
    }
}

/// The plan's own lines in an `EXPLAIN (ANALYZE, BUFFERS)` text: what the
/// statement read running, without the *Planning* buffers. The planner reads
/// catalog pages, which the warm does not warm and a busy machine evicts, so
/// a `read=` there says nothing about the search copies.
fn executed(plan: &str) -> &str {
    plan.find("\nPlanning").map_or(plan, |at| &plan[..at])
}

/// **The first broad word after a start reads nothing from disk.** A word
/// more rows hold than the bound is counted first: its matches and the rows
/// it names, each off its own page of the search copies. With the copies out
/// of the buffers, as every start leaves them, the count reads those pages;
/// after the warm it reads none.
#[tokio::test]
async fn a_broad_words_count_reads_no_page_after_the_warm() {
    let (connector, pool) = scratch("warm_reads").await;
    let s = seed_scale_words(&pool, 300).await;
    sqlx::query("analyze").execute(&pool).await.unwrap();
    let count = || probe_sql("payout", true, &filters(&s), 5).unwrap();

    evict(&pool).await;
    let cold = explain_kept(&pool, count(), false).await.unwrap();
    assert!(
        executed(&cold).contains("read="),
        "the count read its pages from disk once they were out of the buffers:\n{cold}"
    );

    evict(&pool).await;
    let mut conn = connector.connect().await.unwrap();
    let warmed = warm(&mut conn).await.unwrap();
    let warm = explain_kept(&pool, count(), false).await.unwrap();
    assert!(
        !executed(&warm).contains("read="),
        "after the warm the count found every page in the buffers:\n{warm}"
    );
    assert_eq!(warmed.unread, None, "{warmed:?}");
    let read: Vec<&str> = warmed.read.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(read, WARMED, "every relation read, in order: {warmed:?}");
    assert!(warmed.left.is_empty(), "{warmed:?}");
    let pages = |name: &str| {
        warmed
            .read
            .iter()
            .find(|(read, _)| read == name)
            .map(|(_, pages)| *pages)
    };
    let size: i64 = sqlx::query_scalar(
        "select pg_relation_size('sync.item_search') / current_setting('block_size')::bigint",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        pages("sync.item_search"),
        Some(size),
        "read whole: {warmed:?}"
    );
}

/// **The analyze autovacuum is owed is taken at once**, so it does not drop
/// the launcher's kept plans a minute after the start, under a key. A table
/// written past autovacuum's own threshold since its last analyze is
/// analyzed, and the next warm finds nothing owed. A table written below the
/// threshold is left to autovacuum. Both are kept from autovacuum itself, so
/// only the warm can have analyzed them.
#[tokio::test]
async fn the_analyze_autovacuum_is_owed_is_taken_at_once() {
    let (connector, pool) = scratch("warm_owed").await;
    let mut conn = connector.connect().await.unwrap();
    for statement in [
        "create table public.warm_owed (n int) with (autovacuum_enabled = false)",
        "create table public.warm_not_owed (n int) with (autovacuum_enabled = false)",
        "analyze public.warm_owed",
        "analyze public.warm_not_owed",
        "insert into public.warm_owed select generate_series(1, 1000)",
        "insert into public.warm_not_owed select generate_series(1, 10)",
    ] {
        sqlx::query(statement).execute(&mut conn).await.unwrap();
    }
    // The writes' counts reach the cumulative statistics when this session
    // next flushes them.
    sqlx::query("select pg_stat_force_next_flush()")
        .execute(&mut conn)
        .await
        .unwrap();
    sqlx::query("select 1").execute(&mut conn).await.unwrap();
    let modified = async |table: &str| -> i64 {
        sqlx::query_scalar(
            "select n_mod_since_analyze from pg_stat_user_tables \
              where schemaname = 'public' and relname = $1",
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .unwrap()
    };
    assert_eq!(modified("warm_owed").await, 1_000);

    let warmed = warm(&mut conn).await.unwrap();
    assert!(
        warmed.analyzed.iter().any(|t| t == "public.warm_owed"),
        "{warmed:?}"
    );
    assert!(
        !warmed.analyzed.iter().any(|t| t == "public.warm_not_owed"),
        "{warmed:?}"
    );
    sqlx::query("select pg_stat_force_next_flush()")
        .execute(&mut conn)
        .await
        .unwrap();
    sqlx::query("select 1").execute(&mut conn).await.unwrap();
    assert_eq!(modified("warm_owed").await, 0);
    assert_eq!(modified("warm_not_owed").await, 10);

    let again = warm(&mut conn).await.unwrap();
    assert!(
        !again.analyzed.iter().any(|t| t.starts_with("public.")),
        "nothing owed the second time: {again:?}"
    );
}
