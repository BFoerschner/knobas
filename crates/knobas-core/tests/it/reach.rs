//! What a ticket reaches (#817, ADR-0025, R-2.8, R-7.3), against a real
//! PostgreSQL.
//!
//! Every positive case is paired with the entity one step past the edge the
//! rule draws, for `contexts.rs`' reason: a walk that reaches everything is as
//! broken as one that reaches nothing, and only the far edge catches the first.

use knobas_core::context;
use knobas_core::entity::EntityRef;
use knobas_core::link::{self, Origin as LinkOrigin};
use knobas_core::reach::{self, Origin, Reached};
use sqlx::PgPool;
use std::collections::BTreeMap;

const SOURCE: &str = "jira";

async fn scratch() -> PgPool {
    knobas_db::test_util::scratch_database("reach")
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database")
}

async fn entity(pool: &PgPool, namespace: &str, kind: &str, key: &str) -> String {
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

async fn mirrored(pool: &PgPool, kind: &str, key: &str, payload: serde_json::Value) -> String {
    let id = entity(pool, SOURCE, kind, key).await;
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,$2,$3,$4,'',$5)",
    )
    .bind(&id)
    .bind(SOURCE)
    .bind(kind)
    .bind(key)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn ticket(pool: &PgPool, key: &str) -> String {
    mirrored(pool, "ticket", key, serde_json::json!({ "key": key })).await
}

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

async fn draw(pool: &PgPool, from: &str, to: &str, relation: &str) {
    link::create(
        pool,
        &EntityRef::parse(from).unwrap(),
        &EntityRef::parse(to).unwrap(),
        relation,
        LinkOrigin::Manual,
        None,
        "user",
    )
    .await
    .unwrap();
}

/// The read, keyed by what was reached.
async fn reached(pool: &PgPool, ticket: &str) -> BTreeMap<String, Reached> {
    reach::reach(pool, ticket)
        .await
        .unwrap()
        .into_iter()
        .map(|row| (row.entity_id.clone(), row))
        .collect()
}

/// The ticket's own links, one more hop, and nothing past it; each row names
/// the first step of its path, and the ticket itself is not in its own reach.
#[tokio::test]
async fn reasons_name_the_first_step_and_the_walk_stops_at_one_hop() {
    let pool = scratch().await;
    let pay = ticket(&pool, "PAY-231").await;
    let pr = entity(&pool, "gitea", "pr", "payout-service#142").await;
    let build = entity(&pool, "teamcity", "build", "Payout_Build#318").await;
    let past = entity(&pool, "teamcity", "build", "Payout_Deploy#9").await;
    let page = entity(&pool, "confluence", "page", "SEPA-retry").await;
    draw(&pool, &pay, &pr, "implements").await;
    draw(&pool, &pr, &build, "built-by").await;
    draw(&pool, &build, &past, "triggers").await;
    draw(&pool, &page, &pay, "documents").await;

    let got = reached(&pool, &pay).await;
    assert_eq!(
        got.keys().cloned().collect::<Vec<_>>(),
        {
            let mut want = vec![pr.clone(), build.clone(), page.clone()];
            want.sort();
            want
        },
        "the ticket's links and one hop past them, never the second hop and never itself"
    );
    assert_eq!(got[&pr].step, pr, "a direct link is its own first step");
    assert_eq!(
        got[&page].step, page,
        "a link drawn at the ticket counts too"
    );
    assert_eq!(got[&build].step, pr, "the build came via the pull request");
    assert_eq!(got[&build].kind, "build");
    assert!(got.values().all(|row| row.origin == Origin::Own));
}

