//! What Dependencies draws (issue #871, spec #808 *Dependencies*, requirements
//! R-11.10 to R-11.14 and R-11.23): the shared layers, and the one read behind
//! the top-bar opening.
//!
//! # The layers
//!
//! One shared, ordered set with unique names (ruling D14), stored in
//! `knobas.dep_layer` and `knobas.dep_layer_kind` (migration `0036`) and seeded
//! with R-11.11's scheme. Each kind sits in at most one layer. The kinds are
//! Dependencies' own vocabulary ([`KINDS`]): the sixteen asset types plus
//! `ticket`, `pr`, `page`, `repo`, `pipeline` and `route`. This module only
//! reads them; the panel that edits them is #873's.
//!
//! # The read
//!
//! The spec's sentence: "One read returns everything for the top-bar opening:
//! every asset, route, repository, pipeline, open PR and linked ticket, with
//! health and the known relations." Layout, lines, folding and search happen in
//! the UI, so this hands back rows and edges and decides nothing about bands.
//!
//! * **Assets**: every row of `knobas.asset`. Health is the asset's **own**:
//!   the worst of its status and the newest sample of each monitor attached to
//!   it, down over warn over up. Not the effective health the Tree's badge
//!   rolls up: a tile is one thing, and a hypervisor drawn red because a
//!   container three levels under it is down would turn every band above a
//!   problem red. `monitors` is how many monitors are attached, so a tile can
//!   be hatched when nothing watches it (R-11.12). Environment and owner are
//!   the ones in force (#431's inheritance).
//! * **Routes**: every row of `knobas.route`, with the monitors attached to the
//!   route itself and the environment of the asset that exposes it.
//! * **Repositories**: every live `repo` item.
//! * **Pipelines**: what the Builds view lists, through #848's own read
//!   (`commands::entity::pipelines_inner`, ADR-0022's declared frame), so the
//!   two views cannot disagree about what a pipeline is. Its health and state
//!   are its newest run's chip.
//! * **Open pull requests**: live `pr` items whose declared state reads in
//!   flight (tone `info`), the rule `knobas_core::signals` uses for *PR open*.
//! * **Linked tickets**: tickets at one end of a confirmed link whose other end
//!   is one of the rows above.
//!
//! The relations are the ones the stored data holds: confirmed links between
//! two rows of the read (`depends-on`, `runs-on` and the rest, by their own
//! relation name), an asset inside its holder ([`INSIDE`]), a route exposed by
//! its asset ([`EXPOSES`]) and landing on its target ([`LANDS_ON`]), and a pull
//! request's repository from its declared parent ([`IN_REPOSITORY`]). Chains,
//! deploy targets and a pipeline's repository are not declared by any source
//! yet, so no edge says them.
//!
//! **No paging and no summary read**: spec #808 puts performance work out of
//! scope for this round, and R-11.23's summary read is its own ticket later.

use std::collections::HashSet;

use knobas_core::tone::Tone;
use sqlx::{PgPool, Row};

use crate::IpcError;

/// Dependencies' own word for a pipeline, and the kind its layer row names.
pub const PIPELINE: &str = "pipeline";

/// An asset is inside the asset that holds it (`knobas.asset.parent_id`).
pub const INSIDE: &str = "inside";
/// An asset exposes a route (`knobas.route.asset_id`).
pub const EXPOSES: &str = "exposes";
/// A route lands on an asset (`knobas.route.target_id`).
pub const LANDS_ON: &str = "lands-on";
/// A pull request is in a repository (its declared parent, ADR-0018).
pub const IN_REPOSITORY: &str = "in-repository";

/// The non-asset kinds, in the order the layers panel lists them, with the
/// monograms R-11.12 gives them.
const OTHER_KINDS: &[(&str, &str, &str)] = &[
    ("ticket", "Ticket", "TK"),
    ("pr", "Pull request", "PR"),
    ("page", "Page", "PG"),
    ("repo", "Repository", "RE"),
    (PIPELINE, "Pipeline", "PL"),
    ("route", "Route", "RO"),
];

