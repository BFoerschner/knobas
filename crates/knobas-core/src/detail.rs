//! One entity as the detail view reads it (interfaces §2.5): its row, its
//! source, the fields its kind declares, its links, its history and the
//! builds of the commit it stands at (#710). Moved here from the app's
//! command module by #1870; what the adapters declare about a kind
//! (`kind_info`) is the app's, which completes the view from [`Detail`].

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

use crate::CoreError;
use crate::activity::ActivityRow;
use crate::entity::EntityRef;
use crate::entity_list::{EntityRow, entity_row, state_of};
use crate::link::LinkEntry;

/// Which source an entity came from, as the detail view names it.
///
/// Every field falls back to the source id, because `run_once` syncs sources
/// that have no `knobas.source_config` row (tests, ad-hoc imports, interfaces
/// §1). An entity whose source was deleted still has to render.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct SourceRef {
    pub id: String,
    /// The configured display name, falling back to the id: `run_once` syncs
    /// sources that were never configured, so there is not always a name.
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

/// One entity as [`read`] found it: everything the slide-over draws but the
/// adapter's own word for its kind, which the app adds from its registry.
/// Not a wire type; each field is the app's `EntityDetail` field of the same
/// name.
#[derive(Debug, Clone)]
pub struct Detail {
    pub row: EntityRow,
    pub source: SourceRef,
    /// The configured adapter kind, `None` for a source with no
    /// configuration row: what the adapter's word for the kind is looked up
    /// by. Not [`SourceRef::adapter_kind`], which falls back to the source id
    /// and would name an adapter nobody configured.
    pub configured_adapter_kind: Option<String>,
    pub body_text: String,
    pub author: Option<String>,
    pub payload: serde_json::Value,
    pub web_url: Option<String>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub links: Vec<LinkEntry>,
    pub activity: Vec<ActivityRow>,
    pub builds: Vec<EntityRow>,
    pub status: Option<String>,
    pub issue_type: Option<String>,
    pub priority: Option<String>,
    pub estimate_seconds: Option<i64>,
    /// A repository's default branch through its declared `default_branch`
    /// path (#2307); `None` where nothing is declared or the record misses it.
    pub default_branch: Option<String>,
}

/// How many history lines the detail view is given up front.
///
/// Enough to fill the panel without a second round trip; the full history is a
/// later milestone's view, not a scroll in a slide-over.
pub const DETAIL_ACTIVITY: i64 = 20;

/// How many builds the detail draws for a commit (#710): enough for every
/// configuration a commit is commonly built by, failing first, so a cut
/// leaves out passed ones.
pub const DETAIL_BUILDS: i64 = 20;

/// The builds that ran on the commit `$1` (#710), a full object id.
///
/// The commit is the entity's declared `commit`, read by [`DETAIL`] beside
/// the row ([`declared_string!`](crate::declared_string)); a build ran
/// on it where its kind's declared `revision` lists it. That walk is stored
/// (#732, migration `0030`, ADR-0019's pattern): `sync.item_revision` holds
/// one row per commit each item declares, filled by the database as items
/// are written, and this statement asks its index on `revision` and touches
/// only the builds it names -- under a millisecond at 50,000 builds, where the
/// walk over every live row took 437-442 ms. No adapter is named here, no
/// payload key: the rows are matched to the source's configured adapter on
/// `adapter_kind`, so a source whose adapter declares no revision contributes
/// nothing whatever its payloads say. Matched exactly: both sides are full
/// object ids as git spells them.
///
/// **Asked only for an entity that stands at a commit**: the detail of a
/// ticket or a page declares none, so there is no sha to bind, and skipping
/// the round trip costs nothing.
///
/// The builds come from `sync.live_item`, so a withdrawn build or a disabled
/// source's is not drawn as if it stood. Ordered as the Builds tile weighs a
/// state (#680): `err`, `warn`, `info`, `ok`, `muted`, then none; newest
/// first within one. `$2` is the declarations, for the path line and the
/// state chip.
///
/// Public so the app's `tests/it/builds_of_a_commit_perf.rs` can read its
/// plan.
pub const DETAIL_BUILDS_AT: &str = concat!(
    r#"
select b.* from (
  select i.entity_id, i.source_id, i.kind, i.title, i.item_updated_at, i.synced_at,
         "#,
    crate::path_line_read!("$2"),
    r#" as path,
         "#,
    crate::declared_state!("$2"),
    r#" as state
    from sync.item_revision r
    join sync.live_item i on i.entity_id = r.entity_id
    join knobas.source_config s on s.id = i.source_id and s.kind = r.adapter_kind
   where r.revision = $1
) b
 order by coalesce(array_position(array['err', 'warn', 'info', 'ok', 'muted'], b.state->>'tone'), 6),
          b.item_updated_at desc nulls last, b.entity_id
 limit $3
"#
);

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
select i.entity_id, i.source_id, i.kind, i.title, "#,
    crate::body_text_shown!("i.body_text"),
    r#" as body_text, i.author,
       i.item_updated_at, i.synced_at, i.payload, i.web_url,
       "#,
    crate::path_line_read!("$2"),
    r#" as path,
       "#,
    crate::declared_state!("$2"),
    r#" as state,
       "#,
    crate::declared_string!("$2", "commit"),
    r#" as commit,
       "#,
    crate::declared_string!("$2", "status_name"),
    r#" as status,
       "#,
    crate::declared_string!("$2", "issue_type"),
    r#" as issue_type,
       "#,
    crate::declared_string!("$2", "priority"),
    r#" as priority,
       "#,
    crate::declared_integer!("$2", "estimate"),
    r#" as estimate_seconds,
       "#,
    crate::declared_string!("$2", "default_branch"),
    r#" as default_branch,
       e.deleted_at,
       c.display_name, c.kind as adapter_kind,
       coalesce(c.enabled, true) as source_enabled
  from sync.item i
  join knobas.entity e on e.id = i.entity_id
  left join knobas.source_config c on c.id = i.source_id
 where i.entity_id = $1
