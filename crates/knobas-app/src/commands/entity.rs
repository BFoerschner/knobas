//! Entity and room reads -- stream D (interfaces §2.5).
//!
//! Its `State<'_, Lifecycle>` is not an accident and is not stream D being
//! tidy: carry-over §10.6(a). `AppState` exists only once PostgreSQL is up,
//! and a `#[tauri::command]` resolves every argument *before* its body runs,
//! so a command declaring `State<'_, AppState>` is rejected by Tauri itself
//! during bring-up with the bare string `"state not managed"` -- no code for
//! the frontend to branch on. `Lifecycle` is managed at build time and is
//! always there; `lifecycle.pool()?` is the single place `not_ready` comes
//! from.
//!
//! ## Why there is no query builder here
//!
//! Roadmap §4 gotcha 2 asks for dynamic SQL to live in one reviewed module
//! behind `AssertSqlSafe`. This module dodges the question instead: every
//! optional predicate is a **nullable parameter** (`$1::text[] is null or ...`)
//! and the two orderings are two `const` statements chosen by a `match`, so
//! nothing here ever concatenates a string into SQL and `sqlx::query` is only
//! ever handed a `&'static str`. A filter that grows a new optional predicate
//! adds a parameter, not a branch.
//!
//! ## Why the columns are named
//!
//! `sync.live_item` exposes `fts`, a `tsvector` (migration `0002`'s own
//! WARNING). Naming the six columns each statement wants keeps that value out
//! of the result set entirely rather than relying on nobody ever asking for it
//! by name.

use chrono::{DateTime, Utc};
use knobas_core::activity::ActivityRow;
use knobas_core::entity::EntityRef;
use knobas_core::link::LinkRow;
use sqlx::{PgPool, Row};
use tauri::State;

use crate::{IpcError, Lifecycle};

/// One line in a room: what a tile draws, and nothing more.
///
/// Deliberately not the mirror row. `body_text` and `payload` are the detail
/// view's business ([`EntityDetail`]), and a room that fetched them would pull
/// the whole corpus into the webview to render five columns of it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EntityRow {
    pub entity_id: String,
    pub kind: String,
    pub source_id: String,
    pub title: String,
    /// When the *source* says the item changed.
    ///
    /// `None` where the source never said (interfaces §4.1 normalization):
    /// the mirror keeps the hole rather than filling it with `now()`, so a row
    /// that has never been dated is distinguishable from one changed this
    /// second.
    pub updated_at: Option<DateTime<Utc>>,
    /// When knobas last saw it. Always known -- it is knobas' own clock.
    pub synced_at: DateTime<Utc>,
}

/// Which slice of the corpus a room wants.
///
/// Both lists are *unfiltered when empty*, which is why they are bound as
/// `NULL` rather than as an empty array: `= any('{}')` matches nothing, and a
/// room whose filter said "no sources in particular" would come back empty.
///
/// `Serialize` alongside `Deserialize` although nothing sends a filter the
/// other way: it is what lets `tests/entity_mirror.rs` pin an *input* DTO
/// through a round trip, and so see a field this struct has that
/// `app/src/lib/ipc/entity.ts` never declares. A decode-only check is blind to
/// that direction.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct EntityFilter {
    /// Source ids to include; empty means every source.
    pub sources: Vec<String>,
    /// Kinds to include; empty means every kind.
    pub kinds: Vec<String>,
    /// Only items the source dated within this many days. Items the source
    /// never dated are excluded by the window, because the window is about the
    /// source's own timestamp.
    pub updated_within_days: Option<u32>,
    pub order: EntityOrder,
    /// Whether to reach past `sync.live_item` for entities withdrawn upstream
    /// (§5a: links and notes may point at them).
    pub include_deleted: bool,
}

/// The two orderings a room offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityOrder {
    /// Newest first, by the source's own timestamp. Undated items last.
    UpdatedDesc,
    /// Alphabetical by title.
    TitleAsc,
}

/// One page of a room, plus the size of the set it was cut from.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EntityPage {
    pub rows: Vec<EntityRow>,
    /// The whole filtered set, before `limit`/`offset`. `0` for an empty page.
    pub total: i64,
}

/// The live corpus, newest first.
///
/// `count(*) over ()` is the unpaged total in the same round trip, and the
/// window is evaluated before `limit`, so it describes the set rather than the
/// page.
const LIVE_UPDATED: &str = r#"
select entity_id, source_id, kind, title, item_updated_at, synced_at,
       count(*) over () as total
  from sync.live_item
 where ($1::text[] is null or source_id = any($1))
   and ($2::text[] is null or kind      = any($2))
   and ($3::int    is null or item_updated_at >= now() - make_interval(days => $3))
 order by item_updated_at desc nulls last, entity_id
 limit $4 offset $5