/// ADR-0025: the job's read is membership, so containment goes down from
/// whatever the walk reached, a VM included, and never up to an ancestor; and
/// a link layer out of a VM is taken both ways, as ADR-0008 takes it. The
/// spec's R-4.17 rule ("from a VM, proxy or host only further down") is not
/// this read's: it is a follow-up against the ADR.
#[tokio::test]
async fn a_vm_brings_what_it_holds_and_nothing_brings_its_ancestors() {
    let pool = scratch().await;
    let pay = ticket(&pool, "PAY-231").await;
    let host = asset(&pool, "hypervisor", "hv-01", None).await;
    let vm = asset(&pool, "vm", "vm-pay-01", Some(&host)).await;
    let engine = asset(&pool, "container_engine", "docker-pay", Some(&vm)).await;
    let mine = asset(&pool, "container", "payout-service", Some(&engine)).await;
    let other = asset(&pool, "container", "ledger-api", Some(&engine)).await;
    let monitor = entity(&pool, "kuma", "monitor", "vm-pay-01-ping").await;
    let nas = asset(&pool, "vm", "nas-01", None).await;
    let on_nas = asset(&pool, "container", "backup", Some(&nas)).await;
    let stranger = ticket(&pool, "OPS-77").await;
    draw(&pool, &pay, &mine, "deploys").await;
    draw(&pool, &vm, &monitor, "monitored-by").await;
    draw(&pool, &pay, &nas, "related").await;
    draw(&pool, &stranger, &nas, "related").await;

    let got = reached(&pool, &pay).await;
    for ancestor in [&engine, &vm, &host, &monitor, &other] {
        assert!(
            !got.contains_key(ancestor),
            "{ancestor} is above or beside the container, not held under it"
        );
    }
    assert_eq!(got[&mine].step, mine);
    assert_eq!(got[&on_nas].step, nas, "held under the linked VM");
    assert_eq!(
        got[&stranger].step, nas,
        "one link out of the VM, either way"
    );
}

/// The reach is ADR-0025's sentence exactly: a promoted sub-task's
/// membership (ADR-0008, the same statement the room reads), then the
/// parent's own links, and nothing else.
#[tokio::test]
async fn the_reach_is_membership_plus_the_parents_links() {
    let pool = scratch().await;
    let parent = mirrored(
        &pool,
        "ticket",
        "PAY-231",
        serde_json::json!({ "key": "PAY-231", "fields": { "issuetype": { "name": "Story" } } }),
    )
    .await;
    let sub = mirrored(
        &pool,
        "ticket",
        "PAY-232",
        serde_json::json!({ "key": "PAY-232", "fields": { "parent": { "key": "PAY-231" } } }),
    )
    .await;
    let pr = entity(&pool, "gitea", "pr", "payout-service#142").await;
    let build = entity(&pool, "teamcity", "build", "Payout_Build#318").await;
    let past = entity(&pool, "teamcity", "build", "Payout_Deploy#9").await;
    let vm = asset(&pool, "vm", "vm-pay-01", None).await;
    let on_it = asset(&pool, "container", "payout-service", Some(&vm)).await;
    let beside = ticket(&pool, "OPS-77").await;
    let repo = entity(&pool, "gitea", "repo", "tidewater/payout-service").await;
    let repo_build = entity(&pool, "teamcity", "build", "Psp_Build#1").await;
    let added = entity(&pool, "confluence", "page", "Runbook").await;
    draw(&pool, &sub, &pr, "implements").await;
    draw(&pool, &pr, &build, "built-by").await;
    draw(&pool, &build, &past, "triggers").await;
    draw(&pool, &pr, &vm, "deploys").await;
    draw(&pool, &beside, &vm, "related").await;
    draw(&pool, &parent, &repo, "works-in").await;
    draw(&pool, &repo, &repo_build, "built-by").await;

    let promoted = context::promote(&pool, &EntityRef::parse(&sub).unwrap())
        .await
        .unwrap()
        .unwrap();
    draw(&pool, &promoted.context.id, &added, "related").await;

    let mut want: Vec<String> = context::member_ids(&pool, &promoted.context.id)
        .await
        .unwrap()
        .into_iter()
        .filter(|id| *id != sub)
        .collect();
    want.push(repo.clone());
    want.sort();
    want.dedup();

    let got = reached(&pool, &sub).await;
    assert_eq!(got.keys().cloned().collect::<Vec<_>>(), want);
    assert!(
        got.contains_key(&on_it),
        "membership counts through ancestors"
    );
    assert!(
        !got.contains_key(&beside),
        "two link layers out, then it stops"
    );
    assert!(!got.contains_key(&past) && !got.contains_key(&repo_build));
    assert_eq!(got[&on_it].step, pr);
    assert_eq!(
        got[&added].step, added,
        "an explicit add is its own first step"
    );
    assert_eq!(got[&repo].origin, Origin::Parent { ticket: parent });
    assert!(
        !got.contains_key(&promoted.context.id),
        "a context is never reached"
    );
}

