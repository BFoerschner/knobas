- **The IPC schema and `mini_board` — issue #810 (2026-10-15): a `query` argument, and five
  fields on `MiniBoardCard`.** The ticket-first Tickets view (R-2.7, R-3.4, R-3.7, R-3.9) draws
  every synced ticket over the mini board's read and filters it on the server. No new command, no
  migration, no grammar key, no declaration field, no `WriteOp`, no `commands/` or `ipc/` module.

  **The command.** `mini_board` gains `query: Option<String>` (absent is `None`, so every existing
  caller is unchanged): search grammar text, the view's base query and the filter bar's words,
  chips and dropdowns. A non-blank one narrows the board to the tickets it admits; blank or absent
  narrows nothing. It intersects with `ctx_id`, `sources` and `project`. Every column's `total`
  counts only the admitted tickets. A query past the launcher's 512 characters is `invalid`.
  `commands::entity::mini_board_inner` is the pool-level function, public for command-level tests.

  **The match** (`knobas_search::Searcher::matching_ids(raw, kind) -> Option<Vec<String>>`, new).
  The launcher's own parse and merge with no chips, over the live mirror only, every id of `kind`
  the query admits, uncapped, unranked. `None` for a query with neither words nor a filter. `Some`
  of none when `type:` names only other kinds. The statement is `knobas_search::sql::ids_sql`, a
  third builder beside `search_sql` and `saved_summary_sql`. **The keys** match through the same
  `corpus_predicates`, so a key means the same in the view and the launcher. **The words do not**:
  each word (lowercased, split on whitespace) must appear as a substring, ignoring case, in the
  row's key (the entity id past its namespace, so `jira` is not a word of every Jira ticket), its
  title, its declared assignee (`declared_string!` over the bound
  declarations), or a name of one of its declared parents (`sync.item_parent`, a Jira ticket's
  epic). R-2.3 and prototype-6's "Filter by key, summary, epic or person"; the body is not
  searched, no word is stemmed or dropped as a stopword, and the view marks exactly what matched.
  `search_sql`'s statement text is unchanged.

  **The board** (`knobas_core::mini_board`). `read_scoped(pool, Scope, declarations, per_column)`
  is new. `Scope { ctx_id, sources, project, only, identity }` adds `only: Option<&[String]>` (the
  ids a search matched; `Some(&[])` draws nothing) and `identity: &[String]` (the configured
  usernames) to the room's three dimensions. `read` and `read_capped` keep their signatures and
  pass no `only` and no identity.

  **The card.** `MiniBoardCard` gains five fields, each `null` on a miss (ADR-0007: a miss draws
  nothing, never a guess), read only for the cards a column keeps:
  - `issue_type: Option<String>`, the declared `issue_type` path (#811's field);
  - `assignee: Option<String>`, the declared `assignee` path, in the source's account spelling;
  - `updated_at: Option<DateTime<Utc>>`, `sync.item.item_updated_at`;
  - `path: Option<String>`, `knobas_core::path_line_read!`, the line every row draws (a Jira
    ticket's epic key);
  - `mine: bool`, `assignee` equal to one of the configured usernames of the enabled sources,
    ignoring case. The `mini_board` command always loads the identity, so the rooms' Tickets tile
    gets a true answer too and simply does not draw it.
  Mirrored in `app/src/lib/ipc/entity.ts`, held by `entity_mirror.rs`'s
  `the_mini_board_shapes_match_their_typescript_mirror`. The fake IPC answers `query` and the new
  fields over its fixture.
