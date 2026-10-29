- **Migration `0029` and what one frozen wire value means — issue #642 (2026-10-12): `under:`
  coverage is measured on the corpus. Björn ruled it and signed off the migration on #642 on
  2026-10-12** ([comment](https://github.com/BFoerschner/knobas/issues/642#issuecomment-6123571497)):
  *"option 1 — measure it on the data. A source answers `under:` only if something it synced
  actually names a parent."* Asked with the cost stated (a migration on the table #627 signed off
  the same day), he chose it, *"so the migration is allowed and this comment is his sign-off on
  it"*. The deputy's reading of option 1
  ([comment](https://github.com/BFoerschner/knobas/issues/642#issuecomment-6123097681)) is the
  ticket's criteria. **Björn then signed off a second column the same day**
  ([comment](https://github.com/BFoerschner/knobas/issues/642#issuecomment-6124188649)):
  `sync.item_parent.source_id`, *"Yes, add source_id"*, asked with the measurement stated (two
  sources on one adapter cost about 150 ms per `under:` keystroke at 100k items without it, about
  0.04 ms with it).

  **What `FilterAnswer` means for `FilterDimension::Under` — this replaces the #595 entry's
  sentence "The answer is read off the declaration, not measured on the corpus".** A listed source
  is `answered` when some live item it synced names a parent through the **child arm**
  (`knobas_core::declared_parent_names!`), under the adapter the source is configured with, over
  **all** of the source's kinds and not the query's kind scope: `under:` returns the parent itself,
  so `kind:repo under:<repo>` shows the repo, and Gitea answers it because its commits name repos.
  It is `no_values` otherwise, which covers a source whose adapter declares no parent, a classic
  Jira with neither `epic` nor `fields.parent` stored (`epic_link_field` never configured), and a
  Jira project where no ticket is in an epic. A row from the names-as-parent arm does not count:
  every Jira ticket answers to its own key. **Who is listed is unchanged**: the scoped probe that
  serves `author:` still decides which sources put rows in this query's corpus. The wire shape is
  unchanged: no variant, field, command or DTO moves. The `CONTEXT.md` **Coverage** entry already
  says "measured on the corpus", so the glossary does not change; the code now agrees with it.
  `knobas_core::payload::Declarations::declares_a_parent`, whose only caller was the old verdict,
  is removed (`knobas-core` is not on the frozen list).

  **Migration `0029`: two columns, one restated function, one partial index.** No earlier
  migration is edited.
  - `sync.item_parent` gains `arm text not null`, checked `in ('child', 'parent')`: `'child'` for a
    row `declared_parent_names!` wrote, `'parent'` for one `declared_names_as_parent!` wrote.
  - `sync.item_parent` gains `source_id text not null`, the item's `source_id` copied. The update
    trigger already resolves again an item whose `source_id` changes, so the copy cannot go stale,
    and the stored rows still depend on the item and on `sync.parent_declaration` and on nothing
    else (ADR-0019).
  - `sync.resolve_item_parents(text[])` is restated (`create or replace`) to write both. The only
    differences from `0028`'s text are the column lists and a literal tag on each arm's select;
    both expansions are pasted unchanged. The migration recomputes every row with
    `sync.resolve_all_item_parents()` before setting `not null` and, like `0028`'s last
    statement, takes no lock.
  - `item_parent_child_idx` on `sync.item_parent (source_id, adapter_kind) where arm = 'child'`
    serves the coverage probe.
  - No trigger, lock, other function or reader of `0028`'s columns changes: `under:`'s filter, its
    unknown-token lookup and the query box's suggestions still read both arms.

  **Reader.** `knobas_search::coverage::UNDER_COVERAGE_SQL`, one statement asked only for a query
  that filtered by `under:`: the sources among the scoped ids with an `exists` over their own
  child-arm rows (`ip.source_id = s.id`) under their configured adapter whose item is live. A
  source with no child rows answers from an empty index range, whatever another source of its
  adapter has. The launcher's `no_values` reason for `under:` becomes "nothing it has synced names
  a parent" (`app/src/lib/launcher/coverage.ts`), a sentence true of a corpus, like the author one.

  **Witness.** All on real PostgreSQL (ADR-0013); each mutant was applied to a committed baseline.
  - `knobas-search`'s `tests/it/under_coverage.rs`:
    `a_source_answers_under_only_if_something_it_synced_names_a_parent` (a configured classic Jira
    answers; an unconfigured one, a Kuma, and the configured one once its epic child is tombstoned
    or deleted get `no_values`) and `a_kind_scope_does_not_narrow_the_verdict` (`kind:repo under:<repo>` over
    Gitea answers, and the same source given an adapter that declares nothing gets `no_values`).
    Killed: the verdict restored to the declaration; the probe counting rows of both arms; the
    probe narrowed to the kind scope; the probe without the adapter match; the probe reading
    `sync.item` in place of `sync.live_item` (added in review).
  - `tests/it/item_parent.rs`'s agreement test now compares `(entity_id, name, kind, arm)` against
    the walk, with each arm's select tagged, and checks every row's `source_id` against its item's,
    after every change it already exercised and a new one: an item moved to another source and
    back. It dies when the restated function tags the child arm `'parent'`, and when it copies the
    adapter kind in place of the source id.
  - The benchmark, `just search-perf` on head `31ea1950` (load1 5.38 before, 6.85 after), with a
    second, empty Gitea source configured so the probe pays the two-sources-on-one-adapter case:
    `under_is_under_the_same_budget_beside_a_hundred_thousand_items` read repo full name p50 65 /
    p90 67 ms, bare name 64 / 65, repo + word 43 / 44, name nothing has 52 / 55: **worst p90
    67 ms**, against 69 ms on `438be884` without this change (and without the second source, so
    not like for like; the probe's own reading is the one that bears on it). `EXPLAIN ANALYZE` of
    the probe for the empty source: 154 ms with the arm alone (13,334 child rows looked up through `sync.item`),
    0.037 ms with `source_id`. The same run's `search_is_under_a_hundred_milliseconds…` failed on
    the saved-list rail at its cap (`launcher_board` 106 ms p90), as it did on `438be884` the same
    afternoon (`smart_lists` 138 ms p90); neither statement runs the coverage probe.

