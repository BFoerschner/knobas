//! Entity and room reads, and the two link writes -- stream D
//! (interfaces §2.5).
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
use knobas_core::link::{LinkEntry, LinkRow, Origin};
use knobas_core::suggest::{self, SuggestionEntry};
use sqlx::{PgPool, Row};
use tauri::{Emitter, State};
use uuid::Uuid;

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
/// `Serialize` behind `test-util`, although nothing sends a filter the other
/// way: it is what lets `tests/entity_mirror.rs` pin an *input* DTO through a
/// round trip, and so see a field this struct has that
/// `app/src/lib/ipc/entity.ts` never declares. A decode-only check is blind to
/// that direction -- serde reads a missing `Option` as `None`, so a Rust-only
/// optional field decodes clean and the mirror never has to declare it.
///
/// Gated rather than unconditional, for the reason the feature exists at all
/// (see `Cargo.toml`): a filter is something the frontend *sends*, and a
/// production build that can also write one invites a caller to round-trip a
/// command argument through nothing. `cargo test` turns the feature on through
/// this crate's self-dev-dependency; `tauri build` and the `clippy --lib` half
/// of the gate compile without it.
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(feature = "test-util", derive(serde::Serialize))]
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
///
/// `Serialize` behind `test-util` for the same reason [`EntityFilter`] carries
/// it: it rides inside the filter's round trip, and pins this union against the
/// mirror's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[cfg_attr(feature = "test-util", derive(serde::Serialize))]
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
    /// Resolved from the adapter registry by the row's `adapter_kind` -- see
    /// [`kind_info_for`]. `None` for a kind the adapter does not declare, and
    /// for a source with no configuration row at all (which `run_once`
    /// produces). Both cases fall through to the frontend's §3a humaniser,
    /// which is what §3a asks for when nothing declares anything.
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
    /// Every link this entity takes part in, newest first, each with the end
    /// the reader is *not* on already resolved.
    ///
    /// Read undirected by [`knobas_core::link::entries_of`], so a link drawn
    /// from either end is on both ends' detail, and hydrated there, so the
    /// panel draws a kind and a title rather than a raw id -- including for a
    /// target the source withdrew. [`create_link`] and [`unlink`] are what
    /// move it.
    pub links: Vec<LinkEntry>,
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

    let kind: String = row.get("kind");
    let kind_info = kind_info_for(adapter_kind.as_deref(), &kind);

    Ok(EntityDetail {
        row: EntityRow {
            entity_id: row.get("entity_id"),
            kind,
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
        kind_info,
        body_text: row.get("body_text"),
        author: row.get("author"),
        payload: row.get("payload"),
        web_url: row.get("web_url"),
        deleted_at: row.get("deleted_at"),
        links: knobas_core::link::entries_of(pool, &entity).await?,
        activity: knobas_core::activity::recent(pool, DETAIL_ACTIVITY, Some(&entity)).await?,
    })
}

