//! `sync.item_card` (migration `0075`): each ticket's card as the database
//! keeps it, held to the payload walks it replaces for every field and every
//! adapter that makes tickets, and the reads that take it: the same answer
//! whether every ticket, some or none has a stored card.

use knobas_app::sources::Registry;
use knobas_core::mini_board::{LaneField, Lanes, MiniBoard, Scope};
use knobas_core::payload::Declarations;
use serde_json::{Value, json};
use sqlx::PgPool;

/// This file's database, migrated, the adapters' declarations stored as
/// bring-up stores them (ADR-0019).
async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    declared(&pool).await;
    pool
}

/// A database of its own, for a test that changes what every other test's
/// cards stand on (the declarations, the whole fill).
async fn scratch(label: &str) -> PgPool {
    let pool = knobas_db::test_util::scratch_database(label)
        .await
        .pool(4)
        .await
        .expect("a pool on the scratch database");
    declared(&pool).await;
    pool
}

async fn declared(pool: &PgPool) {
    knobas_app::commands::search::searcher(pool, &Registry::for_process())
        .await
        .unwrap();
}

/// A source of adapter `adapter` nothing else in this database holds.
async fn source(pool: &PgPool, adapter: &str) -> String {
    let source = format!(
        "{adapter}-{}",
        &uuid::Uuid::new_v4().simple().to_string()[..12]
    );
    crate::support::source_config(pool, &source, adapter, adapter, &json!({})).await;
    source
}

/// Write `rows` -- `(key, title, payload)` tickets of `source` -- as one
/// multi-row upsert, the sync writer's shape (a sync, and a write's echo,
/// which the sink writes the same way).
async fn upsert(pool: &PgPool, source: &str, rows: &[(&str, &str, Value)]) {
    let ids: Vec<String> = rows
        .iter()
        .map(|(k, _, _)| format!("{source}:{k}"))
        .collect();
    let titles: Vec<&str> = rows.iter().map(|(_, t, _)| *t).collect();
    let payloads: Vec<&Value> = rows.iter().map(|(_, _, p)| p).collect();
    sqlx::query(
        "insert into knobas.entity (id, kind, title)
         select id, 'ticket', id from unnest($1::text[]) id on conflict (id) do nothing",
    )
    .bind(&ids)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at)
         select t.id, $4, 'ticket', t.title, '', t.payload, now() - (t.n || ' minutes')::interval
           from unnest($1::text[], $2::text[], $3::jsonb[]) with ordinality as t(id, title, payload, n)
             on conflict (entity_id) do update set payload = excluded.payload, title = excluded.title",
    )
    .bind(&ids)
    .bind(&titles)
    .bind(&payloads)
    .bind(source)
    .execute(pool)
    .await
    .unwrap();
}

/// Jira tickets: every field found, every kind of miss (absent, blank, not a
/// string, not a list, a number written as a string), a parent by
/// `fields.parent` and one by `epic`, and the parent itself.
fn jira_tickets() -> Vec<(&'static str, &'static str, Value)> {
    vec![
        (
            "PAY-0",
            "Payout reliability",
            json!({ "key": "PAY-0", "fields": { "status": { "name": "In Progress" },
                "issuetype": { "name": "Epic" } } }),
        ),
        (
            "PAY-1",
            "Retry the payout",
            json!({ "key": "PAY-1", "fields": {
                "status": { "name": "To Do" },
                "assignee": { "name": "mara", "key": "mara-key", "displayName": "Mara Lindqvist" },
                "priority": { "name": "High" },
                "issuetype": { "name": "Story" },
                "labels": ["backend", "  ", "", "payouts"],
                "comment": { "total": 3 },
                "parent": { "key": "PAY-0" } },
                "sprints": [{ "id": 1, "state": "closed" }, { "id": 2, "state": " ACTIVE " },
                            { "id": 3, "state": "" }, { "id": 4 }, { "id": 5, "state": 7 },
                            { "id": 6, "state": "future" }] }),
        ),
        (
            "PAY-2",
            "Ledger drift",
            json!({ "key": "PAY-2", "epic": { "key": "PAY-0" }, "fields": {
                "status": { "name": "  " },
                "assignee": { "key": "jon" },
                "priority": { "name": 3 },
                "labels": "not a list",
                "comment": { "total": "4" } },
                "sprints": { "state": "active" } }),
        ),
        ("PAY-3", "Nothing declared here", json!({})),
    ]
}

