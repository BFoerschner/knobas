- **Migration `0026` — issue #592 (2026-10-11): a route's URL is its body.**

  #592, "What to build": *"Every kind ranks a match in an item's title above a match only in its
  body."* Spec #591 says the same for the corpus (*"the full-text corpus weights the title above
  the body for every kind"*) and in story 25. Every corpus already did except `knobas.route`:
  `0018` put a route's name and its URL both at weight A, so a route named for the word and a
  route that only carried it in its URL tied, and recency decided. **Ratified under #592's "What
  to build" by the deputy's ruling of 2026-10-11 on that issue**
  (`docs/archive/2026-10-unattended-rulings.md`), which chose this migration over leaving `0018`
  as it was. Migration additions have been ratified by the orchestrator under the ticket's own
  criterion since the #42 entry; Björn keeps the gate for frozen contracts and this entry is
  flagged for his review.

  **Migration `0026` — `knobas.route.fts`, and nothing else.** One `alter column fts set
  expression`, the shape `0019` used on `knobas.asset`: name at A, URL at B. No table, column,
  constraint or index is added or dropped; `route_fts_idx` survives the rewrite because the column
  is altered rather than re-created. `0018` is not edited (sqlx checksums applied migrations).
  **B and not C** because a route stores no ancestor path in its `fts` (`0018`: *"No `path_text`
  here"*), so B is the next rung below the name in `0019`'s estate rule, and B is the body's weight
  in `0001` for mirrored items and notes. **What a route matches on does not change**: the
  launcher matches on `fts @@ tsquery` and only ranks by weight, so `0018`'s *"'kuma' and '8111'
  find the routes that carry them"* still holds, and the lexing boundary recorded on
  `knobas_search::corpus::ROUTE` is untouched. No IPC command, DTO, descriptor or stored value
  changes; a generated column is recomputed from `name` and `url`.

  **Witness:** `knobas-search`'s `ranking::a_title_match_ranks_above_a_body_match_in_every_kind`,
  through the `Searcher` against a real PostgreSQL. A route named for the token ranks above a
  newer route whose URL alone carries it, beside the same pair for ticket, commit, note and
  asset. With `0026`'s URL weight set back to `A` the test fails on the route group.
  `corpus_seam.rs`' route tests, which find a route by its port, pass unchanged.

