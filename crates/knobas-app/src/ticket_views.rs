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

/// One ticket board column: its name, the statuses on it, and an optional
/// *At most* (R-3.15).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ViewColumn {
    pub name: String,
    pub statuses: Vec<String>,
    pub limit: Option<u32>,
}

/// The list's header sort (R-3.6).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ListSort {
    pub column: String,
    pub direction: String,
}

/// One ticket view as stored.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    pub layout: String,
    /// The board's columns; empty is one column per status.
    pub columns: Vec<ViewColumn>,
    pub lanes: String,
    pub card_fields: Vec<String>,
    pub list_columns: Vec<String>,
    /// `None` is newest first.
    pub list_sort: Option<ListSort>,
    /// The Jira board or filter a Jira view shows (#877); `None` on a view
    /// somebody made.
    pub jira_ref: Option<String>,
}

/// What `save_ticket_view` takes: a view, with the id and position left to
/// the save when it is a new one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TicketViewDraft {
    /// `None` makes a new view with an id from its name; an id saves over that
    /// view, or puts a deleted one back (*Undo*).
    pub id: Option<String>,
    pub name: String,
    pub group: String,
    /// `None` keeps a saved view where it is and puts a new one last.
    pub position: Option<i32>,
    pub query: String,
    pub layout: String,
    pub columns: Vec<ViewColumn>,
    pub lanes: String,
    pub card_fields: Vec<String>,
    pub list_columns: Vec<String>,
    pub list_sort: Option<ListSort>,
    pub jira_ref: Option<String>,
}

/// One row of the views pane: the view, and what its text reads as today.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TicketViewRow {
    #[serde(flatten)]
    pub view: TicketView,
    /// How many tickets the text admits; `None` when the grammar could not
    /// read it.
    pub count: Option<u64>,
    /// The part of the text the grammar could not read, or `None`.
    pub unreadable: Option<String>,
    /// Where a Jira view comes from (#877), or `None` on a view somebody made.
    pub jira: Option<JiraSource>,
}

/// What Jira says about a Jira view (#877, `sync.jira_view`): enough for the
/// views pane and *View settings* to say where it comes from.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct JiraSource {
    /// `board` or `filter`.
    pub kind: String,
    /// `scrum`, `kanban` or `simple` for a board; `None` for a filter.
    pub board_type: Option<String>,
    /// The saved filter's JQL, as Jira holds it.
    pub jql: String,
    /// A Kanban board's sub-filter, or `None`.
    pub sub_query: Option<String>,
    /// A Scrum board's active sprint, `{"id", "name", "end_date"}`, or `None`.
    pub sprint: Option<serde_json::Value>,
    /// When the last sync read it (RFC 3339).
    pub last_read: chrono::DateTime<chrono::Utc>,
    /// Where Jira shows it, or `None`.
    pub web_url: Option<String>,
    /// *Hide from the list* (#878, R-3.25): it keeps syncing, and the views
    /// pane draws it as one line at the bottom with *Show*.
    pub hidden: bool,
    /// Turned up after its source's first sync and not opened yet (#878,
    /// R-3.20).
    pub new: bool,
    /// The tickets Jira put on it that knobas does not have, per project the
    /// source does not sync, most first (#879, R-3.23): the Tickets warn line
    /// and *Boards and filters*' "and 9 in WEB, not synced" (#878, R-13a.2).
    /// A missing ticket of a project the source syncs is a sync still to come.
    pub unsynced: Vec<Unsynced>,
}

/// How many tickets on a Jira view are in one project the source does not
/// sync (#879, R-3.23).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
         list_columns, list_sort, jira_ref"
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
        jira_ref: row.get("jira_ref"),
    })
}