/// One kind Dependencies places in a layer.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "assets.ts")]
pub struct DepKind {
    pub id: String,
    /// The built-in label (*Hypervisor*, ruling D15).
    pub label: String,
    /// The two letters a tile draws.
    pub monogram: String,
}

/// Every kind a layer can hold: the six non-asset kinds, then the asset types
/// in `knobas_core::asset::TYPES`' order.
#[must_use]
pub fn kinds() -> Vec<DepKind> {
    OTHER_KINDS
        .iter()
        .map(|(id, label, monogram)| DepKind {
            id: (*id).to_owned(),
            label: (*label).to_owned(),
            monogram: (*monogram).to_owned(),
        })
        .chain(knobas_core::asset::TYPES.iter().map(|t| DepKind {
            id: t.id.to_owned(),
            label: t.label.to_owned(),
            monogram: t.monogram.to_owned(),
        }))
        .collect()
}

/// One shared layer: its name and the kinds it holds, in order. The layers are
/// the same in every view (D14).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "assets.ts", rename = "DepLayer")]
pub struct Layer {
    pub id: String,
    pub name: String,
    /// [`DepKind`] ids: asset type ids, `ticket`, `pr`, `page`, `repo`,
    /// `pipeline`, `route`.
    pub kinds: Vec<String>,
}

/// One thing Dependencies can draw as a tile.
#[derive(Clone, Debug, PartialEq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "assets.ts")]
pub struct DepItem {
    /// The entity id: `asset:…`, `route:…`, `jira:PAY-231`.
    pub id: String,
    /// One of the [`DepKind`] ids: an asset type id, or one of `ticket`,
    /// `pr`, `page`, `repo`, `pipeline`, `route`.
    pub kind: String,
    pub name: String,
    /// Broken (`err`), warning (`warn`), in flight (`info`), fine (`ok`), or
    /// `null` where nothing says.
    pub health: Option<Tone>,
    /// The source's own state word: a ticket's status, a pull request's state,
    /// a pipeline's newest run. `null` for assets and routes, and where the
    /// declaration misses (ADR-0007).
    pub state: Option<String>,
    /// How many monitors are attached: `0` draws an asset or route hatched.
    /// Always `0` for a mirror item.
    pub monitors: i64,
    /// The environment in force on an asset or route.
    pub environment: Option<String>,
    /// The owner in force on an asset.
    pub owner: Option<String>,
}

/// One known relation between two items of the read.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "assets.ts")]
pub struct DepRelation {
    pub from: String,
    pub to: String,
    /// A confirmed link's relation, or [`INSIDE`], [`EXPOSES`], [`LANDS_ON`],
    /// [`IN_REPOSITORY`] -- `inside`, `exposes`, `lands-on`, `in-repository`.
    pub relation: String,
}

/// Everything the top-bar opening draws.
#[derive(Clone, Debug, PartialEq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "assets.ts")]
pub struct Dependencies {
    pub layers: Vec<Layer>,
    pub kinds: Vec<DepKind>,
    pub items: Vec<DepItem>,
    pub relations: Vec<DepRelation>,
}

const LAYERS: &str = "select l.id, l.name,
            coalesce(array_agg(k.kind order by k.position, k.kind)
                     filter (where k.kind is not null), '{}') as kinds
       from knobas.dep_layer l
       left join knobas.dep_layer_kind k on k.layer_id = l.id
      group by l.id, l.name, l.position
      order by l.position, l.id";

/// The shared layers, top to bottom.
///
/// # Errors
///
/// [`IpcError`] if the read fails.
pub async fn layers(pool: &PgPool) -> Result<Vec<Layer>, IpcError> {
    let rows = sqlx::query(LAYERS).fetch_all(pool).await?;
    rows.iter()
        .map(|row| {
            Ok(Layer {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                kinds: row.try_get("kinds")?,
            })
        })
        .collect::<Result<_, sqlx::Error>>()
        .map_err(IpcError::from)
}