/// What the adapter declares about one kind, if anything does.
///
/// §3a: *"entity kinds with display metadata drive grouping, chips and
/// labels"*. Looked up by the **adapter** kind and not the instance id, because
/// `jira` and `jira-eu` are two instances of one adapter that declare the same
/// kinds (P10).
///
/// `None` in three cases, and all three are the same answer to the frontend:
/// no configuration row for the source (`run_once` produces exactly that), no
/// adapter of that kind compiled in, and an adapter that simply does not
/// declare this kind. §3a's whole point is that an undeclared kind still
/// renders, so the humaniser on the other side is the designed path rather
/// than a degradation.
///
/// **First declaration wins**, deterministically: `ADAPTERS` is a fixed table
/// in a fixed order, so two adapters declaring one kind id resolve the same
/// way on every call rather than by whichever the iterator reached first this
/// time.
fn kind_info_for(adapter_kind: Option<&str>, kind: &str) -> Option<knobas_source::KindInfo> {
    let adapter_kind = adapter_kind?;
    crate::sources::Registry::builtin()
        .templates()
        .into_iter()
        .find(|template| template.adapter_kind == adapter_kind)?
        .entity_kinds
        .into_iter()
        .find(|info| info.id == kind)
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

// -- the link writes --------------------------------------------------------
//
// Two commands, and only two. The *read* stays on `get_entity`: `entries_of`
// is undirected, so an entity's backlinks are the same query as its links and
// need no endpoint of their own, and the target picker reuses `search`.

/// The relation a link takes when the caller names none.
///
/// A quick link costs no extra decisions, which is only true if "no relation"
/// is a relation rather than an empty string: the panel groups by this value
/// and an empty group header is a row nobody can read.
pub const DEFAULT_RELATION: &str = "related";

/// Who a link made through this surface belongs to, in `created_by` and in the
/// activity line's `actor`.
///
/// knobas has no identity system: there is one person using it, and the only
/// other actor the log knows is `sync:<source_id>`. Naming it once here keeps
/// the two spellings from drifting.
const ACTOR: &str = "user";

/// What a link mutation left behind: the row, and the activity line announcing
/// it.
///
/// Not a wire type -- the commands return the link and emit the line. It exists
/// so the behaviour is reachable from a test without a Tauri app: the emit is
/// the only part of these commands that a `#[tauri::command]` shell adds, and
/// everything worth asserting is in here.
#[derive(Debug)]
pub struct LinkMutation {
    pub link: LinkRow,
    pub activity: ActivityRow,
}

/// Text the user did not type is no text.
///
/// A dialog hands back `""` for a field left alone, and `Some("")` is not the
/// same fact as `None` anywhere downstream: an empty relation is an unreadable
/// group header, and an empty note is a line the panel would draw with nothing
/// in it.
fn present(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|text| !text.is_empty())
}

/// The relation a link will carry, folded to the one spelling that groups it.
///
/// A relation is a *key*, not prose: the panel groups rows by this value and
/// the duplicate rule compares it verbatim, so `Blocks` and `blocks` would be
/// two headers for one relationship and neither would see the other as a
/// duplicate. Björn's ruling on the review of #52 (2026-08-28) is that they are
/// one relation, and folding on write is what makes that true everywhere at
/// once -- the stored value, the unique index, and every later reader.
///
/// Lowercase because the curated list, [`DEFAULT_RELATION`] and the design's
/// own vocabulary (`blocks`, `implements`, `documents`, `depends-on`) are all
/// written that way; a folded value is therefore already the spelling #53's
/// inverse-label lookup will key on. Display capitalization is the panel's, and
/// it has the whole string to do it from.
///
/// [`str::to_lowercase`] rather than the ASCII form: a user typing a relation
/// in their own language should get the same folding an English one gets.
///
/// The **note** is deliberately not folded -- it is prose in the user's own
/// words and nothing groups by it.
fn relation_of(value: Option<&str>) -> String {
    present(value).map_or_else(|| DEFAULT_RELATION.to_owned(), |named| named.to_lowercase())
}

/// What an activity line says about a link: the other end, the relation, and
/// which link it was.
///
/// The line is named on the link's `from` end, so `to_id` is the end the reader
/// does not already know. One helper, because `linked` and `unlinked` describe
/// the same link and a reader of the log has to be able to pair them.
fn link_detail(link: &LinkRow) -> serde_json::Value {
    let mut detail = serde_json::json!({
        "link_id": link.id,
        "to_id": link.to_id,
        "relation": link.relation,
    });
    // Only when there is one, which is only ever for a link knobas proposed:
    // "accepted" with no reason beside it is a line that says a decision was
    // made and not what it was about. A key that is absent and a key that is
    // `null` are different facts to a reader of the log, and the second is the
    // one worth avoiding.
    if let (Some(map), Some(reason)) = (detail.as_object_mut(), link.reason.as_deref()) {
        map.insert("reason".to_owned(), serde_json::Value::from(reason));
    }
    detail
}

