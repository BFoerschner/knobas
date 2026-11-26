//! The ticket keys' suggested values, read from what the database stores
//! (`sync.item_field`, `sync.item_card`; the stored-reads pass of
//! 2026-11-12), held to the walk of every sampled payload they replace:
//! `Searcher::field_suggestion_rows_both_ways` answers both statements, and
//! each state below asks them over the same rows.
//!
//! A database of its own: the sample is the newest rows of each source and
//! kind, so another test's rows would be in it, and written between the two
//! reads they would be in one answer alone. Every row here has its own date,
//! so the sample has no tie to break, and each source and kind holds fewer
//! rows than the sample takes.

use knobas_search::Searcher;
use serde_json::{Value, json};
use sqlx::PgPool;

type Rows = Vec<(String, String, Option<String>)>;

async fn scratch() -> PgPool {
    let pool = knobas_db::test_util::scratch_database("field-suggestions")
        .await
        .pool(4)
        .await
        .expect("a pool on the scratch database");
    for (id, kind) in [
        ("jira", "jira"),
        ("forge", "gitea"),
        ("wiki", "confluence"),
        ("mock", "mock"),
    ] {
        sqlx::query(
            "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
             values ($1, $2, $1, 'http://x', 'Pat', '{\"username\": \"mara.lindqvist\"}')",
        )
        .bind(id)
        .bind(kind)
        .execute(&pool)
        .await
        .unwrap();
    }
    pool
}

/// Four adapters whose kinds declare a field the keys read: Jira's and
/// Gitea's tickets every one, Gitea's pull requests their labels alone,
/// Confluence's pages their space as the project, the mock's monitors a
/// status.
async fn searcher(pool: &PgPool) -> Searcher {
    Searcher::declaring(
        pool.clone(),
        [
            knobas_source_jira::descriptor_template(),
            knobas_source_gitea::descriptor_template(),
            knobas_source_confluence::descriptor_template(),
            knobas_source_mock::descriptor_template(),
        ],
    )
    .await
    .unwrap()
}

/// Store `rows` -- `(source, kind, key, payload)` -- one statement a row, each
/// dated a minute before the last, so no two share an instant.
async fn store(pool: &PgPool, rows: &[(&str, &str, String, Value)]) -> Vec<String> {
    let mut ids = Vec::new();
    for (n, (source, kind, key, payload)) in rows.iter().enumerate() {
        let id = format!("{source}:{key}");
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, $3)")
            .bind(&id)
            .bind(kind)
            .bind(key)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                    item_updated_at)
             values ($1, $2, $3, $4, '', $5, now() - make_interval(mins => $6))",
        )
        .bind(&id)
        .bind(source)
        .bind(kind)
        .bind(key)
        .bind(payload)
        .bind(i32::try_from(n).unwrap())
        .execute(pool)
        .await
        .unwrap();
        ids.push(id);
    }
    ids
}

/// A corpus where every key has values, repeated so the counts differ, and
/// every kind of miss: a blank, an absent, a value that is not a string, a
/// list that is not a list, a label that is an object without a name.
fn corpus() -> Vec<(&'static str, &'static str, String, Value)> {
    let mut rows = Vec::new();
    let statuses = ["To Do", "In Progress", "In Review", "Done", "  "];
    let types = ["Bug", "Story", "Task"];
    let priorities = [json!("High"), json!("Low"), json!(3), Value::Null];
    let people = ["jonas.becker", "mara.lindqvist", "priya.nair"];
    let labels: [Value; 5] = [
        json!(["backend", "payouts"]),
        json!(["Backend"]),
        json!([]),
        json!(["frontend", " ", "", "backend"]),
        json!("not a list"),
    ];
    let sprints: [Value; 5] = [
        json!([{ "id": 1, "state": "closed" }]),
        json!([{ "id": 1, "state": "closed" }, { "id": 2, "state": " ACTIVE " }]),
        json!([]),
        json!([{ "id": 3, "state": "future" }, { "id": 4 }, { "id": 5, "state": 7 }]),
        json!({ "state": "active" }),
    ];
    for n in 0..40_usize {
        let project = if n % 3 == 0 { "PAY" } else { "OPS" };
        rows.push((
            "jira",
            "ticket",
            format!("{project}-{n}"),
            json!({
                "key": format!("{project}-{n}"),
                "fields": {
                    "status": { "name": statuses[n % statuses.len()] },
                    "issuetype": { "name": types[n % types.len()] },
                    "priority": { "name": priorities[n % priorities.len()] },
                    "assignee": if n % 4 == 3 { Value::Null } else {
                        json!({ "name": people[n % people.len()], "key": "k" }) },
                    "labels": labels[n % labels.len()],
                    "project": { "key": project, "name": format!("{project} Payments") },
                },
                "sprints": sprints[n % sprints.len()],
            }),
        ));
    }
    for n in 0..24_usize {
        rows.push((
            "forge",
            "ticket",
            format!("o/r/issues/{n}"),
            json!({
                "state": if n % 2 == 0 { json!("open") } else { json!("closed") },
                "assignee": if n % 3 == 0 { Value::Null } else {
                    json!({ "login": people[n % people.len()] }) },
                "labels": [{ "name": "kind/bug" }, { "name": " " }, { "color": "red" },
                           { "name": if n % 2 == 0 { "area/pay" } else { "area/ops" } }],
                "repository": { "full_name": "o/r", "name": "r" },
                "knobas": { "key": format!("o/r/issues/{n}"),
                            "priority": if n % 5 == 0 { Value::Null } else { json!("high") },
                            "issue_type": if n % 2 == 0 { json!("bug") } else { Value::Null } },
            }),
        ));
        // A pull request's labels are no card's: a pull request has none.
        rows.push((
            "forge",
            "pr",
            format!("o/r/pulls/{n}"),
            json!({
                "state": "open",
                "labels": if n % 4 == 0 { json!("none") } else {
                    json!([{ "name": "needs-review" }, { "name": format!("size/{}", n % 3) }]) },
            }),
        ));
    }
    for n in 0..12_usize {
        rows.push((
            "wiki",
            "page",
            format!("page-{n}"),
            json!({ "space": if n % 4 == 0 { json!({ "key": " " }) } else {
                json!({ "key": if n % 2 == 0 { "ENG" } else { "OPS" },
                        "name": if n % 2 == 0 { "Engineering" } else { "Operations" } }) } }),
        ));
        rows.push((
            "mock",
            "monitor",
            format!("monitor-{n}"),
            json!({ "state": if n % 3 == 0 { json!("down") } else { json!("up") } }),
        ));
    }
    rows
}

