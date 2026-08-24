//! The migration baseline and the full-text search built on it.
//!
//! Every test shares one database (see `test_util`), and that database can
//! outlive a run, so seeds are idempotent (`on conflict do nothing`) or carry
//! ids unique per run. `migrate::run` is re-entrant and nothing truncates.

use knobas_db::{migrate, search};

#[tokio::test]
async fn migrates_and_finds_by_fts() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    sqlx::query(
        "insert into knobas.entity (id, kind, title) values ($1,$2,$3) on conflict (id) do nothing",
    )
    .bind("jira:TST-1")
    .bind("ticket")
    .bind("Retry failed SEPA payouts")
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,$2,$3,$4,$5,$6) on conflict (entity_id) do nothing",
    )
    .bind("jira:TST-1")
    .bind("jira")
    .bind("ticket")
    .bind("Retry failed SEPA payouts")
    .bind("Payouts that bounce with a retryable SEPA error should be retried with backoff.")
    .bind(serde_json::json!({"key": "TST-1"}))
    .execute(pool)
    .await
    .unwrap();

    let hits = search::search(pool, "sepa retry", 10).await.unwrap();
    assert_eq!(hits[0].entity_id, "jira:TST-1");
    assert!(hits[0].snippet.to_lowercase().contains("sepa"));
}

/// `link_active_idx` is what the link commands built on this schema rest on,
/// in all three of its parts: a second *active* link over the same
/// `(from, to, relation)` fails with SQLSTATE 23505; a different `relation`
/// over the same pair is a distinct link and must be allowed; and the index
/// being partial means a tombstone never blocks re-linking.
#[tokio::test]
async fn active_links_are_unique_per_relation_and_tombstones_do_not_block() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    // The database outlives a single run, so every run gets its own pair.
    let run = uuid::Uuid::new_v4();
    let from = format!("test:link-{run}-a");
    let to = format!("test:link-{run}-b");
    for id in [&from, &to] {
        sqlx::query("insert into knobas.entity (id, kind) values ($1,'ticket')")
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
    }

    link(pool, &from, &to, "related").await.unwrap();

    let duplicate = link(pool, &from, &to, "related").await.unwrap_err();
    assert_eq!(
        duplicate
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("23505"),
        "a second active link over the same pair and relation must be a unique violation"
    );

    // Third index column: the same pair under another relation is its own link.
    link(pool, &from, &to, "blocks").await.unwrap();

    sqlx::query(
        "update knobas.link set deleted_at = now()
         where from_id = $1 and relation = 'related' and deleted_at is null",
    )
    .bind(&from)
    .execute(pool)
    .await
    .unwrap();

    // The index is partial, so the tombstone does not block a fresh link.
    link(pool, &from, &to, "related").await.unwrap();
}

/// Insert one active link, surfacing the database error rather than panicking.
async fn link(
    pool: &sqlx::PgPool,
    from: &str,
    to: &str,
    relation: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "insert into knobas.link (from_id, to_id, relation, origin, created_by)
         values ($1,$2,$3,'manual','user')",
    )
    .bind(from)
    .bind(to)
    .bind(relation)
    .execute(pool)
    .await
    .map(|_| ())
}

#[tokio::test]
async fn fts_column_is_stored_not_virtual() {
    let pool = &knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(pool).await.unwrap();
    // Not named `gen`: that is a reserved keyword in edition 2024.
    let (generated,): (String,) = sqlx::query_as(
        "select attgenerated::text from pg_attribute
         where attrelid = 'sync.item'::regclass and attname = 'fts'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        generated, "s",
        "fts column must be STORED (PG 18 defaults to virtual!)"
    );
}
