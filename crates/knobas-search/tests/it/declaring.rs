//! `Searcher::declaring` against a sync in flight, on a real PostgreSQL
//! (#627, the deputy's second ruling).
//!
//! A module of its own, so a database of its own (#572): `Searcher::declaring`
//! writes `sync.parent_declaration` whole, which would remove the adapter
//! kinds `item_parent.rs`'s tests store for themselves.

use sqlx::PgPool;

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// **A search command never waits on a sync** (the deputy's second ruling on
/// #627). Another transaction holds the `(627, 0)` key shared -- what a sync
/// run does from its first flush to its commit -- and `Searcher::declaring`,
/// with the declarations it would write already stored, returns well inside a
/// deadline.
///
/// Its two halves of the guarantee: it reads before it writes and writes
/// nothing when nothing differs, and the declaration trigger returns before
/// the exclusive lock for a statement that changed no row. Without both, the
/// call's statement waits for the holder to commit.
#[tokio::test]
async fn declaring_unchanged_declarations_never_waits_on_a_sync() {
    let pool = pool().await;
    let templates = [knobas_source_gitea::descriptor_template()];
    // This module's own database: only this test writes Gitea's real kind.
    knobas_search::Searcher::declaring(pool.clone(), templates.clone())
        .await
        .unwrap();

    let mut holder = pool.begin().await.unwrap();
    sqlx::query("select pg_advisory_xact_lock_shared(627, 0)")
        .execute(&mut *holder)
        .await
        .unwrap();
    let answered = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        knobas_search::Searcher::declaring(pool.clone(), templates),
    )
    .await;
    holder.rollback().await.unwrap();
    assert!(
        answered.is_ok(),
        "Searcher::declaring waited on a transaction holding the shared key"
    );
    answered.unwrap().unwrap();
}
