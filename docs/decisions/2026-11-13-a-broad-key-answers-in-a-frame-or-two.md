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

- ~~**The ids stay a second statement.**~~ Reversed later the same day (*The broad-keys pass*, below): the ids are
  gone, and the board's scan takes the search's predicates. What was tried here kept two scans of the tickets and
  moved only the ids: the search's ids read as a CTE inside the board statement cost the two statements' sum or more
  in every shape tried (a subquery planned the board from the wrong side; a materialized CTE added 5 ms).
- ~~**The launcher's match keeps `sync.live_item`.**~~ Reversed later the same day: the match stage reads
  `sync.item_search` alone and takes the live rows by the withdrawn entities hashed once. What was found here still
  holds (that shape loses the parallel plan under the planner's default charges: 83 ms to 125), and the later pass
  sets the charges.
- **Two smaller rewrites were measured and left:** the board's column list built from a hashed distinct (no gain), and
  `-priority:`'s two card sets read off `sync.item_card` without the materialized `fcards` (2 ms of 27).

**Not stored, and why.** Nothing, in this pass; the later one stored six columns beside the search vector
(migration `0084`, below). The brief offered the migration; no stored copy was found here to remove a wait a person
feels:

- **The board's cut.** A stored date on the card and an index in column order could serve each column's newest cards
  without the sort, about 15 ms of a broad key. It needs the cards of every source in one ordered read, which several
  sources' cards under several keys are not, and a walk beside it for the tickets without a card, which puts the sort
  back. Not taken.
- **`filter_suggestions`, 33 ms against 15.** What is left reads each source and kind's ids to find its newest 200
  (9 ms; the planner sorts rather than reading `item_source_updated_idx` in order, and its ties are taken in no stated
  order, so another order is another answer) and probes the stored values (one read a row instead of four: no gain).
  Tickets' first draw waits for it (`framed`, the still-rows pass), but it now waits for the board's own read longer,
  49 ms, so nothing a person sees waits on the suggestions; the answer cache keeps them after the first open.

**Left, with what it would take.** Both were taken later the same day, under *The broad-keys pass* below.

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

## The broad-keys pass (the same day, later)

**Asked (Björn, on PR #2479):** every broad key under 100 ms at 1x, on his scale, on slow laptops too. And on
storing: "just store more data, what does it take? the constraint is speed and feel, don't just save 'data' for
savings sake." The record above left each broad key 3 to 32 ms over the line, all of it the backend's read, and
ruled three things this pass reverses.

**Where the rest went.** The same rig: a release build of the windowless backend on clones of the saved scale
database, each statement split with `EXPLAIN (ANALYZE, BUFFERS)`.

- **A filter read every ticket twice.** The search's ids (21 ms for a letter), 17,000 ids out of the database and
  back (8 ms), and the board's statement over the same tickets again (47 ms).
- **A key's fallback kept its scan off the index.** `-project:` names the payload in the walk it makes for a
  source whose field is not stored. No source is like that on this mirror, and the scan still read each ticket's
  row to be able to: 25,000 pages where the index-only scan reads 1,150.
- **A key asked of the cards read every card again.** `-priority:` built a set of all 23,000 cards beside a
  statement that could join each ticket's card, and hashed it twice: 14 ms.
- **The board counted its columns in windows.** Two windows over every ticket for the count beside each row, and a
  sort of every ticket's status to find the eight there are: 9 ms, for the counts of the 300 rows kept.
- **`project:` read every project row to find the few with a chain.** 30,919 rows for 91: 5.5 ms.
- **A match under Everything fetched two more rows.** Per matched row, 2 µs for its row of `sync.item` (its kind,
  its date, the words it is named by), 1 µs for its row of `knobas.entity` (is it withdrawn), 2 µs to rank it, on
  three processes. `p` matches 35,232 of 39,770 rows.
- **One row in five kept its vector beside the table.** A vector over 2 kB was stored out of line, and read from
  there three times a match: to match, to test the typed word whole, to rank.
- **The planner saw no work to share.** It prices the ranking as one cheap operator a row. Once the match stage
  read one narrow table the statement looked cheap, and ran on one process: 97 ms.
- **Every hit's text was read to its end.** The page's forty hits carry 800 kB of text for their excerpts. A hit
  shows eight, a letter fills them in the first lines, and the rest was still cut into sentences.

**Taken.**

- **A filter narrows the board's own scan.** The search hands the board its predicates
  (`knobas_search::Narrowing`, a `knobas_core::mini_board::CardsNarrowing`) and no ids. The board's statement takes
  them as a column of its scan and cuts the rows above it, behind `offset 0`, so the scan and its join to the cards
  are planned as the unfiltered board's whatever the search keeps. A board bound to other declarations than the
  search's reads the ids first, as before; the app's never is.
- **A key's walk goes through the row's primary key** (`corpus::walked!`), so the scan stays index-only.
- **A key a stored card answers is read off the joined card** (`LIVE_ITEM_FIELDS_CARDED`): `status:`,
  `issuetype:`, `priority:`, `assignee:`, `label:`, `sprint:`, `assigned:`, in the ids and in the board's scan.
- **The board without lanes counts its columns once**, in one hashed pass, and keeps one window, for the cut.
- **Migration `0084`.** Six columns beside the search vector in `sync.item_search` (a row's kind, source, date and
  the three sets of words it is named by), vectors up to 8 kB kept in their rows, an index of the unfilled rows, and
  an index of the items whose project has a chain. On a mirror whose every search row is filled the launcher's
  match stage reads `sync.item_search` and nothing else (`LIVE_ITEM_MATCH`), the withdrawn entities read once a
  statement.
- **A word search is planned for the work the planner does not see** (`sql::MATCHED_PLANNING`): no charge a row
  passed up from a worker, a start-up charge of 850 where the default is 1,000, three workers. A word half the
  mirror holds is ranked on four processes, a rare one on one.
- **One character reads the narrow table whole** (`fetch_matched_by_a_letter`). Through the index, one process
  lists nearly every row while the others wait, 6 ms, and every page is read anyway.
- **An excerpt list stops reading at its last excerpt.**

The backend alone, the head before (`ec919049`) against this one, each read alternated between two release
backends on clones of the saved database, median of nine, every answer byte-equal:

| Read | Before | After |
|---|---|---|
| board, opening (eight a column) | 51 ms | 39 ms |
| board, `a` / `e` / `o` / `i` | 83 / 83 / 82 / 83 | 54 / 54 / 54 / 55 |
| board, `pay` / `retry` | 58 / 50 | 44 / 41 |
| board, `-project:PAY` / `-project:SCLWEB` | 87 / 85 | 53 / 54 |
| board, `project:OPS` (no ticket) | 45 | 29 |
| board, `-status:Done` / `-assignee:@me` | 72 / 87 | 38 / 48 |
| board, `-priority:High` / `Medium` / `Low` | 88 / 76 / 86 | 46 / 39 / 44 |
| board in lanes, `a` / no words | 94 / 64 | 75 / 65 |
| search, `p` / `r` / `c` under Everything | 99 / 106 / 104 | 52 / 56 / 52 |
| search, `b` / `l` / `g` / `w` | 95 / 95 / 54 / 95 | 48 / 47 / 35 / 48 |
| search, `pa` / `retry` / `deploy` | 93 / 78 / 73 | 47 / 41 / 40 |
| search, `iban` / `ledger drift` / `PAY-231` | 16 / 9 / 9 | 15 / 11 / 11 |
| search, `p` / `retry` under Titles | 58 / 23 | 33 / 18 |
| `filter_suggestions` | 33 | 33 |

From the key to its rows on screen, real input in headless Chrome, a key never asked before each press, median of
seven (five for `-priority:`); no row drawn on screen moved in any press:

| Key | 1x before | 1x after | 4x before | 4x after |
|---|---|---|---|---|
| Tickets, the first letter | 105 ms | 71 ms | 128 ms | 101 ms |
| Tickets, `-project:` | 112 | 83 | 136 | 104 |
| Tickets, `-priority:` | 119 | 64 | 134 | 93 |
| Everything, one letter | 121 | 91 | 144 | 101 |

Both columns were measured in one session with other agents' builds and browsers on the machine (a load average of
9 to 10 during the *after* run, 5 to 9 during the *before*), which the four processes of a one-letter search feel
most: its backend read was 52 ms asked alone and about 70 during the run. A second run of five presses a key, as
loaded, read 96, 75, 67 and 96 at 1x. `testenv/perf/budget-keys.json` holds the 1x lines at Björn's 100, and 90 for
`-priority:`.

**Same answers.** Each changed read keeps the read it replaces in the tree, and a test asks both.

- `tests/it/board_narrowed.rs` (knobas-app) holds the board narrowed in its scan to the board of the ids, the two
  statements a filter was: 53 queries (words that reach the key, the title, the assignee and a parent's name, each
  key alone, left out and repeated, keys beside words, and `under:`, `reach:`, `updated:`, `by:`, `name:`, `has:`,
  `type:`), with and without lanes, cut and whole, narrowed to some ids too, with every card stored, a third gone
  and none, under declarations stored and never stored, with a ticket withdrawn and a source switched off; and the
  ids-first read under declarations that are not the search's.
- `tests/it/ticket_keys.rs` (knobas-search) holds each key read off the card and each walk behind the probe to its
  row-by-row read, as it held the sets.
- `tests/it/item_card.rs` holds the board without lanes to the board in lanes asked for none, as before.
- `tests/it/item_search.rs` holds the match stage off the narrow table to the stage joined to the mirror and to the
  mirror read alone (probes, scopes, typing, limits, sources, kinds, a kind left out, a kind answered as a
  pipeline, a window of days, entities withdrawn, a source switched off, two tickets that tie on rank so the key
  names decide); a search to the same answer with every other row unfilled and with none filled; the six columns
  to the mirror's after every kind of write, and a stamp to no write at all; and the fill.
- `excerpt.rs` holds the early stop to reading to the end.
- Fifteen broken versions each fail a test: the narrowing keeping every row, the ids-first read forgetting the scope's
  ids, a column's count one too many, six of the keys (a card's column swapped, a label compared as spelled, a walk
  behind the probe reading nothing, the sprint key saying yes), the narrow table read while rows are unfilled, the
  match stage forgetting the withdrawn entities and the sources switched off, a match not named by its key, a query
  with a ticket key matched off the narrow table, an excerpt list stopping one short. Two of the keys' passed the
  board's test, which reads a key the same way on both sides, and fail the keys' own. One, the key names, passed
  until the test held two tickets that tie on rank.

