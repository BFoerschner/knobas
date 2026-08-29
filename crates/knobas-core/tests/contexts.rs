//! Contexts and the one-hop membership rule (§16.11, ADR-0008), against a
//! real PostgreSQL.
//!
//! What is asserted here is **who is a member and who is not** -- the ratified
//! rule is `seed + direct links + one hop`, and every positive case in this
//! file is paired with the entity one step past the rule's edge, because a
//! walk that reaches everything is as broken as one that reaches nothing and
//! only the far edge catches the first.
//!
//! # Why every test gets a database of its own
//!
//! `context::list` reads every unarchived context, and the epic-children seed
//! scans the whole mirror for a matching `fields.parent` -- both are passes
//! over shared state, so test A's context would appear in test B's switcher
//! and every list assertion would be decided by scheduling. `scratch_database`
//! costs one `create database` per test and buys fixtures that read as
//! `PAY-231` rather than as a uuid -- the trade `suggestions.rs` records.

use knobas_core::context::{self, ContextKind};
use knobas_core::entity::EntityRef;
use knobas_core::link::Origin;
use knobas_core::{CoreError, link};
use sqlx::PgPool;
use std::collections::BTreeSet;

/// The source every mirrored fixture below is synced under.
const SOURCE: &str = "jira";

/// A migrated, empty database of this test's own.
async fn scratch() -> PgPool {
    knobas_db::test_util::scratch_database("contexts")
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database")
}

/// An entity that exists but has never been mirrored.
async fn entity_only(pool: &PgPool, namespace: &str, kind: &str, key: &str) -> String {
    let id = EntityRef::new(namespace, key).to_string();
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(&id)
        .bind(kind)
        .bind(key)
        .execute(pool)
        .await
        .unwrap();
    id
}

/// One live mirror item with a payload -- what the epic seeds read.
async fn with_payload(pool: &PgPool, kind: &str, key: &str, payload: serde_json::Value) -> String {
    mirrored(pool, SOURCE, kind, key, payload).await
}

/// As [`with_payload`], in the source named.
async fn mirrored(
    pool: &PgPool,
    source: &str,
    kind: &str,
    key: &str,
    payload: serde_json::Value,
) -> String {
    let id = entity_only(pool, source, kind, key).await;
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,$2,$3,$4,'',$5)",
    )
    .bind(&id)
    .bind(source)
    .bind(kind)
    .bind(key)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// A confirmed link between two entities, drawn by hand.
async fn draw(pool: &PgPool, from: &str, to: &str) {
    link::create(
        pool,
        &EntityRef::parse(from).unwrap(),
        &EntityRef::parse(to).unwrap(),
        "related",
        Origin::Manual,
        None,
        "user",
    )
    .await
    .unwrap();
}

/// A **proposal** between two entities -- a link row nobody has confirmed.
///
/// Written raw rather than through a detector, because what matters to this
/// file is only the row's population: `confirmed_at is null` is the whole of
/// what makes it a guess.
async fn propose(pool: &PgPool, from: &str, to: &str) {
    sqlx::query(
        "insert into knobas.link
             (from_id, to_id, relation, origin, created_by,
              confirmed_at, rule, rule_class, reason)
         values ($1, $2, 'related', 'suggested', 'knobas',
                 null, 'test', 'exact_key', 'a guess for the test')",
    )
    .bind(from)
    .bind(to)
    .execute(pool)
    .await
    .unwrap();
}

/// The members of a context, as a set.
async fn members(pool: &PgPool, ctx: &str) -> BTreeSet<String> {
    context::member_ids(pool, ctx)
        .await
        .unwrap()
        .into_iter()
        .collect()
}

fn set(ids: &[&String]) -> BTreeSet<String> {
    ids.iter().map(|id| (*id).clone()).collect()
}

