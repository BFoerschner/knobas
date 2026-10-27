//! Ticket views (#812, spec #808 "Ticket views", R-3.2, R-3.3): the
//! knobas-owned `knobas.ticket_view` rows (migration `0035`) behind three
//! commands on `commands::search`, `ticket_views`, `save_ticket_view` and
//! `delete_ticket_view`.
//!
//! **The stored text is the source of truth.** `query` is written exactly as
//! the caller sent it and read back exactly as stored; nothing here parses it
//! into anything that is stored. Reading a view asks today's grammar what the
//! text means ([`knobas_search::Searcher::readings`]): a count, or the part it
//! could not read, which the Tickets screen shows as text with that line.
//!
//! **No cap.** A view is not a smart list (D6), so `MAX_SAVED_LISTS` does not
//! apply and nothing counts the table before a save.
//!
//! The display words (`lanes`, `card_fields`, `list_columns`, `list_sort`)
//! are the Tickets screen's own vocabulary and are checked here, not in the
//! migration, so a lane a later ticket adds needs a constant, not a schema
//! change.

use sqlx::{PgPool, Row};

use crate::IpcError;

/// The two layouts (D1): a ticket board or a list.
pub const LAYOUTS: &[&str] = &["board", "list"];
/// What a ticket board's lanes split by (R-3.5), as the Tickets screen names them.
pub const LANES: &[&str] = &["none", "epic", "assignee", "priority", "issuetype", "label"];
/// What a card can show (R-3.16, *Cards show*).
pub const CARD_FIELDS: &[&str] = &["type", "signals", "comments", "assignee", "epic", "labels"];
/// The list's columns (R-3.16, *List columns*); `key` is always one of them.
pub const LIST_COLUMNS: &[&str] = &[
    "key", "summary", "status", "priority", "signals", "assignee", "updated", "epic", "labels",
];
/// What a list header sorts by (R-3.6). Signals do not sort.
pub const SORT_COLUMNS: &[&str] = &[
    "key", "summary", "status", "priority", "assignee", "updated",
];
/// A sort's two directions.
pub const SORT_DIRECTIONS: &[&str] = &["ascending", "descending"];

use crate::slug::is_slug;

// The vocabularies above as the generated TypeScript's unions (#1329). The
// wire carries each as a string, checked against its list by the save; these
// enums are never built and only name the union a field is typed with.
// `the_vocabulary_unions_are_the_lists` pins each to its list.

/// [`LAYOUTS`]: a ticket board or a list. Inlined where it is used.
#[derive(ts_rs::TS)]
#[ts(export_to = "search.ts", rename_all = "snake_case")]
pub enum Layout {
    Board,
    List,
}

/// [`LANES`]: what a ticket board's lanes split by (R-3.5).
#[derive(ts_rs::TS)]
#[ts(
    export_to = "search.ts",
    rename = "ViewLanes",
    rename_all = "snake_case"
)]
pub enum Lanes {
    None,
    Epic,
    Assignee,
    Priority,
    Issuetype,
    Label,
}

/// [`CARD_FIELDS`]: what a card can show (R-3.16).
#[derive(ts_rs::TS)]
#[ts(export_to = "search.ts", rename_all = "snake_case")]
pub enum CardField {
    Type,
    Signals,
    Comments,
    Assignee,
    Epic,
    Labels,
}

/// [`LIST_COLUMNS`]: the list's columns (R-3.16); `key` is always one.
#[derive(ts_rs::TS)]
#[ts(export_to = "search.ts", rename_all = "snake_case")]
pub enum ListColumn {
    Key,
    Summary,
    Status,
    Priority,
    Signals,
    Assignee,
    Updated,
    Epic,
    Labels,
}

/// [`SORT_COLUMNS`]: what a list header sorts by (R-3.6).
#[derive(ts_rs::TS)]
#[ts(
    export_to = "search.ts",
    rename = "ViewSortColumn",
    rename_all = "snake_case"
)]
pub enum SortColumn {
    Key,
    Summary,
    Status,
    Priority,
    Assignee,
    Updated,
}

/// [`SORT_DIRECTIONS`]: a sort's two directions. Inlined where it is used.
#[derive(ts_rs::TS)]
#[ts(export_to = "search.ts", rename_all = "snake_case")]
pub enum SortDirection {
    Ascending,
    Descending,
}

/// [`SavedViewSource::kind`]: a board or a filter. Never built, like the
/// enums above; the list is the projection in `knobas_sync::saved_views`,
/// which writes `filter` for a filter and `board` for anything else.
/// Inlined where it is used.
#[derive(ts_rs::TS)]
#[ts(export_to = "search.ts", rename_all = "snake_case")]
pub enum SavedViewKind {
    Board,
    Filter,
}

