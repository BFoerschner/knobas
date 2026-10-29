- **Migration `0027` — issue #612 (2026-10-12): a `/` in an item's title also separates words.**

  #591 opens with *"When I search ⌘K for a Gitea repo, I can't find the repo"*, and story 1 asks
  that typing a repo's name shows that repo at the top of its group. A Gitea repo's title is its
  `full_name` (`knobas-source-gitea`'s `repo_item`), and PostgreSQL's parser reads
  `tidewater/payout-service` as one file-path lexeme, so `payout-service`, `payout` and
  `Payout-Service` matched nothing of the repo. **Ratified under #612's "What to build" by the
  deputy's ruling of 2026-10-12 on that issue** (`docs/archive/2026-10-unattended-rulings.md`),
  which chose this migration over adding the bare name to the repo's body or rewriting its stored
  title. Migration additions have been ratified by the orchestrator under the ticket's own
  criterion since the #42 entry; Björn keeps the gate for frozen contracts and this entry is
  flagged for his review.

  **Migration `0027` — `sync.item.fts`, and nothing else.** One `alter column fts set
  expression`, the shape `0019` and `0026` used. The title stays at A, lexed as `0001` lexes it, so
  the whole `tidewater/payout-service` lexeme is kept. Only when the title contains a `/`, the
  same title with each `/` translated to a space is added, also at A. The body stays at B, lexed
  as before. No table, column, constraint or index is added or dropped; `item_fts_idx` survives
  the rewrite because the column is altered rather than re-created, and `sync.live_item` (`0012`,
  which selects `i.fts`) is untouched. `0001` is not edited (sqlx checksums applied migrations).
  **Additive and title-only:** a title without a `/` gives the same `tsvector` byte for byte, and
  every body keeps the lexing `knobas_search::corpus::ROUTE`'s table records. **No source
  knowledge:** the rule names no source, so a branch `feature/payout-jitter` is found by `jitter`
  too, and a slashed ticket, page or PR title gains stems `knobas_core::suggest`'s `SIMILAR_TEXT`
  can share. **No adapter change:** `repo_item`, its golden tests and `CURSOR_VERSION` are as they
  were, and no stored title changes. A generated column is recomputed by the migration's rewrite,
  so an existing install finds its already-mirrored repos by bare name as soon as `0027` runs,
  with no re-emit. The rewrite was measured on the embedded 18.6 at 0.76 s for 10,000 rows and
  6.9 s for 100,000 (the migration header has the table). No IPC command, DTO, descriptor or
  stored value changes. **Composition:** a later migration that changes `sync.item.fts` (#611's,
  if it takes one) restates this expression whole and never drops the slashed-title term.

  **Witness:** `knobas-search`'s `tests/it/bare_name.rs`, through the `Searcher` against a real
  PostgreSQL, with each repo stored as `repo_item` stores it:
  `a_repo_is_found_by_its_bare_name_in_any_case` (`payout-service`, `payout`, `Payout-Service`
  and the full name), `the_repo_named_for_the_query_outranks_a_newer_one_that_mentions_it` (dies
  with the slashed term weighted B), and `the_bare_name_finds_the_repo_and_not_its_branch_or_commits`.
  `knobas-db`'s `tests/schema.rs`: `zero_twenty_seven_changes_only_a_title_that_holds_a_slash`
  (against `0001`'s expression; dies when the term is added to every title) and
  `zero_twenty_seven_reaches_a_row_mirrored_before_it` (a database wound back to `0001`'s
  expression, a row written there, then the runner). `ranking.rs`, `under.rs`,
  `corpus_seam.rs` and `knobas-core`'s suggestion tests (the readers of `SIMILAR_TEXT`) pass
  unchanged.

