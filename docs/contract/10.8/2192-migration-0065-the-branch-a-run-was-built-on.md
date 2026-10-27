- **Migration `0065` — issue #2192 (2026-10-27): a run's declared `branch` is stored beside its
  pipeline in `sync.item_pipeline`, and *This job*'s branch reason reads it by key.** No command,
  no IPC schema change, no event, no DTO field, no `WriteOp`, nothing in
  `crates/knobas-source/src/**`, `crates/knobas-http/**`, `error.rs` or `profile.rs`, no new module
  in either barrel or in `commands/`. Every command keeps its arguments and its answer. Ruled by
  the implementer on Opus; the rulings are in `docs/decisions/2026-10-unattended-rulings.md` under
  `## #2192`.

  **The migration** (`0065_the_branch_a_run_was_built_on.sql`). No earlier migration edited.
  Transactional. One new column, `sync.item_pipeline.branch text` (nullable), and one new index,
  `item_pipeline_branch_idx (branch, source_id, pkey, adapter_kind) where branch is not null`. Two
  functions restated, each with one addition and otherwise `0062`'s text:
  `sync.pipeline_adapters` keeps each run kind's `branch` candidates beside its `pipeline` ones,
  and `sync.resolve_item_pipelines` stores `branch` in the row it already writes. No trigger
  function is restated: the item and declaration triggers already call the resolver whenever a
  run's row can change (`0062`, `0063`, `0064`), so no function that takes the lock (`627, 0`) is
  restated. The last
  statement refills the table from the stored declarations, taking no lock, as `0062`'s did.

  **What the column is.** `knobas_core::declared_string!(…, "branch")` with `$decl` bound as
  `0062` binds it, pasted as its expansion, stored as the run spells it (`refs/heads/x` stays
  so); null where the path leads to no non-blank string (ADR-0007's miss). Only a run with a
  pipeline has a row, and only such a run can put a pipeline on a job.

  **The reader that moved onto it**, keeping its answer: *This job* (`JOB_PIPELINES`, CTE
  `on_branches`) looks up each branch name the job reaches, as itself and as `refs/heads/<name>`,
  in `item_pipeline_branch_idx`, joined to the live runs of configured sources. The
  `branch_paths` CTE is gone, and the statement's `frames` CTE is now written `as materialized`
  (`frame_kinds_cte!(materialized)`), which keeps the plan its pipelines had while `branch_paths`
  was its second reader.

  **Backups.** A knobas archive holds no `sync` schema (§16.12), so it never carries the column; a
  restore's re-sync fills it through the triggers. `RENAMED_COLUMNS` needs no row.

  Pinned by `knobas-app`'s `tests/it/run_pipeline.rs`
  (`the_stored_resolver_is_the_macros_expansion`, now for `pipeline` and `branch`), by
  `tests/it/job_pipelines_ipc.rs` (`a_pipeline_is_on_a_job_for_its_branch_its_pr_check_or_a_link`:
  a run moved onto and off the job's branch by a later write, and a tombstoned run on it), and by
  `tests/it/read_plans.rs` (*This job* with and without a reached branch, analyzed and not: no
  whole read of `item_pipeline`, no per-row filter on `item`, and its pipelines, `CTE p`, not
  driven from `knobas.entity`).
