//! The fill of `sync.word` (migration `0100`, the fuzzy-search pass,
//! 2026-11-19, D10): every word the mirror's titles and bodies hold, as
//! written, which the launcher reads a typed word against
//! (`knobas_search::picker::Picker::reading`).
//!
//! A write to `sync.item` writes its row's words in the same transaction.
//! What has none is a row mirrored before the migration: the fill reads
//! those, a batch at a time in id order, behind the app's bring-up, and
//! keeps its place in `sync.word_filled_to`, since a word does not say which
//! row gave it. Until the list is marked whole (`sync.word_whole`) the
//! launcher corrects no word.

use sqlx::PgPool;

use crate::CoreError;

/// How many items one batch reads: 150, as the other fills, about 14 ms of
/// the database's time (0.095 ms an item parsed on Björn's scale).
pub const BATCH: i32 = 150;

/// Read the words of every item past the fill's place, [`BATCH`] items at a
/// time, each batch its own transaction (`sync.fill_words_beside`), then
/// mark the list whole (`sync.mark_words_whole`). A batch stops before the
/// first item a running sync holds; the fill waits
/// [`crate::fill::AGAIN_AFTER`] and reads it then. Answers how many batches
/// it ran: none on a list already marked whole, which is every start after
/// the first.
///
/// # Errors
///
/// [`CoreError::Db`] if a batch fails; the batches before it are kept, and
/// the next fill goes on from its place.
pub async fn fill(pool: &PgPool) -> Result<u32, CoreError> {
    if whole(pool).await? {
        return Ok(0);
    }
    let mut batches = 0;
    loop {
        let (read, held): (i32, bool) =
            sqlx::query_as("select read, held from sync.fill_words_beside($1)")
                .bind(BATCH)
                .fetch_one(pool)
                .await?;
        if read > 0 {
            batches += 1;
        }
        if held && read == 0 {
            tokio::time::sleep(crate::fill::AGAIN_AFTER).await;
            continue;
        }
        if read == 0 {
            break;
        }
    }
    sqlx::query("select sync.mark_words_whole()")
        .execute(pool)
        .await?;
    Ok(batches)
}

/// Whether every item's words have been read (`sync.word_whole`): one probe
/// of a table of one row. The triggers write a new item's words with it, so
/// a list found whole stays so.
///
/// # Errors
///
/// [`CoreError::Db`] if the probe fails.
pub async fn whole(pool: &PgPool) -> Result<bool, CoreError> {
    Ok(
        sqlx::query_scalar("select exists (select from sync.word_whole)")
            .fetch_one(pool)
            .await?,
    )
}
