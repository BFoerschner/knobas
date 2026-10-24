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
//! **Which boards** is the *Boards and filters* setting (#878, `boards`):
//! by default the boards on each project in `projects`, deduplicated (a board
//! over two synced projects is listed under both; a source scoped by
//! `jql_filter` alone names no project, so it brings no boards in); or every
//! board this account can see; or the boards whose saved filter is one of its
//! favourite filters; or none. **Favourite filters** (`favourite_filters`,
//! on by default) come in too, each searched by its JQL as Jira holds it.
//!
//! **A Jira without Jira Software** answers the board listing with a 404, and
//! that is no boards, not a failed sync. A board whose configuration or
//! filter this account may not read (403, 404) is left out, as a board the
//! account cannot see. Any other failure fails the run, for the reason the
//! issue walk's does: a partial answer would read as boards deleted in Jira.

use knobas_source::SourceError;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::BoardScope;
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

/// One entry of `GET rest/api/2/filter/favourite` (#878), the parts read.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FavouriteFilter {
    pub id: Value,
    pub name: String,
    #[serde(default)]
    pub jql: String,
    pub view_url: Option<String>,
}

impl FavouriteFilter {
    fn id(&self) -> String {
        match &self.id {
            Value::String(id) => id.clone(),
            other => other.to_string(),
        }
    }
}

/// One page of a keys-only search (`fields=key`).
#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct KeysPage {
    #[serde(default)]
    pub total: u32,
    #[serde(default)]
    pub issues: Vec<KeyOnly>,
}

/// An issue as a keys-only search names it; its `fields` is `null`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct KeyOnly {
    pub key: String,
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
/// The keys-only page: Jira Data Center's search maximum (#1700), the keys
/// being all a view's page carries.
const KEYS_PAGE: u32 = 1_000;

