//! `sync.item_row` (migration `0081`): what a list draws of an item of any
//! kind, as the database keeps it. Its path line, its state chip, and the
//! declared strings Code's attention order reads, which a list takes where
//! an item has a row ([`crate::row_join!`]) and walks the payload for where
//! it has none.
//!
//! A write to `sync.item` keeps its items' rows in the same transaction.
//! What has none is an item written before the migration, and an item whose
//! kind's declaration changed, which drops its row: the fill writes those, a
//! batch at a time, behind the app's bring-up and after the cards
//! ([`crate::item_card::fill`]).

use sqlx::PgPool;

use crate::CoreError;

/// How many items one batch looks at: about 60 ms of the database's time on
/// a mirror of 39,770 items, so a sync or a read waits on no more.
pub const BATCH: i32 = 500;

/// The CTE `row_keys (source_id, kind, k)`: for each source and kind the
/// declarations `$decl` name, the key an item's stored row is read under for
/// those declarations (`sync.row_key`), computed once a statement.
#[macro_export]
macro_rules! row_keys {
    ($decl:literal) => {
        concat!(
            "row_keys as materialized (select d.source_id, k.kind, sync.row_key(k.declared) as k \
               from jsonb_each(case when jsonb_typeof(",
            $decl,
            "::jsonb) = 'object' then ",
            $decl,
            "::jsonb else '{}'::jsonb end) as d(source_id, kinds) \
              cross join lateral jsonb_each(case when jsonb_typeof(d.kinds) = 'object' \
                                                 then d.kinds else '{}'::jsonb end) as k(kind, declared))"
        )
    };
}

/// After a relation `$row` that carries `entity_id`, `source_id` and `kind`:
/// the item's stored row as `$alias`, read under the key [`row_keys!`] gives
/// its source and kind, so a joined row holds each field as the payload
/// macros read it under the bound declarations. Null columns where there is
/// no such row: an item not filled yet, a source whose declarations the
/// database did not store. Read a field through [`row_or!`].
#[macro_export]
macro_rules! row_join {
    ($alias:literal, $row:literal) => {
        concat!(
            " left join row_keys ",
            $alias,
            "_k on ",
            $alias,
            "_k.source_id = ",
            $row,
            ".source_id and ",
            $alias,
            "_k.kind = ",
            $row,
            ".kind left join sync.item_row ",
            $alias,
            " on ",
            $alias,
            ".entity_id = ",
            $row,
            ".entity_id and ",
            $alias,
            ".paths_key = ",
            $alias,
            "_k.k "
        )
    };
}

/// `$alias.$column` of the row [`row_join!`] joined, and `$walk`, the read
/// of the payload the statement made before, where the item has no row.
#[macro_export]
macro_rules! row_or {
    ($alias:literal, $column:literal, $($walk:tt)+) => {
        concat!(
            "(case when ",
            $alias,
            ".entity_id is null then ",
            $($walk)+,
            " else ",
            $alias,
            ".",
            $column,
            " end)"
        )
    };
}

/// Fill every item that has no stored row, [`BATCH`] at a time in id order,
/// each batch its own transaction under the shared key the item triggers
/// take (`sync.fill_item_rows`). A batch writes only the rows no one wrote
/// meanwhile, so a sync running beside it keeps its own. Answers how many
/// batches it ran.
///
/// # Errors
///
/// [`CoreError::Db`] if a batch fails; the batches before it are kept, and
/// the next fill goes on from the first item without a row.
pub async fn fill(pool: &PgPool) -> Result<u32, CoreError> {
    let mut after = String::new();
    let mut batches = 0;
    loop {
        let next: Option<String> = sqlx::query_scalar("select sync.fill_item_rows($1, $2)")
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
