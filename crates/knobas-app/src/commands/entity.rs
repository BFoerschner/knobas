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
    /// Where this row sits **inside its source**, as one line -- a Confluence
    /// page's ancestor path, `Engineering \u{203a} Payments` (#284, ratified
    /// as a contract §10.8 exception under that ticket's criterion 5).
    ///
    /// `None` for every row whose record carries no readable `ancestors`,
    /// which is every kind but a page today: the ADR-0007 **miss**. Joined by
    /// `knobas_core::ancestor_path_read!`, the one statement that spells it,
    /// so this row and the launcher's cannot disagree about what a path is.
    ///
    /// This struct and `knobas_search::EntityRow` are pinned to **one wire
    /// shape** by `commands::search`'s own test; the field is on both.
    pub path: Option<String>,
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
    /// Only members of this stored context (`ctx:<id>`), by the fixed one-hop
    /// rule (§16.11, ADR-0008). `None` is unscoped -- every derived room. The
    /// membership is resolved server-side per read rather than shipped as an
    /// id list, so a page and its `total` always describe the same instant.
    pub context: Option<String>,
    /// Only items carrying this project key, in the source's own word
    /// (ADR-0010, issue #208). `None` is unscoped, the same "empty means
    /// unfiltered" the two lists above have.
    ///
    /// It narrows **within** [`Self::sources`] and never instead of them: a
    /// project key is unique only inside its own source, so a project room
    /// sets both and a filter carrying the key alone would union two sources'
    /// `PAY` into one room. Where a key is read from is the **source's** to
    /// say since #277, and it is the same read the census behind the
    /// switcher's project rooms takes -- one macro over one declaration, so a
    /// room can never disagree with the list of rooms about what a project
    /// is.
    ///
    /// A record whose payload carries no readable project key matches no
    /// value of this field, so it is in no project room and still in *All
    /// work* and its source's room: absence, never a wrong room.
    pub project: Option<String>,
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
const LIVE_UPDATED: &str = concat!(
    r#"
select i.entity_id, i.source_id, i.kind, i.title, i.item_updated_at, i.synced_at,
       "#,
    knobas_core::ancestor_path_read!("i.payload"),
    r#" as path,
       count(*) over () as total
  from sync.live_item i
 where ($1::text[] is null or i.source_id = any($1))
   and ($2::text[] is null or i.kind      = any($2))
   and ($3::int    is null or i.item_updated_at >= now() - make_interval(days => $3))
   and ($6::text[] is null or i.entity_id = any($6))
   and ($7::text   is null or "#,
    knobas_core::project_key_read!("$8"),
    r#" = $7)
 order by i.item_updated_at desc nulls last, i.entity_id
 limit $4 offset $5
"#
);

/// The live corpus, alphabetically. A second statement rather than an
/// interpolated `order by`.
const LIVE_TITLE: &str = concat!(
    r#"
select i.entity_id, i.source_id, i.kind, i.title, i.item_updated_at, i.synced_at,
       "#,
    knobas_core::ancestor_path_read!("i.payload"),
    r#" as path,
       count(*) over () as total
  from sync.live_item i
 where ($1::text[] is null or i.source_id = any($1))
   and ($2::text[] is null or i.kind      = any($2))
   and ($3::int    is null or i.item_updated_at >= now() - make_interval(days => $3))
   and ($6::text[] is null or i.entity_id = any($6))
   and ($7::text   is null or "#,
    knobas_core::project_key_read!("$8"),
    r#" = $7)
 order by i.title asc, i.entity_id
 limit $4 offset $5
"#
);

/// As [`LIVE_UPDATED`], reaching past the tombstone filter.
///
/// The join is `sync.live_item` without its hiding reasons — `deleted_at is
/// null` and, since migration `0012`, the disabled-source filter. Both are
/// dropped on purpose: an `include_deleted` read lists a disabled source's
/// rows too, ratified in the #202 §10.8 entry.
const ALL_UPDATED: &str = concat!(
    r#"
select i.entity_id, i.source_id, i.kind, i.title, i.item_updated_at, i.synced_at,
       "#,
    knobas_core::ancestor_path_read!("i.payload"),
    r#" as path,
       count(*) over () as total
  from sync.item i
  join knobas.entity e on e.id = i.entity_id
 where ($1::text[] is null or i.source_id = any($1))
   and ($2::text[] is null or i.kind      = any($2))
   and ($3::int    is null or i.item_updated_at >= now() - make_interval(days => $3))
   and ($6::text[] is null or i.entity_id = any($6))
   and ($7::text   is null or "#,
    knobas_core::project_key_read!("$8"),
    r#" = $7)
 order by i.item_updated_at desc nulls last, i.entity_id
 limit $4 offset $5
"#
);

/// As [`LIVE_TITLE`], reaching past the tombstone filter.
const ALL_TITLE: &str = concat!(
    r#"
select i.entity_id, i.source_id, i.kind, i.title, i.item_updated_at, i.synced_at,
       "#,
    knobas_core::ancestor_path_read!("i.payload"),
    r#" as path,
       count(*) over () as total
  from sync.item i
  join knobas.entity e on e.id = i.entity_id
 where ($1::text[] is null or i.source_id = any($1))
   and ($2::text[] is null or i.kind      = any($2))
   and ($3::int    is null or i.item_updated_at >= now() - make_interval(days => $3))
   and ($6::text[] is null or i.entity_id = any($6))
   and ($7::text   is null or "#,
    knobas_core::project_key_read!("$8"),
    r#" = $7)
 order by i.title asc, i.entity_id
 limit $4 offset $5
"#
);

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
    declarations: &knobas_core::payload::Declarations,
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

    // Membership is a set of ids, resolved by the store that owns the rule
    // (§16.11): the statements above stay static SQL, and a stored context
    // scopes them by one more nullable parameter. `Some` of an empty set is a
    // context with no members, whose room is honestly empty -- not unscoped.
    let members = match filter.context.as_deref() {
        Some(ctx) => Some(knobas_core::context::member_ids(pool, ctx).await?),
        None => None,
    };

    let rows = sqlx::query(statement(filter))
        .bind(any_of(&filter.sources))
        .bind(any_of(&filter.kinds))
        .bind(within)
        .bind(i64::from(limit))
        .bind(i64::from(offset))
        .bind(members)
        .bind(filter.project.as_deref())
        .bind(declarations.as_param())
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
                path: row.get("path"),
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
    let declarations = declared_paths(&pool).await?;
    list_entities_inner(&pool, &filter, limit, offset, &declarations).await
}

/// Every configured source's declared payload paths (#277).
///
/// **`Registry::builtin()` rather than `SourcesState`'s trait object**, for the
/// reason `Registry::templates` exists as an inherent method at all: a
/// declaration is a property of the adapter *kind*, held in a `const` table
/// compiled into this binary, so a read that wants one has nothing to wait for.
/// Reaching through `SourcesState` would make a room's list, the mini board and
/// the project census answer `not_ready` until the scheduler is up, which is
/// later than the pool -- a widening of `not_ready` bought for nothing. The
/// reads that already hold a `SourcesState` (the inbox, the merge pass) pass
/// its registry instead, so the injected one is still what a test drives.
///
/// # Errors
///
/// `internal` if the source listing fails.
async fn declared_paths(pool: &PgPool) -> Result<knobas_core::payload::Declarations, IpcError> {
    crate::sources::paths::declared_paths(pool, &crate::sources::Registry::builtin()).await
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
    /// Whether the user has this source turned on, **as of this read** --
    /// derived in the [`DETAIL`] statement, never stored (issue #204).
    ///
    /// The detail reaches past `sync.live_item` on purpose (§5a), so it opens
    /// entities every other reader hides -- and since migration `0012` there
    /// are two reasons a reader hides one. `deleted_at` marks the first
    /// (withdrawn upstream); this marks the second (turned off by the user),
    /// and the banner needs both because the remedies differ: nothing undoes
    /// a withdrawal, one click undoes a disable -- which is also why it is
    /// derived: a stored marker would still say "off" after that click.
    ///
    /// `true` for a source with no configuration row, matching `0012`'s
    /// `coalesce(enabled, true)`: absence of configuration is not a decision
    /// the user made.
    pub enabled: bool,
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
/// The join is `sync.live_item`'s minus **both** of its filters, deliberately:
/// §5a says a withdrawn entity must still open, and since migration `0012` a
/// disabled source's must too -- links and notes point at either. Each dropped
/// filter leaves its marker instead: `e.deleted_at` for the withdrawn banner,
/// `coalesce(c.enabled, true)` for the turned-off one (issue #204).
/// `source_config` is a **left** join because `run_once` syncs unconfigured
/// sources.
const DETAIL: &str = concat!(
    r#"
select i.entity_id, i.source_id, i.kind, i.title, i.body_text, i.author,
       i.item_updated_at, i.synced_at, i.payload, i.web_url,
       "#,
    knobas_core::ancestor_path_read!("i.payload"),
    r#" as path,
       e.deleted_at,
       c.display_name, c.kind as adapter_kind,
       coalesce(c.enabled, true) as source_enabled
  from sync.item i
  join knobas.entity e on e.id = i.entity_id
  left join knobas.source_config c on c.id = i.source_id
 where i.entity_id = $1
"#
);

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
            // The detail panel draws the same path the launcher row does, out
            // of the same statement -- the panel *has* the payload, but
            // reading it here would be a second spelling of "where is this",
            // and ADR-0007 requirement 2 exists to stop exactly that.
            path: row.get("path"),
        },
        source: SourceRef {
            display_name: display_name.unwrap_or_else(|| source_id.clone()),
            adapter_kind: adapter_kind.unwrap_or_else(|| source_id.clone()),
            id: source_id,
            enabled: row.get("source_enabled"),
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
    /// The proposal this link displaced, when there was one (#70).
    ///
    /// Drawing the *reverse* of a live proposal withdraws it -- see
    /// [`knobas_core::suggest::resolve_edge`] -- and a proposal leaving the tray
    /// is a mutation, so it gets its own line rather than disappearing quietly.
    /// `None` everywhere else, which is every other path in this module.
    pub superseded: Option<ActivityRow>,
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

    // The pair may already carry a *proposal*: `link_pair_active_idx` spans
    // proposals and links alike (one active edge per pair per relation, whatever
    // its state), so knobas suggesting this link is what would refuse it. The
    // proposal is therefore answered *before* the insert rather than after it
    // fails -- a unique violation aborts the transaction it happens in, and
    // there would be nothing left to answer it with.
    //
    // One transaction, because `Superseded` is only half a mutation: the
    // withdrawn proposal and the link that replaced it land together or not at
    // all.
    let mut tx = pool.begin().await.map_err(knobas_core::CoreError::from)?;
    let answered = suggest::resolve_edge(&mut tx, &from, &to, &relation).await?;

    let (link, superseded) = match answered {
        // Drawing by hand the link knobas proposed is the same act as pressing
        // *Accept*, and the alternative is telling the user "already linked"
        // about a pair whose links panel is empty.
        suggest::Edge::Promoted(link) => (link, None),
        // The user drew the proposal's reverse, which contradicts its direction.
        // The user wins: the proposal is withdrawn and the link they drew is
        // written with the ends they gave it.
        suggest::Edge::Superseded(proposal) => {
            let line = record_link_activity_with(&mut tx, "dismissed", &proposal).await?;
            let link = write_link(&mut tx, &from, &to, &relation, note).await?;
            (link, Some(line))
        }
        // Nothing proposed for this pair. Whatever refuses the write now is a
        // real link, which is what `conflict` has always meant here.
        suggest::Edge::Open => (
            write_link(&mut tx, &from, &to, &relation, note).await?,
            None,
        ),
    };

    let activity = record_link_activity_with(&mut tx, "linked", &link).await?;
    tx.commit().await.map_err(knobas_core::CoreError::from)?;
    Ok(LinkMutation {
        link,
        activity,
        superseded,
    })
}

/// The hand-drawn link itself, once the tray has been answered.
async fn write_link(
    conn: &mut sqlx::PgConnection,
    from: &EntityRef,
    to: &EntityRef,
    relation: &str,
    note: Option<&str>,
) -> Result<LinkRow, IpcError> {
    Ok(knobas_core::link::create_with(
        conn,
        from,
        to,
        relation,
        Origin::Manual,
        present(note),
        ACTOR,
    )
    .await?)
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
    Ok(Some(LinkMutation {
        link,
        activity,
        superseded: None,
    }))
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

/// [`record_link_activity`], against an executor the caller chooses, so a line
/// can be written inside the transaction that made the mutation it describes.
async fn record_link_activity_with(
    conn: &mut sqlx::PgConnection,
    verb: &str,
    link: &LinkRow,
) -> Result<ActivityRow, IpcError> {
    let from = EntityRef::parse(&link.from_id).map_err(IpcError::internal)?;
    Ok(
        knobas_core::activity::record_with(conn, ACTOR, verb, Some(&from), link_detail(link))
            .await?,
    )
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
    // The displaced proposal first, so the strip reads in the order the two
    // things happened rather than in the order this function holds them.
    if let Some(line) = written.superseded {
        announce(&app, line);
    }
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
/// `sources` is a derived room's membership and follows [`EntityFilter`]'s
/// convention: **empty means every source**. A proposal belongs to a room when
/// either of its ends does.
///
/// `ctx` is a stored context's room (#47): its membership -- the fixed
/// one-hop rule, resolved by the store that owns it -- plus the context's own
/// entity, so a proposed *add to this context* surfaces in the room it would
/// add to. Membership is computed from confirmed links only (ADR-0008), which
/// is what keeps "the tray scopes by membership" from becoming circular with
/// "membership is built from links".
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if a query fails.
pub async fn room_suggestions_inner(
    pool: &PgPool,
    sources: &[String],
    ctx: Option<&str>,
    limit: u32,
) -> Result<SuggestionPage, IpcError> {
    let members = match ctx {
        Some(ctx) => {
            let mut ids = knobas_core::context::member_ids(pool, ctx).await?;
            ids.push(ctx.to_owned());
            Some(ids)
        }
        None => None,
    };
    let members = members.as_deref();
    Ok(SuggestionPage {
        rows: suggest::proposals(pool, sources, members, i64::from(limit)).await?,
        total: suggest::proposal_count(pool, sources, members).await?,
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
    Ok(Some(LinkMutation {
        link,
        activity,
        superseded: None,
    }))
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
    Ok(Some(LinkMutation {
        link,
        activity,
        superseded: None,
    }))
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
    ctx: Option<String>,
    limit: u32,
) -> Result<SuggestionPage, IpcError> {
    let pool = lifecycle.pool()?;
    room_suggestions_inner(&pool, &sources, ctx.as_deref(), limit).await
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

// -- the start-work flow ----------------------------------------------------
//
// Six commands, in this module and not a new one -- the `commands/` + `ipc/`
// layout is frozen, and the flow starts from a ticket *entity*, which is what
// this module is about. They are thin: everything they decide lives in
// `crate::start_work`, where a test can reach it without a Tauri app.
//
// **None of them is a write path of its own.** Every side effect the flow has
// is an existing `WriteOp` going through `submit_write`'s queue, or a link
// going through `create_link_inner` -- the same two doors the *Comment* button
// and the *Link to...* dialog use.

/// The flow for a ticket, proposing one if there is none and `repo_id` says
/// where it would go.
///
/// One command rather than a read and a create, because the address
/// `#/start-work/<key>` has to answer both questions at once: *is there a flow,
/// and if not, what would one look like?* An empty answer means there is no
/// flow and no repository was named -- which is the state where the view asks
/// the user to pick one.
///
/// Proposing does **not** dispatch anything. The whole sequence is composed and
/// stored so it can be shown before anything happens; `start_work_run` is what
/// performs it.
///
/// # Errors
///
/// `invalid` for an id that is not an entity id; `not_found` if the ticket is
/// not in the mirror; `not_ready` before bring-up.
#[tauri::command]
pub async fn start_work_flow(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
    repo_id: Option<String>,
) -> Result<Vec<knobas_core::start_work::FlowStep>, IpcError> {
    let pool = lifecycle.pool()?;
    let ticket = EntityRef::parse(&entity_id).map_err(IpcError::invalid)?;
    let existing = knobas_core::start_work::flow(&pool, &ticket).await?;
    if !existing.is_empty() {
        return Ok(existing);
    }
    let Some(repo_id) = repo_id else {
        return Ok(Vec::new());
    };
    let repo = EntityRef::parse(&repo_id).map_err(IpcError::invalid)?;
    crate::start_work::begin(&pool, &ticket, &repo).await
}

/// Run the flow as far as it will go, and answer where it stopped.
///
/// # Errors
///
/// `invalid` for an id that is not an entity id; `not_ready` before bring-up.
#[tauri::command]
pub async fn start_work_run<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
) -> Result<Vec<knobas_core::start_work::FlowStep>, IpcError> {
    let state = crate::sources::state(&app)?;
    let ticket = EntityRef::parse(&entity_id).map_err(IpcError::invalid)?;
    crate::start_work::run(
        &state.pool,
        &crate::start_work::queue::Queue { state: &state },
        &ticket,
    )
    .await
}

/// Retry one step, without redoing the ones that succeeded.
///
/// # Errors
///
/// `not_found` if no step carries the id; `conflict` if an earlier step is
/// where the flow stopped; `not_ready` before bring-up.
#[tauri::command]
pub async fn start_work_retry<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    step_id: i64,
) -> Result<Vec<knobas_core::start_work::FlowStep>, IpcError> {
    let state = crate::sources::state(&app)?;
    crate::start_work::retry(
        &state.pool,
        &crate::start_work::queue::Queue { state: &state },
        step_id,
    )
    .await
}

/// Skip one step, so a ticket that needs no branch still gets its status moved.
///
/// # Errors
///
/// `not_found` if no step carries the id; `not_ready` before bring-up.
#[tauri::command]
pub async fn start_work_skip<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    step_id: i64,
) -> Result<Vec<knobas_core::start_work::FlowStep>, IpcError> {
    let state = crate::sources::state(&app)?;
    crate::start_work::skip(
        &state.pool,
        &crate::start_work::queue::Queue { state: &state },
        step_id,
    )
    .await
}

/// Replace a step's proposal with the one the user edited.
///
/// `payload` is the step's own stored value, edited -- the same shape
/// `amend_write` takes, and untyped on the wire for the same reason: `WriteOp`
/// grows per milestone (ADR-0006), so typing the argument would drag the SPI's
/// enum onto the IPC surface.
///
/// # Errors
///
/// `not_found` if no step carries the id; `conflict` if the step has already
/// happened; `not_ready` before bring-up.
#[tauri::command]
pub async fn start_work_amend(
    lifecycle: State<'_, Lifecycle>,
    step_id: i64,
    payload: serde_json::Value,
) -> Result<Vec<knobas_core::start_work::FlowStep>, IpcError> {
    let pool = lifecycle.pool()?;
    crate::start_work::repropose(&pool, step_id, payload).await
}

/// The reverse direction: move every ticket whose linked pull request has been
/// merged, and answer how many moved.
///
/// A pass over the mirror, invoked the way `detect_suggestions` is -- there is
/// no per-item hook in the sync engine, and adding one to serve this would be a
/// second mechanism. The count is what the shell announces, so an automatic
/// change is visible rather than mysterious.
///
/// # Errors
///
/// `not_ready` before bring-up; `internal` if the pass's own read fails.
#[tauri::command]
pub async fn follow_merges<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<u32, IpcError> {
    let state = crate::sources::state(&app)?;
    let declarations =
        crate::sources::paths::declared_paths(&state.pool, state.registry.as_ref()).await?;
    crate::start_work::merge::follow_merges(
        &state.pool,
        &crate::start_work::queue::Queue { state: &state },
        crate::start_work::plan::IN_REVIEW,
        &declarations,
    )
    .await
}

// -- the inbox (issue #45) --------------------------------------------------
//
// Four commands, all here in the existing `entity` module: inbox items are
// derived from entities and its two write commands act on them, and the
// `commands/` + `ipc/` module layout is frozen -- no new module on either
// side. There is deliberately **no fifth command that acts on a source**: an
// inbox action that changes something at a source is a `WriteOp` through
// `submit_write` above, which is #43's command over #42's queue, and the inbox
// introduces no write path of its own.

/// The identity behind `@me`, loaded the way search loads it.
///
/// `knobas_search::Vocabulary` is where "who am I" lives -- the union of every
/// enabled source's configured username -- and the inbox is an identity
/// feature, so it reads that and not a second mechanism. The kind catalog is
/// the default one because the identity is the only field wanted here; loading
/// it costs the same single query every search already makes.
async fn identity_of(pool: &PgPool) -> Result<Vec<String>, IpcError> {
    Ok(
        knobas_search::Vocabulary::load(pool, knobas_search::KindCatalog::default())
            .await
            .map_err(IpcError::internal)?
            .identity,
    )
}

/// [`inbox_items`], against a pool and a clock.
///
/// # Errors
///
/// `internal` if the derivation, the identity read or the source listing
/// fails.
pub async fn inbox_items_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    now: DateTime<Utc>,
    shelf: knobas_core::inbox::Shelf,
) -> Result<Vec<crate::inbox::InboxEntry>, IpcError> {
    let identity = identity_of(pool).await?;
    crate::inbox::stream(pool, registry, &identity, now, shelf).await
}

/// The inbox: one actionable stream, newest first.
///
/// `shelf` is `stream` -- what needs you now -- or `snoozed`, what you
/// deferred and when it comes back. Both are the same derivation with the same
/// predicate, asked for one shelf or the other.
///
/// Every entry carries the actions its source can really perform: an op the
/// adapter does not declare is absent rather than offered and failing.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database or the sync engine is still coming up, `internal` for a read
/// failure.
#[tauri::command]
pub async fn inbox_items<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    shelf: knobas_core::inbox::Shelf,
) -> Result<Vec<crate::inbox::InboxEntry>, IpcError> {
    let state = crate::sources::state(&app)?;
    inbox_items_inner(&state.pool, state.registry.as_ref(), Utc::now(), shelf).await
}

/// How many items need you now -- the number the top strip shows.
///
/// **Snoozed items are not in it**, because the number means "needs me now";
/// neither are items marked done. It is the stream's own statement, counted,
/// so the badge cannot disagree with the view it opens.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, `internal` for a read failure.
#[tauri::command]
pub async fn inbox_count(lifecycle: State<'_, Lifecycle>) -> Result<i64, IpcError> {
    let pool = lifecycle.pool()?;
    inbox_count_inner(&pool, &crate::sources::Registry::builtin(), Utc::now()).await
}

/// [`inbox_count`], against a pool and a clock.
///
/// # Errors
///
/// `internal` if the count or the identity read fails.
pub async fn inbox_count_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    now: DateTime<Utc>,
) -> Result<i64, IpcError> {
    let identity = identity_of(pool).await?;
    let declarations = crate::sources::paths::declared_paths(pool, registry).await?;
    knobas_core::inbox::count(pool, &identity, now, &declarations)
        .await
        .map_err(IpcError::internal)
}

/// [`snooze_inbox_item`], against a pool and a clock.
///
/// # Errors
///
/// As [`snooze_inbox_item`].
pub async fn snooze_inbox_item_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    now: DateTime<Utc>,
    item_key: &str,
    until: DateTime<Utc>,
) -> Result<ActivityRow, IpcError> {
    let identity = identity_of(pool).await?;
    let declarations = crate::sources::paths::declared_paths(pool, registry).await?;
    crate::inbox::answer(
        pool,
        &identity,
        now,
        item_key,
        crate::inbox::Answer::Snooze(until),
        &declarations,
    )
    .await
}

/// Not now -- come back on this date (#45, stories 13-15).
///
/// `until` is an absolute moment, because the presets (*tomorrow*, *next
/// Monday*, *after the credential expires*) are the caller's arithmetic and a
/// date picker is the general case of the same argument. Snoozing something
/// already snoozed moves its date.
///
/// Refuses an item that is on neither shelf: the key comes from a list the
/// webview has been holding, and an answer to something since resolved at the
/// source would write a durable row about work that no longer exists.
///
/// Writes one activity line and emits `activity:new`, because *every* inbox
/// action is recorded.
///
/// # Errors
///
/// `not_found` for an item on neither shelf,
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// `internal` for a write failure.
#[tauri::command]
pub async fn snooze_inbox_item<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    item_key: String,
    until: DateTime<Utc>,
) -> Result<(), IpcError> {
    let pool = lifecycle.pool()?;
    let written = snooze_inbox_item_inner(
        &pool,
        &crate::sources::Registry::builtin(),
        Utc::now(),
        &item_key,
        until,
    )
    .await?;
    announce(&app, written);
    Ok(())
}

