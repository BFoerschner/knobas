# 2026-11-15: One letter is answered by names

**Asked (Björn, on PR #2479, through the orchestrator):** one letter under *Everything* must answer within the key
targets on his PC: 100, 100 and 200 ms from the key to its rows at CPU 1x, 4x and 6x. The ux13 Windows pass measured
132, 166 and 260 ms in knobas's own window, the search alone 121 ms. The agent before stopped there: "it would need a
decision on how a single letter is ranked". His standing rule: directness, performance and responsiveness over data
scarcity. Decided by Fable under his delegation, and carried out in the same pass.

## What one typed letter is for

A letter is the start of a name. Somebody typing `p` is on the way to `payout-service` or `PAY-231`, and the second
key comes a blink later.

The ranking of words had nothing to say about that. `p` matches every row holding a word that begins with it, 35,232
of 39,770 on his scale, and ranks each by how many such words it holds. The first rows for `p` were the tickets and
pages with the most words beginning with p, whatever they were about. That number is the whole cost, a vector ranked
for every match on four processes, and it depends on the letter, so nobody could store it. It was also no use to the
person.

## Decided

1. **One typed character is answered by names.** A row answers when a word of its title or key begins with the
   character: what *Titles* matches. Under *Titles* and under *Everything* alike. Bodies join at the second character.
2. **Those that begin with it first, then the newest.** A row leads when a name of it begins with the character: its
   title past its stop words (*The payout service* leads with `p`), the text after the title's last `/`
   (`tidewater/payout-service`), a ticket's key, a pull request's number, its key cell. Among rows that lead, and among
   those that do not, the newest first, the id last.
3. **A row a kind at a time.** Ten rows a kind as before, and the page's forty are dealt: every kind's first row, then
   every kind's second. One letter says nothing of which kind is meant, and the forty newest of all are the kinds that
   change most: for `p` that was tickets, pull requests, pages and repositories, and no pipeline or monitor at all.
4. **A stop-word letter answers as any other.** `s`, `a`, `d`, `i`, `m`, `o` and `y` answered nothing, because the
   `english` configuration reads them as stop words. They name rows now. `t` alone stays the time prefix.
5. **No match is counted.** A kind's `total` is the rows the answer holds of it. Counting the matches is the read
   this decision removes, and no screen draws the count (#2421 took it off the headings).
6. **A character no name holds is asked of the bodies under *Everything*,** as two characters are, so *nothing
   matches* is never said of a letter a body holds. `x` on his scale: no name, 727 bodies. Under *Titles* the names
   were the question and none is the answer.
7. **Two characters and more are untouched.** The same statement, the same answer, byte for byte.

## What was weighed

- **Store each row's rank for each first character,** so the old answer comes off an index. 672,505 rows on his
  scale against 253,585, a row's 17 characters rewritten on every edit of its body, to keep an order that means
  nothing. Not taken.
- **Draw a cheap first screenful and let the full ranking replace it if it differs.** It differs for nearly every
  letter, so rows would move under the pointer a frame after they drew, which the still-rows ruling of 2026-11-11
  forbids. Drawing nothing until the full ranking lands is today's wait.
- **A list of each letter's first rows, written during the sync.** 36 letters by ten kinds by ten rows is small, but a
  list answers no filter (`#p p`, a source chip, a window of days), and a row that leaves it needs the full read to
  find its successor.
- **Names, in an order that depends on nothing typed.** Taken. The order is an index, so the answer is the index's
  first rows, and it is the answer a person meant.

What moves: nothing. One answer is drawn, the names' or, for a character no name holds, the bodies'. Never one and
then the other.

## How

**Migration `0093`: `sync.item_initial`.** For each mirror row, one row for each first character of the lexemes of
its weight-A vector, with `leads`, and the row's kind, source and date. So `initial = c` holds exactly where
`fts_a @@ 'c':*` does. `item_initial_newest_idx (initial, kind, leads desc, item_updated_at desc nulls last,
entity_id)` is the answer's order within a kind. Two statement triggers on `sync.item` keep it: one writes a new
row's characters, the other writes again those of a row whose title, key cell, kind, source or date an update changed.
A stamp or an edit of the body writes nothing. A deleted row's go by the foreign key. The migration fills it in one
pass, so there is no background fill and no state in which the launcher must read the old way.

**The statement (`knobas_search::sql::letter_sql`).** The typed character is lexed under `simple`, which lower-cases
it and drops no stop word. One probe of the index a kind finds the kinds that hold it. For each, the first ten live
rows are read in the index's order, and the read stops. Liveness is two probes a row read
(`knobas_core::live_by_key!`), where the words' statement hashes every withdrawn entity once (2 ms). The other
corpora (views, sets, saved lists, assets, routes, notes) hold tens of rows and match their weight-A lexemes as they
do under *Titles*. The stages after are the words' statement's own: the cut by kind, the page, the page's titles, path
lines, state chips and excerpts. No run log is searched: a log is no name.

**One plan, kept (`fetch_named`).** The statement's work is the same whatever was typed, so it runs under
`plan_cache_mode = force_generic_plan`: planned once a connection. Planned for each key it took 4.5 to 16 ms to plan
and 4 to 5 ms to run. No worker is started: on his PC a worker costs 31 to 37 ms.

**A character no name holds** is asked with the words' statement through its index (`fetch_matched`), not on the four
processes a letter read whole was given: such a letter is rare in bodies too, and on his PC the workers alone cost
more than the read.

**The first frame draws the first 24 rows** (`launcher/Results.svelte`, `FIRST_FRAME_ROWS`), the rest the frame
after, below them. Forty rows cost about 1 ms each of a quarter-speed main thread, and the tallest list (*Everything*'s
840 px dialog) shows 24. A list that already drew that many draws the next answer whole at once, so no answer takes
rows away for a frame. Nothing drawn moves. This is the Tickets board's rule of 2026-11-13 for a column that grows.
Nothing looks different, so it did not go through the design skill.

## What it costs

- **Disk.** 253,585 rows on his scale, 6.4 a row: a table of 23 MB and 33 MB of indexes, in a database of 839 MB.
- **The upgrade.** One pass at the first start, 2.6 s on that mirror with the Mac busy (the rows 1.8 s, the key 0.2 s,
  the index 0.5 s). Migration `0077` took 1.5 s the same way.
- **The sync.** A write of a row whose name or date changed lexes its key and its key cell once more and writes about
  six narrow rows. A write that changes neither writes nothing.
- **The contract.** `SearchResponse.total` and `ResultGroup.total` are, for one typed character, the rows the answer
  holds. The generated mirror's two doc comments say so.

## Before and after

On the Mac (M2 Pro, on AC power), release builds of the windowless backend on fresh APFS clones of the saved scale
home, the head before (`0998d62d`) against this one, the two alternated. Other agents' builds were running the whole
time, at a load average of 7 to 13.

The backend's own time on the read (`took_ms`), median of nine:

| Read | Before | After |
|---|---|---|
| `p` / `r` / `c` under *Everything* | 57 / 61 / 63 ms | 10 / 10 / 11 |
| `b` / `l` / `g` / `w` | 47 / 48 / 35 / 48 | 10 / 9 / 10 / 9 |
| `p` / `r` / `c` under *Titles* | 33 / 38 / 34 | 9 / 8 / 9 |
| `s`, `a` (stop words, answered nothing) | 7 / 7 | 9 / 10 |
| `x` (no name, 727 bodies) / `z` (12 bodies) | 15 / 15 | 20 / 15 |
| `pa` / `retry` / `payout` | 100 / 67 / 56 | 103 / 69 / 58 |

Of the 10 ms, the statement runs in about 4 and the rest is the round trips of its transaction and the vocabulary.
A search that answers nothing takes 7.

From the key to its rows on screen, real input in headless Chrome (`keys.mjs`, `everything-letter`), median of seven
presses, three rounds, the baseline page on the baseline backend against this head's on its own. No row moved in any
press:

| CPU | Before | After | Björn's line |
|---|---|---|---|
| 1x | 84 / 84 / 78 ms | 42 / 37 / 42 | 100 |
| 4x | 107 / 125 / 121 | 61 / 63 / 77 | 100 |
| 6x | 125 / 123 / 164 | 94 / 94 / 80 | 200 |

**Expected on his PC:** about 30, 60 and 140 ms at 1x, 4x and 6x, against 132, 166 and 260. The read there was 121 ms
of the 132, on four processes it paid to start. It becomes about 1.5 times the Mac's 10 ms with no worker, so 15 to
20 ms. The page's own share there was 11, 45 and 139 ms, and the first frame's cut takes some of the last two. The 6x
figure is the least certain: part of that 139 ms ran while the page waited for the read, and no longer has a read to
hide behind. Not measured there: the brief kept this pass off the PC.

The aim was half his line on the Mac. It holds at 1x and 6x. At 4x the Mac is at 61 to 77 ms: under his line, not
under half of it. What is left there is the page (the answer is in after 24 to 36 ms): the dialog growing to its
*Everything* size, the preview pane, and 24 rows.

`testenv/perf/budget-keys.json` holds `everything-letter` at 70, 100 and 130 ms. A later run of the whole budget, with
another agent's build on the cores, read 74, 59 and 88.

## Same answers

- `tests/it/one_letter.rs` (knobas-search), seven tests. The decided answer on rows written for it, with leads
  first, then newest, a body alone no match, the page dealt a kind at a time, and a stop-word letter. The bounded read
  held to a ranking of every name match with each row's lead worked out from the row (`letter_sql_unstored`), for 38
  characters, plain and four ways narrowed, at three page sizes. The names held to what *Titles* matched for every
  character that is no stop word. The stored rows held to the mirror's after an insert, a new title, a date, a kind,
  an upsert, a stamp, an edit of the body and a delete. A withdrawn row and a source switched off. Twelve queries of
  two characters and more and two finished one-letter words (`p` and a space), held under both scopes to the answer
  the words' ranking gives (`Searcher::search_by_words`). And the plan, each kind read through the index and never
  sorted.
- `sql.rs`'s own test holds the statement to the builder's two rules, the typed character a bind and the literals the
  closed list, and to refusing what the stored characters do not hold.
- `Launcher.first-frame.test.svelte.ts`: the first answer draws 24 rows and the frame after all forty, each of the
  first still the element it was; a list drawn whole draws the next answer whole.
- On the scale clone, 18 queries of two characters and more, or of a finished word, answer byte for byte as the head
  before.
- Sixteen broken versions each fail a test. Twelve of the read and the stored rows: two characters, or a finished
  word, taking the names' path; a kind read newest first with the leads forgotten, in the statement and in the index;
  every row leading; the page not dealt; a withdrawn row answered; the bodies not asked; a hidden kind walked; a date
  that moves, or a new title, leaving the stored rows as they were; a title's lead taken by spelling. Four of the
  first frame: every row at once, a whole list cut, a heading ahead of its rows, the rest never drawn. The hidden kind
  walked changes what is read and no answer, so it passed every answer test until the statement's own test pinned it.

## Left, with what it would take

- **Two letters are as dear as one was.** `pa` is 100 ms of backend on the Mac and `re` 123, so more on his PC. The
  second key waits for the pause (90 ms) and then for that read. The same idea reaches it: a stored two-character
  prefix, 300,000 rows more. Not built: nobody measured the second key, and the first is the one that makes the box
  feel alive.
- **The box says *Searching…* for a frame or two** between the first key and its answer, where the board was. With a
  10 ms read that is a flicker. Keeping the board until the answer lands needs a ruling on what Enter does meanwhile,
  so it was left.
- **A ticket is not found by its number alone.** `7` names pull request 7 and no ticket: a key's number is lexed with
  its hyphen (`-231`), as before this pass.
- **Two-letter stop words still answer nothing** (`an`, `as`, `in`, `to`): the words' ranking, unchanged.
- **The Docs search** asks one letter the old way. It reads pages alone, 5,032 of them.

## Reversing it

`Searcher::search_as(query, false)` is the old behaviour whole: `rows_by_name` answering `Named::NotAsked`. `0093`
stays, and its table is then written and never read. The first frame: `FIRST_FRAME_ROWS` as large as the page.
