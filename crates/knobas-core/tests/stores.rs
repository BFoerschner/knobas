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

    let id = link::create(
        &pool,
        &t,
        &n,
        "documents",
        link::Origin::Manual,
        None,
        "mara",
    )
    .await
    .unwrap()
    .id;
    let dup = link::create(
        &pool,
        &t,
        &n,
        "documents",
        link::Origin::Manual,
        None,
        "mara",
    )
    .await;
    assert!(matches!(dup, Err(CoreError::Duplicate)), "{dup:?}");
    assert_eq!(link::entries_of(&pool, &t).await.unwrap().len(), 1);
    assert_eq!(link::entries_of(&pool, &n).await.unwrap().len(), 1);

    link::unlink(&pool, id).await.unwrap();
    assert!(link::entries_of(&pool, &t).await.unwrap().is_empty());
    // tombstone remains in the table
    let (cnt,): (i64,) = sqlx::query_as("select count(*) from knobas.link where id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(cnt, 1);
    // and re-linking after unlink is allowed again
    link::create(
        &pool,
        &t,
        &n,
        "documents",
        link::Origin::Manual,
        None,
        "mara",
    )
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
        None,
        "sync:jira",
    )
    .await
    .unwrap()
    .id;

    let rows = link::entries_of(&pool, &n).await.unwrap();
    assert_eq!(rows.len(), 1);
    let row = &rows[0].link;
    assert_eq!(row.id, id);
    assert_eq!(row.from_id, t.to_string());
    assert_eq!(row.to_id, n.to_string());
    assert_eq!(row.relation, "documents");
    assert_eq!(row.origin, link::Origin::Suggested);
    assert_eq!(row.created_by, "sync:jira");

    // Read from `n`, the resolved end is `t` -- the end the reader is *not*
    // on. A hydration keyed on `to_id` would hand `n`'s own panel a row
    // describing `n`.
    let other = &rows[0].other;
    assert_eq!(other.entity_id, t.to_string());
    assert_eq!(other.deleted_at, None);
}

#[tokio::test]
async fn several_relations_coexist_and_come_back_newest_first() {
    let (pool, t, n) = seeded_pool().await;

    let before = chrono::Utc::now();
    let documents = link::create(
        &pool,
        &t,
        &n,
        "documents",
        link::Origin::Manual,
        None,
        "mara",
    )
    .await
    .unwrap()
    .id;
    let blocks = link::create(&pool, &t, &n, "blocks", link::Origin::Manual, None, "mara")
        .await
        .unwrap()
        .id;
    let after = chrono::Utc::now();

    let rows = link::entries_of(&pool, &t).await.unwrap();
    // Newest first -- asserted on ids, and asserted *first*, because these two
    // relation names happen to sort into the same order and so would hide a
    // flipped `order by` behind a passing name comparison.
    let ids: Vec<Uuid> = rows.iter().map(|row| row.link.id).collect();
    assert_eq!(ids, [blocks, documents]);
    // The pair carries both relations: `link_pair_active_idx` is three-column.
    let relations: Vec<&str> = rows.iter().map(|row| row.link.relation.as_str()).collect();
    assert_eq!(relations, ["blocks", "documents"]);

    // `created_at` is the stored insertion time, not the reading query's clock:
    // `entries_of` runs strictly after `after`.
    for row in &rows {
        assert!(
            row.link.created_at >= before && row.link.created_at <= after,
            "created_at {} outside [{before}, {after}]",
            row.link.created_at
        );
    }
    // ... and the two inserts, being separate transactions, are distinct.
    assert!(rows[0].link.created_at > rows[1].link.created_at);
}

