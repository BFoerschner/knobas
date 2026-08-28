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

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::CoreError;
use crate::entity::EntityRef;

crate::closed_vocabulary! {
    /// Where a link came from.
    ///
    /// Stored as lowercase text in `knobas.link.origin`, whose
    /// `link_origin_chk` (migration 0003) allows exactly these spellings --
    /// and `ALL` is what the test that pins the two together walks.
    pub enum Origin {
        /// Drawn by the user.
        Manual => "manual",
        /// Proposed by knobas and confirmed by the user.
        Suggested => "suggested",
        /// Restored from an export.
        Imported => "imported",
        /// Mirrored from a relation the source system already states.
        Source => "source",
        /// Drawn by knobas as a consequence of another action, e.g. linking an
        /// asset to a ticket adding the asset to that ticket's context.
        Implied => "implied",
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
        // Read off `ALL` rather than a second hand-written match: the point of
        // declaring the enum as a closed vocabulary is that there is one list.
        Origin::ALL
            .iter()
            .copied()
            .find(|origin| origin.as_str() == s)
            .ok_or_else(|| UnknownOrigin(s.to_owned()))
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
    .await
    .map_err(CoreError::from_link_write)?;
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

    /// The enum and migration 0003's CHECK constraint are one list written in
    /// two places, and neither may grow without the other: a variant the
    /// constraint does not allow is an `INSERT` that fails at runtime, and a
    /// spelling the enum does not know is a row [`Origin`]'s decoder refuses,
    /// so the link cannot be read back at all.
    ///
    /// Driven by `ALL`, which is generated from the same variant list as the
    /// enum, so a new origin necessarily reaches this assertion. The other
    /// half of the pin -- the constraint as the live catalog reports it --
    /// is in `crates/knobas-db/tests/schema.rs`.
    ///
    /// It reads `0003` because `0003` is where the constraint is, and applied
    /// migrations are never edited: widening the vocabulary means a `0004`
    /// that drops and re-adds it, and rewriting this test to read *that* file
    /// is part of doing so, not an accident of it.
    #[test]
    fn the_origins_are_exactly_what_the_migration_allows() {
        let migration = include_str!("../../knobas-db/migrations/0003_link_origin.sql");
        let line = migration
            .lines()
            .find(|line| line.contains("check (origin in ("))
            .expect("link_origin_chk is missing from 0003");

        for origin in Origin::ALL {
            assert!(
                line.contains(&format!("'{}'", origin.as_str())),
                "{origin:?} is a variant the constraint does not allow: {line}"
            );
        }
        assert_eq!(
            line.matches('\'').count() / 2,
            Origin::ALL.len(),
            "the constraint and the enum list different numbers of origins: {line}"
        );
    }

    #[test]
    fn origin_serializes_as_its_column_value() {
        assert_eq!(
            serde_json::to_string(&Origin::Suggested).unwrap(),
            "\"suggested\""
        );
    }
}
