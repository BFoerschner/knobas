- **Migration `0033` — issue #771 (2026-10-14): `similar_text` proposes at most each item's ten
  most similar counterparts.** Ruled by the deputy for Björn on #771 on 2026-10-14 (the cap, its
  symmetry and the retirement in the issue body; "add the §10.8 file, don't stop to ask" in the
  ruling comment). It edits no earlier migration.

  **Migration `0033`: one table, one index, two columns, one index setting, one truncate.**
  - `sync.similar_text_top (entity_id, other_id, shared, other_at, other_digest)`, primary key
    `(entity_id, other_id)`, `entity_id` referencing `sync.similar_text_seen` `on delete cascade`.
    One row per place in an item's ranking: `other_id` is one of `entity_id`'s ten most similar
    counterparts among the items compared with it, `shared` the distinct shared lexemes (at least
    the floor of five), `other_at` the counterpart's `coalesce(item_updated_at, synced_at)` when the
    pair was judged (the tie break), `other_digest` the counterpart's `0031` digest when the pair
    was judged. `other_id` references nothing, so a counterpart deleted outright leaves its row and
    the ranking it held a place in is found and redone.
  - `similar_text_top_other_idx` on `other_id`: the rankings a changed item holds a place in.
  - `sync.similar_text_seen.lexemes` (`tsvector not null`, the item's `strip(fts)`) and `.at`
    (`timestamptz`, its `coalesce(item_updated_at, synced_at)`), written by `MARK_SEEN` as the item
    is settled: what the ranking reads of a settled counterpart without visiting `sync.item`.
  - `0031`'s `similar_text_seen_lex_idx` set to `fastupdate = off`. Its pending list, never merged
    on tables autovacuum had not reached, made every probe of it read the whole list: 32 ms for 138
    rows a batch item on the probe's mirror, rising with every batch settled.
  - `truncate sync.similar_text_seen`, so the first pass after the upgrade compares and ranks every
    prose item again and retires the unanswered proposals the cap no longer keeps. Derived data
    (`0031`): one full pass, nothing lost.
  - In `sync`, not `knobas`, for `0031`'s reason: rebuilt from the mirror, out of backups.

  **`knobas_core::suggest`.** A pair is proposed when either end ranks the other among its
  `SIMILAR_TEXT_CAP` (10, new `pub const`), by distinct shared lexemes, then newest, then id.
  Ranked on text alone: a counterpart already linked or dismissed keeps its place. A batch now runs
  `PRUNE_SEEN`, `CLEAR_RANKINGS`, `MARK_SEEN`, `RANK_SIMILAR`, the rule's statement (which proposes
  from the rankings) and `RETIRE_SIMILAR` in one `repeatable read` transaction. Before the batches,
  a pass prunes and then unsettles every item whose ranking holds a counterpart that changed or left
  the corpus (`UNSETTLE_RANKINGS`), since the eleventh place is not stored; those items are compared
  again in the same pass. **Retiring** is a delete of the unanswered (`confirmed_at` and
  `deleted_at` null) `similar_text` row, between two items compared with their current text, when
  neither ranks the other: not a tombstone, which the suppression would read as a dismissal.
  Accepted and dismissed rows are never touched. `Rule`, `detect*` and `Pacing` keep their
  signatures.

  **What is not touched.** No command, event or DTO field. `detect_suggestions` still answers the
  number of proposals written.

  Pinned by `knobas-core`'s `tests/it/similar_text_incremental.rs`
  (`incremental_passes_propose_the_whole_corpus_set_capped_at_ten`: at batch sizes 1, 4 and 1,000,
  over arrivals, rewrites, tombstones, a restore, a disabled source and two rounds of accept and
  dismiss, the unanswered proposals are exactly the pre-#745 statement's pairs capped in Rust at
  either end's ten, reason for reason, and the answered rows are unchanged) and
  `tests/it/suggest_plans.rs` (every statement of the pass, the four new ones included).
