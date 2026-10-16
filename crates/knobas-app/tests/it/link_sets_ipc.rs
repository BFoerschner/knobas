//! Link sets (#866, ADR-0025, R-7.1, R-7.4, R-7.5, R-7.11) through the six
//! commands and the job's read, over the embedded PostgreSQL with the
//! declarations the compiled-in Jira adapter makes (`Registry::builtin`),
//! written to `sync.parent_declaration` as the app writes them.
//!
//! This file's database is its own (#572); each test makes keys nothing else
//! holds, under a project of its own.

use knobas_app::IpcErrorCode;
use knobas_app::commands::entity::{
    EntityFilter, EntityOrder, list_entities_inner, promote_context_inner, room_suggestions_inner,
};
use knobas_app::link_sets::{self, LinkSetDraft, LinkSetMember, LinkSetRule};
use knobas_core::reach::{self, Origin};
use sqlx::PgPool;
use std::collections::BTreeMap;

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
         values ('jira', 'jira', 'Jira', 'http://x', 'Pat', '{\"username\": \"mara\"}')
         on conflict (id) do update set kind = excluded.kind, config = excluded.config,
                                        enabled = true",
    )
    .execute(&pool)
    .await
    .unwrap();
    knobas_app::commands::search::searcher(&pool).await.unwrap();
    pool
}

/// A project key nothing else in this database holds.
fn project() -> String {
    let hex = uuid::Uuid::new_v4().simple().to_string();
    let letters: String = hex
        .chars()
        .filter(char::is_ascii_hexdigit)
        .take(6)
        .map(|c| char::from(b'A' + (c.to_digit(16).unwrap() as u8)))
        .collect();
    format!("L{letters}")
}

/// One Jira issue in `project`, under `parent` when given, assigned to
/// `assignee` when given.
async fn issue(
    pool: &PgPool,
    project: &str,
    number: u32,
    parent: Option<&str>,
    assignee: Option<&str>,
) -> String {
    let key = format!("{project}-{number}");
    let id = format!("jira:{key}");
    let mut fields = serde_json::json!({ "project": { "key": project, "name": project } });
    if let Some(parent) = parent {
        let parent_key = parent.trim_start_matches("jira:");
        fields["parent"] = serde_json::json!({ "key": parent_key });
    }
    if let Some(name) = assignee {
        fields["assignee"] = serde_json::json!({ "name": name });
    }
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'ticket',$2)")
        .bind(&id)
        .bind(&key)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at)
         values ($1,'jira','ticket',$2,'',$3, now())",
    )
    .bind(&id)
    .bind(&key)
    .bind(serde_json::json!({ "key": key, "fields": fields }))
    .execute(pool)
    .await
    .unwrap();
    id
}

/// One item knobas knows by id: an entity row, mirrored or not.
async fn thing(pool: &PgPool, kind: &str, name: &str) -> String {
    let id = format!("gitea:{name}-{}", uuid::Uuid::new_v4().simple());
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(&id)
        .bind(kind)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    id
}

async fn link(pool: &PgPool, from: &str, to: &str) {
    sqlx::query(
        "insert into knobas.link (from_id, to_id, relation, origin, created_by)
         values ($1, $2, 'related', 'manual', 'user')",
    )
    .bind(from)
    .bind(to)
    .execute(pool)
    .await
    .unwrap();
}

fn member(kind: &str, entity_id: &str) -> LinkSetMember {
    LinkSetMember {
        kind: kind.to_owned(),
        entity_id: entity_id.to_owned(),
    }
}

fn rule(kind: &str, value: &str) -> LinkSetRule {
    LinkSetRule {
        kind: kind.to_owned(),
        value: value.to_owned(),
    }
}

fn draft(name: &str, members: Vec<LinkSetMember>, rules: Vec<LinkSetRule>) -> LinkSetDraft {
    LinkSetDraft {
        id: None,
        name: name.to_owned(),
        description: String::new(),
        members,
        rules,
    }
}

/// What `ticket` reaches, keyed by entity id.
async fn reached(pool: &PgPool, ticket: &str) -> BTreeMap<String, Origin> {
    reach::reach(pool, ticket)
        .await
        .unwrap()
        .into_iter()
        .map(|row| (row.entity_id, row.origin))
        .collect()
}

