//! The ticket keys (#811, R-3.14) against a real PostgreSQL: `project:`,
//! `status:`, `issuetype:`, `priority:`, `assignee:`, `label:`,
//! `sprint:current` and `assigned:`, each read through what the Jira adapter
//! declares (ADR-0007), never through a payload key named in the search.
//!
//! Items are stored the way the adapter stores them: the issue record
//! verbatim, plus the `sprints` list `map` writes beside it. The declaration
//! is the adapter's own descriptor. One database for the file (#572), a
//! project key unique to each test, so tests never see each other's rows.

use knobas_search::{SearchFilters, SearchQuery, SearchResponse, Searcher};

fn q(raw: &str) -> SearchQuery {
    SearchQuery {
        raw: raw.to_owned(),
        limit: 50,
        filters: SearchFilters::default(),
    }
}

/// A project key unique to one test, letters only.
fn project() -> String {
    let hex = uuid::Uuid::new_v4().simple().to_string();
    let letters: String = hex[..10]
        .chars()
        .map(|c| char::from(b'A' + u8::try_from(c.to_digit(16).unwrap()).unwrap()))
        .collect();
    format!("T{letters}")
}

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    // The user is `mara.lindqvist`, as Test connection fills it in.
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

async fn searcher(pool: &sqlx::PgPool) -> Searcher {
    Searcher::declaring(pool.clone(), [knobas_source_jira::descriptor_template()])
        .await
        .unwrap()
}

/// One ticket's fields, as Jira sends them.
#[derive(Clone)]
struct Ticket {
    status: &'static str,
    issuetype: &'static str,
    priority: Option<&'static str>,
    assignee: Option<&'static str>,
    labels: &'static [&'static str],
    sprints: &'static [&'static str],
    /// Days since Jira last changed it.
    age_days: i32,
}

const BASE: Ticket = Ticket {
    status: "To Do",
    issuetype: "Story",
    priority: Some("Medium"),
    assignee: None,
    labels: &[],
    sprints: &[],
    age_days: 0,
};

/// Store `ticket` as `<project>-<n>`, the record shaped as the adapter
/// stores it, and return its entity id.
async fn store(pool: &sqlx::PgPool, project: &str, n: u32, ticket: Ticket) -> String {
    let key = format!("{project}-{n}");
    let id = format!("jira:{key}");
    let payload = serde_json::json!({
        "id": n.to_string(),
        "key": key,
        "fields": {
            "summary": format!("{key} summary"),
            "status": { "name": ticket.status },
            "issuetype": { "name": ticket.issuetype },
            "priority": ticket.priority.map(|p| serde_json::json!({ "name": p })),
            "assignee": ticket.assignee.map(|a| serde_json::json!({ "name": a, "key": a })),
            "labels": ticket.labels,
            "project": { "key": project, "name": format!("{project} Payments") },
        },
        "sprints": ticket.sprints.iter().enumerate().map(|(i, state)| serde_json::json!({
            "id": i, "name": format!("Sprint {i}"), "state": state
        })).collect::<Vec<_>>(),
    });
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'ticket',$2)")
        .bind(&id)
        .bind(&key)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                item_updated_at)
         values ($1,'jira','ticket',$2,$2,$3, now() - make_interval(days => $4))",
    )
    .bind(&id)
    .bind(&key)
    .bind(payload)
    .bind(ticket.age_days)
    .execute(pool)
    .await
    .unwrap();
    id
}

fn ids(response: &SearchResponse) -> Vec<String> {
    let mut out: Vec<String> = response
        .groups
        .iter()
        .flat_map(|g| g.hits.iter().map(|h| h.row.entity_id.clone()))
        .collect();
    out.sort();
    out
}

/// What `raw` returns among one project's tickets: the project is added as
/// a key so other tests' rows never answer.
async fn answer(pool: &sqlx::PgPool, project: &str, raw: &str) -> Vec<String> {
    let response = searcher(pool)
        .await
        .search(q(&format!("project:{project} {raw}")))
        .await
        .unwrap();
    assert!(
        response.interpreted.unknown_tokens.is_empty(),
        "{raw}: {:?}",
        response.interpreted.unknown_tokens
    );
    ids(&response)
}

