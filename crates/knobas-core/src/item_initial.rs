//! The fill of `sync.item_initial` (migrations `0093` to `0095`): for each
//! mirror row, the first characters of the words it is named by, and one row
//! under no characters at all, in the order the launcher answers a word's
//! first characters in and walks a kind's rows newest first in
//! (`knobas-search`'s `letter_sql` and `broad_sql`).
//!
//! A write to `sync.item` keeps its row's rows in the same transaction. What
//! has none is a row mirrored before the migrations: the fill writes those,
//! a batch at a time, behind the app's bring-up. `0093` and `0094` wrote
//! them in one pass each before the window opened, 2.6 and 3.4 s on Björn's
//! scale (the long-words pass, 2026-11-16, took the passes out). Until the
//! table is marked whole the launcher ranks every match as it did before
//! `0093`, and gives the same answers it gave then
//! (`knobas_search::corpus::stored`).

use sqlx::PgPool;

use crate::CoreError;

/// How many items one batch looks at: about 21 narrow rows an item, 40 ms
/// of the database's time on Björn's scale, so a sync or a search waits on
/// no more.
pub const BATCH: i32 = 150;

/// Give every item that has no row under the empty start all its rows,
/// [`BATCH`] items at a time in id order, each batch its own transaction
/// (`sync.fill_item_initial_beside`, migration `0095`), then vacuum and
/// analyze the table and mark it whole (`sync.mark_item_initial_whole`). A
/// batch steps past an item a running sync holds, and the walk passes over
/// the items again while it stepped past any ([`crate::fill`]). Answers how
/// many batches it ran: none on a table already marked whole, which is
/// every start after the first.
///
/// # Errors
///
/// [`CoreError::Db`] if a batch fails; the batches before it are kept, and
/// the next fill goes on from the first item without its rows.
pub async fn fill(pool: &PgPool) -> Result<u32, CoreError> {
    if whole(pool).await? {
        return Ok(0);
    }
    let batches = crate::fill::walk(
        pool,
        "select last, skipped from sync.fill_item_initial_beside($1, $2)",
        BATCH,
    )
    .await?;
    // The planner's counts and the visibility map before the first read:
    // the launcher's walks are of the index alone.
    sqlx::query("vacuum (analyze) sync.item_initial")
        .execute(pool)
        .await?;
    sqlx::query("select sync.mark_item_initial_whole()")
        .execute(pool)
        .await?;
    Ok(batches)
}

/// Whether every item has its rows (`sync.item_initial_whole`): one probe of
/// a table of one row. No item goes back, so a table found whole stays so.
///
/// # Errors
///
/// [`CoreError::Db`] if the probe fails.
pub async fn whole(pool: &PgPool) -> Result<bool, CoreError> {
    Ok(
        sqlx::query_scalar("select exists (select from sync.item_initial_whole)")
            .fetch_one(pool)
            .await?,
    )
}