/// Draw a link between two entities.
///
/// `relation` defaults to [`DEFAULT_RELATION`] and is folded by
/// [`relation_of`]; `note` is optional and is kept as typed; the origin is
/// always [`Origin::Manual`] -- it is deliberately not client-suppliable in v1,
/// because the other origins belong to the suggestion engine and to import.
///
/// An entity may not be linked to itself.
///
/// The behaviour lives here rather than in the command so it is reachable from
/// a test: a `#[tauri::command]` cannot be called directly.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if either id is not an entity id,
/// or if the two are the same entity;
/// [`NotFound`](crate::IpcErrorCode::NotFound) if either endpoint has no local
/// entity -- linking to something that has not synced yet is an ordinary event,
/// not a fault; [`Conflict`](crate::IpcErrorCode::Conflict) if that pair is
/// already actively linked under that relation;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn create_link_inner(
    pool: &PgPool,
    from_id: &str,
    to_id: &str,
    relation: Option<&str>,
    note: Option<&str>,
) -> Result<LinkMutation, IpcError> {
    // Before any query, as `get_entity_inner` does: a malformed id is a bad
    // address, and the two want different words on screen.
    let from = EntityRef::parse(from_id).map_err(IpcError::invalid)?;
    let to = EntityRef::parse(to_id).map_err(IpcError::invalid)?;
    // Also before any query: an entity linked to itself is a bad *request*, not
    // a missing thing -- both ends resolve, they are simply the same one, and
    // the row would draw as a panel entry pointing at the entity you are
    // already looking at. Björn's ruling on the review of #52 (2026-08-28).
    if from == to {
        return Err(IpcError::invalid(format!(
            "{from} cannot be linked to itself"
        )));
    }

    let relation = relation_of(relation);
    let link = match knobas_core::link::create(
        pool,
        &from,
        &to,
        &relation,
        Origin::Manual,
        present(note),
        ACTOR,
    )
    .await
    {
        Ok(link) => link,
        // The pair may already carry a *proposal*: `link_active_idx` spans
        // proposals and links alike (one active edge per triple, whatever its
        // state), so knobas suggesting this link is what refused it. Drawing by
        // hand the link knobas proposed is the same act as pressing *Accept*,
        // and the alternative is telling the user "already linked" about a pair
        // whose links panel is empty.
        Err(knobas_core::CoreError::Duplicate) => {
            match suggest::accept_edge(pool, &from, &to, &relation).await? {
                Some(promoted) => promoted,
                // Nothing to promote: the blocker is a real link, which is what
                // `conflict` has always meant here.
                None => return Err(knobas_core::CoreError::Duplicate.into()),
            }
        }
        Err(other) => return Err(other.into()),
    };

    let activity = record_link_activity(pool, "linked", &link).await?;
    Ok(LinkMutation { link, activity })
}

/// Withdraw a link.
///
/// `Ok(None)` when the link was already withdrawn: the store is idempotent, and
/// nothing was mutated, so nothing is written to the log and nothing is
/// announced.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `link_id` is not a UUID;
/// [`NotFound`](crate::IpcErrorCode::NotFound) if no link carries it;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn unlink_inner(pool: &PgPool, link_id: &str) -> Result<Option<LinkMutation>, IpcError> {
    let id: Uuid = link_id
        .parse()
        .map_err(|_| IpcError::invalid(format!("{link_id} is not a link id")))?;

    let Some(link) = knobas_core::link::unlink(pool, id).await? else {
        return Ok(None);
    };
    let activity = record_link_activity(pool, "unlinked", &link).await?;
    Ok(Some(LinkMutation { link, activity }))
}

/// One line, on the end the link was drawn from.
///
/// The to-end's own history panel therefore does not list it -- a known and
/// accepted v1 limitation; that entity's links panel still shows the link. If
/// the inbox (#45) ever needs the other side, the fix is a read-side change and
/// not a second row.
async fn record_link_activity(
    pool: &PgPool,
    verb: &str,
    link: &LinkRow,
) -> Result<ActivityRow, IpcError> {
    let from = EntityRef::parse(&link.from_id).map_err(IpcError::internal)?;
    Ok(knobas_core::activity::record(pool, ACTOR, verb, Some(&from), link_detail(link)).await?)
}

