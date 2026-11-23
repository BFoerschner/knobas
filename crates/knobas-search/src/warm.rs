//! The launcher's reads made ready after the app starts (the ux30 end gate,
//! 2026-11-23; `docs/decisions/2026-11-23-the-launcher-is-ready-before-the-first-key.md`).
//!
//! Two things made the first broad word typed after a start slower than the
//! same word a minute later, by 20 to 60 ms of backend on the Mac:
//!
//! * **Its rows were not in PostgreSQL's buffers.** A word more than 2,000 rows
//!   hold is counted before it is answered ([`crate::sql::probe_sql`]): 2,001
//!   of its rows and every row it names, each off its own page of the search
//!   copies. `conf` read 2,525 pages of `sync.item_search` that way, 122 ms on
//!   a copy of Björn's scale nothing had read yet, 30 ms where only the
//!   system's file cache held them, 5.6 ms from the buffers. Every word's
//!   first characters land on other pages, so each new word paid it again.
//! * **Autovacuum's analyze dropped the kept plans.** A table written by the
//!   session before (a fill, a sync) is analyzed about a minute after the
//!   next start, and an analyze throws away every plan that reads the table.
//!   The launcher's statements are planned once a connection and kept
//!   ([`crate::sql::fetch_kept`]); `sync.item_initial`'s analyze made each of
//!   the five connections plan them again, 9 to 11 ms on the key that met it.
//!
//! [`warm`] does both before anyone is likely to type: it analyzes the tables
//! autovacuum is owed, then reads [`WARMED`] into the buffers, in that order,
//! within half of `shared_buffers`. On its own connection, behind the app: on
//! a copy of the scale home the eleven tables it was owed took 172 ms to
//! analyze, and the 112 MB of [`WARMED`] 93 ms to read from a cold disk.

use sqlx::PgConnection;

/// What a word many rows hold is counted and answered from, in the order it
/// is read: the indexes of the names and of the words, the search copies'
/// key, the search copies themselves, then the rows each answer's page joins
/// (who is withdrawn, what a list draws of an item). The page's titles are
/// read from `sync.item`, which holds every payload and is not read whole.
pub const WARMED: &[&str] = &[
    "sync.item_search_fts_a_idx",
    "sync.item_search_fts_idx",
    "sync.item_search_pkey",
    "sync.item_search",
    "knobas.entity_pkey",
    "knobas.entity",
    "sync.item_pkey",
    "sync.item_row_pkey",
    "sync.item_row",
];

/// The tables autovacuum would analyze now: more rows written since their
/// last analyze than its own threshold, read from the same counters it reads
/// (`autovacuum_analyze_threshold` and `autovacuum_analyze_scale_factor` of
/// the planner's row count). Quoted, ready to name in a statement.
macro_rules! owed {
    () => {
        "select format('%I.%I', s.schemaname, s.relname)
  from pg_stat_user_tables s
  join pg_class c on c.oid = s.relid
 where s.n_mod_since_analyze >
       current_setting('autovacuum_analyze_threshold')::float8
       + current_setting('autovacuum_analyze_scale_factor')::float8 * greatest(c.reltuples, 0)
 order by 1"
    };
}
const OWED: &str = owed!();

/// Each table of [`OWED`] analyzed, by the server: the names come out of the
/// catalog and are quoted there, and no statement is built here (this crate
/// builds its runtime SQL in `sql.rs` alone). One transaction, which an
/// analyze may run in, unlike a vacuum.
const ANALYZE_OWED: &str = concat!(
    "do $$ declare t text; begin for t in ",
    owed!(),
    " loop execute 'analyze ' || t; end loop; end $$"
);

/// Each relation of [`WARMED`] this database holds, with its size in pages,
/// in that order.
const SIZES: &str = "
select w.name, pg_relation_size(to_regclass(w.name)) / current_setting('block_size')::bigint
  from unnest($1::text[]) with ordinality w(name, n)
 where to_regclass(w.name) is not null
 order by w.n";

/// What [`warm`] did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Warmed {
    /// The tables it analyzed, those autovacuum was owed.
    pub analyzed: Vec<String>,
    /// Each relation it read into the buffers, with the pages read.
    pub read: Vec<(String, i64)>,
    /// The relations left out because the buffers' half was spent.
    pub left: Vec<String>,
    /// Why nothing was read, where `pg_prewarm` could not be had.
    pub unread: Option<String>,
}

/// Analyze what autovacuum is owed, then read [`WARMED`] into the buffers.
///
/// A relation is read whole or not at all, in [`WARMED`]'s order, while the
/// pages read stay within half of `shared_buffers`; one that does not fit is
/// left out and the next is tried. Without `pg_prewarm` (a PostgreSQL built
/// without its contrib modules) nothing is read and the analyze still runs.
///
/// # Errors
///
/// [`sqlx::Error`] if a catalog read or an analyze fails.
pub async fn warm(conn: &mut PgConnection) -> Result<Warmed, sqlx::Error> {
    let mut warmed = Warmed {
        analyzed: sqlx::query_scalar(OWED).fetch_all(&mut *conn).await?,
        ..Warmed::default()
    };
    if !warmed.analyzed.is_empty() {
        sqlx::query(ANALYZE_OWED).execute(&mut *conn).await?;
    }
    if let Err(error) = sqlx::query("create extension if not exists pg_prewarm")
        .execute(&mut *conn)
        .await
    {
        warmed.unread = Some(error.to_string());
        return Ok(warmed);
    }
    let budget: i64 = sqlx::query_scalar(
        "select setting::bigint / 2 from pg_settings where name = 'shared_buffers'",
    )
    .fetch_one(&mut *conn)
    .await?;
    let names: Vec<String> = WARMED.iter().map(|name| (*name).to_owned()).collect();
    let sizes: Vec<(String, i64)> = sqlx::query_as(SIZES)
        .bind(&names)
        .fetch_all(&mut *conn)
        .await?;
    let mut spent = 0_i64;
    for (name, pages) in sizes {
        if spent + pages > budget {
            warmed.left.push(name);
            continue;
        }
        let read: i64 = sqlx::query_scalar("select pg_prewarm(to_regclass($1))")
            .bind(&name)
            .fetch_one(&mut *conn)
            .await?;
        spent += read;
        warmed.read.push((name, read));
    }
    Ok(warmed)
}
