//! The numbers the status bar and the diagnostics view read.

use sqlx::PgPool;

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// Insert `n` mirror rows under a fresh source id, and return the id.
async fn seed(pool: &PgPool, n: usize) -> String {
    let id = format!("stats-{}", uuid::Uuid::new_v4().simple());
    for i in 0..n {
        let entity = format!("{id}:S-{i}");
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', 's')")
            .bind(&entity)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, title, payload)
             values ($1, $2, 'ticket', 's', '{}'::jsonb)",
        )
        .bind(&entity)
        .bind(&id)
        .execute(pool)
        .await
        .unwrap();
    }
    id
}

#[tokio::test]
async fn db_stats_reports_a_size_counts_and_a_per_source_breakdown() {
    let pool = pool().await;
    // Two sources with **different** counts, which is what makes the breakdown
    // a breakdown. An ungrouped `count(*)` would give both the same number.
    let one = seed(&pool, 1).await;
    let three = seed(&pool, 3).await;

    let stats = knobas_sync::stats::db_stats(&pool).await.unwrap();
    assert!(stats.db_bytes > 0);
    assert!(stats.entity_count >= 4);
    assert!(stats.item_count >= 4);
    assert!(stats.oldest_synced_at.is_some() && stats.newest_synced_at.is_some());
    assert!(stats.oldest_synced_at <= stats.newest_synced_at);

    let row_for = |id: &str| {
        stats
            .per_source
            .iter()
            .find(|s| s.source_id == id)
            .unwrap_or_else(|| panic!("{id} is missing from the breakdown"))
    };
    assert_eq!(row_for(&one).items, 1, "each row counts its own source");
    assert_eq!(row_for(&three).items, 3);
    assert!(row_for(&one).synced_at.is_some());
    assert!(
        stats
            .per_source
            .windows(2)
            .all(|w| w[0].source_id <= w[1].source_id),
        "id order, so the diagnostics list does not reshuffle between polls"
    );

    // Deliberately **not** "the rows sum to `item_count`". The database is
    // shared across this file, the totals and the breakdown are two
    // statements, and another test inserting between them makes that assertion
    // fail for a reason that has nothing to do with the code -- which is how it
    // failed in CI on the first run of this file. Two sources with different
    // counts pin the grouping without depending on anything global.
}

/// The re-index button. `reindex index concurrently` cannot run inside a
/// transaction block, so it goes straight to the pool -- and the FTS index must
/// still answer afterwards, which is the thing worth asserting.
#[tokio::test]
async fn reindexing_leaves_the_fts_index_usable() {
    let pool = pool().await;
    let id = format!("reidx-{}", uuid::Uuid::new_v4().simple());
    let entity = format!("{id}:R-1");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', 'sepa retry')")
        .bind(&entity)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1, $2, 'ticket', 'sepa retry', 'retry the sepa payouts', '{}'::jsonb)",
    )
    .bind(&entity)
    .bind(&id)
    .execute(&pool)
    .await
    .unwrap();

    knobas_sync::stats::reindex_fts(&pool).await.unwrap();

    // A row that *should* match, so a rebuild that left the index unusable --
    // or rebuilt the wrong one -- shows up as zero rather than as "0 >= 0".
    let (n,): (i64,) = sqlx::query_as(
        "select count(*) from sync.item, websearch_to_tsquery('english', $1) q
          where fts @@ q and source_id = $2",
    )
    .bind("sepa retry")
    .bind(&id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n, 1, "the index still answers the query it was rebuilt for");
}

/// The index this rebuilds has to exist, or `reindex_fts` is a button that
/// throws. Asserted against the catalogue rather than against the migration
/// text: a renamed index fails here with the name it now has.
#[tokio::test]
async fn the_index_it_rebuilds_is_the_one_the_schema_declares() {
    let pool = pool().await;
    let (schema, name) = knobas_sync::stats::FTS_INDEX
        .split_once('.')
        .expect("FTS_INDEX is schema-qualified");
    let (exists,): (bool,) = sqlx::query_as(
        "select exists(
             select 1 from pg_indexes where schemaname = $1 and indexname = $2)",
    )
    .bind(schema)
    .bind(name)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        exists,
        "{} is not an index in this schema",
        knobas_sync::stats::FTS_INDEX
    );
}
