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
///
/// The **source id** is unique too, not just the key: `due` is scoped by
/// source and the database is shared by every test in this binary, so a fixed
/// namespace would make each test its neighbours' fixture.
async fn seeded_pool() -> (sqlx::PgPool, EntityRef) {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();

    let ticket = EntityRef::new(&format!("wq{}", Uuid::new_v4().simple()), "WQ-1");
    mirror(
        &pool,
        &ticket,
        "a payout fails",
        "a payout fails\n\nit does",
    )
    .await;
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
    let snapshot = wq::project(
        "comment",
        wq::target_of(pool, entity).await.unwrap().as_ref(),
    );
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

/// Story 22: writes against one entity flush in the order they were queued,
/// which means the queue offers exactly one of them at a time -- the oldest.
/// The second cannot be handed out while the first is still owed, or a comment
/// written second lands first.
#[tokio::test]
async fn one_entitys_writes_are_offered_oldest_first_and_one_at_a_time() {
    let (pool, ticket) = seeded_pool().await;
    let source = ticket.namespace.clone();

    let first = queue_comment(&pool, &ticket, "first").await;
    let second = queue_comment(&pool, &ticket, "second").await;

    let due = wq::due(&pool, &source).await.unwrap();
    assert_eq!(
        due.iter().map(|w| w.id).collect::<Vec<_>>(),
        vec![first.id],
        "the second write must wait behind the first"
    );

    wq::sent(&pool, first.id).await.unwrap().unwrap();
    let due = wq::due(&pool, &source).await.unwrap();
    assert_eq!(
        due.iter().map(|w| w.id).collect::<Vec<_>>(),
        vec![second.id]
    );
}

/// Story 21 and the other half of story 22: different entities do not block
/// one another, and neither do different sources. A queue that offered one
/// write at a time globally would stop everything for one dead credential.
#[tokio::test]
async fn different_entities_and_sources_never_block_one_another() {
    let (pool, ticket) = seeded_pool().await;
    let other = EntityRef::new(&ticket.namespace, "WQ-2");
    mirror(&pool, &other, "another", "another\n\nbody").await;
    let elsewhere = EntityRef::new(&format!("wq{}", Uuid::new_v4().simple()), "tidewater/api#1");
    mirror(
        &pool,
        &elsewhere,
        "a pull request",
        "a pull request\n\nbody",
    )
    .await;

    // Two writes on the blocked ticket, one on each of the others.
    let blocked = queue_comment(&pool, &ticket, "first").await;
    queue_comment(&pool, &ticket, "second").await;
    let sibling = queue_comment(&pool, &other, "unrelated").await;
    let far = queue_comment(&pool, &elsewhere, "elsewhere").await;

    // The first write is stuck: the server did not answer.
    wq::wait(
        &pool,
        blocked.id,
        wq::WaitReason::Unreachable,
        Some("timed out"),
    )
    .await
    .unwrap()
    .unwrap();

    let mut due: Vec<i64> = wq::due(&pool, &ticket.namespace)
        .await
        .unwrap()
        .iter()
        .map(|w| w.id)
        .collect();
    due.sort_unstable();
    let mut expected = vec![blocked.id, sibling.id];
    expected.sort_unstable();
    assert_eq!(
        due, expected,
        "the sibling entity is not behind the blocked one"
    );

    let elsewhere_due = wq::due(&pool, &elsewhere.namespace).await.unwrap();
    assert_eq!(
        elsewhere_due.iter().map(|w| w.id).collect::<Vec<_>>(),
        vec![far.id],
        "a second source's queue is untouched by the first's"
    );
}

/// Stories 5 and 19: a pending write says why it is waiting, and a refusal is
/// a different thing from a blip -- it stops being offered at all.
#[tokio::test]
async fn a_refusal_stops_being_offered_and_a_blip_does_not() {
    let (pool, ticket) = seeded_pool().await;
    let source = ticket.namespace.clone();

    let write = queue_comment(&pool, &ticket, "hello").await;
    assert!(write.wait_reason.is_none(), "nothing has been tried yet");
    assert!(write.attempted_at.is_none());

    let waited = wq::wait(
        &pool,
        write.id,
        wq::WaitReason::Unauthorized,
        Some("401 from jira"),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(waited.wait_reason, Some(wq::WaitReason::Unauthorized));
    assert_eq!(waited.detail.as_deref(), Some("401 from jira"));
    assert_eq!(waited.attempts, 1);
    assert!(waited.attempted_at.is_some());
    assert_eq!(
        wq::due(&pool, &source).await.unwrap().len(),
        1,
        "a retryable fault is still offered"
    );

    let refused = wq::refuse(&pool, write.id, "issue type does not accept comments")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(refused.state, WriteState::Refused);
    assert_eq!(
        refused.detail.as_deref(),
        Some("issue type does not accept comments"),
        "what the source said is kept, not just that it said no"
    );
    assert!(
        refused.wait_reason.is_none(),
        "a refusal is not a reason to wait"
    );
    assert!(
        wq::due(&pool, &source).await.unwrap().is_empty(),
        "a refused write is never retried"
    );
}

/// Stories 3, 17 and 18: the visible list and the count, and held told apart
/// from merely pending.
#[tokio::test]
async fn the_open_list_and_the_counts_separate_what_needs_a_decision() {
    let (pool, ticket) = seeded_pool().await;
    let held_target = EntityRef::new(&ticket.namespace, "WQ-2");
    mirror(&pool, &held_target, "held", "held\n\nbody").await;

    let pending = queue_comment(&pool, &ticket, "waiting").await;
    let to_hold = queue_comment(&pool, &held_target, "conflicted").await;
    let to_send = queue_comment(&pool, &held_target, "delivered").await;
    let to_drop = queue_comment(&pool, &ticket, "withdrawn").await;

    wq::hold(
        &pool,
        to_hold.id,
        serde_json::json!({"op": "comment", "live": true}),
    )
    .await
    .unwrap()
    .unwrap();
    wq::sent(&pool, to_send.id).await.unwrap().unwrap();
    wq::discard(&pool, to_drop.id).await.unwrap().unwrap();

    let open = wq::open(&pool).await.unwrap();
    let ids: Vec<i64> = open.iter().map(|w| w.id).collect();
    assert!(ids.contains(&pending.id) && ids.contains(&to_hold.id));
    assert!(
        !ids.contains(&to_send.id) && !ids.contains(&to_drop.id),
        "a settled write is history, not something knobas still owes"
    );
    // Newest first, so the list reads like the activity stream beside it.
    let mine: Vec<i64> = ids
        .into_iter()
        .filter(|id| *id == pending.id || *id == to_hold.id)
        .collect();
    assert_eq!(mine, vec![to_hold.id, pending.id]);

    let counts = wq::counts(&pool).await.unwrap();
    assert!(counts.pending >= 1 && counts.held >= 1);
    // The one distinction the shell badge rests on: a held write is not
    // counted as merely pending, or "3 waiting" would hide a decision.
    let listed = wq::open(&pool).await.unwrap();
    assert_eq!(
        listed.iter().find(|w| w.id == to_hold.id).unwrap().state,
        WriteState::Held
    );
    assert_eq!(
        listed.iter().find(|w| w.id == pending.id).unwrap().state,
        WriteState::Pending
    );
}

/// Story 16, and the rule the whole feature rests on: a held write is terminal
/// until the user acts. Nothing the queue does on its own moves it.
#[tokio::test]
async fn a_held_write_is_never_offered_however_often_the_queue_looks() {
    let (pool, ticket) = seeded_pool().await;
    let source = ticket.namespace.clone();

    let write = queue_comment(&pool, &ticket, "mine").await;
    let now = serde_json::json!({"op": "comment", "live": true, "text": "theirs"});
    let held = wq::hold(&pool, write.id, now.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(held.state, WriteState::Held);
    // Both versions, side by side (story 12): what it was, and what it is.
    assert_eq!(held.target_snapshot, write.target_snapshot);
    assert_eq!(held.held_snapshot, Some(now));

    for _ in 0..5 {
        assert!(
            wq::due(&pool, &source).await.unwrap().is_empty(),
            "a held write must never be offered for flushing"
        );
    }
    assert_eq!(
        wq::get(&pool, write.id).await.unwrap().unwrap().state,
        WriteState::Held,
        "and nothing may move it but the user"
    );
}

/// Stories 13, 14 and 15: the three things a user may do with a held write,
/// and nothing else.
#[tokio::test]
async fn the_user_may_apply_discard_or_edit_a_held_write() {
    let (pool, ticket) = seeded_pool().await;
    let source = ticket.namespace.clone();
    let theirs = serde_json::json!({"op": "comment", "live": true, "text": "theirs"});

    // Apply anyway: it goes back in the queue, and the version the user was
    // shown becomes the one it is measured against -- so the flush that
    // follows sends it instead of holding it again.
    let apply = queue_comment(&pool, &ticket, "anyway").await;
    wq::hold(&pool, apply.id, theirs.clone())
        .await
        .unwrap()
        .unwrap();
    let applied = wq::apply_anyway(&pool, apply.id).await.unwrap().unwrap();
    assert_eq!(applied.state, WriteState::Pending);
    assert_eq!(applied.target_snapshot, theirs);
    assert_eq!(
        wq::due(&pool, &source)
            .await
            .unwrap()
            .iter()
            .map(|w| w.id)
            .collect::<Vec<_>>(),
        vec![apply.id]
    );
    wq::sent(&pool, apply.id).await.unwrap().unwrap();

    // Discard: one action, and it is terminal.
    let drop_it = queue_comment(&pool, &ticket, "concede").await;
    wq::hold(&pool, drop_it.id, theirs.clone())
        .await
        .unwrap()
        .unwrap();
    let discarded = wq::discard(&pool, drop_it.id).await.unwrap().unwrap();
    assert_eq!(discarded.state, WriteState::Discarded);
    assert!(discarded.settled_at.is_some());
    assert!(
        wq::discard(&pool, drop_it.id).await.unwrap().is_none(),
        "discarding twice changes nothing, and says so"
    );
    wq::sent(&pool, drop_it.id).await.unwrap();
    assert_eq!(
        wq::get(&pool, drop_it.id).await.unwrap().unwrap().state,
        WriteState::Discarded,
        "a settled write cannot be revived by the flush loop"
    );

    // Edit and send: the user merges the two intentions themselves.
    let edit = queue_comment(&pool, &ticket, "original").await;
    wq::hold(&pool, edit.id, theirs.clone())
        .await
        .unwrap()
        .unwrap();
    let amended = wq::amend(
        &pool,
        edit.id,
        serde_json::json!({"Comment": {"entity": ticket.to_string(), "body": "merged"}}),
        theirs.clone(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(amended.state, WriteState::Pending);
    assert_eq!(amended.payload["Comment"]["body"], "merged");
    assert_eq!(
        amended.queued_at, edit.queued_at,
        "an edited write keeps the moment it was first queued"
    );

    // A refused write is editable too -- that is the only way out of a refusal
    // other than conceding it.
    let refused = queue_comment(&pool, &ticket, "rejected").await;
    wq::refuse(&pool, refused.id, "no").await.unwrap().unwrap();
    let retried = wq::amend(
        &pool,
        refused.id,
        serde_json::json!({"Comment": {"entity": ticket.to_string(), "body": "shorter"}}),
        theirs,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(retried.state, WriteState::Pending);
    assert!(
        retried.detail.is_none(),
        "the old refusal is not still shown"
    );
}

/// The queue outlives its target. A write queued against something the source
/// later withdrew is not a broken row to be swept -- it is a held write the
/// user is asked about, which is exactly what `project` reading `None` means.
#[tokio::test]
async fn a_write_survives_its_target_being_withdrawn() {
    let (pool, ticket) = seeded_pool().await;

    let write = queue_comment(&pool, &ticket, "still owed").await;
    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(ticket.to_string())
        .execute(&pool)
        .await
        .unwrap();

    let still_there = wq::get(&pool, write.id).await.unwrap().unwrap();
    assert_eq!(still_there.state, WriteState::Pending);
    let now = wq::project(
        "comment",
        wq::target_of(&pool, &ticket).await.unwrap().as_ref(),
    );
    assert_ne!(
        now, write.target_snapshot,
        "a target that vanished has changed, and must hold the write"
    );
}
