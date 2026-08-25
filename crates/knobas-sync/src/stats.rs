//! Live numbers for the status bar and the diagnostics view (§3: "FTS index
//! state, re-index button, DB size").
//!
//! Computed live rather than kept in a table: `pg_database_size` and two counts
//! are cheap, and a cached copy would be a second truth every sync run has to
//! remember to update.

use chrono::{DateTime, Utc};
use sqlx::PgPool;

#[derive(Debug, Clone, serde::Serialize)]
pub struct DbStats {
    pub db_bytes: i64,
    pub entity_count: i64,
    pub item_count: i64,
    pub per_source: Vec<SourceCount>,
    pub oldest_synced_at: Option<DateTime<Utc>>,
    pub newest_synced_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SourceCount {
    pub source_id: String,
    pub items: i64,
    pub synced_at: Option<DateTime<Utc>>,
}

/// # Errors
/// [`sqlx::Error`] if either query fails.
pub async fn db_stats(pool: &PgPool) -> Result<DbStats, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct Totals {
        db_bytes: i64,
        entity_count: i64,
        item_count: i64,
        oldest_synced_at: Option<DateTime<Utc>>,
        newest_synced_at: Option<DateTime<Utc>>,
    }
    // `pg_database_size` returns bigint; the counts are bigint. Named columns
    // rather than `select *` from anything holding a tsvector.
    let totals: Totals = sqlx::query_as(
        r"select pg_database_size(current_database())::bigint as db_bytes,
                 (select count(*) from knobas.entity)         as entity_count,
                 (select count(*) from sync.item)             as item_count,
                 (select min(synced_at) from sync.item)       as oldest_synced_at,
                 (select max(synced_at) from sync.item)       as newest_synced_at",
    )
    .fetch_one(pool)
    .await?;

    #[derive(sqlx::FromRow)]
    struct PerSource {
        source_id: String,
        items: i64,
        synced_at: Option<DateTime<Utc>>,
    }
    let rows: Vec<PerSource> = sqlx::query_as(
        r"select source_id, count(*) as items, max(synced_at) as synced_at
            from sync.item group by source_id order by source_id",
    )
    .fetch_all(pool)
    .await?;

    Ok(DbStats {
        db_bytes: totals.db_bytes,
        entity_count: totals.entity_count,
        item_count: totals.item_count,
        oldest_synced_at: totals.oldest_synced_at,
        newest_synced_at: totals.newest_synced_at,
        per_source: rows
            .into_iter()
            .map(|r| SourceCount {
                source_id: r.source_id,
                items: r.items,
                synced_at: r.synced_at,
            })
            .collect(),
    })
}

/// The index the re-index button rebuilds.
///
/// Named once: `reindex` takes an identifier, not a bind parameter, so this is
/// interpolated -- and a constant of this crate is the audit `AssertSqlSafe`
/// asks for. `knobas.note_fts_idx` joins it when notes land in M2.
pub const FTS_INDEX: &str = "sync.item_fts_idx";

/// Rebuild the FTS index (§3, "re-index button").
///
/// `concurrently`, and therefore **not** inside a transaction -- PostgreSQL
/// refuses `REINDEX CONCURRENTLY` in a transaction block, and the pool's
/// autocommit is what makes this legal. The alternative would lock the mirror
/// against every search for the length of the rebuild.
///
/// # Errors
/// [`sqlx::Error`] if the rebuild fails.
pub async fn reindex_fts(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "reindex index concurrently {FTS_INDEX}"
    )))
    .execute(pool)
    .await?;
    Ok(())
}
