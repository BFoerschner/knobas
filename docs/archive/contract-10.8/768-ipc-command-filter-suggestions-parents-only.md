- **IPC command argument, issue #768 (2026-10-14): `filter_suggestions` gains
  `parents_only: Option<bool>`.** Ruled by the deputy for Björn on #768 on 2026-10-14: the
  launcher "reads kinds and sources once per opening (as before #767) and only the parents per
  keystroke ... whichever keeps the IPC change smallest". Amends #761's entry, whose "What the
  keystroke path still pays" this removes.

  **What changes.** The `filter_suggestions` command takes a second optional argument,
  `parents_only` (absent means `false`; over the wire `parentsOnly`). With `true` it answers the
  parents for `under` exactly as before and returns `kinds` and `sources` as empty arrays,
  unread. `filter_suggestions_inner(pool, under, parents_only)` takes it, and `Searcher` gains
  `filter_parent_suggestions(&self, under)`, which does not load the vocabulary. No new command;
  `FilterSuggestions` keeps its shape, so the TypeScript mirror changes only in docs, and
  `ipc/search.ts`'s `filterSuggestions(under = "", parentsOnly = false)` sends both.

  **The shell.** `Session.loadSuggestions` makes the opening's first read in full and keeps its
  `kinds` and `sources`; every later read of the opening asks with `parentsOnly` and draws the
  kept lists with the new parents. A read made while the first is in flight waits for its lists.
  `dispose()` forgets them, so the next opening reads them again.

  **The statements.** `KINDS_SQL` and `SOURCES_SQL` no longer read through `sync.live_item`. On
  tables never analyzed the view's `knobas.entity` join was estimated at 84 rows for its
  `deleted_at is null`, the planner drove from it, and the reads took 90 ms and 437 ms. Liveness is
  now two correlated subqueries in the `where` (the entity not withdrawn, the source not turned
  off), over an ordered probe that stops at the first live row: `item_source_updated_idx` per
  source, and `item_kind_updated_idx` per kind with the distinct kinds found by a recursive skip
  scan. Same answer as before (a withdrawn item or a disabled source vouches for nothing).

  Measured on `just responsiveness-probe` (22,000 items), warm medians: kinds 90 ms never analyzed
  and 8 ms analyzed before; sources 520 ms and 0.5 ms before. After: kinds 0.5 ms and 0.5 ms,
  sources 0.4 ms and 0.4 ms, custom plan and generic alike. The whole opening read (kinds, sources and the
  parents for an empty `under:`) is 28 ms on the probe never analyzed and 27 ms analyzed, from 560 ms
  never analyzed.

  Pinned by `knobas-search`'s `tests/it/suggest.rs`
  (`the_parents_alone_are_the_parents_of_the_full_read_without_its_lists`,
  `a_withdrawn_item_vouches_for_neither_its_kind_nor_its_source`, and the earlier disabled-source
  and live-row tests) and by `Launcher.test.svelte.ts` (`typing several under: values reads the
  kinds and the sources once`, `a second under: value typed while the opening's read is in flight
  reads no kinds either`).