/// [`complete_inbox_item`], against a pool and a clock.
///
/// # Errors
///
/// As [`complete_inbox_item`].
pub async fn complete_inbox_item_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    now: DateTime<Utc>,
    item_key: &str,
) -> Result<ActivityRow, IpcError> {
    let identity = identity_of(pool).await?;
    let declarations = crate::sources::paths::declared_paths(pool, registry).await?;
    crate::inbox::answer(
        pool,
        &identity,
        now,
        item_key,
        crate::inbox::Answer::Done,
        &declarations,
    )
    .await
}

/// I handled this (#45, story 16).
///
/// The item leaves the stream, and **comes back if its subject moves again** --
/// *done* is stored as the moment it was answered and compared against the
/// item's own, so marking a mention done hides it until somebody says
/// something new rather than muting the ticket for ever.
///
/// Writes one activity line and emits `activity:new`.
///
/// # Errors
///
/// As [`snooze_inbox_item`].
#[tauri::command]
pub async fn complete_inbox_item<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    item_key: String,
) -> Result<(), IpcError> {
    let pool = lifecycle.pool()?;
    let written = complete_inbox_item_inner(
        &pool,
        &crate::sources::Registry::builtin(),
        Utc::now(),
        &item_key,
    )
    .await?;
    announce(&app, written);
    Ok(())
}