/// Gitea issues: the declared `knobas` block, labels as objects, and a
/// parent named in it.
fn gitea_tickets() -> Vec<(&'static str, &'static str, Value)> {
    vec![
        (
            "o/r/issues/1",
            "Cart rejects IBANs",
            json!({ "state": "open", "comments": 2,
                "assignee": { "login": "jonas", "full_name": "Jonas Becker" },
                "labels": [{ "name": "kind/bug" }, { "name": " " }, { "color": "red" }, { "name": "area/pay" }],
                "knobas": { "key": "o/r/issues/1", "priority": "high", "issue_type": null,
                            "parent": { "knobas": { "key": "o/r/issues/9" } } } }),
        ),
        (
            "o/r/issues/9",
            "Payments epic",
            json!({ "state": "closed", "labels": [], "knobas": { "key": "o/r/issues/9" } }),
        ),
        ("o/r/issues/2", "Bare", json!({ "state": 7 })),
    ]
}

/// The mock adapter's tickets, the third adapter that declares them.
fn mock_tickets() -> Vec<(&'static str, &'static str, Value)> {
    vec![
        (
            "M-1",
            "Mock child",
            json!({ "key": "M-1", "status": "Doing", "assignee": "ada", "priority": "P1",
                "type": "Bug", "labels": ["one", "two"], "parent": { "key": "M-0" } }),
        ),
        (
            "M-0",
            "Mock parent",
            json!({ "key": "M-0", "status": "Done" }),
        ),
    ]
}

/// What each stored field is, in [`both`]'s arrays.
const FIELDS: [&str; 15] = [
    "status",
    "assignee",
    "assignee_name",
    "priority",
    "issue_type",
    "path",
    "labels",
    "first_label",
    "comment_count",
    "parent_kind",
    "parent_name",
    "answers_to",
    "parent_text",
    "source_id",
    "sprint_states",
];

/// Every field of each of `source`'s tickets as the payload macros walk it
/// under the bound declarations, the first label being the first of the
/// labels walked (what `MiniBoardCard::lane` reads); the answers-to names and
/// the parent names as sorted sets.
const WALKED: &str = concat!(
    "select i.entity_id, jsonb_build_array(",
    knobas_core::declared_string!("$1", "status_name"),
    ", ",
    knobas_core::declared_string!("$1", "assignee"),
    ", ",
    knobas_core::declared_string!("$1", "assignee_name"),
    ", ",
    knobas_core::declared_string!("$1", "priority"),
    ", ",
    knobas_core::declared_string!("$1", "issue_type"),
    ", ",
    knobas_core::path_line_read!("$1"),
    ", array(select l.value from ",
    knobas_core::declared_list!("$1", "labels"),
    " l), (array(select l.value from ",
    knobas_core::declared_list!("$1", "labels"),
    " l))[1], ",
    knobas_core::declared_integer!("$1", "comment_count"),
    ", (select pn.kind from ",
    knobas_core::declared_parent_names!("$1"),
    " pn order by pn.pos desc, pn.name_ord limit 1), (select pn.name from ",
    knobas_core::declared_parent_names!("$1"),
    " pn order by pn.pos desc, pn.name_ord limit 1), array(select n.name from ",
    knobas_core::declared_names_as_parent!("$1"),
    " n order by 1), array(select lower(ip.name) from sync.item_parent ip
       where ip.entity_id = i.entity_id order by 1), i.source_id,
       array(select l.value from ",
    knobas_core::declared_list!("$1", "sprint_state"),
    " l))
       from sync.item i where i.source_id = $2 order by i.entity_id"
);

/// The same, off each ticket's stored card under the key of the bound
/// declarations; `null` for a ticket with none.
const STORED: &str =
    "select i.entity_id, case when c.entity_id is null then null else jsonb_build_array(
       c.status, c.assignee, c.assignee_name, c.priority, c.issue_type, c.path, c.labels,
       c.first_label, c.comment_count, c.parent_kind, c.parent_name,
       array(select unnest(c.answers_to) order by 1),
       array(select x from unnest(string_to_array(c.parent_text, chr(31))) x order by 1),
       c.source_id, c.sprint_states) end
       from sync.item i
       left join sync.item_card c
         on c.entity_id = i.entity_id and c.paths_key = sync.card_key($1::jsonb -> i.source_id)
      where i.source_id = $2 order by i.entity_id";

