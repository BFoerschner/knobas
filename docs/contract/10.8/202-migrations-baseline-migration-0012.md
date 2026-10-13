- **The migrations baseline, issue #202 (2026-08-31):** migration `0012`
  (`0012_disabled_sources_leave_the_mirror.sql`) redefines the `sync.live_item` view so that a
  source the user has **disabled** is invisible to every reader. `crates/knobas-db/migrations/**`
  is frozen at all; this is the ratified exception, written with the implementing PR per the
  #175/#177 pattern.

  **The ruling.** Björn, 2026-08-31, answering the two questions #200 deliberately left open, and
  asked explicitly about blast radius. Both answered **no**: `@me` must not resolve through a
  disabled source's username (already the behaviour — no code change, the question is now settled
  rather than deferred), and a disabled source's rows must not be searchable *or visible to any
  other reader* — launcher, smart lists, board, room tiles, mini board, inbox, contexts, entity
  detail. "Turned off" means one thing everywhere. The alternative of making disable *tombstone*
  its items was offered and declined: it turns a toggle into a destructive write and collides with
  what Purge means.

  **The statement:**

  ```sql
  create or replace view sync.live_item as
  select i.entity_id, i.source_id, i.kind, i.title, i.body_text, i.author,
         i.item_updated_at, i.synced_at, i.payload, i.web_url, i.fts,
         e.updated_at as entity_updated_at
    from sync.item i
    join knobas.entity e on e.id = i.entity_id
    left join knobas.source_config s on s.id = i.source_id
   where e.deleted_at is null
     and coalesce(s.enabled, true);
  ```

  `create or replace view`, not drop-and-recreate: the column list, its order and its types are
  unchanged, so no dependent object is dropped. `0001` and `0002` are untouched — sqlx checksums
  applied migrations and an edit fails startup on every existing database.

  **`left join` and `coalesce(s.enabled, true)` are load-bearing, and an inner join is the bug this
  entry exists to forbid.** `sync.item.source_id` has **no foreign key** to `knobas.source_config`,
  deliberately — `0002` records the reason for the sibling table: *"run_once syncs unconfigured
  sources (tests, ad-hoc imports)"*. An inner join therefore silently drops every row whose source
  was never configured. Not hypothetical: `crates/knobas-search/tests/search.rs` seeds items for
  `teamcity` and `confluence` while registering only `jira` and `gitea`, and the *pre-existing*
  `live_item_hides_what_a_source_deleted` seeds `source_id = 'test'` with no config row at all —
  both fail under an inner join, which is how the mutation was confirmed. So the filter answers
  "did the user turn this source off" and must not quietly also answer "was this source ever
  configured": **no config row leaves the items visible, exactly as before.**

  **Why the view and not each reader.** The same decision `0002` made for the tombstone filter, in
  its own words: *"a smart-list author who forgets the join ships a launcher that offers rows that
  no longer exist"*. 37 files read this view across five crates; putting the rule in one of them
  and trusting the other 36 to match is the silent-disagreement failure #82 and #141 were about.
  Inheritance is asserted rather than claimed: `the_board_drops_a_source_the_user_turned_off` runs
  it through `home::recent`, which is a different statement from the search and never touches the
  FTS index.

  **What it changed elsewhere.** `coverage::in_scope` reverts to the enabled-only population —
  #200 had widened it on the premise that a disabled source's rows stayed searchable, which this
  ruling overturns, so `a_disabled_sources_rows_are_searchable_so_it_is_still_verdicted` flips to
  `..._leave_the_corpus_so_it_gets_no_verdict`. `SourceVocab::enabled` stays: the two consumers now
  agree, but they are still different questions and the field is what lets one query answer both.
  In `crates/knobas-sync/tests/scheduler_loop.rs`, `retire` and `re_add` held sources off the
  ticker with `enabled = false`, which was free while `enabled` meant only "the scheduler may sync
  this"; `0012` gave it a second meaning, so four purge tests began asserting a visibility they
  never meant to. They now use `backoff_until`, which `config::due` respects and
  `Scheduler::trigger` ignores — the same asymmetry, without touching what a reader sees.

  **What did not change.** No IPC command, DTO field or event name; `SearchResponse` and every
  mirror keep their shape, so no TypeScript moves. Nothing under `crates/knobas-source/src/**`,
  `crates/knobas-http/**` or `crates/knobas-app/src/{error,profile}.rs`. `0013` is the next free
  number.

  **Two readers whose behaviour this changes, recorded rather than discovered later.**
  `knobas_core::write_queue::target_of` reads through the view precisely so a withdrawn target
  reads as `None` and its write is *held*; disabling a source therefore holds every pending write
  against it. That is defensible — the source is off — but the write is held for the wrong stated
  reason, and distinguishing "source disabled" from "target withdrawn" is left open.
  `knobas_app::start_work::queue`'s `BRANCH_BY_NAME` and `PULL_REQUEST_BY_HEAD` stop resolving for
  a disabled source, which is correct and is now the documented consequence.

  **One reader that deliberately does not change, stated so nobody reads it as a miss.** The
  entity detail (`knobas_app::commands::entity` — its `DETAIL` statement and the `include_deleted`
  room reads) reaches past `sync.live_item` on purpose, per §5a: links and notes may point at
  withdrawn entities, which must still open. That precedent carries over unchanged — a disabled
  source's entity still opens by direct address, exactly as a tombstoned one does — but where a
  tombstone has `deleted_at` for the banner to read, "source disabled" leaves no marker in the
  detail row, and a room read with `include_deleted` now also lists a disabled source's rows.
  Distinguishing "source disabled" in the detail is left open with the same status as the
  `target_of` question above.

  Ratified by the orchestrator as issue #202 itself, whose ruling specifies the answer, the blast
  radius and this entry.

