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
#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct ActivityRow {
    pub id: i64,
    pub at: chrono::DateTime<chrono::Utc>,
    /// `"user"`, or `"sync:<source_id>"` for a synced event.
    pub actor: String,
    pub verb: String,
    pub entity_id: Option<String>,
    pub detail: serde_json::Value,
}

/// Append one line to the log.
///
/// `detail` is stored as jsonb; a [`serde_json::Value::Null`] is stored as the
/// column's `{}` default so the log never carries a jsonb null.
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
) -> Result<(), CoreError> {
    let detail = match detail {
        serde_json::Value::Null => serde_json::Value::Object(serde_json::Map::new()),
        other => other,
    };
    sqlx::query(
        r#"insert into knobas.activity (actor, verb, entity_id, detail)
           values ($1, $2, $3, $4)"#,
    )
    .bind(actor)
    .bind(verb)
    .bind(entity.map(EntityRef::to_string))
    .bind(detail)
    .execute(pool)
    .await?;
    Ok(())
}

/// The `limit` most recent lines, newest first.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn recent(pool: &PgPool, limit: i64) -> Result<Vec<ActivityRow>, CoreError> {
    let rows = sqlx::query_as::<_, ActivityRow>(
        r#"select id, at, actor, verb, entity_id, detail
           from knobas.activity
           order by at desc, id desc
           limit $1"#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