/// The same pair linked the other way round is the **same** link (#70).
///
/// `link_pair_active_idx` (migration `0011`) normalises the pair with
/// `least`/`greatest`, so the rule and `entries_of`'s undirected read finally
/// agree. Before it, `A -> B` and `B -> A` both landed and both panels drew two
/// rows for one relationship -- which is #40's story 14 read backwards.
///
/// Unordered for uniqueness, **ordered for storage**: the surviving row still
/// says which end is which, or `blocks` could not be told from `blocked by`
/// (story 7).
#[tokio::test]
async fn the_reverse_of_an_active_link_is_a_duplicate_and_the_stored_direction_survives() {
    let (pool, t, n) = seeded_pool().await;

    let drawn = link::create(&pool, &t, &n, "blocks", link::Origin::Manual, None, "mara")
        .await
        .unwrap();

    let reversed = link::create(&pool, &n, &t, "blocks", link::Origin::Manual, None, "mara").await;
    assert!(
        matches!(reversed, Err(CoreError::Duplicate)),
        "the reverse of an active link must be a duplicate, not a second row: {reversed:?}"
    );

    // One row, and it still points the way it was drawn.
    let rows = link::entries_of(&pool, &n).await.unwrap();
    let blocking: Vec<&link::LinkEntry> = rows
        .iter()
        .filter(|row| row.link.relation == "blocks")
        .collect();
    assert_eq!(
        blocking.len(),
        1,
        "the panel must show one row for one link"
    );
    assert_eq!(blocking[0].link.from_id, t.to_string());
    assert_eq!(blocking[0].link.to_id, n.to_string());
    assert_eq!(blocking[0].link.id, drawn.id);

    // Still per relation, and still from either end (story 15).
    link::create(
        &pool,
        &n,
        &t,
        "documents",
        link::Origin::Manual,
        None,
        "mara",
    )
    .await
    .expect("the same pair under another relation is its own link");

    // Still partial: withdrawing frees the pair the other way round too.
    link::unlink(&pool, drawn.id).await.unwrap();
    link::create(&pool, &n, &t, "blocks", link::Origin::Manual, None, "mara")
        .await
        .expect("a tombstone must not block re-linking in the other direction");
}

#[tokio::test]
async fn unlink_is_idempotent_but_unknown_ids_are_reported() {
    let (pool, t, n) = seeded_pool().await;

    let id = link::create(
        &pool,
        &t,
        &n,
        "documents",
        link::Origin::Manual,
        None,
        "mara",
    )
    .await
    .unwrap()
    .id;
    let withdrawn = link::unlink(&pool, id).await.unwrap();
    assert_eq!(
        withdrawn.map(|row| row.id),
        Some(id),
        "the call that tombstoned the link hands back the row it tombstoned"
    );
    assert_eq!(
        link::unlink(&pool, id).await.unwrap().map(|row| row.id),
        None,
        "the second call changed nothing, and says so by handing back no row -- \
         a caller that logged one line per `Ok` would write a second `unlinked` \
         for a link that was already withdrawn"
    );

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
    let rows = activity::recent(&pool, 10, None).await.unwrap();
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

    let mine: Vec<_> = activity::recent(&pool, 200, None)
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
    assert_eq!(activity::recent(&pool, 1, None).await.unwrap().len(), 1);
}

/// The detail view's history panel reads one entity's lines and no one else's.
///
/// Both directions are asserted: the other entity's line is absent from the
/// scoped read *and* present in the unscoped one. A `where` that matched
/// nothing would satisfy only the first.
#[tokio::test]
async fn activity_can_be_scoped_to_one_entity() {
    let (pool, ticket, note) = seeded_pool().await;
    let verb = format!("verb-{}", Uuid::new_v4());

    for entity in [&ticket, &note] {
        activity::record(&pool, "user", &verb, Some(entity), serde_json::json!({}))
            .await
            .unwrap();
    }

    let scoped = activity::recent(&pool, 200, Some(&ticket)).await.unwrap();
    assert_eq!(
        scoped
            .iter()
            .filter(|row| row.verb == verb)
            .map(|row| row.entity_id.clone())
            .collect::<Vec<_>>(),
        vec![Some(ticket.to_string())]
    );
    assert!(
        scoped
            .iter()
            .all(|row| row.entity_id.as_deref() == Some(ticket.to_string().as_str())),
        "a scoped read returns nothing but that entity's lines"
    );

    let global = activity::recent(&pool, 500, None).await.unwrap();
    assert_eq!(
        global.iter().filter(|row| row.verb == verb).count(),
        2,
        "the unscoped read still sees both"
    );
}

/// The `limit` applies to the scoped read too -- it is the same `$1`.
#[tokio::test]
async fn a_scoped_read_is_still_capped_and_newest_first() {
    let (pool, ticket, _note) = seeded_pool().await;
    let verb = format!("verb-{}", Uuid::new_v4());

    for _ in 0..3 {
        activity::record(&pool, "user", &verb, Some(&ticket), serde_json::json!({}))
            .await
            .unwrap();
    }

    let capped = activity::recent(&pool, 2, Some(&ticket)).await.unwrap();
    assert_eq!(capped.len(), 2);
    // Three rows written in three transactions, so a flipped order shows up.
    assert!(capped[0].id > capped[1].id);
}

