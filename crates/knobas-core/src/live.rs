//! That an item is **live** (`CONTEXT.md`) -- its entity not withdrawn, its
//! source not turned off -- for a statement that reads `sync.item` itself
//! rather than through `sync.live_item` (#778).
//!
//! The view joins `knobas.entity` for `deleted_at is null`, and on tables never
//! analyzed -- a fresh install until the first analyze after its first sync --
//! the planner reads that filter as 0.5 % of the table. Every read of the view
//! is then planned from `knobas.entity`, with a probe into `sync.item` per
//! entity, and a statement that joins what it read to anything else does so by
//! nested loops over rows it thinks are few: the *All* room's builds tile took
//! 6.8 s on the probe's 22,000 items, 23 ms analyzed (#775). The two shapes
//! here take liveness out of the join order, so no row estimate can drive a
//! plan from it.

/// Live, as two **correlated primary-key lookups** in the `where`: the
/// entity's `deleted_at` and the source's `enabled`, once per row that reaches
/// them (#768's shape, #770's `live!`).
///
/// For a statement that reads few rows: a page cut by an index in order (the
/// launcher's newest of each kind) or a rule whose cheaper predicates have
/// already narrowed the rows. PostgreSQL evaluates a subquery after the cheap
/// predicates beside it, so each row pays two index probes only once it
/// would otherwise be kept.
#[macro_export]
macro_rules! live_by_key {
    ($alias:literal) => {
        concat!(
            "(select e.deleted_at is null from knobas.entity e where e.id = ",
            $alias,
            ".entity_id)
         and coalesce((select c.enabled from knobas.source_config c where c.id = ",
            $alias,
            ".source_id), true)"
        )
    };
}

/// Live, as two **hashed exclusion sets**: the withdrawn entities and the
/// turned-off sources, each read once per statement and probed per row.
///
/// For a statement that reads a whole room or kind: the builds tile, the
/// board, the inbox's rules. Measured on the probe's 2,900-item mirror, the
/// builds tile's scan took 5.7 ms never analyzed and 6.4 ms analyzed this way,
/// against 9.6 and 10.3 ms with [`live_by_key!`]'s two probes a row and 196
/// and 6.5 ms through the view.
///
/// Each set is a `not in` over an uncorrelated subquery, which PostgreSQL
/// cannot turn into a join and runs as a hashed subplan: a scan of
/// `knobas.entity` for its tombstones, and of `knobas.source_config`. Its
/// cost is the entity table's size once, not a probe per row. Both ids are
/// primary keys, so the `not in` has no null to trip on.
///
/// **What it cannot survive:** the planner hashes the set only while its
/// estimate fits in `work_mem` times `hash_mem_multiplier` (8 MB by default,
/// about 150,000 ids). Past that it rescans the materialized set per row, which
/// costs the row count times the *actual* number of tombstones -- nothing on a
/// fresh install, whose first sync has withdrawn nothing, and a quadratic read
/// only on a mirror holding that many withdrawn entities.
#[macro_export]
macro_rules! live_by_exclusion {
    ($alias:literal) => {
        concat!(
            $alias,
            ".entity_id not in (select e.id from knobas.entity e where e.deleted_at is not null)
         and ",
            $alias,
            ".source_id not in (select c.id from knobas.source_config c where not c.enabled)"
        )
    };
}
