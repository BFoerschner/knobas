- **The search barrel's types are generated: `knobas-search`, `crates/knobas-source/src/query.rs`
  and the IPC surface — issue #1329 (2026-10-20).** Parent spec
  #1306, the switching ticket for `search.ts` after #1325's app barrel and #1326's entity barrel.
  No migration, no event, no `WriteOp`, no command added, removed or renamed, no argument changed,
  no field added, removed or renamed, no wire shape changed, nothing added to either append-only
  barrel (`app/src/lib/ipc/index.ts` and `lib.rs`'s handler list are untouched). Ratified by the
  implementer on Opus under Björn's 2026-10-18 reserved-steps rule and checked against the merge
  head by the merge-manager on Opus; the rulings are in
  `docs/archive/2026-10-unattended-rulings.md` under `## #1329`.

  **Crates that gain a derive.** Three: `knobas-search`, `knobas-source` (only `query.rs`) and
  `knobas-app`, listed type by type below. None gains a dependency: all three already had
  `ts-rs`. `knobas-sync` and `knobas-core` are unchanged. `knobas-source-jira` changes in one
  test only (below). `knobas-source` still builds for `wasm32-wasip2` (the gate's
  `wasm-fixture` step).

  **`knobas-search`** (the frozen search IPC types, interfaces §2.4). `types::{SearchQuery,
  SearchFilters, ParsedQuery, Prefix, SearchResponse, FilterCoverage, FilterDimension,
  SourceAnswer, FilterAnswer, ResultGroup, EntityRow, SearchHit, FilterSuggestions,
  FieldSuggestion, KindSuggestion, SourceSuggestion, ParentSuggestion}` and
  `lists::SmartListSummary` derive `ts_rs::TS` into `search.ts` (`Segment` already did, since
  #1326). `EntityRow` is reached only through `SearchHit`'s `#[serde(flatten)]`, so its fields are
  inlined there and no `EntityRow` of the search crate is exported. Derives, doc comments and the
  deletion of `types`' two mirror tests only: no statement, bind or query changed, so `just search-perf` has nothing to measure. The two
  `FilterDimension`/`FilterAnswer` variant docs moved into the enums' docs (#1325's convention).

  **`crates/knobas-source/src/query.rs`.** `Key`, `Condition`, `LeftOut` and `Translation` derive
  `ts_rs::TS`, exported as `QueryKey`, `QueryCondition`, `LeftOut` and `QueryTranslation`, the
  names the hand-written mirror gave them; `Key`'s variant docs moved into its doc. No item,
  field, variant, serde attribute or behaviour changed.

  **`knobas-sync`.** Unchanged: `LauncherHome` reaches `health::CredentialHealth`, which #1327
  exported to `sources.ts`, so `generated/search.ts` imports it from there.

  **`knobas-app`.** `commands::search::LauncherHome` and `ticket_views::{ViewColumn, ListSort,
  TicketView, TicketViewDraft, TicketViewRow, SavedViewSource, Unsynced}` derive `ts_rs::TS`
  (`Unsynced` exported as `UnsyncedTickets`). `LauncherHome::recent` is
  `#[ts(as = "Vec<knobas_core::entity_list::EntityRow>")]`: the shell has one `EntityRow` for
  the room's and the search crate's identical wire shapes, as the hand-written mirror had, and
  `the_two_entity_rows_are_one_wire_shape` keeps the two structs agreeing.

  **New Rust items, TypeScript-only.** The ticket views' display words are `String` on the wire,
  checked by the save against `LAYOUTS`, `LANES`, `CARD_FIELDS`, `LIST_COLUMNS`, `SORT_COLUMNS`
  and `SORT_DIRECTIONS`, and the shell typed them as unions. `ticket_views::{Layout, Lanes,
  CardField, ListColumn, SortColumn, SortDirection}` are enums, never constructed, used through
  `#[ts(as = …)]` (`Lanes` exported as `ViewLanes`, `SortColumn` as `ViewSortColumn`; `Layout`
  and `SortDirection` inlined), and `the_vocabulary_unions_are_the_lists` pins each to its list.
  `SavedViewKind` (inlined, `board` or `filter`, the projection `knobas_sync::saved_views` writes)
  and `ActiveSprint` (inlined, the shape of `SavedViewSource::sprint`'s JSON, which stays a
  `serde_json::Value`) type the two other fields the shell typed more narrowly than the Rust.
  Both are the types the hand-written mirror had. `ref_kind`'s check constraint
  (`jira_view_ref_kind_chk`, migration 0044) holds the kind to `board` or `filter`. The sprint
  object is what the Jira adapter writes (`views.rs`, `{"id", "name", "end_date"}`), and
  `sync.saved_view` passes any object through unchecked, so the adapter's test
  `each_board_arrives_as_a_view_with_jiras_columns_limits_sprint_and_keys_in_order` now asserts
  the whole object rather than its `name`: a renamed or added key fails there.

  **The IPC surface.** `app/src/lib/ipc/search.ts` keeps every `invoke` function and
  `noFilters`, and re-exports its 34 types from `./generated/search` instead of declaring them,
  so no caller changes. The old and the new barrel are type-identical name for name (the PR's
  witness). The fixture tests go with the hand-written types: `knobas_search::types`' two mirror
  tests, `commands::search`'s two (`LauncherHome`, `FilterSuggestions`), and the type half of
  `ticket_views::the_views_and_their_mirror_agree`, whose command-name half stays as
  `the_view_commands_are_invoked_by_their_names`.

  **Module layout.** `app/src/lib/ipc/generated/search.ts`, booked by #1325's entry.
