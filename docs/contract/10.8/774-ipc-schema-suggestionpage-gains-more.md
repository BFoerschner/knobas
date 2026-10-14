- **IPC schema, issue #774 (2026-10-14): `SuggestionPage` gains `more`.** Ruled by the deputy for
  Björn on #774: the tray shows at most three rows per item, and an item with more gets one line,
  "+N more similar to <title>", that opens it. An added field was the smallest change that could
  carry the counts; no new command, no new error kind (a retired proposal already answers
  `not_found`, which the shell now treats as gone).

  **What changes.** `room_suggestions` answers `SuggestionPage { rows, total, more }`.
  `more` is `Vec<SuggestionMore>`, each `{ item: LinkEnd, more: i64 }`: an item **among the drawn
  rows** that holds more than three of the room's proposals, and how many of them the tray did not
  draw, most hidden first. `rows` is bounded by `suggest::proposals`: the room is walked newest
  first, in chunks over `link_proposed_idx`, and a row is drawn while neither of its items holds
  three drawn rows, until `limit` rows are drawn or the room is exhausted (a stored context's room
  reads its members' proposals by the pair indexes instead). `total` still counts every proposal in
  the room, drawn or not. The TypeScript mirror (`ipc/entity.ts`) gains `more: SuggestionMore[]`.
  Core gains `suggest::proposal_overflow(pool, sources, members, drawn_rows)` and
  `suggest::SuggestionMore`; `suggest::proposals` keeps its signature and now returns the bounded
  set.

  **The shell.** The tray draws one `.row.more` line per `more` entry, which opens the item. A
  not-found answer to *Accept* or *Dismiss* removes the row quietly: no toast, one re-read.

  Pinned by `knobas-core`'s `tests/it/suggestions.rs`
  (`a_hub_holds_three_rows_in_the_tray_and_the_rest_are_counted`,
  `rows_a_full_hub_leaves_out_do_not_count_against_the_other_item`), by `entity_mirror.rs`'s
  `SuggestionPage` shape assertion, and by `SuggestionTray.test.svelte.ts`.
