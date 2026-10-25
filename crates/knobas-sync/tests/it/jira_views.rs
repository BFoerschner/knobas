//! Jira views (#877) through `knobas_sync::run_once`: a source that emits
//! `jira_view` items the way the Jira adapter does, then `sync.saved_view`,
//! `sync.saved_view_member` and `knobas.ticket_view` read back.
//!
//! Each test has a scratch database of its own, so source ids repeat freely.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use knobas_source::{
    Capability, ConnectionInfo, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError,
    SyncItem, WriteOp,
};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};

/// What the fake emits on its next run: `(key, name, payload, deleted)`.
type Corpus = Arc<Mutex<Vec<(String, String, Value, bool)>>>;

/// The source the fake stands for: Jira by default, or a ticket system
/// knobas has no adapter for (#1207). `kind` is the item kind its views
/// arrive as, which declares the saved-view frame when `saved_view` says so.
#[derive(Clone, Copy)]
struct Adapter {
    id: &'static str,
    name: &'static str,
    kind: &'static str,
    saved_view: bool,
    /// The ticket system its views are listed under, as the Jira adapter
    /// declares `Jira`; `None` lists them under the source's `name`.
    system: Option<&'static str>,
    /// Where its views' query, board type, sub-query and sprint are (#2050):
    /// what the saved-view frame reads, and all it reads them at.
    paths: fn() -> knobas_source::KindPaths,
}

/// What the Jira adapter declares of its views since #2050: the saved
/// filter's `jql`, the board's `type`, `sub_query` and `sprint`.
fn jira_paths() -> knobas_source::KindPaths {
    let at = |key: &str| vec![knobas_core::payload::PayloadPath::of([key])];
    knobas_source::KindPaths {
        source_query: at("jql"),
        board_type: at("type"),
        sub_query: at("sub_query"),
        sprint: at("sprint"),
        ..knobas_source::KindPaths::default()
    }
}

const JIRA: Adapter = Adapter {
    id: "jira",
    name: "Jira",
    kind: "jira_view",
    saved_view: true,
    system: Some("Jira"),
    paths: jira_paths,
};

struct Boards {
    corpus: Corpus,
    adapter: Adapter,
}

fn kind(id: &str) -> KindInfo {
    KindInfo {
        id: id.to_owned(),
        label: id.to_owned(),
        plural: id.to_owned(),
        monogram: "JV".into(),
        full_sync_exhaustive: true,
        refreshes: true,
    }
}

#[async_trait]
impl Source for Boards {
    async fn view(
        &self,
        _entity: &str,
        _section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, SourceError> {
        Ok(None)
    }

    fn descriptor(&self) -> SourceDescriptor {
        let Adapter {
            id,
            name,
            kind: views,
            saved_view,
            system,
            paths,
        } = self.adapter;
        SourceDescriptor {
            id: id.into(),
            adapter_kind: id.into(),
            name: name.into(),
            capabilities: Vec::<Capability>::new(),
            adapter_version: "0.1.0".into(),
            auth_methods: Vec::new(),
            accepts_account: false,
            min_sync_interval_secs: 60,
            roles: Vec::new(),
            source_actions: Vec::new(),
            clone_from: Vec::new(),
            url_forms: Vec::new(),
            write_ops: Vec::new(),
            entity_kinds: vec![kind("ticket"), kind(views)],
            config_schema: json!({ "type": "object", "properties": {} }),
            // What the Jira adapter declares since #1207: its views are the
            // ticket system's saved views.
            payload_paths: vec![knobas_source::KindPaths {
                kind: views.into(),
                frame: saved_view.then_some(knobas_core::payload::Frame::SavedView),
                system_name: system.map(str::to_owned),
                ..paths()
            }],
        }
    }

    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        Ok(ConnectionInfo::default())
    }

    async fn sync(
        &self,
        _cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        let corpus = self.corpus.lock().unwrap().clone();
        for (key, name, payload, deleted) in corpus {
            sink.item(SyncItem {
                entity: knobas_core::entity::EntityRef::new(self.adapter.id, &key),
                kind: self.adapter.kind.to_owned(),
                title: name,
                body_text: String::new(),
                author: None,
                updated_at: None,
                payload,
                web_url: None,
                deleted,
            })
            .await?;
        }
        Ok("c".to_owned())
    }

