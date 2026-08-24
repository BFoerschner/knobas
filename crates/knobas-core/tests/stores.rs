//! The link and activity stores, against a real PostgreSQL.
//!
//! `test_util` roots its server at `$TMPDIR/knobas-test-<pid>` and reaps
//! earlier runs, so each `cargo test` gets a freshly `initdb`-ed database --
//! but one database, shared by every test in this binary, and those tests run
//! concurrently. Each test therefore seeds entity ids unique to itself rather
//! than fixed ones, so that its rows are its own; truncating the shared tables
//! instead would break the tests running beside it.

use knobas_core::entity::EntityRef;
use knobas_core::{CoreError, activity, link};
use uuid::Uuid;

/// A migrated pool plus a ticket and a note entity unique to this run.
async fn seeded_pool() -> (sqlx::PgPool, EntityRef, EntityRef) {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();

    let run = Uuid::new_v4();
    let ticket = EntityRef::new("jira", &format!("LNK-{run}"));
    let note = EntityRef::new("note", &format!("lnk-{run}"));
    for (entity, kind) in [(&ticket, "ticket"), (&note, "note")] {
        sqlx::query(
            "insert into knobas.entity (id, kind) values ($1,$2) on conflict (id) do nothing",
        )
        .bind(entity.to_string())
        .bind(kind)
        .fetch_optional(&pool)
        .await
        .unwrap();
    }
    (pool, ticket, note)
}

#[tokio::test]
async fn link_lifecycle_with_tombstone() {
    let (pool, t, n) = seeded_pool().await;

    let id = link::create(&pool, &t, &n, "documents", link::Origin::Manual, "mara")
        .await
        .unwrap();
    // duplicate active link is rejected
    let dup = link::create(&pool, &t, &n, "documents", link::Origin::Manual, "mara").await;
    assert!(matches!(dup, Err(CoreError::Duplicate)), "{dup:?}");
    // visible from both ends
    assert_eq!(link::links_of(&pool, &t).await.unwrap().len(), 1);
    assert_eq!(link::links_of(&pool, &n).await.unwrap().len(), 1);

    link::unlink(&pool, id).await.unwrap();
    assert!(link::links_of(&pool, &t).await.unwrap().is_empty());
    // tombstone remains in the table
    let (cnt,): (i64,) = sqlx::query_as("select count(*) from knobas.link where id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(cnt, 1);
    // and re-linking after unlink is allowed again
    link::create(&pool, &t, &n, "documents", link::Origin::Manual, "mara")
        .await
        .unwrap();
}

#[tokio::test]
async fn link_row_carries_its_origin_and_direction() {
    let (pool, t, n) = seeded_pool().await;

    let id = link::create(
        &pool,
        &t,
        &n,
        "documents",
        link::Origin::Suggested,
        "sync:jira",
    )
    .await
    .unwrap();

    let rows = link::links_of(&pool, &n).await.unwrap();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.id, id);
    assert_eq!(row.from_id, t.to_string());
    assert_eq!(row.to_id, n.to_string());
    assert_eq!(row.relation, "documents");
    assert_eq!(row.origin, link::Origin::Suggested);
    assert_eq!(row.created_by, "sync:jira");
}

#[tokio::test]
async fn several_relations_coexist_and_come_back_newest_first() {
    let (pool, t, n) = seeded_pool().await;

    let before = chrono::Utc::now();
    let documents = link::create(&pool, &t, &n, "documents", link::Origin::Manual, "mara")
        .await
        .unwrap();
    let blocks = link::create(&pool, &t, &n, "blocks", link::Origin::Manual, "mara")
        .await
        .unwrap();
    let after = chrono::Utc::now();

    let rows = link::links_of(&pool, &t).await.unwrap();
    // Newest first -- asserted on ids, and asserted *first*, because these two
    // relation names happen to sort into the same order and so would hide a
    // flipped `order by` behind a passing name comparison.
    let ids: Vec<Uuid> = rows.iter().map(|row| row.id).collect();
    assert_eq!(ids, [blocks, documents]);
    // The pair carries both relations: `link_active_idx` is three-column.
    let relations: Vec<&str> = rows.iter().map(|row| row.relation.as_str()).collect();
    assert_eq!(relations, ["blocks", "documents"]);

    // `created_at` is the stored insertion time, not the reading query's clock:
    // `links_of` runs strictly after `after`.
    for row in &rows {
        assert!(
            row.created_at >= before && row.created_at <= after,
            "created_at {} outside [{before}, {after}]",
            row.created_at
        );
    }
    // ... and the two inserts, being separate transactions, are distinct.
    assert!(rows[0].created_at > rows[1].created_at);
}

#[tokio::test]
async fn unlink_is_idempotent_but_unknown_ids_are_reported() {
    let (pool, t, n) = seeded_pool().await;

    let id = link::create(&pool, &t, &n, "documents", link::Origin::Manual, "mara")
        .await
        .unwrap();
    link::unlink(&pool, id).await.unwrap();
    // unlinking an already-tombstoned link changes nothing and is not an error
    link::unlink(&pool, id).await.unwrap();

    let missing = link::unlink(&pool, Uuid::new_v4()).await;
    assert!(
        matches!(missing, Err(CoreError::LinkNotFound(_))),
        "{missing:?}"
    );
}

#[tokio::test]
async fn activity_records_and_lists() {
    let (pool, t, _n) = seeded_pool().await;

    activity::record(
        &pool,
        "user",
        "commented",
        Some(&t),
        serde_json::json!({"len": 42}),
    )
    .await
    .unwrap();

    let entity_id = t.to_string();
    let rows = activity::recent(&pool, 10).await.unwrap();
    assert!(
        rows.iter()
            .any(|r| r.verb == "commented" && r.entity_id.as_deref() == Some(entity_id.as_str()))
    );
}

#[tokio::test]
async fn activity_defaults_and_orders_newest_first() {
    let (pool, t, _n) = seeded_pool().await;
    // The activity table is shared with every other test, so this run's rows
    // are found by an actor nobody else uses.
    let actor = format!("sync:{}", Uuid::new_v4());

    activity::record(&pool, &actor, "synced", None, serde_json::Value::Null)
        .await
        .unwrap();
    activity::record(
        &pool,
        &actor,
        "linked",
        Some(&t),
        serde_json::json!({"n": 1}),
    )
    .await
    .unwrap();

    let mine: Vec<_> = activity::recent(&pool, 200)
        .await
        .unwrap()
        .into_iter()
        .filter(|row| row.actor == actor)
        .collect();

    assert_eq!(mine.len(), 2);
    // newest first
    assert_eq!(mine[0].verb, "linked");
    assert_eq!(mine[0].entity_id.as_deref(), Some(t.to_string().as_str()));
    assert_eq!(mine[0].detail, serde_json::json!({"n": 1}));
    // strict: two separate transactions, so a flipped `order by` shows up here
    assert!(mine[0].at > mine[1].at);
    // a null detail is stored as the column's empty-object default
    assert_eq!(mine[1].verb, "synced");
    assert_eq!(mine[1].entity_id, None);
    assert_eq!(mine[1].detail, serde_json::json!({}));

    // the limit actually caps the result -- the table holds our two rows at least
    assert_eq!(activity::recent(&pool, 1).await.unwrap().len(), 1);
}