/// Which inbox categories may raise a desktop notification (#290).
///
/// Empty for a profile nobody has switched one on in, which is what makes the
/// feature opt-in per kind (spec #272, story 71).
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the read
/// fails.
#[tauri::command]
pub async fn notification_kinds(
    lifecycle: State<'_, Lifecycle>,
) -> Result<Vec<knobas_core::inbox::Category>, IpcError> {
    let pool = lifecycle.pool()?;
    crate::inbox::notification_kinds(&pool).await
}

/// Set which inbox categories may notify, and answer with what is now stored.
///
/// Takes the words rather than the enum, the shape [`unlink`] takes its id in
/// and for the same reason: a value Tauri itself cannot deserialize is
/// rejected with a bare string and no [`IpcErrorCode`](crate::IpcErrorCode),
/// so a category this build does not know reads to the settings section as a
/// window that broke rather than as `invalid` with a sentence in it.
///
/// **Nothing here asks about the OS permission.** Requesting it is the
/// settings surface's, on the click that switches the first kind on (story
/// 72); this command records a *preference*, and a preference that could only
/// be stored while some other system said yes would be a setting that
/// silently forgot itself.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) for a word that is
/// not an inbox category,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the write
/// fails.
#[tauri::command]
pub async fn set_notification_kinds(
    lifecycle: State<'_, Lifecycle>,
    kinds: Vec<String>,
) -> Result<Vec<knobas_core::inbox::Category>, IpcError> {
    let pool = lifecycle.pool()?;
    crate::inbox::set_notification_kinds(&pool, &kinds).await
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
            context: None,
            project: None,
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

// -- contexts (#47) ---------------------------------------------------------
//
// In this module rather than one of their own: §10.8 freezes the `commands/`
// + `ipc/` module layout ("no new module"), and contexts are the entity
// stream's own objects — linkable, listable, and read by the same surfaces.

/// The switcher's list: every unarchived context, newest first.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, [`Internal`](crate::IpcErrorCode::Internal)
/// for a query failure.
#[tauri::command]
pub async fn list_contexts(
    lifecycle: State<'_, Lifecycle>,
) -> Result<Vec<knobas_core::context::ContextRow>, IpcError> {
    let pool = lifecycle.pool()?;
    Ok(knobas_core::context::list(&pool).await?)
}

/// Create an ad-hoc context from a label.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if the label is blank -- a room
/// with no name is not addressable by a person;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn create_context_inner(
    pool: &PgPool,
    title: &str,
) -> Result<knobas_core::context::ContextRow, IpcError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(IpcError::invalid("a context needs a label"));
    }
    let row = knobas_core::context::create_adhoc(pool, title).await?;
    record(pool, "created", &row).await?;
    Ok(row)
}