/// [`SavedViewSource::sprint`] as the shell reads it: the adapter's payload
/// passed through as JSON, `{"id", "name", "end_date"}`. Never built; it
/// only types the field in the generated TypeScript. Inlined where it is used.
#[derive(ts_rs::TS)]
#[ts(export_to = "search.ts")]
pub struct ActiveSprint {
    pub id: i64,
    pub name: String,
    pub end_date: Option<String>,
}

/// One ticket board column: its name, the statuses on it, and an optional
/// *At most* (R-3.15).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
pub struct ViewColumn {
    pub name: String,
    pub statuses: Vec<String>,
    /// *At most*, or `None`.
    pub limit: Option<u32>,
}

/// The list's header sort (R-3.6).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
pub struct ListSort {
    #[ts(as = "SortColumn")]
    pub column: String,
    #[ts(as = "SortDirection", inline)]
    pub direction: String,
}

/// One ticket view as stored.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
pub struct TicketView {
    /// A slug: `#/tickets/<id>` opens the view.
    pub id: String,
    pub name: String,
    /// *Mine*, *Team*, *Epics*, *Projects* or a group somebody typed.
    pub group: String,
    /// Where the view sits in the list; lower first.
    pub position: i32,
    /// The conditions as search text, exactly as saved; empty is every ticket.
    pub query: String,
    /// `board` or `list`.
    #[ts(as = "Layout", inline)]
    pub layout: String,
    /// The board's columns; empty is one column per status.
    pub columns: Vec<ViewColumn>,
    #[ts(as = "Lanes")]
    pub lanes: String,
    #[ts(as = "Vec<CardField>")]
    pub card_fields: Vec<String>,
    #[ts(as = "Vec<ListColumn>")]
    pub list_columns: Vec<String>,
    /// `None` is newest first.
    pub list_sort: Option<ListSort>,
    /// The saved view (a board or filter, `sync.saved_view`'s entity id) a
    /// view from a ticket system shows (#877; named for the role by #1245);
    /// `None` on a view somebody made.
    pub saved_view: Option<String>,
}

/// What `save_ticket_view` takes: a view, with the id and position left to
/// the save when it is a new one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
pub struct TicketViewDraft {
    /// `None` makes a new view with an id from its name; an id saves over that
    /// view, or puts a deleted one back (*Undo*).
    pub id: Option<String>,
    pub name: String,
    pub group: String,
    /// `None` keeps a saved view where it is and puts a new one last.
    pub position: Option<i32>,
    pub query: String,
    #[ts(as = "Layout", inline)]
    pub layout: String,
    pub columns: Vec<ViewColumn>,
    #[ts(as = "Lanes")]
    pub lanes: String,
    #[ts(as = "Vec<CardField>")]
    pub card_fields: Vec<String>,
    #[ts(as = "Vec<ListColumn>")]
    pub list_columns: Vec<String>,
    pub list_sort: Option<ListSort>,
    pub saved_view: Option<String>,
}

/// One row of the views pane: the view, and what its text reads as today.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
pub struct TicketViewRow {
    #[serde(flatten)]
    pub view: TicketView,
    /// How many tickets the text admits; `None` when the grammar could not
    /// read it.
    pub count: Option<u64>,
    /// The part of the text the grammar could not read, or `None`.
    pub unreadable: Option<String>,
    /// What the source says about the saved view a view shows (#877, #1273),
    /// or `None` on a view somebody made.
    pub saved_view_source: Option<SavedViewSource>,
}

/// What a ticket system says about one of its saved views (#877, #1273,
/// `sync.saved_view`): enough for the views pane and *View settings* to say
/// where it comes from.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
pub struct SavedViewSource {
    /// `board` or `filter`.
    #[ts(as = "SavedViewKind", inline)]
    pub kind: String,
    /// The ticket system it comes from, as its group names it (`Jira`, #2032):
    /// the kind's declared `system_name`, else the adapter's name, by
    /// `knobas_sync::saved_views::declared`; the source's display name when its
    /// adapter is not compiled in or declares no saved views. What the shell's
    /// words about the view name: *Open in Jira*, *Jira decides which tickets*.
    pub system: String,
    /// The board's type in the source's own word (Jira's `scrum`, `kanban`
    /// or `simple`), read at the kind's declared `board_type`; `None` for a
    /// filter or a kind that declares none.
    pub board_type: Option<String>,
    /// The saved view's query in the source's own language, as the source
    /// holds it (Jira's JQL); empty when its record carries none. Read from
    /// `sync.saved_view.source_query`, which the projection fills from the
    /// path the kind declares (`KindPaths::source_query`, #2050).
    pub source_query: String,
    /// A Kanban board's sub-filter, read at the declared `sub_query`, or `None`.
    pub sub_query: Option<String>,
    /// A Scrum board's active sprint, `{"id", "name", "end_date"}`, read at the
    /// declared `sprint`, or `None`.
    #[ts(as = "Option<ActiveSprint>", inline)]
    pub sprint: Option<serde_json::Value>,
    /// When the last sync read it (RFC 3339).
    pub last_read: chrono::DateTime<chrono::Utc>,
    /// Where the source shows it, or `None`.
    pub web_url: Option<String>,
    /// *Hide from the list* (#878, R-3.25): it keeps syncing, and the views
    /// pane draws it as one line at the bottom with *Show*.
    pub hidden: bool,
    /// Turned up after its source's first sync and not opened yet (#878,
    /// R-3.20).
    pub new: bool,
    /// The tickets the source put on it that knobas does not have, per project the
    /// source does not sync, most first (#879, R-3.23): the Tickets warn line
    /// and *Boards and filters*' "and 9 in WEB, not synced" (#878, R-13a.2).
    /// A missing ticket of a project the source syncs is a sync still to come.
    pub unsynced: Vec<Unsynced>,
}

