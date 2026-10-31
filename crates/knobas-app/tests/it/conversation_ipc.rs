//! The job's conversation (#819) through its two commands' bodies: the last
//! visit (`visit_entity`) and the thread (`entity_thread`), over the embedded
//! PostgreSQL with a Jira record stored as the adapter stores it.
//!
//! This file's database is its own (#572); each test uses a key nothing else
//! holds, and a test that reads the newest lines of the whole activity log
//! takes a database of its own ([`scratch`], #1907). The identity `@me` resolves to is `mara`, the one username any source
//! here is configured with.

use knobas_app::commands::entity::get_entity_inner;
use knobas_app::conversation::{thread, visit};
use knobas_app::sources::Registry;
use knobas_core::entity::EntityRef;
use knobas_source::Markup;

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    with_sources(pool).await
}

/// A database of its own, for a test that reads the newest line of the
/// database-wide activity log: on the shared one another test's line is newer
/// by scheduling (#1907).
async fn scratch(label: &str) -> sqlx::PgPool {
    let scratch = knobas_db::test_util::scratch_database(label).await;
    let pool = scratch
        .pool(2)
        .await
        .expect("a pool on the scratch database");
    with_sources(pool).await
}

async fn with_sources(pool: sqlx::PgPool) -> sqlx::PgPool {
    for (id, config) in [("jira", r#"{"username":"mara"}"#), ("gitea", "{}")] {
        sqlx::query(
            "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
             values ($1, $1, $1, 'http://x', 'Pat', $2::jsonb)
             on conflict (id) do update set kind = excluded.kind, config = excluded.config, enabled = true",
        )
        .bind(id)
        .bind(config)
        .execute(&pool)
        .await
        .unwrap();
    }
    pool
}

/// A ticket key nothing else in this database holds.
fn key() -> String {
    let hex = uuid::Uuid::new_v4().simple().to_string();
    format!("CNV-{}", u32::from_str_radix(&hex[..6], 16).unwrap())
}

/// One Jira issue whose payload is `payload`, stored as the adapter stores it.
async fn issue(pool: &sqlx::PgPool, payload: serde_json::Value) -> String {
    let id = format!("jira:{}", key());
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'ticket',$1)")
        .bind(&id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at)
         values ($1,'jira','ticket',$1,'',$2, now())",
    )
    .bind(&id)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// A Jira comment as `/search` with `fields=comment` writes one.
fn comment(name: &str, display: &str, created: &str, body: &str) -> serde_json::Value {
    serde_json::json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "author": { "name": name, "displayName": display },
        "created": created,
        "body": body,
    })
}