/// Every asset and route, with the environment and owner in force and its
/// monitors.
///
/// * `in_force` is #431's inheritance: an asset's own environment and owner,
///   else the nearest holder's. A route takes the asset that exposes it.
/// * `watched` is the monitors attached to each asset or route: how many, and
///   the worst newest sample, ranked as `ROLLUP` ranks a state (smaller is
///   worse). `$1` the relation (`monitored-by`), `$2` the monitor kind.
const ESTATE: &str = "with recursive in_force (id, environment, owner) as (
         select a.id, a.environment, a.owner from knobas.asset a where a.parent_id is null
         union all
         select c.id, coalesce(c.environment, f.environment), coalesce(c.owner, f.owner)
           from in_force f join knobas.asset c on c.parent_id = f.id
     ),
     watched as (
         select x.id, count(distinct i.entity_id) as monitors,
                min(case s.state when 'down' then 0 when 'warn' then 1
                                 when 'up' then 2 else 3 end) as severity
           from (select id from knobas.asset union all select id from knobas.route) x
           join knobas.confirmed_link l
             on (l.from_id = x.id or l.to_id = x.id) and l.relation = $1
           join sync.live_item i
             on i.entity_id = case when l.from_id = x.id then l.to_id else l.from_id end
            and i.kind = $2
           left join lateral (
                select ms.state from knobas.monitor_sample ms
                 where ms.entity_id = i.entity_id
                 order by ms.taken_at desc, ms.id desc limit 1
           ) s on true
          group by x.id
     )
     select a.id, a.parent_id, a.type_id as kind, a.name, f.environment, f.owner,
            least(case a.status when 'down' then 0 when 'warn' then 1
                                when 'up' then 2 else 3 end,
                  coalesce(w.severity, 3)) as severity,
            coalesce(w.monitors, 0) as monitors,
            null::text as target_id
       from knobas.asset a
       left join in_force f on f.id = a.id
       left join watched w on w.id = a.id
     union all
     select r.id, r.asset_id, 'route', r.name, f.environment, f.owner,
            coalesce(w.severity, 3), coalesce(w.monitors, 0), r.target_id
       from knobas.route r
       left join in_force f on f.id = r.asset_id
       left join watched w on w.id = r.id";

/// The repositories and pull requests of the mirror, with their
/// declared state. `$1` the declarations, `$2` the kinds.
const MIRROR: &str = concat!(
    "select i.entity_id, i.kind, i.title, ",
    knobas_core::declared_state!("$1"),
    " as state
       from sync.live_item i
      where i.kind = any($2::text[])"
);

/// The declared parent of each of a set of items, among the parents of one
/// kind. `$1` the child ids, `$2` the parent kind.
const DECLARED_PARENT: &str = "select distinct c.entity_id as child, p.entity_id as parent
       from sync.item_parent c
       join sync.item_parent p
         on p.arm = 'parent' and p.kind = c.kind and p.source_id = c.source_id
        and lower(p.name) = lower(c.name) and p.entity_id <> c.entity_id
      where c.arm = 'child' and c.kind = $2 and c.entity_id = any($1::text[])";

/// Every confirmed link with at least one end in `$1`.
const LINKS: &str = "select l.from_id, l.to_id, l.relation
       from knobas.confirmed_link l
      where l.from_id = any($1::text[]) or l.to_id = any($1::text[])
      order by l.from_id, l.to_id, l.relation";

/// The tickets among `$1`, live, with their declared status. `$2` the
/// declarations.
const TICKETS: &str = concat!(
    "select i.entity_id, i.title, ",
    knobas_core::declared_string!("$2", "status_name"),
    " as status
       from sync.live_item i
      where i.kind = 'ticket' and i.entity_id = any($1::text[])
      order by i.entity_id"
);

