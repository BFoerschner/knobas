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
        // The negation keeps the other project's ticket.
        (
            format!("project:{p} project:{o} -project:{p}"),
            vec![&theirs],
        ),
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
    let stale = store(
        &pool,
        &p,
        3,
        Ticket {
            age_days: 20,
            ..mine
        },
    )
    .await;
    let theirs = store(
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
    // The negation keeps what is not mine-and-recent: mine but stale, and
    // somebody else's however recent.
    assert_eq!(
        answer(&pool, &p, "-assigned:week").await,
        sorted(vec![stale, theirs])
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

/// A mirror row whose source declares no path for the field takes the
/// mirror's own predicate, not the constant one a note gets: the declared
/// read finds no candidate, yields null, and so fails the key and passes its
/// negation. Here a kind the Jira descriptor declares nothing for, even with
/// a record that has a `fields.status.name` a payload-key read would find.
#[tokio::test]
async fn a_mirror_row_whose_kind_declares_no_path_fails_the_key_and_passes_its_negation() {
    let pool = pool().await;
    let p = project();
    let id = format!("jira:{p}-page");
    let title = format!("{p} page");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'undeclared', $2)")
        .bind(&id)
        .bind(&title)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                item_updated_at)
         values ($1, 'jira', 'undeclared', $2, $2, $3, now())",
    )
    .bind(&id)
    .bind(&title)
    .bind(serde_json::json!({ "fields": { "status": { "name": "Done" } } }))
    .execute(&pool)
    .await
    .unwrap();
    let s = searcher(&pool).await;
    for (raw, found) in [("status:Done", false), ("-status:Done", true)] {
        let got = ids(&s.search(q(&format!("{p} {raw}"))).await.unwrap());
        assert_eq!(got.contains(&id), found, "{raw}: {got:?}");
    }
}

/// `filter_suggestions` offers the eight keys' values (#811): the values the
/// live mirror holds for each field, in the source's own spelling, a
/// project with its name beside it, `@me` first among the assignees,
/// `current` for the sprint only while a ticket is in an active one, and
/// `assigned:`'s two windows. The parents-only read carries none of them.
#[tokio::test]
async fn filter_suggestions_offer_each_keys_values() {
    let pool = pool().await;
    let p = project();
    store(
        &pool,
        &p,
        1,
        Ticket {
            status: "In Review",
            issuetype: "Bug",
            priority: Some("High"),
            assignee: Some("jonas.becker"),
            labels: &["sepa", "psp"],
            sprints: &["active"],
            ..BASE
        },
    )
    .await;
    // A ticket missing priority, assignee and labels offers nothing for them:
    // a miss is absence, never an empty or a null value.
    store(
        &pool,
        &p,
        2,
        Ticket {
            priority: None,
            ..BASE
        },
    )
    .await;
    let s = searcher(&pool).await;
    let lists = s.filter_suggestions("").await.unwrap();
    assert!(
        lists.fields.iter().all(|f| !f.value.trim().is_empty()),
        "{:?}",
        lists.fields
    );
    let values = |key: &str| -> Vec<(String, String)> {
        lists
            .fields
            .iter()
            .filter(|f| f.key == key)
            .map(|f| (f.value.clone(), f.detail.clone()))
            .collect()
    };
    let has = |key: &str, value: &str| values(key).iter().any(|(v, _)| v == value);
    assert!(
        values("project").contains(&(p.clone(), format!("{p} Payments"))),
        "{:?}",
        values("project")
    );
    assert!(has("status", "In Review"), "{:?}", values("status"));
    assert!(has("issuetype", "Bug"), "{:?}", values("issuetype"));
    assert!(has("priority", "High"), "{:?}", values("priority"));
    assert!(
        has("label", "psp") && has("label", "sepa"),
        "{:?}",
        values("label")
    );
    let assignees = values("assignee");
    assert_eq!(
        assignees.first().map(|(v, _)| v.as_str()),
        Some("@me"),
        "{assignees:?}"
    );
    assert!(has("assignee", "jonas.becker"), "{assignees:?}");
    assert_eq!(
        values("sprint"),
        [("current".to_owned(), "the active sprint".to_owned())]
    );
    let windows: Vec<String> = values("assigned").into_iter().map(|(v, _)| v).collect();
    assert_eq!(windows, ["today", "week"]);
    // Every value offered is one the grammar reads as a filter.
    for f in &lists.fields {
        let raw = if f.value.contains(' ') {
            format!("{}:\"{}\"", f.key, f.value)
        } else {
            format!("{}:{}", f.key, f.value)
        };
        let response = s.search(q(&raw)).await.unwrap();
        assert!(response.interpreted.unknown_tokens.is_empty(), "{raw}");
    }

    assert!(
        s.filter_parent_suggestions("")
            .await
            .unwrap()
            .fields
            .is_empty()
    );
}

