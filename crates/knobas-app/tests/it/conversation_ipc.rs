//! The job's conversation (#819) through its two commands' bodies: the last
//! visit (`visit_entity`) and the thread (`entity_thread`), over the embedded
//! PostgreSQL with a Jira record stored as the adapter stores it.
//!
//! This file's database is its own (#572); each test uses a key nothing else
//! holds. The identity `@me` resolves to is `mara`, the one username any source
//! here is configured with.

use knobas_app::conversation::{thread, visit};
use knobas_core::entity::EntityRef;

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
         values ('jira', 'jira', 'Jira', 'http://x', 'Pat', '{\"username\":\"mara\"}')
         on conflict (id) do update set kind = excluded.kind, config = excluded.config, enabled = true",
    )
    .execute(&pool)
    .await
    .unwrap();
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

/// **A visit is bookkeeping**: the history panel and the status bar
/// (`recent_activity`) never draw one, while a real change on the same ticket
/// still shows.
#[tokio::test]
async fn a_visit_is_not_in_the_recent_activity() {
    let pool = pool().await;
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

    let read = thread(&pool, &id).await.unwrap();
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
    assert!(read.changes.is_empty());
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
            thread(&pool, &id).await.unwrap().comments.is_empty(),
            "{payload}"
        );
    }
    let missing = format!("jira:{}", key());
    assert_eq!(
        thread(&pool, &missing).await.unwrap_err().code,
        knobas_app::IpcErrorCode::NotFound
    );
}

/// **Status changes never count as comments** (R-5.1): a move knobas delivered
/// is a change on the thread, beside the comments and not among them; one
/// still waiting in the queue is not drawn as made.
#[tokio::test]
async fn a_status_move_knobas_sent_is_a_change_and_not_a_comment() {
    let pool = pool().await;
    let id = issue(
        &pool,
        serde_json::json!({ "fields": { "comment": { "comments": [
            comment("lena", "Lena Hoffmann", "2026-08-12T09:14:00.000+0200", "Finance asks."),
        ] } } }),
    )
    .await;
    let entity = EntityRef::parse(&id).unwrap();
    let mut queued = Vec::new();
    for status in ["In Review", "Done"] {
        let op = knobas_source::WriteOp::Transition {
            entity: id.clone(),
            status: status.to_owned(),
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
        queued.push(row.id);
    }
    knobas_core::write_queue::sent(&pool, queued[0], None)
        .await
        .unwrap()
        .unwrap();

    let read = thread(&pool, &id).await.unwrap();
    assert_eq!(read.comments.len(), 1);
    assert_eq!(
        read.changes
            .iter()
            .map(|c| c.status.as_str())
            .collect::<Vec<_>>(),
        ["In Review"]
    );
}