async fn declarations(pool: &PgPool) -> Declarations {
    knobas_app::sources::paths::declared_paths(pool, &Registry::for_process())
        .await
        .unwrap()
}

/// `(walked, stored)` per ticket of `source`.
async fn both(pool: &PgPool, source: &str) -> Vec<(String, Value, Option<Value>)> {
    let declared = declarations(pool).await.as_param();
    let walked: Vec<(String, Value)> = sqlx::query_as(WALKED)
        .bind(&declared)
        .bind(source)
        .fetch_all(pool)
        .await
        .unwrap();
    let stored: Vec<(String, Option<Value>)> = sqlx::query_as(STORED)
        .bind(&declared)
        .bind(source)
        .fetch_all(pool)
        .await
        .unwrap();
    assert_eq!(walked.len(), stored.len());
    walked
        .into_iter()
        .zip(stored)
        .map(|((id, walked), (sid, stored))| {
            assert_eq!(id, sid);
            (id, walked, stored)
        })
        .collect()
}

/// Every stored field of every ticket is the walk's, field by field.
fn assert_stored_is_walked(read: &[(String, Value, Option<Value>)], when: &str) {
    for (id, walked, stored) in read {
        let stored = stored
            .as_ref()
            .unwrap_or_else(|| panic!("{when}: {id} has no stored card"));
        for (n, field) in FIELDS.iter().enumerate() {
            assert_eq!(
                stored[n], walked[n],
                "{when}: {id} {field}: the stored value is the walk's"
            );
        }
    }
}