"#;

/// The live corpus, alphabetically. A second statement rather than an
/// interpolated `order by`.
const LIVE_TITLE: &str = r#"
select entity_id, source_id, kind, title, item_updated_at, synced_at,
       count(*) over () as total
  from sync.live_item
 where ($1::text[] is null or source_id = any($1))
   and ($2::text[] is null or kind      = any($2))
   and ($3::int    is null or item_updated_at >= now() - make_interval(days => $3))
 order by title asc, entity_id
 limit $4 offset $5
"#;

/// As [`LIVE_UPDATED`], reaching past the tombstone filter.
///
/// The join is what `sync.live_item` is; dropping only its `deleted_at is
/// null` is the whole difference.
const ALL_UPDATED: &str = r#"
select i.entity_id, i.source_id, i.kind, i.title, i.item_updated_at, i.synced_at,
       count(*) over () as total
  from sync.item i
  join knobas.entity e on e.id = i.entity_id
 where ($1::text[] is null or i.source_id = any($1))
   and ($2::text[] is null or i.kind      = any($2))
   and ($3::int    is null or i.item_updated_at >= now() - make_interval(days => $3))
 order by i.item_updated_at desc nulls last, i.entity_id
 limit $4 offset $5
"#;

/// As [`LIVE_TITLE`], reaching past the tombstone filter.
const ALL_TITLE: &str = r#"
select i.entity_id, i.source_id, i.kind, i.title, i.item_updated_at, i.synced_at,
       count(*) over () as total
  from sync.item i
  join knobas.entity e on e.id = i.entity_id
 where ($1::text[] is null or i.source_id = any($1))
   and ($2::text[] is null or i.kind      = any($2))
   and ($3::int    is null or i.item_updated_at >= now() - make_interval(days => $3))
 order by i.title asc, i.entity_id
 limit $4 offset $5
"#;

/// The statement this filter reads through. Four constants, one `match`.
fn statement(filter: &EntityFilter) -> &'static str {
    match (filter.include_deleted, filter.order) {
        (false, EntityOrder::UpdatedDesc) => LIVE_UPDATED,
        (false, EntityOrder::TitleAsc) => LIVE_TITLE,
        (true, EntityOrder::UpdatedDesc) => ALL_UPDATED,
        (true, EntityOrder::TitleAsc) => ALL_TITLE,
    }
}

/// An empty list is *no filter*, which on the wire is `NULL`.
fn any_of(values: &[String]) -> Option<Vec<String>> {
    (!values.is_empty()).then(|| values.to_vec())
}

/// One page of the room addressed by `filter`.
///
/// The behaviour lives here rather than in the command so it is reachable from
/// a test: a `#[tauri::command]` cannot be called directly.
///
/// # Errors
///
/// [`IpcError`] with [`Internal`](crate::IpcErrorCode::Internal) if the query
/// fails.
pub async fn list_entities_inner(
    pool: &PgPool,
    filter: &EntityFilter,
    limit: u32,
    offset: u32,
) -> Result<EntityPage, IpcError> {
    // `u32 -> i32` and not `as`: `updated_within_days` past `i32::MAX` is a
    // caller error, and silently wrapping it into a negative interval would
    // turn "the last four billion days" into "the next few".
    let within =
        match filter.updated_within_days {
            Some(days) => Some(i32::try_from(days).map_err(|_| {
                IpcError::invalid(format!("updated_within_days {days} is too large"))
            })?),
            None => None,
        };

    let rows = sqlx::query(statement(filter))
        .bind(any_of(&filter.sources))
        .bind(any_of(&filter.kinds))
        .bind(within)
        .bind(i64::from(limit))
        .bind(i64::from(offset))
        .fetch_all(pool)
        .await?;

    // Off the first row, because the window function is per-row and every row
    // carries the same value. An empty page has no row to carry it, and 0 is
    // the truth for a page past the end of an empty set as much as for one
    // past the end of a full one -- the count the caller can act on is the one
    // it can see.
    let total: i64 = rows.first().map_or(0, |row| row.get("total"));

    Ok(EntityPage {
        rows: rows
            .iter()
            .map(|row| EntityRow {
                entity_id: row.get("entity_id"),
                kind: row.get("kind"),
                source_id: row.get("source_id"),
                title: row.get("title"),
                updated_at: row.get("item_updated_at"),
                synced_at: row.get("synced_at"),
            })
            .collect(),
        total,
    })
}

