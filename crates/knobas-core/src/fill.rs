//! The fills' one walk (migration `0091`): bring-up's fills of the stored
//! rows an item has none of yet (`crate::item_card`, `crate::item_row`,
//! `crate::item_run`), a batch at a time, stepping past the items a running
//! sync has written.
//!
//! A batch takes its items `for share skip locked`, so the fill never waits
//! on a sync's run and the two cannot deadlock (they did, one run in three,
//! on a sync started right after an upgrade: the ux13 forge-load pass,
//! 2026-11-15). An item stepped past is one a running sync holds. Its own
//! trigger writes the row of an item it changed, but a run that locks an
//! item and leaves it unchanged writes none, so the walk passes over the
//! items again, after [`AGAIN_AFTER`], while a pass stepped past any.

use std::time::Duration;

use sqlx::PgPool;

use crate::CoreError;

/// How long the walk waits before passing over the items again, when a pass
/// stepped past some: a sync's run holds its items until it commits.
pub const AGAIN_AFTER: Duration = Duration::from_secs(2);

/// Run `statement` (`select last, skipped from sync.fill_item_*_beside($1,
/// $2)`, bound to the last item looked at and the batch's size) over every
/// item, `batch` at a time in id order, each batch its own transaction, and
/// again while a pass stepped past any. Answers how many batches it ran.
///
/// # Errors
///
/// [`CoreError::Db`] if a batch fails; the batches before it are kept.
pub(crate) async fn walk(
    pool: &PgPool,
    statement: &'static str,
    batch: i32,
) -> Result<u32, CoreError> {
    let mut batches = 0;
    loop {
        let mut after = String::new();
        let mut stepped_past = 0;
        loop {
            let (last, skipped): (Option<String>, i32) = sqlx::query_as(statement)
                .bind(&after)
                .bind(batch)
                .fetch_one(pool)
                .await?;
            stepped_past += skipped;
            let Some(last) = last else {
                break;
            };
            batches += 1;
            after = last;
        }
        if stepped_past == 0 {
            return Ok(batches);
        }
        tokio::time::sleep(AGAIN_AFTER).await;
    }
}