/// How many tickets on a saved view are in one project the source does not
/// sync (#879, R-3.23).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "search.ts", rename = "UnsyncedTickets")]
pub struct Unsynced {
    /// The project key, read off the ticket key (`WEB-5` → `WEB`).
    pub project: String,
    pub count: u64,
}

/// The columns a view is read from, in [`view_of`]'s terms; a macro so the
/// statements stay `&'static str`.
macro_rules! fields {
    () => {
        "id, name, view_group, position, query, layout, columns, lanes, card_fields, \
         list_columns, list_sort, saved_view"
    };
}

/// A value bound as `jsonb`; a value that cannot be one is `internal`.
fn json<T: serde::Serialize>(value: &T) -> Result<serde_json::Value, IpcError> {
    serde_json::to_value(value).map_err(IpcError::internal)
}

fn view_of(row: &sqlx::postgres::PgRow) -> Result<TicketView, IpcError> {
    let stored = |column: &str| -> serde_json::Value { row.get(column) };
    Ok(TicketView {
        id: row.get("id"),
        name: row.get("name"),
        group: row.get("view_group"),
        position: row.get("position"),
        query: row.get("query"),
        layout: row.get("layout"),
        columns: serde_json::from_value(stored("columns")).map_err(IpcError::internal)?,
        lanes: row.get("lanes"),
        card_fields: serde_json::from_value(stored("card_fields")).map_err(IpcError::internal)?,
        list_columns: serde_json::from_value(stored("list_columns")).map_err(IpcError::internal)?,
        list_sort: row
            .get::<Option<serde_json::Value>, _>("list_sort")
            .map(serde_json::from_value)
            .transpose()
            .map_err(IpcError::internal)?,
        saved_view: row.get("saved_view"),
    })
}

/// Every ticket view in list order, each with its count or the part its text
/// the grammar could not read.
///
/// # Errors
///
/// `internal` if a read fails.
pub async fn list(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
) -> Result<Vec<TicketViewRow>, IpcError> {
    // A Jira view whose item is not live is not listed: its source is turned
    // off, and it comes back with the source, or its source was deleted with
    // the mirror purged, which leaves the entity tombstoned and no
    // `sync.saved_view` row, and no later sync of that source to drop this one.
    // Read off the entity rather than `sync.saved_view` for that second case.
    let rows = sqlx::query(concat!(
        "select ",
        fields!(),
        " from knobas.ticket_view t
          where not exists (select 1 from knobas.entity e
                             where e.id = t.saved_view
                               and not exists (select 1 from sync.live_item i
                                                where i.entity_id = e.id))
          order by position, id"
    ))
    .fetch_all(pool)
    .await?;
    let views = rows.iter().map(view_of).collect::<Result<Vec<_>, _>>()?;
    let mut sources = saved_view_sources(pool, registry, &views).await?;
    let texts: Vec<&str> = views.iter().map(|view| view.query.as_str()).collect();
    let readings = crate::commands::search::searcher(pool, registry)
        .await?
        .readings(&texts, "ticket")
        .await?;
    Ok(views
        .into_iter()
        .zip(readings)
        .map(|(view, reading)| {
            // A saved view counts what its source put on it that knobas
            // mirrors, which is what its board draws (#877); its text is not
            // its own.
            let source = view.saved_view.as_ref().and_then(|r| sources.remove(r));
            let reading = match &source {
                Some((count, _)) => knobas_search::Reading::Count(*count),
                None => reading,
            };
            let saved_view_source = source.map(|(_, source)| source);
            match reading {
                knobas_search::Reading::Count(count) => TicketViewRow {
                    view,
                    count: Some(count),
                    unreadable: None,
                    saved_view_source,
                },
                knobas_search::Reading::Unreadable(part) => TicketViewRow {
                    view,
                    count: None,
                    unreadable: Some(part),
                    saved_view_source,
                },
            }
        })
        .collect())
}

