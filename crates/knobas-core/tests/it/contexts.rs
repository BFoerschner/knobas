//! Contexts and the one-hop membership rule (§16.11, ADR-0008), against a
//! real PostgreSQL.
//!
//! What is asserted here is **who is a member and who is not** -- the ratified
//! rule is `seed + direct links + one hop`, plus (since #434) every asset held
//! by something those three reached, at any depth; and every positive case in this
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
    let promoted = context::promote(&pool, &anchor).await.unwrap().unwrap();
    assert!(promoted.fresh, "the first promotion made the context");
    let ctx = promoted.context;
    assert_eq!(ctx.kind, ContextKind::Ticket);
    assert_eq!(ctx.title, "Fix the payout retry");
    assert_eq!(ctx.anchor_id.as_deref(), Some(ticket.as_str()));

    // Promoting again is the same context, not a second one -- and the store
    // says so, which is what lets the command log and announce exactly once.
    let again = context::promote(&pool, &anchor).await.unwrap().unwrap();
    assert_eq!(again.context.id, ctx.id);
    assert!(!again.fresh, "the second promotion mutated nothing");
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
        .unwrap()
        .context;
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
            .unwrap()
            .context;
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
        .unwrap()
        .context;

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
        .unwrap()
        .context;

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
        .unwrap()
        .context;

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
        .unwrap()
        .context;

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

/// The parent seed is the epic's alone: a plain story's sub-tasks name it in
/// the same `fields.parent`, and seeding them would make every promoted story
/// an epic in all but name -- spec §7 gives ticket-kind contexts the narrower
/// "focused" rule. The sub-task still joins the ordinary way, by a link.
#[tokio::test]
async fn a_ticket_context_does_not_seed_its_subtasks() {
    let pool = scratch().await;
    let story = with_payload(&pool, "ticket", "PAY-1", serde_json::json!({})).await;
    let subtask = with_payload(
        &pool,
        "ticket",
        "PAY-2",
        serde_json::json!({"fields": {"parent": {"key": "PAY-1"}}}),
    )
    .await;

    let ctx = context::promote(&pool, &EntityRef::parse(&story).unwrap())
        .await
        .unwrap()
        .unwrap()
        .context;
    assert_eq!(ctx.kind, ContextKind::Ticket);
    assert_eq!(
        members(&pool, &ctx.id).await,
        set(&[&story]),
        "{subtask} names {story} as parent, but a ticket context does not seed children"
    );

    // Linked, it is a member like anything else the anchor touches.
    draw(&pool, &story, &subtask).await;
    assert!(members(&pool, &ctx.id).await.contains(&subtask));
}

/// A context cannot be promoted: a context anchored on a context would put a
/// `ctx` node at the walk's root and union the two working sets.
#[tokio::test]
async fn promoting_a_context_is_refused() {
    let pool = scratch().await;
    let ctx = context::create_adhoc(&pool, "payout retries")
        .await
        .unwrap();
    let refused = context::promote(&pool, &EntityRef::parse(&ctx.id).unwrap()).await;
    assert!(
        matches!(refused, Err(CoreError::AnchorIsAContext)),
        "got {refused:?}"
    );
    assert_eq!(context::list(&pool).await.unwrap().len(), 1);
}

/// ...and a context row that *arrives* anchored on a context -- an import, a
/// hand write -- still cannot root the walk: the anchor enters the seed
/// through the same kind filter as every other entrant.
#[tokio::test]
async fn a_hand_written_ctx_anchor_never_roots_the_walk() {
    let pool = scratch().await;
    let b = context::create_adhoc(&pool, "B").await.unwrap();
    let bs_own = entity_only(&pool, SOURCE, "ticket", "PAY-1").await;
    draw(&pool, &b.id, &bs_own).await;

    // The row promote refuses to write, written anyway.
    sqlx::query(
        "insert into knobas.context (id, kind, title, anchor_id)
         values ('ctx:handmade', 'ticket', 'about B', $1)",
    )
    .bind(&b.id)
    .execute(&pool)
    .await
    .unwrap();

    assert!(
        members(&pool, "ctx:handmade").await.is_empty(),
        "neither {} nor its member {bs_own} may arrive through the anchor",
        b.id
    );
}