/// An epic with a story, a sub-task under the story, and a ticket under
/// another epic: the set on the first epic reaches the story and, two
/// levels down, the sub-task, and labels its rows with its name; the other
/// epic's ticket and the epic itself see nothing of it. A ticket that syncs
/// under the epic afterwards shows the set on its next read (R-7.4).
#[tokio::test]
async fn an_epic_rule_shows_its_members_on_every_ticket_under_the_epic() {
    let pool = pool().await;
    let p = project();
    let epic = issue(&pool, &p, 1, None, None).await;
    let story = issue(&pool, &p, 2, Some(&epic), None).await;
    let sub = issue(&pool, &p, 3, Some(&story), None).await;
    let other_epic = issue(&pool, &p, 4, None, None).await;
    let elsewhere = issue(&pool, &p, 5, Some(&other_epic), None).await;
    let repo = thing(&pool, "repo", "payout-service").await;

    let saved = link_sets::save(
        &pool,
        draft(
            "Payout stack",
            vec![member("repo", &repo)],
            vec![rule("epic", &epic)],
        ),
    )
    .await
    .unwrap();
    let from_set = Origin::LinkSet {
        set: saved.set.id.clone(),
        name: "Payout stack".to_owned(),
    };

    assert_eq!(reached(&pool, &story).await.get(&repo), Some(&from_set));
    assert_eq!(
        reached(&pool, &sub).await.get(&repo),
        Some(&from_set),
        "a sub-task is under its story's epic"
    );
    assert_eq!(reached(&pool, &elsewhere).await.get(&repo), None);
    assert_eq!(
        reached(&pool, &epic).await.get(&repo),
        None,
        "the epic is not under itself"
    );

    let late = issue(&pool, &p, 6, Some(&epic), None).await;
    assert_eq!(reached(&pool, &late).await.get(&repo), Some(&from_set));
}

/// A parent rule is the ticket's direct children only; a project rule every
/// ticket of that project in that source; a manual rule one ticket.
#[tokio::test]
async fn parent_project_and_manual_rules_match_what_they_name() {
    let pool = pool().await;
    let p = project();
    let q = project();
    let story = issue(&pool, &p, 1, None, None).await;
    let child = issue(&pool, &p, 2, Some(&story), None).await;
    let grandchild = issue(&pool, &p, 3, Some(&child), None).await;
    let in_q = issue(&pool, &q, 1, None, None).await;
    let lone = issue(&pool, &p, 4, None, None).await;
    let page = thing(&pool, "page", "Runbook").await;
    let monitor = thing(&pool, "monitor", "payouts-health").await;
    let asset = thing(&pool, "asset", "vm-pay-01").await;

    link_sets::save(
        &pool,
        draft(
            "Children",
            vec![member("page", &page)],
            vec![rule("parent", &story)],
        ),
    )
    .await
    .unwrap();
    link_sets::save(
        &pool,
        draft(
            "Project",
            vec![member("monitor", &monitor)],
            vec![rule("project", &format!("jira:{q}"))],
        ),
    )
    .await
    .unwrap();
    link_sets::save(
        &pool,
        draft(
            "By hand",
            vec![member("asset", &asset)],
            vec![rule("manual", &lone)],
        ),
    )
    .await
    .unwrap();

    assert!(reached(&pool, &child).await.contains_key(&page));
    assert!(
        !reached(&pool, &grandchild).await.contains_key(&page),
        "one level only"
    );
    assert!(reached(&pool, &in_q).await.contains_key(&monitor));
    assert!(
        !reached(&pool, &story).await.contains_key(&monitor),
        "another project"
    );
    assert!(reached(&pool, &lone).await.contains_key(&asset));
    assert!(!reached(&pool, &child).await.contains_key(&asset));
}