/// What [`saved_view_sources`] reads, per saved view among `$1`.
///
/// Every member of the views is read once, live by
/// [`knobas_core::live_by_exclusion!`] over `sync.item` rather than through
/// `sync.live_item` (#2193): on tables never analyzed the view's join probed
/// `knobas.entity` once per member, about 700 ms for #2182's 4,500 tickets on
/// three views. `view_projects` is what is left of them, one row per view,
/// project and liveness, which `n` and `unsynced` read per view.
///
/// `unsynced` reads the member's entity id, `<source>:<KEY>-<n>`: the
/// project is what sits between the source's colon and the last hyphen.
/// ADR-0007's direction: an id of any other shape is a miss, never a
/// guess, and is left out of the count.
const SAVED_VIEW_SOURCES: &str = concat!(
    "with view_projects as (
       select m.view_id, substring(m.entity_id from ':([^:]+)-[^-]*$') as project,
              t.entity_id is not null as live, count(*) as n
         from sync.saved_view_member m
         left join sync.item t on t.entity_id = m.entity_id and t.kind = 'ticket'
                              and ",
    knobas_core::live_by_exclusion!("t"),
    "
        where m.view_id = any($1)
        group by 1, 2, 3)
     select v.entity_id, v.ref_kind, v.board_type, v.source_query, v.sub_query, v.sprint,
            v.last_read, i.web_url, tv.hidden, tv.is_new, c.kind as adapter_kind,
            coalesce(nullif(btrim(c.display_name), ''), v.source_id) as source_name,
            (select coalesce(sum(p.n), 0)::bigint from view_projects p
              where p.view_id = v.entity_id and p.live) as n,
            -- R-3.23: the keys Jira returned minus the tickets knobas
            -- has, per project; a project the source syncs is left
            -- out, as a sync still to come rather than one not made.
            -- The Jira adapter's config (`JiraConfig::projects`) is read
            -- here and nowhere else in this crate. Only a
            -- source that names its projects leaves any out: an empty
            -- `projects` syncs everything the credential sees, and a
            -- `jql_filter` source has none (the two are exclusive), so
            -- adding a project there would narrow it or be refused.
            -- Failure direction (ADR-0007): a `projects` that is missing,
            -- empty or not an array counts nothing -- a warn line lost,
            -- never a *Sync WEB too* that narrows the source.
            (select coalesce(jsonb_agg(jsonb_build_object('project', u.project, 'count', u.n)
                                       order by u.n desc, u.project), '[]'::jsonb)
               from view_projects u
              where u.view_id = v.entity_id and not u.live
                and u.project is not null
                and exists (
                      select 1 from knobas.source_config c
                       where c.id = v.source_id
                         and jsonb_typeof(c.config->'projects') = 'array'
                         and jsonb_array_length(c.config->'projects') > 0)
                and not exists (
                      select 1 from knobas.source_config c
                       cross join lateral jsonb_array_elements_text(
                             case when jsonb_typeof(c.config->'projects') = 'array'
                                  then c.config->'projects' else '[]'::jsonb end) p(key)
                       where c.id = v.source_id and upper(p.key) = u.project)) as unsynced
       from sync.saved_view v
       join sync.item i on i.entity_id = v.entity_id
       join knobas.ticket_view tv on tv.saved_view = v.entity_id
       left join knobas.source_config c on c.id = v.source_id
      where v.entity_id = any($1)"
);

/// [`SAVED_VIEW_SOURCES`], for the plan test (`tests/it/read_plans.rs`,
/// #2193). It binds what `saved_view_sources` binds: the saved views' ids.
#[doc(hidden)]
#[must_use]
pub fn planned_saved_view_sources() -> &'static str {
    SAVED_VIEW_SOURCES
}

/// Per saved view among `views`: how many of the tickets its source put on it
/// the mirror holds live, and where it comes from (#877). A view whose
/// `saved_view` names no saved view is absent, and is counted by its text like
/// any other.
async fn saved_view_sources(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    views: &[TicketView],
) -> Result<std::collections::HashMap<String, (u64, SavedViewSource)>, IpcError> {
    let refs: Vec<&str> = views
        .iter()
        .filter_map(|view| view.saved_view.as_deref())
        .collect();
    if refs.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let rows = sqlx::query(SAVED_VIEW_SOURCES)
        .bind(&refs)
        .fetch_all(pool)
        .await?;
    // Once, not once per row: `descriptors()` builds every template.
    let templates = registry.descriptors();
    let system = |adapter_kind: Option<String>, source_name: String| {
        templates
            .iter()
            .find(|t| Some(&t.adapter_kind) == adapter_kind.as_ref())
            .and_then(knobas_sync::saved_views::declared)
            .map_or(source_name, |declared| declared.source_name().to_owned())
    };
    rows.iter()
        .map(|row| {
            let n: i64 = row.get("n");
            let source = SavedViewSource {
                kind: row.get("ref_kind"),
                system: system(row.get("adapter_kind"), row.get("source_name")),
                board_type: row.get("board_type"),
                source_query: row.get("source_query"),
                sub_query: row.get("sub_query"),
                sprint: row.get("sprint"),
                last_read: row.get("last_read"),
                web_url: row.get("web_url"),
                hidden: row.get("hidden"),
                new: row.get("is_new"),
                unsynced: serde_json::from_value(row.get("unsynced"))
                    .map_err(IpcError::internal)?,
            };
            Ok((
                row.get("entity_id"),
                (u64::try_from(n).unwrap_or(0), source),
            ))
        })
        .collect()
}