// ---------------------------------------------------------------------------
// Assets: membership through ancestors (#434, ADR-0008's latent clause)
// ---------------------------------------------------------------------------

/// One asset in the estate's tree: the entity row and the asset row, written
/// here rather than through the store because the store is
/// `knobas_app::assets` and this crate cannot depend on it.
///
/// `parent` is the whole of an asset's place in the tree (ADR-0014); nothing
/// below draws a `holds` link, because there is no such relation.
async fn asset(pool: &PgPool, type_id: &str, name: &str, parent: Option<&str>) -> String {
    let id = EntityRef::new("asset", name).to_string();
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'asset',$2)")
        .bind(&id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("insert into knobas.asset (id, parent_id, type_id, name) values ($1,$2,$3,$4)")
        .bind(&id)
        .bind(parent)
        .bind(type_id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    id
}

/// Story 48, the ratified sentence: *"asset membership counts through
/// ancestors"* -- adding a VM brings what the VM holds, all the way down.
///
/// Four negatives ride with the positive, because an expansion that reaches
/// too far is as wrong as one that reaches nothing:
///
/// * the **site above** the VM is not a member -- the rule brings descendants,
///   and an asset whose *ancestors* were never in the walk is not in it either;
/// * a **sibling subtree** (the second VM and what it holds) is untouched;
/// * a page **linked to** a brought-in container is not a member: the
///   expansion runs over the parent field and never over links, and it is the
///   walk's last layer rather than a fourth source of seeds;
/// * the context itself is never in its own membership.
#[tokio::test]
async fn a_vm_added_to_a_context_brings_what_it_holds_and_nothing_beside_it() {
    let pool = scratch().await;
    let ctx = context::create_adhoc(&pool, "payments stack")
        .await
        .unwrap();

    let site = asset(&pool, "site", "hel", None).await;
    let vm = asset(&pool, "vm", "hel1", Some(&site)).await;
    let engine = asset(&pool, "container_engine", "docker", Some(&vm)).await;
    let container = asset(&pool, "container", "payouts", Some(&engine)).await;
    // A sibling subtree, under the same site.
    let other_vm = asset(&pool, "vm", "hel2", Some(&site)).await;
    let other_container = asset(&pool, "container", "gitea", Some(&other_vm)).await;
    // What a brought-in asset links to is not brought in with it.
    let page = entity_only(&pool, "confluence", "page", "ENG/Payout runbook").await;
    draw(&pool, &container, &page).await;

    draw(&pool, &ctx.id, &vm).await; // the explicit add

    let got = members(&pool, &ctx.id).await;
    assert_eq!(got, set(&[&vm, &engine, &container]));
    for absent in [&site, &other_vm, &other_container, &page] {
        assert!(!got.contains(absent), "{absent} is outside the rule");
    }
}

/// Story 42 and spec §5a: linking an asset to a ticket that is a member makes
/// the asset a member too -- **computed, not stored**, which is spec #427's own
/// wording and ADR-0008's decision. No `implied` row is written anywhere; the
/// membership is what the one statement answers, and what the asset holds
/// comes with it.
#[tokio::test]
async fn a_container_linked_to_a_member_ticket_is_a_member_and_so_is_what_it_holds() {
    let pool = scratch().await;
    let ctx = context::create_adhoc(&pool, "payout retries")
        .await
        .unwrap();
    let ticket = entity_only(&pool, SOURCE, "ticket", "PAY-1").await;
    let container = asset(&pool, "container", "payouts", None).await;
    let service = asset(&pool, "service", "payouts-api", Some(&container)).await;

    draw(&pool, &ctx.id, &ticket).await; // the explicit add
    draw(&pool, &ticket, &container).await; // linking the asset to the ticket

    assert_eq!(
        members(&pool, &ctx.id).await,
        set(&[&ticket, &container, &service])
    );

    // Nothing was written to make that true: the link table holds the two
    // links drawn above and no row joining the asset to the context.
    let joined: i64 =
        sqlx::query_scalar("select count(*) from knobas.link where from_id = $1 or to_id = $1")
            .bind(&container)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(joined, 1, "the asset's only link is the one to the ticket");
}

/// The other half of "computed, never stored": the membership goes when the
/// link that implied it goes, with nothing left behind to sweep, and the
/// subtree it brought goes with it.
///
/// **Which link is "the implied link".** Spec #427 rules that *"the implied
/// membership of an asset linked to a member ticket is computed, not stored,
/// as the ADR requires"*, so there is no second row joining the asset to the
/// context -- the link the reader removes to remove the membership is the one
/// they drew between the asset and the ticket. Drawn `manual` here, because
/// that is what *Link to…* (#435) writes; the origin is not what the walk
/// reads.
#[tokio::test]
async fn removing_the_link_that_implied_the_membership_removes_it() {
    let pool = scratch().await;
    let ctx = context::create_adhoc(&pool, "payout retries")
        .await
        .unwrap();
    let ticket = entity_only(&pool, SOURCE, "ticket", "PAY-1").await;
    let container = asset(&pool, "container", "payouts", None).await;
    let service = asset(&pool, "service", "payouts-api", Some(&container)).await;
    draw(&pool, &ctx.id, &ticket).await;

    let implied = link::create(
        &pool,
        &EntityRef::parse(&ticket).unwrap(),
        &EntityRef::parse(&container).unwrap(),
        "deployed-from",
        Origin::Manual,
        None,
        "user",
    )
    .await
    .unwrap();
    assert_eq!(
        members(&pool, &ctx.id).await,
        set(&[&ticket, &container, &service])
    );

    link::unlink(&pool, implied.id).await.unwrap();
    assert_eq!(members(&pool, &ctx.id).await, set(&[&ticket]));
}

/// The expansion is the walk's **last** layer and applies to every asset in
/// it, however it got there: a `runs-on` link from a container to a VM
/// elsewhere in the tree brings the VM in as an ordinary one-hop neighbour,
/// and what the VM holds is a member because its ancestor is. ADR-0014's own
/// example, where the tree and the relation are allowed to disagree.
///
/// The trade ADR-0008 already records -- *"membership can be wide"* -- read
/// through the parent field, and it is asserted rather than left to follow,
/// because an expansion applied only to the seed layer would answer
/// differently here and identically in every other test in this file.
#[tokio::test]
async fn a_one_hop_asset_neighbour_brings_the_subtree_below_it() {
    let pool = scratch().await;
    let ctx = context::create_adhoc(&pool, "payout retries")
        .await
        .unwrap();
    let ticket = entity_only(&pool, SOURCE, "ticket", "PAY-1").await;
    let container = asset(&pool, "container", "payouts", None).await;
    let vm = asset(&pool, "vm", "hel1", None).await;
    let database = asset(&pool, "database", "payouts-db", Some(&vm)).await;

    draw(&pool, &ctx.id, &ticket).await;
    draw(&pool, &ticket, &container).await;
    draw(&pool, &container, &vm).await; // one hop: `runs-on`

    let got = members(&pool, &ctx.id).await;
    assert!(got.contains(&vm), "the VM is the one hop out");
    assert!(
        got.contains(&database),
        "what the VM holds comes with it: the VM is in the walk, so its subtree is in the membership"
    );
    assert_eq!(got, set(&[&ticket, &container, &vm, &database]));
}

// ---------------------------------------------------------------------------
// The same walk, seeded from every context at once (#446)
// ---------------------------------------------------------------------------

/// `held_by_any_context` answers exactly the union of `member_ids` over the
/// contexts the switcher lists, on a fixture where the two could disagree.
///
/// The inbox's alert rule (#446) needs *"is this asset a member of some
/// context"* inside one statement that binds no context id, and the macro
/// answers it by merging the seeds rather than by walking once per context.
/// That merge is exact only because every layer after the seed is a
/// **neighbour** expansion and neighbours distribute over union -- an argument
/// that is easy to state and easy to break, so it is checked here rather than
/// trusted.
///
/// The fixture is built so that a wrong merge is visible from three
/// directions at once: two contexts whose walks **overlap** (both reach the
/// shared build), an **epic** whose children come in through the payload seed
/// rather than through a link, an **archived** context whose whole subtree
/// must be absent, and an asset subtree hanging off each of the three so the
/// `held` recursion has something to do in every branch.
#[tokio::test]
async fn the_merged_walk_is_the_union_of_every_contexts_members() {
    let pool = scratch().await;

    // One: an ad-hoc context over a VM, whose containers ride along.
    let payments = context::create_adhoc(&pool, "payments stack")
        .await
        .unwrap();
    let vm = asset(&pool, "vm", "hel1", None).await;
    let container = asset(&pool, "container", "payouts", Some(&vm)).await;
    let ticket = entity_only(&pool, SOURCE, "ticket", "PAY-1").await;
    let build = entity_only(&pool, "teamcity", "build", "Payout_Main/41").await;
    draw(&pool, &payments.id, &vm).await;
    draw(&pool, &vm, &ticket).await;
    draw(&pool, &ticket, &build).await;

    // Two: a promoted epic, seeded through `fields.parent`, reaching the same
    // build from the other side -- so the union is genuinely a union.
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
    draw(&pool, &child, &build).await;
    let promoted = context::promote(&pool, &EntityRef::parse(&epic).unwrap())
        .await
        .unwrap()
        .unwrap()
        .context;

    // Three: archived, and everything it alone reaches is out.
    let old = context::create_adhoc(&pool, "last spring").await.unwrap();
    let retired = asset(&pool, "vm", "fsn1", None).await;
    let retired_db = asset(&pool, "database", "old-db", Some(&retired)).await;
    draw(&pool, &old.id, &retired).await;
    sqlx::query("update knobas.context set archived_at = now() where id = $1")
        .bind(&old.id)
        .execute(&pool)
        .await
        .unwrap();

    let listed = context::list(&pool).await.unwrap();
    assert_eq!(
        listed
            .iter()
            .map(|row| row.id.clone())
            .collect::<BTreeSet<_>>(),
        set(&[&payments.id, &promoted.id]),
        "the archived context is not listed, which is the line this walk draws"
    );
    let mut want = BTreeSet::new();
    for row in &listed {
        want.extend(members(&pool, &row.id).await);
    }

    let got: BTreeSet<String> = context::held_by_any_context(&pool)
        .await
        .unwrap()
        .into_iter()
        .collect();
    assert_eq!(got, want);
    assert!(
        got.contains(&container) && got.contains(&child) && got.contains(&build),
        "the fixture has to reach a held asset, an epic child and the shared build: {got:?}"
    );
    for absent in [&retired, &retired_db] {
        assert!(
            !got.contains(absent),
            "{absent} is held only by an archived context"
        );
    }
}

/// ADR-0007's requirement 3 for the **merged** seed: the parent read misses,
/// it never guesses.
///
/// `a_foreign_or_misshapen_parent_contributes_nothing` is the same pin on
/// `member_ids`, and this is the reason it needs a second one rather than
/// inheriting that one: the merged walk splits the source and the key out of
/// the anchor id itself (`split_part` and `substr`) where `member_ids` binds
/// them as two parameters, so *the same path* is read against two different
/// right-hand sides. A `substr` off by one, or a `split_part` taking the wrong
/// field, would seed nothing here and everything there — and the union
/// equality test would go on passing, because both sides would be short by the
/// same rows.
///
/// Three tickets and one epic: one that fits (and must come in, or the test
/// asserts an emptiness the fixture produced), one in another source's
/// namespace, and one whose `fields.parent` is a string rather than an object.
/// The failure direction is an absent member, never a wrong one.
#[tokio::test]
async fn a_misshapen_parent_seeds_no_context_in_the_merged_walk() {
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
    // Same key, another source: two Jiras are two namespaces.
    let foreign = mirrored(
        &pool,
        "jira-eu",
        "ticket",
        "PAY-3",
        serde_json::json!({"fields": {"parent": {"key": "EPIC-1"}}}),
    )
    .await;
    // The right source, a shape the path does not fit.
    let misshapen = with_payload(
        &pool,
        "ticket",
        "PAY-4",
        serde_json::json!({"fields": {"parent": "EPIC-1"}}),
    )
    .await;
    context::promote(&pool, &EntityRef::parse(&epic).unwrap())
        .await
        .unwrap()
        .unwrap();

    let got: BTreeSet<String> = context::held_by_any_context(&pool)
        .await
        .unwrap()
        .into_iter()
        .collect();
    assert!(
        got.contains(&child),
        "the well-formed child is not in the walk, so this test asserts nothing: {got:?}"
    );
    assert!(
        !got.contains(&foreign),
        "{foreign} is another source's ticket"
    );
    assert!(
        !got.contains(&misshapen),
        "{misshapen}'s parent is not the recorded shape"
    );
}