/// No ticket in an active sprint, no `current` to offer: picking it would
/// lead to an empty result.
#[tokio::test]
async fn no_active_sprint_offers_no_current() {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    // This file's database holds other tests' active sprints, so the
    // question is asked with no declaration: nothing reads as a sprint.
    let lists = Searcher::new(pool.clone())
        .filter_suggestions("")
        .await
        .unwrap();
    assert!(
        lists.fields.iter().all(|f| f.key != "sprint"),
        "{:?}",
        lists.fields
    );
}

/// A list saved from a ticket-key query counts and lists the rows the query
/// answers with: the rail's count and the rows behind it come from one
/// predicate (`sql::corpus_predicates`), ticket keys included.
#[tokio::test]
async fn a_saved_list_of_ticket_keys_counts_what_it_lists() {
    let pool = pool().await;
    let p = project();
    let done = store(
        &pool,
        &p,
        1,
        Ticket {
            status: "Done",
            ..BASE
        },
    )
    .await;
    store(&pool, &p, 2, BASE).await;
    let s = searcher(&pool).await;
    let saved = s
        .save_list(
            &format!("{p} done"),
            &format!("project:{p} -status:\"To Do\" sprint:current status:done -sprint:current"),
        )
        .await
        .unwrap();
    // Contradictory on purpose: `sprint:current` and its negation leave nothing.
    assert_eq!(saved.count, 0);
    let saved = s
        .save_list(
            &format!("{p} done 2"),
            &format!("project:{p} -status:\"To Do\""),
        )
        .await
        .unwrap();
    assert_eq!(saved.count, 1);
    let rows = s.smart_list_items(&saved.id, 10).await.unwrap();
    assert_eq!(ids(&rows), vec![done]);
    let rail = s.smart_lists().await.unwrap();
    assert_eq!(
        rail.iter().find(|l| l.id == saved.id).map(|l| l.count),
        Some(1)
    );
}