/// Put an activity line on `activity:new`.
///
/// Best-effort, like every other emit in this app: a failure means no window is
/// listening, which is not a reason to fail a write that already landed. The
/// command emits it itself rather than re-reading the log the way the scheduler
/// does -- the write hands its row back precisely so it does not have to guess
/// which of the table's rows is its own.
fn announce<R: tauri::Runtime>(app: &tauri::AppHandle<R>, row: ActivityRow) {
    if let Err(error) = app.emit(crate::events::ACTIVITY_NEW, row) {
        tracing::debug!(
            event = crate::events::ACTIVITY_NEW,
            %error,
            "nothing was listening for this event"
        );
    }
}

/// Draw a link between two entities, and announce it.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`create_link_inner`] refuses with.
#[tauri::command]
pub async fn create_link<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    from_id: String,
    to_id: String,
    relation: Option<String>,
    note: Option<String>,
) -> Result<LinkRow, IpcError> {
    let pool = lifecycle.pool()?;
    let written = create_link_inner(
        &pool,
        &from_id,
        &to_id,
        relation.as_deref(),
        note.as_deref(),
    )
    .await?;
    announce(&app, written.activity);
    Ok(written.link)
}

/// Withdraw a link, and announce it.
///
/// Idempotent: withdrawing an already-withdrawn link succeeds, writes no second
/// line and announces nothing.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`unlink_inner`] refuses with.
/// Ask a source to change something (issue #43): the one way the UI starts a
/// write-back.
///
/// `payload` is a serialized `knobas_source::WriteOp` -- the same shape
/// [`crate::commands::sources::pending_writes`] hands back and
/// [`crate::commands::sources::amend_write`] takes. Untyped on the wire for
/// #42's ratified reasoning: `WriteOp` grows per milestone (ADR-0006), and a
/// typed argument would drag the SPI's enum onto the IPC surface and make
/// every growth an IPC change.
///
/// **The write is queued, not sent** -- and the row that comes back is the
/// write *as queued*, before the attempt, because that is what happened. Read
/// the outcome back with `pending_writes`, or watch `activity:new`: every
/// queue transition writes a line. A UI that reported "sent" from this return
/// value would be reporting a hope. The queue is what decides send, pend or
/// hold (issue #42), and there is no path around it.
///
/// **There is no `source_id` argument.** Interfaces §4.1 makes the instance id
/// and the `EntityRef` namespace the same string, so the target already names
/// the source; a second argument could only agree or contradict, and a
/// contradiction would aim a write at a source the target does not belong to.
///
/// # Errors
///
/// `invalid` if the payload is not a write op, if its target is not an entity
/// id, or if the source does not offer that op; `not_found` if the target
/// names a source that is not configured;
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up.
#[tauri::command]
pub async fn submit_write<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    payload: serde_json::Value,
) -> Result<knobas_core::write_queue::QueuedWrite, IpcError> {
    let state = crate::sources::state(&app)?;
    crate::sources::write_queue::submit(&state, payload).await
}

#[tauri::command]
pub async fn unlink<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    link_id: String,
) -> Result<(), IpcError> {
    let pool = lifecycle.pool()?;
    if let Some(written) = unlink_inner(&pool, &link_id).await? {
        announce(&app, written.activity);
    }
    Ok(())
}

/// Everything the note view draws for one note (#46).
///
/// Deliberately **not** [`EntityDetail`], and the difference is not tidiness:
/// that DTO is shaped around a mirrored item -- a source, a `payload`, a
/// `web_url`, a `synced_at` -- and a note has none of those. Half of it would
/// be filled in with placeholders that a reader could not tell from real ones.
/// A note is an entity and shares the address space; it is not a mirror row.
///
/// `refs` and `links` overlap and both are here because they answer different
/// questions. `refs` is *the body's own list*, in body order, including the
/// ones that resolve to nothing -- which is what the editor draws chips from
/// and what makes an unresolved ref visible (story 10). `links` is the panel
/// #53 built: every link this note takes part in, in either direction, so the
/// note shows what points at it as well as what it points at.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NoteDetail {
    pub note: knobas_core::note::NoteRow,
    /// The `[[refs]]` the body names, in body order, resolved where they
    /// resolve.
    pub refs: Vec<knobas_core::note::NoteRef>,
    /// Every link this note takes part in, each with its other end resolved --
    /// the ref links it derived, plus anything drawn by hand from either side.
    pub links: Vec<LinkEntry>,
}