/// R-7.3: a sub-task reaches its parent's own links, labelled with the
/// parent, behind a switch that is on by default -- and its own link wins
/// over the parent's for the same item.
#[tokio::test]
async fn parent_links_come_in_labelled_behind_the_switch() {
    let pool = scratch().await;
    let parent = mirrored(
        &pool,
        "ticket",
        "PAY-231",
        serde_json::json!({ "key": "PAY-231", "fields": { "issuetype": { "name": "Story" } } }),
    )
    .await;
    let sub = mirrored(
        &pool,
        "ticket",
        "PAY-232",
        serde_json::json!({ "key": "PAY-232", "fields": { "parent": { "key": "PAY-231" } } }),
    )
    .await;
    let repo = entity(&pool, "gitea", "repo", "tidewater/payout-service").await;
    let shared = entity(&pool, "confluence", "page", "SEPA-retry").await;
    let past = entity(&pool, "teamcity", "build", "Payout_Build#1").await;
    draw(&pool, &parent, &repo, "works-in").await;
    draw(&pool, &parent, &shared, "documents").await;
    draw(&pool, &repo, &past, "built-by").await;
    draw(&pool, &sub, &shared, "documents").await;

    assert!(reach::parent_links(&pool).await.unwrap(), "on by default");
    let got = reached(&pool, &sub).await;
    assert_eq!(
        got[&repo].origin,
        Origin::Parent {
            ticket: parent.clone()
        }
    );
    assert_eq!(got[&repo].step, parent, "the parent is the first step");
    assert_eq!(
        got[&shared].origin,
        Origin::Own,
        "the sub-task's own link wins"
    );
    assert!(
        !got.contains_key(&past),
        "the parent's own links, and no further"
    );

    assert!(!reach::set_parent_links(&pool, false).await.unwrap());
    let off = reached(&pool, &sub).await;
    assert!(
        !off.contains_key(&repo),
        "switched off, the parent brings nothing"
    );
    assert!(off.contains_key(&shared));

    assert!(reach::set_parent_links(&pool, true).await.unwrap());
    assert!(reached(&pool, &sub).await.contains_key(&repo));
}

/// An epic named as `fields.parent` is not "the parent" of R-7.3: a story
/// under it reaches none of the epic's links through the switch.
#[tokio::test]
async fn an_epic_parent_brings_nothing() {
    let pool = scratch().await;
    let epic = mirrored(
        &pool,
        "ticket",
        "PAY-200",
        serde_json::json!({ "key": "PAY-200", "fields": { "issuetype": { "name": "Epic" } } }),
    )
    .await;
    let story = mirrored(
        &pool,
        "ticket",
        "PAY-231",
        serde_json::json!({ "key": "PAY-231", "fields": { "parent": { "key": "PAY-200" } } }),
    )
    .await;
    let page = entity(&pool, "confluence", "page", "Payouts-overview").await;
    draw(&pool, &epic, &page, "documents").await;

    assert!(reached(&pool, &story).await.is_empty());
}

/// An id nothing answers to reaches nothing, and does not fail.
#[tokio::test]
async fn an_unknown_ticket_reaches_nothing() {
    let pool = scratch().await;
    assert!(reach::reach(&pool, "jira:NOPE-1").await.unwrap().is_empty());
}

/// The miss direction of the epic check (ADR-0007): a parent whose issue
/// type cannot be read might be an epic, so it brings nothing.
#[tokio::test]
async fn a_parent_whose_type_cannot_be_read_brings_nothing() {
    let pool = scratch().await;
    let parent = ticket(&pool, "PAY-231").await;
    let sub = mirrored(
        &pool,
        "ticket",
        "PAY-232",
        serde_json::json!({ "key": "PAY-232", "fields": { "parent": { "key": "PAY-231" } } }),
    )
    .await;
    let repo = entity(&pool, "gitea", "repo", "tidewater/payout-service").await;
    draw(&pool, &parent, &repo, "works-in").await;

    assert!(reached(&pool, &sub).await.is_empty());
}