/// The tone a tile draws for a rank `ROLLUP`'s order gives, read back
/// through [`super::AssetStatus`] so the two orders cannot drift.
fn tone_of_severity(severity: i32) -> Option<Tone> {
    match super::AssetStatus::of_severity(severity) {
        super::AssetStatus::Down => Some(Tone::Err),
        super::AssetStatus::Warn => Some(Tone::Warn),
        super::AssetStatus::Up => Some(Tone::Ok),
        super::AssetStatus::None => None,
    }
}

fn chip(row: &sqlx::postgres::PgRow) -> Result<Option<knobas_core::payload::StateChip>, IpcError> {
    let raw: Option<serde_json::Value> = row.try_get("state")?;
    raw.map(serde_json::from_value)
        .transpose()
        .map_err(IpcError::internal)
}

/// [`Dependencies`]: the layers, the kinds and the whole estate with its
/// relations.
///
/// # Errors
///
/// [`IpcError`] if a read fails.
pub async fn read(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
) -> Result<Dependencies, IpcError> {
    // The stored declarations `sync.item_parent` is kept from are
    // `registry`'s: the same call every search makes before it reads (#627).
    knobas_search::Searcher::declaring(pool.clone(), registry.descriptors()).await?;
    let declarations = crate::sources::paths::declared_paths(pool, registry).await?;

    let mut items: Vec<DepItem> = Vec::new();
    let mut relations: Vec<DepRelation> = Vec::new();

    for row in sqlx::query(ESTATE)
        .bind(super::MONITORED_BY)
        .bind(super::MONITOR_KIND)
        .fetch_all(pool)
        .await?
    {
        let id: String = row.try_get("id")?;
        let kind: String = row.try_get("kind")?;
        let parent: Option<String> = row.try_get("parent_id")?;
        let target: Option<String> = row.try_get("target_id")?;
        let route = kind == "route";
        if let Some(parent) = parent {
            relations.push(if route {
                DepRelation {
                    from: parent,
                    to: id.clone(),
                    relation: EXPOSES.to_owned(),
                }
            } else {
                DepRelation {
                    from: id.clone(),
                    to: parent,
                    relation: INSIDE.to_owned(),
                }
            });
        }
        if let Some(target) = target {
            relations.push(DepRelation {
                from: id.clone(),
                to: target,
                relation: LANDS_ON.to_owned(),
            });
        }
        let environment: Option<String> = row.try_get("environment")?;
        let owner: Option<String> = row.try_get("owner")?;
        items.push(DepItem {
            id,
            kind,
            name: row.try_get("name")?,
            health: tone_of_severity(row.try_get("severity")?),
            state: None,
            monitors: row.try_get("monitors")?,
            environment,
            owner: if route { None } else { owner },
        });
    }

    let mirror_kinds: Vec<String> = vec!["repo".to_owned(), "pr".to_owned()];
    let mut prs: Vec<String> = Vec::new();
    for row in sqlx::query(MIRROR)
        .bind(declarations.as_param())
        .bind(&mirror_kinds)
        .fetch_all(pool)
        .await?
    {
        let id: String = row.try_get("entity_id")?;
        let kind: String = row.try_get("kind")?;
        let state = chip(&row)?;
        let (kind, health, state) = if kind == "pr" {
            // An open pull request is one whose declared state reads in
            // flight; a merged or closed one is not drawn.
            match state {
                Some(chip) if chip.tone == Tone::Info => {
                    ("pr".to_owned(), Some(Tone::Info), Some(chip.text))
                }
                _ => continue,
            }
        } else {
            (kind, None, None)
        };
        if kind == "pr" {
            prs.push(id.clone());
        }
        items.push(DepItem {
            id,
            kind,
            name: row.try_get("title")?,
            health,
            state,
            monitors: 0,
            environment: None,
            owner: None,
        });
    }

    for pipeline in crate::commands::entity::pipelines_inner(pool, registry).await? {
        let newest = pipeline.recent.into_iter().next().and_then(|run| run.state);
        items.push(DepItem {
            id: pipeline.entity_id,
            kind: PIPELINE.to_owned(),
            name: pipeline.name,
            health: newest.as_ref().map(|chip| chip.tone),
            state: newest.map(|chip| chip.text),
            monitors: 0,
            environment: None,
            owner: None,
        });
    }

    let known: HashSet<String> = items.iter().map(|item| item.id.clone()).collect();
    for row in sqlx::query(DECLARED_PARENT)
        .bind(&prs)
        .bind("repo")
        .fetch_all(pool)
        .await?
    {
        let repo: String = row.try_get("parent")?;
        if known.contains(&repo) {
            relations.push(DepRelation {
                from: row.try_get("child")?,
                to: repo,
                relation: IN_REPOSITORY.to_owned(),
            });
        }
    }

    let ids: Vec<String> = known.iter().cloned().collect();
    let mut linked: Vec<DepRelation> = Vec::new();
    let mut ends: HashSet<String> = HashSet::new();
    for row in sqlx::query(LINKS).bind(&ids).fetch_all(pool).await? {
        let relation = DepRelation {
            from: row.try_get("from_id")?,
            to: row.try_get("to_id")?,
            relation: row.try_get("relation")?,
        };
        for end in [&relation.from, &relation.to] {
            if !known.contains(end) {
                ends.insert(end.clone());
            }
        }
        linked.push(relation);
    }

    let ends: Vec<String> = ends.into_iter().collect();
    let mut tickets: HashSet<String> = HashSet::new();
    for row in sqlx::query(TICKETS)
        .bind(&ends)
        .bind(declarations.as_param())
        .fetch_all(pool)
        .await?
    {
        let id: String = row.try_get("entity_id")?;
        tickets.insert(id.clone());
        let key = id
            .split_once(':')
            .map_or(id.as_str(), |(_, key)| key)
            .to_owned();
        items.push(DepItem {
            id,
            kind: "ticket".to_owned(),
            name: key,
            health: None,
            state: row.try_get("status")?,
            monitors: 0,
            environment: None,
            owner: None,
        });
    }

    relations.extend(linked.into_iter().filter(|relation| {
        let drawn = |end: &String| known.contains(end) || tickets.contains(end);
        drawn(&relation.from) && drawn(&relation.to)
    }));

    Ok(Dependencies {
        layers: layers(pool).await?,
        kinds: kinds(),
        items,
        relations,
    })
}