/// *Make an editable copy* (#1208): the saved view `id`'s query, read into
/// the search grammar by its source's adapter (`Source::translate_query`).
/// `None` when the adapter translates no query: that source offers no copy.
///
/// # Errors
///
/// `not_found` for no view, or a saved view whose item is not live;
/// `invalid` for a view somebody made; the source's own failure as
/// [`crate::sources::to_ipc`] maps it; `internal` if a read fails.
pub async fn translate(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &std::sync::Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    id: &str,
) -> Result<Option<knobas_source::query::Translation>, IpcError> {
    let row: Option<(Option<String>, Option<serde_json::Value>)> = sqlx::query_as(
        "select tv.saved_view, i.payload
           from knobas.ticket_view tv
           left join sync.live_item i on i.entity_id = tv.saved_view
          where tv.id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let Some((saved, payload)) = row else {
        return Err(IpcError::not_found(format!(
            "no ticket view is called {id}"
        )));
    };
    let Some(saved) = saved else {
        return Err(IpcError::invalid(format!(
            "{id} is a view of your own: only a saved view of a source has a query to translate"
        )));
    };
    let Some(payload) = payload else {
        return Err(IpcError::not_found(format!("{saved} is not synced")));
    };
    let namespace = knobas_core::entity::EntityRef::parse(&saved)
        .map(|entity| entity.namespace)
        .map_err(IpcError::invalid)?;
    crate::sources::crud::translate_query(pool, secrets, registry, &saved, &payload)
        .await
        .map_err(|error| crate::sources::to_ipc(&error, Some(&namespace)))
}

/// *Hide from the list* or *Show* a Jira view (#878, R-3.25). It keeps
/// syncing either way. One statement writes it, and only on a Jira view;
/// when it writes nothing, a second read says why.
///
/// # Errors
///
/// `not_found` for no view, `invalid` for a view somebody made, `internal`
/// if the write fails.
pub async fn hide(pool: &PgPool, id: &str, hidden: bool) -> Result<(), IpcError> {
    let written = sqlx::query(
        "update knobas.ticket_view set hidden = $2
          where id = $1 and saved_view in (select entity_id from sync.saved_view)",
    )
    .bind(id)
    .bind(hidden)
    .execute(pool)
    .await?
    .rows_affected();
    if written > 0 {
        return Ok(());
    }
    let exists: bool =
        sqlx::query_scalar("select exists (select 1 from knobas.ticket_view where id = $1)")
            .bind(id)
            .fetch_one(pool)
            .await?;
    Err(if exists {
        IpcError::invalid(format!(
            "{id} is not a Jira view: only a Jira view hides from the list; delete your own"
        ))
    } else {
        IpcError::not_found(format!("no ticket view is called {id}"))
    })
}

/// A view was opened: a Jira view that said *new* stops saying it (#878,
/// R-3.20). Opening any other view, or one opened before, changes nothing.
///
/// # Errors
///
/// `not_found` for no view, `internal` if the write fails.
pub async fn open(pool: &PgPool, id: &str) -> Result<(), IpcError> {
    let found: Option<bool> = sqlx::query_scalar(
        "with opened as (update knobas.ticket_view set is_new = false
                          where id = $1 and is_new returning 1)
         select true from knobas.ticket_view where id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    if found.is_none() {
        return Err(IpcError::not_found(format!(
            "no ticket view is called {id}"
        )));
    }
    Ok(())
}

/// The view `id` names if it shows a saved view, such as a Jira view (#877,
/// #1245), as stored.
async fn saved_view_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: &str,
) -> Result<Option<TicketView>, IpcError> {
    let row = sqlx::query(concat!(
        "select ",
        fields!(),
        " from knobas.ticket_view
          where id = $1 and saved_view in (select entity_id from sync.saved_view)"
    ))
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?;
    row.as_ref().map(view_of).transpose()
}

