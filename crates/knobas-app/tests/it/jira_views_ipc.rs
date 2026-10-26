//! Jira views (#877) through the IPC bodies they reach: `ticket_views` (a
//! Jira view is a ticket view row with a `saved_view`), `save_ticket_view`
//! (*View settings*), `delete_ticket_view`, and `mini_board` with its
//! `saved_view` argument ([`saved_view_board`]). The views are made the way a
//! sync makes them: a `jira_view` item in the mirror, projected by
//! `knobas_sync::saved_views::project` under the Jira adapter's own
//! declaration.
//!
//! The database is shared by this binary's tests; each uses a project key and
//! a board id nothing else holds.

use knobas_app::IpcErrorCode;
use knobas_app::commands::entity::{saved_view_board, saved_view_board_laned};
use knobas_app::sources::Registry;
use knobas_app::ticket_views::{self, TicketViewDraft, TicketViewRow};
use serde_json::{Value, json};

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
         values ('jira', 'jira', 'Jira', 'http://x', 'pat', '{\"username\": \"mara.lindqvist\"}')
         on conflict (id) do update set kind = excluded.kind, enabled = true,
                                         auth_kind = excluded.auth_kind,
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
    format!("J{letters}")
}

/// One mirrored row: an issue as the adapter stores it, or a Jira view.
async fn item(pool: &sqlx::PgPool, key: &str, kind: &str, title: &str, payload: Value) {
    let id = format!("jira:{key}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, $3)")
        .bind(&id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at)
         values ($1, 'jira', $2, $3, $3, $4, now())",
    )
    .bind(&id)
    .bind(kind)
    .bind(title)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
}

async fn issue(pool: &sqlx::PgPool, key: &str, status: &str, summary: &str) {
    let project = key.split('-').next().unwrap();
    let payload = json!({
        "key": key,
        "fields": {
            "summary": summary,
            "status": { "name": status },
            "issuetype": { "name": "Story" },
            "project": { "key": project, "name": "Payments" },
        },
    });
    item(pool, key, "ticket", summary, payload).await;
}

/// A Kanban board over `p` holding `keys` in that order, with three columns,
/// projected as a sync would. Answers the ticket view's id and the view's
/// entity id.
async fn board(pool: &sqlx::PgPool, p: &str, keys: &[String]) -> (String, String) {
    let key = format!("board-{}", p.to_lowercase());
    item(
        pool,
        &key,
        "jira_view",
        &format!("{p} flow"),
        json!({
            "kind": "board", "id": 22, "type": "kanban", "jql": format!("project = {p}"),
            "query": format!("(project = {p})"), "sprint": null, "keys": keys,
            "columns": [
                { "name": "To Do", "statuses": ["To Do"], "limit": null },
                { "name": "Doing", "statuses": ["In Progress", "In Review"], "limit": 2 },
                { "name": "Done", "statuses": ["Done"], "limit": null },
            ],
        }),
    )
    .await;
    let mut tx = pool.begin().await.unwrap();
    let declared = knobas_sync::saved_views::declared(&knobas_source_jira::descriptor_template())
        .expect("Jira declares saved views");
    knobas_sync::saved_views::project(&mut tx, "jira", &declared)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    (format!("jira-{key}"), format!("jira:{key}"))
}

async fn listed(pool: &sqlx::PgPool, id: &str) -> Option<TicketViewRow> {
    ticket_views::list(pool, &Registry::for_process())
        .await
        .expect("ticket_views")
        .into_iter()
        .find(|row| row.view.id == id)
}

/// Four tickets on a board in Jira's order, plus one from a project the
/// source does not sync.
async fn seeded(pool: &sqlx::PgPool) -> (String, String, String) {
    let p = project();
    issue(pool, &format!("{p}-1"), "In Review", "Settle the ledger").await;
    issue(pool, &format!("{p}-2"), "In Progress", "Retry payouts").await;
    issue(pool, &format!("{p}-3"), "To Do", "Retry the export").await;
    issue(pool, &format!("{p}-4"), "Blocked", "Off the board").await;
    let keys = ["1", "WEB", "2", "3", "4"].map(|n| {
        if n == "WEB" {
            "WEB-5".to_owned()
        } else {
            format!("{p}-{n}")
        }
    });
    let (id, view) = board(pool, &p, &keys).await;
    (p, id, view)
}