// -- The layer writes (#873) --------------------------------------------------
//
// The layers panel's five writes. Each answers with every layer, top to
// bottom, so the panel draws what the database now holds rather than a guess.
// Changes apply at once and to every view (R-11.13): there is no draft.

const LAYER_EXISTS: &str = "select exists (select 1 from knobas.dep_layer where id = $1)";
const VIEW_EXISTS: &str = "select exists (select 1 from knobas.dep_view where id = $1)";

/// Whether `query` (one of the `*_EXISTS` reads) finds `id`.
async fn exists(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    query: &'static str,
    id: &str,
) -> Result<bool, IpcError> {
    Ok(sqlx::query_scalar(query)
        .bind(id)
        .fetch_one(&mut **tx)
        .await?)
}

/// The name index `0036` refuses a second spelling of a name with.
const LAYER_NAME_INDEX: &str = "dep_layer_name_idx";

fn layer_name(name: &str) -> Result<&str, IpcError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(IpcError::invalid("A layer needs a name."));
    }
    Ok(name)
}

/// The refusal a taken name gets, in prototype-6's words, or the error as it
/// was when it is anything else.
fn name_taken(error: sqlx::Error, name: &str) -> IpcError {
    let unique = error
        .as_database_error()
        .and_then(|db| db.constraint())
        .is_some_and(|constraint| constraint == LAYER_NAME_INDEX);
    if unique {
        IpcError::conflict(format!(
            "There is already a layer called {name}. Names are unique, so a name always means the same things."
        ))
    } else {
        IpcError::from(error)
    }
}

