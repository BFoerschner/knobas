//! The activity log: an append-only record of what happened (spec §2a).
//!
//! Every action taken through knobas and every notable synced event lands
//! here as one line: when, who, what verb, which entity, and a free-form
//! `detail` object. Nothing in here is ever updated or deleted.

use serde::Serialize;
use sqlx::PgPool;

use crate::CoreError;
use crate::entity::EntityRef;

/// One line of the activity log.
#[derive(Clone, Debug, Serialize, sqlx::FromRow, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct ActivityRow {
    pub id: i64,
    /// RFC 3339 on the wire.
    pub at: chrono::DateTime<chrono::Utc>,
    /// `"user"`; `"knobas"` for an action knobas took on its own (the
    /// start-work flow's reverse direction); or `"sync:<source_id>"` for a
    /// synced event.
    pub actor: String,
    pub verb: String,
    pub entity_id: Option<String>,
    /// Free-form jsonb, so §2.6 maps it to `unknown`.
    ///
    /// Not `Record<string, unknown>`: the Rust type is any JSON value, and only
    /// `activity::insert` coerces a JSON `null` to `{}`. A row written with a
    /// string, a number or an array is well-typed here and would make the
    /// narrower declaration a lie, the kind that type-checks in the frontend
    /// and throws at runtime. Narrow it with a check where you read it.
    #[ts(type = "unknown")]
    pub detail: serde_json::Value,
}

/// Append one line to the log, and hand back the line that was written.
///
/// `detail` is stored as jsonb; a [`serde_json::Value::Null`] is stored as the
/// column's `{}` default so the log never carries a jsonb null.
///
/// The row comes back from the `returning` clause of the insert itself rather
/// than from a second statement: `id` and `at` are the database's to choose,
/// and a caller that has to announce the line -- `activity:new` carries an
/// [`ActivityRow`] -- would otherwise have to read back what it just wrote, in
/// a table where "the newest row" is not reliably its own. Callers that only
/// wanted the write drop the value.
///
/// # Errors
///
/// [`CoreError::Db`] if the insert fails.
pub async fn record(
    pool: &PgPool,
    actor: &str,
    verb: &str,
    entity: Option<&EntityRef>,
    detail: serde_json::Value,
) -> Result<ActivityRow, CoreError> {
    record_with(pool, actor, verb, entity, detail).await
}

/// [`record`], against an executor the caller chooses.
///
/// The one thing this adds is that a line can be written inside the transaction
/// that made the mutation it describes -- so a log line for a write that rolls
/// back rolls back with it.
///
/// # Errors
///
/// [`CoreError::Db`] if the insert fails.
pub async fn record_with<'e, E>(
    executor: E,
    actor: &str,
    verb: &str,
    entity: Option<&EntityRef>,
    detail: serde_json::Value,
) -> Result<ActivityRow, CoreError>
where
    E: sqlx::PgExecutor<'e>,
{
    let detail = match detail {
        serde_json::Value::Null => serde_json::Value::Object(serde_json::Map::new()),
        other => other,
    };
    let row = sqlx::query_as::<_, ActivityRow>(
        r#"insert into knobas.activity (actor, verb, entity_id, detail)
           values ($1, $2, $3, $4)
           returning id, at, actor, verb, entity_id, detail"#,
    )
    .bind(actor)
    .bind(verb)
    .bind(entity.map(EntityRef::to_string))
    .bind(detail)
    .fetch_one(executor)
    .await?;
    Ok(row)
}

/// The verb of a job's opening (#819): the reader's own bookkeeping, which
/// `knobas_app::conversation` writes and reads back as *last visit*.
///
/// **Not a change, so [`recent`] leaves it out.** The history panel and the
/// status bar's latest-change line draw what happened to an item; a line per
/// opened job would push every real change off both. The literal in the two
/// statements below is this value, pinned by
/// `a_visit_is_not_in_the_recent_activity`.
pub const VISITED: &str = "visited";

/// Every line but a visit, newest first.
///
/// `0002`'s `activity_recent_idx (at desc, id desc)` is what this reads
/// through.
const RECENT_GLOBAL: &str = r#"select id, at, actor, verb, entity_id, detail
   from knobas.activity
   where verb <> 'visited'
   order by at desc, id desc
   limit $1"#;

/// One entity's lines but its visits, newest first -- the detail view's history panel
/// (spec §12.1).
///
/// A second statement rather than an interpolated `where`, and it reads
/// through a different index: `0001`'s `activity_entity_idx (entity_id, at
/// desc)`. `$2` is the entity id.
const RECENT_SCOPED: &str = r#"select id, at, actor, verb, entity_id, detail
   from knobas.activity
   where entity_id = $2 and verb <> 'visited'
   order by at desc, id desc
   limit $1"#;

/// The `limit` most recent lines, newest first.
///
/// `entity` scopes the read to one entity's history; `None` is the global
/// stream the status bar's latest-change line reads.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn recent(
    pool: &PgPool,
    limit: i64,
    entity: Option<&EntityRef>,
) -> Result<Vec<ActivityRow>, CoreError> {
    let mut query = sqlx::query_as::<_, ActivityRow>(match entity {
        None => RECENT_GLOBAL,
        Some(_) => RECENT_SCOPED,
    })
    .bind(limit);
    if let Some(entity) = entity {
        query = query.bind(entity.to_string());
    }
    Ok(query.fetch_all(pool).await?)
}