/// Promote an entity to a context of its own (spec §7).
///
/// Idempotent by the store's own promise: promoting twice answers with the one
/// context, and only the first call writes an activity line -- the second
/// mutated nothing, so it announces nothing.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `entity_id` is not an entity
/// id, or if it is itself a context;
/// [`NotFound`](crate::IpcErrorCode::NotFound) if nothing local carries it;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn promote_context_inner(
    pool: &PgPool,
    entity_id: &str,
) -> Result<knobas_core::context::Promoted, IpcError> {
    let anchor = EntityRef::parse(entity_id).map_err(IpcError::invalid)?;
    let promoted = knobas_core::context::promote(pool, &anchor)
        .await?
        .ok_or_else(|| IpcError::not_found(format!("{anchor} is not in the local index")))?;
    // One line per *mutation*: the store says which call inserted, so a
    // re-promotion answers with the room and writes nothing.
    if promoted.fresh {
        record(pool, "promoted", &promoted.context).await?;
    }
    Ok(promoted)
}

/// Who is in this context, by the fixed rule (§16.11, ADR-0008).
///
/// The per-context inbox filter ("3 here") intersects the inbox stream with
/// this set on the frontend, so the badge and the list it filters are drawn
/// from the same rows by construction.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, [`Internal`](crate::IpcErrorCode::Internal)
/// for a query failure.
#[tauri::command]
pub async fn context_members(
    lifecycle: State<'_, Lifecycle>,
    ctx_id: String,
) -> Result<Vec<String>, IpcError> {
    let pool = lifecycle.pool()?;
    Ok(knobas_core::context::member_ids(&pool, &ctx_id).await?)
}