/// **Last visit** (R-5.5): the first open answers nothing, the second answers
/// the first, and every open is one `visited` line on the ticket.
#[tokio::test]
async fn a_visit_answers_the_one_before_it() {
    let pool = pool().await;
    let id = issue(&pool, serde_json::json!({})).await;

    assert_eq!(visit(&pool, &id).await.unwrap(), None);
    let (first,): (chrono::DateTime<chrono::Utc>,) =
        sqlx::query_as("select at from knobas.activity where entity_id = $1 and verb = 'visited'")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(visit(&pool, &id).await.unwrap(), Some(first));

    let (lines,): (i64,) = sqlx::query_as(
        "select count(*) from knobas.activity where entity_id = $1 and verb = 'visited' and actor = 'user'",
    )
    .bind(&id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(lines, 2);
}

/// A visit is the ticket's own: another ticket's visit is not its last one.
#[tokio::test]
async fn a_visit_to_another_ticket_is_not_this_ones() {
    let pool = pool().await;
    let one = issue(&pool, serde_json::json!({})).await;
    let other = issue(&pool, serde_json::json!({})).await;
    visit(&pool, &other).await.unwrap();
    assert_eq!(visit(&pool, &one).await.unwrap(), None);
}

/// The refusals: an address that is no entity id, and one nothing holds.
#[tokio::test]
async fn a_visit_to_nothing_is_refused_and_writes_nothing() {
    let pool = pool().await;
    assert_eq!(
        visit(&pool, "no-colon").await.unwrap_err().code,
        knobas_app::IpcErrorCode::Invalid
    );
    let missing = format!("jira:{}", key());
    assert_eq!(
        visit(&pool, &missing).await.unwrap_err().code,
        knobas_app::IpcErrorCode::NotFound
    );
    let (lines,): (i64,) =
        sqlx::query_as("select count(*) from knobas.activity where entity_id = $1")
            .bind(&missing)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(lines, 0);
}

/// **A visit is bookkeeping**: the activity read (`knobas_core::activity::
/// recent`, the detail's history) never draws one, while a real change on the
/// same ticket still shows.
#[tokio::test]
async fn a_visit_is_not_in_the_recent_activity() {
    let pool = scratch("conversation_ipc_activity").await;
    let id = issue(&pool, serde_json::json!({})).await;
    let entity = EntityRef::parse(&id).unwrap();
    knobas_core::activity::record(
        &pool,
        "user",
        "linked",
        Some(&entity),
        serde_json::json!({}),
    )
    .await
    .unwrap();
    visit(&pool, &id).await.unwrap();

    let scoped = knobas_core::activity::recent(&pool, 10, Some(&entity))
        .await
        .unwrap();
    assert_eq!(
        scoped
            .iter()
            .map(|line| line.verb.as_str())
            .collect::<Vec<_>>(),
        ["linked"]
    );
    let global = knobas_core::activity::recent(&pool, 1, None).await.unwrap();
    assert_eq!(
        global[0].verb, "linked",
        "the newest line drawn is the link, not the visit after it"
    );
}

/// **The thread** (R-5.2): Jira's comments in its order, each with the name a
/// person reads, the time, the reader's own marked, and *mentions me* matched
/// against the identity in both spellings Jira Data Center writes.
#[tokio::test]
async fn a_jira_thread_reads_its_comments_with_mine_and_mentions() {
    let pool = pool().await;
    let id = issue(
        &pool,
        serde_json::json!({ "fields": { "comment": { "total": 3, "comments": [
            comment("lena", "Lena Hoffmann", "2026-08-12T09:14:00.000+0200", "Can we retry the transient ones?"),
            comment("priya", "Priya Nair", "2026-08-12T09:40:00.000+0200", "Yes. [~mara] can you pick this up?"),
            comment("mara", "Mara Lindqvist", "2026-08-12T10:02:00.000+0200", "Sure. @mara.lindqvist is not me."),
        ] } } }),
    )
    .await;

    let read = thread(&pool, &Registry::for_process(), &id).await.unwrap();
    assert_eq!(read.me, ["mara"]);
    let lines: Vec<_> = read
        .comments
        .iter()
        .map(|c| (c.author.as_deref().unwrap(), c.mine, c.mentions_me))
        .collect();
    assert_eq!(
        lines,
        [
            ("Lena Hoffmann", false, false),
            ("Priya Nair", false, true),
            ("Mara Lindqvist", true, false),
        ]
    );
    assert_eq!(
        read.comments[0].at,
        Some("2026-08-12T07:14:00Z".parse().unwrap())
    );
    assert_eq!(read.comments[1].body, "Yes. [~mara] can you pick this up?");
}

/// **Failure direction** (ADR-0007): a payload of another shape, and a comment
/// container that is not a list, read as no comments -- never as an error and
/// never as a guess.
#[tokio::test]
async fn a_payload_of_another_shape_reads_as_no_comments() {
    let pool = pool().await;
    for payload in [
        serde_json::json!({}),
        serde_json::json!({ "fields": { "comment": { "comments": "three" } } }),
        serde_json::json!({ "fields": { "comment": [ { "body": "not where Jira puts it" } ] } }),
    ] {
        let id = issue(&pool, payload.clone()).await;
        assert!(
            thread(&pool, &Registry::for_process(), &id)
                .await
                .unwrap()
                .comments
                .is_empty(),
            "{payload}"
        );
    }
    let missing = format!("jira:{}", key());
    assert_eq!(
        thread(&pool, &Registry::for_process(), &missing)
            .await
            .unwrap_err()
            .code,
        knobas_app::IpcErrorCode::NotFound
    );
}

/// A Gitea pull request as the adapter stores it: Gitea's own record.
async fn pull_request(
    pool: &sqlx::PgPool,
    number: u32,
    created: &str,
    merged: Option<&str>,
) -> String {
    let id = format!("gitea:tidewater/payout-service-{}#{number}", &key()[4..]);
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'pr',$1)")
        .bind(&id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at)
         values ($1,'gitea','pr',$1,'',$2, now())",
    )
    .bind(&id)
    .bind(serde_json::json!({
        "number": number,
        "created_at": created,
        "merged": merged.is_some(),
        "merged_at": merged,
        "state": if merged.is_some() { "closed" } else { "open" },
        "base": { "repo": { "full_name": "tidewater/payout-service", "name": "payout-service" } },
    }))
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn confirmed(pool: &sqlx::PgPool, from: &str, to: &str) {
    sqlx::query(
        "insert into knobas.link (from_id, to_id, relation, origin, created_by, confirmed_at)
         values ($1, $2, 'related', 'manual', 'user', now())",
    )
    .bind(from)
    .bind(to)
    .execute(pool)
    .await
    .unwrap();
}