/// The resolver carries the payload macros' expansions as text, because a
/// migration is SQL and cannot call them. This holds that text to what they
/// expand to **now**: change one and this fails until a new migration
/// restates `sync.item_card_rows`.
#[tokio::test]
async fn the_stored_rows_are_the_macros_expansions() {
    let pool = pool().await;
    let source: String = sqlx::query_scalar(
        "select p.prosrc from pg_proc p join pg_namespace n on n.oid = p.pronamespace
          where n.nspname = 'sync' and p.proname = 'item_card_rows'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    for expansion in [
        knobas_core::declared_string!("dd.decl", "status_name"),
        knobas_core::declared_string!("dd.decl", "assignee"),
        knobas_core::declared_string!("dd.decl", "assignee_name"),
        knobas_core::declared_string!("dd.decl", "priority"),
        knobas_core::declared_string!("dd.decl", "issue_type"),
        knobas_core::path_line_read!("dd.decl"),
        knobas_core::declared_list!("dd.decl", "labels"),
        knobas_core::declared_list!("dd.decl", "sprint_state"),
        knobas_core::declared_integer!("dd.decl", "comment_count"),
        knobas_core::declared_parent_names!("dd.decl"),
        knobas_core::declared_names_as_parent!("dd.decl"),
        knobas_core::declared_array!("dd.decl", "labels"),
        knobas_core::declared_segments!("cand->'at'"),
        knobas_core::declared_segments!("cand->'entry'"),
    ] {
        assert!(
            source.contains(expansion),
            "sync.item_card_rows no longer carries a macro's expansion; \
             restate it in a new migration:\n{expansion}"
        );
    }
}

/// Every field of every ticket, for each adapter that makes tickets, is what
/// the walk reads: on the first write, after a sync rewrites the payload, and
/// untouched by a stamp, which writes nothing here (#2189).
#[tokio::test]
async fn a_stored_card_is_the_walks_answer_for_every_field_and_adapter() {
    let pool = pool().await;
    for (adapter, tickets) in [
        ("jira", jira_tickets()),
        ("gitea", gitea_tickets()),
        ("mock", mock_tickets()),
    ] {
        let source = source(&pool, adapter).await;
        upsert(&pool, &source, &tickets).await;
        let read = both(&pool, &source).await;
        assert_eq!(read.len(), tickets.len());
        assert_stored_is_walked(&read, adapter);
        // Something found for every field somewhere, so no field passes by
        // being null on both sides.
        if adapter == "jira" {
            let child = &read
                .iter()
                .find(|(id, _, _)| id.ends_with(":PAY-1"))
                .unwrap()
                .1;
            assert_eq!(
                child.as_array().unwrap()[..11],
                json!([
                    "To Do",
                    "mara",
                    "Mara Lindqvist",
                    "High",
                    "Story",
                    "PAY-0",
                    ["backend", "payouts"],
                    "backend",
                    3,
                    "ticket",
                    "PAY-0"
                ])
                .as_array()
                .unwrap()[..],
            );
            assert!(child[12].as_array().unwrap().contains(&json!("pay-0")));
            // Every sprint's state that is a string, trimmed, in the record's
            // order: a blank, an absent and a numeric one are left out.
            assert_eq!(child[14], json!(["closed", "ACTIVE", "future"]));
            let parent = &read
                .iter()
                .find(|(id, _, _)| id.ends_with(":PAY-0"))
                .unwrap()
                .1;
            assert_eq!(parent[11], json!(["PAY-0"]), "the epic answers to its key");
        }

        // A sync that rewrites a payload rewrites its card.
        let (key, title, _) = &tickets[0];
        upsert(
            &pool,
            &source,
            &[(
                key,
                title,
                json!({ "key": "X", "status": "Moved", "state": "moved",
                "fields": { "status": { "name": "Moved" }, "labels": ["moved"] } }),
            )],
        )
        .await;
        assert_stored_is_walked(
            &both(&pool, &source).await,
            &format!("{adapter}, rewritten"),
        );

        // A stamp leaves the card where it is.
        let before: Vec<(String, String)> = sqlx::query_as(
            "select c.entity_id, c.xmin::text from sync.item_card c
               join sync.item i on i.entity_id = c.entity_id where i.source_id = $1 order by 1",
        )
        .bind(&source)
        .fetch_all(&pool)
        .await
        .unwrap();
        sqlx::query(
            "update sync.item set synced_at = now() + interval '1 second' where source_id = $1",
        )
        .bind(&source)
        .execute(&pool)
        .await
        .unwrap();
        let after: Vec<(String, String)> = sqlx::query_as(
            "select c.entity_id, c.xmin::text from sync.item_card c
               join sync.item i on i.entity_id = c.entity_id where i.source_id = $1 order by 1",
        )
        .bind(&source)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(before, after, "{adapter}: a stamp writes no card");
    }
}

/// Every read a typed word in Tickets makes, over `sources`' tickets: the
/// board as the page reads it and in each kind of lane, a saved view's
/// statuses in lanes, the sources' statuses, and the search's ids for words
/// that each reach another field (the key, the title, the assignee, a
/// parent's name), two words, a phrase, a word left out, alternatives and a
/// word nothing holds. The ids are cut to `sources`' tickets, which nothing
/// else in this file writes.
async fn every_read(pool: &PgPool, sources: &[String], ids: &[String]) -> Vec<String> {
    let registry = Registry::for_process();
    let declared = declarations(pool).await;
    let mut out = Vec::new();
    let board = |lanes: Option<Lanes>, per_column: Option<u32>| {
        let declared = &declared;
        async move {
            let board: MiniBoard = knobas_core::mini_board::read_scoped(
                pool,
                Scope {
                    sources,
                    identity: &["MARA".to_owned()],
                    lanes,
                    ..Scope::default()
                },
                declared,
                per_column,
            )
            .await
            .unwrap();
            serde_json::to_string(&board).unwrap()
        }
    };
    out.push(board(None, None).await);
    out.push(board(None, Some(1)).await);
    for by in [
        LaneField::Epic,
        LaneField::Assignee,
        LaneField::Priority,
        LaneField::IssueType,
        LaneField::Label,
    ] {
        out.push(board(Some(Lanes { by, first: None }), Some(1)).await);
        out.push(board(Some(Lanes { by, first: Some(1) }), None).await);
        let placed = knobas_core::mini_board::statuses_of(pool, ids, &declared, Some(by))
            .await
            .unwrap();
        out.push(format!("{:?} {:?}", placed.columns, placed.sources));
    }
    let only: Vec<String> = ids.iter().take(3).cloned().collect();
    let kept: MiniBoard = knobas_core::mini_board::read_scoped(
        pool,
        Scope {
            only: Some(&only),
            ..Scope::default()
        },
        &declared,
        None,
    )
    .await
    .unwrap();
    out.push(serde_json::to_string(&kept).unwrap());
    let sources_set = sources.iter().cloned().collect();
    out.push(format!(
        "{:?}",
        knobas_core::mini_board::observed_statuses(pool, &sources_set, &declared)
            .await
            .unwrap()
            .iter()
            .map(|s| (s.source_id.clone(), s.statuses.clone()))
            .collect::<Vec<_>>()
    ));
    let searcher = knobas_app::commands::search::searcher(pool, &registry)
        .await
        .unwrap();
    for words in [
        "pay-1",
        "payout",
        "mara",
        "jon",
        "pay-0",
        "o/r/issues/9",
        "m-0",
        "mara payout",
        "\"retry the\"",
        "pay -mara",
        "mara or jonas",
        "zzqx",
        // The keys a stored card answers as a set (`priority:`,
        // `issuetype:`, `label:`) and the ones `sync.item_field` does,
        // each also left out, whose answer keeps the tickets that miss.
        "priority:high",
        "-priority:high",
        "issuetype:story issuetype:epic",
        "-issuetype:story",
        "label:backend",
        "label:kind/bug label:payouts",
        "-label:backend",
        "status:\"to do\"",
        "status:open status:\"in progress\"",
        "-status:open",
        "assignee:mara",
        "-assignee:jonas",
        "priority:high pay",
    ] {
        let mut matched = searcher
            .matching_ids(words, "ticket")
            .await
            .unwrap()
            .unwrap_or_default();
        matched.retain(|id| ids.contains(id));
        matched.sort();
        out.push(format!("{words}: {matched:?}"));
    }
    out
}

/// The reads answer the same whether every ticket has a stored card, some
/// do, or none: a ticket with none walks its payload, as before `0075`, and
/// the fill brings the cards back to the same answer.
#[tokio::test]
async fn the_reads_answer_the_same_with_every_card_some_or_none() {
    let pool = pool().await;
    let jira = source(&pool, "jira").await;
    let gitea = source(&pool, "gitea").await;
    upsert(&pool, &jira, &jira_tickets()).await;
    upsert(&pool, &gitea, &gitea_tickets()).await;
    let sources = vec![jira.clone(), gitea.clone()];
    let mut ids: Vec<String> = sqlx::query_scalar(
        "select entity_id from sync.item where source_id = any($1) order by entity_id",
    )
    .bind(&sources)
    .fetch_all(&pool)
    .await
    .unwrap();
    ids.sort();
    let cards = async |pool: &PgPool| -> i64 {
        sqlx::query_scalar("select count(*) from sync.item_card where entity_id = any($1)")
            .bind(&ids)
            .fetch_one(pool)
            .await
            .unwrap()
    };
    assert_eq!(
        cards(&pool).await,
        ids.len() as i64,
        "every ticket written has its card"
    );
    let every = every_read(&pool, &sources, &ids).await;
    assert!(
        every.iter().any(|read| read.contains("Mara Lindqvist")),
        "the board drew the cards: {every:?}"
    );

    let half: Vec<&String> = ids.iter().step_by(2).collect();
    sqlx::query("delete from sync.item_card where entity_id = any($1)")
        .bind(&half)
        .execute(&pool)
        .await
        .unwrap();
    assert!(cards(&pool).await < ids.len() as i64);
    let some = every_read(&pool, &sources, &ids).await;
    for (n, (every, some)) in every.iter().zip(&some).enumerate() {
        assert_eq!(some, every, "read {n}: half the cards stored");
    }

    sqlx::query("delete from sync.item_card where entity_id = any($1)")
        .bind(&ids)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(cards(&pool).await, 0);
    let none = every_read(&pool, &sources, &ids).await;
    for (n, (every, none)) in every.iter().zip(&none).enumerate() {
        assert_eq!(none, every, "read {n}: no card stored");
    }

    knobas_core::item_card::fill(&pool).await.unwrap();
    assert_eq!(
        cards(&pool).await,
        ids.len() as i64,
        "the fill wrote them again"
    );
    let filled = every_read(&pool, &sources, &ids).await;
    for (n, (every, filled)) in every.iter().zip(&filled).enumerate() {
        assert_eq!(filled, every, "read {n}: filled");
    }
}

/// `(source, status)` as `statement` answers for `among`, in one order, on
/// `conn`.
async fn statuses(
    conn: &mut sqlx::PgConnection,
    statement: &'static str,
    among: Option<&[String]>,
    declared: &sqlx::types::Json<Value>,
) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = sqlx::query_as(statement)
        .persistent(false)
        .bind(among)
        .bind(declared)
        .fetch_all(conn)
        .await
        .unwrap();
    rows.sort();
    rows
}

