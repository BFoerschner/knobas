//! One sync run, against a real PostgreSQL.

use knobas_source_mock::MockSource;

#[tokio::test]
async fn mock_sync_lands_in_postgres_and_is_searchable() {
    let pool = knobas_db::test_util::test_pool().await;
    let pool = &pool;
    knobas_db::migrate::run(pool).await.unwrap();

    let src = MockSource::new();
    let report = knobas_sync::run_once(pool, &src, None).await.unwrap();
    assert!(
        report.upserted > 10,
        "expected the full fixture, got {}",
        report.upserted
    );

    // idempotent: second full run upserts the same rows, no dupes
    let again = knobas_sync::run_once(pool, &src, None).await.unwrap();
    assert_eq!(report.upserted, again.upserted);
    let (cnt,): (i64,) = sqlx::query_as("select count(*) from sync.item where source_id = 'mock'")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(cnt as u64, report.upserted);

    // incremental from the cursor is a no-op
    let inc = knobas_sync::run_once(pool, &src, Some(report.cursor.clone()))
        .await
        .unwrap();
    assert_eq!(inc.upserted, 0);

    // and the synced corpus answers FTS
    let hits = knobas_db::search::search(pool, "sepa retry", 10).await.unwrap();
    assert!(
        hits.iter().any(|h| h.entity_id == "mock:PAY-231"),
        "hits: {hits:?}"
    );

    // sync wrote an activity line
    let acts = knobas_core::activity::recent(pool, 50).await.unwrap();
    assert!(
        acts.iter()
            .any(|a| a.actor == "sync:mock" && a.verb == "synced")
    );
}