#[tokio::test]
async fn a_jira_view_is_listed_under_jira_boards_with_what_its_board_draws() {
    let pool = pool().await;
    let (p, id, view) = seeded(&pool).await;
    let row = listed(&pool, &id).await.expect("the Jira view is listed");
    assert_eq!(row.view.name, format!("{p} flow"));
    assert_eq!(row.view.group, "Jira boards");
    assert_eq!(row.view.saved_view.as_deref(), Some(view.as_str()));
    assert_eq!(row.view.columns.len(), 3);
    assert_eq!(row.view.columns[1].limit, Some(2));
    assert_eq!(
        row.count,
        Some(4),
        "the four it mirrors, not WEB-5 and not every ticket"
    );
    let jira = row.saved_view_source.expect("where it comes from");
    assert_eq!(
        (jira.kind.as_str(), jira.board_type.as_deref()),
        ("board", Some("kanban"))
    );
    assert_eq!(jira.source_query, format!("project = {p}"));
    assert_eq!(jira.sprint, None);

    // A view somebody made says nothing about Jira.
    let own = ticket_views::save(
        &pool,
        TicketViewDraft {
            id: None,
            name: format!("{p} mine"),
            group: "Mine".into(),
            position: None,
            query: format!("project:{p}"),
            layout: "board".into(),
            columns: Vec::new(),
            lanes: "none".into(),
            card_fields: vec!["type".into()],
            list_columns: vec!["key".into()],
            list_sort: None,
            saved_view: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        listed(&pool, &own.id).await.unwrap().saved_view_source,
        None
    );
}

#[tokio::test]
async fn a_jira_views_board_is_jiras_tickets_in_jiras_order_under_its_columns() {
    let pool = pool().await;
    let (p, _, view) = seeded(&pool).await;
    let board = saved_view_board(
        &pool,
        &knobas_app::sources::Registry::for_process(),
        &view,
        None,
        None,
    )
    .await
    .expect("the board");
    let drawn: Vec<(Option<String>, Vec<String>, u32)> = board
        .columns
        .iter()
        .map(|c| {
            (
                c.status.clone(),
                c.cards.iter().map(|card| card.key.clone()).collect(),
                c.total,
            )
        })
        .collect();
    assert_eq!(
        drawn,
        [
            (Some("To Do".into()), vec![format!("{p}-3")], 1),
            (
                Some("Doing".into()),
                vec![format!("{p}-1"), format!("{p}-2")],
                2
            ),
            (Some("Done".into()), vec![], 0),
        ],
        "Jira's order inside a column of two statuses, an empty column kept, a status on no column not drawn"
    );
    // #1423: the column is the view's name; each card carries its own status.
    let statuses: Vec<(String, Option<String>)> = board
        .columns
        .iter()
        .flat_map(|c| {
            c.cards
                .iter()
                .map(|card| (card.key.clone(), card.status.clone()))
        })
        .collect();
    assert_eq!(
        statuses,
        [
            (format!("{p}-3"), Some("To Do".into())),
            (format!("{p}-1"), Some("In Review".into())),
            (format!("{p}-2"), Some("In Progress".into())),
        ],
        "a card under *Doing* reads its own status, never the column's name"
    );

    let capped = saved_view_board(
        &pool,
        &knobas_app::sources::Registry::for_process(),
        &view,
        Some(1),
        None,
    )
    .await
    .unwrap();
    assert_eq!(capped.columns[1].cards.len(), 1);
    assert_eq!(capped.columns[1].cards[0].key, format!("{p}-1"));
    assert_eq!(capped.columns[1].total, 2);

    let narrowed = saved_view_board(
        &pool,
        &knobas_app::sources::Registry::for_process(),
        &view,
        None,
        Some("retry"),
    )
    .await
    .unwrap();
    let keys: Vec<String> = narrowed
        .columns
        .iter()
        .flat_map(|c| c.cards.iter().map(|card| card.key.clone()))
        .collect();
    assert_eq!(keys, [format!("{p}-3"), format!("{p}-2")]);

    let missing = saved_view_board(
        &pool,
        &knobas_app::sources::Registry::for_process(),
        "jira:board-nothing",
        None,
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(missing.code, IpcErrorCode::NotFound);
}

#[tokio::test]
async fn view_settings_keep_what_jira_decides_and_a_jira_view_is_not_deleted() {
    let pool = pool().await;
    let (p, id, view) = seeded(&pool).await;
    let stored = listed(&pool, &id).await.unwrap().view;
    let saved = ticket_views::save(
        &pool,
        TicketViewDraft {
            id: Some(id.clone()),
            name: "Renamed".into(),
            group: "Mine".into(),
            position: None,
            query: "assignee:@me".into(),
            layout: "list".into(),
            columns: Vec::new(),
            lanes: "assignee".into(),
            card_fields: vec!["type".into()],
            list_columns: vec!["key".into(), "summary".into()],
            list_sort: None,
            saved_view: None,
        },
    )
    .await
    .expect("view settings save");
    assert_eq!(
        (saved.layout.as_str(), saved.lanes.as_str()),
        ("list", "assignee")
    );
    assert_eq!(saved.card_fields, ["type"]);
    assert_eq!(saved.name, format!("{p} flow"));
    assert_eq!(saved.group, "Jira boards");
    assert_eq!(saved.query, stored.query);
    assert_eq!(saved.columns, stored.columns);
    assert_eq!(saved.saved_view.as_deref(), Some(view.as_str()));

    let refused = ticket_views::delete(&pool, &id).await.unwrap_err();
    assert_eq!(refused.code, IpcErrorCode::Invalid);
    assert!(listed(&pool, &id).await.is_some());
}

/// A Jira source deleted with its mirror purged takes its views' items, and
/// with them their `sync.saved_view` rows, but no sync of that source runs
/// again to drop the ticket view rows. Those rows must not then be listed as
/// views somebody made, whose empty text holds every ticket.
#[tokio::test]
async fn a_jira_view_whose_mirror_was_purged_is_not_listed() {
    let pool = pool().await;
    let (_, id, view) = seeded(&pool).await;
    assert!(listed(&pool, &id).await.is_some());
    // What `knobas_sync::config::delete`'s purge does, to this view's item
    // alone: the shared database's other tests hold the rest of `jira`.
    sqlx::query("delete from sync.item where entity_id = $1")
        .bind(&view)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(&view)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        listed(&pool, &id).await.is_none(),
        "a purged Jira view is not a view of every ticket"
    );
}

fn own_draft(name: String) -> TicketViewDraft {
    TicketViewDraft {
        id: None,
        name,
        group: "Mine".into(),
        position: None,
        query: String::new(),
        layout: "board".into(),
        columns: Vec::new(),
        lanes: "none".into(),
        card_fields: vec!["type".into()],
        list_columns: vec!["key".into()],
        list_sort: None,
        saved_view: None,
    }
}

/// #878, R-3.25: *Hide from the list* and *Show*. Only a Jira view hides.
#[tokio::test]
async fn a_jira_view_hides_and_shows_again_and_keeps_its_tickets() {
    let pool = pool().await;
    let (p, id, _) = seeded(&pool).await;
    assert!(
        !listed(&pool, &id)
            .await
            .unwrap()
            .saved_view_source
            .unwrap()
            .hidden
    );

    ticket_views::hide(&pool, &id, true).await.expect("hide");
    let row = listed(&pool, &id)
        .await
        .expect("a hidden view is still listed");
    assert!(row.saved_view_source.as_ref().unwrap().hidden);
    assert_eq!(row.count, Some(4), "it keeps syncing");

    ticket_views::hide(&pool, &id, false).await.expect("show");
    assert!(
        !listed(&pool, &id)
            .await
            .unwrap()
            .saved_view_source
            .unwrap()
            .hidden
    );

    let own = ticket_views::save(&pool, own_draft(format!("{p} mine")))
        .await
        .unwrap();
    let refused = ticket_views::hide(&pool, &own.id, true).await.unwrap_err();
    assert_eq!(refused.code, IpcErrorCode::Invalid);
    let missing = ticket_views::hide(&pool, "no-such-view-878", true)
        .await
        .unwrap_err();
    assert_eq!(missing.code, IpcErrorCode::NotFound);
}

/// #878, R-3.20: a new Jira view says *new* until it is opened.
#[tokio::test]
async fn opening_a_new_jira_view_takes_its_new_mark_away() {
    let pool = pool().await;
    let (_, id, _) = seeded(&pool).await;
    sqlx::query("update knobas.ticket_view set is_new = true where id = $1")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        listed(&pool, &id)
            .await
            .unwrap()
            .saved_view_source
            .unwrap()
            .new
    );

    ticket_views::open(&pool, &id).await.expect("open");
    assert!(
        !listed(&pool, &id)
            .await
            .unwrap()
            .saved_view_source
            .unwrap()
            .new
    );
    ticket_views::open(&pool, &id)
        .await
        .expect("opening it again is no change");
    let missing = ticket_views::open(&pool, "no-such-view-878")
        .await
        .unwrap_err();
    assert_eq!(missing.code, IpcErrorCode::NotFound);
}