/// An item shows once, with its first reason (R-7.4): the ticket's own link
/// before any set, and of two sets the same one on every read.
#[tokio::test]
async fn an_item_shows_once_with_its_first_reason() {
    let pool = pool().await;
    let p = project();
    let ticket = issue(&pool, &p, 1, None, None).await;
    let own = thing(&pool, "repo", "linked-by-hand").await;
    let shared = thing(&pool, "page", "in-two-sets").await;
    link(&pool, &ticket, &own).await;
    link_sets::save(
        &pool,
        draft(
            "B",
            vec![member("repo", &own), member("page", &shared)],
            vec![rule("manual", &ticket)],
        ),
    )
    .await
    .unwrap();
    let a = link_sets::save(
        &pool,
        draft(
            "A",
            vec![member("page", &shared)],
            vec![rule("manual", &ticket)],
        ),
    )
    .await
    .unwrap();

    let rows = reach::reach(&pool, &ticket).await.unwrap();
    assert_eq!(rows.iter().filter(|r| r.entity_id == own).count(), 1);
    assert_eq!(rows.iter().filter(|r| r.entity_id == shared).count(), 1);
    let got = reached(&pool, &ticket).await;
    assert_eq!(got[&own], Origin::Own, "the ticket's own link comes first");
    let first = got[&shared].clone();
    for _ in 0..3 {
        assert_eq!(reached(&pool, &ticket).await[&shared], first);
    }
    assert_eq!(
        first,
        Origin::LinkSet {
            set: a.set.id.clone(),
            name: "A".to_owned()
        },
        "of two sets, the one whose id sorts first"
    );
}

/// Editing a set changes every job at once (R-7.4): nothing was copied onto
/// a ticket, so a member added or removed is on, or off, both tickets' next
/// read; deleting the set takes it off both, and leaves hand-drawn links.
#[tokio::test]
async fn editing_a_set_changes_every_job_at_once() {
    let pool = pool().await;
    let p = project();
    let epic = issue(&pool, &p, 1, None, None).await;
    let one = issue(&pool, &p, 2, Some(&epic), None).await;
    let two = issue(&pool, &p, 3, Some(&epic), None).await;
    let first = thing(&pool, "repo", "first").await;
    let second = thing(&pool, "monitor", "second").await;
    let own = thing(&pool, "page", "own").await;
    link(&pool, &one, &own).await;
    let saved = link_sets::save(
        &pool,
        draft(
            "Stack",
            vec![member("repo", &first)],
            vec![rule("epic", &epic)],
        ),
    )
    .await
    .unwrap();

    let mut edit = draft(
        "Stack",
        vec![member("monitor", &second)],
        vec![rule("epic", &epic)],
    );
    edit.id = Some(saved.set.id.clone());
    link_sets::save(&pool, edit).await.unwrap();
    for ticket in [&one, &two] {
        let got = reached(&pool, ticket).await;
        assert!(got.contains_key(&second), "{ticket} sees the added member");
        assert!(
            !got.contains_key(&first),
            "{ticket} no longer sees the removed one"
        );
    }
    let links: i64 = sqlx::query_scalar(
        "select count(*) from knobas.link where from_id in ($1, $2) or to_id in ($1, $2)",
    )
    .bind(&one)
    .bind(&two)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(links, 1, "only the hand-drawn link: a set writes none");

    link_sets::delete(&pool, &saved.set.id).await.unwrap();
    assert!(!reached(&pool, &two).await.contains_key(&second));
    assert!(reached(&pool, &one).await.contains_key(&own));
    let gone = link_sets::delete(&pool, &saved.set.id).await.unwrap_err();
    assert_eq!(gone.code, IpcErrorCode::NotFound);
}

/// *Hide here* hides one member on one ticket, and the set and every other
/// ticket are as they were; *Show again* puts it back.
#[tokio::test]
async fn hiding_a_member_on_one_ticket_leaves_the_set_and_the_others() {
    let pool = pool().await;
    let p = project();
    let epic = issue(&pool, &p, 1, None, None).await;
    let one = issue(&pool, &p, 2, Some(&epic), None).await;
    let two = issue(&pool, &p, 3, Some(&epic), None).await;
    let repo = thing(&pool, "repo", "hidden-here").await;
    let saved = link_sets::save(
        &pool,
        draft(
            "Stack",
            vec![member("repo", &repo)],
            vec![rule("epic", &epic)],
        ),
    )
    .await
    .unwrap();

    assert!(link_sets::hide(&pool, &one, &repo, true).await.unwrap());
    assert!(!reached(&pool, &one).await.contains_key(&repo));
    assert!(reached(&pool, &two).await.contains_key(&repo));
    assert_eq!(
        link_sets::get(&pool, &saved.set.id)
            .await
            .unwrap()
            .set
            .members
            .len(),
        1
    );

    assert!(!link_sets::hide(&pool, &one, &repo, false).await.unwrap());
    assert!(reached(&pool, &one).await.contains_key(&repo));
    let refused = link_sets::hide(&pool, "PAY-1", &repo, true)
        .await
        .unwrap_err();
    assert_eq!(refused.code, IpcErrorCode::Invalid);
}

