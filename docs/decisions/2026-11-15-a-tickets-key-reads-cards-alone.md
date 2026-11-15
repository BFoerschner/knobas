# 2026-11-15: A Tickets key reads the cards alone

**Asked (Björn, on PR #2479):** every broad Tickets key was over the 4x line in knobas's own window on Windows (116 to
214 ms), because the read alone answered in 81 to 115 ms at 1x on his PC, about 1.5 times the Mac's. The ux12 pass
(`2026-11-15-the-ux12-windows-pass-fixes.md`, item 4) left the next step named: store what the board cuts and filters
by on each ticket's stored card, so the board reads cards alone. Aim: every broad key's read under about 45 ms at 1x
on the Mac, room for the PC's 1.5 times. His rule: directness, performance and responsiveness over data scarcity.

**How it was measured.** Headless Chrome on the Mac (M2 Pro, on AC power) at CPU 1x, 4x and 6x, through release builds
of the windowless backend on APFS clones of the saved scale home (39,770 items, 23,176 tickets), first brought to the
head's schema and every stored table filled, then vacuumed and analyzed. Before is `cf2a729c`, after this head; two
backends on two clones side by side, their reads alternated. The backend's own time on a read is the median of five
rounds of each key (`mini_board` and `search` asked over the backend's HTTP, every key of `keys.mjs`'s set);
statements were split from the server's log and with `EXPLAIN (ANALYZE, BUFFERS)`.

## Where a letter's read went

54 ms on the Mac. The board's statement was 44 of it: parse 1, plan 2 to 4, execute 41. Beside it, on a second
connection, the statuses (10 ms); after it the signals (2 ms); the rest the answer.

Of the statement, `scoped` was 25 ms: each ticket's entry of `item_ticket_brief_idx` read for its title and date
(5 ms, 1.9 of it the withdrawn entities' scan), hashed with its key, and every card probed against it (14 ms); then each
row's filter, whose words took `lower()` of the ticket's key and its title, 23,176 times a letter (about 5 ms). The cut
sorts every kept ticket by column and date (9 ms); the 300 kept cards' fields, a probe of `sync.item` each for the title
and date, and the parents' titles came to 4 ms. Planning cost 2 to 4 ms in a run and 9 cold, because the statement
carries the payload's walk behind every field for a ticket without a card.

## Taken: migration `0089`, the card carries what the board cuts by

- **Five columns on `sync.item_card`**: the ticket's `title` and `item_updated_at`; `words`, its key and its title
  lowered and joined by U+001F, which a word without that character is found in exactly where it is in one of the two;
  and `project_key` and `project_name`, the declared project, whose paths go into the card's key (so the table is
  emptied at the migration and filled again, as `0082` did). A third statement trigger, `item_card_on_change`, writes
  the title, date and words of the cards of a row whose title or date an update changed; a stamp changes neither and
  writes nothing, and no index holds a column it writes, so on a page with room (`fillfactor` 90) it is heap-only.
  `sync.resolve_item_cards` now upserts the cards it writes and deletes only those it did not, in one statement.