/// The statuses a board's sources show, read off the cards' index (migration
/// `0078`), are the ones the earlier statement read by joining every live
/// ticket to its card: for the sources asked and for every source, with
/// every card stored, with a source's cards half gone (it is then read the
/// earlier way), with a status only a withdrawn ticket holds, and with a
/// source switched off.
#[tokio::test]
async fn the_statuses_off_the_cards_index_are_the_joined_reads() {
    let pool = pool().await;
    let jira = source(&pool, "jira").await;
    let gitea = source(&pool, "gitea").await;
    let off = source(&pool, "jira").await;
    upsert(&pool, &jira, &jira_tickets()).await;
    upsert(&pool, &gitea, &gitea_tickets()).await;
    upsert(&pool, &off, &jira_tickets()).await;
    // A status one ticket alone holds.
    upsert(
        &pool,
        &jira,
        &[(
            "PAY-9",
            "Withdrawn",
            json!({ "key": "PAY-9", "fields": { "status": { "name": "Only Withdrawn" } } }),
        )],
    )
    .await;
    let declared = declarations(&pool).await.as_param();
    let asked = vec![jira.clone(), gitea.clone(), off.clone()];
    let now = knobas_core::mini_board::planned_observed_statuses();
    let before = knobas_core::mini_board::planned_observed_statuses_joined();

    // One snapshot for both statements: the other tests of this file write
    // tickets of their own, which the read of every source sees.
    let both = async |among: Option<&[String]>, when: &str| -> Vec<(String, String)> {
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("set transaction isolation level repeatable read")
            .execute(&mut *tx)
            .await
            .unwrap();
        let read = statuses(&mut tx, now, among, &declared).await;
        let joined = statuses(&mut tx, before, among, &declared).await;
        assert_eq!(read, joined, "{when}");
        read
    };

    let whole = both(Some(&asked), "every card stored").await;
    assert!(
        whole.contains(&(jira.clone(), "Only Withdrawn".to_owned()))
            && whole.contains(&(gitea.clone(), "open".to_owned())),
        "{whole:?}"
    );
    both(None, "every source").await;

    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(format!("{jira}:PAY-9"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("update knobas.source_config set enabled = false where id = $1")
        .bind(&off)
        .execute(&pool)
        .await
        .unwrap();
    let live = both(Some(&asked), "a withdrawn ticket and a source switched off").await;
    assert!(
        !live.iter().any(|(_, status)| status == "Only Withdrawn")
            && !live.iter().any(|(source, _)| *source == off),
        "{live:?}"
    );
    both(None, "every source, after").await;

    sqlx::query(
        "delete from sync.item_card where entity_id in (
           select entity_id from sync.item where source_id = $1 order by entity_id limit 2)",
    )
    .bind(&jira)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        both(Some(&asked), "a source with cards missing").await,
        live
    );
    both(None, "every source, cards missing").await;
}