/// #879, R-3.23: the tickets Jira put on a view that knobas does not have,
/// counted per project the source does not sync, for the Tickets warn line
/// and *Boards and filters* (#878, R-13a.2). A missing key of a project it
/// does sync is not one: that is a sync still to come.
#[tokio::test]
async fn a_jira_view_counts_its_tickets_in_projects_the_source_does_not_sync() {
    let pool = knobas_db::test_util::scratch_database("jira_views_unsynced")
        .await
        .pool(2)
        .await
        .expect("a pool onto the scratch database");
    let p = project();
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
         values ('jira', 'jira', 'Jira', 'http://x', 'Pat', $1)",
    )
    .bind(json!({ "projects": [p.clone(), "OPS"] }))
    .execute(&pool)
    .await
    .unwrap();
    issue(&pool, &format!("{p}-1"), "To Do", "Settle the ledger").await;
    let keys = [
        format!("{p}-1"),
        "WEB-5".into(),
        format!("{p}-9"),
        "WEB-6".into(),
        "CUS-1".into(),
    ];
    let (id, _) = board(&pool, &p, &keys).await;
    let jira = listed(&pool, &id).await.unwrap().saved_view_source.unwrap();
    let unsynced: Vec<(&str, u64)> = jira
        .unsynced
        .iter()
        .map(|u| (u.project.as_str(), u.count))
        .collect();
    assert_eq!(unsynced, [("WEB", 2), ("CUS", 1)], "most first");

    // A source that names no projects syncs everything the credential sees,
    // and a JQL source has no projects to add to: neither has an unsynced
    // project, and *Sync WEB too* there would narrow the source or be refused.
    for config in [
        json!({}),
        json!({ "projects": [] }),
        json!({ "jql_filter": "labels = on-call" }),
    ] {
        sqlx::query("update knobas.source_config set config = $1 where id = 'jira'")
            .bind(&config)
            .execute(&pool)
            .await
            .unwrap();
        let jira = listed(&pool, &id).await.unwrap().saved_view_source.unwrap();
        assert_eq!(jira.unsynced, [], "{config} leaves no project out");
    }
}