/// One activity line per context mutation, on the context's own entity.
async fn record(
    pool: &PgPool,
    verb: &str,
    row: &knobas_core::context::ContextRow,
) -> Result<(), IpcError> {
    let entity = EntityRef::parse(&row.id).map_err(IpcError::internal)?;
    let mut detail = serde_json::json!({
        "context_id": row.id,
        "kind": row.kind,
        "title": row.title,
    });
    // Absent rather than null for an ad-hoc context, the discipline
    // `link_detail` records: a reader of the log should not meet a key that
    // says "there is nothing here".
    if let (Some(map), Some(anchor)) = (detail.as_object_mut(), row.anchor_id.as_deref()) {
        map.insert("anchor_id".to_owned(), serde_json::Value::from(anchor));
    }
    knobas_core::activity::record(pool, ACTOR, verb, Some(&entity), detail).await?;
    Ok(())
}

/// Create an ad-hoc context, and announce the switcher has a new room.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, and whatever [`create_context_inner`] refuses
/// with.
#[tauri::command]
pub async fn create_context<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    title: String,
) -> Result<knobas_core::context::ContextRow, IpcError> {
    let pool = lifecycle.pool()?;
    let row = create_context_inner(&pool, &title).await?;
    announce_context(&app, &row);
    Ok(row)
}