/// ADR-0025: sets never feed membership. A promoted ticket's context, the
/// inbox's every-context read and the proposal tray scoped to the context
/// answer exactly as without the set; only the job's read shows the member.
/// The share export carries no link-set table (the context export's part).
#[tokio::test]
async fn sets_never_feed_membership_the_tray_the_inbox_or_the_export() {
    let pool = pool().await;
    let p = project();
    let ticket = issue(&pool, &p, 1, None, None).await;
    let own = thing(&pool, "repo", "own-repo").await;
    let from_set = thing(&pool, "repo", "set-repo").await;
    let beyond = thing(&pool, "page", "beyond").await;
    let guess = thing(&pool, "page", "guess").await;
    link(&pool, &ticket, &own).await;
    link(&pool, &from_set, &beyond).await;
    let context = promote_context_inner(&pool, &ticket).await.unwrap();
    sqlx::query(
        "insert into knobas.link (from_id, to_id, relation, origin, created_by, confirmed_at,
                                  rule, rule_class, reason)
         values ($1, $2, 'related', 'suggested', 'knobas', null, 'r', 'exact_key', 'named'),
                ($3, $2, 'related', 'suggested', 'knobas', null, 'r', 'exact_key', 'named')",
    )
    .bind(&from_set)
    .bind(&guess)
    .bind(&own)
    .execute(&pool)
    .await
    .unwrap();

    let members_before = knobas_core::context::member_ids(&pool, &context.context.id)
        .await
        .unwrap();
    let held_before = knobas_core::context::held_by_any_context(&pool)
        .await
        .unwrap();
    let tray_before = room_suggestions_inner(&pool, &[], Some(&context.context.id), 500)
        .await
        .unwrap();

    link_sets::save(
        &pool,
        draft(
            "Set",
            vec![member("repo", &from_set)],
            vec![rule("manual", &ticket)],
        ),
    )
    .await
    .unwrap();

    assert!(
        reached(&pool, &ticket).await.contains_key(&from_set),
        "the job shows it"
    );
    assert!(
        !reached(&pool, &ticket).await.contains_key(&beyond),
        "and the walk takes no hop from it"
    );
    let members = knobas_core::context::member_ids(&pool, &context.context.id)
        .await
        .unwrap();
    assert_eq!(members, members_before);
    assert!(!members.contains(&from_set));
    let held = knobas_core::context::held_by_any_context(&pool)
        .await
        .unwrap();
    assert_eq!(held, held_before);
    assert!(!held.contains(&from_set));
    let tray = room_suggestions_inner(&pool, &[], Some(&context.context.id), 500)
        .await
        .unwrap();
    assert_eq!(tray.total, tray_before.total);
    assert_eq!(
        tray.total, 1,
        "the own repo's proposal, never the set member's"
    );

    let parts = knobas_app::backup::share::ShareParts::default();
    assert!(
        parts
            .tables()
            .iter()
            .all(|table| !table.starts_with("link_set"))
    );
}