/// #1208: *Make an editable copy* asks the saved view's own source to read its
/// query, with the payload the source emitted; the Jira adapter answers JQL.
#[tokio::test]
async fn make_an_editable_copy_asks_the_saved_views_source_to_translate_its_query() {
    let pool = pool().await;
    let p = project();
    let (id, _) = board(&pool, &p, &[]).await;
    let secrets: std::sync::Arc<dyn knobas_secrets::SecretStore> =
        std::sync::Arc::new(knobas_secrets::MemoryStore::new());
    secrets
        .put(
            &knobas_secrets::KeychainAccount::source("jira"),
            &knobas_secrets::Secret::just(knobas_source::AuthMethod::Pat, "a-token"),
        )
        .unwrap();
    let registry = knobas_app::sources::Registry::for_process();

    let copy = ticket_views::translate(
        &pool,
        &secrets,
        &crate::support::shared(registry.clone()),
        &id,
    )
    .await
    .expect("the read answers")
    .expect("Jira translates its views' JQL");
    assert_eq!(
        serde_json::to_value(copy).unwrap(),
        json!({
            "conditions": [{ "key": "project", "negated": false, "values": [p] }],
            "kept": 1,
            "left_out": [],
        })
    );

    // A view of a source whose adapter answers no translation: no copy.
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
         values ('views-mock', 'mock', 'Mock', '', 'none', '{}')
         on conflict (id) do nothing",
    )
    .execute(&pool)
    .await
    .unwrap();
    let mock_ref = format!("views-mock:{}", p.to_lowercase());
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'jira_view', 'M')")
        .bind(&mock_ref)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at)
         values ($1, 'views-mock', 'jira_view', 'M', 'M', '{\"jql\": \"project = PAY\"}', now())",
    )
    .bind(&mock_ref)
    .execute(&pool)
    .await
    .unwrap();
    let mut draft = own_draft(format!("Mock {p}"));
    draft.saved_view = Some(mock_ref);
    let mock_view = ticket_views::save(&pool, draft).await.unwrap();
    let none = ticket_views::translate(
        &pool,
        &secrets,
        &crate::support::shared(registry.clone()),
        &mock_view.id,
    )
    .await
    .expect("the read answers");
    assert_eq!(none, None);

    // A view of your own has no source query; no view is no view.
    let own = ticket_views::save(&pool, own_draft(format!("Own {p}")))
        .await
        .unwrap();
    let refused = ticket_views::translate(
        &pool,
        &secrets,
        &crate::support::shared(registry.clone()),
        &own.id,
    )
    .await
    .unwrap_err();
    assert_eq!(refused.code, IpcErrorCode::Invalid, "{refused:?}");
    let missing = ticket_views::translate(
        &pool,
        &secrets,
        &crate::support::shared(registry.clone()),
        "no-such-view-1208",
    )
    .await
    .unwrap_err();
    assert_eq!(missing.code, IpcErrorCode::NotFound, "{missing:?}");
}

