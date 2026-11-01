//! The job (#816) through the commands the job card and its Code section
//! call: `get_entity` for the card's fields, `promote_context` and the
//! membership-scoped `list_entities` for the section, over the embedded
//! PostgreSQL with the declarations the compiled-in Jira adapter makes
//! (`Registry::for_process`), never a copy of them.
//!
//! This file's database is its own (#572); each test uses a key nothing else
//! holds.

use knobas_app::commands::entity::{
    create_link_inner, get_entity_inner, list_entities_inner, promote_context_inner,
};
use knobas_app::sources::Registry;
use knobas_core::entity_list::{EntityFilter, EntityOrder};

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
         values ('jira', 'jira', 'Jira', 'http://x', 'Pat', '{}')
         on conflict (id) do update set kind = excluded.kind, enabled = true",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

/// A ticket key nothing else in this database holds.
fn key() -> String {
    let hex = uuid::Uuid::new_v4().simple().to_string();
    format!("JOB-{}", u32::from_str_radix(&hex[..6], 16).unwrap())
}

/// One mirror item, stored as an adapter stores it.
async fn item(pool: &sqlx::PgPool, id: &str, source: &str, kind: &str, payload: serde_json::Value) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$1)")
        .bind(id)
        .bind(kind)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                item_updated_at)
         values ($1,$2,$3,$1,'',$4, now())",
    )
    .bind(id)
    .bind(source)
    .bind(kind)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
}

/// One Jira issue: the record as Jira sends it.
async fn issue(pool: &sqlx::PgPool, key: &str, fields: serde_json::Value) -> String {
    let id = format!("jira:{key}");
    item(
        pool,
        &id,
        "jira",
        "ticket",
        serde_json::json!({ "key": key, "fields": fields }),
    )
    .await;
    id
}

/// **The job card's type, priority and estimate** (R-4.2) come with the
/// detail, read through the fields the Jira adapter declares: the type and
/// priority by name, the estimate as Jira's `timeoriginalestimate`, seconds.
#[tokio::test]
async fn a_tickets_detail_carries_its_type_priority_and_estimate() {
    let pool = pool().await;
    let id = issue(
        &pool,
        &key(),
        serde_json::json!({
            "status": { "name": "In Progress" },
            "issuetype": { "name": "Story" },
            "priority": { "name": "High" },
            "timeoriginalestimate": 10800,
        }),
    )
    .await;

    let detail = get_entity_inner(&pool, &Registry::for_process(), &id)
        .await
        .unwrap();
    assert_eq!(detail.issue_type.as_deref(), Some("Story"));
    assert_eq!(detail.priority.as_deref(), Some("High"));
    assert_eq!(detail.estimate_seconds, Some(10800));
}

/// **The card's labels** (#2400) come with the detail, through the
/// `labels` path the Jira adapter declares, in the record's order; a ticket
/// with none, or with a blank one, carries none of those, and the wire leaves
/// the field out when the list is empty.
#[tokio::test]
async fn a_tickets_detail_carries_its_labels_and_leaves_them_off_the_wire_when_there_are_none() {
    let pool = pool().await;
    let labelled = issue(
        &pool,
        &key(),
        serde_json::json!({ "labels": ["sepa", " ", "psp"] }),
    )
    .await;
    let bare = issue(&pool, &key(), serde_json::json!({ "labels": [] })).await;

    let detail = get_entity_inner(&pool, &Registry::for_process(), &labelled)
        .await
        .unwrap();
    assert_eq!(
        detail.labels.as_deref(),
        Some(&["sepa".to_owned(), "psp".to_owned()][..])
    );
    let none = get_entity_inner(&pool, &Registry::for_process(), &bare)
        .await
        .unwrap();
    assert_eq!(none.labels, None);
    let wire = serde_json::to_value(&none).unwrap();
    assert!(wire.get("labels").is_none(), "{wire}");
}