/// Every ticket view in list order, each with its count or the part its text
/// the grammar could not read.
///
/// # Errors
///
/// `internal` if a read fails.
pub async fn list(pool: &PgPool) -> Result<Vec<TicketViewRow>, IpcError> {
    // A Jira view whose item is not live is not listed: its source is turned
    // off, and it comes back with the source, or its source was deleted with
    // the mirror purged, which leaves the entity tombstoned and no
    // `sync.jira_view` row, and no later sync of that source to drop this one.
    // Read off the entity rather than `sync.jira_view` for that second case.
    let rows = sqlx::query(concat!(
        "select ",
        fields!(),
        " from knobas.ticket_view t
          where not exists (select 1 from knobas.entity e
                             where e.id = t.jira_ref
                               and not exists (select 1 from sync.live_item i
                                                where i.entity_id = e.id))
          order by position, id"
    ))
    .fetch_all(pool)
    .await?;
    let views = rows.iter().map(view_of).collect::<Result<Vec<_>, _>>()?;
    let mut jira = jira_sources(pool, &views).await?;
    let texts: Vec<&str> = views.iter().map(|view| view.query.as_str()).collect();
    let readings = crate::commands::search::searcher(pool)
        .await?
        .readings(&texts, "ticket")
        .await?;
    Ok(views
        .into_iter()
        .zip(readings)
        .map(|(view, reading)| {
            // A Jira view counts what Jira put on it that knobas mirrors,
            // which is what its board draws (#877); its text is not its own.
            let source = view.jira_ref.as_ref().and_then(|r| jira.remove(r));
            let reading = match &source {
                Some((count, _)) => knobas_search::Reading::Count(*count),
                None => reading,
            };
            let jira = source.map(|(_, source)| source);
            match reading {
                knobas_search::Reading::Count(count) => TicketViewRow {
                    view,
                    count: Some(count),
                    unreadable: None,
                    jira,
                },
                knobas_search::Reading::Unreadable(part) => TicketViewRow {
                    view,
                    count: None,
                    unreadable: Some(part),
                    jira,
                },
            }
        })
        .collect())
}

