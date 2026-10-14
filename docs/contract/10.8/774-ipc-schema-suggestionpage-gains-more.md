- **IPC schema, issue #774 (2026-10-14): `SuggestionPage` gains `more`.** Ruled by the deputy for
  Björn on #774: the tray shows at most three rows per item, and an item with more gets one line,
  "+N more similar to <title>", that opens it. An added field was the smallest change that could
  carry the counts; no new command, no new error kind (a retired proposal already answers
  `not_found`, which the shell now treats as gone).

  **What changes.** `room_suggestions` answers `SuggestionPage { rows, total, more }`.
  `more` is `Vec<SuggestionMore>`, each `{ item: LinkEnd, more: i64 }`: an item **among the drawn
  rows** that holds more proposals than the three the tray draws for it, and how many beyond them,
  most hidden first. `rows` is bounded in SQL by a window function over each item's proposals
  counted from either end (a row is drawn only if it is within the newest three of both its ends),
  ranked over the room's newest slice and widened until the tray is full or the room exhausted
  (a row's rank counts only newer rows, so a slice ranks exactly), and then cut by `limit` as
  before. `total` still counts every proposal in the room, drawn or not. The TypeScript mirror (`ipc/entity.ts`) gains `more: SuggestionMore[]`. Core gains
  `suggest::proposal_overflow(pool, sources, members, drawn_rows)` and `suggest::SuggestionMore`; `suggest::proposals` keeps its
  signature and now returns the bounded set.

  **The shell.** The tray draws one `.row.more` line per `more` entry, which opens the item. A
  not-found answer to *Accept* or *Dismiss* removes the row quietly: no toast, one re-read.

  Pinned by `knobas-core`'s `tests/it/suggestions.rs`
  (`a_hub_holds_three_rows_in_the_tray_and_the_rest_are_counted`), by `entity_mirror.rs`'s
  `SuggestionPage` shape assertion, and by `SuggestionTray.test.svelte.ts`.