/// Promote an entity to a context, and announce it.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, and whatever [`promote_context_inner`] refuses
/// with.
#[tauri::command]
pub async fn promote_context<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
) -> Result<knobas_core::context::ContextRow, IpcError> {
    let pool = lifecycle.pool()?;
    let promoted = promote_context_inner(&pool, &entity_id).await?;
    // Only a mutation is news: a re-promotion changed nothing the switcher
    // could learn from re-listing.
    if promoted.fresh {
        announce_context(&app, &promoted.context);
    }
    Ok(promoted.context)
}

/// Put a changed context on `contexts:changed`.
///
/// Best-effort, like every emit in this app: a failure means no window is
/// listening, which is not a reason to fail a write that already landed. What
/// rides on the event is the row itself, so a listener can splice rather than
/// re-list -- though re-listing is also correct, and is what the switcher
/// does.
fn announce_context<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    row: &knobas_core::context::ContextRow,
) {
    if let Err(error) = app.emit(crate::events::CONTEXTS_CHANGED, row) {
        tracing::debug!(
            event = crate::events::CONTEXTS_CHANGED,
            %error,
            "nothing was listening for this event"
        );
    }
}

// -- the mini board (#177) --------------------------------------------------
//
// In this module for the reason the contexts block above gives: §10.8 freezes
// the `commands/` + `ipc/` layout, so a new read gets a section rather than a
// file. It is a room read, and the shell reaches it from the room.

