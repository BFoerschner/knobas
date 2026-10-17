//! Jira views (#877, spec #808 "Jira views", R-3.20, R-3.21, R-3.24, R-3.26):
//! the Jira boards on the projects this source syncs, read at every sync and
//! handed to the sink as items of kind [`crate::KIND_JIRA_VIEW`].
//!
//! Read-only, so no `WriteOp`. Per board: its configuration (columns with
//! the statuses on each and their limits, the filter, a Kanban sub-query),
//! its saved filter's JQL, a Scrum board's active sprint, and then **one
//! keys-only search** of the board's query -- the filter, plus the Kanban
//! sub-filter, plus `sprint in openSprints()` for Scrum. Nothing is
//! translated: Jira decides which tickets a view holds, and the keys are kept
//! in the order Jira answered them.
//!
//! **Which boards.** The boards on each project in `projects`, deduplicated
//! (a board over two synced projects is listed under both). A source scoped
//! by `jql_filter` alone names no project, so it brings no boards in; the
//! *Boards and filters* setting is #878's.
//!
//! **A Jira without Jira Software** answers the board listing with a 404, and
//! that is no boards, not a failed sync. A board whose configuration or
//! filter this account may not read (403, 404) is left out, as a board the
//! account cannot see. Any other failure fails the run, for the reason the
//! issue walk's does: a partial answer would read as boards deleted in Jira.

use knobas_source::SourceError;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::JiraApi;

/// One page of `GET rest/agile/1.0/board`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BoardPage {
    #[serde(default)]
    pub values: Vec<Board>,
    #[serde(default)]
    pub is_last: Option<bool>,
}

/// One board as the listing names it.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Board {
    pub id: u64,
    pub name: String,
    /// `scrum`, `kanban` or `simple`.
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
}

/// `GET rest/agile/1.0/board/{id}/configuration`, the parts read.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BoardConfiguration {
    pub filter: Option<IdRef>,
    pub sub_query: Option<SubQuery>,
    pub column_config: Option<ColumnConfig>,
}

/// `{"id": "10100", "self": ".."}`; Jira sends the id as a string here.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct IdRef {
    pub id: Value,
}