"#
);

/// Everything the slide-over needs for `entity`, read through
/// `declarations` (the path line reads the parent through them, #596, as a
/// room's line does: the panel and the row draw one path), in three round
/// trips and, for an entity that stands at a commit, a fourth: the row (with
/// its declared commit), the builds of that commit (#710), its links and its
/// history. `None` if nothing carries it.
///
/// # Errors
///
/// [`CoreError::Db`] for a query failure, or a state chip that does not
/// decode.
pub async fn read(
    pool: &PgPool,
    declarations: &crate::payload::Declarations,
    entity: &EntityRef,
) -> Result<Option<Detail>, CoreError> {
    let Some(row) = sqlx::query(DETAIL)
        .bind(entity.to_string())
        .bind(declarations.as_param())
        .fetch_optional(pool)
        .await?
    else {
        return Ok(None);
    };

    let source_id: String = row.get("source_id");
    let display_name: Option<String> = row.get("display_name");
    let adapter_kind: Option<String> = row.get("adapter_kind");

    let kind: String = row.get("kind");
    // The board draws live tickets only (`sync.live_item`); the detail opens
    // withdrawn ones too, and says nothing of a status the board would not.
    let status: Option<String> = row.get("status");
    let status = status.filter(|_| {
        kind == "ticket"
            && row.get::<Option<DateTime<Utc>>, _>("deleted_at").is_none()
            && row.get::<bool, _>("source_enabled")
    });

    // The builds of the commit this entity stands at, asked only when it
    // stands at one (see [`DETAIL_BUILDS_AT`] for why).
    let commit: Option<String> = row.get("commit");
    let builds = match commit {
        Some(sha) => sqlx::query(DETAIL_BUILDS_AT)
            .bind(sha)
            .bind(declarations.as_param())
            .bind(DETAIL_BUILDS)
            .fetch_all(pool)
            .await?
            .iter()
            .map(entity_row)
            .collect::<Result<_, _>>()?,
        None => Vec::new(),
    };

    Ok(Some(Detail {
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
            state: state_of(&row)?,
        },
        source: SourceRef {
            display_name: display_name.unwrap_or_else(|| source_id.clone()),
            adapter_kind: adapter_kind.clone().unwrap_or_else(|| source_id.clone()),
            id: source_id,
            enabled: row.get("source_enabled"),
        },
        configured_adapter_kind: adapter_kind,
        body_text: row.get("body_text"),
        author: row.get("author"),
        payload: row.get("payload"),
        web_url: row.get("web_url"),
        deleted_at: row.get("deleted_at"),
        links: crate::link::entries_of(pool, entity).await?,
        activity: crate::activity::recent(pool, DETAIL_ACTIVITY, Some(entity)).await?,
        builds,
        status,
        issue_type: row.get("issue_type"),
        priority: row.get("priority"),
        estimate_seconds: row.get("estimate_seconds"),
        default_branch: row.get("default_branch"),
    }))
}