/// The list and the editor's read (R-7.6, R-7.7): contents and rules in the
/// order saved, each rule named by its ticket's title, "N (M yours)" by the
/// configured username, and the tickets it shows on in key order.
#[tokio::test]
async fn a_set_reads_with_its_contents_rules_and_tickets_now() {
    let pool = pool().await;
    let p = project();
    let epic = issue(&pool, &p, 1, None, None).await;
    let mine = issue(&pool, &p, 2, Some(&epic), Some("mara")).await;
    let theirs = issue(&pool, &p, 10, Some(&epic), Some("jonas")).await;
    let lone = issue(&pool, &p, 3, None, Some("Mara")).await;
    let repo = thing(&pool, "repo", "payout-service").await;
    let page = thing(&pool, "page", "PSP error codes").await;

    let saved = link_sets::save(
        &pool,
        LinkSetDraft {
            id: None,
            name: "  Payout stack ".to_owned(),
            description: "What a payout change touches.".to_owned(),
            members: vec![member("repo", &repo), member("page", &page)],
            rules: vec![rule("epic", &epic), rule("manual", &lone)],
        },
    )
    .await
    .unwrap();
    assert!(saved.set.id.starts_with("payout-stack"), "{}", saved.set.id);
    assert_eq!(saved.set.name, "Payout stack");
    let titles: Vec<_> = saved
        .set
        .members
        .iter()
        .map(|m| m.title.clone().unwrap())
        .collect();
    assert_eq!(titles, ["payout-service", "PSP error codes"]);
    assert_eq!(
        saved.set.rules[0].title.as_deref(),
        Some(format!("{p}-1").as_str())
    );
    assert_eq!((saved.set.tickets, saved.set.mine), (3, 2));
    let keys: Vec<_> = saved
        .shows_on
        .iter()
        .map(|t| (t.entity_id.clone(), t.mine))
        .collect();
    assert_eq!(
        keys,
        [
            (mine.clone(), true),
            (lone.clone(), true),
            (theirs.clone(), false)
        ]
    );

    let listed = link_sets::list(&pool).await.unwrap();
    let row = listed.iter().find(|row| row.id == saved.set.id).unwrap();
    assert_eq!(row, &saved.set);
}

/// The two directions agree: a rule's preview is exactly the tickets whose
/// job read shows the set, on every ticket of a fixture with every rule kind.
#[tokio::test]
async fn a_rules_preview_is_exactly_the_tickets_that_show_the_set() {
    let pool = pool().await;
    let p = project();
    let q = project();
    let epic = issue(&pool, &p, 1, None, None).await;
    let story = issue(&pool, &p, 2, Some(&epic), None).await;
    let sub = issue(&pool, &p, 3, Some(&story), None).await;
    let loose = issue(&pool, &p, 4, None, None).await;
    let other = issue(&pool, &q, 1, None, None).await;
    let tickets = [&epic, &story, &sub, &loose, &other];
    for (kind, value) in [
        ("epic", epic.clone()),
        ("parent", story.clone()),
        ("project", format!("jira:{q}")),
        ("manual", loose.clone()),
    ] {
        let repo = thing(&pool, "repo", kind).await;
        let the_rule = rule(kind, &value);
        let saved = link_sets::save(
            &pool,
            draft(kind, vec![member("repo", &repo)], vec![the_rule.clone()]),
        )
        .await
        .unwrap();
        let previewed: Vec<String> = link_sets::preview(&pool, &the_rule)
            .await
            .unwrap()
            .into_iter()
            .map(|t| t.entity_id)
            .collect();
        let mut shown = Vec::new();
        for ticket in tickets {
            if reached(&pool, ticket).await.contains_key(&repo) {
                shown.push((*ticket).clone());
            }
        }
        shown.sort_by_key(|id| (id.len(), id.clone()));
        assert_eq!(previewed, shown, "{kind}");
        assert!(!previewed.is_empty(), "{kind} matches something");
        link_sets::delete(&pool, &saved.set.id).await.unwrap();
    }
}

/// What cannot be a set is refused before anything is written: an item
/// knobas has no entity for, an item of another kind, a rule value that is no
/// ticket id; an id nobody holds is not found.
#[tokio::test]
async fn a_set_knobas_cannot_store_is_refused() {
    let pool = pool().await;
    let p = project();
    let ticket = issue(&pool, &p, 1, None, None).await;
    let page = thing(&pool, "page", "a page").await;
    for (refused, why) in [
        (
            draft("X", vec![member("repo", "gitea:nobody")], vec![]),
            "knobas has no item",
        ),
        (
            draft("X", vec![member("repo", &page)], vec![]),
            "is a page, not a repo",
        ),
        (
            draft("X", vec![], vec![rule("epic", "PAY-1")]),
            "is not a ticket id",
        ),
        (
            draft(" ", vec![], vec![rule("manual", &ticket)]),
            "needs a name",
        ),
    ] {
        let error = link_sets::save(&pool, refused).await.unwrap_err();
        assert_eq!(error.code, IpcErrorCode::Invalid);
        assert!(
            error.message.contains(why),
            "{} should say {why}",
            error.message
        );
    }
    let missing = link_sets::get(&pool, "no-such-set").await.unwrap_err();
    assert_eq!(missing.code, IpcErrorCode::NotFound);

    // An editor still open on a set somebody deleted cannot bring it back.
    let saved = link_sets::save(
        &pool,
        draft("Gone soon", vec![], vec![rule("manual", &ticket)]),
    )
    .await
    .unwrap();
    link_sets::delete(&pool, &saved.set.id).await.unwrap();
    let mut stale = draft("Gone soon", vec![], vec![rule("manual", &ticket)]);
    stale.id = Some(saved.set.id.clone());
    let error = link_sets::save(&pool, stale).await.unwrap_err();
    assert_eq!(error.code, IpcErrorCode::NotFound);
    assert!(
        link_sets::get(&pool, &saved.set.id).await.is_err(),
        "a save over a deleted set brought it back"
    );
}

