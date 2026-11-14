//! The fill of `sync.item_search`'s six columns (migration `0084`): each
//! mirror row's kind, source, date and the words it is named by, beside its
//! search vector, which the launcher's match stage reads in place of the
//! row itself (`knobas-search`'s `LIVE_ITEM_MATCH`), once every row has
//! them.
//!
//! A write to `sync.item` keeps its row's columns in the same transaction.
//! What has none is a row written before the migration: the fill writes
//! those, a batch at a time, behind the app's bring-up, and stores each
//! one's vector again so it lands in the row and not beside the table.
//! Until every row is filled the launcher matches through the mirror's own
//! rows, as before the migration, and gives the same answer
//! (`knobas_search::corpus::mirror`).

use sqlx::PgPool;

use crate::CoreError;

/// How many rows one batch writes: about 40 ms of the database's time on a
/// mirror whose rows hold a kilobyte of vector each, so a sync or a search
/// waits on no more.
pub const BATCH: i32 = 250;

/// After how many batches the table is vacuumed. Each batch leaves the
/// versions it replaced behind, and on a table whose pages were full every
/// one is a new page: vacuumed every 4,000 rows, the later batches write
/// into the room the earlier ones left, 74 MB of table on Björn's scale
/// where one vacuum at the end left 94.
const VACUUM_EVERY: u32 = 16;

/// Fill every row whose `kind` is null, [`BATCH`] at a time in id order,
/// each batch its own transaction (`sync.fill_item_search`), then vacuum and
/// analyze the table, so the planner counts the new columns and the pages
/// the old versions held are free. A batch writes only rows still unfilled
/// when it reaches them, so a sync running beside it keeps its own. Answers
/// how many batches it ran; none, and nothing is vacuumed, on a mirror
/// already filled.
///
/// # Errors
///
/// [`CoreError::Db`] if a batch fails; the batches before it are kept, and
/// the next fill goes on from the first unfilled row.
pub async fn fill(pool: &PgPool) -> Result<u32, CoreError> {
    let mut after = String::new();
    let mut batches = 0;
    loop {
        let next: Option<String> = sqlx::query_scalar("select sync.fill_item_search($1, $2)")
            .bind(&after)
            .bind(BATCH)
            .fetch_one(pool)
            .await?;
        let Some(next) = next else {
            break;
        };
        batches += 1;
        after = next;
        if batches % VACUUM_EVERY == 0 {
            sqlx::query("vacuum sync.item_search").execute(pool).await?;
        }
    }
    if batches > 0 {
        sqlx::query("vacuum (analyze) sync.item_search")
            .execute(pool)
            .await?;
    }
    Ok(batches)
}