/// **The ticket page's attachment list** (#2404) comes with the detail,
/// through the `attachments` declaration the Jira adapter makes, in the
/// record's order with each file's name, size, author, date and addresses; a
/// ticket with none leaves the field off the wire.
#[tokio::test]
async fn a_tickets_detail_carries_its_attachments_and_leaves_them_off_the_wire_when_there_are_none()
{
    let pool = pool().await;
    let attached = issue(
        &pool,
        &key(),
        serde_json::json!({ "attachment": [
            { "id": "50000", "filename": "psp-409-trace.png", "mimeType": "image/png",
              "size": 186_368, "author": { "name": "lena.hoffmann", "displayName": "Lena Hoffmann" },
              "created": "2026-08-12T11:14:00.000+0200",
              "content": "https://jira.example/secure/attachment/50000/psp-409-trace.png",
              "thumbnail": "https://jira.example/secure/thumbnail/50000/psp-409-trace.png" },
            { "id": "50002", "filename": "sepa-error-codes.pdf", "mimeType": "application/pdf",
              "size": 1_258_291, "author": { "name": "priya.nair", "displayName": "Priya Nair" },
              "created": "2026-08-12T13:30:00.000+0200",
              "content": "https://jira.example/secure/attachment/50002/sepa-error-codes.pdf" }
        ] }),
    )
    .await;
    let bare = issue(&pool, &key(), serde_json::json!({ "attachment": [] })).await;

    let detail = get_entity_inner(&pool, &Registry::for_process(), &attached)
        .await
        .unwrap();
    let wire = serde_json::to_value(&detail).unwrap();
    assert_eq!(
        wire["attachments"],
        serde_json::json!([
            { "id": "50000", "name": "psp-409-trace.png", "mime": "image/png", "size": 186_368,
              "author": "Lena Hoffmann", "created": "2026-08-12T09:14:00Z",
              "content_url": "https://jira.example/secure/attachment/50000/psp-409-trace.png",
              "thumbnail_url": "https://jira.example/secure/thumbnail/50000/psp-409-trace.png" },
            { "id": "50002", "name": "sepa-error-codes.pdf", "mime": "application/pdf",
              "size": 1_258_291, "author": "Priya Nair", "created": "2026-08-12T11:30:00Z",
              "content_url": "https://jira.example/secure/attachment/50002/sepa-error-codes.pdf",
              "thumbnail_url": null }
        ])
    );
    let none = get_entity_inner(&pool, &Registry::for_process(), &bare)
        .await
        .unwrap();
    assert_eq!(none.attachments, None);
    let wire = serde_json::to_value(&none).unwrap();
    assert!(wire.get("attachments").is_none(), "{wire}");
}

/// A miss is a miss (ADR-0007): no estimate, an estimate Jira wrote as `null`
/// and one that is not a number all read as none, never as zero.
#[tokio::test]
async fn a_missing_or_unreadable_estimate_is_none() {
    let pool = pool().await;
    for fields in [
        serde_json::json!({ "issuetype": { "name": "Bug" } }),
        serde_json::json!({ "timeoriginalestimate": null }),
        serde_json::json!({ "timeoriginalestimate": "3h" }),
    ] {
        let id = issue(&pool, &key(), fields.clone()).await;
        let detail = get_entity_inner(&pool, &Registry::for_process(), &id)
            .await
            .unwrap();
        assert_eq!(detail.estimate_seconds, None, "{fields}");
        assert_eq!(detail.priority, None, "{fields}");
    }
}

