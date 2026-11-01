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
    let pool = knobas_db::test_util::scratch_database("reach")
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database");
    declare(&pool, &["fields", "issuetype", "name"]).await;
    pool
}

/// The source `jira`, configured with the adapter `jira`, and that adapter's
/// stored ticket declaration naming `issue_type` at `path` (#2364): the walk
/// reads a parent's type through the database's stored copy, as bring-up and
/// every search command write it (`sync.parent_declaration`).
async fn declare(pool: &PgPool, path: &[&str]) {
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ($1, $1, $1, 'http://127.0.0.1:1', 'pat')
         on conflict (id) do nothing",
    )
    .bind(SOURCE)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into sync.parent_declaration (adapter_kind, payload_paths) values ($1, $2)
         on conflict (adapter_kind) do update set payload_paths = excluded.payload_paths",
    )
    .bind(SOURCE)
    .bind(serde_json::json!([{ "kind": "ticket", "issue_type": [path] }]))
    .execute(pool)
    .await
    .unwrap();
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

/// R-4.17, ADR-0025 as amended on #942: from a VM, a proxy or a host the walk
/// only goes further down. A ticket linked to a container reaches the engine,
/// the VM and the host it runs on, and the monitors of each, all via the
/// container -- and never the other service on the same machine, nor what
/// links to the VM.
#[tokio::test]
async fn a_container_brings_its_vm_its_host_and_their_monitors_via_itself() {
    let pool = scratch().await;
    let pay = ticket(&pool, "PAY-231").await;
    let host = asset(&pool, "hypervisor", "hv-01", None).await;
    let vm = asset(&pool, "vm", "vm-pay-01", Some(&host)).await;
    let engine = asset(&pool, "container_engine", "docker-pay", Some(&vm)).await;
    let mine = asset(&pool, "container", "payout-service", Some(&engine)).await;
    let other = asset(&pool, "container", "ledger-api", Some(&engine)).await;
    let vm_ping = entity(&pool, "kuma", "monitor", "vm-pay-01-ping").await;
    let host_ping = entity(&pool, "kuma", "monitor", "hv-01-ping").await;
    let other_ping = entity(&pool, "kuma", "monitor", "ledger-api-http").await;
    let stranger = ticket(&pool, "OPS-77").await;
    draw(&pool, &pay, &mine, "deploys").await;
    draw(&pool, &vm, &vm_ping, "monitored-by").await;
    draw(&pool, &host, &host_ping, "monitored-by").await;
    draw(&pool, &other, &other_ping, "monitored-by").await;
    draw(&pool, &stranger, &vm, "related").await;

    let got = reached(&pool, &pay).await;
    for down in [&mine, &engine, &vm, &host, &vm_ping, &host_ping] {
        assert!(got.contains_key(down), "{down} is down from the ticket");
        assert_eq!(got[down].step, mine, "{down} came via the container");
        assert_eq!(got[down].origin, Origin::Own);
    }
    for beside in [&other, &other_ping, &stranger] {
        assert!(
            !got.contains_key(beside),
            "{beside} is beside the container or up from its VM"
        );
    }
}

/// The same rule at the link layers: with a VM and a proxy one link from the
/// ticket, the next link step out of them is taken only toward what they
/// stand on and toward their monitors -- never to a ticket that links to the
/// VM, a container that runs on it or a route the proxy serves.
#[tokio::test]
async fn the_walk_never_goes_up_from_a_vm_a_proxy_or_a_host() {
    let pool = scratch().await;
    let pay = ticket(&pool, "PAY-231").await;
    let host = asset(&pool, "hypervisor", "hv-01", None).await;
    let vm = asset(&pool, "vm", "vm-pay-01", Some(&host)).await;
    let on_vm = asset(&pool, "container", "ledger-api", Some(&vm)).await;
    let nas = asset(&pool, "vm", "nas-01", None).await;
    let monitor = entity(&pool, "kuma", "monitor", "vm-pay-01-ping").await;
    let stranger = ticket(&pool, "OPS-77").await;
    let proxy = asset(&pool, "reverse_proxy", "traefik", None).await;
    let elsewhere = asset(&pool, "container", "flows-stage", None).await;
    let hosted = asset(&pool, "vm", "vm-ops-01", Some(&host)).await;

    draw(&pool, &pay, &vm, "related").await;
    draw(&pool, &pay, &proxy, "related").await;
    // Down from the VM: its monitor, and what it depends on.
    draw(&pool, &vm, &monitor, "monitored-by").await;
    draw(&pool, &vm, &nas, "depends-on").await;
    // Up from the VM and the proxy: another ticket that links to the VM, and a
    // container that runs on it and is served by the proxy.
    draw(&pool, &stranger, &vm, "related").await;
    draw(&pool, &elsewhere, &vm, "runs-on").await;
    draw(&pool, &elsewhere, &proxy, "served-by").await;

    let got = reached(&pool, &pay).await;
    for down in [&vm, &proxy, &monitor, &nas, &host] {
        assert!(got.contains_key(down), "{down} is down from the ticket");
    }
    for up in [&stranger, &elsewhere, &on_vm, &hosted] {
        assert!(!got.contains_key(up), "{up} is up from a VM, proxy or host");
    }
    assert_eq!(got[&monitor].step, vm);
    assert_eq!(got[&host].step, vm, "the host is under the VM");
}