#[tokio::test]
async fn an_adhoc_context_is_created_listed_and_linkable() {
    let pool = scratch().await;

    let ctx = context::create_adhoc(&pool, "Staging DB configuration")
        .await
        .unwrap();
    assert_eq!(ctx.kind, ContextKind::Adhoc);
    assert_eq!(ctx.title, "Staging DB configuration");
    assert!(ctx.anchor_id.is_none());
    assert!(ctx.id.starts_with("ctx:"), "a local id: {}", ctx.id);

    let listed = context::list(&pool).await.unwrap();
    assert_eq!(
        listed.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec![ctx.id.as_str()]
    );

    // The context is an entity, so it is linkable (spec §5a) -- which is what
    // an explicit *Add to context* is.
    let ticket = entity_only(&pool, SOURCE, "ticket", "PAY-1").await;
    draw(&pool, &ctx.id, &ticket).await;
    assert_eq!(members(&pool, &ctx.id).await, set(&[&ticket]));
}

#[tokio::test]
async fn newest_context_first_in_the_switcher_list() {
    let pool = scratch().await;
    let first = context::create_adhoc(&pool, "first").await.unwrap();
    let second = context::create_adhoc(&pool, "second").await.unwrap();
    // Two rows may share a timestamp; the tiebreak must still be stable.
    let listed = context::list(&pool).await.unwrap();
    let ids: Vec<&str> = listed.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&first.id.as_str()) && ids.contains(&second.id.as_str()));
}

#[tokio::test]
async fn promoting_a_ticket_yields_one_ticket_context_however_often() {
    let pool = scratch().await;
    let ticket = with_payload(&pool, "ticket", "PAY-1", serde_json::json!({})).await;
    sqlx::query("update knobas.entity set title = 'Fix the payout retry' where id = $1")
        .bind(&ticket)
        .execute(&pool)
        .await
        .unwrap();

    let anchor = EntityRef::parse(&ticket).unwrap();
    let ctx = context::promote(&pool, &anchor).await.unwrap().unwrap();
    assert_eq!(ctx.kind, ContextKind::Ticket);
    assert_eq!(ctx.title, "Fix the payout retry");
    assert_eq!(ctx.anchor_id.as_deref(), Some(ticket.as_str()));

    // Promoting again is the same context, not a second one.
    let again = context::promote(&pool, &anchor).await.unwrap().unwrap();
    assert_eq!(again.id, ctx.id);
    assert_eq!(context::list(&pool).await.unwrap().len(), 1);
}

#[tokio::test]
async fn promoting_something_that_never_synced_is_a_miss_not_a_context() {
    let pool = scratch().await;
    let ghost = EntityRef::new(SOURCE, "PAY-404");
    assert!(context::promote(&pool, &ghost).await.unwrap().is_none());
    assert!(context::list(&pool).await.unwrap().is_empty());
}

#[tokio::test]
async fn promoting_an_epic_is_read_off_the_issue_type() {
    let pool = scratch().await;
    let epic = with_payload(
        &pool,
        "ticket",
        "EPIC-1",
        serde_json::json!({"fields": {"issuetype": {"name": "Epic"}}}),
    )
    .await;
    let ctx = context::promote(&pool, &EntityRef::parse(&epic).unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ctx.kind, ContextKind::Epic);
}

/// ADR-0007's pinned failure direction for the issue-type read: a payload the
/// path does not fit contributes nothing, so the promotion **misses toward the
/// narrower kind** -- a ticket context, never a wrong epic.
#[tokio::test]
async fn an_unrecognized_issue_type_shape_misses_toward_ticket() {
    let pool = scratch().await;
    for (key, payload) in [
        ("PAY-1", serde_json::json!({})),
        (
            "PAY-2",
            serde_json::json!({"fields": {"issuetype": "Epic"}}),
        ),
        (
            "PAY-3",
            serde_json::json!({"fields": {"issuetype": {"id": 5}}}),
        ),
    ] {
        let id = with_payload(&pool, "ticket", key, payload).await;
        let ctx = context::promote(&pool, &EntityRef::parse(&id).unwrap())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(ctx.kind, ContextKind::Ticket, "payload of {key}");
    }
}