/// Every *This job* view reads `list_entities` with `reach:`, and a set's
/// member is in it (the reach read gained the set term, #817's shape).
#[tokio::test]
async fn a_this_job_list_holds_the_sets_members() {
    let pool = pool().await;
    let p = project();
    let ticket = issue(&pool, &p, 1, None, None).await;
    let page = format!("jira:{p}-PAGE");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'page','Runbook')")
        .bind(&page)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,'jira','page','Runbook','','{}')",
    )
    .bind(&page)
    .execute(&pool)
    .await
    .unwrap();
    link_sets::save(
        &pool,
        draft(
            "Docs",
            vec![member("page", &page)],
            vec![rule("manual", &ticket)],
        ),
    )
    .await
    .unwrap();
    let filter = EntityFilter {
        sources: Vec::new(),
        kinds: vec!["page".to_owned()],
        updated_within_days: None,
        context: None,
        project: None,
        order: EntityOrder::UpdatedDesc,
        include_deleted: false,
        reach: Some(ticket.clone()),
    };
    let page_rows = list_entities_inner(
        &pool,
        &filter,
        50,
        0,
        &knobas_core::payload::Declarations::empty(),
    )
    .await
    .unwrap();
    let ids: Vec<_> = page_rows
        .rows
        .iter()
        .map(|row| row.entity_id.clone())
        .collect();
    assert_eq!(ids, [page]);
}

/// Gives `ticket` an issue type, which the parent's arm of the reach reads.
async fn typed(pool: &PgPool, ticket: &str, issuetype: &str) {
    sqlx::query(
        "update sync.item set payload = jsonb_set(payload, '{fields,issuetype}', $2)
          where entity_id = $1",
    )
    .bind(ticket)
    .bind(serde_json::json!({ "name": issuetype }))
    .execute(pool)
    .await
    .unwrap();
}