/// The note at `note_id`, its refs and its links.
///
/// Three round trips, like [`get_entity_inner`]'s: the note, the refs, the
/// links.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `note_id` is not an entity id,
/// [`NotFound`](crate::IpcErrorCode::NotFound) if no note carries it,
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn get_note_inner(pool: &PgPool, note_id: &str) -> Result<NoteDetail, IpcError> {
    let id = EntityRef::parse(note_id).map_err(IpcError::invalid)?;
    let note = knobas_core::note::get(pool, &id)
        .await?
        .ok_or_else(|| IpcError::not_found(format!("there is no note at {id}")))?;
    Ok(NoteDetail {
        note,
        refs: knobas_core::note::refs_of(pool, &id).await?,
        links: knobas_core::link::entries_of(pool, &id).await?,
    })
}

/// Write a new note.
///
/// Both arguments are optional because the affordance is *"start writing"*: a
/// note created from an empty editor has no title and no body yet, and it still
/// has to exist -- story 2 is that a thought is never lost to a closed window,
/// which needs the row to be there before the first keystroke settles.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) for a write failure.
pub async fn create_note_inner(
    pool: &PgPool,
    title: Option<&str>,
    body_md: Option<&str>,
) -> Result<NoteDetail, IpcError> {
    let note = knobas_core::note::create(
        pool,
        title.unwrap_or_default(),
        body_md.unwrap_or_default(),
        ACTOR,
    )
    .await?;
    get_note_inner(pool, &note.id).await
}

/// Save a note's title and body, and bring its `[[refs]]` back into step.
///
/// This is what the editor's autosave calls, so it answers with the whole
/// [`NoteDetail`]: the refs it just reconciled are what the editor redraws its
/// chips from, and a second read to fetch them would race the next keystroke.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `note_id` is not an entity id,
/// [`NotFound`](crate::IpcErrorCode::NotFound) if no note carries it -- an
/// editor whose note was deleted elsewhere is told so rather than silently
/// resurrecting it, [`Internal`](crate::IpcErrorCode::Internal) for a write
/// failure.
pub async fn save_note_inner(
    pool: &PgPool,
    note_id: &str,
    title: &str,
    body_md: &str,
) -> Result<NoteDetail, IpcError> {
    let id = EntityRef::parse(note_id).map_err(IpcError::invalid)?;
    knobas_core::note::save(pool, &id, title, body_md, ACTOR)
        .await?
        .ok_or_else(|| IpcError::not_found(format!("there is no note at {id}")))?;
    get_note_inner(pool, note_id).await
}

/// Delete a note: the body goes, the address stays, tombstoned.
///
/// `Ok(false)` when there was nothing to delete. Idempotent, like
/// [`unlink_inner`]: a second *Delete* on a note already gone is not an error,
/// and the caller can tell the two apart.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `note_id` is not an entity id,
/// [`Internal`](crate::IpcErrorCode::Internal) for a write failure.
pub async fn delete_note_inner(pool: &PgPool, note_id: &str) -> Result<bool, IpcError> {
    let id = EntityRef::parse(note_id).map_err(IpcError::invalid)?;
    Ok(knobas_core::note::delete(pool, &id).await?)
}

/// One note, with its refs and its links.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`get_note_inner`] refuses with.
#[tauri::command]
pub async fn get_note(
    lifecycle: State<'_, Lifecycle>,
    note_id: String,
) -> Result<NoteDetail, IpcError> {
    let pool = lifecycle.pool()?;
    get_note_inner(&pool, &note_id).await
}