/// The Tickets view's server-side filter (#810): every ticket the grammar
/// admits, past the launcher's ten a kind, and only tickets. A query with
/// nothing in it narrows nothing.
#[tokio::test]
async fn matching_ids_are_every_ticket_the_query_admits() {
    let pool = pool().await;
    let p = project();
    let mut bugs = Vec::new();
    for n in 1..=12 {
        bugs.push(
            store(
                &pool,
                &p,
                n,
                Ticket {
                    issuetype: "Bug",
                    ..BASE
                },
            )
            .await,
        );
    }
    let story = store(&pool, &p, 13, BASE).await;
    sqlx::query("update sync.item set title = 'Retry failed SEPA payouts' where entity_id = $1")
        .bind(&story)
        .execute(&pool)
        .await
        .unwrap();
    let searcher = searcher(&pool).await;
    assert_eq!(
        searcher
            .matching_ids(&format!("project:{p} payouts retr"), "ticket")
            .await
            .unwrap(),
        Some(vec![story.clone()]),
        "every word must appear, a part of a word as well"
    );
    // The key, the assignee and the epic are words of the row too (R-2.3,
    // "Filter by key, summary, epic or person"); the body is not.
    let key = format!("{p}-13");
    sqlx::query(
        "update sync.item set payload = payload
              || jsonb_build_object('epic', jsonb_build_object('key', $2::text))
              || jsonb_build_object('fields', (payload -> 'fields')
                   || '{\"assignee\": {\"name\": \"priya.nair\", \"key\": \"priya.nair\"}}'::jsonb),
              body_text = 'a description that mentions zeppelins'
          where entity_id = $1",
    )
    .bind(&story)
    .bind(format!("{p}-EPIC"))
    .execute(&pool)
    .await
    .unwrap();
    for typed in [
        key.to_lowercase(),
        "PRIYA sepa".to_owned(),
        format!("{p}-epic retry"),
    ] {
        assert_eq!(
            searcher
                .matching_ids(&format!("project:{p} {typed}"), "ticket")
                .await
                .unwrap(),
            Some(vec![story.clone()]),
            "{typed}"
        );
    }
    assert_eq!(
        searcher
            .matching_ids(&format!("project:{p} zeppelins"), "ticket")
            .await
            .unwrap(),
        Some(Vec::new()),
        "a word only the description holds is not a word of the row"
    );
    // The key is the source's, `PAY-13`: the entity id's namespace is not
    // drawn, so it is not a word of the row either.
    assert_eq!(
        searcher
            .matching_ids(&format!("project:{p} jira"), "ticket")
            .await
            .unwrap(),
        Some(Vec::new()),
        "the namespace is not a word of the row"
    );

    let matched = searcher
        .matching_ids(&format!("project:{p} issuetype:bug"), "ticket")
        .await
        .unwrap()
        .expect("a query with keys narrows");
    assert_eq!(sorted(matched), sorted(bugs.clone()));

    // Only live tickets (#1163, which reads the base table by exclusion):
    // a withdrawn one, and a copy of one in a source that is switched off.
    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(&bugs[0])
        .execute(&pool)
        .await
        .unwrap();
    let off = format!("{p}-off");
    let copy = format!("{off}:{p}-99");
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, enabled)
         values ($1, 'jira', 'Off', 'http://x', 'Pat', false)",
    )
    .bind(&off)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', $1)")
        .bind(&copy)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         select $1, $2, kind, title, body_text, payload from sync.item where entity_id = $3",
    )
    .bind(&copy)
    .bind(&off)
    .bind(&bugs[1])
    .execute(&pool)
    .await
    .unwrap();
    let live = searcher
        .matching_ids(&format!("project:{p} issuetype:bug"), "ticket")
        .await
        .unwrap()
        .expect("a query with keys narrows");
    assert_eq!(sorted(live), sorted(bugs[1..].to_vec()));

    assert_eq!(searcher.matching_ids("   ", "ticket").await.unwrap(), None);
    assert_eq!(
        searcher
            .matching_ids(&format!("project:{p} type:page"), "ticket")
            .await
            .unwrap(),
        Some(Vec::new()),
        "a query for another kind matches no ticket"
    );
}

