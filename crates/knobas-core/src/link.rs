//! Links: the object knobas owns.
//!
//! A link joins two entities (spec §5a) and is never written back to a source
//! system. It carries the `relation` it asserts, where it came from
//! ([`Origin`]) and who made it. Unlinking is a tombstone -- the row stays,
//! `deleted_at` is set -- so a link that was made and withdrawn stays visible
//! to the activity log and to exports.
//!
//! Uniqueness is the database's: `link_pair_active_idx`, a partial unique index
//! on `(least(from_id, to_id), greatest(from_id, to_id), relation) where
//! deleted_at is null`. Two entities may therefore carry several links as long
//! as their relations differ, and a withdrawn link may be recreated -- but one
//! *pair* carries one active link per relation, whichever way round it was
//! drawn, which is the rule [`entries_of`]'s undirected read always assumed and
//! `link_active_idx` did not state until migration `0011` (#70).
//!
//! **Unordered for uniqueness, ordered for storage.** Only the index expression
//! normalises the pair; `from_id` and `to_id` keep what was written, because
//! `blocks` and `blocked by` are the same row read from two ends and
//! canonicalising the stored pair would lose which end is which.
//!
//! Since #41 the table holds two populations, told apart by
//! [`LinkRow::confirmed_at`]: confirmed links, which are the graph, and
//! *proposals*, which are suggestions nobody has accepted yet. Everything in
//! this module is about the first; [`crate::suggest`] owns the second. The
//! index above spans both, deliberately -- one active edge per
//! `(from, to, relation)` whatever its state -- so a proposal and a link for
//! one pair can never coexist and disagree.

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

/// The relation a [`monitor`](crate::inbox::Category::Alert) is attached to
/// the asset it watches by.
///
/// `CONTEXT.md`, **Monitor**: *"Attached to an asset by a `monitored-by`
/// link"*. Here, in the crate every other one depends on, because **four**
/// places have to agree on the spelling and three of them are SQL: the inbox's
/// alert rule ([`crate::inbox`]), the `monitor_url_host` suggestion
/// ([`crate::suggest`], #478), the recovery line the sync engine writes
/// (`knobas_sync::alerts`), and `knobas_app::assets`, which is what draws the
/// link in the first place. A fifth spelling of it would be a monitor visibly
/// attached to an asset that never colours it, never reaches the inbox and
/// never takes a recovery line -- with nothing failing anywhere.
///
/// The two statements that cannot name the constant -- both are compile-time
/// `concat!`s -- are pinned to it by
/// `inbox::tests::the_alert_rule_reads_the_relation_the_estate_draws` and
/// `suggest::tests::the_relation_this_rule_proposes_is_the_one_the_estate_reads`.
///
/// The rest of the relation vocabulary is a *rendering* decision and stays in
/// `app/src/lib/detail/relations.ts`; this one is load-bearing in a `where`
/// clause, which is a different thing.
pub const MONITORED_BY: &str = "monitored-by";

/// The relation an [asset](crate::asset) draws at the thing it needs.
///
/// `CONTEXT.md`, **Depends on this**: *"every asset linked to it by
/// `depends-on` or `runs-on`, transitively over both … `depends-on` and
/// `runs-on` are load-bearing relations from here on, beside `monitored-by`;
/// the rest of the vocabulary stays open."* That sentence is what puts these
/// two here rather than leaving them in the frontend's table: from #505 on,
/// a `where` clause reads them, which is the same thing that earned
/// [`MONITORED_BY`] its place and a different thing from a word that only has
/// to be *rendered*.
///
/// Read from the **`to`** end. A link is directed, `from` → `to`, and this one
/// says *from* depends on *to* -- so the blast radius of an asset is the
/// `from` ends of the links pointing at it, which is spec #491's
/// *"confirmed links with relation `depends-on` or `runs-on` toward the
/// asset"*. `knobas_app::assets::depends_on_this` is the one reader.
pub const DEPENDS_ON: &str = "depends-on";

/// The relation an [asset](crate::asset) draws at the thing it runs on.
///
/// [`DEPENDS_ON`]'s twin, and read the same way round: *from* runs on *to*, so
/// the container is the `from` end and the machine is the `to` end. Its
/// inverse reading is *hosts*, which `app/src/lib/detail/relations.ts` gives
/// it -- a label and not a second stored word, which is why the walk filters
/// on this one spelling and finds every row however the reader phrased it.
///
/// **Not containment.** ADR-0014 makes holding a `parent_id` field and this a
/// link, so a container held under a compose project may still run on a
/// machine elsewhere in the tree, and both facts reach the panel by different
/// routes.
pub const RUNS_ON: &str = "runs-on";

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
        /// Drawn by knobas as a consequence of what the user wrote: a note's
        /// `[[ref]]` links are the population (`crate::note`'s `reconcile_refs`,
        /// which owns every row of this origin).
        ///
        /// **Not** the asset-in-a-context case this example used to give.
        /// Spec #427 ruled that membership is computed rather than stored --
        /// *"the implied membership of an asset linked to a member ticket is
        /// computed, not stored, as the ADR requires"* -- so `context::member_ids`
        /// answers it from the link the user drew and no row of this origin is
        /// written for it (ADR-0008, amended by #434).
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
            "created_at, ",
            $prefix,
            "confirmed_at, ",
            $prefix,
            "rule, ",
            $prefix,
            "rule_class, ",
            $prefix,
            "reason"
        )
    };
}

