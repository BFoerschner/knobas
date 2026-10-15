//! The ticket keys (#811) through the commands the launcher calls:
//! `search` and `filter_suggestions`, over the embedded PostgreSQL, with
//! the declarations the compiled-in Jira adapter makes (`Registry::builtin`),
//! never a copy of them.
//!
//! This file's database is its own (#572); each test uses a project key
//! nothing else holds, and scopes every query by it.

use knobas_app::commands::search::{filter_suggestions_inner, search_inner};
use knobas_search::{SearchFilters, SearchQuery};

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
         values ('jira', 'jira', 'Jira', 'http://x', 'Pat', '{\"username\": \"mara.lindqvist\"}')
         on conflict (id) do update set kind = excluded.kind, enabled = true,
                                         config = excluded.config",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

/// A project key nothing else in this database holds: letters only.
fn project() -> String {
    let hex = uuid::Uuid::new_v4().simple().to_string();
    let letters: String = hex[..10]
        .chars()
        .map(|c| char::from(b'A' + u8::try_from(c.to_digit(16).unwrap()).unwrap()))
        .collect();
    format!("K{letters}")
}

/// One issue, stored the way the Jira adapter stores it: the record as Jira
/// sends it and the `sprints` list `map` writes beside it.
async fn issue(pool: &sqlx::PgPool, key: &str, fields: serde_json::Value, sprints: &[&str]) {
    let mut fields = fields;
    fields["summary"] = serde_json::json!(format!("{key} summary"));
    let project = key.split('-').next().unwrap();
    fields["project"] = serde_json::json!({ "key": project, "name": "Payments" });
    let payload = serde_json::json!({
        "key": key,
        "fields": fields,
        "sprints": sprints.iter().map(|s| serde_json::json!({ "id": 1, "name": "S", "state": s }))
            .collect::<Vec<_>>(),
    });
    let id = format!("jira:{key}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'ticket',$2)")
        .bind(&id)
        .bind(key)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                item_updated_at)
         values ($1,'jira','ticket',$2,$2,$3, now())",
    )
    .bind(&id)
    .bind(key)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
}

async fn keys(pool: &sqlx::PgPool, raw: &str) -> Vec<String> {
    let response = search_inner(
        pool,
        SearchQuery {
            raw: raw.to_owned(),
            limit: 50,
            filters: SearchFilters::default(),
        },
    )
    .await
    .unwrap();
    assert!(
        response.interpreted.unknown_tokens.is_empty(),
        "{raw}: {:?}",
        response.interpreted.unknown_tokens
    );
    let mut out: Vec<String> = response
        .groups
        .iter()
        .flat_map(|g| g.hits.iter().map(|h| h.row.entity_id.replace("jira:", "")))
        .collect();
    out.sort();
    out
}

/// Every key through the `search` command: one value, two values meaning
/// either, and the negation, read through what the built-in Jira adapter
/// declares.
#[tokio::test]
async fn the_search_command_answers_every_ticket_key() {
    let pool = pool().await;
    let p = project();
    let (a, b) = (format!("{p}-1"), format!("{p}-2"));
    issue(
        &pool,
        &a,
        serde_json::json!({
            "status": { "name": "In Review" }, "issuetype": { "name": "Bug" },
            "priority": { "name": "High" }, "labels": ["sepa"],
            "assignee": { "name": "mara.lindqvist", "key": "JIRAUSER1" },
        }),
        &["active"],
    )
    .await;
    issue(
        &pool,
        &b,
        serde_json::json!({
            "status": { "name": "Done" }, "issuetype": { "name": "Story" },
            "priority": { "name": "Low" }, "labels": [],
            "assignee": { "name": "jonas.becker", "key": "JIRAUSER2" },
        }),
        &["closed"],
    )
    .await;
    let both = vec![a.clone(), b.clone()];
    for (raw, expected) in [
        (format!("project:{p}"), both.clone()),
        (format!("project:{p} status:\"in review\""), vec![a.clone()]),
        (
            format!("project:{p} status:Done status:\"In Review\""),
            both.clone(),
        ),
        (format!("project:{p} -status:Done"), vec![a.clone()]),
        (format!("project:{p} issuetype:Story"), vec![b.clone()]),
        (format!("project:{p} -issuetype:story"), vec![a.clone()]),
        (
            format!("project:{p} priority:high priority:low"),
            both.clone(),
        ),
        (format!("project:{p} -priority:High"), vec![b.clone()]),
        (format!("project:{p} assignee:@me"), vec![a.clone()]),
        (format!("project:{p} -assignee:@me"), vec![b.clone()]),
        (
            format!("project:{p} assignee:jonas.becker assignee:@me"),
            both.clone(),
        ),
        (format!("project:{p} label:SEPA"), vec![a.clone()]),
        (format!("project:{p} -label:sepa"), vec![b.clone()]),
        (format!("project:{p} sprint:current"), vec![a.clone()]),
        (format!("project:{p} -sprint:current"), vec![b.clone()]),
        (format!("project:{p} assigned:today"), vec![a.clone()]),
        (format!("project:{p} -assigned:week"), vec![b.clone()]),
        (format!("-project:{p} project:{p}"), vec![]),
    ] {
        assert_eq!(keys(&pool, &raw).await, expected, "{raw}");
    }
}

/// `filter_suggestions` through its command offers the keys' values,
/// and the parents-only read offers none.
#[tokio::test]
async fn the_filter_suggestions_command_offers_the_keys_values() {
    let pool = pool().await;
    let p = project();
    issue(
        &pool,
        &format!("{p}-1"),
        serde_json::json!({
            "status": { "name": "In Review" }, "issuetype": { "name": "Bug" },
            "priority": { "name": "High" }, "labels": ["psp"],
        }),
        &["active"],
    )
    .await;
    let lists = filter_suggestions_inner(&pool, "", false).await.unwrap();
    let offered = |key: &str, value: &str| {
        lists
            .fields
            .iter()
            .any(|f| f.key == key && f.value == value)
    };
    for (key, value) in [
        ("project", p.as_str()),
        ("status", "In Review"),
        ("issuetype", "Bug"),
        ("priority", "High"),
        ("label", "psp"),
        ("assignee", "@me"),
        ("sprint", "current"),
        ("assigned", "today"),
        ("assigned", "week"),
    ] {
        assert!(offered(key, value), "{key}:{value} in {:?}", lists.fields);
    }
    let parents_only = filter_suggestions_inner(&pool, "", true).await.unwrap();
    assert!(parents_only.fields.is_empty());
}
