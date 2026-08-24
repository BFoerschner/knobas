//! The migration baseline and the full-text search built on it.
//!
//! Every test shares one database (see `test_util`), so seeds use ids unique
//! to the test that writes them and are idempotent -- `migrate::run` is
//! re-entrant and the tests never truncate.

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

/// `link_active_idx` is what makes re-linking an existing pair fail with
/// SQLSTATE 23505, and unlinking a tombstone rather than a delete. Both halves
/// matter to the link commands built on top of this schema.
#[tokio::test]
async fn active_links_are_unique_while_tombstoned_ones_are_not() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    for id in ["test:link-a", "test:link-b"] {
        sqlx::query(
            "insert into knobas.entity (id, kind) values ($1,'ticket') on conflict (id) do nothing",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }
    let link = || {
        sqlx::query(
            "insert into knobas.link (from_id, to_id, origin, created_by)
             values ('test:link-a','test:link-b','manual','user')",
        )
        .execute(pool)
    };

    link().await.unwrap();

    let duplicate = link().await.unwrap_err();
    assert_eq!(
        duplicate
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("23505"),
        "a second active link over the same pair must be a unique violation"
    );

    sqlx::query(
        "update knobas.link set deleted_at = now()
         where from_id = 'test:link-a' and deleted_at is null",
    )
    .execute(pool)
    .await
    .unwrap();

    // The index is partial, so the tombstone does not block a fresh link.
    link().await.unwrap();
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
