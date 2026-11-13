# 2026-11-13: A broad key answers in a frame or two

**Asked (Björn, on PR #2479):** the app should be instant on everything, on his Windows PC and on slow work laptops. A
filter that keeps most of the board (`-project:PAY`, a single letter typed first) took 90 to 140 ms of backend a key
on v3.2.9-ux9 at 1x, one letter under Everything about 106 ms, and `filter_suggestions` 32 ms on Tickets' first open
against 15. The lines: every key under 100 ms at 1x and 200 ms at 4x to its rows on screen, nothing moving after the
first draw, `filter_suggestions` under 15 ms.

**Where a broad key's time went.** Measured with real input in headless Chrome over the saved scale database (20,007
tickets, 23,176 cards of every source) and a release build of the windowless backend, each key one the session had
never asked (`testenv/perf/keys.mjs`), and each statement split with `EXPLAIN (ANALYZE, BUFFERS)` on the server.

- **Waiting for keys that were not coming: 90 ms.** The filter and the launcher waited 90 ms after every key, so the
  first letter of a filter was asked 90 ms after it was typed.
- **The backend: 100 to 160 ms**, two statements in series, each over every ticket:
  - the search's ids, 20 ms for a letter, 30 for `-priority:`, 55 for `-project:`, whose four stored-field tests each
    hashed a record holding a jsonb per row, about 18 ms each;
  - the 17,000 ids out and back in as a parameter, about 10 ms;
  - the board itself, 55 to 72 ms whatever the filter: building the ticket set 22 ms, sorting all of it to cut each
    column to fifty 23 ms (each row carrying its title), grouping it by a lane an unlaned board does not have and
    joining that back 10 ms, the 300 kept cards' fields 9 ms.
- **The page: 15 ms at 1x, 45 to 60 ms at 4x.** The first letter takes each column from the eight cards it drew to
  fifty: 300 new cards in one frame, where the screen shows five to eight a column. The answer is 119 KB; reading and
  parsing it is a few ms at 4x. The matching was never the larger part.

**Taken.**

- **The first key reads at once.** Tickets' filter and the launcher's box (and the link pickers built on it) ask a key
  at once when no read is out and the last key and read are older than the pause. The rest of a word typed at speed
  still waits for the keys to pause, a key typed while a read is out still waits for it, and an answer to older text is
  still never drawn. Every case lost the 90 ms.
- **The stored keys are asked once a source and kind** (`stored_field_in!`, `fsu`). `project:`, `status:`, `assignee:`
  and `assigned:` asked each row whether its candidates are stored, and then whether `(id, candidates)` is in the
  stored set: two hashes of a record holding a jsonb, per row and per key. `fsu` holds each declared source and kind
  whose field is stored, with its candidates, so a row is tested by its source and kind. `-project:`'s statement 51 ms
  to 26.
- **The board sorts what it ranks by, and a board without lanes groups none** (`cards!`, `CARDS_UNLANED`). The title
  and the date sent are read for the kept cards alone, and without lanes the one lane is written out. A letter's board
  statement 72 ms to 50, `-priority:Low`'s 55 to 44. Every board read gains it, filtered or not: the eight-a-column board
  Tickets opens on, 72 ms to 49.
- **A column that grows draws the screen's cards first.** The frame an answer lands in draws a growing column's first
  sixteen cards and the next frame the rest, below them. A column that already drew sixteen draws whole at once, so no
  answer takes a drawn card away from under the pointer.

The backend alone, the head before (`1f84766e`) against this one, each read alternated between two release backends
on clones of the saved database, median of seven, the answers byte-equal:

| Read | Before | After |
|---|---|---|
| board, opening (eight a column) | 72 ms | 49 ms |
| board, `a` / `e` / `o` | 110 / 109 / 101 | 82 / 82 / 81 |
| board, `-project:PAY` / `-project:SCLWEB` | 146 / 127 | 89 / 84 |
| board, `-status:Done` / `-assignee:@me` | 95 / 121 | 69 / 82 |
| board, `-priority:High` / `-priority:Medium` | 105 / 85 | 87 / 74 |
| search, `p` under Everything | 105 | 105 |
| `filter_suggestions` | 33 | 34 |