/// Write a new note.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`create_note_inner`] refuses with.
#[tauri::command]
pub async fn create_note(
    lifecycle: State<'_, Lifecycle>,
    title: Option<String>,
    body_md: Option<String>,
) -> Result<NoteDetail, IpcError> {
    let pool = lifecycle.pool()?;
    create_note_inner(&pool, title.as_deref(), body_md.as_deref()).await
}

/// Save a note, refs and all.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`save_note_inner`] refuses with.
#[tauri::command]
pub async fn save_note(
    lifecycle: State<'_, Lifecycle>,
    note_id: String,
    title: String,
    body_md: String,
) -> Result<NoteDetail, IpcError> {
    let pool = lifecycle.pool()?;
    save_note_inner(&pool, &note_id, &title, &body_md).await
}

/// Delete a note.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`delete_note_inner`] refuses with.
#[tauri::command]
pub async fn delete_note(
    lifecycle: State<'_, Lifecycle>,
    note_id: String,
) -> Result<bool, IpcError> {
    let pool = lifecycle.pool()?;
    delete_note_inner(&pool, &note_id).await
}

// -- suggestions and the room tray -----------------------------------------
//
// Four commands, in this module and not a new one: a suggestion **is** a link
// row (#41), these are the writes that move it between the two states the link
// store already has, and the read is the same graph the panel above reads from
// the other side. A `commands/suggest.rs` would be a second module over one
// table.

/// One page of a room's tray: the proposals, and how many there are in all.
///
/// `total` is the whole room, not the page -- #41's story 19 is "see how many
/// suggestions are waiting, so that I can choose when to spend attention on
/// them", and a number capped by `limit` answers a different question. Same
/// shape and same reason as [`EntityPage`].
#[derive(Debug, Clone, serde::Serialize)]
pub struct SuggestionPage {
    pub rows: Vec<SuggestionEntry>,
    /// Every proposal in the room, before `limit`.
    pub total: i64,
}

/// Run a detection pass, returning how many proposals it wrote.
///
/// Idempotent and cheap to repeat, which is what lets the room call it rather
/// than the user (#41 story 22): a pass over an unchanged mirror writes
/// nothing, and a pass after a sync writes only what the new items justify.
/// It never creates a confirmed link and never writes to a source.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if a statement fails.
pub async fn detect_suggestions_inner(pool: &PgPool) -> Result<u32, IpcError> {
    let written = suggest::detect(pool).await?;
    // A pass that proposed more than four billion links is not a number the
    // frontend needs to be exact about.
    Ok(u32::try_from(written).unwrap_or(u32::MAX))
}

/// The proposals a room holds, newest first, with both ends resolved.
///
/// `sources` is the room's membership and follows [`EntityFilter`]'s
/// convention: **empty means every source**. A proposal belongs to a room when
/// either of its ends does.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if a query fails.
pub async fn room_suggestions_inner(
    pool: &PgPool,
    sources: &[String],
    limit: u32,
) -> Result<SuggestionPage, IpcError> {
    Ok(SuggestionPage {
        rows: suggest::proposals(pool, sources, i64::from(limit)).await?,
        total: suggest::proposal_count(pool, sources).await?,
    })
}

/// Accept a proposal, making it an ordinary link.
///
/// `Ok(None)` when there was nothing to accept -- it was accepted or dismissed
/// already. Nothing was mutated, so nothing is written to the log and nothing
/// is announced, exactly as [`unlink_inner`] does.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `link_id` is not a UUID;
/// [`NotFound`](crate::IpcErrorCode::NotFound) if no link carries it;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn accept_suggestion_inner(
    pool: &PgPool,
    link_id: &str,
) -> Result<Option<LinkMutation>, IpcError> {
    let id = suggestion_id(link_id)?;
    let Some(link) = suggest::accept(pool, id).await? else {
        return Ok(None);
    };
    let activity = record_link_activity(pool, "accepted", &link).await?;
    Ok(Some(LinkMutation { link, activity }))
}