/// #2032: a saved view names its ticket system as its group does, from the
/// kind's declared `system_name`, never the source's display name, so the
/// shell's words about it read *Jira* whatever a person called the source.
#[tokio::test]
async fn a_saved_view_names_its_ticket_system_not_its_source() {
    let pool = pool().await;
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
         values ('jira-eu', 'jira', 'Tidewater Jira EU', 'http://x', 'pat', '{}')
         on conflict (id) do update set kind = excluded.kind, enabled = true,
                                         display_name = excluded.display_name",
    )
    .execute(&pool)
    .await
    .unwrap();
    let p = project();
    let entity = format!("jira-eu:board-{}", p.to_lowercase());
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'jira_view', $2)")
        .bind(&entity)
        .bind(format!("{p} flow"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at)
         values ($1, 'jira-eu', 'jira_view', $2, $2, $3, now())",
    )
    .bind(&entity)
    .bind(format!("{p} flow"))
    .bind(json!({
        "kind": "board", "id": 23, "type": "kanban", "jql": format!("project = {p}"),
        "query": format!("(project = {p})"), "sprint": null, "keys": [], "columns": [],
    }))
    .execute(&pool)
    .await
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    let declared = knobas_sync::saved_views::declared(&knobas_source_jira::descriptor_template())
        .expect("Jira declares saved views");
    knobas_sync::saved_views::project(&mut tx, "jira-eu", &declared)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let row = listed(&pool, &format!("jira-eu-board-{}", p.to_lowercase()))
        .await
        .expect("the view is listed");
    assert_eq!(row.view.group, "Jira boards");
    assert_eq!(
        row.saved_view_source.expect("where it comes from").system,
        "Jira",
        "the ticket system, not the source's name"
    );
}

/// What one statement sent back, off sqlx's `sqlx::query` event: its text and
/// its row count. `command_statements.rs` counts the same events.
#[derive(Default)]
struct Sent {
    statement: String,
    rows: u64,
}

impl tracing::field::Visit for Sent {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "db.statement" {
            value.clone_into(&mut self.statement);
        }
    }
    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        if field.name() == "rows_returned" {
            self.rows = value;
        }
    }
    fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}
}

struct Record(std::sync::Arc<std::sync::Mutex<Vec<Sent>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Record {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        if event.metadata().target() == "sqlx::query" {
            let mut sent = Sent::default();
            event.record(&mut sent);
            self.0.lock().unwrap().push(sent);
        }
    }
}

/// The most rows the board's card statement (the one that reads each card's
/// parent title) sent back while `view`'s board was read with `per_column`
/// and `lanes`. Read on this thread's subscriber; a first registration that
/// straddles another thread's is looked at again, as `command_statements.rs`
/// does.
async fn cards_sent(
    pool: &sqlx::PgPool,
    view: &str,
    per_column: Option<u32>,
    lanes: Option<knobas_core::mini_board::Lanes>,
) -> u64 {
    use tracing_subscriber::layer::SubscriberExt;
    for _ in 0..3 {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let guard = tracing::subscriber::set_default(
            tracing_subscriber::registry().with(Record(seen.clone())),
        );
        saved_view_board_laned(
            pool,
            &Registry::for_process(),
            view,
            per_column,
            None,
            lanes,
        )
        .await
        .expect("the board");
        drop(guard);
        let seen = seen.lock().unwrap();
        if let Some(rows) = seen
            .iter()
            .filter(|sent| sent.statement.contains("parent_title"))
            .map(|sent| sent.rows)
            .max()
        {
            return rows;
        }
    }
    panic!("the card statement was never seen");
}

