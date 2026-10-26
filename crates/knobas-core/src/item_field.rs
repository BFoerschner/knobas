//! A declared field read through `sync.item_field` (#2184, migration `0063`,
//! ADR-0019's pattern): the database keeps each item's status, assignee
//! and project as [`crate::declared_string!`] reads them, and a read that
//! would walk every ticket's payload for one of them probes that table by
//! the item's id instead.

/// A row's declared string for `$field`, stored where it can be and walked
/// where it cannot: the stored value of `i`'s `$field` whose `paths` are the
/// candidates `$decl` gives `i`'s source and kind, and `$walk` -- the read
/// the statement made before, which must be [`crate::first_string_of!`]'s
/// rule over those same candidates -- where no stored row has them.
///
/// **Equal to `$walk` by construction.** A stored value is
/// [`crate::declared_string!`] over the stored paths and the item's record,
/// recomputed whenever either changes; a row is read only when its paths
/// are the ones the statement binds. So a read binding declarations the
/// database never stored -- a test's own, an adapter not yet written down
/// -- walks, as it always did, and so does an item whose kind declares no
/// such field. Pinned by `knobas-app`'s `tests/it/item_field.rs`.
///
/// One index probe per row (`item_field_entity_idx`), where the walk
/// detoasted the payload: about 2 µs against 20 µs for a Jira ticket's
/// status on 2,500 tickets. `offset 0` keeps the probe one subplan rather
/// than one per reference.
///
/// `$decl` is where the statement binds the declarations; `i` is the
/// `sync.item` row, as for every payload macro.
#[macro_export]
macro_rules! stored_string {
    ($decl:literal, $field:literal, $($walk:tt)+) => {
        concat!(
            "(select case when sf.a is null then ",
            $($walk)+,
            " else sf.a[1] end from (select (select array[f.value] from sync.item_field f \
              where f.entity_id = i.entity_id and f.field = '",
            $field,
            "' and f.paths = (",
            $decl,
            "::jsonb #> array[i.source_id, i.kind, '",
            $field,
            "']) limit 1) as a offset 0) sf)"
        )
    };
}

/// [`stored_string!`] for a statement that reads one field of many tickets:
/// the same answer, joined rather than probed per row (#2184). On a mirror
/// of small records the per-row probe cost what the walk it replaced did
/// (2 µs a ticket on 15,000 records of a few hundred bytes), where a hash
/// join costs a fraction of it.
///
/// After `from sync.item i`: the stored row of `i`'s `$field` whose `paths`
/// are the candidates `$decl` gives `i`'s source and kind, as `$alias`
/// (`sync.item_field` holds at most one per item, field and paths). Read it
/// through [`stored_or!`].
#[macro_export]
macro_rules! stored_join {
    ($alias:literal, $decl:literal, $field:literal) => {
        concat!(
            " left join sync.item_field ",
            $alias,
            " on ",
            $alias,
            ".entity_id = i.entity_id and ",
            $alias,
            ".field = '",
            $field,
            "' and ",
            $alias,
            ".paths = (",
            $decl,
            "::jsonb #> array[i.source_id, i.kind, '",
            $field,
            "']) "
        )
    };
}

/// [`stored_join!`]'s value where there is a row, `$walk` -- the read the
/// statement made before, [`crate::first_string_of!`]'s rule over the same
/// candidates -- where there is none.
#[macro_export]
macro_rules! stored_or {
    ($alias:literal, $($walk:tt)+) => {
        concat!(
            "(case when ",
            $alias,
            ".entity_id is null then ",
            $($walk)+,
            " else ",
            $alias,
            ".value end)"
        )
    };
}
