//! `sync.item_run` (migration `0088`): what Builds reads of a run, and a
//! commit's own sha, as the database keeps them. A run's fields as
//! [`crate::builds`]'s readers walk them out of its payload, which they take
//! where a run has a row ([`crate::run_join!`]) and walk the payload for where
//! it has none; and the declared `commit` of a commit, which the runs' commits
//! are found by.
//!
//! A write to `sync.item` keeps its items' rows in the same transaction. What
//! has none is an item written before the migration, and an item whose
//! kind's declaration changed, which drops its row: the fill writes those, a
//! batch at a time, behind the app's bring-up and after the lists' rows
//! ([`crate::item_row::fill`]).

use sqlx::PgPool;

use crate::CoreError;

/// How many items one batch looks at: about 50 ms of the database's time, so
/// a sync or a read waits on no more.
pub const BATCH: i32 = 500;

/// The CTE `run_keys (source_id, kind, k)`: for each source and kind the
/// declarations `$decl` name, the key an item's stored run row is read under
/// for those declarations (`sync.run_key`), computed once a statement.
#[macro_export]
macro_rules! run_keys {
    ($decl:literal) => {
        concat!(
            "run_keys as materialized (select d.source_id, k.kind, sync.run_key(k.declared) as k \
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
/// the item's stored run row as `$alias`, read under the key [`run_keys!`]
/// gives its source and kind, so a joined row holds each field as the payload
/// macros read it under the bound declarations. Null columns where there is
/// no such row: an item not filled yet, a source whose declarations the
/// database did not store. Read a field through [`crate::row_or!`].
#[macro_export]
macro_rules! run_join {
    ($alias:literal, $row:literal) => {
        concat!(
            " left join run_keys ",
            $alias,
            "_k on ",
            $alias,
            "_k.source_id = ",
            $row,
            ".source_id and ",
            $alias,
            "_k.kind = ",
            $row,
            ".kind left join sync.item_run ",
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

/// Fill every item of the kinds `sync.item_run` keeps that has no row,
/// [`BATCH`] at a time in id order, each batch its own transaction under the
/// shared key the item triggers take (`sync.fill_item_runs_beside`, migration
/// `0091`). A batch writes only the rows no one wrote meanwhile and steps past
/// an item a running sync holds, and the fill passes again while it stepped
/// past any ([`crate::fill`]): a sync running beside it keeps its own, and the
/// two never deadlock. Answers how many batches it ran.
///
/// # Errors
///
/// [`CoreError::Db`] if a batch fails; the batches before it are kept, and
/// the next fill goes on from the first item without a row.
pub async fn fill(pool: &PgPool) -> Result<u32, CoreError> {
    crate::fill::walk(
        pool,
        "select last, skipped from sync.fill_item_runs_beside($1, $2)",
        BATCH,
    )
    .await
}