/// **Opening a job promotes its ticket exactly once** (#816): the job page
/// calls `promote_context` on every open, so the second open must answer the
/// first one's context and write nothing, and the Code section's read --
/// `list_entities` scoped by that context -- holds what the ticket links.
#[tokio::test]
async fn opening_a_job_twice_promotes_its_ticket_once() {
    let pool = pool().await;
    let key = key();
    let ticket = issue(&pool, &key, serde_json::json!({})).await;
    let pr = format!("gitea-{key}:tidewater/payout-service#1");
    item(
        &pool,
        &pr,
        &format!("gitea-{key}"),
        "pr",
        serde_json::json!({}),
    )
    .await;
    create_link_inner(&pool, &ticket, &pr, Some("implements"), None)
        .await
        .unwrap();

    let first = promote_context_inner(
        &pool,
        &knobas_app::sources::Registry::for_process(),
        &ticket,
    )
    .await
    .unwrap();
    let again = promote_context_inner(
        &pool,
        &knobas_app::sources::Registry::for_process(),
        &ticket,
    )
    .await
    .unwrap();
    assert_eq!(first.context.id, again.context.id);

    let (contexts,): (i64,) =
        sqlx::query_as("select count(*) from knobas.context where anchor_id = $1")
            .bind(&ticket)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        contexts, 1,
        "one context per ticket, however often it opens"
    );
    let (lines,): (i64,) = sqlx::query_as(
        "select count(*) from knobas.activity where verb = 'promoted' and entity_id = $1",
    )
    .bind(&first.context.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(lines, 1, "the second open mutated nothing and logs nothing");

    let code = list_entities_inner(
        &pool,
        &Registry::for_process(),
        &EntityFilter {
            sources: Vec::new(),
            kinds: vec!["pr".into(), "branch".into(), "commit".into(), "repo".into()],
            updated_within_days: None,
            context: Some(first.context.id.clone()),
            project: None,
            order: EntityOrder::UpdatedDesc,
            include_deleted: false,
            reach: None,
            role: None,
        },
        200,
        0,
        &knobas_core::payload::Declarations::empty(),
    )
    .await
    .unwrap();
    let ids: Vec<&str> = code.rows.iter().map(|r| r.entity_id.as_str()).collect();
    assert_eq!(ids, vec![pr.as_str()]);
}

/// **A job's sections read through the reach** (#817): `list_entities` with
/// `reach` keeps what the ticket reaches -- its pull request and the build
/// one hop past it -- and nothing the ticket does not reach.
#[tokio::test]
async fn list_entities_scoped_by_reach_keeps_what_the_ticket_reaches() {
    let pool = pool().await;
    let key = key();
    let ticket = issue(&pool, &key, serde_json::json!({})).await;
    let source = format!("gitea-{key}");
    let pr = format!("{source}:tidewater/payout-service#1");
    let build = format!("{source}:build-1");
    let other = format!("{source}:build-2");
    item(&pool, &pr, &source, "pr", serde_json::json!({})).await;
    item(&pool, &build, &source, "build", serde_json::json!({})).await;
    item(&pool, &other, &source, "build", serde_json::json!({})).await;
    create_link_inner(&pool, &ticket, &pr, Some("implements"), None)
        .await
        .unwrap();
    create_link_inner(&pool, &pr, &build, Some("built-by"), None)
        .await
        .unwrap();

    let filter = |kinds: &[&str]| EntityFilter {
        sources: Vec::new(),
        kinds: kinds.iter().map(|k| (*k).to_owned()).collect(),
        updated_within_days: None,
        context: None,
        project: None,
        order: EntityOrder::UpdatedDesc,
        include_deleted: false,
        reach: Some(ticket.clone()),
        role: None,
    };
    let none = knobas_core::payload::Declarations::empty();
    let builds = list_entities_inner(
        &pool,
        &Registry::for_process(),
        &filter(&["build"]),
        50,
        0,
        &none,
    )
    .await
    .unwrap();
    let ids: Vec<&str> = builds.rows.iter().map(|r| r.entity_id.as_str()).collect();
    assert_eq!(
        ids,
        vec![build.as_str()],
        "the build via the PR, not the other"
    );
    assert_eq!(builds.total, 1);

    let everything =
        list_entities_inner(&pool, &Registry::for_process(), &filter(&[]), 50, 0, &none)
            .await
            .unwrap();
    assert_eq!(
        everything.total, 2,
        "the PR and the build, never the ticket"
    );
}