**Ruled along the way.**

- **The narrowing is a column behind `offset 0`, not a `where`.** As a `where` on the scan the planner, which
  cannot count what a search keeps, halved its estimate for each test: `-project:PAY` keeps 20,000 tickets, was
  counted as 1,489, and each ticket's card was fetched by its key, 21 ms where one read of the cards takes 5. A
  fence around the cards instead made it worse than anything (the cards rescanned per ticket, 18 s). The fence
  around the scan gives every search the unfiltered board's plan.
- **The launcher reads the narrow table only when every row is filled.** Asked of each row instead (the copy where
  it holds the six, `sync.item` by key where not), the lookup for an unfilled row is a correlated subplan, which
  PostgreSQL runs above the workers: the whole ranking on one process. A filled mirror stays filled, since every
  trigger writes the six with the copy, so one probe of an index that is empty after the fill decides
  (`corpus::mirror`).
- **The start-up charge is calibrated, on one mirror.** At 700 a word 553 rows hold (`iban`) starts workers and
  takes 13 ms where it took 10; at 1,000 a word 19,273 rows hold (`deploy`) no longer does. 850 sits between. The
  planner's own estimate grows with the rows a word matches and with the mirror, so the line moves the right way on
  another one; where it falls between those two words, and on another mirror, was not measured.