/// An endpoint with no `knobas.entity` row is its own error, not a database
/// fault.
///
/// The distinction is what the IPC boundary needs: linking to an entity that
/// has not synced yet is a normal event the UI reports as `not_found`, while
/// `CoreError::Db` crosses as `internal` -- "knobas is broken" for something
/// the user merely mistyped.
#[tokio::test]
async fn a_link_endpoint_with_no_entity_row_is_its_own_error() {
    let (pool, ticket, _note) = seeded_pool().await;
    let absent = EntityRef::new("jira", &format!("GONE-{}", Uuid::new_v4()));

    // Both ends, because `from_id` and `to_id` carry a foreign key each and
    // they are separate constraints: a classifier keyed on one of them by name
    // would leave the other end reporting `internal`.
    for (from, to) in [(&ticket, &absent), (&absent, &ticket)] {
        let created = link::create(
            &pool,
            from,
            to,
            "documents",
            link::Origin::Manual,
            None,
            "mara",
        )
        .await;
        assert!(
            matches!(created, Err(CoreError::EndpointMissing)),
            "{created:?}"
        );
    }
}

/// The write hands back the row it wrote, so a caller can announce the line
/// without reading it again.
///
/// Asserted against an independent read rather than against the arguments: the
/// point of returning the row is that it is the *stored* one -- the id and the
/// timestamp the database chose, and the `detail` after the null coercion.
#[tokio::test]
async fn the_activity_write_returns_the_row_it_wrote() {
    let (pool, ticket, _note) = seeded_pool().await;
    let actor = format!("sync:{}", Uuid::new_v4());

    let written = activity::record(
        &pool,
        &actor,
        "linked",
        Some(&ticket),
        serde_json::json!({"n": 1}),
    )
    .await
    .unwrap();

    let stored = activity::recent(&pool, 200, Some(&ticket))
        .await
        .unwrap()
        .into_iter()
        .find(|row| row.actor == actor)
        .expect("the written line is in the log");

    assert_eq!(written.id, stored.id);
    assert_eq!(written.at, stored.at);
    assert_eq!(written.actor, stored.actor);
    assert_eq!(written.verb, stored.verb);
    assert_eq!(written.entity_id, stored.entity_id);
    assert_eq!(written.detail, stored.detail);
    assert_eq!(
        written.entity_id.as_deref(),
        Some(ticket.to_string().as_str())
    );

    // The null-detail coercion is visible in the returned row too. A write that
    // echoed its argument back would hand the caller a jsonb null that the log
    // itself does not carry -- and `ActivityRow.detail` is `unknown` in the
    // TypeScript mirror precisely because nothing downstream re-checks it.
    let coerced = activity::record(&pool, &actor, "synced", None, serde_json::Value::Null)
        .await
        .unwrap();
    assert_eq!(coerced.detail, serde_json::json!({}));
    assert_eq!(coerced.entity_id, None);
    assert!(
        coerced.id > written.id,
        "the identity column advances: {} then {}",
        written.id,
        coerced.id
    );
}

/// A foreign-key violation somewhere other than a link write is **not** an
/// endpoint.
///
/// `EndpointMissing` says "one of the link's endpoints has no entity" and
/// crosses the bridge as `not_found`, which is right for a user naming an
/// entity that has not synced yet -- and wrong for anything else. `0001`
/// carries foreign keys on `knobas.context.anchor_id` and
/// `sync.item.entity_id` as well, and violating one of those is knobas' own
/// bug: `internal`, not "no such thing".
///
/// This is the test that keeps the claim honest as this crate grows writes.
/// Classifying every 23503 crate-wide would pass every other test in this file
/// and mislabel the first one of those writes that lands.
#[tokio::test]
async fn a_foreign_key_violation_outside_a_link_write_is_not_an_endpoint() {
    let (pool, _t, _n) = seeded_pool().await;
    let absent = EntityRef::new("jira", &format!("GONE-{}", Uuid::new_v4()));

    let violated = sqlx::query(
        "insert into knobas.context (id, kind, title, anchor_id) values ($1,'adhoc','ctx',$2)",
    )
    .bind(format!("ctx:{}", Uuid::new_v4()))
    .bind(absent.to_string())
    .execute(&pool)
    .await
    .expect_err("the anchor has no entity row, so the foreign key rejects it");

    // The same conversion every `?` in this crate performs.
    let classified = CoreError::from(violated);
    assert!(
        matches!(classified, CoreError::Db(_)),
        "a foreign key that is not a link endpoint must stay a database fault, \
         got {classified:?}"
    );
}

