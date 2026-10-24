- **`knobas_core::payload` (four `KindPaths` fields), `crates/knobas-source/src/**` (the
  ticket-system minimum's saved view, `KindMinimum::optional_paths`, battery clauses 6 and the
  frame check) and the `sources.ts` mirror — issue #2050 (2026-10-25): a saved view's query,
  board type, sub-query and sprint are declared paths, and the projection reads no key named
  after Jira.** No migration, no new command, no `commands/` or `ipc/` module, no `WriteOp`, no
  WIT version, no IPC type changed. Parent spec #1192, stories 2 and 7 (ADR-0029). Follows the
  #1207 entry, whose saved view had no required paths. Ruled by the implementer on Opus (`## #2050`
  in `docs/decisions/2026-10-unattended-rulings.md`).

  **The declarations** (`knobas_core::payload`, re-exported by `knobas_source`). `KindPaths`
  gains `source_query`, `board_type` and `sub_query` (each a string) and `sprint` (a JSON
  object, `{"id", "name", "end_date"}`), all `Vec<PayloadPath>`, `#[serde(default)]`, in
  `field::ALL` as `field::SOURCE_QUERY`, `field::BOARD_TYPE`, `field::SUB_QUERY` and
  `field::SPRINT`. Mirrored in `app/src/lib/ipc/generated/sources.ts` (regenerated). They cross
  the WIT boundary inside the descriptor JSON, so by the #1198 entry a `#[serde(default)]`
  addition stays additive for a component: no `knobas:adapter` version.

  **The minimum** (`knobas_source::role::MINIMUMS`). `KindMinimum` gains `optional_paths:
  &'static [Field]`, the paths a role reads when a kind declares them and does without when it
  does not; empty on every kind but one. The ticket-system minimum's optional saved view (frame
  `saved_view`) now **requires** `source_query` (`Field::SourceQuery`) and lists `board_type`,
  `sub_query` and `sprint` (`Field::BoardType`, `Field::SubQuery`, `Field::Sprint`) as optional
  paths. A ticket system that emits no saved view is still not asked. The role data check (#1199)
  weighs a query, board type and sub-query as strings and a sprint as an object. Every other
  row of the 1193 and 1224 entries is unchanged.

  **The battery.** Clause 10 refuses a ticket-system claim whose saved-view kind declares no
  `source_query`, naming the path (`role "ticket-system" needs kind "…" to declare the path
  source_query`). Beside the Builds frame's checks, a kind declaring frame `saved_view` with no
  `source_query` is refused whether or not the source claims the role, since the projection keys
  on the frame. Clause 6 weighs the three strings as strings and `sprint` as an object
  (`Wanted::Object`). Pinned by `role::tests::a_saved_view_kind_needs_its_query` and
  `contract::tests::rejects_a_saved_view_kind_with_no_query_path`,
  `accepts_a_saved_view_kind_that_declares_its_paths` and `rejects_a_sprint_that_is_no_object`.

  **The built-in.** Jira's `jira_view` declares `source_query` at `jql`, `board_type` at
  `type`, `sub_query` at `sub_query` and `sprint` at `sprint`: the keys `crate::views` has
  written since #877. Its payload is unchanged.

  **The reader.** `knobas_sync::saved_views::project` binds the saved-view kinds' declarations
  (`Declarations`) and reads `sync.saved_view`'s `source_query`, `board_type` and `sub_query`
  through `declared_string!` and `sprint` as the first declared candidate that is an object. It no
  longer reads `jql`, `type`, `sub_query` or `sprint` by name. The frame's own keys (`kind`, `id`,
  `columns`, `keys`, `query`) are read as before. **Rows already mirrored need no re-sync or
  migration:** Jira's declaration names the keys its payloads already carry, so the next run
  projects every existing view to the same row. The one difference is that a query is trimmed and
  a blank one is the empty query, as every declared string is read.