- **Windows was not measured.** Starting a worker costs more there than on the Mac. The broadest words already
  ran on three processes before this pass, by the planner's own choice.
- **The vectors stay in one table.** A second copy of each vector in a table built for the match stage was the
  cleaner build, and 80 MB more. The rows are rewritten in place instead.
- **No `VACUUM FULL` after the fill.** It would give back 48 MB and hold every search for a second and a half once.

**What it costs.** One migration, `0084`: 57 ms at the first start on the scale database. After the app is
up the fill rewrites every row of `sync.item_search` once, about 6 s of database time, done within ten seconds of the start, in the background, behind the cards' and the
rows' fills, and the launcher reads as before until it is done. The search copy goes from 70 MB to 126 MB on
disk: the vectors that were out of line are in the table (28 MB of table to 74), and the 29 MB they left stay
allocated to it, room only a vector over 8 kB will use. A date that moves now rewrites its search row; with a
seventh of each page left free that is a heap-only update and writes no index entry. A re-sync of an unchanged
item writes nothing here.

**Left, with what it would take.**

- **One letter under Everything has 9 ms to spare on a loaded machine.** 52 ms in the backend alone. Ranking
  35,000 rows is 60 ms of processor time whoever reads them, spread over four processes; the cut to ten a kind reads
  the matches once a kind, 10 ms; planning the statement is 6. Further would take the rank of each row for each
  first character stored beside its vector, so that one character reads no vector at all, or the page's rows drawn
  by a second, prepared statement, so that the first plans in a third of the time. Neither was built.
- **A rare word is a millisecond slower**, 9 ms to 11: the match stage reads the withdrawn entities once a
  statement, 2 ms, where it probed one a match.
- **The board in lanes kept its windows.** A filtered laned board went 94 ms to 75, from the scan alone.
- **Every key is about 100 ms at 4x**, half the line. What is left there is the page's own work, not the read.
- **`filter_suggestions`**, 33 ms, untouched.

**Reversing it.** `0084` stays. The board: `Admits::Some` in `commands/entity/board.rs` back to the ids
(`Narrowing::ids`, `Scope::only`). The keys: `fields_carded: &[]` on `LIVE_ITEM`. The cut: `ranked_unlaned!` back
to its three windows. The launcher: `corpus::mirror` answering `LIVE_ITEM`. The planning: `MATCHED_PLANNING` back
to `enable_seqscan` alone, and `rows_of` to `fetch_matched` for every word.
