//! The fill of `sync.item_card` (migration `0075`): each ticket's card as
//! the database keeps it, every field the board draws, which the board's
//! reads and the search's words take where a ticket has one
//! ([`crate::card_join!`]) and walk the payload for where it has none.
//!
//! A write to `sync.item` keeps its tickets' rows in the same transaction.
//! What has none is a ticket written before the migration, and every ticket
//! after a change of the adapters' declarations, which drops the rows: the
//! fill writes those, a batch at a time, behind the app's bring-up.

use sqlx::PgPool;

use crate::CoreError;

/// How many tickets one batch looks at: about 40 ms of the database's time
/// on a mirror of Jira tickets, so a sync or a read waits on no more.
pub const BATCH: i32 = 500;

/// Fill every ticket that has no stored card, [`BATCH`] at a time in id
/// order, each batch its own transaction under the shared key the item
/// triggers take (`sync.fill_item_cards`). A batch writes only the rows no
/// one wrote meanwhile, so a sync running beside it keeps its own. Answers
/// how many batches it ran.
///
/// # Errors
///
/// [`CoreError::Db`] if a batch fails; the batches before it are kept, and
/// the next fill goes on from the first ticket without a card.
pub async fn fill(pool: &PgPool) -> Result<u32, CoreError> {
    let mut after = String::new();
    let mut batches = 0;
    loop {
        let next: Option<String> = sqlx::query_scalar("select sync.fill_item_cards($1, $2)")
            .bind(&after)
            .bind(BATCH)
            .fetch_one(pool)
            .await?;
        let Some(next) = next else {
            return Ok(batches);
        };
        batches += 1;
        after = next;
    }
}