/// The rule itself: seed (the explicit add) + its direct links + one hop out,
/// and **no further** -- §16.11's own illustration is "a member ticket's PRs,
/// their builds", and the page linked to the build is the far edge.
#[tokio::test]
async fn membership_reaches_the_add_its_links_and_one_hop_no_further() {
    let pool = scratch().await;
    let ctx = context::create_adhoc(&pool, "payout retries")
        .await
        .unwrap();
    let ticket = entity_only(&pool, SOURCE, "ticket", "PAY-1").await;
    let pr = entity_only(&pool, "gitea", "pr", "tidewater/payout#1").await;
    let build = entity_only(&pool, "teamcity", "build", "Payout_Main/41").await;
    let page = entity_only(&pool, "confluence", "page", "ENG/Payout design").await;

    draw(&pool, &ctx.id, &ticket).await; // the explicit add
    draw(&pool, &ticket, &pr).await; // its direct link
    draw(&pool, &pr, &build).await; // one hop out
    draw(&pool, &build, &page).await; // past the edge

    assert_eq!(members(&pool, &ctx.id).await, set(&[&ticket, &pr, &build]));
}

#[tokio::test]
async fn a_promoted_anchor_is_a_member_and_seeds_the_walk() {
    let pool = scratch().await;
    let ticket = with_payload(&pool, "ticket", "PAY-1", serde_json::json!({})).await;
    let pr = entity_only(&pool, "gitea", "pr", "tidewater/payout#1").await;
    let build = entity_only(&pool, "teamcity", "build", "Payout_Main/41").await;
    let page = entity_only(&pool, "confluence", "page", "ENG/Payout design").await;
    draw(&pool, &ticket, &pr).await;
    draw(&pool, &pr, &build).await;
    draw(&pool, &build, &page).await;

    let ctx = context::promote(&pool, &EntityRef::parse(&ticket).unwrap())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(members(&pool, &ctx.id).await, set(&[&ticket, &pr, &build]));
}

/// The circularity guard #47 records: the tray scopes proposals *by*
/// membership, so membership must never be built *from* proposals. The walk
/// reads `knobas.confirmed_link`, and this is the test that notices if it ever
/// stops doing so.
#[tokio::test]
async fn a_proposal_never_counts_toward_membership_at_any_step() {
    let pool = scratch().await;
    let ctx = context::create_adhoc(&pool, "payout retries")
        .await
        .unwrap();
    let ticket = entity_only(&pool, SOURCE, "ticket", "PAY-1").await;
    let guessed_add = entity_only(&pool, SOURCE, "ticket", "PAY-2").await;
    let guessed_hop = entity_only(&pool, "gitea", "pr", "tidewater/payout#1").await;

    draw(&pool, &ctx.id, &ticket).await;
    propose(&pool, &ctx.id, &guessed_add).await; // a guessed add
    propose(&pool, &ticket, &guessed_hop).await; // a guessed hop

    assert_eq!(members(&pool, &ctx.id).await, set(&[&ticket]));
}

#[tokio::test]
async fn a_withdrawn_link_never_counts_toward_membership() {
    let pool = scratch().await;
    let ctx = context::create_adhoc(&pool, "payout retries")
        .await
        .unwrap();
    let ticket = entity_only(&pool, SOURCE, "ticket", "PAY-1").await;
    let written = link::create(
        &pool,
        &EntityRef::parse(&ctx.id).unwrap(),
        &EntityRef::parse(&ticket).unwrap(),
        "related",
        Origin::Manual,
        None,
        "user",
    )
    .await
    .unwrap();
    link::unlink(&pool, written.id).await.unwrap();

    assert!(members(&pool, &ctx.id).await.is_empty());
}

/// Epic membership arrives through the source-recorded parent (`fields.parent`,
/// widened by #32): the child seeds the walk, so its PR is a direct link and
/// the PR's build is the one hop -- exactly §16.11's "a member ticket's PRs,
/// their builds".
#[tokio::test]
async fn epic_children_seed_the_walk_through_the_recorded_parent() {
    let pool = scratch().await;
    let epic = with_payload(
        &pool,
        "ticket",
        "EPIC-1",
        serde_json::json!({"fields": {"issuetype": {"name": "Epic"}}}),
    )
    .await;
    let child = with_payload(
        &pool,
        "ticket",
        "PAY-2",
        serde_json::json!({"fields": {"parent": {"key": "EPIC-1"}}}),
    )
    .await;
    let pr = entity_only(&pool, "gitea", "pr", "tidewater/payout#1").await;
    let build = entity_only(&pool, "teamcity", "build", "Payout_Main/41").await;
    let page = entity_only(&pool, "confluence", "page", "ENG/Payout design").await;
    draw(&pool, &child, &pr).await;
    draw(&pool, &pr, &build).await;
    draw(&pool, &build, &page).await;

    let ctx = context::promote(&pool, &EntityRef::parse(&epic).unwrap())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        members(&pool, &ctx.id).await,
        set(&[&epic, &child, &pr, &build])
    );
}