/// Why a draft cannot be saved, or `None`. The text is never a reason: it is
/// stored as it came, and a text the grammar cannot read is a state a view can
/// be in.
fn refusal(draft: &TicketViewDraft) -> Option<String> {
    let one_of = |what: &str, value: &str, allowed: &[&str]| {
        (!allowed.contains(&value))
            .then(|| format!("{what} `{value}` is not one of {}", allowed.join(", ")))
    };
    if draft.name.trim().is_empty() {
        return Some("a view needs a name".to_owned());
    }
    if draft.group.trim().is_empty() {
        return Some("a view needs a group".to_owned());
    }
    if let Some(id) = &draft.id
        && !is_slug(id)
    {
        return Some(format!("`{id}` is not a view id"));
    }
    if let Some(why) = one_of("layout", &draft.layout, LAYOUTS) {
        return Some(why);
    }
    if let Some(why) = one_of("lanes", &draft.lanes, LANES) {
        return Some(why);
    }
    for (what, words, allowed) in [
        ("card field", &draft.card_fields, CARD_FIELDS),
        ("list column", &draft.list_columns, LIST_COLUMNS),
    ] {
        for (at, word) in words.iter().enumerate() {
            if let Some(why) = one_of(what, word, allowed) {
                return Some(why);
            }
            if words[..at].contains(word) {
                return Some(format!("{what} `{word}` is named twice"));
            }
        }
    }
    if !draft.list_columns.iter().any(|column| column == "key") {
        return Some("the list always shows Key".to_owned());
    }
    let mut placed: Vec<&str> = Vec::new();
    for column in &draft.columns {
        if column.name.trim().is_empty() {
            return Some("a column needs a name".to_owned());
        }
        if column.limit == Some(0) {
            return Some(format!("column `{}` has a limit of 0", column.name));
        }
        for status in &column.statuses {
            if placed.contains(&status.as_str()) {
                return Some(format!("status `{status}` sits on two columns"));
            }
            placed.push(status);
        }
    }
    if let Some(sort) = &draft.list_sort {
        if let Some(why) = one_of("sort column", &sort.column, SORT_COLUMNS) {
            return Some(why);
        }
        if let Some(why) = one_of("sort direction", &sort.direction, SORT_DIRECTIONS) {
            return Some(why);
        }
    }
    None
}

/// A view name as an address: `view` for a name with no letter or digit.
fn slug(name: &str) -> String {
    crate::slug::slug(name, "view")
}

/// Save a view: a new one under an id from its name, or over the view the
/// draft names (putting it back if it was deleted), and answer it as stored.
///
/// # Errors
///
/// `invalid` with the reason for a draft [`refusal`] names; `internal` if the
/// write fails.
pub async fn save(pool: &PgPool, mut draft: TicketViewDraft) -> Result<TicketView, IpcError> {
    if let Some(why) = refusal(&draft) {
        return Err(IpcError::invalid(why));
    }
    let mut tx = pool.begin().await?;
    // One writer at a time picks ids and positions, so two saves cannot take
    // the same slug or the same last place.
    sqlx::query("lock table knobas.ticket_view in share row exclusive mode")
        .execute(&mut *tx)
        .await?;
    // *View settings* on a Jira view (#877, R-3.24): the layout, lanes, card
    // fields, list columns and sort are the reader's; the name, group, which
    // tickets and the columns are Jira's, so the stored ones stand whatever
    // the draft says.
    if let Some(id) = &draft.id
        && let Some(stored) = saved_view_row(&mut tx, id).await?
    {
        draft.name = stored.name;
        draft.group = stored.group;
        draft.query = stored.query;
        draft.columns = stored.columns;
        draft.saved_view = stored.saved_view;
    }
    let id = match &draft.id {
        Some(id) => id.clone(),
        None => {
            let base = slug(&draft.name);
            let taken: Vec<String> =
                sqlx::query_scalar("select id from knobas.ticket_view where id = $1 or id like $2")
                    .bind(&base)
                    .bind(format!("{base}-%"))
                    .fetch_all(&mut *tx)
                    .await?;
            crate::slug::first_free(&base, &taken)
        }
    };
    let last: i32 =
        sqlx::query_scalar("select coalesce(max(position) + 1, 0) from knobas.ticket_view")
            .fetch_one(&mut *tx)
            .await?;
    let row = sqlx::query(concat!(
        "insert into knobas.ticket_view as v (id, name, view_group, position, query, layout, columns, \
                                              lanes, card_fields, list_columns, list_sort, saved_view)
         values ($1, $2, $3, coalesce($4, $5), $6, $7, $8, $9, $10, $11, $12, $13)
         on conflict (id) do update set
           name = excluded.name, view_group = excluded.view_group,
           position = coalesce($4, v.position), query = excluded.query, layout = excluded.layout,
           columns = excluded.columns, lanes = excluded.lanes, card_fields = excluded.card_fields,
           list_columns = excluded.list_columns, list_sort = excluded.list_sort,
           saved_view = excluded.saved_view, updated_at = now()
         returning ",
        fields!()
    ))
    .bind(&id)
    .bind(draft.name.trim())
    .bind(draft.group.trim())
    .bind(draft.position)
    .bind(last)
    .bind(&draft.query)
    .bind(&draft.layout)
    .bind(json(&draft.columns)?)
    .bind(&draft.lanes)
    .bind(json(&draft.card_fields)?)
    .bind(json(&draft.list_columns)?)
    .bind(draft.list_sort.as_ref().map(json).transpose()?)
    .bind(&draft.saved_view)
    .fetch_one(&mut *tx)
    .await?;
    let view = view_of(&row)?;
    tx.commit().await?;
    Ok(view)
}