/// The Tickets view reads its words as the launcher grammar does (#1651,
/// #1889): a quoted phrase is one word, a `-word` or a `-"phrase"` leaves
/// out the rows that hold it, and `or` splits the words into alternatives,
/// each binding tighter than `or` as `websearch_to_tsquery` reads them. A
/// row holds a word through its key, title, assignee or epic, so a word
/// left out is left out through each of them. The rail's `readings` agree
/// with `matching_ids` on every text.
#[tokio::test]
async fn the_view_reads_phrases_negations_and_or_as_the_grammar_does() {
    let pool = pool().await;
    let p = project();
    let one = store(&pool, &p, 1, BASE).await;
    let two = store(&pool, &p, 2, BASE).await;
    let three = store(&pool, &p, 3, BASE).await;
    let four = store(&pool, &p, 4, BASE).await;
    for (id, title) in [
        (&one, "Fix sepa retry loop"),
        (&two, "Retry the sepa job"),
        (&three, "Blocked: dead letter queue"),
        (&four, "Ledger letter is dead"),
    ] {
        sqlx::query("update sync.item set title = $2 where entity_id = $1")
            .bind(id)
            .bind(title)
            .execute(&pool)
            .await
            .unwrap();
    }
    // An epic only the second ticket's row names, and an assignee only the
    // fourth's: words a title does not hold.
    sqlx::query(
        "update sync.item set payload = payload
              || jsonb_build_object('epic', jsonb_build_object('key', $2::text))
          where entity_id = $1",
    )
    .bind(&two)
    .bind(format!("{p}-EPIC"))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "update sync.item set payload = jsonb_set(payload, '{fields,assignee}',
              '{\"name\": \"priya.nair\", \"key\": \"priya.nair\"}'::jsonb)
          where entity_id = $1",
    )
    .bind(&four)
    .execute(&pool)
    .await
    .unwrap();
    let searcher = searcher(&pool).await;
    let epic = format!("{p}-epic");
    let key = format!("{p}-1");
    let cases: Vec<(String, Vec<&String>)> = vec![
        ("\"sepa retry\"".to_owned(), vec![&one]),
        ("-zeppelin".to_owned(), vec![&one, &two, &three, &four]),
        ("sepa -retry".to_owned(), vec![]),
        ("-blocked".to_owned(), vec![&one, &two, &four]),
        ("-\"dead letter\"".to_owned(), vec![&one, &two, &four]),
        ("-dead letter".to_owned(), vec![]),
        ("-dead".to_owned(), vec![&one, &two]),
        // Left out through the key, the epic and the assignee.
        (format!("-{key}"), vec![&two, &three, &four]),
        (format!("-{epic}"), vec![&one, &three, &four]),
        ("-PRIYA".to_owned(), vec![&one, &two, &three]),
        ("ledger or \"sepa retry\"".to_owned(), vec![&one, &four]),
        ("ledger OR loop".to_owned(), vec![&one, &four]),
        // `and` binds tighter than `or`: (dead & letter) | loop, never
        // dead & (letter | loop).
        ("dead letter or loop".to_owned(), vec![&one, &three, &four]),
        // A `-` binds to its own alternative: (sepa & !job) | job, never
        // (sepa | job) & !job.
        ("sepa -job or job".to_owned(), vec![&one, &two]),
        // An `or` with nothing on one side is no alternative.
        ("or ledger or".to_owned(), vec![&four]),
    ];
    let texts: Vec<String> = cases
        .iter()
        .map(|(typed, _)| format!("project:{p} {typed}"))
        .collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let readings = searcher.readings(&refs, "ticket").await.unwrap();
    for ((text, (typed, expected)), reading) in texts.iter().zip(&cases).zip(readings) {
        let ids = searcher
            .matching_ids(text, "ticket")
            .await
            .unwrap()
            .expect("a query with a key narrows");
        assert_eq!(
            reading,
            knobas_search::Reading::Count(ids.len().try_into().unwrap()),
            "readings agree with matching_ids on {typed}"
        );
        assert_eq!(
            sorted(ids),
            sorted(expected.iter().map(|id| (*id).clone()).collect()),
            "{typed}"
        );
    }
    // A negation alone narrows: it is a word filter, not nothing typed.
    let alone = searcher
        .matching_ids(&format!("-{key}"), "ticket")
        .await
        .unwrap()
        .expect("a negated word narrows");
    assert!(alone.contains(&two) && !alone.contains(&one));
}

