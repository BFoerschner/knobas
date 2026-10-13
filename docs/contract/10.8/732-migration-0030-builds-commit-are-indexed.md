- **Migration `0030` — issue #732 (2026-10-13): the builds of a commit are an indexed lookup, and
  the detail draws them as one line above the tabs.** Spec #701, a follow-up of #710's review
  (PR #729), signed off under the spec's delegation of 2026-10-13; #732 asks for this entry for the
  migration. It amends #710's entry above in two places, named below.

  **Migration `0030`: one table, two indexes, five functions, three restated trigger functions.**
  No earlier migration is edited, and no trigger is added or dropped.
  - `sync.item_revision (entity_id, adapter_kind, revision)`, `entity_id` referencing `sync.item`
    `on delete cascade`, indexed on `revision` (`item_revision_revision_idx`, the reader's) and on
    `entity_id` (the resolver's delete). One row per distinct commit an item declares under each
    stored adapter that declares a `revision` for its kind.
  - `sync.revision_kinds_reached(jsonb)`, `sync.revision_adapters()`,
    `sync.resolve_item_revisions(text[])`, `sync.resolve_kind_revisions(text[])` and
    `sync.resolve_all_item_revisions()`: the counterparts of `0028`'s `parent_kinds_reached`,
    `declared_adapters`, `resolve_item_parents`, `resolve_kind_parents` and
    `resolve_all_item_parents`. The resolver carries `knobas_core::declared_list!("dd.decl",
    "revision")`'s expansion as text, `$decl` bound per adapter as `{<source_id>: <kinds>}` like
    `0028`'s.
  - `sync.item_inserted()`, `sync.item_updated()` and `sync.parent_declaration_changed()` are
    restated (`create or replace`) to resolve both tables over the same ids or kinds; the parent
    half is `0028`'s text. The lock and its invariant are `0028`'s, unchanged: item writes take
    `(627, 0)` shared, a declaration statement that changed a row takes it exclusive.
  - The stored rows depend on the item and on `sync.parent_declaration` and on nothing else
    (ADR-0019): that table already holds each template's whole `payload_paths`, `revision`
    included, so no declaration table is added. The adapter is matched at read time, so there is
    still no trigger on `knobas.source_config`. The migration's last statement fills every
    existing row from the stored declarations and, like `0028`'s, takes no lock.

  **Reader** (amends #710's "the builds statement ... scans the live corpus"). `DETAIL_BUILDS_AT`,
  now `pub` so the perf test can read its plan, selects from `sync.item_revision r` joined to
  `sync.live_item i` and to `knobas.source_config s on s.id = i.source_id and s.kind =
  r.adapter_kind`, `where r.revision = $1`; the path line, state, order and limit are unchanged.
  It is still sent only for an entity that declares a commit. `EntityDetail.builds` keeps its
  shape and meaning. Measured by `builds_of_a_commit_perf.rs` at 50,000 builds on the 2026-10-13
  machine (load average about 23): a bitmap index scan on `item_revision_revision_idx`, then
  primary-key lookups, 0.47–0.51 ms execution, against 437–442 ms for #710's walk on the same
  fixture. Seeding those 50,000 builds took 7–14 s with and without `0030` on that loaded
  machine, so the write-path cost was within the noise.

  **The shell** (replaces #710's "The shell" paragraph). `detail/BuildsPanel.svelte` is removed.
  `detail/BuildsSummary.svelte` draws one line, `Builds · 1 failing · 1 running · 3 passed`, a
  count per state (`err` failing, `warn` warning, `info` running, `ok` passed, `muted` or none
  other), only the failing and running counts toned. Clicking it opens the builds in place as room
  lines, in the read's order, each opening its build. `SourceView` takes an optional `summary`
  snippet and draws it between the toolbar and the tab strip, on every tab. Where there is no
  view (still reading, `null`, or failed), the detail draws the line under the header instead.
  Whether the list is open is kept by the detail per entity, so the view landing does not close
  it. Absent when there are no builds.

  **What is not touched.** No command, event, DTO field, `WriteOp`, `Block` or `ViewRow` field,
  and no TypeScript mirror.

  Pinned by: `knobas-app`'s `tests/it/builds_of_a_commit.rs`
  (`a_commits_builds_are_the_walk_after_every_change`, which holds the detail's builds to #710's
  walk after an insert batch, a payload update, a tombstone, a re-sync in the writer's shape, the
  tombstone lifted, a source's adapter kind changed and back, a declaration change and a delete;
  `the_stored_resolver_is_the_macros_expansion`) and `tests/it/builds_of_a_commit_perf.rs`
  (`a_commits_builds_are_an_index_lookup`, in the gate at 3,000 builds after an `analyze`: the
  plan uses the revision index and scans neither `sync.item` nor `sync.item_revision`; and
  `a_commits_builds_take_under_five_milliseconds_at_fifty_thousand_builds`, `#[ignore]`d like
  `knobas-search`'s `tests/perf.rs` because a wall-clock budget inside a gate measures the
  machine: in a scratch database of 50,000 builds, the same plan, and the fastest of ten
  `explain analyze` runs under 5 ms; `just builds-perf` runs it), with #710's two `entity.rs` tests unchanged; the
  frontend's `BuildsSummary.test.svelte.ts` and `Detail.builds.test.svelte.ts`. Mutants, each on
  a committed baseline, each killed: the update trigger not resolving revisions; a declaration
  update not recomputing them; the resolver without `distinct`; the reader without the adapter
  match; the revision index dropped (the plan test); the summary not passed to the view; the running count untoned; the open state not kept
  by the detail.