/// Add a layer at the bottom, holding nothing.
///
/// # Errors
///
/// `invalid` for a blank name, `conflict` for a name another layer has.
pub async fn create_layer(pool: &PgPool, name: &str) -> Result<Vec<Layer>, IpcError> {
    let name = layer_name(name)?;
    let id = format!("layer-{}", uuid::Uuid::new_v4());
    sqlx::query(
        "insert into knobas.dep_layer (id, name, position)
         select $1, $2, coalesce(max(position) + 1, 0) from knobas.dep_layer",
    )
    .bind(&id)
    .bind(name)
    .execute(pool)
    .await
    .map_err(|error| name_taken(error, name))?;
    layers(pool).await
}

/// Rename a layer.
///
/// # Errors
///
/// `invalid` for a blank name, `conflict` for a name another layer has,
/// `not_found` for an id no layer carries.
pub async fn rename_layer(pool: &PgPool, id: &str, name: &str) -> Result<Vec<Layer>, IpcError> {
    let name = layer_name(name)?;
    let done = sqlx::query("update knobas.dep_layer set name = $2 where id = $1")
        .bind(id)
        .bind(name)
        .execute(pool)
        .await
        .map_err(|error| name_taken(error, name))?;
    if done.rows_affected() == 0 {
        return Err(IpcError::not_found(format!("No layer {id}.")));
    }
    layers(pool).await
}

/// Remove a layer from every view; the kinds it held go to no layer (the
/// foreign key's `on delete cascade`).
///
/// # Errors
///
/// `not_found` for an id no layer carries.
pub async fn delete_layer(pool: &PgPool, id: &str) -> Result<Vec<Layer>, IpcError> {
    let done = sqlx::query("delete from knobas.dep_layer where id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    if done.rows_affected() == 0 {
        return Err(IpcError::not_found(format!("No layer {id}.")));
    }
    layers(pool).await
}

/// Put the layers in this order, top to bottom. `ids` names every layer once.
///
/// # Errors
///
/// `invalid` when `ids` is not every layer exactly once.
pub async fn order_layers(pool: &PgPool, ids: &[String]) -> Result<Vec<Layer>, IpcError> {
    let mut tx = pool.begin().await?;
    let mut have: Vec<String> = sqlx::query_scalar("select id from knobas.dep_layer for update")
        .fetch_all(&mut *tx)
        .await?;
    have.sort();
    let mut want = ids.to_vec();
    want.sort();
    if have != want {
        return Err(IpcError::invalid(
            "The order must name every layer exactly once.",
        ));
    }
    sqlx::query(
        "update knobas.dep_layer l set position = o.ord - 1
           from unnest($1::text[]) with ordinality as o(id, ord)
          where l.id = o.id",
    )
    .bind(ids)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    layers(pool).await
}

/// Put a kind at the end of a layer, or in no layer when `layer_id` is `None`.
///
/// # Errors
///
/// `invalid` for a kind Dependencies does not know, `not_found` for a layer
/// that is not there.
pub async fn place_kind(
    pool: &PgPool,
    kind: &str,
    layer_id: Option<&str>,
) -> Result<Vec<Layer>, IpcError> {
    if !kinds().iter().any(|known| known.id == kind) {
        return Err(IpcError::invalid(format!(
            "{kind} is not a kind Dependencies knows."
        )));
    }
    let mut tx = pool.begin().await?;
    sqlx::query("delete from knobas.dep_layer_kind where kind = $1")
        .bind(kind)
        .execute(&mut *tx)
        .await?;
    if let Some(layer) = layer_id {
        if !exists(&mut tx, LAYER_EXISTS, layer).await? {
            return Err(IpcError::not_found(format!("No layer {layer}.")));
        }
        sqlx::query(
            "insert into knobas.dep_layer_kind (kind, layer_id, position)
             select $1, $2, coalesce(max(position) + 1, 0)
               from knobas.dep_layer_kind where layer_id = $2",
        )
        .bind(kind)
        .bind(layer)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    layers(pool).await
}

// -- The saved views (#873) ---------------------------------------------------

/// How a layer's tiles group in a view (prototype-6's *Group by*).
///
/// - `none`: *No groups*.
/// - `environment`: *Environment*.
/// - `owner`: *Owner or team*.
/// - `parent`: *What it runs on*: the asset it sits inside.
/// - `project`: *Project*.
/// - `status`: *Status*.
///
/// The groupings are documented here and not on each variant because the
/// generated TypeScript (`app/src/lib/ipc/generated/assets.ts`) carries a
/// type's and a field's doc but not a variant's (#1325).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS,
)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "assets.ts", rename = "DepGroup")]
pub enum Group {
    #[default]
    None,
    Environment,
    Owner,
    Parent,
    Project,
    Status,
}