/// The link write hands back the row it wrote, `note` and all.
///
/// The same reason [`the_activity_write_returns_the_row_it_wrote`] exists: the
/// caller has to announce what it wrote -- the command layer puts the link's
/// id, its other end and its relation into an activity line -- and `id` and
/// `created_at` are the database's to choose. A write that handed back only an
/// id would make the caller read back a row it just wrote, in a table where
/// "the newest row" is not reliably its own.
///
/// Asserted against an independent [`link::entries_of`] read rather than against
/// the arguments, so an implementation that echoed its own inputs back fails.
#[tokio::test]
async fn the_link_write_returns_the_stored_row_including_its_note() {
    let (pool, t, n) = seeded_pool().await;

    let written = link::create(
        &pool,
        &t,
        &n,
        "documents",
        link::Origin::Manual,
        Some("the retry storm postmortem"),
        "user",
    )
    .await
    .unwrap();

    let stored = link::entries_of(&pool, &t)
        .await
        .unwrap()
        .into_iter()
        .find(|entry| entry.link.id == written.id)
        .expect("the written link is in the store")
        .link;

    assert_eq!(written.from_id, stored.from_id);
    assert_eq!(written.to_id, stored.to_id);
    assert_eq!(written.relation, stored.relation);
    assert_eq!(written.origin, stored.origin);
    assert_eq!(written.created_by, stored.created_by);
    assert_eq!(written.created_at, stored.created_at);
    assert_eq!(
        written.note.as_deref(),
        Some("the retry storm postmortem"),
        "the note the caller gave is the note the row carries"
    );
    assert_eq!(
        stored.note, written.note,
        "and the read sees the same one -- `note` is a column, not a field the \
         writer invented on the way out"
    );

    // A link made without one carries no note, rather than an empty string:
    // `note` is nullable, and `Some(\"\")` is a note the user did not write.
    let bare = link::create(&pool, &t, &n, "blocks", link::Origin::Manual, None, "user")
        .await
        .unwrap();
    assert_eq!(bare.note, None);
    assert_eq!(
        link::entries_of(&pool, &n)
            .await
            .unwrap()
            .into_iter()
            .find(|entry| entry.link.id == bare.id)
            .expect("the second link is in the store")
            .link
            .note,
        None
    );
}

/// Unlinking hands back the row it tombstoned, so its caller can name the
/// link, its other end and its relation without a second read -- and hands
/// back nothing when there was nothing to withdraw.
///
/// The distinction is the whole point: `unlink` is idempotent, so `Ok` alone
/// cannot tell "I withdrew this" from "somebody already had". One activity
/// line per *mutation* needs the difference.
#[tokio::test]
async fn unlink_returns_the_row_it_withdrew_and_only_the_first_time() {
    let (pool, t, n) = seeded_pool().await;

    let created = link::create(
        &pool,
        &t,
        &n,
        "documents",
        link::Origin::Manual,
        Some("why"),
        "user",
    )
    .await
    .unwrap();

    let withdrawn = link::unlink(&pool, created.id)
        .await
        .unwrap()
        .expect("the first unlink withdrew the link");
    assert_eq!(withdrawn.id, created.id);
    assert_eq!(withdrawn.from_id, created.from_id);
    assert_eq!(withdrawn.to_id, created.to_id);
    assert_eq!(withdrawn.relation, created.relation);
    assert_eq!(
        withdrawn.note.as_deref(),
        Some("why"),
        "the withdrawn row is the whole row, not a stub carrying an id"
    );

    assert!(
        link::unlink(&pool, created.id).await.unwrap().is_none(),
        "the second unlink withdrew nothing"
    );
    // ... and it is still not an error, which is the behaviour that was there
    // before the return type grew.
    assert!(link::unlink(&pool, created.id).await.is_ok());
}