    async fn write(&self, _op: WriteOp) -> Result<knobas_source::WriteReceipt, SourceError> {
        Err(SourceError::protocol("read-only"))
    }

    async fn reachable_transitions(&self, _entity: &str) -> Result<Vec<String>, SourceError> {
        Err(SourceError::protocol("no workflow"))
    }
}

fn board(id: u64, keys: &[&str], columns: Value) -> Value {
    json!({
        "kind": "board", "id": id, "type": "kanban", "jql": "project = PAY ORDER BY Rank ASC",
        "sub_query": "fixVersion is EMPTY", "query": "(project = PAY) AND (fixVersion is EMPTY) ORDER BY Rank ASC",
        "columns": columns, "sprint": null, "keys": keys,
    })
}

fn three_columns() -> Value {
    json!([
        { "name": "To Do", "statuses": ["To Do"], "limit": null },
        { "name": "Doing", "statuses": ["In Progress", "In Review"], "limit": 3 },
        { "name": "Done", "statuses": ["Done"], "limit": null },
    ])
}

async fn pool(label: &str) -> PgPool {
    pool_of(label, 2).await
}

async fn pool_of(label: &str, connections: u32) -> PgPool {
    let pool = knobas_db::test_util::scratch_database(label)
        .await
        .pool(connections)
        .await
        .expect("a pool on the scratch database");
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
         values ('jira', 'jira', 'Jira', 'https://jira.example', 'Pat', '{}')",
    )
    .execute(&pool)
    .await
    .expect("the source row");
    pool
}

fn source(views: Vec<(String, String, Value, bool)>) -> (Boards, Corpus) {
    source_of(JIRA, views)
}

fn source_of(adapter: Adapter, views: Vec<(String, String, Value, bool)>) -> (Boards, Corpus) {
    let corpus: Corpus = Arc::new(Mutex::new(views));
    (
        Boards {
            corpus: Arc::clone(&corpus),
            adapter,
        },
        corpus,
    )
}

async fn members(pool: &PgPool, view: &str) -> Vec<(String, i32)> {
    sqlx::query(
        "select entity_id, rank from sync.saved_view_member where view_id = $1 order by rank",
    )
    .bind(view)
    .fetch_all(pool)
    .await
    .unwrap()
    .into_iter()
    .map(|row| (row.get("entity_id"), row.get("rank")))
    .collect()
}