/// The tile size (prototype-6's *Tiles*).
///
/// - `auto`: small when a layer holds more than 30.
/// - `roomy`: always roomy.
/// - `compact`: always small.
///
/// The sizes are documented here and not on each variant because the
/// generated TypeScript (`app/src/lib/ipc/generated/assets.ts`) carries a
/// type's and a field's doc but not a variant's (#1325).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS,
)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "assets.ts", rename = "DepTiles")]
pub enum Tiles {
    #[default]
    Auto,
    Roomy,
    Compact,
}

/// What a view keeps for one layer.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(deny_unknown_fields)]
#[ts(export_to = "assets.ts")]
pub struct LayerInView {
    pub hidden: bool,
    pub folded: bool,
    pub group: Group,
}

/// Healthy groups fold above this many items in a layer, unless a view says.
pub const FOLD_AT: i32 = 24;

/// Everything a view keeps, and nothing else (R-11.13).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(deny_unknown_fields)]
#[ts(export_to = "assets.ts")]
pub struct DepViewSettings {
    /// By layer id. A layer not named here shows, unfolded and ungrouped.
    pub layers: std::collections::BTreeMap<String, LayerInView>,
    /// The environment filter: empty is every environment.
    pub environments: Vec<String>,
    /// *Problems and what they touch*.
    pub problems: bool,
    pub tiles: Tiles,
    /// Healthy groups fold above this many items in a layer.
    pub fold_at: i32,
}

impl Default for DepViewSettings {
    fn default() -> Self {
        Self {
            layers: std::collections::BTreeMap::new(),
            environments: Vec::new(),
            problems: false,
            tiles: Tiles::Auto,
            fold_at: FOLD_AT,
        }
    }
}

/// One saved view.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "assets.ts")]
pub struct DepView {
    pub id: String,
    pub name: String,
    pub settings: DepViewSettings,
}

/// Every view, in order, and which opens first from a ticket and from the top
/// bar.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "assets.ts")]
pub struct DepViews {
    pub views: Vec<DepView>,
    pub opens_from_ticket: String,
    pub opens_from_top: String,
}

fn view_of(row: &sqlx::postgres::PgRow) -> Result<DepView, IpcError> {
    let settings: serde_json::Value = row.try_get("settings")?;
    Ok(DepView {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        settings: serde_json::from_value(settings).map_err(IpcError::internal)?,
    })
}

fn view_name(name: &str) -> Result<&str, IpcError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(IpcError::invalid("A view needs a name."));
    }
    Ok(name)
}

fn check_settings(settings: &DepViewSettings) -> Result<serde_json::Value, IpcError> {
    if !(1..=100_000).contains(&settings.fold_at) {
        return Err(IpcError::invalid(
            "The fold count must be a positive number.",
        ));
    }
    serde_json::to_value(settings).map_err(IpcError::internal)
}

