//! Favourites (#2428, spec #2419 stories 39 and 40): what a reader keeps at
//! the top of the launcher's empty box, above the recent items.
//!
//! One table, `knobas.favourite` (migration `0069`): an entity id or a saved
//! list id, and a position. The board draws both under one heading in that
//! order, each as an [`EntityRow`] so one row component draws it: an entity
//! as the board's recent rows are drawn, a saved list as its `#l` hit is
//! (kind `list`, source `list`, its text on the path line, the refusal as its
//! state when today's grammar cannot run it; `corpus::SAVED_LIST`).
//!
//! An entity favourite whose entity is unknown or withdrawn is kept and not
//! drawn: a share export brings the favourite before the colleague's machine
//! has synced the item, and the row resolves once it has.

use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::SearchError;
use crate::types::EntityRow;
use crate::vocab::Vocabulary;

/// What a favourite names: an entity (`knobas.entity.id`) or a saved list
/// (`knobas.smart_list.id`, the `list` hit's `entity_id`).
///
/// On the wire `{ "entity": "<id>" }` or `{ "list": "<id>" }`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "search.ts")]
pub enum FavouriteTarget {
    Entity(String),
    List(String),
}

/// The favourited entities, live, with what the board's row draws.
///
/// `$1` the declarations the path line and the state read (#596). A mirrored
/// item reads as the board's recent rows do; an entity knobas owns (a note, an
/// asset, a route) has no mirror row and reads its kind and title from
/// `knobas.entity`, its kind standing in for the source as the corpora's do.
/// A row of a turned-off source is not drawn, as the board draws none.
macro_rules! entities_sql {
    () => {
        concat!(
            "select f.position, e.id as entity_id, coalesce(m.kind, e.kind) as kind,\n",
            "       coalesce(m.source_id, e.kind) as source_id, coalesce(m.title, e.title) as title,\n",
            "       m.item_updated_at as updated_at, coalesce(m.synced_at, e.updated_at) as synced_at,\n",
            "       m.path, m.state, m.cell\n",
            "  from knobas.favourite f\n",
            "  join knobas.entity e on e.id = f.entity_id and e.deleted_at is null\n",
            "  left join lateral (\n",
            "      select i.kind, i.source_id, i.title, i.item_updated_at, i.synced_at,\n",
            "             i.key_cell as cell,\n",
            "             ",
            crate::mirror_path!("$1"),
            " as path,\n",
            "             ",
            crate::mirror_state!("$1"),
            " as state,\n",
            "             coalesce((select c.enabled from knobas.source_config c\n",
            "                        where c.id = i.source_id), true) as enabled\n",
            "        from sync.item i\n",
            "       where i.entity_id = e.id\n",
            "  ) m on true\n",
            " where coalesce(m.enabled, true)\n",
            " order by f.position\n"
        )
    };
}

const ENTITIES_SQL: &str = entities_sql!();

const LISTS_SQL: &str = "select f.position, l.id, l.label, l.query, l.updated_at
   from knobas.favourite f
   join knobas.smart_list l on l.id = f.list_id
  order by f.position";

#[derive(sqlx::FromRow)]
struct EntityFavourite {
    position: i64,
    entity_id: String,
    kind: String,
    source_id: String,
    title: String,
    updated_at: Option<DateTime<Utc>>,
    synced_at: DateTime<Utc>,
    path: Option<String>,
    state: Option<sqlx::types::Json<knobas_core::payload::StateChip>>,
    cell: Option<String>,
}

#[derive(sqlx::FromRow)]
struct ListFavourite {
    position: i64,
    id: String,
    label: String,
    query: String,
    updated_at: DateTime<Utc>,
}

/// Every favourite the board can draw, in the order they were favourited.
///
/// Two reads, the entities and the lists, merged by position.
///
/// # Errors
///
/// [`SearchError::Db`] if either read fails.
pub async fn list(pool: &PgPool, vocab: &Vocabulary) -> Result<Vec<EntityRow>, SearchError> {
    let frames = crate::frames::LauncherFrames::of(&vocab.declarations);
    let entities = sqlx::query_as::<_, EntityFavourite>(ENTITIES_SQL)
        .bind(vocab.declarations.as_param())
        .fetch_all(pool)
        .await?;
    let lists = sqlx::query_as::<_, ListFavourite>(LISTS_SQL)
        .fetch_all(pool)
        .await?;
    let mut rows: Vec<(i64, EntityRow)> = entities
        .into_iter()
        .map(|row| {
            (
                row.position,
                EntityRow {
                    entity_id: row.entity_id,
                    kind: frames.answered_as(&row.kind).to_owned(),
                    source_id: row.source_id,
                    title: row.title,
                    updated_at: row.updated_at,
                    synced_at: row.synced_at,
                    path: row.path,
                    state: row.state.map(|chip| chip.0),
                    cell: row.cell,
                },
            )
        })
        .collect();
    rows.extend(lists.into_iter().map(|row| {
        let state = crate::saved::plan(&row.query, vocab).err().map(|refusal| {
            knobas_core::payload::StateChip {
                text: refusal.description(),
                tone: knobas_core::tone::Tone::Warn,
            }
        });
        (
            row.position,
            EntityRow {
                entity_id: row.id,
                kind: "list".to_owned(),
                source_id: "list".to_owned(),
                title: row.label,
                updated_at: Some(row.updated_at),
                synced_at: row.updated_at,
                path: Some(row.query),
                state,
                cell: None,
            },
        )
    }));
    rows.sort_by_key(|(position, _)| *position);
    Ok(rows.into_iter().map(|(_, row)| row).collect())
}

/// Favourite `target`, last in the block. Favouriting what already is one
/// leaves it where it is.
///
/// # Errors
///
/// [`SearchError::UnknownList`] for a list nobody saved, [`SearchError::Db`]
/// if the write fails.
pub async fn add(pool: &PgPool, target: &FavouriteTarget) -> Result<(), SearchError> {
    // One past the highest number used, so the newest is last even after a
    // removal; `on conflict` keeps a second favourite of the same thing in
    // its place.
    let (statement, id) = match target {
        FavouriteTarget::Entity(id) => (
            "insert into knobas.favourite (position, entity_id)
             select coalesce(max(position), 0) + 1, $1 from knobas.favourite
             on conflict (entity_id) do nothing",
            id,
        ),
        FavouriteTarget::List(id) => (
            "insert into knobas.favourite (position, list_id)
             select coalesce(max(position), 0) + 1, $1 from knobas.favourite
             on conflict (list_id) do nothing",
            id,
        ),
    };
    match sqlx::query(statement).bind(id).execute(pool).await {
        Ok(_) => Ok(()),
        Err(sqlx::Error::Database(error)) if error.is_foreign_key_violation() => {
            Err(SearchError::UnknownList(id.clone()))
        }
        Err(error) => Err(error.into()),
    }
}

/// Unfavourite `target`. Not a favourite is no error: the block is what the
/// reader asked for either way.
///
/// # Errors
///
/// [`SearchError::Db`] if the delete fails.
pub async fn remove(pool: &PgPool, target: &FavouriteTarget) -> Result<(), SearchError> {
    let (statement, id) = match target {
        FavouriteTarget::Entity(id) => ("delete from knobas.favourite where entity_id = $1", id),
        FavouriteTarget::List(id) => ("delete from knobas.favourite where list_id = $1", id),
    };
    sqlx::query(statement).bind(id).execute(pool).await?;
    Ok(())
}