From the key to its rows on screen, real input in headless Chrome, a key never asked before each press, median of
seven (five for `-priority:`; the launcher's letters none of the English stopwords, which answer in 2 ms); no row
drawn on screen moved in any press:

| Key | 1x before | 1x after | 4x before | 4x after |
|---|---|---|---|---|
| Tickets, the first letter | 234 ms | 103 ms | 277 ms | 115 ms |
| Tickets, `-project:` | 257 | 119 | 285 | 132 |
| Tickets, `-priority:` | 219 | 117 | 279 | 156 |
| Everything, one letter | 225 | 132 | 228 | 134 |

**Same answers.** Each changed read is held to the read it replaces on data that would catch a difference:

- `ticket_keys.rs` holds every key asked as a set to its row-by-row read, under the declarations the database stored
  and, new, under ones it never stored, where each stored key walks the payload. A mutant whose `fsu` called every
  source and kind stored fails it.
- `item_card.rs` holds the board without lanes to the board in lanes asked for none, every row and column in the same
  order on one snapshot: every card stored, half, none; cut to one, three and none; narrowed and not; dates that tie;
  a withdrawn ticket. A mutant that cut by id alone fails it.
- `TicketsPage.test.svelte.ts` holds a growing column to sixteen cards in its first frame and fifty in the next, every
  card of the first still drawn.
- On the scale clone every board answer of the before-and-after table is byte-equal.

**Ruled along the way.**

- **The ids stay a second statement.** The search's ids read as a CTE inside the board statement, so 17,000 ids never
  leave the database, cost the two statements' sum or more in every shape tried (a subquery planned the board from the
  wrong side; a materialized CTE added 5 ms), where the round trip it saves is about 10 ms.
- **The launcher's match keeps `sync.live_item`.** Taking the live rows by the withdrawn entities hashed once, as the
  board does, gives the same rows but loses the statement's parallel plan: 83 ms to 125.
- **Two smaller rewrites were measured and left:** the board's column list built from a hashed distinct (no gain), and
  `-priority:`'s two card sets read off `sync.item_card` without the materialized `fcards` (2 ms of 27).

**Not stored, and why.** Nothing. The brief offered migration `0084`; no stored copy removes a wait a person feels
here:

- **The board's cut.** A stored date on the card and an index in column order could serve each column's newest cards
  without the sort, about 15 ms of a broad key. It needs the cards of every source in one ordered read, which several
  sources' cards under several keys are not, and a walk beside it for the tickets without a card, which puts the sort
  back. Not taken.
- **`filter_suggestions`, 33 ms against 15.** What is left reads each source and kind's ids to find its newest 200
  (9 ms; the planner sorts rather than reading `item_source_updated_idx` in order, and its ties are taken in no stated
  order, so another order is another answer) and probes the stored values (one read a row instead of four: no gain).
  Tickets' first draw waits for it (`framed`, the still-rows pass), but it now waits for the board's own read longer,
  49 ms, so nothing a person sees waits on the suggestions; the answer cache keeps them after the first open.

**Left, with what it would take.**

- **Every broad key is 3 to 32 ms over 100 at 1x**, all of it the backend's read: a letter's board 82 ms (the search's
  ids 21, the round trip of the ids 10, the board 45, the rest the vocabulary, the signals and the answer). Under 100
  would take the board's cut off the sort (above), or one statement that filters as it scans, which the search's
  grammar and the board's statement do not share today.
- **One letter under Everything, 105 ms in the backend, 132 to its rows at 1x.** `p` matches 28,955 rows and ranks
  each, every one probing `sync.item` and `knobas.entity` by key across three workers. Storing the kind and the date
  beside the search vector would take the first probe off, about a third; the stored-reads record of 2026-11-12
  declined it because a date that moves rewrites the vector's row and its two index entries. Not reopened here.

**Reversing it.** The pause: `out === 0 && …` in `board-reader.svelte.ts`'s `setFilter` and `settled` in the launcher
session's `#schedule`. The first frame's cards: `FIRST_FRAME_CARDS` as large as `MORE_STEP`. The keys: `stored_field_in!`
back to the `stored_under!` shape. The board: `CARDS` for both, and `title` back in `scoped`.