/// The downward rule also stops containment: a VM linked to the ticket does
/// not bring in what runs on it, while an ordinary asset still brings in what
/// it holds, as membership does.
#[tokio::test]
async fn nothing_held_under_a_vm_comes_in_but_a_service_brings_its_modules() {
    let pool = scratch().await;
    let pay = ticket(&pool, "PAY-231").await;
    let vm = asset(&pool, "vm", "vm-pay-01", None).await;
    let on_it = asset(&pool, "container", "payout-service", Some(&vm)).await;
    let service = asset(&pool, "service", "ledger", None).await;
    let module = asset(&pool, "module", "ledger-core", Some(&service)).await;
    draw(&pool, &pay, &vm, "related").await;
    draw(&pool, &pay, &service, "related").await;

    let got = reached(&pool, &pay).await;
    assert!(got.contains_key(&vm));
    assert!(!got.contains_key(&on_it), "up from the VM");
    assert_eq!(got[&module].step, service, "held under an ordinary asset");
}

/// Membership is unchanged by this read (ADR-0025, ADR-0008): a promoted
/// ticket's context still holds what runs on a linked VM and what links to
/// it, while the reach leaves both out; the context's explicit adds seed the
/// reach, and with no VM, proxy or host on the way the reach is membership
/// plus the parent's own links.
#[tokio::test]
async fn membership_stays_adr_0008s_and_the_context_seeds_the_reach() {
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
    draw(&pool, &sub, &vm, "deploys").await;
    draw(&pool, &beside, &vm, "related").await;
    draw(&pool, &parent, &repo, "works-in").await;
    draw(&pool, &repo, &repo_build, "built-by").await;

    let promoted = context::promote(
        &pool,
        &EntityRef::parse(&sub).unwrap(),
        &knobas_core::payload::Declarations::empty(),
    )
    .await
    .unwrap()
    .unwrap();
    draw(&pool, &promoted.context.id, &added, "related").await;

    let members = context::member_ids(&pool, &promoted.context.id)
        .await
        .unwrap();
    for id in [&pr, &build, &vm, &on_it, &beside, &added] {
        assert!(members.contains(id), "membership still holds {id}");
    }
    assert!(!members.contains(&past) && !members.contains(&repo));

    let got = reached(&pool, &sub).await;
    for up in [&on_it, &beside] {
        assert!(
            !got.contains_key(up),
            "{up} is in the context, but up from the VM"
        );
    }
    let mut want: Vec<String> = members
        .into_iter()
        .filter(|id| *id != sub && *id != on_it && *id != beside)
        .collect();
    want.push(repo.clone());
    want.sort();
    want.dedup();
    assert_eq!(got.keys().cloned().collect::<Vec<_>>(), want);
    assert!(!got.contains_key(&past) && !got.contains_key(&repo_build));
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

/// The downward rule holds from the first link step too: a VM the context
/// adds by hand is a start, so the step out of it is `l1`'s, not `l2`'s. It
/// brings its monitor and its host, never a ticket or a container that links
/// to it.
#[tokio::test]
async fn a_vm_the_context_adds_only_goes_down() {
    let pool = scratch().await;
    let pay = ticket(&pool, "PAY-231").await;
    let host = asset(&pool, "hypervisor", "hv-01", None).await;
    let vm = asset(&pool, "vm", "vm-pay-01", Some(&host)).await;
    let monitor = entity(&pool, "kuma", "monitor", "vm-pay-01-ping").await;
    let stranger = ticket(&pool, "OPS-77").await;
    let elsewhere = asset(&pool, "container", "flows-stage", None).await;
    draw(&pool, &vm, &monitor, "monitored-by").await;
    draw(&pool, &stranger, &vm, "related").await;
    draw(&pool, &elsewhere, &vm, "runs-on").await;

    let promoted = context::promote(
        &pool,
        &EntityRef::parse(&pay).unwrap(),
        &knobas_core::payload::Declarations::empty(),
    )
    .await
    .unwrap()
    .unwrap();
    draw(&pool, &promoted.context.id, &vm, "related").await;

    let got = reached(&pool, &pay).await;
    for down in [&vm, &monitor, &host] {
        assert!(got.contains_key(down), "{down} is down from the added VM");
    }
    for up in [&stranger, &elsewhere] {
        assert!(!got.contains_key(up), "{up} is up from the added VM");
    }
    assert_eq!(got[&monitor].step, vm);
}

/// The parent's own links still count when one is a VM, and stop there: the
/// parent's layer is one link, so nothing under, above or watching that VM
/// comes in through it.
#[tokio::test]
async fn a_parents_vm_comes_in_alone() {
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
    let host = asset(&pool, "hypervisor", "hv-01", None).await;
    let vm = asset(&pool, "vm", "vm-pay-01", Some(&host)).await;
    let monitor = entity(&pool, "kuma", "monitor", "vm-pay-01-ping").await;
    draw(&pool, &parent, &vm, "deploys").await;
    draw(&pool, &vm, &monitor, "monitored-by").await;

    let got = reached(&pool, &sub).await;
    assert_eq!(got.keys().cloned().collect::<Vec<_>>(), vec![vm.clone()]);
    assert_eq!(got[&vm].origin, Origin::Parent { ticket: parent });
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

/// The parent's issue type is read where its kind **declares** it (#2364),
/// not at Jira's `fields.issuetype.name`: under a declaration that keeps it
/// at `type`, the parent whose `type` is a story brings its links and the one
/// whose `type` is an epic brings none, whatever Jira's path says of either.
#[tokio::test]
async fn a_parents_type_is_read_where_its_kind_declares_it() {
    let pool = scratch().await;
    declare(&pool, &["type"]).await;
    let story = mirrored(
        &pool,
        "ticket",
        "PAY-231",
        serde_json::json!({ "key": "PAY-231", "type": "Story",
                            "fields": { "issuetype": { "name": "Epic" } } }),
    )
    .await;
    let epic = mirrored(
        &pool,
        "ticket",
        "PAY-200",
        serde_json::json!({ "key": "PAY-200", "type": "Epic",
                            "fields": { "issuetype": { "name": "Story" } } }),
    )
    .await;
    let under_story = mirrored(
        &pool,
        "ticket",
        "PAY-232",
        serde_json::json!({ "key": "PAY-232", "fields": { "parent": { "key": "PAY-231" } } }),
    )
    .await;
    let under_epic = mirrored(
        &pool,
        "ticket",
        "PAY-233",
        serde_json::json!({ "key": "PAY-233", "fields": { "parent": { "key": "PAY-200" } } }),
    )
    .await;
    let repo = entity(&pool, "gitea", "repo", "tidewater/payout-service").await;
    let page = entity(&pool, "confluence", "page", "Payouts-overview").await;
    draw(&pool, &story, &repo, "works-in").await;
    draw(&pool, &epic, &page, "documents").await;

    assert_eq!(
        reached(&pool, &under_story)
            .await
            .keys()
            .collect::<Vec<_>>(),
        [&repo],
        "a story's links come in"
    );
    assert!(
        reached(&pool, &under_epic).await.is_empty(),
        "an epic's do not"
    );
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

/// A done job is archived (#1501), and opening it is reading it: its reach
/// still starts from what its context holds, as it did while it was live.
#[tokio::test]
async fn an_archived_contexts_adds_are_still_reached() {
    let pool = scratch().await;
    let pay = ticket(&pool, "PAY-501").await;
    let vm = asset(&pool, "vm", "vm-done-01", None).await;
    let promoted = context::promote(
        &pool,
        &EntityRef::parse(&pay).unwrap(),
        &knobas_core::payload::Declarations::empty(),
    )
    .await
    .unwrap()
    .unwrap();
    draw(&pool, &promoted.context.id, &vm, "related").await;
    assert!(reached(&pool, &pay).await.contains_key(&vm));

    sqlx::query("update knobas.context set archived_at = now() where id = $1")
        .bind(&promoted.context.id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        reached(&pool, &pay).await.contains_key(&vm),
        "an archived job still shows what it held"
    );
}