/// *Also on this job* (#867, R-4.4, R-6.2): every set that applies, by
/// name, with the first of its rules that matches as the reason, its fixed
/// members with whether each is hidden on this ticket; never a set whose
/// rules match another ticket. Hiding a member leaves it in the set.
#[tokio::test]
async fn the_job_reads_each_set_that_applies_with_its_reason() {
    let pool = pool().await;
    let p = project();
    let epic = issue(&pool, &p, 1, None, None).await;
    let other_epic = issue(&pool, &p, 2, None, None).await;
    let story = issue(&pool, &p, 3, Some(&epic), None).await;
    let repo = thing(&pool, "repo", "payout-service").await;
    let page = thing(&pool, "page", "Runbook").await;

    let stack = link_sets::save(
        &pool,
        draft(
            "Stack",
            vec![member("repo", &repo), member("page", &page)],
            vec![rule("manual", &other_epic), rule("epic", &epic)],
        ),
    )
    .await
    .unwrap();
    link_sets::save(
        &pool,
        draft(
            "Kit",
            vec![member("page", &page)],
            vec![rule("project", &format!("jira:{p}")), rule("epic", &epic)],
        ),
    )
    .await
    .unwrap();
    link_sets::save(
        &pool,
        draft(
            "Elsewhere",
            vec![member("page", &page)],
            vec![rule("epic", &other_epic)],
        ),
    )
    .await
    .unwrap();
    link_sets::hide(&pool, &story, &repo, true).await.unwrap();

    let read = link_sets::on_job(&pool, &story).await.unwrap();
    let names: Vec<&str> = read.sets.iter().map(|set| set.name.as_str()).collect();
    assert_eq!(names, ["Kit", "Stack"], "by name, and only what applies");
    let kit = &read.sets[0];
    assert_eq!(
        (kit.via.kind.as_str(), kit.via.value.clone()),
        ("project", format!("jira:{p}")),
        "the first rule that matches, in the set's order"
    );
    let on_stack = &read.sets[1];
    assert_eq!(on_stack.id, stack.set.id);
    assert_eq!(
        (on_stack.via.kind.as_str(), on_stack.via.value.as_str()),
        ("epic", epic.as_str()),
        "the manual rule names another ticket, so the epic's is the reason"
    );
    assert_eq!(on_stack.via.title.as_deref(), Some(format!("{p}-1").as_str()));
    let members: Vec<(&str, &str, Option<&str>, bool)> = on_stack
        .members
        .iter()
        .map(|m| {
            (
                m.kind.as_str(),
                m.entity_id.as_str(),
                m.title.as_deref(),
                m.hidden,
            )
        })
        .collect();
    assert_eq!(
        members,
        [
            ("repo", repo.as_str(), Some("payout-service"), true),
            ("page", page.as_str(), Some("Runbook"), false),
        ]
    );
    assert_eq!(on_stack.members[0].entity_kind.as_deref(), Some("repo"));
    assert_eq!(
        link_sets::get(&pool, &stack.set.id).await.unwrap().set.members.len(),
        2,
        "hiding never edits the set"
    );

    let on_epic = link_sets::on_job(&pool, &other_epic).await.unwrap();
    let names: Vec<(&str, &str)> = on_epic
        .sets
        .iter()
        .map(|set| (set.name.as_str(), set.via.kind.as_str()))
        .collect();
    assert_eq!(
        names,
        [("Kit", "project"), ("Stack", "manual")],
        "the epic itself is not under itself: Elsewhere stays off"
    );
    assert!(
        on_epic.sets[1].members.iter().all(|m| !m.hidden),
        "a hide is per ticket"
    );
    assert!(
        link_sets::on_job(&pool, "jira:NOPE-1")
            .await
            .unwrap()
            .sets
            .is_empty()
    );
}

/// The parent's group (R-6.2, R-7.3): a sub-task reads its parent ticket and
/// that ticket's own links, behind the global switch; an epic is never a
/// parent here (its tickets get it through link sets).
#[tokio::test]
async fn a_sub_task_reads_its_parents_own_links_behind_the_switch() {
    let pool = pool().await;
    let p = project();
    let epic = issue(&pool, &p, 1, None, None).await;
    typed(&pool, &epic, "Epic").await;
    let story = issue(&pool, &p, 2, Some(&epic), None).await;
    typed(&pool, &story, "Story").await;
    let sub = issue(&pool, &p, 3, Some(&story), None).await;
    let repo = thing(&pool, "repo", "psp-client").await;
    link(&pool, &story, &repo).await;
    link(&pool, &sub, &story).await;

    let read = link_sets::on_job(&pool, &sub).await.unwrap();
    let parent = read.parent.expect("the story is the sub-task's parent");
    assert_eq!(parent.ticket, story);
    assert_eq!(parent.title.as_deref(), Some(format!("{p}-2").as_str()));
    let links: Vec<(&str, &str, Option<&str>)> = parent
        .links
        .iter()
        .map(|l| (l.entity_id.as_str(), l.kind.as_str(), l.title.as_deref()))
        .collect();
    assert_eq!(
        links,
        [(repo.as_str(), "repo", Some("psp-client"))],
        "the parent's own links, never the sub-task itself"
    );
    assert!(
        link_sets::on_job(&pool, &story).await.unwrap().parent.is_none(),
        "an epic is not a parent here"
    );

    // No other test in this file reads a parent, so the switch is this one's.
    reach::set_parent_links(&pool, false).await.unwrap();
    let off = link_sets::on_job(&pool, &sub).await;
    reach::set_parent_links(&pool, true).await.unwrap();
    assert!(off.unwrap().parent.is_none(), "the switch is off");
}