/// A Jira view's board as drawn: per column its name, its cards' keys and
/// its total, and the statuses its sources show.
type Drawn = (
    Vec<(Option<String>, Vec<String>, u32)>,
    Vec<(String, Vec<String>)>,
);

/// [`Drawn`] of `board`.
fn drawn(board: &knobas_core::mini_board::MiniBoard) -> Drawn {
    (
        board
            .columns
            .iter()
            .map(|c| {
                (
                    c.status.clone(),
                    c.cards.iter().map(|card| card.key.clone()).collect(),
                    c.total,
                )
            })
            .collect(),
        board
            .sources
            .iter()
            .map(|s| (s.source_id.clone(), s.statuses.clone()))
            .collect(),
    )
}

/// A Jira view's board reads only the cards it draws (Björn, 2026-10-26):
/// it read every ticket on the view through the whole card statement and
/// cut each column after, 0.6 s for a board of 2,250 tickets and 1.1 s for a
/// filter of 4,500, where eight a column are drawn. The capped answer is the
/// uncapped one cut, for a board with columns and a filter without.
#[tokio::test]
async fn a_jira_views_board_reads_only_the_cards_it_draws() {
    let pool = pool().await;
    let p = project();
    let statuses = ["To Do", "In Progress", "In Review", "Done", "Blocked"];
    let mut keys = Vec::new();
    for n in 1..=300 {
        let key = format!("{p}-{n}");
        issue(
            &pool,
            &key,
            statuses[n % statuses.len()],
            "Settle the ledger",
        )
        .await;
        keys.push(key);
    }
    // A ticket on the view that is gone from the mirror is never counted.
    let gone = format!("{p}-301");
    issue(&pool, &gone, "To Do", "Gone").await;
    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(format!("jira:{gone}"))
        .execute(&pool)
        .await
        .unwrap();
    keys.push(gone);
    keys.reverse();
    let (_, with_columns) = board(&pool, &p, &keys).await;
    let filter = format!("filter-{}", p.to_lowercase());
    item(
        &pool,
        &filter,
        "jira_view",
        &format!("{p} filter"),
        json!({
            "kind": "filter", "id": p, "type": null, "name": format!("{p} filter"), "filter_id": p,
            "jql": format!("project = {p}"), "sub_query": null, "query": format!("project = {p}"),
            "columns": [], "sprint": null, "keys": keys,
        }),
    )
    .await;
    let mut tx = pool.begin().await.unwrap();
    let declared = knobas_sync::saved_views::declared(&knobas_source_jira::descriptor_template())
        .expect("Jira declares saved views");
    knobas_sync::saved_views::project(&mut tx, "jira", &declared)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let without_columns = format!("jira:{filter}");

    let filter_columns = ["To Do", "In Progress", "In Review", "Done", "Blocked"];
    for (view, columns) in [
        (
            with_columns,
            vec![("To Do", 60), ("Doing", 120), ("Done", 60)],
        ),
        (
            without_columns,
            filter_columns.map(|status| (status, 60)).to_vec(),
        ),
    ] {
        let registry = Registry::for_process();
        let whole = saved_view_board(&pool, &registry, &view, None, None)
            .await
            .unwrap();
        let capped = saved_view_board(&pool, &registry, &view, Some(8), None)
            .await
            .unwrap();
        let (mut cut, sources) = drawn(&whole);
        for column in &mut cut {
            column.1.truncate(8);
        }
        let named: Vec<(Option<String>, u32)> = cut.iter().map(|c| (c.0.clone(), c.2)).collect();
        let expected: Vec<(Option<String>, u32)> = columns
            .iter()
            .map(|(name, total)| (Some((*name).to_owned()), *total))
            .collect();
        assert_eq!(
            named, expected,
            "{view}: the columns in the board's order, each counted whole"
        );
        let columns = columns.len();
        assert_eq!(
            drawn(&capped),
            (cut, sources),
            "{view}: the capped board is the whole one cut"
        );

        // A cut that keeps no card still answers for the view's source.
        let none = saved_view_board(&pool, &registry, &view, Some(0), None)
            .await
            .unwrap();
        assert_eq!(
            drawn(&none).1,
            drawn(&whole).1,
            "{view}: the sources' statuses"
        );

        // The count is read off a named field of sqlx's event; were that
        // field gone it would read 0 and the cap below pass on any code.
        let uncapped = cards_sent(&pool, &view, None, None).await;
        assert!(
            uncapped > 8 * columns as u64,
            "{view}: the uncapped card statement sent {uncapped} rows, so the count is read"
        );
        let sent = cards_sent(&pool, &view, Some(8), None).await;
        assert!(
            sent <= 8 * columns as u64,
            "{view}: the card statement sent {sent} rows for {columns} columns of eight"
        );
    }
}

