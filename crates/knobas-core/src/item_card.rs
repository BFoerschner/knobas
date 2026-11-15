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
/// on a mirror of Jira tickets, so a sync or a read waits on no more. 300
/// since migration `0089` put the title, date, words and project on the card,
/// when 500 took 68 ms.
pub const BATCH: i32 = 300;

/// Fill every ticket that has no stored card, [`BATCH`] at a time in id
/// order, each batch its own transaction under the shared key the item
/// triggers take (`sync.fill_item_cards`), then mark the sources whose cards
/// are whole ([`mark_whole`]). A batch writes only the rows no one wrote
/// meanwhile, so a sync running beside it keeps its own. Answers how many
/// batches it ran.
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
            break;
        };
        batches += 1;
        after = next;
    }
    // A sync holding the key now is marked by the next board that finds its
    // scope not whole ([`mark_whole_soon`]).
    mark_whole(pool).await?;
    Ok(batches)
}

/// Recompute which sources' cards are whole (`sync.card_whole`, migration
/// `0089`): each source and key whose cards are exactly the source's
/// tickets, which the board then reads off the cards alone
/// (`crate::mini_board`). Under the key the item triggers share, taken
/// exclusive or not at all: answers `false`, having changed nothing, while a
/// write holds it.
///
/// # Errors
///
/// [`CoreError::Db`] if the statement fails.
pub async fn mark_whole(pool: &PgPool) -> Result<bool, CoreError> {
    Ok(sqlx::query_scalar("select sync.mark_cards_whole()")
        .fetch_one(pool)
        .await?)
}

/// The least time between two marks [`mark_whole_soon`] makes.
const MARK_EVERY: std::time::Duration = std::time::Duration::from_secs(30);

/// When [`mark_whole_soon`] last marked, in this process.
static MARKED_AT: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// [`mark_whole`], unless this process marked within [`MARK_EVERY`]: what a
/// board read that found its scope not whole runs beside the board it then
/// reads, so cards a sync made whole since bring-up's mark are read alone from
/// the next read on, a source that stays not whole (declarations the database
/// did not store) costs one mark a half minute, and a failed mark is logged
/// and the read goes on.
pub async fn mark_whole_soon(pool: &PgPool) {
    {
        let mut at = MARKED_AT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if at.is_some_and(|at| at.elapsed() < MARK_EVERY) {
            return;
        }
        *at = Some(std::time::Instant::now());
    }
    if let Err(error) = mark_whole(pool).await {
        tracing::warn!(%error, "the whole cards could not be marked");
    }
}