/// The query a view's keys come from (R-3.21): the board's filter, plus the
/// Kanban sub-filter, plus `sprint in openSprints()` on a Scrum board, keeping
/// the filter's own `ORDER BY` so the keys come back in the board's order.
pub(crate) fn view_jql(filter: &str, sub_query: Option<&str>, scrum: bool) -> String {
    let (condition, order) = split_order(filter);
    let mut clauses: Vec<String> = Vec::new();
    for part in [Some(condition), sub_query.map(str::trim)]
        .into_iter()
        .flatten()
    {
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

/// How long an idle run leaves the boards unread (#1700, ruling D6): a
/// board's filter can change its membership with no issue changing, so the
/// floor is an hour rather than never.
const REREAD_AFTER: chrono::Duration = chrono::Duration::hours(1);

/// Whether this run reads the boards though the issue walk emitted nothing:
/// never read, an hour gone since, a clock that cannot say, or a setting
/// changed since.
pub(crate) fn due(
    last: Option<&crate::cursor::BoardRead>,
    now: Option<chrono::DateTime<chrono::Utc>>,
    cfg: &crate::JiraConfig,
) -> bool {
    match (last, now) {
        (Some(last), Some(now)) => {
            last.scope != scope(cfg)
                || !(chrono::Duration::zero()..REREAD_AFTER).contains(&(now - last.at))
        }
        _ => true,
    }
}

/// Whether the setting brings any view in. One that does not asks Jira
/// nothing, so it keeps no read stamp and its cursor bytes stay put.
pub(crate) fn reads_any(cfg: &crate::JiraConfig) -> bool {
    cfg.boards != BoardScope::None || cfg.favourite_filters
}

/// What the *Boards and filters* setting reads, to tell a changed setting from
/// an idle hour.
pub(crate) fn scope(cfg: &crate::JiraConfig) -> String {
    format!(
        "{:?}|{}|{}",
        cfg.boards,
        cfg.favourite_filters,
        cfg.projects.join(",")
    )
}

/// Whether a failure is "this account may not read it", which leaves the
/// thing out rather than failing the run.
fn not_readable(error: &SourceError) -> bool {
    matches!(error.status(), Some(403 | 404))
}

/// Every Jira view this source brings in: the boards the *Boards and
/// filters* setting names, in board id order, then the favourite filters, in
/// filter id order (#878).
pub(crate) async fn read(
    api: &(dyn JiraApi + '_),
    cfg: &crate::JiraConfig,
    base_url: &str,
) -> Result<Vec<JiraView>, SourceError> {
    let favourites = if cfg.favourite_filters || cfg.boards == BoardScope::Favourites {
        // A 404 is a Jira that answers no favourites here: none, not a
        // failed sync, for the board listing's reason.
        match api.favourite_filters().await {
            Ok(favourites) => favourites,
            Err(error) if error.status() == Some(404) => Vec::new(),
            Err(error) => return Err(error),
        }
    } else {
        Vec::new()
    };
    let mut views = boards(api, cfg, base_url, &favourites).await?;
    if cfg.favourite_filters {
        let mut filters = favourites;
        filters.sort_by_key(|filter| (filter.id().parse::<u64>().ok(), filter.id()));
        filters.dedup_by_key(|filter| filter.id());
        for filter in &filters {
            match filter_view(api, base_url, filter).await {
                Ok(view) => views.push(view),
                // A filter Jira can no longer run (400: a field or a project
                // gone since it was saved) or this account may not read is
                // left out, like an unreadable board: one stale favourite
                // must not fail every sync of the source.
                Err(error) if not_readable(&error) || error.status() == Some(400) => {}
                Err(error) => return Err(error),
            }
        }
    }
    Ok(views)
}

/// The boards the setting names (#878, D17). *Only boards built on my
/// favourite filters* is the boards whose saved filter is a favourite filter: Jira
/// Data Center's Agile REST has no favourite-board read (ruling in #878's PR).
async fn boards(
    api: &(dyn JiraApi + '_),
    cfg: &crate::JiraConfig,
    base_url: &str,
    favourites: &[FavouriteFilter],
) -> Result<Vec<JiraView>, SourceError> {
    let listings: Vec<Option<&str>> = match cfg.boards {
        BoardScope::None => return Ok(Vec::new()),
        BoardScope::Synced => cfg.projects.iter().map(|p| Some(p.as_str())).collect(),
        BoardScope::Favourites | BoardScope::All => vec![None],
    };
    let mut boards: Vec<Board> = Vec::new();
    for project in listings {
        boards.extend(listing(api, project).await?);
    }
    boards.sort_by_key(|board| board.id);
    boards.dedup_by_key(|board| board.id);
    if boards.is_empty() {
        return Ok(Vec::new());
    }
    let only: Option<std::collections::HashSet<String>> = (cfg.boards == BoardScope::Favourites)
        .then(|| favourites.iter().map(FavouriteFilter::id).collect());
    let names: std::collections::HashMap<String, String> = api
        .statuses()
        .await?
        .into_iter()
        .map(|status| (status.id, status.name))
        .collect();
    let mut views = Vec::new();
    for board in boards {
        match one(api, base_url, &board, &names, only.as_ref()).await {
            Ok(Some(view)) => views.push(view),
            Ok(None) => {}
            // A board's filter Jira can no longer run (400), like a favourite's.
            Err(error) if not_readable(&error) || error.status() == Some(400) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(views)
}

/// One board listing: a project's, or with `None` every board this account
/// can see, read twice (#2003).
///
/// **Why twice.** The listing pages by `startAt`, and Jira Software lists
/// boards by name, ignoring case (measured on the seeded Jira 10.3 for #2003;
/// `orderBy` is ignored). A board deleted from a page already read shifts the
/// first board of the next page onto it, and a board renamed into a page
/// already read moves out of the unread ones; either way no page returns the
/// board it displaced. The listing offers no keyset, and since a rename
/// moves a row, no order the issue walk's anchoring (`walk.rs`) could stand
/// on. Every board read is followed by [`gone`] for each view the run did not
/// read, so a board missed here would be delivered as gone. Reading the
/// listing a second time and keeping the union is what Gitea's repository
/// listing does (#1979): a board one listing missed is in the other unless a
/// second change lands on the same row in the second. The caller drops the
/// boards met twice.
async fn listing(
    api: &(dyn JiraApi + '_),
    project: Option<&str>,
) -> Result<Vec<Board>, SourceError> {
    let mut boards = listed_once(api, project).await?;
    boards.extend(listed_once(api, project).await?);
    Ok(boards)
}

/// One pass over a board listing, paged.
async fn listed_once(
    api: &(dyn JiraApi + '_),
    project: Option<&str>,
) -> Result<Vec<Board>, SourceError> {
    let mut boards = Vec::new();
    let mut start_at = 0;
    for page in 0.. {
        if page >= MAX_PAGES {
            return Err(SourceError::protocol(format!(
                "the Jira board listing of {} did not end within {MAX_PAGES} pages",
                project.unwrap_or("every project")
            )));
        }
        // A 404 is no boards on this project, not no boards at all: a
        // Jira without Jira Software answers it on every project, and a
        // project whose listing alone answers it must not take the other
        // projects' boards with it, which would deliver them as gone.
        let listed = match api.boards(project, start_at, BOARD_PAGE).await {
            Ok(listed) => listed,
            Err(error) if error.status() == Some(404) => break,
            Err(error) => return Err(error),
        };
        let returned = u32::try_from(listed.values.len()).unwrap_or(u32::MAX);
        boards.extend(listed.values);
        start_at += returned;
        if returned == 0 || listed.is_last.unwrap_or(true) {
            break;
        }
    }
    Ok(boards)
}

/// One favourite filter's view (#878): its JQL as Jira holds it, searched as
/// it is, keys in Jira's order. No columns: Jira gives a filter none.
async fn filter_view(
    api: &(dyn JiraApi + '_),
    base_url: &str,
    filter: &FavouriteFilter,
) -> Result<JiraView, SourceError> {
    let id = filter.id();
    let keys = keys(api, &filter.jql).await?;
    Ok(JiraView {
        key: format!("filter-{id}"),
        name: filter.name.clone(),
        web_url: filter
            .view_url
            .clone()
            .unwrap_or_else(|| format!("{}/issues/?filter={id}", base_url.trim_end_matches('/'))),
        payload: json!({
            "kind": "filter",
            "id": id,
            "type": null,
            "name": filter.name,
            "filter_id": id,
            "jql": filter.jql,
            "sub_query": null,
            "query": filter.jql,
            "columns": [],
            "sprint": null,
            "keys": keys,
        }),
    })
}

/// One board's view: configuration, filter, sprint, then its keys. `None`
/// when `only` names the filters a board must have and this one's is not
/// among them.
async fn one(
    api: &(dyn JiraApi + '_),
    base_url: &str,
    board: &Board,
    names: &std::collections::HashMap<String, String>,
    only: Option<&std::collections::HashSet<String>>,
) -> Result<Option<JiraView>, SourceError> {
    let config = api.board_configuration(board.id).await?;
    let filter_id = config.filter.as_ref().map(IdRef::id);
    if let Some(only) = only
        && !filter_id.as_ref().is_some_and(|id| only.contains(id))
    {
        return Ok(None);
    }
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
    let keys = keys(api, &query).await?;
    Ok(Some(JiraView {
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
    }))
}

/// The keys a view's query answers, in Jira's order: `fields=key`, paged like
/// the issue walk.
async fn keys(api: &(dyn JiraApi + '_), jql: &str) -> Result<Vec<String>, SourceError> {
    let mut keys: Vec<String> = Vec::new();
    let mut start_at = 0;
    for _ in 0..MAX_PAGES {
        let page = api.search_keys(jql, start_at, KEYS_PAGE).await?;
        let returned = u32::try_from(page.issues.len()).unwrap_or(u32::MAX);
        keys.extend(page.issues.into_iter().map(|issue| issue.key));
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
    use std::sync::atomic::Ordering;

    /// A Jira Software with boards: each board is `(id, name, type, filter
    /// jql, sub-query, columns, keys its query answers)`. No issues: the
    /// issue walk answers nothing, so whatever a run emits is a view.
    type FakeBoard = (
        u64,
        &'static str,
        &'static str,
        &'static str,
        Option<&'static str>,
        Value,
        Vec<&'static str>,
    );

    #[derive(Default)]
    struct Agile {
        boards: Vec<FakeBoard>,
        no_software: bool,
        /// Projects whose board listing alone answers 404.
        unlisted: Vec<&'static str>,
        forbidden: Vec<u64>,
        /// Boards on no synced project: only *Every board I can see* lists them.
        elsewhere: Vec<FakeBoard>,
        /// This account's favourite filters: `(id, name, jql, keys)`.
        favourites: Vec<(u64, &'static str, &'static str, Vec<&'static str>)>,
        /// A query Jira refuses with a 400 (a field gone since it was saved).
        refused: Option<&'static str>,
        /// Minutes the server clock stands past 10:00 UTC.
        clock_minutes: i64,
        queries: Mutex<Vec<String>>,
        /// The page size each keys-only search asked for.
        key_pages: Mutex<Vec<u32>>,
        /// What Jira holds once the first board page has been answered: a
        /// board deleted or renamed while the listing is read.
        then: Option<Vec<FakeBoard>>,
        /// Board pages answered so far.
        board_pages: std::sync::atomic::AtomicU32,
    }

    impl Agile {
        /// The boards Jira holds now.
        fn held(&self) -> &[FakeBoard] {
            match &self.then {
                Some(then) if self.board_pages.load(Ordering::SeqCst) > 0 => then,
                _ => &self.boards,
            }
        }

        fn board(&self, id: u64) -> Option<&FakeBoard> {
            self.held()
                .iter()
                .chain(&self.elsewhere)
                .find(|b| b.0 == id)
        }
    }

    fn not_found() -> SourceError {
        SourceError::Protocol {
            status: Some(404),
            message: "no".to_owned(),
        }
    }

    #[async_trait::async_trait]
    impl JiraApi for Agile {
        async fn server_info(&self) -> Result<ServerInfo, SourceError> {
            let time = format!(
                "2026-10-17T{:02}:{:02}:00.000+0000",
                10 + self.clock_minutes / 60,
                self.clock_minutes % 60
            );
            Ok(serde_json::from_value(json!({ "serverTime": time })).unwrap())
        }
        async fn myself(&self) -> Result<Myself, SourceError> {
            unreachable!("a sync run does not ask")
        }
        async fn fields(&self) -> Result<Vec<crate::model::FieldMeta>, SourceError> {
            unreachable!("a sync run does not ask")
        }
        async fn search(
            &self,
            jql: &str,
            start_at: u32,
            max: u32,
            fields: &str,
        ) -> Result<SearchPage, SourceError> {
            if fields != "key" {
                return Ok(serde_json::from_value(json!({ "total": 0, "issues": [] })).unwrap());
            }
            self.queries.lock().unwrap().push(jql.to_owned());
            self.key_pages.lock().unwrap().push(max);
            if self.refused == Some(jql) {
                return Err(SourceError::Protocol {
                    status: Some(400),
                    message: "Field 'gone' does not exist".to_owned(),
                });
            }
            let keys: Vec<&str> = match self.favourites.iter().find(|f| f.2 == jql) {
                Some(filter) => filter.3.clone(),
                None => self
                    .boards
                    .iter()
                    .chain(&self.elsewhere)
                    .find(|b| {
                        jql.starts_with(&format!("({})", b.3.split(" ORDER").next().unwrap()))
                    })
                    .map(|b| b.6.clone())
                    .unwrap_or_default(),
            };
            let page: Vec<Value> = keys
                .iter()
                .skip(start_at as usize)
                .take(max as usize)
                .map(|k| json!({ "key": k }))
                .collect();
            Ok(serde_json::from_value(json!({ "total": keys.len(), "issues": page })).unwrap())
        }
        async fn comments(&self, _: &str, _: u32, _: u32) -> Result<CommentPage, SourceError> {
            unreachable!()
        }
        async fn worklogs(&self, _: &str) -> Result<WorklogPage, SourceError> {
            unreachable!()
        }
        async fn boards(
            &self,
            project: Option<&str>,
            start_at: u32,
            max: u32,
        ) -> Result<BoardPage, SourceError> {
            if self.no_software || project.is_some_and(|p| self.unlisted.contains(&p)) {
                return Err(not_found());
            }
            let mut listed: Vec<&FakeBoard> = match project {
                Some(_) => self.held().iter().collect(),
                None => self.held().iter().chain(&self.elsewhere).collect(),
            };
            // Jira Software 10.3's order, measured on the seeded Jira for
            // #2003: by name, ignoring case, so a rename moves a row.
            listed.sort_by_key(|b| b.1.to_lowercase());
            self.board_pages.fetch_add(1, Ordering::SeqCst);
            let values: Vec<Value> = listed
                .iter()
                .skip(start_at as usize)
                .take(max as usize)
                .map(|b| json!({ "id": b.0, "name": b.1, "type": b.2 }))
                .collect();
            let is_last = start_at as usize + values.len() >= listed.len();
            Ok(serde_json::from_value(json!({ "values": values, "isLast": is_last })).unwrap())
        }
        async fn favourite_filters(&self) -> Result<Vec<FavouriteFilter>, SourceError> {
            Ok(self
                .favourites
                .iter()
                .map(|f| {
                    serde_json::from_value(json!({
                        "id": f.0.to_string(), "name": f.1, "jql": f.2,
                        "viewUrl": format!("https://jira.example/issues/?filter={}", f.0),
                        "favourite": true,
                    }))
                    .unwrap()
                })
                .collect())
        }
        async fn board_configuration(&self, id: u64) -> Result<BoardConfiguration, SourceError> {
            if self.forbidden.contains(&id) {
                return Err(SourceError::Protocol {
                    status: Some(403),
                    message: "no".to_owned(),
                });
            }
            let b = self.board(id).ok_or_else(not_found)?;
            let mut config =
                json!({ "filter": { "id": format!("{}", 100 + id) }, "columnConfig": b.5 });
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
            Ok(Filter {
                jql: self.board(board_id).ok_or_else(not_found)?.3.to_owned(),
            })
        }
        async fn statuses(&self) -> Result<Vec<Status>, SourceError> {
            Ok(["To Do", "In Progress", "In Review", "Done"]
                .iter()
                .enumerate()
                .map(|(n, name)| Status {
                    id: format!("{}", 10_000 + n),
                    name: (*name).to_owned(),
                })
                .collect())
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
                (
                    21,
                    "PAY sprint board",
                    "scrum",
                    "project = PAY ORDER BY Rank ASC",
                    None,
                    columns(Some(9), "none"),
                    vec!["PAY-231", "PAY-228"],
                ),
                (
                    22,
                    "PAY flow",
                    "kanban",
                    "project in (PAY, WEB) ORDER BY Rank ASC",
                    Some("fixVersion is EMPTY"),
                    columns(Some(3), "issueCount"),
                    vec!["WEB-5", "PAY-240", "PAY-231"],
                ),
            ],
            ..Agile::default()
        }
    }

    fn cfg() -> crate::JiraConfig {
        crate::JiraConfig::from_json(&json!({ "projects": ["PAY", "OPS"] })).unwrap()
    }

    async fn sync(api: &Agile, cursor: Option<String>) -> (Vec<knobas_source::SyncItem>, String) {
        sync_with(api, &cfg(), cursor).await
    }

    /// A run under a *Boards and filters* setting other than the default.
    fn cfg_with(boards: &str, favourite_filters: bool) -> crate::JiraConfig {
        crate::JiraConfig::from_json(&json!({
            "projects": ["PAY", "OPS"],
            "boards": boards,
            "favourite_filters": favourite_filters,
        }))
        .unwrap()
    }

    async fn sync_with(
        api: &Agile,
        cfg: &crate::JiraConfig,
        cursor: Option<String>,
    ) -> (Vec<knobas_source::SyncItem>, String) {
        let mut sink = VecSink(Vec::new());
        let next = crate::sync::SyncRun {
            api,
            cfg,
            source_id: "jira",
            base_url: "https://jira.example/",
        }
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
            view_jql(
                "project in (PAY, WEB) order by rank",
                Some(" fixVersion is EMPTY "),
                false
            ),
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
        assert_eq!(
            scrum.web_url.as_deref(),
            Some("https://jira.example/secure/RapidBoard.jspa?rapidView=21")
        );
        assert_eq!(scrum.payload["keys"], json!(["PAY-231", "PAY-228"]));
        // The whole object, because the shell types it field for field:
        // `knobas_app::ticket_views::ActiveSprint` (#1329) is this shape, and
        // `sync.saved_view` passes any object through unchecked.
        assert_eq!(
            scrum.payload["sprint"],
            json!({ "id": 7, "name": "Sprint of 21", "end_date": "2026-10-31T10:00:00.000Z" })
        );
        assert_eq!(
            scrum.payload["query"],
            "(project = PAY) AND sprint in openSprints() ORDER BY Rank ASC"
        );
        // A board whose limits do not count (`none`) has none.
        assert_eq!(
            scrum.payload["columns"][1],
            json!({ "name": "Doing", "statuses": ["In Progress", "In Review"], "limit": null })
        );

        let kanban = &items[1];
        assert_eq!(
            kanban.payload["keys"],
            json!(["WEB-5", "PAY-240", "PAY-231"]),
            "Jira's order, unsynced project included"
        );
        assert_eq!(kanban.payload["sprint"], Value::Null);
        assert_eq!(kanban.payload["sub_query"], "fixVersion is EMPTY");
        assert_eq!(kanban.payload["columns"][1]["limit"], 3);
        assert_eq!(
            kanban.payload["columns"][2]["statuses"],
            json!(["Done"]),
            "a status id Jira does not name is left out"
        );
        assert_eq!(
            api.queries.lock().unwrap().clone(),
            [
                "(project = PAY) AND sprint in openSprints() ORDER BY Rank ASC",
                "(project in (PAY, WEB)) AND (fixVersion is EMPTY) ORDER BY Rank ASC",
            ]
        );
    }

    /// What Jira DC 10.3 answered to `fields=key` on the seeded instance
    /// (#877's first `jira-live`): `"fields": null` on every issue, which the
    /// issue walk's record refuses.
    #[test]
    fn a_keys_only_page_reads_jiras_null_fields() {
        let page: KeysPage = serde_json::from_str(
            r#"{"expand":"names,schema","startAt":0,"maxResults":100,"total":1,
                "issues":[{"expand":"operations","id":"10307","self":"x","key":"PAY-231","fields":null}]}"#,
        )
        .expect("a keys-only page");
        assert_eq!(page.total, 1);
        assert_eq!(page.issues[0].key, "PAY-231");
    }

    #[tokio::test]
    async fn an_unchanged_board_is_not_delivered_again_and_the_position_stays_put() {
        let api = seeded();
        let (_, first) = sync(&api, None).await;
        let (items, second) = sync(&api, Some(first.clone())).await;
        assert!(items.is_empty(), "{items:?}");
        assert_eq!(
            second, first,
            "an idle run hands back the cursor it was given"
        );
    }

    #[tokio::test]
    async fn a_board_whose_keys_changed_is_delivered_again() {
        let mut api = seeded();
        let (_, first) = sync(&api, None).await;
        api.boards[1].6 = vec!["PAY-231", "PAY-240"];
        api.clock_minutes = 120; // an hour on: the boards are read again
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
        api.clock_minutes = 120; // an hour on: the boards are read again
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
        let api = Agile {
            no_software: true,
            ..seeded()
        };
        let (items, cursor) = sync(&api, None).await;
        assert!(items.is_empty());
        assert!(!cursor.contains("views"), "{cursor}");
    }

    /// One project's listing answering 404 leaves the other projects' boards
    /// in, rather than reading as a Jira without Jira Software and
    /// delivering every board as gone.
    #[tokio::test]
    async fn a_project_whose_board_listing_answers_404_keeps_the_other_projects_boards() {
        let mut api = seeded();
        let (_, first) = sync(&api, None).await;
        api.unlisted = vec!["OPS"];
        let (items, second) = sync(&api, Some(first.clone())).await;
        assert!(items.is_empty(), "{items:?}");
        assert_eq!(second, first);
    }

    /// The upgrade: a cursor written before the views (no `views` field, the
    /// shape a Jira without boards still writes) delivers every board once,
    /// and none as gone.
    #[tokio::test]
    async fn a_cursor_from_before_the_views_delivers_every_board_once() {
        let mut api = Agile {
            no_software: true,
            ..seeded()
        };
        let (_, old) = sync(&api, None).await;
        assert!(!old.contains("views"), "{old}");
        api.no_software = false;
        api.clock_minutes = 120; // an hour on: the boards are read again
        let (items, next) = sync(&api, Some(old)).await;
        let keys: Vec<(&str, bool)> = items
            .iter()
            .map(|i| (i.entity.key.as_str(), i.deleted))
            .collect();
        assert_eq!(keys, [("board-21", false), ("board-22", false)]);
        let (again, _) = sync(&api, Some(next)).await;
        assert!(again.is_empty(), "{again:?}");
    }

    #[tokio::test]
    async fn a_board_this_account_cannot_read_is_left_out() {
        let api = Agile {
            forbidden: vec![21],
            ..seeded()
        };
        let (items, _) = sync(&api, None).await;
        let keys: Vec<&str> = items.iter().map(|i| i.entity.key.as_str()).collect();
        assert_eq!(keys, ["board-22"]);
    }

    /// The seed's favourite filter, plus one boards 22's filter is (id 122).
    fn with_favourites() -> Agile {
        Agile {
            favourites: vec![
                (
                    10_300,
                    "PAY without a fix version",
                    "project = PAY AND fixVersion is EMPTY ORDER BY updated DESC",
                    vec!["PAY-240", "WEB-9", "PAY-231"],
                ),
                (
                    122,
                    "PAY flow filter",
                    "project in (PAY, WEB) ORDER BY Rank ASC",
                    vec!["WEB-5"],
                ),
            ],
            elsewhere: vec![(
                31,
                "WEB board",
                "kanban",
                "project = WEB ORDER BY Rank ASC",
                None,
                columns(None, "none"),
                vec!["WEB-5"],
            )],
            ..seeded()
        }
    }

    fn keys_of(items: &[knobas_source::SyncItem]) -> Vec<(&str, bool)> {
        items
            .iter()
            .map(|i| (i.entity.key.as_str(), i.deleted))
            .collect()
    }

    #[tokio::test]
    async fn a_favourite_filter_arrives_as_a_view_holding_what_its_jql_answers_in_jiras_order() {
        let api = with_favourites();
        let (items, _) = sync(&api, None).await;
        assert_eq!(
            keys_of(&items),
            [
                ("board-21", false),
                ("board-22", false),
                ("filter-122", false),
                ("filter-10300", false),
            ]
        );
        let filter = &items[3];
        assert_eq!(filter.kind, "jira_view");
        assert_eq!(filter.title, "PAY without a fix version");
        assert_eq!(
            filter.web_url.as_deref(),
            Some("https://jira.example/issues/?filter=10300")
        );
        assert_eq!(filter.payload["kind"], "filter");
        assert_eq!(filter.payload["id"], "10300");
        assert_eq!(
            filter.payload["jql"],
            "project = PAY AND fixVersion is EMPTY ORDER BY updated DESC"
        );
        assert_eq!(
            filter.payload["query"], filter.payload["jql"],
            "a filter is searched as Jira holds it, untranslated"
        );
        assert_eq!(
            filter.payload["keys"],
            json!(["PAY-240", "WEB-9", "PAY-231"])
        );
        assert_eq!(filter.payload["columns"], json!([]));
        assert_eq!(filter.payload["sprint"], Value::Null);
    }

    #[tokio::test]
    async fn favourite_filters_turned_off_are_delivered_as_gone_at_the_next_sync() {
        let api = with_favourites();
        let (_, first) = sync(&api, None).await;
        let (items, _) = sync_with(&api, &cfg_with("synced", false), Some(first)).await;
        assert_eq!(
            keys_of(&items),
            [("filter-122", true), ("filter-10300", true)]
        );
    }

    #[tokio::test]
    async fn every_board_i_can_see_adds_the_boards_on_projects_this_source_does_not_sync() {
        let api = with_favourites();
        let (items, _) = sync_with(&api, &cfg_with("all", false), None).await;
        assert_eq!(
            keys_of(&items),
            [
                ("board-21", false),
                ("board-22", false),
                ("board-31", false)
            ]
        );
    }

    #[tokio::test]
    async fn favourite_boards_are_the_boards_whose_filter_is_a_favourite_filter() {
        let api = with_favourites();
        let (items, _) = sync_with(&api, &cfg_with("favourites", false), None).await;
        assert_eq!(keys_of(&items), [("board-22", false)]);
    }

    #[tokio::test]
    async fn no_boards_brings_none_in_and_takes_the_ones_there_were_away() {
        let api = with_favourites();
        let (_, first) = sync(&api, None).await;
        let (items, _) = sync_with(&api, &cfg_with("none", true), Some(first)).await;
        assert_eq!(keys_of(&items), [("board-21", true), ("board-22", true)]);
    }

    /// A favourite filter Jira can no longer run (a field deleted since it
    /// was saved) is left out, rather than failing every sync of the source.
    #[tokio::test]
    async fn a_favourite_filter_jira_refuses_to_run_is_left_out() {
        let api = Agile {
            refused: Some("project = PAY AND fixVersion is EMPTY ORDER BY updated DESC"),
            ..with_favourites()
        };
        let (items, _) = sync(&api, None).await;
        let keys: Vec<&str> = items.iter().map(|i| i.entity.key.as_str()).collect();
        assert_eq!(keys, ["board-21", "board-22", "filter-122"]);
    }

    /// A board whose saved filter Jira can no longer run (a field deleted
    /// since it was saved) is left out, rather than failing every sync.
    #[tokio::test]
    async fn a_board_jira_refuses_to_run_is_left_out() {
        let api = Agile {
            refused: Some("(project in (PAY, WEB)) AND (fixVersion is EMPTY) ORDER BY Rank ASC"),
            ..with_favourites()
        };
        let (items, _) = sync(&api, None).await;
        let keys: Vec<&str> = items.iter().map(|i| i.entity.key.as_str()).collect();
        assert_eq!(keys, ["board-21", "filter-122", "filter-10300"]);
    }

    /// Sixty boards on PAY, more than one listing page: board `100 + n` is
    /// named `board nn`, so Jira lists them in id order.
    fn many() -> Vec<FakeBoard> {
        (1..=60_u64)
            .map(|n| {
                let leak = |text: String| -> &'static str { Box::leak(text.into_boxed_str()) };
                (
                    100 + n,
                    leak(format!("board {n:02}")),
                    "kanban",
                    leak(format!("project = PAY AND labels = b{n} ORDER BY Rank ASC")),
                    None,
                    columns(None, "none"),
                    vec!["PAY-1"],
                )
            })
            .collect()
    }

    /// A run over `then` after a first run over `before`, an hour on, so the
    /// second reads the boards: what it delivers, as `(key, gone)`. PAY
    /// alone, so one listing: the fake lists every board on each project.
    async fn read_again(before: Vec<FakeBoard>, then: Vec<FakeBoard>) -> Vec<(String, bool)> {
        let pay = crate::JiraConfig::from_json(&json!({ "projects": ["PAY"] })).unwrap();
        let (_, first) = sync_with(
            &Agile {
                boards: before.clone(),
                ..Agile::default()
            },
            &pay,
            None,
        )
        .await;
        let api = Agile {
            boards: before,
            then: Some(then),
            clock_minutes: 120,
            ..Agile::default()
        };
        let (items, _) = sync_with(&api, &pay, Some(first)).await;
        items
            .iter()
            .map(|i| (i.entity.key.clone(), i.deleted))
            .collect()
    }

    /// A board deleted from a page already read shifts the first board of
    /// the next page onto it (#2003): no page of that listing holds it, and
    /// the run must not deliver it as gone.
    #[tokio::test]
    async fn a_board_deleted_while_the_listing_is_read_hides_no_other() {
        let before = many();
        let mut then = before.clone();
        then.retain(|b| b.0 != 110);
        assert_eq!(
            read_again(before, then).await,
            [("board-110".to_owned(), true)],
            "the deleted board goes, and every other board stays"
        );
    }

    /// A rename moves a row in Jira's name order (#2003): a board renamed
    /// from an unread page to one already read is on no page of that listing.
    #[tokio::test]
    async fn a_board_renamed_while_the_listing_is_read_hides_no_other() {
        let before = many();
        let mut then = before.clone();
        let renamed = then.iter_mut().find(|b| b.0 == 155).unwrap();
        renamed.1 = "board 00";
        let delivered = read_again(before, then).await;
        assert!(
            delivered.iter().all(|(_, gone)| !gone),
            "nothing went: {delivered:?}"
        );
        assert_eq!(
            delivered,
            [("board-155".to_owned(), false)],
            "the renamed board, delivered under its new name"
        );
    }

    /// Jira Data Center's search maximum (#1700): the keys are all a view's
    /// page carries, so a board is walked in few requests.
    #[tokio::test]
    async fn the_keys_are_asked_for_at_jiras_page_maximum() {
        let api = with_favourites();
        sync(&api, None).await;
        let pages = api.key_pages.lock().unwrap();
        assert!(!pages.is_empty());
        assert!(pages.iter().all(|&max| max == 1000), "{pages:?}");
    }
}
