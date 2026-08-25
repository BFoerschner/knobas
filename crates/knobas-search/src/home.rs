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
//! index-backed and bounded, and `tests/home.rs` asserts the plan rather than
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
            "       s.item_updated_at as updated_at, s.synced_at\n",
            "  from unnest($1::text[]) as k(kind)\n",
            "  cross join lateral (\n",
            "      select i.entity_id, i.kind, i.source_id, i.title,\n",
            "             i.item_updated_at, i.synced_at\n",
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
/// rows. One index-only scan of `item_kind_updated_idx`.
const RECENT_KINDS_SQL: &str = "select distinct kind from sync.item";

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
        })
        .collect())
}

/// The plan `recent` runs under, for the test that keeps it index-backed.
///
/// # Errors
///
/// [`SearchError::Db`] if the plan cannot be read.
#[cfg(any(test, feature = "test-util"))]
pub async fn explain_recent(pool: &sqlx::PgPool, limit: u32) -> Result<String, SearchError> {
    let kinds = kinds(pool).await?;
    let lines: Vec<String> = sqlx::query_scalar(EXPLAIN_RECENT_SQL)
        .bind(&kinds)
        .bind(i64::from(limit))
        .fetch_all(pool)
        .await?;
    Ok(lines.join("\n"))
}

async fn kinds(pool: &sqlx::PgPool) -> Result<Vec<String>, SearchError> {
    Ok(sqlx::query_scalar::<_, String>(RECENT_KINDS_SQL)
        .fetch_all(pool)
        .await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The plan variant has to be the *same* statement, or the assertion in
    /// `tests/home.rs` is about a query nobody runs.
    #[test]
    fn the_explained_statement_is_the_one_that_runs() {
        assert_eq!(
            EXPLAIN_RECENT_SQL
                .strip_prefix("explain (analyze false, costs false)\n")
                .expect("the plan variant is the statement with a prefix"),
            RECENT_SQL
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