/// The reads take a stored card's fields where there is one, and nothing
/// else: shown by a card nothing but this test would write.
#[tokio::test]
async fn the_reads_take_the_stored_card() {
    let pool = pool().await;
    let jira = source(&pool, "jira").await;
    upsert(&pool, &jira, &jira_tickets()).await;
    let id = format!("{jira}:PAY-1");
    sqlx::query(
        "update sync.item_card set status = 'Stored', priority = 'Stored', assignee = 'zzstored',
                labels = '{stored}', parent_text = 'zzparent'
          where entity_id = $1",
    )
    .bind(&id)
    .execute(&pool)
    .await
    .unwrap();
    let declared = declarations(&pool).await;
    let only = vec![id.clone()];
    let board = knobas_core::mini_board::read_scoped(
        &pool,
        Scope {
            only: Some(&only),
            ..Scope::default()
        },
        &declared,
        None,
    )
    .await
    .unwrap();
    let card = &board.columns[0].cards[0];
    assert_eq!(
        (
            card.status.as_deref(),
            card.priority.as_deref(),
            card.assignee.as_deref(),
            card.labels.clone()
        ),
        (
            Some("Stored"),
            Some("Stored"),
            Some("zzstored"),
            vec!["stored".to_owned()]
        )
    );
    let searcher = knobas_app::commands::search::searcher(&pool, &Registry::for_process())
        .await
        .unwrap();
    for word in ["zzstored", "zzparent"] {
        let matched = searcher
            .matching_ids(word, "ticket")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(matched, only, "{word} is the stored card's");
    }
}