/// One ticket with the fields a lane reads: an assignee, labels and an epic,
/// each missing on some tickets (`n` picks which).
async fn laned_issue(pool: &sqlx::PgPool, p: &str, n: usize, status: &str) -> String {
    let key = format!("{p}-{n}");
    let mut fields = json!({
        "summary": "Settle the ledger",
        "status": { "name": status },
        "issuetype": { "name": (["Story", "Bug", "Task"][n % 3]) },
        "priority": { "name": (["High", "Low", "Medium", "Lowest"][n % 4]) },
        "project": { "key": p, "name": "Payments" },
    });
    if n % 7 != 0 {
        fields["assignee"] = json!({ "name": (["mara", "jon", "li", "ana"][n % 4]) });
    }
    if n % 5 != 0 {
        fields["labels"] = json!([(["payments", "ops", " ", "web"][n % 4]), "second"]);
    }
    if n % 3 != 0 {
        fields["parent"] = json!({ "key": format!("{p}-EPIC{}", n % 4) });
    }
    item(pool, &key, "ticket", "Settle the ledger", json!({ "key": key, "fields": fields })).await;
    key
}

/// A Jira view's board in lanes reads only the cards its lanes keep (#2183),
/// with the answer it gave when it read the whole view as cards and laned it
/// after: [`knobas_core::mini_board::in_lanes`] over the whole board.
#[tokio::test]
async fn a_laned_jira_views_board_reads_only_the_cards_it_draws() {
    use knobas_core::mini_board::{LaneField, Lanes, in_lanes};
    let pool = pool().await;
    let p = project();
    let statuses = ["To Do", "In Progress", "In Review", "Done", "Blocked"];
    let mut keys = Vec::new();
    for n in 1..=200 {
        keys.push(laned_issue(&pool, &p, n, statuses[n % statuses.len()]).await);
    }
    keys.reverse();
    let (_, view) = board(&pool, &p, &keys).await;
    let registry = Registry::for_process();
    let whole = saved_view_board(&pool, &registry, &view, None, None)
        .await
        .unwrap();
    for by in [
        LaneField::Epic,
        LaneField::Assignee,
        LaneField::Priority,
        LaneField::IssueType,
        LaneField::Label,
    ] {
        let lanes = Lanes {
            by,
            first: Some(2),
        };
        let expected = in_lanes(whole.clone(), lanes, Some(3));
        let laned = saved_view_board_laned(&pool, &registry, &view, Some(3), None, Some(lanes))
            .await
            .unwrap();
        assert_eq!(drawn(&laned), drawn(&expected), "{by:?}: the columns");
        let lanes_of = |board: &knobas_core::mini_board::MiniBoard| {
            board
                .lanes
                .iter()
                .map(|lane| {
                    (
                        lane.key.clone(),
                        lane.count,
                        lane.columns
                            .iter()
                            .map(|c| (c.status.clone(), c.ids.clone(), c.total))
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(lanes_of(&laned), lanes_of(&expected), "{by:?}: the lanes");
        assert_eq!(laned.more_lanes, expected.more_lanes, "{by:?}: the lanes past");
        assert!(expected.more_lanes > 0, "{by:?}: some lanes are left out");

        let kept: usize = expected
            .lanes
            .iter()
            .flat_map(|lane| &lane.columns)
            .map(|c| c.ids.len())
            .sum();
        let sent = cards_sent(&pool, &view, Some(3), Some(lanes)).await;
        assert!(
            sent <= kept as u64,
            "{by:?}: the card statement sent {sent} rows for {kept} cards drawn"
        );
    }
}