async fn both(searcher: &Searcher, when: &str) -> Rows {
    let (stored, walked) = searcher.field_suggestion_rows_both_ways().await.unwrap();
    assert_eq!(stored, walked, "{when}: the stored read is the walk's");
    stored
}

fn values<'a>(rows: &'a Rows, key: &str) -> Vec<&'a str> {
    rows.iter()
        .filter(|(k, _, _)| k == key)
        .map(|(_, v, _)| v.as_str())
        .collect()
}

/// The stored read offers what the walk offered, with every value stored,
/// with some tickets' cards gone, with none, with the cards filled again,
/// and with declarations the database never stored, where everything walks.
#[tokio::test]
async fn the_stored_values_are_the_walks_for_every_key() {
    let pool = scratch().await;
    let searcher = searcher(&pool).await;
    let ids = store(&pool, &corpus()).await;

    let rows = both(&searcher, "every value stored").await;
    // Each key offers something, so no key agrees by offering nothing.
    for key in [
        "project",
        "status",
        "issuetype",
        "priority",
        "assignee",
        "label",
        "sprint",
    ] {
        assert!(!values(&rows, key).is_empty(), "{key}: {rows:?}");
    }
    assert_eq!(values(&rows, "sprint"), ["current"]);
    // The values of a kind no card holds, and of the kinds no ticket is.
    assert!(values(&rows, "label").contains(&"needs-review"), "{rows:?}");
    assert!(values(&rows, "project").contains(&"ENG"), "{rows:?}");
    assert!(values(&rows, "status").contains(&"down"), "{rows:?}");
    assert!(
        rows.contains(&(
            "project".to_owned(),
            "PAY".to_owned(),
            Some("PAY Payments".to_owned())
        )),
        "{rows:?}"
    );
    let stored: i64 =
        sqlx::query_scalar("select count(*) from sync.item_card where entity_id = any($1)")
            .bind(&ids)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, 64, "every ticket had its card");

    // Some tickets' cards gone, as tickets the fill has not reached.
    sqlx::query("delete from sync.item_card where entity_id in (select entity_id from sync.item_card order by entity_id offset 5 limit 31)")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(both(&searcher, "some cards gone").await, rows);

    sqlx::query("delete from sync.item_card")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(both(&searcher, "no card").await, rows);

    knobas_core::item_card::fill(&pool).await.unwrap();
    assert_eq!(both(&searcher, "the cards filled again").await, rows);

    // Declarations the database never stored: the searcher still binds the
    // adapters' own, and no stored row is under them.
    sqlx::query("delete from sync.parent_declaration")
        .execute(&pool)
        .await
        .unwrap();
    let left: (i64, i64) = sqlx::query_as(
        "select (select count(*) from sync.item_card), (select count(*) from sync.item_field)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(left, (0, 0), "nothing is stored without a declaration");
    assert_eq!(both(&searcher, "nothing stored").await, rows);
}
