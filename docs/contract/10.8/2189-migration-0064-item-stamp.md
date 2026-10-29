- **Migration `0064` — issue #2189 (2026-10-26): a re-sync of an unchanged mirror row stamps its
  `synced_at` in place, and the triggers on `sync.item` do no work for rows nothing moved in.**
  Ruled by the implementer on Opus; the rulings are in `docs/archive/2026-10-unattended-rulings.md`
  under `## #2189`. It edits no earlier migration. No command, no IPC schema change, no event, no
  DTO field, no `WriteOp`, nothing in `crates/knobas-source/src/**`, `crates/knobas-http/**`,
  `error.rs` or `profile.rs`, no new module in either barrel or in `commands/`.

  **Migration `0064`: one storage parameter, two restated functions.** No table, column, index or
  trigger is added or dropped.
  - `alter table sync.item set (fillfactor = 50)`: a page keeps room for a second version of each
    row, so an update that changes no indexed column (the stamp of `synced_at`) is heap-only. The
    heap doubles; existing pages are laid out again by the first writes that move their rows.
  - `sync.item_inserted()` returns before taking `0028`'s lock when the statement inserted no row
    (an `insert ... on conflict do update` fires it with none).
  - `sync.item_updated()` calls the five resolvers only when some row changed `source_id`, `kind`
    or `payload`, and rewrites `at` in `sync.item_parent`, `sync.item_project` and
    `sync.item_pipeline` only for the rows whose `coalesce(item_updated_at, synced_at)` or
    `item_updated_at` moved, reading their new time from `sync.item` by key. Its two joins of the
    transition tables are `execute`d, so each is planned on the statement's row counts. While the
    side tables hold their item's time, it writes what `0063`'s wrote.

  **The writer** (`knobas-sync`, no frozen surface): `ENTITY_UPSERT` and `ITEM_UPSERT` leave out a
  row whose stored columns already hold what arrived, and `ITEM_STAMP` sets `synced_at = now()` on
  every row of the batch that does not carry it yet. The sweep's `synced_at < now()` is unchanged.

  **Backups.** The `sync` schema is in no archive (§16.12). `RENAMED_COLUMNS` needs no row.

  Pinned by `knobas-sync`'s `tests::a_resync_of_an_unchanged_item_only_stamps_it` (every row the
  run saw carries its time, and every unchanged row's write is heap-only) and
  `tests::the_upserts_leave_an_unchanged_row_alone`; the trigger's moved rows by `knobas-search`'s
  `item_parent::an_undated_items_parents_take_its_new_sync_time`; the sweep and the side tables'
  `at` by the existing `tests/it/sweep.rs` and the item-parent, project and pipeline suites.