/// A word holding the character the stored parent names are joined by is
/// held to the names one by one, so it never matches across two of them.
#[tokio::test]
async fn a_word_never_matches_across_two_parent_names() {
    let pool = pool().await;
    let jira = source(&pool, "jira").await;
    upsert(&pool, &jira, &jira_tickets()).await;
    let id = format!("{jira}:PAY-1");
    let joined: Option<String> =
        sqlx::query_scalar("select parent_text from sync.item_card where entity_id = $1")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(joined.unwrap().split('\u{1f}').any(|name| name == "pay-0"));
    // Two names stored, as two parents would be.
    sqlx::query(
        "insert into sync.item_parent (entity_id, adapter_kind, name, kind, arm, source_id)
         values ($1, 'jira', 'Zeta', 'ticket', 'child', $2)",
    )
    .bind(&id)
    .bind(&jira)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("select sync.resolve_item_cards(array[$1])")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    // Joined in the one order a word could run across them in.
    sqlx::query(
        "update sync.item_card set parent_text = 'pay-0' || chr(31) || 'zeta' where entity_id = $1",
    )
    .bind(&id)
    .execute(&pool)
    .await
    .unwrap();
    let searcher = knobas_app::commands::search::searcher(&pool, &Registry::for_process())
        .await
        .unwrap();
    let across = searcher
        .matching_ids("\"0\u{1f}zeta\"", "ticket")
        .await
        .unwrap()
        .unwrap();
    assert!(!across.contains(&id), "no name holds 0␟zeta: {across:?}");
    let within = searcher
        .matching_ids("zeta", "ticket")
        .await
        .unwrap()
        .unwrap();
    assert!(within.contains(&id), "a name holds zeta");
}

/// A change to the declarations that reaches tickets drops every card, and
/// the reads walk until the fill writes them again, under the new paths. A
/// fill writes only the tickets with no card, so a card a sync wrote while it
/// ran is left as that sync wrote it.
#[tokio::test]
async fn a_declaration_change_drops_the_cards_and_the_fill_writes_them_again() {
    let pool = scratch("item-card-declarations").await;
    let jira = source(&pool, "jira").await;
    upsert(&pool, &jira, &jira_tickets()).await;
    let count = async |pool: &PgPool| -> i64 {
        sqlx::query_scalar("select count(*) from sync.item_card")
            .fetch_one(pool)
            .await
            .unwrap()
    };
    assert_eq!(count(&pool).await, 4);

    // Another adapter's change that does not reach tickets keeps them.
    sqlx::query(
        "update sync.parent_declaration
            set payload_paths = payload_paths || '[]'::jsonb || jsonb_build_array(jsonb_build_object('kind', 'nothing'))
          where adapter_kind = 'kuma'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(count(&pool).await, 4, "kuma's declarations reach no ticket");

    sqlx::query(
        "update sync.parent_declaration set payload_paths = payload_paths || '[]'::jsonb
          where adapter_kind = 'jira' and false",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(count(&pool).await, 4, "a statement that changed nothing");

    // Jira's declarations as the binary has them, written again changed.
    sqlx::query(
        "update sync.parent_declaration
            set payload_paths = (select jsonb_agg(case when kp->>'kind' = 'ticket'
                                   then kp || '{\"priority\": [[\"fields\", \"status\", \"name\"]]}'::jsonb
                                   else kp end)
                                   from jsonb_array_elements(payload_paths) kp)
          where adapter_kind = 'jira'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(count(&pool).await, 0, "jira's change reaches its tickets");
    let read = both(&pool, &jira).await;
    assert!(read.iter().all(|(_, _, stored)| stored.is_none()));

    // The stored declarations are the binary's again (bring-up writes them),
    // the cards are dropped once more, and the fill writes them under them.
    declared(&pool).await;
    knobas_core::item_card::fill(&pool).await.unwrap();
    assert_stored_is_walked(&both(&pool, &jira).await, "filled");

    let id = format!("{jira}:PAY-1");
    sqlx::query("update sync.item_card set status = 'Synced meanwhile' where entity_id = $1")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("delete from sync.item_card where entity_id = $1")
        .bind(format!("{jira}:PAY-2"))
        .execute(&pool)
        .await
        .unwrap();
    knobas_core::item_card::fill(&pool).await.unwrap();
    let status: String =
        sqlx::query_scalar("select status from sync.item_card where entity_id = $1")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "Synced meanwhile", "a card that was there is kept");
    assert_eq!(count(&pool).await, 4, "the one with none is written");
}
