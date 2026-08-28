//! Links: the object knobas owns.
//!
//! A link joins two entities (spec §5a) and is never written back to a source
//! system. It carries the `relation` it asserts, where it came from
//! ([`Origin`]) and who made it. Unlinking is a tombstone -- the row stays,
//! `deleted_at` is set -- so a link that was made and withdrawn stays visible
//! to the activity log and to exports.
//!
//! Uniqueness is the database's: `link_active_idx`, a partial unique index on
//! `(from_id, to_id, relation) where deleted_at is null`. Two entities may
//! therefore carry several links as long as their relations differ, and a
//! withdrawn link may be recreated.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::CoreError;
use crate::entity::EntityRef;

/// Where a link came from.
///
/// Stored as lowercase text in `knobas.link.origin`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    /// Drawn by the user.
    Manual,
    /// Proposed by knobas and confirmed by the user.
    Suggested,
    /// Restored from an export.
    Imported,
    /// Mirrored from a relation the source system already states.
    Source,
    /// Drawn by knobas as a consequence of another action, e.g. linking an
    /// asset to a ticket adding the asset to that ticket's context.
    Implied,
}

impl Origin {
    /// The value stored in the `origin` column.
    pub fn as_str(self) -> &'static str {
        match self {
            Origin::Manual => "manual",
            Origin::Suggested => "suggested",
            Origin::Imported => "imported",
            Origin::Source => "source",
            Origin::Implied => "implied",
        }
    }
}

impl std::fmt::Display for Origin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An `origin` column value that is not one of the five known origins.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown link origin {0:?}")]
pub struct UnknownOrigin(pub String);

impl std::str::FromStr for Origin {
    type Err = UnknownOrigin;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "manual" => Ok(Origin::Manual),
            "suggested" => Ok(Origin::Suggested),
            "imported" => Ok(Origin::Imported),
            "source" => Ok(Origin::Source),
            "implied" => Ok(Origin::Implied),
            other => Err(UnknownOrigin(other.to_owned())),
        }
    }
}

// The column is plain `text`, so the codec borrows `str`'s rather than
// declaring a PostgreSQL enum type that does not exist.
impl sqlx::Type<sqlx::Postgres> for Origin {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <str as sqlx::Type<sqlx::Postgres>>::type_info()
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        <&str as sqlx::Type<sqlx::Postgres>>::compatible(ty)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Postgres> for Origin {
    fn decode(value: sqlx::postgres::PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let text = <&str as sqlx::Decode<sqlx::Postgres>>::decode(value)?;
        Ok(text.parse()?)
    }
}

/// One active link, as seen from either of its ends.
#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct LinkRow {
    pub id: Uuid,
    pub from_id: String,
    pub to_id: String,
    pub relation: String,
    pub origin: Origin,
    pub created_by: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Link `from` to `to`, returning the new link's id.
///
/// Links are directed as stated but read undirected by [`links_of`].
///
/// # Errors
///
/// [`CoreError::Duplicate`] if an active link with the same
/// `(from, to, relation)` already exists; [`CoreError::EndpointMissing`] if
/// either endpoint has no `knobas.entity` row; [`CoreError::Db`] for anything
/// else.
pub async fn create(
    pool: &PgPool,
    from: &EntityRef,
    to: &EntityRef,
    relation: &str,
    origin: Origin,
    created_by: &str,
) -> Result<Uuid, CoreError> {
    let (id,): (Uuid,) = sqlx::query_as(
        r#"insert into knobas.link (from_id, to_id, relation, origin, created_by)
           values ($1, $2, $3, $4, $5)
           returning id"#,
    )
    .bind(from.to_string())
    .bind(to.to_string())
    .bind(relation)
    .bind(origin.as_str())
    .bind(created_by)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Every active link `entity` takes part in, in either direction, newest
/// first.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn links_of(pool: &PgPool, entity: &EntityRef) -> Result<Vec<LinkRow>, CoreError> {
    let rows = sqlx::query_as::<_, LinkRow>(
        r#"select id, from_id, to_id, relation, origin, created_by, created_at
           from knobas.link
           where deleted_at is null and (from_id = $1 or to_id = $1)
           order by created_at desc, id desc"#,
    )
    .bind(entity.to_string())
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Withdraw a link by tombstoning it: the row stays, `deleted_at` is set.
///
/// Idempotent -- withdrawing an already-withdrawn link succeeds and changes
/// nothing.
///
/// # Errors
///
/// [`CoreError::LinkNotFound`] if no link carries `id` at all;
/// [`CoreError::Db`] if the statement fails.
pub async fn unlink(pool: &PgPool, id: Uuid) -> Result<(), CoreError> {
    let updated: Option<(Uuid,)> = sqlx::query_as(
        r#"update knobas.link set deleted_at = now()
           where id = $1 and deleted_at is null
           returning id"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    if updated.is_some() {
        return Ok(());
    }

    // Nothing was updated: the link is either already tombstoned, which is
    // fine, or it never existed, which the caller wants to hear about.
    let existing: Option<(Uuid,)> = sqlx::query_as("select id from knobas.link where id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    existing.map(|_| ()).ok_or(CoreError::LinkNotFound(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_roundtrips_through_its_column_value() {
        for origin in [
            Origin::Manual,
            Origin::Suggested,
            Origin::Imported,
            Origin::Source,
            Origin::Implied,
        ] {
            assert_eq!(origin.as_str().parse(), Ok(origin));
        }
        assert!("confirmed".parse::<Origin>().is_err());
    }

    #[test]
    fn origin_serializes_as_its_column_value() {
        assert_eq!(
            serde_json::to_string(&Origin::Suggested).unwrap(),
            "\"suggested\""
        );
    }
}