/// Per Jira view among `views`: how many of the tickets Jira put on it the
/// mirror holds live, and where it comes from (#877). A view whose
/// `jira_ref` names no Jira view is absent, and is counted by its text like
/// any other.
async fn jira_sources(
    pool: &PgPool,
    views: &[TicketView],
) -> Result<std::collections::HashMap<String, (u64, JiraSource)>, IpcError> {
    let refs: Vec<&str> = views
        .iter()
        .filter_map(|view| view.jira_ref.as_deref())
        .collect();
    if refs.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    // `unsynced` reads the member's entity id, `<source>:<KEY>-<n>`: the
    // project is what sits between the source's colon and the last hyphen.
    // ADR-0007's direction: an id of any other shape is a miss, never a
    // guess, and is left out of the count.
    let rows = sqlx::query(
        "select v.entity_id, v.ref_kind, v.board_type, v.jql, v.sub_query, v.sprint,
                v.last_read, i.web_url, tv.hidden, tv.is_new,
                (select count(*) from sync.jira_view_member m
                   join sync.live_item t on t.entity_id = m.entity_id and t.kind = 'ticket'
                  where m.view_id = v.entity_id) as n,
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
                   from (select substring(m.entity_id from ':([^:]+)-[^-]*$') as project,
                                count(*) as n
                           from sync.jira_view_member m
                          where m.view_id = v.entity_id
                            and not exists (select 1 from sync.live_item l
                                             where l.entity_id = m.entity_id
                                               and l.kind = 'ticket')
                          group by 1) u
                  where u.project is not null
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
           from sync.jira_view v
           join sync.item i on i.entity_id = v.entity_id
           join knobas.ticket_view tv on tv.jira_ref = v.entity_id
          where v.entity_id = any($1)",
    )
    .bind(&refs)
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|row| {
            let n: i64 = row.get("n");
            let source = JiraSource {
                kind: row.get("ref_kind"),
                board_type: row.get("board_type"),
                jql: row.get("jql"),
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
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    id: &str,
) -> Result<Option<knobas_source::query::Translation>, IpcError> {
    let row: Option<(Option<String>, Option<serde_json::Value>)> = sqlx::query_as(
        "select tv.jira_ref, i.payload
           from knobas.ticket_view tv
           left join sync.live_item i on i.entity_id = tv.jira_ref
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
          where id = $1 and jira_ref in (select entity_id from sync.jira_view)",
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

/// The view `id` names if it is a Jira view (#877), as stored.
async fn jira_view_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: &str,
) -> Result<Option<TicketView>, IpcError> {
    let row = sqlx::query(concat!(
        "select ",
        fields!(),
        " from knobas.ticket_view
          where id = $1 and jira_ref in (select entity_id from sync.jira_view)"
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
        && let Some(jira) = jira_view_row(&mut tx, id).await?
    {
        draft.name = jira.name;
        draft.group = jira.group;
        draft.query = jira.query;
        draft.columns = jira.columns;
        draft.jira_ref = jira.jira_ref;
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
                                              lanes, card_fields, list_columns, list_sort, jira_ref)
         values ($1, $2, $3, coalesce($4, $5), $6, $7, $8, $9, $10, $11, $12, $13)
         on conflict (id) do update set
           name = excluded.name, view_group = excluded.view_group,
           position = coalesce($4, v.position), query = excluded.query, layout = excluded.layout,
           columns = excluded.columns, lanes = excluded.lanes, card_fields = excluded.card_fields,
           list_columns = excluded.list_columns, list_sort = excluded.list_sort,
           jira_ref = excluded.jira_ref, updated_at = now()
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
    .bind(&draft.jira_ref)
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
                         where id = $1 and jira_ref in (select entity_id from sync.jira_view))",
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
            jira_ref: None,
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

    /// The wire shape and its hand-written mirror (`app/src/lib/ipc/search.ts`)
    /// name the same fields and the same three commands.
    #[test]
    fn the_views_and_their_mirror_agree() {
        let mirror = include_str!("../../../app/src/lib/ipc/search.ts");
        let view = TicketView {
            id: String::new(),
            name: String::new(),
            group: String::new(),
            position: 0,
            query: String::new(),
            layout: String::new(),
            columns: vec![ViewColumn {
                name: String::new(),
                statuses: vec![],
                limit: None,
            }],
            lanes: String::new(),
            card_fields: vec![],
            list_columns: vec![],
            list_sort: Some(ListSort {
                column: String::new(),
                direction: String::new(),
            }),
            jira_ref: None,
        };
        let row = serde_json::to_value(TicketViewRow {
            view: view.clone(),
            count: None,
            unreadable: None,
            jira: Some(JiraSource {
                kind: String::new(),
                board_type: None,
                jql: String::new(),
                sub_query: None,
                sprint: None,
                last_read: chrono::DateTime::UNIX_EPOCH,
                web_url: None,
                hidden: false,
                new: false,
                unsynced: vec![Unsynced {
                    project: String::new(),
                    count: 0,
                }],
            }),
        })
        .unwrap();
        let draft = serde_json::to_value(draft()).unwrap();
        let keys = |value: &serde_json::Value| -> Vec<String> {
            value.as_object().unwrap().keys().cloned().collect()
        };
        for key in keys(&row)
            .iter()
            .chain(&keys(&row["columns"][0]))
            .chain(&keys(&row["list_sort"]))
            .chain(&keys(&row["jira"]))
            .chain(&keys(&row["jira"]["unsynced"][0]))
            .chain(&keys(&draft))
        {
            assert!(
                mirror.contains(&format!("  {key}:")) || mirror.contains(&format!("  {key}?:")),
                "{key} is not mirrored"
            );
        }
        for name in [
            "export interface TicketView {",
            "export interface TicketViewRow extends TicketView {",
            "export interface TicketViewDraft {",
            "export interface ViewColumn {",
            "export interface ListSort {",
            "export interface JiraSource {",
            "export interface UnsyncedTickets {",
            "\"ticket_views\"",
            "\"save_ticket_view\"",
            "\"delete_ticket_view\"",
            "\"hide_jira_view\"",
            "\"open_jira_view\"",
        ] {
            assert!(mirror.contains(name), "{name} is not in the mirror");
        }
        for (words, mirrored) in [
            (LANES, "export type ViewLanes"),
            (CARD_FIELDS, "export type CardField"),
            (LIST_COLUMNS, "export type ListColumn"),
            (SORT_COLUMNS, "export type ViewSortColumn"),
        ] {
            let at = mirror
                .find(mirrored)
                .unwrap_or_else(|| panic!("{mirrored} is not in the mirror"));
            let line = &mirror[at..at + mirror[at..].find(';').unwrap()];
            for word in words {
                assert!(
                    line.contains(&format!("\"{word}\"")),
                    "{mirrored} lacks {word}: {line}"
                );
            }
        }
    }
}