/// Dismiss a proposal, and remember it.
///
/// The tombstone is the withdrawal memory Links v1 already had, so a dismissal
/// suppresses re-proposal for exactly the same reason an unlink does -- there
/// is one mechanism, not two.
///
/// `Ok(None)` when there was nothing to dismiss, which includes a link that has
/// already been accepted: that is a link, and the panel's *Unlink* is what
/// withdraws it.
///
/// # Errors
///
/// As [`accept_suggestion_inner`].
pub async fn dismiss_suggestion_inner(
    pool: &PgPool,
    link_id: &str,
) -> Result<Option<LinkMutation>, IpcError> {
    let id = suggestion_id(link_id)?;
    let Some(link) = suggest::dismiss(pool, id).await? else {
        return Ok(None);
    };
    let activity = record_link_activity(pool, "dismissed", &link).await?;
    Ok(Some(LinkMutation { link, activity }))
}

/// A suggestion is addressed by its link id, because it is a link row.
fn suggestion_id(link_id: &str) -> Result<Uuid, IpcError> {
    link_id
        .parse()
        .map_err(|_| IpcError::invalid(format!("{link_id} is not a link id")))
}

/// Run a detection pass over the mirror.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`detect_suggestions_inner`] refuses with.
#[tauri::command]
pub async fn detect_suggestions(lifecycle: State<'_, Lifecycle>) -> Result<u32, IpcError> {
    let pool = lifecycle.pool()?;
    detect_suggestions_inner(&pool).await
}

/// One page of a room's suggestion tray.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`room_suggestions_inner`] refuses with.
#[tauri::command]
pub async fn room_suggestions(
    lifecycle: State<'_, Lifecycle>,
    sources: Vec<String>,
    limit: u32,
) -> Result<SuggestionPage, IpcError> {
    let pool = lifecycle.pool()?;
    room_suggestions_inner(&pool, &sources, limit).await
}

/// Accept a proposal, and announce it.
///
/// Idempotent: accepting an already-accepted suggestion resolves, writes no
/// second line and announces nothing.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`accept_suggestion_inner`] refuses with.
#[tauri::command]
pub async fn accept_suggestion<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    link_id: String,
) -> Result<(), IpcError> {
    let pool = lifecycle.pool()?;
    if let Some(written) = accept_suggestion_inner(&pool, &link_id).await? {
        announce(&app, written.activity);
    }
    Ok(())
}