/// The Tickets tile's mini board (ADR-0009, spec #175).
///
/// Scoped like every other tile in a room, by the same two dimensions the
/// switcher's rooms carry: `ctx_id` is a stored room's context, `sources` a
/// derived room's source list -- empty for *All work* -- and exactly one of
/// them ever narrows. The grouping, the column order and the two payload reads
/// all live in [`knobas_core::mini_board`]; there is nothing for this seam to
/// add, so it adds nothing, the same as [`context_members`].
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, [`Internal`](crate::IpcErrorCode::Internal)
/// for a query failure. An unknown context is an empty board, not a
/// `not_found`: the room's other tiles answer that way too, and a tile that
/// errored would say "this broke" where the truth is "nothing here".
#[tauri::command]
pub async fn mini_board(
    lifecycle: State<'_, Lifecycle>,
    ctx_id: Option<String>,
    sources: Vec<String>,
    project: Option<String>,
) -> Result<knobas_core::mini_board::MiniBoard, IpcError> {
    let pool = lifecycle.pool()?;
    let declarations = declared_paths(&pool).await?;
    Ok(knobas_core::mini_board::read(
        &pool,
        ctx_id.as_deref(),
        &sources,
        project.as_deref(),
        &declarations,
    )
    .await?)
}

// -- the projects a corpus shows (#208) -------------------------------------
//
// Here for the reason the two sections above give: §10.8 freezes the
// `commands/` + `ipc/` layout, so an entity read gets a section rather than a
// file. A project is a grouping of *entities*, read out of the same mirror
// rows this module already lists.

