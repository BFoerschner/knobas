//! Jira views (#877) through `knobas_sync::run_once`: a source that emits
//! `jira_view` items the way the Jira adapter does, then `sync.jira_view`,
//! `sync.jira_view_member` and `knobas.ticket_view` read back.
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

struct Boards {
    corpus: Corpus,
}

fn kind(id: &str) -> KindInfo {
    KindInfo {
        id: id.to_owned(),
        label: id.to_owned(),
        plural: id.to_owned(),
        monogram: "JV".into(),
        full_sync_exhaustive: true,
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
        SourceDescriptor {
            id: "jira".into(),
            adapter_kind: "jira".into(),
            name: "Jira".into(),
            capabilities: Vec::<Capability>::new(),
            adapter_version: "0.1.0".into(),
            auth_methods: Vec::new(),
            accepts_account: false,
            min_sync_interval_secs: 60,
            roles: Vec::new(),
            write_ops: Vec::new(),
            entity_kinds: vec![kind("ticket"), kind("jira_view")],
            config_schema: json!({ "type": "object", "properties": {} }),
            payload_paths: Vec::new(),
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
                entity: knobas_core::entity::EntityRef::new("jira", &key),
                kind: "jira_view".to_owned(),
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
    let corpus: Corpus = Arc::new(Mutex::new(views));
    (
        Boards {
            corpus: Arc::clone(&corpus),
        },
        corpus,
    )
}

async fn members(pool: &PgPool, view: &str) -> Vec<(String, i32)> {
    sqlx::query(
        "select entity_id, rank from sync.jira_view_member where view_id = $1 order by rank",
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
        "select name, view_group, columns, lanes, jira_ref from knobas.ticket_view where id = $1",
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
            row.get("jira_ref"),
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

    let row = sqlx::query("select ref_kind, ref_id, board_type, name, sub_query, columns from sync.jira_view where entity_id = 'jira:board-22'")
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
        "select ref_kind, columns, sprint from sync.jira_view where entity_id = 'jira:board-9'",
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
        sqlx::query_scalar("select entity_id from sync.jira_view order by entity_id")
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
        "select ref_kind from sync.jira_view where entity_id = 'jira:filter-10300'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ref_kind, "filter");
    let (name, group, columns, _, jira_ref) = ticket_view(&pool, "jira-filter-10300")
        .await
        .expect("a ticket view for the filter");
    assert_eq!(
        (name.as_str(), group.as_str(), columns, jira_ref.as_deref()),
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
    let projection = tokio::spawn(async move {
        let projected = knobas_sync::jira_views::project(&mut run, "jira").await;
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
        sqlx::query_scalar("select entity_id from sync.jira_view order by entity_id")
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
