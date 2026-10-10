//! What the launcher shows before anything is typed.
//!
//! Spec §4: an empty box is a **board**, not a query -- the smart lists with
//! their counts, and the newest items across every kind. Answering it with the
//! search statement would be a full-corpus scan that returns nothing, which is
//! why [`crate::Searcher::search`] refuses a query with neither text nor a
//! filter and points here instead.
//!
//! # Why `recent` is not `order by item_updated_at desc limit 20`
//!
//! Because migration `0002` indexes `(kind, item_updated_at desc)` and
//! `(source_id, item_updated_at desc)` -- **not** `item_updated_at` alone. The
//! naive form has no index to walk, so it sorts the whole mirror on every
//! `⌘K`. Reading the newest per kind through the compound index and merging is
//! index-backed and bounded, and `tests/it/home.rs` asserts the plan rather than
//! trusting the comment: a later "small refactor" that turns the board into a
//! sequential scan fails there.
//!
//! # What the board promises, and what it does not
//!
//! It promises the newest `limit` items, ordered newest first. It does **not**
//! promise every kind a slot: a kind that genuinely holds the twenty newest
//! items owns the board, because that is what "recent" means. Fetching `limit`
//! rows *per kind* is what makes the read index-backed and bounded, not a
//! fairness device -- the per-kind rail the round-3 mockup draws is the smart
//! lists, which have their own counts.

use chrono::{DateTime, Utc};

use crate::SearchError;
use crate::lists::SmartListSummary;
use crate::types::EntityRow;

/// How many items the board shows.
pub const RECENT_LIMIT: u32 = 20;

/// The newest items of every kind the mirror holds, newest first.
///
/// `$1` is the kinds to read, `$2` the limit. `limit $2` appears twice on
/// purpose: once inside the lateral, so no kind's branch is unbounded, and once
/// outside, so the merge is too.
///
/// The columns are named rather than `select *`ed -- `sync.live_item` carries a
/// `tsvector` and reading one into a `FromRow` struct panics at runtime
/// (interfaces §1).
macro_rules! recent_sql {
    ($prefix:literal) => {
        concat!(
            $prefix,
            "select s.entity_id, s.kind, s.source_id, s.title,\n",
            "       s.item_updated_at as updated_at, s.synced_at, s.path\n",
            "  from unnest($1::text[]) as k(kind)\n",
            "  cross join lateral (\n",
            "      select i.entity_id, i.kind, i.source_id, i.title,\n",
            "             i.item_updated_at, i.synced_at,\n",
            "             ",
            knobas_core::ancestor_path_read!("i.payload"),
            " as path\n",
            "        from sync.live_item i\n",
            "       where i.kind = k.kind\n",
            "       order by i.item_updated_at desc nulls last, i.entity_id\n",
            "       limit $2\n",
            "  ) s\n",
            " order by s.item_updated_at desc nulls last, s.entity_id\n",
            " limit $2\n"
        )
    };
}

const RECENT_SQL: &str = recent_sql!("");

/// The same statement, planned rather than run.
///
/// `analyze false` so it is a plan and not an execution, `costs false` so the
/// output is stable enough to assert on.
#[cfg(any(test, feature = "test-util"))]
const EXPLAIN_RECENT_SQL: &str = recent_sql!("explain (analyze false, costs false)\n");

/// Which kinds to read the newest of.
///
/// Read from the **mirror**, not from the kind catalog, and deliberately so.
/// The catalog says what the compiled-in adapters *declare*; the board shows
/// what the mirror *holds*. Those differ in both directions: a declared kind
/// with no rows contributes nothing to the board (so consulting the catalog
/// buys nothing), and a kind the catalog does not declare -- an adapter since
/// removed, a source the user disabled, a mirror older than the build -- still
/// has items the user can open, so leaving it out would be a board missing
/// rows.
///
/// # Why this is not `select distinct kind`
///
/// Because PostgreSQL has no loose index scan, and `select distinct kind from
/// sync.item` therefore plans as `HashAggregate → Seq Scan on item` -- a full
/// scan of the mirror, on the board's path, growing with the corpus. This is
/// the classic recursive emulation: start at the first key, then repeatedly ask
/// the index for the next one strictly greater. `item_kind_updated_idx` leads
/// with `kind`, so every step is an `Index Only Scan` and the whole read is
/// O(distinct kinds), flat in corpus size.
///
/// Measured at 3 200 rows, `analyze`d: 47 buffers / 0.31 ms for the `distinct`
/// against 16 buffers / 0.07 ms for this -- and only the first of those two
/// numbers grows. The cost was never the point; the *claim* was. This module's
/// whole stated rationale is that the board is index-backed, and a comment
/// asserting that above a sequential scan is worth less than no comment.
///
/// `sync.item` rather than `sync.live_item`: the view's join to
/// `knobas.entity` is what would cost the index-only scan its "only", and a
/// kind whose every row is tombstoned merely buys one lateral probe that
/// returns nothing.
macro_rules! recent_kinds_sql {
    ($prefix:literal) => {
        concat!(
            $prefix,
            "with recursive k as (\n",
            "    (select kind from sync.item order by kind limit 1)\n",
            "  union all\n",
            "    select (select i.kind from sync.item i\n",
            "              where i.kind > k.kind order by i.kind limit 1)\n",
            "      from k where k.kind is not null\n",
            ")\n",
            "select kind from k where kind is not null\n"
        )
    };
}

const RECENT_KINDS_SQL: &str = recent_kinds_sql!("");

#[cfg(any(test, feature = "test-util"))]
const EXPLAIN_RECENT_KINDS_SQL: &str = recent_kinds_sql!("explain (analyze false, costs false)\n");

