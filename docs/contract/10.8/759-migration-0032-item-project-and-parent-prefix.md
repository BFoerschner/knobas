- **Migration `0032` — issues #759 and #761 (2026-10-14): the project census reads stored rows,
  and `sync.item_parent` has a prefix index on its name and its item's time.** Ruled by the deputy for Björn on #759 and
  #761 on 2026-10-14 ("both (1) and (2)" on #759, "the prefix answer" on #761, one migration for
  both). It follows `0028`'s and `0030`'s pattern (ADR-0019) and edits no earlier migration.

  **Migration `0032`: one table, one column, three indexes, five functions, four restated
  functions.** No trigger is added or dropped.
  - `sync.item_project (entity_id, adapter_kind, source_id, key, name, at)`, `entity_id`
    referencing `sync.item` `on delete cascade`. One row per item per stored adapter that declares
    a `project_key` for the item's kind, when the key resolves: `key` is
    `knobas_core::project_key_read!`'s answer, `name` is `declared_string!(…, "project_name")`'s or
    null, `source_id` is the item's, copied (like `0029`'s on `sync.item_parent`), and `at` is the
    item's `coalesce(item_updated_at, synced_at)`. Indexed on `(source_id, key, adapter_kind,
    (name is null), at desc, name)` (`item_project_census_idx`, the census's) and on `entity_id`
    (the resolver's delete).
  - `item_parent_name_prefix_idx` on `sync.item_parent (lower(name) text_pattern_ops)`, the range
    #761's prefix read uses. `0028`'s `item_parent_name_idx` stays for `under:`'s equality lookup.
  - `sync.item_parent.at`, the item's `coalesce(item_updated_at, synced_at)`, copied as
    `sync.item_project.at` is, so #761's ranking reads `sync.item_parent` alone. `0029`'s
    `sync.resolve_item_parents` is restated with that one column added and both expansions
    unchanged; the migration fills it on every existing row with one `update`. Joining each row to
    `sync.item` for its time instead was 196–260 ms for a broad prefix on tables never analyzed.
  - `sync.project_kinds_reached(jsonb)`, `sync.project_adapters()`,
    `sync.resolve_item_projects(text[])`, `sync.resolve_kind_projects(text[])` and
    `sync.resolve_all_item_projects()`: `0030`'s revision functions' counterparts. The resolver
    carries both macros' expansions as text, `$decl` bound per adapter as `{<source_id>: <kinds>}`.
  - `sync.item_inserted()`, `sync.item_updated()` and `sync.parent_declaration_changed()` are
    restated to resolve all three tables. `item_updated()` also rewrites `at` in place, on
    `sync.item_parent` and `sync.item_project`, for every updated row whose time moved, whatever
    else changed. A row whose time did not move is not written; an item whose source reports no
    `updated` moves with every `synced_at`. The lock and its invariant are `0028`'s, unchanged.
  - The stored rows depend on the item and on `sync.parent_declaration` and on nothing else. The
    adapter is matched at read time, so there is still no trigger on `knobas.source_config`. The
    last statement fills every existing row and takes no lock.

  **Reader** (amends #208's census and #277's "resolved per read"). `knobas_core::project::list`
  takes the pool only (its `declarations` argument is gone) and reads `sync.item_project`: the
  distinct `(source_id, key, adapter_kind)` joined to `knobas.source_config` on the adapter and on
  `coalesce(enabled, true)`, then per project a `limit 1` walk of `item_project_census_idx`
  joined to `sync.live_item`. Same answer as before: one row per project, the newest readable name
  over none, ordered by source then key. `list_projects_inner` stores the declarations first
  (`commands::search::searcher`, one read when nothing changed), as every search command does.
  `Project` and `list_projects` keep their shape.

  **What is not touched.** No command, event or DTO field.

  Measured with `just responsiveness-probe` (22,000 items, no `ANALYZE`): `list_projects` 401 ms
  warm median before (load 8.2 to 17.3), 6.0 ms after (load 2.9). With fresh statistics the old
  read was 57 ms, so the gain is not a planner artefact.

  Pinned by `knobas-app`'s `tests/it/project_census.rs`
  (`the_census_is_the_walk_after_every_change`, which holds `list_projects` to the pre-#759 walk
  after an insert batch with a rename and a nameless item, a payload rename, a time-only re-sync,
  a tombstone, a source turned off and on, an adapter kind change and back, a declaration change
  and restore, and a delete; and `the_stored_resolver_is_the_macros_expansion`), and by
  `knobas-core`'s `tests/it/projects.rs`, which now stores its declarations and configures its
  sources. Mutants, each on a committed baseline, each killed: the update trigger not resolving
  projects; `at` not refreshed on a time-only update; the census without the adapter match; the
  census ordering oldest first.