/// Forget a view. Its *Undo* is a [`save`] of the view it answered.
///
/// # Errors
///
/// `not_found` if no view has that id; `invalid` for a Jira view (#877);
/// `internal` if the delete fails.
pub async fn delete(pool: &PgPool, id: &str) -> Result<(), IpcError> {
    // A Jira view is Jira's: deleted here, the next sync would put it back.
    let jira: bool = sqlx::query_scalar(
        "select exists (select 1 from knobas.ticket_view
                         where id = $1 and saved_view in (select entity_id from sync.saved_view))",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;
    if jira {
        return Err(IpcError::invalid(format!(
            "{id} is a Jira view: it comes from Jira at every sync and cannot be deleted here"
        )));
    }
    let gone = sqlx::query("delete from knobas.ticket_view where id = $1")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
    if gone == 0 {
        return Err(IpcError::not_found(format!(
            "no ticket view is called {id}"
        )));
    }
    Ok(())
}

/// A saved view as its board draws it (#877, #1245, R-3.21, R-3.24): the
/// view's own columns and the tickets the source put on it
/// (`sync.saved_view_member`), in the source's order.
#[derive(Debug, Clone)]
pub struct SavedViewLayout {
    pub columns: Vec<ViewColumn>,
    /// Entity ids, the source's order. Keys the source returned that knobas
    /// does not mirror stay here and are never drawn.
    pub members: Vec<String>,
}

/// The layout of saved view `view`, or `None` for one the mirror does not
/// hold.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) for a read failure, or
/// stored columns that are not a column list.
pub async fn saved_view_layout(
    pool: &PgPool,
    view: &str,
) -> Result<Option<SavedViewLayout>, IpcError> {
    let columns: Option<serde_json::Value> =
        sqlx::query_scalar("select columns from sync.saved_view where entity_id = $1")
            .bind(view)
            .fetch_optional(pool)
            .await?;
    let Some(columns) = columns else {
        return Ok(None);
    };
    let columns: Vec<ViewColumn> = serde_json::from_value(columns).map_err(IpcError::internal)?;
    let members: Vec<String> = sqlx::query_scalar(
        "select entity_id from sync.saved_view_member where view_id = $1 order by rank",
    )
    .bind(view)
    .fetch_all(pool)
    .await?;
    Ok(Some(SavedViewLayout { columns, members }))
}