/// What an empty box answers with (interfaces §2.4).
///
/// The IPC `LauncherHome` adds `sources` and `pending_writes` on top of this
/// (open question **E-Q5**); those belong to streams F and G, and neither is
/// this crate's to read.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LauncherBoard {
    pub smart_lists: Vec<SmartListSummary>,
    pub recent: Vec<EntityRow>,
}

/// One row of [`RECENT_SQL`], before it becomes an [`EntityRow`].
///
/// A struct of its own rather than a `FromRow` on the IPC type: the wire shape
/// is a contract and the column names are a query's business, and tying them
/// together means a renamed column is a renamed JSON field.
#[derive(sqlx::FromRow)]
struct RecentRow {
    entity_id: String,
    kind: String,
    source_id: String,
    title: String,
    updated_at: Option<DateTime<Utc>>,
    synced_at: DateTime<Utc>,
    /// Where the row sits inside its source (#284); null for a record with no
    /// readable `ancestors`.
    path: Option<String>,
}

/// The newest items across every kind the mirror holds.
///
/// # Errors
///
/// [`SearchError::Db`] if either read fails.
pub async fn recent(pool: &sqlx::PgPool, limit: u32) -> Result<Vec<EntityRow>, SearchError> {
    let kinds = kinds(pool).await?;
    let rows = sqlx::query_as::<_, RecentRow>(RECENT_SQL)
        .bind(&kinds)
        .bind(i64::from(limit))
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| EntityRow {
            entity_id: row.entity_id,
            kind: row.kind,
            source_id: row.source_id,
            title: row.title,
            updated_at: row.updated_at,
            synced_at: row.synced_at,
            path: row.path,
        })
        .collect())
}

/// The plans `recent` runs under, for the test that keeps it index-backed.
///
/// **Both** statements, and that is the whole point of the helper's shape.
/// [`recent`] runs [`RECENT_KINDS_SQL`] and then [`RECENT_SQL`]; a probe that
/// explained only the second would report "no sequential scan" while the board
/// sequentially scanned the mirror in the statement it did not look at. That is
/// exactly what this helper did in review round 1, and a mutation reverting the
/// kinds read to `select distinct` left every home test green -- a detector
/// that was sound but scoped to half of what it claimed to cover.
///
/// # Errors
///
/// [`SearchError::Db`] if either plan cannot be read.
#[cfg(any(test, feature = "test-util"))]
pub async fn explain_recent(pool: &sqlx::PgPool, limit: u32) -> Result<String, SearchError> {
    let kinds_plan: Vec<String> = sqlx::query_scalar(EXPLAIN_RECENT_KINDS_SQL)
        .fetch_all(pool)
        .await?;
    let kinds = kinds(pool).await?;
    let rows_plan: Vec<String> = sqlx::query_scalar(EXPLAIN_RECENT_SQL)
        .bind(&kinds)
        .bind(i64::from(limit))
        .fetch_all(pool)
        .await?;
    Ok(format!(
        "-- kinds --\n{}\n-- rows --\n{}",
        kinds_plan.join("\n"),
        rows_plan.join("\n")
    ))
}

async fn kinds(pool: &sqlx::PgPool) -> Result<Vec<String>, SearchError> {
    Ok(sqlx::query_scalar::<_, String>(RECENT_KINDS_SQL)
        .fetch_all(pool)
        .await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each plan variant has to be the *same* statement, or the assertion in
    /// `tests/it/home.rs` is about a query nobody runs.
    ///
    /// Both of them, because [`recent`] runs both -- see `explain_recent`.
    #[test]
    fn the_explained_statements_are_the_ones_that_run() {
        const PREFIX: &str = "explain (analyze false, costs false)\n";
        for (explained, run) in [
            (EXPLAIN_RECENT_SQL, RECENT_SQL),
            (EXPLAIN_RECENT_KINDS_SQL, RECENT_KINDS_SQL),
        ] {
            assert_eq!(
                explained
                    .strip_prefix(PREFIX)
                    .expect("the plan variant is the statement with a prefix"),
                run
            );
        }
    }

    /// The kinds read walks the index rather than the table.
    ///
    /// Pinned in the source as well as in the plan (`tests/it/home.rs`) because
    /// the two catch different mistakes: the plan assertion catches the
    /// planner changing its mind, this catches someone "simplifying" the
    /// recursion back to the `select distinct` that PostgreSQL cannot serve
    /// from an index at all.
    #[test]
    fn the_kinds_read_is_a_loose_index_scan_and_not_a_distinct() {
        assert!(
            RECENT_KINDS_SQL.contains("with recursive"),
            "{RECENT_KINDS_SQL}"
        );
        assert!(!RECENT_KINDS_SQL.contains("distinct"), "{RECENT_KINDS_SQL}");
        // The step that makes it a *scan* rather than a loop over a table.
        assert!(
            RECENT_KINDS_SQL.contains("i.kind > k.kind"),
            "{RECENT_KINDS_SQL}"
        );
    }

    /// Twice, and both matter: the inner one bounds each kind's branch, the
    /// outer one bounds the merge. Dropping the inner one makes the board read
    /// every row of the busiest kind.
    #[test]
    fn the_limit_binds_inside_the_lateral_and_outside_it() {
        assert_eq!(RECENT_SQL.matches("limit $2").count(), 2, "{RECENT_SQL}");
        // And the ordering the index provides is spelled the same in both
        // places, or the merge re-sorts what the lateral already ordered.
        assert_eq!(
            RECENT_SQL
                .matches("item_updated_at desc nulls last")
                .count(),
            2,
            "{RECENT_SQL}"
        );
    }
}
