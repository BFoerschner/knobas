- **Migration `0062` — issue #2185 (2026-10-26): `sync.item_pipeline`, the pipeline each run
  declares it belongs to, kept by the database as the run is written.** No command, no IPC schema
  change, no event, no DTO field, no `WriteOp`, nothing in `crates/knobas-source/src/**`,
  `crates/knobas-http/**`, `error.rs` or `profile.rs`, no new module in either barrel or in
  `commands/`. Every command keeps its arguments and its answer. Ruled by the implementer on Opus;
  the rulings are in `docs/decisions/2026-10-unattended-rulings.md` under `## #2185`.

  **The migration** (`0062_the_pipeline_a_run_belongs_to.sql`). No earlier migration edited.
  Transactional. One new table, `sync.item_pipeline (entity_id text collate "C" references
  sync.item on delete cascade, adapter_kind, source_id, pkey, at)`, with
  `item_pipeline_runs_idx (source_id, pkey, adapter_kind, at desc nulls last, entity_id desc)` and
  `item_pipeline_entity_idx (entity_id)`. Five new functions: `sync.pipeline_kinds_reached`,
  `sync.pipeline_adapters`, `sync.resolve_item_pipelines`, `sync.resolve_kind_pipelines`,
  `sync.resolve_all_item_pipelines`. Three restated, each with one addition and otherwise `0032`'s
  text: `sync.item_inserted` and `sync.item_updated` also resolve the pipelines of the ids they
  already resolve, and `sync.item_updated` rewrites `at` on every updated row whose stored `at`
  differs from its `item_updated_at`;
  `sync.parent_declaration_changed` also recomputes the kinds a changed declaration makes a run
  with a pipeline. The lock (`627, 0`) is unchanged. The last statement fills the table from the
  stored declarations, taking no lock, as `0032`'s did.

  **What a row is.** For each item and each stored adapter that declares the item's kind
  `frame = run` with a non-empty `pipeline`, at most one row: `pkey` is
  `knobas_core::declared_string!(…, "pipeline")` with `$decl` bound as `0028` binds it, pasted as
  its expansion; `source_id` is the item's; `at` its `item_updated_at`. A path that leads to no
  non-blank string stores no row (ADR-0007's miss). Every item is stored whatever its liveness;
  readers join the live items and match `adapter_kind` to the source's configured adapter, so
  there is no trigger on `knobas.source_config`. A delete cascades with the item, so removing a
  source removes its rows.

  **The readers that moved onto it**, each keeping its answer: the Builds list
  (`knobas_core::builds::PIPELINES`), one pipeline's runs (`PIPELINE_RUNS_OF`), *This job*
  (`JOB_PIPELINES`) and a link-set member's state (`knobas_app::link_sets::MEMBER_ROWS`). The
  pipeline, the job and the link-set commands write the stored declarations first
  (`commands::search::declare`), as the census and the Docs reads do. The alert reconcile's roster
  (`knobas_sync::alerts::ROSTER`) reads each monitor's newest sample by a one-row lookup; it
  touches no schema.

  **Backups.** A knobas archive holds no `sync` schema (§16.12), so it never carries the table; a
  restore's re-sync fills it through the triggers. `RENAMED_COLUMNS` needs no row.

  Pinned by `knobas-app`'s `tests/it/run_pipeline.rs`:
  `a_pipelines_runs_are_the_walk_after_every_change` (the page, the Builds row and the member
  state held to the walk before #2185 after an insert, a payload update, a time that moves alone,
  a tombstone, a re-sync, an adapter-kind change, a declaration change and a delete, with two
  sources sharing a key and two runs whose pipeline misses) and
  `the_stored_resolver_is_the_macros_expansion`; by `tests/it/read_plans.rs` (the five
  statements' plans, analyzed and not); and by `knobas-sync`'s
  `alerts::the_newest_sample_decides_over_a_long_history`.