async fn ticket_view(
    pool: &PgPool,
    id: &str,
) -> Option<(String, String, Value, String, Option<String>)> {
    sqlx::query(
        "select name, view_group, columns, lanes, saved_view from knobas.ticket_view where id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .unwrap()
    .map(|row| {
        (
            row.get("name"),
            row.get("view_group"),
            row.get("columns"),
            row.get("lanes"),
            row.get("saved_view"),
        )
    })
}

#[tokio::test]
async fn a_jira_view_is_kept_with_its_tickets_in_jiras_order_and_shows_as_a_ticket_view() {
    let pool = pool("jira_views_projected").await;
    let (src, _) = source(vec![(
        "board-22".into(),
        "PAY flow".into(),
        board(22, &["WEB-5", "PAY-240", "PAY-231"], three_columns()),
        false,
    )]);
    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    let row = sqlx::query("select ref_kind, ref_id, board_type, name, sub_query, columns from sync.saved_view where entity_id = 'jira:board-22'")
        .fetch_one(&pool)
        .await
        .expect("the view is kept");
    assert_eq!(row.get::<String, _>("ref_kind"), "board");
    assert_eq!(row.get::<String, _>("ref_id"), "22");
    assert_eq!(
        row.get::<Option<String>, _>("board_type").as_deref(),
        Some("kanban")
    );
    assert_eq!(
        row.get::<Option<String>, _>("sub_query").as_deref(),
        Some("fixVersion is EMPTY")
    );
    assert_eq!(row.get::<Value, _>("columns"), three_columns());
    assert_eq!(
        members(&pool, "jira:board-22").await,
        [
            ("jira:WEB-5".to_owned(), 1),
            ("jira:PAY-240".to_owned(), 2),
            ("jira:PAY-231".to_owned(), 3)
        ],
        "Jira's order, and a key from a project the source does not sync is kept"
    );
    assert_eq!(
        ticket_view(&pool, "jira-board-22").await,
        Some((
            "PAY flow".into(),
            "Jira boards".into(),
            three_columns(),
            "none".into(),
            Some("jira:board-22".into())
        ))
    );
}

#[tokio::test]
async fn name_and_columns_follow_jira_and_the_readers_settings_stay() {
    let pool = pool("jira_views_follow").await;
    let (src, corpus) = source(vec![(
        "board-22".into(),
        "PAY flow".into(),
        board(22, &["PAY-1"], three_columns()),
        false,
    )]);
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    sqlx::query("update knobas.ticket_view set lanes = 'assignee', layout = 'list' where id = 'jira-board-22'")
        .execute(&pool)
        .await
        .unwrap();

    let two = json!([{ "name": "Open", "statuses": ["To Do"], "limit": 5 }, { "name": "Closed", "statuses": ["Done"], "limit": null }]);
    *corpus.lock().unwrap() = vec![(
        "board-22".into(),
        "PAY flow v2".into(),
        board(22, &["PAY-2", "PAY-1"], two.clone()),
        false,
    )];
    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    let (name, _, columns, lanes, _) = ticket_view(&pool, "jira-board-22").await.unwrap();
    assert_eq!(
        (name.as_str(), columns, lanes.as_str()),
        ("PAY flow v2", two, "assignee")
    );
    assert_eq!(
        members(&pool, "jira:board-22").await,
        [("jira:PAY-2".to_owned(), 1), ("jira:PAY-1".to_owned(), 2)]
    );
}

/// ADR-0007's failure direction: a payload that carries none of the paths
/// read is a board with no columns, no sprint and no tickets -- a miss, never
/// a guess.
#[tokio::test]
async fn a_view_whose_payload_says_nothing_holds_nothing() {
    let pool = pool("jira_views_miss").await;
    let (src, _) = source(vec![(
        "board-9".into(),
        "Odd board".into(),
        json!({ "columns": "three", "keys": { "PAY-1": 1 }, "sprint": "Sprint 1" }),
        false,
    )]);
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    let row = sqlx::query(
        "select ref_kind, columns, sprint from sync.saved_view where entity_id = 'jira:board-9'",
    )
    .fetch_one(&pool)
    .await
    .expect("the view is kept");
    assert_eq!(row.get::<String, _>("ref_kind"), "board");
    assert_eq!(row.get::<Value, _>("columns"), json!([]));
    assert_eq!(row.get::<Option<Value>, _>("sprint"), None);
    assert!(members(&pool, "jira:board-9").await.is_empty());
}

#[tokio::test]
async fn a_board_deleted_in_jira_leaves_with_its_tickets_and_its_ticket_view() {
    let pool = pool("jira_views_gone").await;
    let (src, corpus) = source(vec![
        (
            "board-21".into(),
            "PAY sprint board".into(),
            board(21, &["PAY-231"], three_columns()),
            false,
        ),
        (
            "board-22".into(),
            "PAY flow".into(),
            board(22, &["PAY-240"], three_columns()),
            false,
        ),
    ]);
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    // An incremental run, so no sweep: the adapter's own word is what removes it.
    *corpus.lock().unwrap() = vec![("board-21".into(), "board-21".into(), json!({}), true)];
    knobas_sync::run_once(&pool, &src, Some("c".to_owned()))
        .await
        .unwrap();

    let views: Vec<String> =
        sqlx::query_scalar("select entity_id from sync.saved_view order by entity_id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(views, ["jira:board-22"]);
    assert!(members(&pool, "jira:board-21").await.is_empty());
    assert_eq!(ticket_view(&pool, "jira-board-21").await, None);
    assert!(
        ticket_view(&pool, "jira-board-22").await.is_some(),
        "the other board stays"
    );
}

fn filter(id: &str, keys: &[&str]) -> Value {
    json!({
        "kind": "filter", "id": id, "type": null,
        "jql": "project = PAY AND fixVersion is EMPTY ORDER BY updated DESC",
        "sub_query": null, "query": "project = PAY AND fixVersion is EMPTY ORDER BY updated DESC",
        "columns": [], "sprint": null, "keys": keys,
    })
}

/// `(layout, hidden, is_new)` of a ticket view row.
async fn flags(pool: &PgPool, id: &str) -> (String, bool, bool) {
    let row = sqlx::query("select layout, hidden, is_new from knobas.ticket_view where id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("the ticket view row");
    (row.get("layout"), row.get("hidden"), row.get("is_new"))
}

/// #878: a favourite filter is a Jira view under *Jira filters*, drawn as a
/// list (Jira gives a filter no columns), its tickets in Jira's order.
#[tokio::test]
async fn a_favourite_filter_shows_as_a_list_under_jira_filters() {
    let pool = pool("jira_views_filter").await;
    let (src, _) = source(vec![(
        "filter-10300".into(),
        "PAY without a fix version".into(),
        filter("10300", &["PAY-240", "PAY-231"]),
        false,
    )]);
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    let ref_kind: String = sqlx::query_scalar(
        "select ref_kind from sync.saved_view where entity_id = 'jira:filter-10300'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ref_kind, "filter");
    let (name, group, columns, _, saved_view) = ticket_view(&pool, "jira-filter-10300")
        .await
        .expect("a ticket view for the filter");
    assert_eq!(
        (
            name.as_str(),
            group.as_str(),
            columns,
            saved_view.as_deref()
        ),
        (
            "PAY without a fix version",
            "Jira filters",
            json!([]),
            Some("jira:filter-10300")
        )
    );
    assert_eq!(flags(&pool, "jira-filter-10300").await.0, "list");
    assert_eq!(
        members(&pool, "jira:filter-10300").await,
        [
            ("jira:PAY-240".to_owned(), 1),
            ("jira:PAY-231".to_owned(), 2)
        ]
    );
}

/// #878, R-3.20 and R-3.25: the views a source's first sync brings are not
/// *new*; one that turns up at a later sync is. Hiding is the reader's and
/// no sync undoes it.
#[tokio::test]
async fn a_view_that_turns_up_after_the_first_sync_is_new_and_a_hidden_one_stays_hidden() {
    let pool = pool("jira_views_new").await;
    let (src, corpus) = source(vec![(
        "board-21".into(),
        "PAY sprint board".into(),
        board(21, &["PAY-231"], three_columns()),
        false,
    )]);
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(
        flags(&pool, "jira-board-21").await,
        ("board".into(), false, false)
    );
    sqlx::query("update knobas.ticket_view set hidden = true where id = 'jira-board-21'")
        .execute(&pool)
        .await
        .unwrap();

    corpus.lock().unwrap().push((
        "board-22".into(),
        "PAY flow".into(),
        board(22, &["PAY-240"], three_columns()),
        false,
    ));
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(
        flags(&pool, "jira-board-22").await,
        ("board".into(), false, true),
        "a board new in Jira is marked new"
    );
    assert_eq!(
        flags(&pool, "jira-board-21").await,
        ("board".into(), true, false),
        "hidden stays hidden, and an old board does not turn new"
    );
}

/// #1088: a board's item deleted while the projection reads it -- a purge
/// (`config::delete`'s `PURGE_ITEMS`) committing in the middle of a run's
/// `project` -- is a board that is gone, not a failed sync. The delete is held
/// open until the projection is seen waiting on it, so the projection has read
/// the board as live and meets the delete only at its row; then the delete
/// commits.
#[tokio::test]
async fn a_board_whose_item_is_deleted_mid_projection_is_skipped_not_a_failed_sync() {
    let pool = pool_of("jira_views_purged_mid_projection", 3).await;
    let (src, _) = source(vec![
        (
            "board-21".into(),
            "PAY sprint board".into(),
            board(21, &["PAY-231"], three_columns()),
            false,
        ),
        (
            "board-22".into(),
            "PAY flow".into(),
            board(22, &["PAY-240"], three_columns()),
            false,
        ),
    ]);
    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    let mut purge = pool.begin().await.unwrap();
    sqlx::query("delete from sync.item where entity_id = 'jira:board-21'")
        .execute(&mut *purge)
        .await
        .unwrap();

    let mut run = pool.begin().await.unwrap();
    let pid: i32 = sqlx::query_scalar("select pg_backend_pid()")
        .fetch_one(&mut *run)
        .await
        .unwrap();
    let declared = knobas_sync::saved_views::declared(&src.descriptor()).expect("saved views");
    let projection = tokio::spawn(async move {
        let projected = knobas_sync::saved_views::project(&mut run, "jira", &declared).await;
        (projected, run)
    });

    let mut waited = false;
    for _ in 0..200 {
        let wait: Option<String> =
            sqlx::query_scalar("select wait_event_type from pg_stat_activity where pid = $1")
                .bind(pid)
                .fetch_one(&pool)
                .await
                .unwrap();
        if wait.as_deref() == Some("Lock") {
            waited = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert!(
        waited,
        "the projection never met the open delete, so this test witnessed nothing"
    );
    purge.commit().await.unwrap();

    let (projected, run) = projection.await.unwrap();
    projected.expect("a board whose item went mid-projection is skipped, not a failed sync");
    run.commit().await.unwrap();

    let views: Vec<String> =
        sqlx::query_scalar("select entity_id from sync.saved_view order by entity_id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(views, ["jira:board-22"]);
    assert_eq!(ticket_view(&pool, "jira-board-21").await, None);
    assert_eq!(
        members(&pool, "jira:board-22").await,
        [("jira:PAY-240".to_owned(), 1)]
    );
}

/// #1207: the projection is the ticket system's saved views, keyed on the
/// frame a kind declares and not on Jira. A source knobas has no adapter for
/// whose views arrive as `project_view` gets a ticket view from each, grouped
/// under its own name, with its columns and its tickets in its order.
#[tokio::test]
async fn a_saved_view_of_a_ticket_system_that_is_not_jira_shows_as_a_ticket_view() {
    let pool = pool("saved_views_not_jira").await;
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
         values ('forge', 'forge', 'Forge', 'https://forge.example', 'Pat', '{}')",
    )
    .execute(&pool)
    .await
    .expect("the source row");
    let forge = FORGE;
    let (src, _) = source_of(
        forge,
        vec![
            (
                "view-7".into(),
                "Release train".into(),
                json!({ "kind": "project", "columns": three_columns(), "keys": ["REL-2", "REL-1"] }),
                false,
            ),
            (
                "query-3".into(),
                "Mine".into(),
                json!({ "kind": "filter", "keys": ["REL-1"] }),
                false,
            ),
        ],
    );
    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert_eq!(
        ticket_view(&pool, "forge-view-7").await,
        Some((
            "Release train".into(),
            "Forge boards".into(),
            three_columns(),
            "none".into(),
            Some("forge:view-7".into())
        )),
        "a kind the projection has never heard of is a board, as a missing one is"
    );
    assert_eq!(
        members(&pool, "forge:view-7").await,
        [("forge:REL-2".to_owned(), 1), ("forge:REL-1".to_owned(), 2)]
    );
    assert_eq!(
        flags(&pool, "forge-query-3").await,
        ("list".to_owned(), false, false)
    );
    let group: String =
        sqlx::query_scalar("select view_group from knobas.ticket_view where id = 'forge-query-3'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(group, "Forge filters");
}

/// A ticket system knobas has no adapter for, whose views keep their query
/// at `search.text`, their layout at `layout` and their current iteration at
/// `iteration` (#2050).
fn forge_paths() -> knobas_source::KindPaths {
    let at = |path: &[&str]| vec![knobas_core::payload::PayloadPath::of(path.iter().copied())];
    knobas_source::KindPaths {
        source_query: at(&["search", "text"]),
        board_type: at(&["layout"]),
        sprint: at(&["iteration"]),
        ..knobas_source::KindPaths::default()
    }
}

const FORGE: Adapter = Adapter {
    id: "forge",
    name: "Forge",
    kind: "project_view",
    saved_view: true,
    system: None,
    paths: forge_paths,
};

/// #2050: the projection reads a view's query, board type, sub-query and
/// sprint at the paths its kind declares, and at no key named after Jira. A
/// view of a second ticket system keeps its own query; Jira's spellings beside
/// it are not read; a view whose record carries no query keeps none.
#[tokio::test]
async fn a_saved_view_keeps_the_query_its_kind_declares_and_no_key_named_after_jira() {
    let pool = pool("saved_views_declared_query").await;
    let (src, _) = source_of(
        FORGE,
        vec![
            (
                "view-7".into(),
                "Release train".into(),
                json!({
                    "kind": "project", "keys": ["REL-2"],
                    "search": { "text": "is:open milestone:train" },
                    "layout": "kanban",
                    "iteration": { "id": 4, "name": "Train 12", "end_date": "2026-11-02" },
                    "jql": "project = REL", "type": "scrum", "sub_query": "fixVersion is EMPTY",
                    "sprint": { "id": 9, "name": "Not this one", "end_date": null },
                }),
                false,
            ),
            (
                "query-3".into(),
                "Mine".into(),
                json!({ "kind": "filter", "keys": ["REL-1"], "jql": "assignee = currentUser()" }),
                false,
            ),
        ],
    );
    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    let read = |id: &'static str| {
        let pool = pool.clone();
        async move {
            let row = sqlx::query(
                "select source_query, board_type, sub_query, sprint from sync.saved_view
                  where entity_id = $1",
            )
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("the view is kept");
            (
                row.get::<String, _>("source_query"),
                row.get::<Option<String>, _>("board_type"),
                row.get::<Option<String>, _>("sub_query"),
                row.get::<Option<Value>, _>("sprint"),
            )
        }
    };
    assert_eq!(
        read("forge:view-7").await,
        (
            "is:open milestone:train".to_owned(),
            Some("kanban".to_owned()),
            None,
            Some(json!({ "id": 4, "name": "Train 12", "end_date": "2026-11-02" })),
        ),
        "its own query, layout and iteration; no sub-query declared, so none"
    );
    assert_eq!(
        read("forge:query-3").await,
        (String::new(), None, None, None),
        "a record with no query at the declared path has none, whatever its `jql` says"
    );
}

/// The other direction: a kind named `jira_view` that does not declare the
/// saved-view frame is not a saved view, so nothing is projected from it.
#[tokio::test]
async fn a_view_kind_that_declares_no_saved_view_frame_makes_no_ticket_view() {
    let pool = pool("saved_views_undeclared").await;
    let undeclared = Adapter {
        saved_view: false,
        ..JIRA
    };
    let (src, _) = source_of(
        undeclared,
        vec![(
            "board-22".into(),
            "PAY flow".into(),
            board(22, &["PAY-1"], three_columns()),
            false,
        )],
    );
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(ticket_view(&pool, "jira-board-22").await, None);
}

/// #1207, "Jira's boards and filters behave exactly as today": a Jira source
/// a person named *Jira EU* still lists its views under *Jira boards* and
/// *Jira filters*, the groups the Tickets pane puts last. The group is the
/// ticket system's name the kind declares, never the source's own.
#[tokio::test]
async fn a_renamed_jira_source_still_lists_its_views_under_jira_boards_and_filters() {
    let pool = pool("saved_views_renamed_jira").await;
    let renamed = Adapter {
        name: "Jira EU",
        ..JIRA
    };
    let (src, _) = source_of(
        renamed,
        vec![
            (
                "board-22".into(),
                "PAY flow".into(),
                board(22, &["PAY-1"], three_columns()),
                false,
            ),
            (
                "filter-9".into(),
                "Mine".into(),
                filter("9", &["PAY-1"]),
                false,
            ),
        ],
    );
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    let groups: Vec<(String, String)> = sqlx::query_as(
        "select id, view_group from knobas.ticket_view where saved_view like 'jira:%' order by id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        groups,
        [
            ("jira-board-22".to_owned(), "Jira boards".to_owned()),
            ("jira-filter-9".to_owned(), "Jira filters".to_owned())
        ]
    );
}