fn sorted(mut ids: Vec<String>) -> Vec<String> {
    ids.sort();
    ids
}

/// Each scalar key: one value, two values meaning either, the negation, and
/// any case typed. A ticket whose record misses the field (no priority) is
/// left out of the key and kept by its negation.
#[tokio::test]
async fn each_scalar_key_matches_its_field_any_value_and_negates() {
    let pool = pool().await;
    let p = project();
    let a = store(
        &pool,
        &p,
        1,
        Ticket {
            status: "Done",
            issuetype: "Bug",
            priority: Some("High"),
            assignee: Some("jonas.becker"),
            ..BASE
        },
    )
    .await;
    let b = store(
        &pool,
        &p,
        2,
        Ticket {
            status: "In Review",
            issuetype: "Story",
            priority: Some("Low"),
            assignee: Some("mara.lindqvist"),
            ..BASE
        },
    )
    .await;
    let c = store(
        &pool,
        &p,
        3,
        Ticket {
            status: "To Do",
            issuetype: "Task",
            priority: None,
            ..BASE
        },
    )
    .await;

    for (raw, expected) in [
        ("status:Done", vec![&a]),
        ("status:done", vec![&a]),
        (r#"status:Done status:"In Review""#, vec![&a, &b]),
        ("-status:Done", vec![&b, &c]),
        (r#"-status:Done -status:"in review""#, vec![&c]),
        ("issuetype:Bug", vec![&a]),
        ("issuetype:BUG issuetype:Task", vec![&a, &c]),
        ("-issuetype:Story", vec![&a, &c]),
        ("priority:High", vec![&a]),
        ("priority:High priority:Low", vec![&a, &b]),
        // No priority at all: not High, and so kept by the negation.
        ("-priority:High", vec![&b, &c]),
        ("assignee:jonas.becker", vec![&a]),
        ("assignee:@me", vec![&b]),
        ("assignee:me assignee:Jonas.Becker", vec![&a, &b]),
        ("-assignee:@me", vec![&a, &c]),
        // Every condition must hold.
        ("status:Done priority:Low", vec![]),
        ("status:Done -priority:Low", vec![&a]),
    ] {
        let expected = sorted(expected.into_iter().cloned().collect());
        assert_eq!(answer(&pool, &p, raw).await, expected, "{raw}");
    }
}

/// `project:` matches the key or the name, ignoring case, and a second
/// project means either.
#[tokio::test]
async fn project_matches_the_key_or_the_name() {
    let pool = pool().await;
    let (p, o) = (project(), project());
    let ours = store(&pool, &p, 1, BASE).await;
    let theirs = store(&pool, &o, 1, BASE).await;
    let s = searcher(&pool).await;
    for (raw, expected) in [
        (format!("project:{p}"), vec![&ours]),
        (format!("project:{}", p.to_lowercase()), vec![&ours]),
        (format!(r#"project:"{p} payments""#), vec![&ours]),
        (format!("project:{p} project:{o}"), vec![&ours, &theirs]),
    ] {
        let got = ids(&s.search(q(&raw)).await.unwrap());
        assert_eq!(
            got,
            sorted(expected.into_iter().cloned().collect()),
            "{raw}"
        );
    }
    let got = ids(&s
        .search(q(&format!("project:{p} -project:{p}")))
        .await
        .unwrap());
    assert!(got.is_empty(), "{got:?}");
}

/// `label:` matches any of an item's labels, ignoring case; repeated values
/// mean any; the negation keeps an item with no labels.
#[tokio::test]
async fn label_matches_any_of_an_items_labels() {
    let pool = pool().await;
    let p = project();
    let a = store(
        &pool,
        &p,
        1,
        Ticket {
            labels: &["sepa", "psp"],
            ..BASE
        },
    )
    .await;
    let b = store(
        &pool,
        &p,
        2,
        Ticket {
            labels: &["tech-debt"],
            ..BASE
        },
    )
    .await;
    let c = store(&pool, &p, 3, BASE).await;
    for (raw, expected) in [
        ("label:psp", vec![&a]),
        ("label:SEPA", vec![&a]),
        ("label:psp label:tech-debt", vec![&a, &b]),
        ("-label:tech-debt", vec![&a, &c]),
    ] {
        let expected = sorted(expected.into_iter().cloned().collect());
        assert_eq!(answer(&pool, &p, raw).await, expected, "{raw}");
    }
}

/// `sprint:current` is a ticket in an active sprint: not one only in a
/// future or closed one, nor one in none.
#[tokio::test]
async fn sprint_current_is_a_ticket_in_an_active_sprint() {
    let pool = pool().await;
    let p = project();
    let active = store(
        &pool,
        &p,
        1,
        Ticket {
            sprints: &["closed", "active"],
            ..BASE
        },
    )
    .await;
    let future = store(
        &pool,
        &p,
        2,
        Ticket {
            sprints: &["future"],
            ..BASE
        },
    )
    .await;
    let closed = store(
        &pool,
        &p,
        3,
        Ticket {
            sprints: &["closed"],
            ..BASE
        },
    )
    .await;
    let none = store(&pool, &p, 4, BASE).await;
    assert_eq!(answer(&pool, &p, "sprint:current").await, vec![active]);
    assert_eq!(
        answer(&pool, &p, "-sprint:current").await,
        sorted(vec![future, closed, none])
    );
}

/// `assigned:` is assigned to the user and changed within the window --
/// the inbox's admission, since the record holds no assignment time.
#[tokio::test]
async fn assigned_is_mine_and_changed_within_the_window() {
    let pool = pool().await;
    let p = project();
    let mine = Ticket {
        assignee: Some("mara.lindqvist"),
        ..BASE
    };
    let today = store(&pool, &p, 1, mine.clone()).await;
    let this_week = store(
        &pool,
        &p,
        2,
        Ticket {
            age_days: 4,
            ..mine.clone()
        },
    )
    .await;
    store(
        &pool,
        &p,
        3,
        Ticket {
            age_days: 20,
            ..mine
        },
    )
    .await;
    store(
        &pool,
        &p,
        4,
        Ticket {
            assignee: Some("jonas.becker"),
            ..BASE
        },
    )
    .await;
    assert_eq!(
        answer(&pool, &p, "assigned:today").await,
        vec![today.clone()]
    );
    assert_eq!(
        answer(&pool, &p, "assigned:week").await,
        sorted(vec![today, this_week])
    );
}

/// A key alone is a query somebody asked, not a browse to refuse; and a
/// note, which has none of a ticket's fields, matches no value of one.
#[tokio::test]
async fn a_ticket_key_alone_is_a_query_and_matches_no_note() {
    let pool = pool().await;
    let p = project();
    let ticket = store(
        &pool,
        &p,
        1,
        Ticket {
            status: "Blocked",
            ..BASE
        },
    )
    .await;
    let note = format!("note:{}", uuid::Uuid::new_v4());
    for statement in [
        "insert into knobas.entity (id, kind, title) values ($1, 'note', $2)",
        "insert into knobas.note (id, title, body_md) values ($1, $2, 'Blocked')",
    ] {
        sqlx::query(statement)
            .bind(&note)
            .bind(format!("{p} note"))
            .execute(&pool)
            .await
            .unwrap();
    }
    let got = ids(&searcher(&pool)
        .await
        .search(q("status:Blocked"))
        .await
        .unwrap());
    assert!(got.contains(&ticket), "{got:?}");
    assert!(got.iter().all(|id| id.starts_with("jira:")), "{got:?}");
    // And the negation holds on it: a note is not Done.
    let got = ids(&searcher(&pool)
        .await
        .search(q(&format!("{p} -status:Done")))
        .await
        .unwrap());
    assert!(got.contains(&note), "{got:?}");
}