/// Every project the live corpus shows, ordered by source then key
/// (ADR-0010, #208).
///
/// What the switcher builds its project rooms from. Flat rather than grouped
/// per source, because the switcher wants a room list and grouping it here
/// would only be ungrouped there. It cannot come from a room's own scan
/// instead: that read is a window over the newest items and explicitly not a
/// census, so a quiet project would silently have no room.
///
/// Unscoped, and answering for the **live** corpus -- so a source the user
/// turned off reports no projects (migration `0012`), and its projects come
/// back with it. Everything it decides lives in [`knobas_core::project`];
/// there is nothing for this seam to add, so it adds nothing, the same as
/// [`mini_board`] and [`context_members`].
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, [`Internal`](crate::IpcErrorCode::Internal)
/// for a query failure.
#[tauri::command]
pub async fn list_projects(
    lifecycle: State<'_, Lifecycle>,
) -> Result<Vec<knobas_core::project::Project>, IpcError> {
    list_projects_inner(&lifecycle.pool()?).await
}

/// [`list_projects`] with the pool handed in, so a test can reach it.
///
/// The same split every other read in this module has, and for the same
/// reason: `tauri::State` cannot be constructed by hand, so a command that
/// resolved its own pool would be unreachable and a test of it would have to
/// re-type the two lines it contains -- which is a test that goes on passing
/// after the command stops doing this. Pinned by
/// `a_confluence_space_is_a_project_room_and_a_jira_project_is_another` in
/// `tests/entity.rs`, which dies when the declarations stop being resolved
/// here.
///
/// # Errors
///
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) for a query
/// failure or a source listing that fails.
pub async fn list_projects_inner(
    pool: &PgPool,
) -> Result<Vec<knobas_core::project::Project>, IpcError> {
    let declarations = declared_paths(pool).await?;
    Ok(knobas_core::project::list(pool, &declarations).await?)
}

// -- the standup digest (#288) ----------------------------------------------
//
// Here for the reason the two sections above give, and the reason spec #272
// gives in as many words: §10.8 freezes the `commands/` + `ipc/` layout, and
// "standup and Confluence reads go into the entity module as usual" -- the
// `time` module pair was the one ratified exception and this is not a second
// one. What the read decides lives in `crate::standup`; this seam adds the
// pool, the identity and the declarations, the same three things
// `inbox_items_inner` adds, and nothing else.

/// The standup digest for one day (issue #288, spec #272 stories 58-63).
///
/// Three lists -- yesterday, today, blockers -- every line carrying the item
/// it came from, which source said so and which verb it was. `CONTEXT.md`'s
/// **digest**; [`crate::standup`] holds the rules and the reasoning.
///
/// **The webview computes the days**, each as a date and the two instants it
/// spans: `today` is the day being asked about and `earlier` the days before
/// it, oldest first. The rule is [`day_blocks`](crate::commands::time::day_blocks)'s
/// in full -- the machine's timezone is a fact only that side holds, and a UTC
/// offset would be the wrong shape as well as the wrong owner for a day
/// containing a daylight-saving change. How far back the *yesterday* rule may
/// reach is **not** the webview's to say: at most
/// [`standup::LOOKBACK_DAYS`](crate::standup::LOOKBACK_DAYS) of `earlier` are
/// consulted however many are sent.
///
/// **The registry is the injected one**, the shape [`inbox_items`] has and for
/// the same reason: the blockers list is read through the descriptors'
/// declared paths (#277), and a test that could not hand over a descriptor
/// could only witness the declarations the shipped adapters happen to carry --
/// which is a battery that passes just as well against a hardcoded list of
/// English status words. That is the one mistake this read must not be able to
/// make.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database or the sync engine is still coming up,
/// [`Internal`](crate::IpcErrorCode::Internal) for a read failure.
#[tauri::command]
pub async fn standup_digest<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    today: crate::time::week::DayWindow,
    earlier: Vec<crate::time::week::DayWindow>,
) -> Result<crate::standup::StandupDigest, IpcError> {
    let state = crate::sources::state(&app)?;
    standup_digest_inner(
        &state.pool,
        state.registry.as_ref(),
        Utc::now(),
        today,
        &earlier,
    )
    .await
}

/// [`standup_digest`] with the pool, the registry and the clock handed in, so
/// a test can reach it -- the split every read in this module has.
///
/// # Errors
///
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) for a read
/// failure or a source listing that fails.
pub async fn standup_digest_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    now: DateTime<Utc>,
    today: crate::time::week::DayWindow,
    earlier: &[crate::time::week::DayWindow],
) -> Result<crate::standup::StandupDigest, IpcError> {
    let identity = identity_of(pool).await?;
    let declarations = crate::sources::paths::declared_paths(pool, registry).await?;
    crate::standup::digest(pool, &identity, &declarations, now, today, earlier).await
}
