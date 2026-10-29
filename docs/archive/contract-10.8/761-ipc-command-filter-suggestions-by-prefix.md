- **IPC command semantics, issue #761 (2026-10-14): `filter_suggestions` takes the `under:` value
  being typed, and `FilterSuggestions.parents` is the newest twenty parents with that prefix.**
  Ruled by the deputy for Björn on #761 on 2026-10-14: "the prefix answer, not the cap", with
  "§10.8 file for the changed `FilterSuggestions` meaning". Amends #601's entry.

  **What changes.** `filter_suggestions` gains an optional argument `under: Option<String>`
  (absent and `""` mean no prefix). `filter_suggestions_inner(pool, under: &str)` and
  `Searcher::filter_suggestions(&self, under: &str)` take it too, and
  `knobas_search::PARENT_SUGGESTIONS` (20) is exported. `parents` used to be every name a parent
  answers to, ordered by name. It is now the newest twenty whose name starts with `under`,
  ignoring case, newest first. A name is ranked by the newest mirrored item that names it (or the
  asset's `updated_at`), live or not, and offered only while `under::parent_names!`, the fragment
  `under:` knows names from, has the same name and kind. `kinds` and `sources` keep their meaning.
  The DTO keeps its shape, so the TypeScript mirror `FilterSuggestions` changes only in its doc;
  `ipc/search.ts`'s `filterSuggestions(under = "")` sends `{ under }`.

  **The shell.** The launcher's session asks once per distinct `under:` value typed (and with `""`
  for a `kind:` or `source:` token). The newest request wins: an older answer that lands late is
  dropped. A failed read is not retried until the next opening. `suggest()` still narrows the
  last answer client-side while the next one is in flight.

  **The statement** is unnamed (`.persistent(false)`). A named one switched to a generic plan
  after its fifth execution, and that plan cannot see how narrow the prefix is (`under:j` went
  from 50 ms cold to 313 ms warm). It ranges over `item_parent_name_prefix_idx` and ranks by
  `sync.item_parent.at` (both migration `0032`), so the parents read alone measured 27 ms for
  `""`, 20 ms for `j`, 3 ms for `jira0-1` and 1 ms for `pay` on tables never analyzed and
  analyzed alike, custom or generic plan.

  **What the keystroke path still pays.** Each call also re-reads the kinds and the sources, as
  it did once per opening before. Those two statements are not this entry's and depend on
  statistics: about 11 ms analyzed, about 520 ms on tables never analyzed (`SOURCES_SQL` 437 ms,
  `KINDS_SQL` 90 ms), a state autovacuum ends within a minute of a first sync.

  Measured with `just responsiveness-probe` (22,000 items): before, one read of 314 ms warm
  median and 680 KB (load 8.2 to 17.3). After (load 2.9): `under:` 44 ms, `under:j` 39 ms,
  `under:jira0-1` 31 ms, `under:pay` 12 ms, about 1 KB each. Those times include the kinds and
  sources reads.

  Pinned by `knobas-search`'s `tests/it/suggest.rs`
  (`a_parent_is_offered_by_the_prefix_typed_newest_first`: the newest twenty of twenty-five,
  newest first, any case typed; a longer prefix reaches older ones; a name that only contains
  the typed text is not offered; `the_parents_statement_is_never_a_named_prepared_statement`; the
  #601/#615 tests, now asking per prefix), and by `Launcher.test.svelte.ts` (`under: asks the
  engine for the value being typed and draws its answer`, `an older under: answer that lands late
  does not replace a newer one`). Mutants killed: ranking by name instead of newest; a
  case-sensitive prefix; no liveness probe; a named statement; the session's late-answer guard
  removed. Changing only the inner sort is result-equivalent: the outer `order by` decides, and
  the inner one is what lets the outer limit stop early.