// `crate::suggest` writes and reads the same rows and must name the same
// columns; the whole point of the list above is that there is one of it.
pub(crate) use link_columns;

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
    /// When the user said yes -- and, by being `None`, the whole difference
    /// between a link and a [proposal](crate::suggest).
    ///
    /// A row this is `None` on is a suggestion knobas made and nobody has
    /// accepted. It is not in the graph: [`entries_of`] reads
    /// `knobas.confirmed_link`, which cannot contain it, and the tray reads
    /// `knobas.proposed_link`, which cannot contain anything else. Every link
    /// that predates migration `0007` carries its own `created_at` here.
    pub confirmed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// The named detection rule that proposed this link, or `None` for one a
    /// person drew.
    ///
    /// Deliberately not a closed vocabulary: rules are expected to grow, and a
    /// constrained column would make each new one a migration. The *class*
    /// below is the closed axis.
    pub rule: Option<String>,
    /// Which class of evidence [`rule`](Self::rule) is, or `None` for a link a
    /// person drew.
    pub rule_class: Option<crate::suggest::RuleClass>,
    /// Why knobas proposed this link, in the detector's own words -- "the
    /// branch name contains PAY-231".
    ///
    /// Stored, not rendered from [`rule`](Self::rule) at display time: a
    /// suggestion whose reason cannot be shown is not shippable (#41), and a
    /// reason assembled by whichever surface happens to draw it is one that
    /// can be missing from the next surface. `link_proposal_chk` (migration
    /// `0007`) refuses an unconfirmed row without one.
    pub reason: Option<String>,
}

/// Link `from` to `to`, returning the row that was written.
///
/// Links are directed as stated, and both read ([`entries_of`]) and made unique
/// undirected.
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
/// The uniqueness this rests on is over the **unordered** pair --
/// `link_pair_active_idx` (migration `0011`) normalises it with
/// `least`/`greatest`, matching [`entries_of`]'s undirected read -- so the same
/// pair linked the other way round *is* a duplicate here, and the error below
/// means what it says. Ordered for storage all the same: the row keeps the ends
/// it was written with, or `blocks` could not be told from `blocked by`.
///
/// [`CoreError::Duplicate`] if an active link joins this pair under this
/// relation, whichever way round it was drawn; [`CoreError::EndpointMissing`] if
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
    create_with(pool, from, to, relation, origin, note, created_by).await
}

/// [`create`], against an executor the caller chooses.
///
/// The one thing this adds is that the write can share a transaction with what
/// made room for it: [`crate::suggest::resolve_edge`] withdrawing a reversed
/// proposal is only half a mutation, and a tombstone committed without the link
/// that replaced it would have thrown away a proposal for nothing.
///
/// # Errors
///
/// Exactly [`create`]'s.
pub async fn create_with<'e, E>(
    executor: E,
    from: &EntityRef,
    to: &EntityRef,
    relation: &str,
    origin: Origin,
    note: Option<&str>,
    created_by: &str,
) -> Result<LinkRow, CoreError>
where
    E: sqlx::PgExecutor<'e>,
{
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
    .fetch_one(executor)
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

/// Every **confirmed** link `entity` takes part in, in either direction,
/// newest first, with the other end of each resolved.
///
/// ## Why the read is `knobas.confirmed_link` and not `knobas.link`
///
/// A suggestion is a link row whose `confirmed_at` is null (#41), so the table
/// itself holds two populations and the panel may show exactly one of them: a
/// links panel drawing an unconfirmed guess is a correctness bug, not a
/// cosmetic one. `knobas.confirmed_link` and `knobas.proposed_link` (migration
/// `0007`) are each other's negation over the same rows, so this read and the
/// tray's *cannot* overlap however either is later edited -- which a `where`
/// clause written out here twice could not promise. Same treatment, same
/// reason, as `sync.live_item` and the tombstone filter.
///
/// The view also drops the `deleted_at is null` half of the old predicate,
/// because it is inside the view.
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
           from knobas.confirmed_link l
           join knobas.entity e
             on e.id = case when l.from_id = $1 then l.to_id else l.from_id end
          where l.from_id = $1 or l.to_id = $1
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
