//! The write queue store, against a real PostgreSQL (issue #42).
//!
//! Same shape as `stores.rs` and for the same reason: one database is shared
//! by every test in this binary and they run concurrently, so each test seeds
//! entity ids unique to itself rather than truncating tables its neighbours
//! are using.
//!
//! What these assert is what a caller can observe -- a write survives a
//! restart, a changed target produces a held write, a held write never flushes
//! on its own -- never the shape of an internal state machine.

use knobas_core::entity::EntityRef;
use knobas_core::write_queue::{self as wq, WriteState};
use uuid::Uuid;

/// A migrated pool plus a mirrored ticket unique to this run.
async fn seeded_pool() -> (sqlx::PgPool, EntityRef) {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();

    let ticket = EntityRef::new("jira", &format!("WQ-{}", Uuid::new_v4()));
    mirror(&pool, &ticket, "a payout fails", "a payout fails\n\nit does").await;
    (pool, ticket)
}

/// Put `entity` in the mirror, or move it if it is already there.
async fn mirror(pool: &sqlx::PgPool, entity: &EntityRef, title: &str, body: &str) {
    sqlx::query(
        "insert into knobas.entity (id, kind, title) values ($1,'ticket',$2)
         on conflict (id) do update set title = excluded.title, deleted_at = null",
    )
    .bind(entity.to_string())
    .bind(title)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1, $2, 'ticket', $3, $4, '{}'::jsonb)
         on conflict (entity_id) do update
           set title = excluded.title, body_text = excluded.body_text",
    )
    .bind(entity.to_string())
    .bind(&entity.namespace)
    .bind(title)
    .bind(body)
    .execute(pool)
    .await
    .unwrap();
}

/// Queue a comment against `entity`, snapshotting the target as it stands.
async fn queue_comment(pool: &sqlx::PgPool, entity: &EntityRef, body: &str) -> wq::QueuedWrite {
    let snapshot = wq::project("comment", wq::target_of(pool, entity).await.unwrap().as_ref());
    wq::queue(
        pool,
        &entity.namespace,
        entity,
        "comment",
        serde_json::json!({ "Comment": { "entity": entity.to_string(), "body": body } }),
        snapshot,
    )
    .await
    .unwrap()
}

/// Story 8: an edit made while a source cannot take it is still there after a
/// restart. The second pool is a *new connection to the same database*, which
/// is all a restart is from the queue's point of view.
#[tokio::test]
async fn a_queued_write_survives_a_restart() {
    let (pool, ticket) = seeded_pool().await;

    let queued = queue_comment(&pool, &ticket, "on it").await;
    assert_eq!(queued.state, WriteState::Pending);
    assert_eq!(queued.entity_id, ticket.to_string());
    assert_eq!(queued.op, "comment");

    drop(pool);
    let restarted = knobas_db::test_util::test_pool().await;
    let found = wq::get(&restarted, queued.id).await.unwrap().unwrap();
    assert_eq!(found.id, queued.id);
    assert_eq!(found.state, WriteState::Pending);
    assert_eq!(found.payload["Comment"]["body"], "on it");
    // And the snapshot came back with it -- without that, hold detection has
    // nothing to compare against after a restart.
    assert_eq!(found.target_snapshot, queued.target_snapshot);
}
