- **Migration `0028` — issue #627 (2026-10-12): the parents an item answers to are stored, and
  the database keeps them. Björn signed it off on #627 on 2026-10-12**
  ([comment](https://github.com/BFoerschner/knobas/issues/627#issuecomment-6121166771)): the
  migration as built in PR #630, *"is fine"*.

  `under:` walked every live item through the declarations bound per query, on every keystroke.
  At 100,000 items the unknown-token lookup took 335 ms, and the scoped statement about as much,
  against spec §14's 100 ms. The ticket allows a migration with *"a §10.8 entry and Björn's
  sign-off before it merges"*. **The deputy's two rulings of 2026-10-12 on #627 decide its shape**
  (`docs/archive/2026-10-unattended-rulings.md`). The second ruling moved the source-to-adapter
  match to read time, after a trigger on `knobas.source_config` deadlocked `delete_source`. Neither
  ruling stood in for the sign-off. ADR-0019 records the decision, accepted with the sign-off.

  **Migration `0028`: two tables, two indexes, functions and triggers on those two tables and on
  `sync.item`. No existing object changes.**
  - `sync.parent_declaration (adapter_kind text primary key, payload_paths jsonb not null)` holds
    each compiled-in adapter template's `payload_paths`. `Searcher::declaring` (below) writes the
    table whole.
  - `sync.item_parent (entity_id text not null references sync.item on delete cascade, adapter_kind
    text not null, name text not null, kind text not null)` is indexed on `lower(name)` and on
    `entity_id`. For each item, and for each stored adapter whose declarations reach the item's kind
    (it declares a parent for that kind, or names it as a parent's kind), the table holds the
    `(name, kind)` pairs that the two payload arms of `knobas_search::under::parent_names!` yielded
    for the item before this migration. They keep their original case and are tagged with that
    adapter kind. Every item gets rows, live or not.
  - The functions:
    - `sync.parent_kinds_reached(jsonb)` returns the kinds that one adapter's declarations reach.
    - `sync.declared_adapters()` returns each stored adapter's declarations keyed by kind, cut to the
      `parent` field, together with the kinds they reach.
    - `sync.resolve_item_parents(text[])` replaces the given items' rows. It carries
      `knobas_core::declared_parent_names!` and `declared_names_as_parent!` as text, expanded with
      `$decl` = `dd.decl` and bound per adapter as `{<source_id>: <that adapter's kinds>}`.
    - `sync.resolve_kind_parents(text[])` and `sync.resolve_all_item_parents()` call it.
  - The triggers:
    - On `sync.item`: statement-level after insert and after update, over transition tables. The
      update trigger resolves only the rows whose `source_id`, `kind` or `payload` changed, because
      PostgreSQL refuses a column list beside transition tables.
    - On `sync.parent_declaration`: statement-level after insert, update, delete and truncate. They
      recompute the items of every kind that the changed rows' old or new declarations reach. A
      statement that changed no row returns early. A truncate recomputes everything.
  - **The stored rows depend on the item and on `sync.parent_declaration`, and on nothing else.**
    There is no trigger on `knobas.source_config`, and no `sync.parent_declarations()`.
  - **The lock.** The item triggers take `pg_advisory_xact_lock_shared(627, 0)`. The declaration
    triggers take `pg_advisory_xact_lock(627, 0)`, after the no-row early return. An item write and
    a declaration change that overlap therefore cannot leave a row resolved against superseded
    declarations. **Invariant:** the exclusive key is taken only by statements on
    `sync.parent_declaration`, and never by a trigger on a table that a sync run or a user command
    writes. So the lock cannot deadlock a run against `delete_source`, `add_source` or any other
    source command. **Residual (ADR-0019):** a declaration change waits for every in-flight run to
    commit. That happens only when the stored declarations differ from the binary's.
  - `sync.item`, `sync.live_item`, `0027`'s `fts` expression and every `knobas` table are untouched.
    No earlier migration is edited.

  **Readers.** `under::parent_names!` becomes `sync.item_parent` joined to `sync.live_item` and to
  `knobas.source_config` on `s.id = i.source_id and s.kind = ip.adapter_kind`. The asset arm is
  unchanged. This is `Vocabulary::declaring`'s rule, applied at read time. `KNOWN_SQL` and
  `suggest::PARENTS_SQL` still share the fragment (#615), and neither binds the declarations any
  more. `corpus::LIVE_ITEM.under` becomes
  `i.entity_id in (select … from sync.item_parent ip join sync.item x … join knobas.source_config s
  on s.id = x.source_id and s.kind = ip.adapter_kind where lower(ip.name) = any(ud.v))`. The path
  line still calls `declared_parents!` per result row.

  **Write side.** There is no Rust on the write path: `knobas-sync`'s writer, every adapter and
  every `CURSOR_VERSION` are as they were. `knobas_search::Searcher::declaring(pool, templates)` is
  new and public. It is the one function that writes `sync.parent_declaration` and builds the
  searcher from the same list. It reads the table first and writes only when something differs.
  `knobas_app::commands::search::searcher` calls it with `Registry::builtin().templates()` at
  bring-up, before the first sync, and on every search command. **No IPC command, DTO, event or
  descriptor changes.**

  **Backup.** The `sync` schema is in no archive (§16.12), so `sync.item_parent` is rebuilt and
  never dumped. A restore lands before any mirror row can exist, and the re-sync's inserts resolve
  themselves. `knobas-db`'s `backup.rs` is unchanged.

  **Witness.** All on real PostgreSQL (ADR-0013). Each named mutant was applied to a committed
  baseline and killed the test.
  - `knobas-search`'s `tests/it/item_parent.rs`:
    - `the_stored_parents_are_the_walk_after_every_change` holds what the readers see (stored rows
      joined on `adapter_kind`) equal to the per-row macro walk bound to `Vocabulary::declaring`'s
      declarations. It checks after each of seven steps: an insert, a payload update, a delete, a
      declaration change, a source configured after its items, a source given another adapter kind,
      and a source removed. The fixture has both `at` shapes, an unmirrored parent, a mirrored parent
      and an asset. It dies when the trigger's names-as-parent arm is removed, and when the recompute
      on a declaration update is skipped.
    - `the_stored_function_is_the_macros_expansion` dies when either macro's expansion drifts.
    - `an_item_written_during_a_declaration_change_resolves_against_it` and
      `a_declaration_change_during_an_item_write_reaches_the_item` run the race in both orders. Two
      connections are ordered by `pg_locks`, not by sleeps. The first test dies when the item insert
      trigger's shared lock is removed. Both die when the declaration trigger's exclusive lock is
      removed.
  - `knobas-search`'s `tests/it/declaring.rs`: `declaring_unchanged_declarations_never_waits_on_a_sync`
    has `Searcher::declaring` return within 5 s while another transaction holds the key shared. It
    dies when `Searcher::declaring` always writes and the declaration trigger's no-row early return
    is removed.
  - `knobas-sync`'s `tests/it/delete_during_run.rs`:
    `a_source_deleted_after_its_run_flushed_returns_and_the_run_finishes` and
    `a_source_purged_after_its_run_flushed_new_items_returns_and_the_run_finishes`. Each drives
    `run_once` with a fake `Source` past one `BATCH` flush, which takes the key shared, then pauses
    it and calls `config::delete`. The delete must return while the run is paused, and the run must
    then finish without error. Both tests die on a restored `knobas.source_config` row trigger that
    takes `pg_advisory_xact_lock(627, 0)`, and on `for key share` of the batch's config rows in the
    item insert trigger.
  - `knobas-app`'s `backup::a_restored_backup_answers_under_as_the_machine_it_came_from` covers the
    round trip.
  - The benchmark, `knobas-search/tests/perf.rs`'s
    `under_is_under_the_same_budget_beside_a_hundred_thousand_items`, read p50 619 / p90 623 ms
    before and a worst p90 of 58 ms after.
  - Write cost on the same fixture:

    | Operation | Before `0028` | First version (adapter resolved at write time) | This version (adapter matched at read time) |
    |---|---|---|---|
    | `seed_corpus(100_000)` | 5.1 s | 5.36 s | 5.75 s |
    | Update giving 6,667 commits a repository | 190 ms | 460 ms | 760 ms |

  - `under.rs`, `suggest.rs` (including `under_knows_exactly_the_names_the_box_offers`) and
    `corpus_seam.rs` pass, with their seeds still raw `insert`s.