/// One page of a room.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, [`Internal`](crate::IpcErrorCode::Internal)
/// for a query failure.
#[tauri::command]
pub async fn list_entities(
    lifecycle: State<'_, Lifecycle>,
    filter: EntityFilter,
    limit: u32,
    offset: u32,
) -> Result<EntityPage, IpcError> {
    let pool = lifecycle.pool()?;
    list_entities_inner(&pool, &filter, limit, offset).await
}

// -- the detail view --------------------------------------------------------

/// Which source an entity came from, as the detail view names it.
///
/// Every field falls back to the source id, because `run_once` syncs sources
/// that have no `knobas.source_config` row (tests, ad-hoc imports, interfaces
/// §1). An entity whose source was deleted still has to render.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SourceRef {
    pub id: String,
    pub display_name: String,
    /// The *adapter* kind (`jira`, `mock`), not the instance id.
    pub adapter_kind: String,
}

/// Everything the slide-over draws for one entity (interfaces §2.5).
#[derive(Debug, Clone, serde::Serialize)]
pub struct EntityDetail {
    pub row: EntityRow,
    pub source: SourceRef,
    /// The adapter's own label and monogram for this kind.
    ///
    /// **`None` throughout M1 phase 1.** Resolving it means asking the adapter
    /// registry (`crates/knobas-app/src/sources/**`, stream F) what an
    /// installed adapter declares, and that does not exist yet; task 21 fills
    /// it in. The field ships now because it is in the contract and because
    /// the frontend's fallback -- the title-cased kind -- is what §3a asks for
    /// when nothing declares one anyway.
    pub kind_info: Option<knobas_source::KindInfo>,
    /// Untrusted source text. Rendered as text, never as markup (gotcha 7).
    pub body_text: String,
    pub author: Option<String>,
    /// The source record **verbatim** (§3a): what the generic detail view
    /// projects. It is untrusted text all the way down.
    pub payload: serde_json::Value,
    /// Where the item lives in its own system, or `None` when the adapter
    /// reported no page -- in which case *Open in browser* is absent (P5).
    pub web_url: Option<String>,
    /// Set when the source withdrew the entity. The mirror row survives, so
    /// links and notes still resolve (§5a).
    pub deleted_at: Option<DateTime<Utc>>,
    /// Always empty in M1: nothing writes `knobas.link` until M2.
    pub links: Vec<LinkRow>,
    /// This entity's own history, newest first (spec §12.1).
    pub activity: Vec<ActivityRow>,
}

/// How many history lines the detail view is given up front.
///
/// Enough to fill the panel without a second round trip; the full history is a
/// later milestone's view, not a scroll in a slide-over.
const DETAIL_ACTIVITY: i64 = 20;

/// One entity, deleted or not.
///
/// The join is `sync.live_item`'s minus its tombstone filter, deliberately:
/// §5a says a withdrawn entity must still open, and `e.deleted_at` is what the
/// banner reads. `source_config` is a **left** join because `run_once` syncs
/// unconfigured sources.
const DETAIL: &str = r#"
select i.entity_id, i.source_id, i.kind, i.title, i.body_text, i.author,
       i.item_updated_at, i.synced_at, i.payload, i.web_url,
       e.deleted_at,
       c.display_name, c.kind as adapter_kind
  from sync.item i
  join knobas.entity e on e.id = i.entity_id
  left join knobas.source_config c on c.id = i.source_id
 where i.entity_id = $1
"#;

/// Everything the slide-over needs for `entity_id`, in three round trips.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `entity_id` is not an entity
/// id -- which is how a mistyped deep link reports itself rather than as a
/// 500; [`NotFound`](crate::IpcErrorCode::NotFound) if nothing carries it;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn get_entity_inner(pool: &PgPool, entity_id: &str) -> Result<EntityDetail, IpcError> {
    // First, and before any query: `#/ticket/not-an-id` is a bad address, not
    // a missing entity, and the two want different words on screen.
    let entity = EntityRef::parse(entity_id).map_err(IpcError::invalid)?;

    let row = sqlx::query(DETAIL)
        .bind(entity.to_string())
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| IpcError::not_found(format!("{entity} is not in the local index")))?;

    let source_id: String = row.get("source_id");
    let display_name: Option<String> = row.get("display_name");
    let adapter_kind: Option<String> = row.get("adapter_kind");

    Ok(EntityDetail {
        row: EntityRow {
            entity_id: row.get("entity_id"),
            kind: row.get("kind"),
            source_id: source_id.clone(),
            title: row.get("title"),
            updated_at: row.get("item_updated_at"),
            synced_at: row.get("synced_at"),
        },
        source: SourceRef {
            display_name: display_name.unwrap_or_else(|| source_id.clone()),
            adapter_kind: adapter_kind.unwrap_or_else(|| source_id.clone()),
            id: source_id,
        },
        // Task 21, once there is a registry to ask. See the field's docs.
        kind_info: None,
        body_text: row.get("body_text"),
        author: row.get("author"),
        payload: row.get("payload"),
        web_url: row.get("web_url"),
        deleted_at: row.get("deleted_at"),
        links: knobas_core::link::links_of(pool, &entity).await?,
        activity: knobas_core::activity::recent(pool, DETAIL_ACTIVITY, Some(&entity)).await?,
    })
}