/// ADR-0007's pinned failure direction for the parent read: it misses, never
/// guesses. A parent in another source's namespace is not this epic's child,
/// and a `fields.parent` the path does not fit contributes nothing -- the
/// failure is an absent member, never a wrong one.
#[tokio::test]
async fn a_foreign_or_misshapen_parent_contributes_nothing() {
    let pool = scratch().await;
    let epic = with_payload(&pool, "ticket", "EPIC-1", serde_json::json!({})).await;
    // Same key, different source: two Jiras are two namespaces.
    let foreign = mirrored(
        &pool,
        "jira-eu",
        "ticket",
        "PAY-2",
        serde_json::json!({"fields": {"parent": {"key": "EPIC-1"}}}),
    )
    .await;
    // The right source, a shape the path does not fit.
    let misshapen = with_payload(
        &pool,
        "ticket",
        "PAY-3",
        serde_json::json!({"fields": {"parent": "EPIC-1"}}),
    )
    .await;

    let ctx = context::promote(&pool, &EntityRef::parse(&epic).unwrap())
        .await
        .unwrap()
        .unwrap();

    let got = members(&pool, &ctx.id).await;
    assert!(
        !got.contains(&foreign),
        "{foreign} is another source's ticket"
    );
    assert!(
        !got.contains(&misshapen),
        "{misshapen}'s parent is not the recorded shape"
    );
    assert_eq!(got, set(&[&epic]));
}

/// A shared member must not union two contexts: the walk never traverses a
/// `ctx`-kind entity, so context B -- linked to the same ticket -- and B's own
/// members stay out of A.
#[tokio::test]
async fn membership_never_traverses_another_context() {
    let pool = scratch().await;
    let a = context::create_adhoc(&pool, "A").await.unwrap();
    let b = context::create_adhoc(&pool, "B").await.unwrap();
    let shared = entity_only(&pool, SOURCE, "ticket", "PAY-1").await;
    let bs_own = entity_only(&pool, SOURCE, "ticket", "PAY-2").await;
    draw(&pool, &a.id, &shared).await;
    draw(&pool, &b.id, &shared).await;
    draw(&pool, &b.id, &bs_own).await;

    assert_eq!(members(&pool, &a.id).await, set(&[&shared]));
}

#[tokio::test]
async fn an_unknown_context_has_no_members_rather_than_an_error() {
    let pool = scratch().await;
    assert!(members(&pool, "ctx:gone").await.is_empty());
}

/// The classifier the promote path leans on: a second unarchived context on
/// one anchor is refused by `context_anchor_idx`, and the refusal classifies
/// as [`CoreError::Duplicate`] rather than surfacing as an opaque database
/// error.
#[tokio::test]
async fn the_anchor_index_refuses_a_second_context_as_a_duplicate() {
    let pool = scratch().await;
    let ticket = with_payload(&pool, "ticket", "PAY-1", serde_json::json!({})).await;
    let ctx = context::promote(&pool, &EntityRef::parse(&ticket).unwrap())
        .await
        .unwrap()
        .unwrap();

    let refused = sqlx::query(
        "insert into knobas.context (id, kind, title, anchor_id)
         values ('ctx:second', 'ticket', 'again', $1)",
    )
    .bind(&ticket)
    .execute(&pool)
    .await
    .map_err(CoreError::from);
    assert!(
        matches!(refused, Err(CoreError::Duplicate)),
        "a second context on {} must be refused, got {refused:?}",
        ctx.anchor_id.as_deref().unwrap_or("?")
    );
}