/// Every view and the two that open first. A default whose view was deleted
/// falls back to the first view.
///
/// # Errors
///
/// [`IpcError`] if the read fails or a stored view no longer reads.
pub async fn views(pool: &PgPool) -> Result<DepViews, IpcError> {
    let rows = sqlx::query(
        "select id, name, settings, opens_from_ticket, opens_from_top
           from knobas.dep_view order by position, created_at, id",
    )
    .fetch_all(pool)
    .await?;
    let mut views = Vec::with_capacity(rows.len());
    let mut ticket = None;
    let mut top = None;
    for row in &rows {
        let view = view_of(row)?;
        if row.try_get::<bool, _>("opens_from_ticket")? {
            ticket = Some(view.id.clone());
        }
        if row.try_get::<bool, _>("opens_from_top")? {
            top = Some(view.id.clone());
        }
        views.push(view);
    }
    let first = views
        .first()
        .map(|view| view.id.clone())
        .unwrap_or_default();
    Ok(DepViews {
        opens_from_ticket: ticket.unwrap_or_else(|| first.clone()),
        opens_from_top: top.unwrap_or(first),
        views,
    })
}

/// Save a view: a new one at the end when `id` is `None` (*Save as a new
/// view…*), else that view's name and settings (*Save changes*, *Rename*).
///
/// # Errors
///
/// `invalid` for a blank name or a fold count below one, `not_found` for an id
/// no view carries.
pub async fn save_view(
    pool: &PgPool,
    id: Option<&str>,
    name: &str,
    settings: &DepViewSettings,
) -> Result<DepView, IpcError> {
    let name = view_name(name)?;
    let value = check_settings(settings)?;
    let row = if let Some(id) = id {
        sqlx::query(
            "update knobas.dep_view set name = $2, settings = $3 where id = $1
             returning id, name, settings",
        )
        .bind(id)
        .bind(name)
        .bind(&value)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| IpcError::not_found(format!("No view {id}.")))?
    } else {
        sqlx::query(
            "insert into knobas.dep_view (id, name, position, settings)
             select $1, $2, coalesce(max(position) + 1, 0), $3 from knobas.dep_view
             returning id, name, settings",
        )
        .bind(format!("view-{}", uuid::Uuid::new_v4()))
        .bind(name)
        .bind(&value)
        .fetch_one(pool)
        .await?
    };
    view_of(&row)
}

/// Lock every view row for the rest of `tx`, and say how many there are.
async fn lock_views(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>) -> Result<usize, IpcError> {
    let ids: Vec<String> = sqlx::query_scalar("select id from knobas.dep_view for update")
        .fetch_all(&mut **tx)
        .await?;
    Ok(ids.len())
}

/// Delete a view. The last one stays: Dependencies always opens in a view.
///
/// # Errors
///
/// `conflict` for the last view, `not_found` for an id no view carries.
pub async fn delete_view(pool: &PgPool, id: &str) -> Result<DepViews, IpcError> {
    let mut tx = pool.begin().await?;
    // Lock every view first, so two deletes at once cannot both see two views
    // and leave none.
    let count = lock_views(&mut tx).await?;
    if !exists(&mut tx, VIEW_EXISTS, id).await? {
        return Err(IpcError::not_found(format!("No view {id}.")));
    }
    if count <= 1 {
        return Err(IpcError::conflict("The last view cannot be deleted."));
    }
    sqlx::query("delete from knobas.dep_view where id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    views(pool).await
}

/// Choose which view opens from a ticket's page and which from the top bar.
///
/// # Errors
///
/// `not_found` when either id names no view.
pub async fn set_view_defaults(
    pool: &PgPool,
    from_ticket: &str,
    from_top: &str,
) -> Result<DepViews, IpcError> {
    let mut tx = pool.begin().await?;
    // Lock every view first, so a second choice at once waits and then clears
    // the flags this one set instead of tripping the partial unique indexes.
    lock_views(&mut tx).await?;
    for id in [from_ticket, from_top] {
        if !exists(&mut tx, VIEW_EXISTS, id).await? {
            return Err(IpcError::not_found(format!("No view {id}.")));
        }
    }
    sqlx::query(
        "update knobas.dep_view set opens_from_ticket = false, opens_from_top = false
          where opens_from_ticket or opens_from_top",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "update knobas.dep_view set opens_from_ticket = (id = $1), opens_from_top = (id = $2)
          where id in ($1, $2)",
    )
    .bind(from_ticket)
    .bind(from_top)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    views(pool).await
}