/// **The conversation is the ticket's comments** (R-5.1, R-5.2; #2163): a
/// ticket whose payload still holds a Jira changelog, with a status move knobas
/// sent and a confirm-linked merged pull request, answers its comments and
/// nothing else -- no status move from either record and no pull-request line.
#[tokio::test]
async fn the_thread_is_the_comments_and_nothing_else() {
    let pool = pool().await;
    let ticket = issue(
        &pool,
        serde_json::json!({
            "fields": { "comment": { "comments": [
                comment("lena", "Lena Hoffmann", "2026-08-12T09:14:00.000+0200", "Finance asks."),
            ] } },
            "changelog": { "startAt": 0, "maxResults": 1, "total": 1, "histories": [{
                "id": "10001",
                "author": { "name": "priya", "displayName": "Priya Nair", "active": true },
                "created": "2026-08-12T09:30:00.000+0200",
                "items": [{ "field": "status", "fieldtype": "jira", "from": "1",
                            "fromString": "To Do", "to": "3", "toString": "In Progress" }],
            }] },
        }),
    )
    .await;
    let entity = EntityRef::parse(&ticket).unwrap();
    let op = knobas_source::WriteOp::Transition {
        entity: ticket.clone(),
        status: "In Review".to_owned(),
    };
    let row = knobas_core::write_queue::queue(
        &pool,
        "jira",
        &entity,
        "transition",
        serde_json::to_value(&op).unwrap(),
        serde_json::json!({}),
    )
    .await
    .unwrap();
    knobas_core::write_queue::sent(&pool, row.id, None)
        .await
        .unwrap()
        .unwrap();
    let merged = pull_request(
        &pool,
        57,
        "2026-08-12T09:00:00+02:00",
        Some("2026-08-13T15:00:00+02:00"),
    )
    .await;
    confirmed(&pool, &ticket, &merged).await;

    let read = thread(&pool, &Registry::for_process(), &ticket)
        .await
        .unwrap();
    let wire = serde_json::to_value(&read).unwrap();
    let mut keys: Vec<_> = wire.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(keys, ["comments", "me"], "the answer carries comments only");
    assert_eq!(
        read.comments
            .iter()
            .map(|c| (c.author.as_deref(), c.body.as_str()))
            .collect::<Vec<_>>(),
        [(Some("Lena Hoffmann"), "Finance asks.")]
    );
}

/// **A ticket's text carries its markup** (#2308): a Jira ticket answers its
/// description as Jira wrote it, not the search text, and says both it and
/// each comment are Jira's wiki markup, as the Jira adapter declares.
#[tokio::test]
async fn a_jira_ticket_says_its_description_and_comments_are_wiki_markup() {
    let pool = pool().await;
    let id = issue(
        &pool,
        serde_json::json!({ "fields": {
            "description": "h2. Why\n\n# first\n# second",
            "comment": { "comments": [
                comment("lena", "Lena Hoffmann", "2026-08-12T09:14:00.000+0200", "*Bold* ask"),
            ] },
        } }),
    )
    .await;

    let detail = get_entity_inner(&pool, &Registry::for_process(), &id)
        .await
        .unwrap();
    assert_eq!(
        detail.description.as_deref(),
        Some("h2. Why\n\n# first\n# second")
    );
    assert_eq!(detail.markup, Markup::JiraWiki);

    let read = thread(&pool, &Registry::for_process(), &id).await.unwrap();
    assert_eq!(
        read.comments
            .iter()
            .map(|c| (c.body.as_str(), c.markup))
            .collect::<Vec<_>>(),
        [("*Bold* ask", Markup::JiraWiki)]
    );
}

/// **A source that declares nothing is text** (#2308): a ticket of a source
/// no adapter describes has no description and its comments are text, never
/// parsed as markup nobody said it was.
#[tokio::test]
async fn a_ticket_of_an_undeclared_source_is_text() {
    let pool = pool().await;
    let id = format!("nowhere:{}", key());
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'ticket',$1)")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at)
         values ($1,'nowhere','ticket',$1,'',$2, now())",
    )
    .bind(&id)
    .bind(serde_json::json!({
        "fields": { "description": "h2. Why" },
        "comments": [{ "who": "mara", "when": "2026-08-12T10:02:00Z", "text": "*Sure*" }],
    }))
    .execute(&pool)
    .await
    .unwrap();

    let detail = get_entity_inner(&pool, &Registry::for_process(), &id)
        .await
        .unwrap();
    assert_eq!(detail.description, None);
    assert_eq!(detail.markup, Markup::Text);
    let read = thread(&pool, &Registry::for_process(), &id).await.unwrap();
    assert_eq!(read.comments[0].markup, Markup::Text);
}