/// One entity, for the detail slide-over.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, and whatever [`get_entity_inner`] refuses
/// with.
#[tauri::command]
pub async fn get_entity(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
) -> Result<EntityDetail, IpcError> {
    let pool = lifecycle.pool()?;
    get_entity_inner(&pool, &entity_id).await
}

/// The `limit` most recent activity lines, globally or for one entity.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `entity` is not an entity id,
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn recent_activity_inner(
    pool: &PgPool,
    limit: u32,
    entity: Option<&EntityRef>,
) -> Result<Vec<ActivityRow>, IpcError> {
    Ok(knobas_core::activity::recent(pool, i64::from(limit), entity).await?)
}

/// The `limit` most recent activity-log lines, newest first.
///
/// `entityId` scopes the read to one entity's history; omitting it is the
/// global stream the status bar reads.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, [`Invalid`](crate::IpcErrorCode::Invalid) for
/// a malformed `entityId`, [`Internal`](crate::IpcErrorCode::Internal) for a
/// query failure.
#[tauri::command]
pub async fn recent_activity(
    lifecycle: State<'_, Lifecycle>,
    limit: u32,
    entity_id: Option<String>,
) -> Result<Vec<ActivityRow>, IpcError> {
    let pool = lifecycle.pool()?;
    let entity = entity_id
        .as_deref()
        .map(EntityRef::parse)
        .transpose()
        .map_err(IpcError::invalid)?;
    recent_activity_inner(&pool, limit, entity.as_ref()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The four statements are four, and each names its columns.
    ///
    /// This reads the constants themselves -- the strings that are sent to
    /// PostgreSQL -- not the source file that spells them, so it is an
    /// assertion about the statement rather than about how it was written.
    #[test]
    fn every_statement_names_its_columns_and_no_two_are_the_same() {
        let all = [LIVE_UPDATED, LIVE_TITLE, ALL_UPDATED, ALL_TITLE];
        for sql in all {
            assert!(
                !sql.contains("select *"),
                "`sync.live_item.fts` is a tsvector: name the columns (gotcha 2)"
            );
            assert!(sql.contains("count(*) over ()"), "the unpaged total");
        }
        let distinct = all.iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            distinct.len(),
            4,
            "an ordering or a scope shares a statement"
        );
    }

    /// Every combination reaches its own statement.
    #[test]
    fn the_four_combinations_choose_the_four_statements() {
        let filter = |include_deleted, order| EntityFilter {
            sources: Vec::new(),
            kinds: Vec::new(),
            updated_within_days: None,
            order,
            include_deleted,
        };
        assert_eq!(
            statement(&filter(false, EntityOrder::UpdatedDesc)),
            LIVE_UPDATED
        );
        assert_eq!(statement(&filter(false, EntityOrder::TitleAsc)), LIVE_TITLE);
        assert_eq!(
            statement(&filter(true, EntityOrder::UpdatedDesc)),
            ALL_UPDATED
        );
        assert_eq!(statement(&filter(true, EntityOrder::TitleAsc)), ALL_TITLE);
    }

    /// An empty list is not an empty array.
    #[test]
    fn an_empty_list_binds_as_null() {
        assert_eq!(any_of(&[]), None);
        assert_eq!(any_of(&["jira".to_owned()]), Some(vec!["jira".to_owned()]));
    }

    /// The frontend sends `"updated_desc"`; a rename here is a room that
    /// cannot be ordered.
    #[test]
    fn the_order_deserializes_as_the_frontend_spells_it() {
        assert_eq!(
            serde_json::from_str::<EntityOrder>("\"updated_desc\"").unwrap(),
            EntityOrder::UpdatedDesc
        );
        assert_eq!(
            serde_json::from_str::<EntityOrder>("\"title_asc\"").unwrap(),
            EntityOrder::TitleAsc
        );
        assert!(serde_json::from_str::<EntityOrder>("\"UpdatedDesc\"").is_err());
    }
}