impl IdRef {
    fn id(&self) -> String {
        match &self.id {
            Value::String(id) => id.clone(),
            other => other.to_string(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct SubQuery {
    pub query: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ColumnConfig {
    #[serde(default)]
    pub columns: Vec<Column>,
    /// `issueCount` when the limits count, `none` when the board has none.
    pub constraint_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Column {
    pub name: String,
    #[serde(default)]
    pub statuses: Vec<IdRef>,
    pub max: Option<u32>,
}

/// `GET rest/agile/1.0/board/{id}/sprint?state=active`.
#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct SprintPage {
    #[serde(default)]
    pub values: Vec<Sprint>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Sprint {
    pub id: u64,
    pub name: String,
    pub end_date: Option<String>,
}

/// `GET rest/api/2/filter/{id}`, the part read.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Filter {
    #[serde(default)]
    pub jql: String,
}

/// One entry of `GET rest/api/2/status`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Status {
    pub id: String,
    pub name: String,
}

/// One Jira view as read this sync: its item key (`board-<id>`), its name,
/// where Jira shows it, and the payload the engine projects.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct JiraView {
    pub key: String,
    pub name: String,
    pub web_url: String,
    pub payload: Value,
}

/// How many board pages, and key pages per view, a run reads before it
/// refuses: the same reason as the issue walk's `MAX_PAGES`.
const MAX_PAGES: u32 = 1_000;
/// Boards per listing page; Jira's own default.
const BOARD_PAGE: u32 = 50;

/// The query a view's keys come from (R-3.21): the board's filter, plus the
/// Kanban sub-filter, plus `sprint in openSprints()` on a Scrum board, keeping
/// the filter's own `ORDER BY` so the keys come back in the board's order.
pub(crate) fn view_jql(filter: &str, sub_query: Option<&str>, scrum: bool) -> String {
    let (condition, order) = split_order(filter);
    let mut clauses: Vec<String> = Vec::new();
    for part in [Some(condition), sub_query.map(str::trim)].into_iter().flatten() {
        if !part.is_empty() {
            clauses.push(format!("({part})"));
        }
    }
    if scrum {
        clauses.push("sprint in openSprints()".to_owned());
    }
    let mut jql = clauses.join(" AND ");
    if let Some(order) = order {
        if !jql.is_empty() {
            jql.push(' ');
        }
        jql.push_str(order);
    }
    jql
}

/// A JQL text split at its `ORDER BY` (outside quotes, any case).
fn split_order(jql: &str) -> (&str, Option<&str>) {
    let lower = jql.to_ascii_lowercase();
    let mut quote: Option<char> = None;
    let mut found = None;
    for (at, ch) in lower.char_indices() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => {}
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None => {
                let starts_word = at == 0 || !lower[..at].ends_with(|c: char| c.is_alphanumeric());
                if starts_word && lower[at..].starts_with("order by") {
                    found = Some(at);
                }
            }
        }
    }
    match found {
        Some(at) => (jql[..at].trim(), Some(jql[at..].trim())),
        None => (jql.trim(), None),
    }
}

/// Whether a failure is "this account may not read it", which leaves the
/// thing out rather than failing the run.
fn not_readable(error: &SourceError) -> bool {
    matches!(error.status(), Some(403 | 404))
}

/// Every Jira view this source brings in, in board id order.
pub(crate) async fn read(
    api: &(dyn JiraApi + '_),
    cfg: &crate::JiraConfig,
    base_url: &str,
) -> Result<Vec<JiraView>, SourceError> {
    let mut boards: Vec<Board> = Vec::new();
    for project in &cfg.projects {
        let mut start_at = 0;
        for page in 0.. {
            if page >= MAX_PAGES {
                return Err(SourceError::protocol(format!(
                    "the Jira board listing of {project} did not end within {MAX_PAGES} pages"
                )));
            }
            let listed = match api.boards(project, start_at, BOARD_PAGE).await {
                Ok(listed) => listed,
                Err(error) if error.status() == Some(404) => return Ok(Vec::new()),
                Err(error) => return Err(error),
            };
            let returned = u32::try_from(listed.values.len()).unwrap_or(u32::MAX);
            boards.extend(listed.values);
            start_at += returned;
            if returned == 0 || listed.is_last.unwrap_or(true) {
                break;
            }
        }
    }
    boards.sort_by_key(|board| board.id);
    boards.dedup_by_key(|board| board.id);
    if boards.is_empty() {
        return Ok(Vec::new());
    }
    let names: std::collections::HashMap<String, String> = api
        .statuses()
        .await?
        .into_iter()
        .map(|status| (status.id, status.name))
        .collect();
    let mut views = Vec::new();
    for board in boards {
        match one(api, cfg, base_url, &board, &names).await {
            Ok(view) => views.push(view),
            Err(error) if not_readable(&error) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(views)
}

/// One board's view: configuration, filter, sprint, then its keys.
async fn one(
    api: &(dyn JiraApi + '_),
    cfg: &crate::JiraConfig,
    base_url: &str,
    board: &Board,
    names: &std::collections::HashMap<String, String>,
) -> Result<JiraView, SourceError> {
    let config = api.board_configuration(board.id).await?;
    let filter_id = config.filter.as_ref().map(IdRef::id);
    let filter_jql = match &filter_id {
        Some(id) => api.filter(id).await?.jql,
        None => String::new(),
    };
    let sub_query = config
        .sub_query
        .as_ref()
        .and_then(|sub| sub.query.clone())
        .filter(|query| !query.trim().is_empty());
    let scrum = board.kind.as_deref() == Some("scrum");
    let sprint = if scrum {
        api.active_sprints(board.id)
            .await?
            .values
            .into_iter()
            .next()
            .map(|sprint| json!({ "id": sprint.id, "name": sprint.name, "end_date": sprint.end_date }))
    } else {
        None
    };
    let limits_count = config
        .column_config
        .as_ref()
        .and_then(|columns| columns.constraint_type.as_deref())
        .is_some_and(|kind| kind != "none");
    let columns: Vec<Value> = config
        .column_config
        .map(|columns| columns.columns)
        .unwrap_or_default()
        .into_iter()
        .map(|column| {
            let statuses: Vec<&String> = column
                .statuses
                .iter()
                .filter_map(|status| names.get(&status.id()))
                .collect();
            let limit = column.max.filter(|max| limits_count && *max > 0);
            json!({ "name": column.name, "statuses": statuses, "limit": limit })
        })
        .collect();
    let query = view_jql(&filter_jql, sub_query.as_deref(), scrum);
    let keys = keys(api, cfg, &query).await?;
    Ok(JiraView {
        key: format!("board-{}", board.id),
        name: board.name.clone(),
        web_url: format!(
            "{}/secure/RapidBoard.jspa?rapidView={}",
            base_url.trim_end_matches('/'),
            board.id
        ),
        payload: json!({
            "kind": "board",
            "id": board.id,
            "type": board.kind,
            "name": board.name,
            "filter_id": filter_id,
            "jql": filter_jql,
            "sub_query": sub_query,
            "query": query,
            "columns": columns,
            "sprint": sprint,
            "keys": keys,
        }),
    })
}

/// The keys a view's query answers, in Jira's order: `fields=key`, paged like
/// the issue walk.
async fn keys(
    api: &(dyn JiraApi + '_),
    cfg: &crate::JiraConfig,
    jql: &str,
) -> Result<Vec<String>, SourceError> {
    let mut keys: Vec<String> = Vec::new();
    let mut start_at = 0;
    for _ in 0..MAX_PAGES {
        let page = api.search(jql, start_at, cfg.page_size, "key").await?;
        let returned = u32::try_from(page.issues.len()).unwrap_or(u32::MAX);
        keys.extend(page.issues.into_iter().map(|raw| raw.issue.key));
        start_at += returned;
        if returned == 0 || start_at >= page.total {
            return Ok(keys);
        }
    }
    Err(SourceError::protocol(format!(
        "the keys of a Jira view did not end within {MAX_PAGES} pages"
    )))
}

/// A view as the item the sink takes.
pub(crate) fn item(source_id: &str, view: &JiraView) -> knobas_source::SyncItem {
    knobas_source::SyncItem {
        entity: knobas_core::entity::EntityRef::new(source_id, &view.key),
        kind: crate::KIND_JIRA_VIEW.to_owned(),
        title: view.name.clone(),
        // Not a text anybody searches for: the tickets are.
        body_text: String::new(),
        author: None,
        updated_at: None,
        payload: view.payload.clone(),
        web_url: Some(view.web_url.clone()),
        deleted: false,
    }
}

/// The item that says a view is gone from Jira.
pub(crate) fn gone(source_id: &str, key: &str) -> knobas_source::SyncItem {
    knobas_source::SyncItem {
        entity: knobas_core::entity::EntityRef::new(source_id, key),
        kind: crate::KIND_JIRA_VIEW.to_owned(),
        title: key.to_owned(),
        body_text: String::new(),
        author: None,
        updated_at: None,
        payload: json!({}),
        web_url: None,
        deleted: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CommentPage, Myself, SearchPage, ServerInfo, WorklogPage};
    use knobas_source::contract::VecSink;
    use std::sync::Mutex;

    /// A Jira Software with boards: each board is `(id, name, type, filter
    /// jql, sub-query, columns, keys its query answers)`. No issues: the
    /// issue walk answers nothing, so whatever a run emits is a view.
    #[derive(Default)]
    struct Agile {
        boards: Vec<(u64, &'static str, &'static str, &'static str, Option<&'static str>, Value, Vec<&'static str>)>,
        no_software: bool,
        forbidden: Vec<u64>,
        queries: Mutex<Vec<String>>,
    }

    fn board(id: u64) -> impl Fn(&Agile) -> Option<&(u64, &'static str, &'static str, &'static str, Option<&'static str>, Value, Vec<&'static str>)> {
        move |agile: &Agile| agile.boards.iter().find(|b| b.0 == id)
    }

    fn not_found() -> SourceError {
        SourceError::Protocol { status: Some(404), message: "no".to_owned() }
    }

    #[async_trait::async_trait]
    impl JiraApi for Agile {
        async fn server_info(&self) -> Result<ServerInfo, SourceError> {
            Ok(serde_json::from_value(json!({ "serverTime": "2026-10-17T10:00:00.000+0000" })).unwrap())
        }
        async fn myself(&self) -> Result<Myself, SourceError> {
            unreachable!("a sync run does not ask")
        }
        async fn fields(&self) -> Result<Vec<crate::model::FieldMeta>, SourceError> {
            unreachable!("a sync run does not ask")
        }
        async fn search(&self, jql: &str, start_at: u32, max: u32, fields: &str) -> Result<SearchPage, SourceError> {
            if fields != "key" {
                return Ok(serde_json::from_value(json!({ "total": 0, "issues": [] })).unwrap());
            }
            self.queries.lock().unwrap().push(jql.to_owned());
            let keys: Vec<&str> = self
                .boards
                .iter()
                .find(|b| jql.starts_with(&format!("({})", b.3.split(" ORDER").next().unwrap())))
                .map(|b| b.6.clone())
                .unwrap_or_default();
            let page: Vec<Value> = keys.iter().skip(start_at as usize).take(max as usize).map(|k| json!({ "key": k })).collect();
            Ok(serde_json::from_value(json!({ "total": keys.len(), "issues": page })).unwrap())
        }
        async fn comments(&self, _: &str, _: u32, _: u32) -> Result<CommentPage, SourceError> {
            unreachable!()
        }
        async fn worklogs(&self, _: &str) -> Result<WorklogPage, SourceError> {
            unreachable!()
        }
        async fn boards(&self, _project: &str, start_at: u32, max: u32) -> Result<BoardPage, SourceError> {
            if self.no_software {
                return Err(not_found());
            }
            let values: Vec<Value> = self.boards.iter().skip(start_at as usize).take(max as usize)
                .map(|b| json!({ "id": b.0, "name": b.1, "type": b.2 })).collect();
            let is_last = start_at as usize + values.len() >= self.boards.len();
            Ok(serde_json::from_value(json!({ "values": values, "isLast": is_last })).unwrap())
        }
        async fn board_configuration(&self, id: u64) -> Result<BoardConfiguration, SourceError> {
            if self.forbidden.contains(&id) {
                return Err(SourceError::Protocol { status: Some(403), message: "no".to_owned() });
            }
            let b = board(id)(self).ok_or_else(not_found)?;
            let mut config = json!({ "filter": { "id": format!("{}", 100 + id) }, "columnConfig": b.5 });
            if let Some(sub) = b.4 {
                config["subQuery"] = json!({ "query": sub });
            }
            Ok(serde_json::from_value(config).unwrap())
        }
        async fn active_sprints(&self, id: u64) -> Result<SprintPage, SourceError> {
            Ok(serde_json::from_value(json!({ "values": [
                { "id": 7, "name": format!("Sprint of {id}"), "state": "active", "endDate": "2026-10-31T10:00:00.000Z" }
            ] })).unwrap())
        }
        async fn filter(&self, id: &str) -> Result<Filter, SourceError> {
            let board_id: u64 = id.parse::<u64>().unwrap() - 100;
            Ok(Filter { jql: board(board_id)(self).ok_or_else(not_found)?.3.to_owned() })
        }
        async fn statuses(&self) -> Result<Vec<Status>, SourceError> {
            Ok(["To Do", "In Progress", "In Review", "Done"].iter().enumerate()
                .map(|(n, name)| Status { id: format!("{}", 10_000 + n), name: (*name).to_owned() }).collect())
        }
    }

    fn columns(limit: Option<u32>, constraint: &str) -> Value {
        json!({ "constraintType": constraint, "columns": [
            { "name": "To Do", "statuses": [{ "id": "10000" }] },
            { "name": "Doing", "statuses": [{ "id": "10001" }, { "id": "10002" }], "max": limit },
            { "name": "Done", "statuses": [{ "id": "10003" }, { "id": "99999" }] },
        ] })
    }

    fn seeded() -> Agile {
        Agile {
            boards: vec![
                (21, "PAY sprint board", "scrum", "project = PAY ORDER BY Rank ASC", None,
                 columns(Some(9), "none"), vec!["PAY-231", "PAY-228"]),
                (22, "PAY flow", "kanban", "project in (PAY, WEB) ORDER BY Rank ASC", Some("fixVersion is EMPTY"),
                 columns(Some(3), "issueCount"), vec!["WEB-5", "PAY-240", "PAY-231"]),
            ],
            ..Agile::default()
        }
    }

    fn cfg() -> crate::JiraConfig {
        crate::JiraConfig::from_json(&json!({ "projects": ["PAY", "OPS"] })).unwrap()
    }

    async fn sync(api: &Agile, cursor: Option<String>) -> (Vec<knobas_source::SyncItem>, String) {
        let mut sink = VecSink(Vec::new());
        let cfg = cfg();
        let next = crate::sync::SyncRun { api, cfg: &cfg, source_id: "jira", base_url: "https://jira.example/" }
            .run(cursor, &mut sink)
            .await
            .expect("the run succeeds");
        (sink.0, next)
    }

    #[test]
    fn a_views_query_is_the_filter_the_sub_filter_and_the_open_sprint_in_the_filters_order() {
        assert_eq!(
            view_jql("project = PAY ORDER BY Rank ASC", None, true),
            "(project = PAY) AND sprint in openSprints() ORDER BY Rank ASC"
        );
        assert_eq!(
            view_jql("project in (PAY, WEB) order by rank", Some(" fixVersion is EMPTY "), false),
            "(project in (PAY, WEB)) AND (fixVersion is EMPTY) order by rank"
        );
        assert_eq!(
            view_jql(r#"summary ~ "order by me" ORDER BY created"#, None, false),
            r#"(summary ~ "order by me") ORDER BY created"#
        );
        assert_eq!(view_jql("ORDER BY Rank", None, false), "ORDER BY Rank");
    }

    #[tokio::test]
    async fn each_board_arrives_as_a_view_with_jiras_columns_limits_sprint_and_keys_in_order() {
        let api = seeded();
        let (items, _) = sync(&api, None).await;
        let keys: Vec<&str> = items.iter().map(|i| i.entity.key.as_str()).collect();
        assert_eq!(keys, ["board-21", "board-22"]);
        assert!(items.iter().all(|i| i.kind == "jira_view" && !i.deleted));

        let scrum = &items[0];
        assert_eq!(scrum.title, "PAY sprint board");
        assert_eq!(scrum.web_url.as_deref(), Some("https://jira.example/secure/RapidBoard.jspa?rapidView=21"));
        assert_eq!(scrum.payload["keys"], json!(["PAY-231", "PAY-228"]));
        assert_eq!(scrum.payload["sprint"]["name"], "Sprint of 21");
        assert_eq!(scrum.payload["query"], "(project = PAY) AND sprint in openSprints() ORDER BY Rank ASC");
        // A board whose limits do not count (`none`) has none.
        assert_eq!(scrum.payload["columns"][1], json!({ "name": "Doing", "statuses": ["In Progress", "In Review"], "limit": null }));

        let kanban = &items[1];
        assert_eq!(kanban.payload["keys"], json!(["WEB-5", "PAY-240", "PAY-231"]), "Jira's order, unsynced project included");
        assert_eq!(kanban.payload["sprint"], Value::Null);
        assert_eq!(kanban.payload["sub_query"], "fixVersion is EMPTY");
        assert_eq!(kanban.payload["columns"][1]["limit"], 3);
        assert_eq!(kanban.payload["columns"][2]["statuses"], json!(["Done"]), "a status id Jira does not name is left out");
        assert_eq!(
            api.queries.lock().unwrap().clone(),
            [
                "(project = PAY) AND sprint in openSprints() ORDER BY Rank ASC",
                "(project in (PAY, WEB)) AND (fixVersion is EMPTY) ORDER BY Rank ASC",
            ]
        );
    }

    #[tokio::test]
    async fn an_unchanged_board_is_not_delivered_again_and_the_position_stays_put() {
        let api = seeded();
        let (_, first) = sync(&api, None).await;
        let (items, second) = sync(&api, Some(first.clone())).await;
        assert!(items.is_empty(), "{items:?}");
        assert_eq!(second, first, "an idle run hands back the cursor it was given");
    }

    #[tokio::test]
    async fn a_board_whose_keys_changed_is_delivered_again() {
        let mut api = seeded();
        let (_, first) = sync(&api, None).await;
        api.boards[1].6 = vec!["PAY-231", "PAY-240"];
        let (items, second) = sync(&api, Some(first.clone())).await;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].payload["keys"], json!(["PAY-231", "PAY-240"]));
        assert_ne!(second, first);
    }

    #[tokio::test]
    async fn a_board_deleted_in_jira_is_delivered_as_gone_at_the_next_sync() {
        let mut api = seeded();
        let (_, first) = sync(&api, None).await;
        api.boards.remove(0);
        let (items, second) = sync(&api, Some(first)).await;
        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!(items[0].entity.key, "board-21");
        assert!(items[0].deleted);
        let (again, third) = sync(&api, Some(second.clone())).await;
        assert!(again.is_empty(), "gone once, not on every run: {again:?}");
        assert_eq!(third, second);
    }

    #[tokio::test]
    async fn a_jira_without_jira_software_brings_no_views_and_keeps_the_cursor_free_of_them() {
        let api = Agile { no_software: true, ..seeded() };
        let (items, cursor) = sync(&api, None).await;
        assert!(items.is_empty());
        assert!(!cursor.contains("views"), "{cursor}");
    }

    #[tokio::test]
    async fn a_board_this_account_cannot_read_is_left_out() {
        let api = Agile { forbidden: vec![21], ..seeded() };
        let (items, _) = sync(&api, None).await;
        let keys: Vec<&str> = items.iter().map(|i| i.entity.key.as_str()).collect();
        assert_eq!(keys, ["board-22"]);
    }
}