impl SavedViewLayout {
    /// `by_status` -- the view's tickets, one column per status, as the mini
    /// board groups them -- under this view's columns, **in the source's
    /// order**: each column holds the tickets whose status is one of its
    /// statuses, empty ones included, as on the source's board. A status on
    /// no column does not show. A view with no columns keeps one column per
    /// status. A column's `status` is the view column's name. `per_column`
    /// cuts each column after its first tickets in that order; the `u32`
    /// counts them all. Generic over what a ticket is, so the board can cut
    /// the ids before it reads their cards (Björn, 2026-10-26).
    pub fn arrange<T>(
        &self,
        by_status: Vec<(Option<String>, Vec<T>)>,
        id: impl Fn(&T) -> &str,
        per_column: Option<u32>,
    ) -> Vec<(Option<String>, Vec<T>, u32)> {
        let rank: std::collections::HashMap<&str, usize> = self
            .members
            .iter()
            .enumerate()
            .map(|(at, id)| (id.as_str(), at))
            .collect();
        let in_order = |mut tickets: Vec<T>| -> (Vec<T>, u32) {
            tickets.sort_by_key(|ticket| rank.get(id(ticket)).copied().unwrap_or(usize::MAX));
            let total = u32::try_from(tickets.len()).unwrap_or(u32::MAX);
            if let Some(cap) = per_column {
                tickets.truncate(cap as usize);
            }
            (tickets, total)
        };
        if self.columns.is_empty() {
            return by_status
                .into_iter()
                .map(|(status, tickets)| {
                    let (tickets, total) = in_order(tickets);
                    (status, tickets, total)
                })
                .collect();
        }
        let mut by_status: std::collections::HashMap<Option<String>, Vec<T>> =
            by_status.into_iter().collect();
        self.columns
            .iter()
            .map(|column| {
                let (tickets, total) = in_order(
                    column
                        .statuses
                        .iter()
                        .flat_map(|status| {
                            by_status.remove(&Some(status.clone())).unwrap_or_default()
                        })
                        .collect(),
                );
                (Some(column.name.clone()), tickets, total)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slug::MAX_ID_CHARS;

    fn draft() -> TicketViewDraft {
        TicketViewDraft {
            id: None,
            name: "My work".to_owned(),
            group: "Mine".to_owned(),
            position: None,
            query: "assignee:@me".to_owned(),
            layout: "board".to_owned(),
            columns: Vec::new(),
            lanes: "none".to_owned(),
            card_fields: vec!["type".to_owned(), "assignee".to_owned()],
            list_columns: vec!["key".to_owned(), "summary".to_owned()],
            list_sort: None,
            saved_view: None,
        }
    }

    #[test]
    fn a_slug_is_an_address_whatever_the_name_was() {
        for (name, expected) in [
            ("My work", "my-work"),
            ("  Payments   backlog ", "payments-backlog"),
            ("Mine with a failing build!", "mine-with-a-failing-build"),
            ("...", "view"),
            ("\u{4e2d}\u{6587}", "view"),
        ] {
            let made = slug(name);
            assert_eq!(made, expected, "{name:?}");
            assert!(is_slug(&made), "{made:?}");
        }
        assert!(slug(&"a".repeat(200)).len() <= MAX_ID_CHARS);
    }

    /// The text is never a reason to refuse: it is what the reader saved.
    #[test]
    fn any_text_saves_and_only_the_display_words_are_checked() {
        assert_eq!(refusal(&draft()), None);
        assert_eq!(
            refusal(&TicketViewDraft {
                query: "sprint:next nonsense:: ".to_owned(),
                ..draft()
            }),
            None
        );
        for (bad, says) in [
            (
                TicketViewDraft {
                    name: " ".to_owned(),
                    ..draft()
                },
                "name",
            ),
            (
                TicketViewDraft {
                    group: String::new(),
                    ..draft()
                },
                "group",
            ),
            (
                TicketViewDraft {
                    layout: "columns".to_owned(),
                    ..draft()
                },
                "layout",
            ),
            (
                TicketViewDraft {
                    lanes: "who".to_owned(),
                    ..draft()
                },
                "lanes",
            ),
            (
                TicketViewDraft {
                    card_fields: vec!["who".to_owned()],
                    ..draft()
                },
                "card field",
            ),
            (
                TicketViewDraft {
                    list_columns: vec!["summary".to_owned()],
                    ..draft()
                },
                "Key",
            ),
            (
                TicketViewDraft {
                    list_columns: vec!["key".to_owned(), "key".to_owned()],
                    ..draft()
                },
                "twice",
            ),
            (
                TicketViewDraft {
                    columns: vec![
                        ViewColumn {
                            name: "Doing".to_owned(),
                            statuses: vec!["In Progress".to_owned()],
                            limit: None,
                        },
                        ViewColumn {
                            name: "Also".to_owned(),
                            statuses: vec!["In Progress".to_owned()],
                            limit: Some(3),
                        },
                    ],
                    ..draft()
                },
                "two columns",
            ),
            (
                TicketViewDraft {
                    list_sort: Some(ListSort {
                        column: "signals".to_owned(),
                        direction: "ascending".to_owned(),
                    }),
                    ..draft()
                },
                "sort column",
            ),
            (
                TicketViewDraft {
                    id: Some("Not A Slug".to_owned()),
                    ..draft()
                },
                "view id",
            ),
        ] {
            let why = refusal(&bad).unwrap_or_else(|| panic!("{bad:?} was not refused"));
            assert!(why.contains(says), "{why:?} does not say {says:?}");
        }
    }

    /// The barrel (`app/src/lib/ipc/search.ts`) invokes every `search` command
    /// by its registered name, the two #1245 renamed included, and invokes
    /// nothing the handler list lacks: a command is addressed by a string, so
    /// a barrel line left on an old name is a button Tauri answers with
    /// "command not found". Only `invoke<..>("name"` call sites count, so a
    /// doc comment naming a command stands in for nothing (#1984).
    #[test]
    fn the_view_commands_are_invoked_by_their_names() {
        let mirror = include_str!("../../../app/src/lib/ipc/search.ts");
        crate::commands::mirror::assert_mirrors("search", mirror, "app/src/lib/ipc/search.ts", 13);
    }

    /// Each union the generated TypeScript declares for a display vocabulary
    /// (#1329) is exactly its list: a word added to the list alone would
    /// reach the shell as one its types do not have, and one added to the
    /// enum alone would be offered and then refused by the save.
    #[test]
    fn the_vocabulary_unions_are_the_lists() {
        let union = |words: &[&str]| {
            words
                .iter()
                .map(|word| format!("\"{word}\""))
                .collect::<Vec<_>>()
                .join(" | ")
        };
        let cfg = ts_rs::Config::new();
        for (generated, list) in [
            (<Layout as ts_rs::TS>::inline(&cfg), LAYOUTS),
            (<Lanes as ts_rs::TS>::inline(&cfg), LANES),
            (<CardField as ts_rs::TS>::inline(&cfg), CARD_FIELDS),
            (<ListColumn as ts_rs::TS>::inline(&cfg), LIST_COLUMNS),
            (<SortColumn as ts_rs::TS>::inline(&cfg), SORT_COLUMNS),
            (<SortDirection as ts_rs::TS>::inline(&cfg), SORT_DIRECTIONS),
        ] {
            assert_eq!(generated, union(list));
        }
    }
}
