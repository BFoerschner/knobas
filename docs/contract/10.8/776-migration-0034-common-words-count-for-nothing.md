- **Migration `0034` — issue #776 (2026-10-14): `similar_text` leaves out the words in more than
  5% of the prose items.** Ruled by the deputy for Björn on #776 on 2026-10-14 (the 5% line, the
  200-item floor, stored and incrementally kept frequencies, "a new table or column needs a
  migration plus a §10.8 file. Don't ask."). It edits no earlier migration.

  **Migration `0034`: two tables, two indexes, one column, one truncate.**
  - `sync.similar_text_lexeme (lexeme, items, common)`, primary key `lexeme`. `items` is how many
    counted prose items hold the lexeme; `common` is whether the pass treated it as common when it
    last ranked the items that hold it. Partial index `similar_text_lexeme_common_idx` on `lexeme`
    `where common`: the common words every statement of the pass reads once.
  - `sync.similar_text_counted (entity_id, lexemes)`, primary key `entity_id`, no foreign key: one
    row per counted live prose item and the `strip(fts)` it was counted with, so a pass takes an
    item's old lexemes off the counts when it changes or leaves the corpus. No foreign key so a
    mirror row deleted outright leaves its counted row for the pass to take off.
  - `sync.similar_text_seen.fts` (`tsvector not null`, the item's `fts` when it was settled) and
    GIN index `similar_text_seen_fts_idx` on it, `fastupdate = off` for `0033`'s reason: a batch
    anchor's title query finds the settled items it matches there rather than in `item_fts_idx`
    over the whole mirror.
  - `truncate sync.similar_text_top, sync.similar_text_seen`: the first pass after the upgrade
    compares and ranks every prose item under the new rule and retires the unanswered proposals
    only common words made. Derived data: one full pass, nothing lost.
  - In `sync`, not `knobas`, for `0031`'s reason.

  **`knobas_core::suggest`.** A lexeme is common when more than 5% of the live prose items hold it
  (`items * 20 > n`), once there are `COMMON_FROM_ITEMS` (200, new `pub const`) of them; under that
  nothing is common. A common lexeme is not in a title query, not in a seen row's `title_lex` or
  `lexemes`, and not among the shared lexemes the floor of five and the ranking count, nor in the
  reason. A pass first runs `COUNT_LEFT` and `COUNT_ARRIVED` (exact incremental counts, no
  recount), then, if any count moved, `FLIP_COMMON`, which flips the lexemes that crossed the line
  and deletes the seen rows of the items holding one, all in one transaction. `PRUNE_SEEN` now runs
  once a pass, not in every batch; `RETIRE_SIMILAR` also requires both ends live. The tray read
  (`proposals`, `proposal_overflow`, `proposal_count`) and the answers (`accept`, `dismiss`,
  `resolve_edge`) move to `suggest/tray.rs`, re-exported from `suggest` with unchanged paths and
  signatures.

  **What is not touched.** No command, event or DTO field. `Rule`, `detect*` and `Pacing` keep
  their signatures; the threshold of five and the cap of ten stay.

  Pinned by `knobas-core`'s `tests/it/similar_text_incremental.rs`
  (`common_words_count_for_nothing_as_they_cross_the_line`: at batch sizes 1 and 1,000, over 300
  topical items and then rewrites, arrivals, a tombstone, a disabled source and answers, the
  unanswered proposals are exactly the capped set of an independent whole-corpus statement over
  the uncommon words, with words crossing the line both ways; the existing capped-oracle test now
  also shows a corpus under 200 prose items has no common words) and `tests/it/suggest_plans.rs`
  (the three new statements planned in both statistics states).