/// The Tickets view's words with no key beside them (#897), which is what a
/// keystroke on *All tickets* sends: the key, the title and the declared
/// assignee are each a word of the row, and every word is still required.
#[tokio::test]
async fn words_alone_match_the_key_the_title_and_the_declared_assignee() {
    let pool = pool().await;
    let p = project();
    let person: &'static str = Box::leak(format!("{}.nair", p.to_lowercase()).into_boxed_str());
    let theirs = store(
        &pool,
        &p,
        1,
        Ticket {
            assignee: Some(person),
            ..BASE
        },
    )
    .await;
    let nobodys = store(&pool, &p, 2, BASE).await;
    // A real ticket's title is its summary, not its key, so the key is a
    // word of the row only through the key branch of the predicate (a
    // Jira ticket answers as its own epic only when it is one).
    sqlx::query("update sync.item set title = 'Rename the ledger' where entity_id = $1")
        .bind(&nobodys)
        .execute(&pool)
        .await
        .unwrap();
    // A ticket whose payload names no key of its own: nothing in
    // `sync.item_parent` answers for its key, so only the key branch does.
    let bare = format!("jira:{p}-3");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'ticket','Bare')")
        .bind(&bare)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at)
         values ($1,'jira','ticket','Bare','Bare','{}'::jsonb, now())",
    )
    .bind(&bare)
    .execute(&pool)
    .await
    .unwrap();
    let searcher = searcher(&pool).await;
    for (typed, expected) in [
        (format!("{p}-3"), vec![bare.clone()]),
        (person.to_uppercase(), vec![theirs.clone()]),
        (format!("{person} {p}"), vec![theirs.clone()]),
        (format!("{p}-2"), vec![nobodys.clone()]),
        (
            p.clone(),
            vec![theirs.clone(), nobodys.clone(), bare.clone()],
        ),
        (format!("{p}-2 {person}"), Vec::new()),
    ] {
        assert_eq!(
            searcher
                .matching_ids(&typed, "ticket")
                .await
                .unwrap()
                .map(sorted),
            Some(sorted(expected)),
            "{typed}"
        );
    }
}

/// A value the server and Rust lower-case differently (#1843): `İ` is `i̇`
/// (two code points) to Rust's `to_lowercase` and `i` to the server's
/// `lower()`. The typed value must meet the stored one under the server's
/// fold, for each key that compares a name.
#[tokio::test]
async fn a_value_with_a_dotted_capital_i_matches_under_the_servers_fold() {
    let pool = pool().await;
    let p = project();
    let a = store(
        &pool,
        &p,
        1,
        Ticket {
            status: "İn Review",
            issuetype: "İssue",
            priority: Some("İmportant"),
            assignee: Some("İlker"),
            labels: &["İstanbul"],
            ..BASE
        },
    )
    .await;
    let other = store(&pool, &p, 2, BASE).await;
    for (raw, expected) in [
        (r#"status:"İn Review""#, vec![&a]),
        (r#"-status:"İn Review""#, vec![&other]),
        ("issuetype:İssue", vec![&a]),
        ("priority:İmportant", vec![&a]),
        ("assignee:İlker", vec![&a]),
        ("label:İstanbul", vec![&a]),
    ] {
        let expected = sorted(expected.into_iter().cloned().collect());
        assert_eq!(answer(&pool, &p, raw).await, expected, "{raw}");
    }
}

/// The Tickets view's words under the server's fold too (#1988): `İstanbul`
/// is `i̇stanbul` to Rust and `istanbul` to the server, so a word folded in
/// Rust missed the title the server folds. Kept and left out alike.
#[tokio::test]
async fn a_word_with_a_dotted_capital_i_matches_under_the_servers_fold() {
    let pool = pool().await;
    let p = project();
    let rollout = store(&pool, &p, 1, BASE).await;
    sqlx::query("update sync.item set title = 'İstanbul Rollout' where entity_id = $1")
        .bind(&rollout)
        .execute(&pool)
        .await
        .unwrap();
    let other = store(&pool, &p, 2, BASE).await;
    let searcher = searcher(&pool).await;
    for (typed, expected) in [
        ("İstanbul", vec![rollout.clone()]),
        ("-İstanbul", vec![other.clone()]),
        ("İSTANBUL or nothing-at-all", vec![rollout.clone()]),
    ] {
        assert_eq!(
            searcher
                .matching_ids(&format!("project:{p} {typed}"), "ticket")
                .await
                .unwrap()
                .map(sorted),
            Some(expected),
            "{typed}"
        );
    }
}
