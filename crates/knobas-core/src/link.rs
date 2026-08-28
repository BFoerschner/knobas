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

/// Every column of `knobas.link` that leaves this module, in one place.
///
/// [`LinkRow`] is a `FromRow`, so a column this list forgets is not a compile
/// error anywhere -- it is a decode failure at run time, in whichever of the
/// three statements below was written without it. Naming them once is what
/// stops `create`'s `returning`, `entries_of`'s `select` and `unlink`'s
/// `returning` from drifting apart.
///
/// A macro rather than a `const` because the statements are `concat!`ed at
/// compile time, and `concat!` takes literals -- which is also why the table
/// alias is a parameter: [`entries_of`] joins a second table and every column
/// there has to be qualified.
macro_rules! link_columns {
    ($prefix:literal) => {
        concat!(
            $prefix,
            "id, ",
            $prefix,
            "from_id, ",
            $prefix,
            "to_id, ",
            $prefix,
            "relation, ",
            $prefix,
            "origin, ",
            $prefix,
            "note, ",
            $prefix,
            "created_by, ",
            $prefix,
            "created_at"
        )
    };
}

/// One active link, as seen from either of its ends.
#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct LinkRow {
    pub id: Uuid,
    pub from_id: String,
    pub to_id: String,
    pub relation: String,
    pub origin: Origin,
    /// Why the link exists, in the user's own words -- `None` unless one was
    /// given.
    ///
    /// The column has been in the schema since `0001` and reached nothing
    /// until Links v1 wired it through (#40): store row, DTO, TypeScript
    /// mirror. Nullable rather than defaulted to `""`, because "no reason
    /// recorded" and "a reason recorded as nothing" are different facts and
    /// only one of them is worth a line in the panel.
    pub note: Option<String>,
    pub created_by: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Link `from` to `to`, returning the row that was written.
///
/// Links are directed as stated but read undirected by [`entries_of`].
///
/// The whole row and not just the id, for the reason
/// [`crate::activity::record`] hands its row back: `id` and `created_at` are
/// the database's to choose, and the caller has to *announce* what it wrote --
/// the link command puts the link's id, its other end and its relation into an
/// activity line. Reading back what was just written would mean guessing which
/// of a table's rows is your own.
///
/// # Errors
///
/// The uniqueness this rests on is **directed** -- `link_active_idx` is on
/// `(from_id, to_id, relation)` while [`entries_of`] reads undirected, so the
/// same pair linked the other way round is not a duplicate here and shows as a
/// second row on both ends. Known, filed as **#70**; do not read the error
/// below as "this pair is linked".
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
    note: Option<&str>,
    created_by: &str,
) -> Result<LinkRow, CoreError> {
    let row = sqlx::query_as::<_, LinkRow>(concat!(
        "insert into knobas.link (from_id, to_id, relation, origin, note, created_by)
         values ($1, $2, $3, $4, $5, $6)
         returning ",
        link_columns!("")
    ))
    .bind(from.to_string())
    .bind(to.to_string())
    .bind(relation)
    .bind(origin.as_str())
    .bind(note)
    .bind(created_by)
    .fetch_one(pool)
    .await
    // Not a plain `?`: the crate-wide `From<sqlx::Error>` does not classify a
    // foreign-key violation, and an endpoint that has not synced yet would
    // cross the IPC boundary as `internal` rather than as `not_found`.
    .map_err(CoreError::from_link_write)?;
    Ok(row)
}

/// The end of a link that the reader is **not** looking at.
///
/// A link row carries two ids and nothing else, and an id is not something a
/// person recognises: the panel has to say *what* was linked (spec §5a, #40's
/// story 9). Resolved here rather than by a second read per row, because a
/// detail view drawing ten links would otherwise make ten round trips.
#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct LinkEnd {
    pub entity_id: String,
    pub kind: String,
    /// The entity's own title. Untrusted source text: render it as text.
    pub title: String,
    /// Set when the source withdrew this entity.
    ///
    /// Not a filter -- a *fact* the reader is shown. A link may point at
    /// something that vanished upstream, and hiding it would be a link that
    /// dangles silently (#40's story 10).
    pub deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// One link, as a detail view draws it: the record, and its other end.
#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct LinkEntry {
    #[sqlx(flatten)]
    pub link: LinkRow,
    /// The end that is not the entity this was read for.
    ///
    /// Which end that is depends on who is reading, which is why this cannot
    /// be resolved once and cached on the row: `A -> B` hydrates `B` on A's
    /// detail and `A` on B's.
    #[sqlx(flatten)]
    pub other: LinkEnd,
}

/// Every active link `entity` takes part in, in either direction, newest
/// first, with the other end of each resolved.
///
/// ## Why the join is on `knobas.entity` and not on `sync.live_item`
///
/// `sync.live_item` has the tombstone filter built in, and every *other*
/// reader in the app is supposed to go through it. This one deliberately does
/// not: §5a says a link may point at an entity the source withdrew, and such a
/// link must stay visible and openable rather than vanishing from the panel.
/// `knobas.entity` is the durable identity -- it survives the sweep, it
/// carries `kind`, `title` and the tombstone itself -- so joining it is what
/// makes "marked withdrawn" possible instead of "silently gone".
///
/// It is also the table that has a row for entities the mirror never had:
/// notes and contexts are knobas' own (M2), and they are linkable.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn entries_of(pool: &PgPool, entity: &EntityRef) -> Result<Vec<LinkEntry>, CoreError> {
    let rows = sqlx::query_as::<_, LinkEntry>(concat!(
        "select ",
        link_columns!("l."),
        ", e.id as entity_id, e.kind, e.title, e.deleted_at
           from knobas.link l
           join knobas.entity e
             on e.id = case when l.from_id = $1 then l.to_id else l.from_id end
          where l.deleted_at is null and (l.from_id = $1 or l.to_id = $1)
          order by l.created_at desc, l.id desc"
    ))
    .bind(entity.to_string())
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Withdraw a link by tombstoning it: the row stays, `deleted_at` is set.
///
/// Idempotent -- withdrawing an already-withdrawn link succeeds and changes
/// nothing. The two outcomes are distinguishable, which is what the `Option`
/// is for: `Some(row)` is "this call withdrew that link", `None` is "there was
/// nothing left to withdraw". A caller writing one activity line per *mutation*
/// needs the difference -- `Ok` alone would have it announce an unlink that did
/// not happen.
///
/// The row comes back whole so the caller can name the link's ends and its
/// relation. It is the row **as it now stands**, tombstone included.
///
/// # Errors
///
/// [`CoreError::LinkNotFound`] if no link carries `id` at all;
/// [`CoreError::Db`] if the statement fails.
pub async fn unlink(pool: &PgPool, id: Uuid) -> Result<Option<LinkRow>, CoreError> {
    let updated = sqlx::query_as::<_, LinkRow>(concat!(
        "update knobas.link set deleted_at = now()
         where id = $1 and deleted_at is null
         returning ",
        link_columns!("")
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    if updated.is_some() {
        return Ok(updated);
    }

    // Nothing was updated: the link is either already tombstoned, which is
    // fine, or it never existed, which the caller wants to hear about.
    let existing: Option<(Uuid,)> = sqlx::query_as("select id from knobas.link where id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    existing.map(|_| None).ok_or(CoreError::LinkNotFound(id))
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