- **`sync.card_whole (source_id, paths_key)`**: every ticket of the source has its card under the key, and none is
  under another. `sync.mark_cards_whole()` recomputes it from the cards and the tickets (each source whose cards are
  exactly its tickets, under one key) under `0028`'s key taken exclusive, or does nothing while a write holds it.
  Bring-up runs it after the fill; a board that found its scope not whole runs it beside the board it then reads, at
  most once a half minute, so a mirror a first sync wrote is read off the cards from the next key on. A row goes when
  a write leaves a ticket of the source without its card under the key or gives the source a card under another
  (`sync.resolve_item_cards`), when a card goes while its ticket stands (`item_card_on_delete`, which the declaration
  trigger's delete meets), and at a truncate.
- **The board off the cards alone** (`knobas_core::mini_board`, `cards_carded!`). Where every source the scope admits
  is whole under the key the declarations give it, `scoped` is one read of `sync.item_card`: the ticket `i` is its
  card's id, source, kind, title and date, the status, lane and project the card's, no join to a key and no walk. The
  kept cards read their fields, title and date off the card. Where a source is not whole the statement answers one row
  of no ticket's id, and `read_scoped_narrowed` reads the board as it always did.
- **The filter off the card** (`knobas_search::sql::board_sql` over a carded statement): the keys read the card's
  columns (`LIVE_ITEM_FIELDS_CARD_ONLY`, `project:` the card's project beside the chain), the words `crd.words`, and of a
  key's CTEs only `fdecl` is written, so the statement plans in a third of the time.

Nothing else was stored. `-project:`'s stored set and its walk are gone from the carded statement, the words' two
`lower()`s a row are one `strpos` of a stored text, and the card's key join is gone from the scan.

## Before and after

The backend's own read, median of five rounds of every key, the two backends alternated twice (ms):

| Read | before | after |
|---|---|---|
| the board's open, eight a column | 40 | 33.5 |
| a letter (`a e o i s r n`) | 54 | 39.4 |
| `-project:` (seven projects) | 54.5 | 40 |
| `-priority:` (five priorities) | 45.7 | 37.4 |
| the expanded board, a letter (250 a column) | 71.4 | 53.5 |
| the expanded board, `-project:` | 72 | 55.7 |
| a letter under Everything | 48 | 48.5 |

From the key pressed to its rows on screen, `just keys-budget --real`, median of seven presses (five for
`-priority:`), no row moved in any press (ms, before / after):

| Key | 1x | 4x | 6x |
|---|---|---|---|
| Tickets, the first letter | 84 / 57 | 80 / 64 | 95 / 77 |
| Tickets, `-project:` | 94 / 60 | 81 / 61 | 84 / 69 |
| Tickets, `-priority:` | 65 / 49 | 72 / 61 | 83 / 73 |
| expanded, the first letter | 98 / 69 | 106 / 84 | 118 / 96 |
| expanded, `-project:` | 90 / 68 | 106 / 92 | 110 / 98 |
| Everything, one letter | 82 / 83 | 98 / 98 | 106 / 108 |

The board's open, `just open-budget --real --only tickets-board,ticket-preview,ticket,standup`, before and after on
the two backends (ms; the 4x and 6x first opens draw what the answer cache warmed, so their reads are a few ms):

| Tickets board | useful | drawn | settled | the board's read |
|---|---|---|---|---|
| 1x, first open | 143 / 60 | 159 / 77 | 143 / 81 | 48 / 39 |
| 1x, after a sync | 31 / 31 | 47 / 48 | 63 / 59 | 42 / 37 |
| 4x, first open | 32 / 18 | 182 / 168 | 172 / 165 | 3 / 3 |
| 4x, after a sync | 27 / 33 | 44 / 49 | 80 / 77 | 44 / 35 |
| 6x, first open | 18 / 21 | 252 / 237 | 230 / 223 | 5 / 4 |
| 6x, after a sync | 19 / 29 | 68 / 62 | 95 / 88 | 46 / 37 |

The 1x first open before was over its 100 ms line: the first read of a fresh backend, it waited for
`filter_suggestions` (117 ms cold; 41 after; it reads the cards' values among others, and one cold read is the
noisiest number here). A ticket's page and Standup are within a few ms either way.

**Expected on the PC.** The Tickets reads that took 81 to 115 ms there were 54 to 72 here; at the PC's 1.5 times the
reads after are about 56 to 60 ms for a letter, `-project:` and `-priority:`, and 80 to 84 for the expanded board,
which puts every Tickets key's rows near 75 to 90 ms at 1x and inside 200 at 4x in his window. Owed: the keys in the
window on the next build.

**First start and disk.** The migration is 24 ms (five columns added without a rewrite, a table of a few rows, the
cards truncated). The fill then writes every ticket's card again behind the app, as after `0082`: 3.2 s of database
time for 23,176 tickets in batches of 300 (about 40 ms each; 500 took 68 ms with the new columns), and the mark 14 ms
after it. Until a source's tickets are all filled its boards read as before. The cards go from 9.5 MB to 14.2 MB, and
`sync.card_whole` is 64 kB.

## Same answers

- On the scale clone, 101 answers of the two backends are byte-equal (34 MB): the board open at 8, 50, 250 and every
  card; sixteen letters at 50 and 250; seven projects asked and left out; five priorities; status, assignee, `@me`,
  label, sprint, `has:`, `under:`, `updated:`, `by:`, `name:`, `reach:`, a phrase, words left out and alternatives;
  lanes by every field; two project rooms.
- `tests/it/item_card.rs`: every stored field is the walk's for every adapter, the five new ones among them; the board
  off the cards is the board joined row for row on one snapshot, in lanes by every field and without, cut and whole,
  narrowed and not, a project room, dates that tie, a withdrawn ticket, a title and date changed alone; a scope with a
  source not whole answers the one row and nothing else, and a scope without it still reads the cards; a source stays
  whole across a ticket written, rewritten, renamed, deleted and made a page, and stops at a card deleted or a card
  written under another key.
- `tests/it/board_narrowed.rs`: every word and key the grammar has, in the board's scan off the cards, is the board of
  the ids, and the same boards as with a third of the cards gone and with none.
- Mutants: the words reading only the first four characters of `crd.words` fail `board_narrowed`; the migration without
  `item_card_on_change` fails both new `item_card` tests.
- `read_plans.rs` plans both carded statements on tables analyzed and never analyzed. Two tables are small by
  construction and in its list: `sync.card_whole`, one row a source, and the two CTEs `ticket_sources` and `whole`.
  The first version joined the cards to their keys, which on tables never analyzed was estimated at a few rows and
  rescanned once a lane; that is why the scan reads the cards with no key and `whole` asks for one key a source.

## Gates

On this head, in a clone of its own: `just check-front` ok; `bash testenv/desktop-witness-test.sh` ok (307 checks);
vitest 5,123 passed in 333 files (1 skipped); `svelte-check --threshold error` 0 errors in 1,466 files; the press trips
with `--strict`, `rows-still` among them, 272 of 272; `just frame-budget` 45 within and `--real` 65 within; `just
open-budget --real` every open within its line (225 opens); `just keys-budget --real` every key within its line;
`just check-scripts` ok; `just clippy`, `just clippy-libs` and `cargo fmt --all --check` clean; `just wasm-fixture` then
`just test` 4,909 passed with one failure, the known flaky `coverage_plans::a_probe_never_reads_a_table_to_prove_a_source_has_nothing`,
which passed alone three times out of three.

`just test` rewrites `app/src/lib/ipc/generated/entity.ts` without `RangeNames` and 43 more lines in the clone (ts-rs
exports from several test binaries into one file), which `svelte-check` then fails on; the file was restored from the
head before `svelte-check`. Not this change's; noted for whoever next runs both.

## Left

- **One letter under Everything**, 48 ms, unchanged: the launcher's match stage ranks every row a letter matches (28,955
  of 39,770 for `p`), its order begins with the rank, and the rank is the query's, so it cannot be stored or cut first
  without another answer. Planning is 5 ms of it.
- **The cut's sort**, 9 ms of a broad key: every kept ticket sorted by column and date. A bounded sort per column was
  measured and refused before (`read_plans.rs`, a rescan per column).
- **The statuses beside the board**, 10 ms on their own connection, still count the tickets and the cards to tell a
  whole source; `sync.card_whole` could answer that in a probe. Off the critical path while the board takes longer.