/// Dismiss a proposal, and announce it.
///
/// Idempotent, for the same reason [`accept_suggestion`] is.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`dismiss_suggestion_inner`] refuses with.
#[tauri::command]
pub async fn dismiss_suggestion<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    link_id: String,
) -> Result<(), IpcError> {
    let pool = lifecycle.pool()?;
    if let Some(written) = dismiss_suggestion_inner(&pool, &link_id).await? {
        announce(&app, written.activity);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIRROR: &str = include_str!("../../../../app/src/lib/ipc/entity.ts");

    /// Every command this module registers is invoked by that name from the
    /// mirror, and every command the mirror invokes is registered.
    ///
    /// A `#[tauri::command]` is addressed by a *string*, so a rename on one
    /// side is not a compile error anywhere -- it is a button that rejects with
    /// Tauri's own "command not found" the first time somebody presses it. The
    /// list is read off `lib.rs`'s handler barrel rather than written out here,
    /// so a command registered and never mirrored fails this too.
    #[test]
    fn the_mirror_invokes_the_commands_by_their_registered_names() {
        let barrel = include_str!("../lib.rs");
        let registered: Vec<&str> = barrel
            .lines()
            .filter_map(|line| line.trim().strip_prefix("commands::entity::"))
            .map(|line| line.trim_end_matches(','))
            .collect();
        assert!(
            registered.len() >= 9,
            "only {} entity commands found in the handler barrel -- the parse \
             is wrong, not the barrel",
            registered.len()
        );
        for command in &registered {
            assert!(
                MIRROR.contains(&format!("\"{command}\"")),
                "{command} is registered but never invoked from entity.ts"
            );
        }
        for invoked in MIRROR.split("invoke<").skip(1) {
            let name = invoked
                .split_once('"')
                .and_then(|(_, rest)| rest.split_once('"'))
                .map(|(name, _)| name)
                .expect("every invoke names a command");
            assert!(
                registered.contains(&name),
                "entity.ts invokes {name}, which is not in the handler barrel"
            );
        }
    }

    /// §3a, as a resolution rather than as a claim.
    ///
    /// The whole promise is *"a new source's items get grouped, chipped and
    /// labeled without touching core"*, and it holds only if this lookup
    /// actually reaches the adapter's own declaration. The mock declares
    /// `ticket`, so a mock ticket must come back with the mock's words --
    /// which the frontend's humaniser could not produce (it has no `plural`
    /// and no monogram to invent).
    #[test]
    fn a_declared_kind_resolves_to_the_adapters_own_metadata() {
        let mock = crate::sources::Registry::builtin()
            .templates()
            .into_iter()
            .find(|template| template.adapter_kind == "mock")
            .expect("the mock adapter is compiled in");
        let declared = mock
            .entity_kinds
            .first()
            .expect("the mock declares at least one kind")
            .clone();

        let resolved =
            kind_info_for(Some("mock"), &declared.id).expect("a kind the mock declares resolves");
        assert_eq!(resolved.id, declared.id);
        assert_eq!(resolved.label, declared.label);
        assert_eq!(resolved.plural, declared.plural);
        assert_eq!(resolved.monogram, declared.monogram);
    }

    /// Three ways to have nothing to say, and all three say nothing.
    ///
    /// The middle one is the case that matters operationally: `run_once`
    /// writes mirror rows for a source with **no configuration row**, so
    /// `adapter_kind` is genuinely `None` there and a lookup that unwrapped it
    /// would panic on a perfectly ordinary corpus.
    #[test]
    fn nothing_declared_resolves_to_none_rather_than_to_a_guess() {
        assert!(kind_info_for(Some("mock"), "no-such-kind").is_none());
        assert!(kind_info_for(None, "ticket").is_none());
        assert!(kind_info_for(Some("not-a-compiled-in-adapter"), "ticket").is_none());
    }

    /// One kind id declared by two adapters resolves by **adapter**, not by
    /// kind.
    ///
    /// `jira` and `mock` both declare `ticket` today. A lookup that searched
    /// every template for the kind would hand a Jira ticket the mock's words
    /// (or the reverse) depending only on table order -- which is exactly the
    /// per-adapter table §3a exists to avoid, built by accident.
    #[test]
    fn two_adapters_declaring_one_kind_do_not_fight() {
        let templates = crate::sources::Registry::builtin().templates();
        let declaring: Vec<&knobas_source::SourceDescriptor> = templates
            .iter()
            .filter(|template| template.entity_kinds.iter().any(|info| info.id == "ticket"))
            .collect();
        assert!(
            declaring.len() >= 2,
            "only {} adapter(s) declare `ticket`, so this test proves nothing",
            declaring.len()
        );

        for template in declaring {
            let resolved = kind_info_for(Some(&template.adapter_kind), "ticket")
                .expect("the adapter declares `ticket`");
            let expected = template
                .entity_kinds
                .iter()
                .find(|info| info.id == "ticket")
                .expect("it is in the list this loop filtered on");
            // Field by field: `KindInfo` has no `PartialEq`, and adding one
            // would be an edit to the frozen SPI for a test's convenience.
            assert_eq!(
                (
                    resolved.label.as_str(),
                    resolved.plural.as_str(),
                    resolved.monogram.as_str(),
                    resolved.full_sync_exhaustive
                ),
                (
                    expected.label.as_str(),
                    expected.plural.as_str(),
                    expected.monogram.as_str(),
                    expected.full_sync_exhaustive
                ),
                "`ticket` from {} resolved to another adapter's declaration",
                template.adapter_kind
            );
        }
    }

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
