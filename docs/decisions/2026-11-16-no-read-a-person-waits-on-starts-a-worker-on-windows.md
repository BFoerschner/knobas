# 2026-11-16: No read a person waits on starts a PostgreSQL worker on Windows

**Asked (Björn, on PR #2479, through the orchestrator):** that the worker problem is fixed. No read a person waits on
(pages, lists, keys, the launcher, the Docs search) pays for worker processes on Windows; background work (the sync,
the fills) may keep workers where they help. Measure on his PC first, decide from its numbers, and lock it in with a
check a later change cannot get past. His standing rule: directness, performance and responsiveness over data
scarcity; a judgment goes to one Fable agent. Targets at CPU 1x / 4x / 6x: a page usable within 100 / 200 / 300 ms,
a key within 100 / 100 / 200 ms.

## What a worker is, on Windows

A process PostgreSQL starts for one query and ends with it. PostgreSQL on Windows has no `fork`, so each is a new
process: on his PC three workers cost about 24 ms of a query that runs in 14 without them (`x` below). None can be
kept between queries. The long-words pass (`2026-11-16-a-word-many-rows-hold.md`, part three) made the app's
connections on Windows ask for none by default and left the numbers on his PC to this pass.

## How it was measured

On his PC (Ryzen 5 5600X, idle all pass, nothing on screen): PostgreSQL 18.6 from the ux14 build's own archive,
unpacked into a scratch folder and run by hand with no window, on a copy of the scale data at this head's schema
(migration `0095`, 39,770 items, 23,176 tickets, 5,032 pages; a `pg_dump` of the saved scale home after this head's
windowless backend migrated it and its fills finished), with the app's `shared_buffers` and durable settings. Each
statement was sent as the app sends it: its parameters as logged by the Mac's server, after the `set_config` its
transaction runs first. Median of eight, at 1x (the throttle slows the WebView only). "With" is the plan as the app
asked for it with the server's own rule for the rest; "without" adds `max_parallel_workers_per_gather` 0.

To find which reads plan a worker at all, the Mac's windowless backend ran the open budget's tour of every page at
1x with `auto_explain` logging every plan, and every distinct statement the tour sent (222 statement and parameter
pairs) was then planned on the PC's server. A setting that only changes plans with a `Gather` changes nothing in a
read that plans none, so a read with no `Gather` is the same with and without workers.

## The PC's numbers

| Statement | Workers | With | Without |
|---|---|---|---|
| Tickets' views' text counts (`Searcher::readings`), on every opening of Tickets | 2 | 32 ms | 34 ms |
| Tickets' saved views' own counts (`ticket_views` `SAVED_VIEW_SOURCES`) | none planned | 18 | 18 |
| Docs search, one character read whole: `r` / `p` / `x` | 3 / 3 / none | 50 / 51 / 8 | 67 / 52 / 8 |
| Launcher, one character read whole while the names fill: `r` / `p` / `x` | 3 | 95 / 82 / 38 | 169 / 137 / 14 |
| Docs words ranked whole: `payout` / `retry` / `iban` | none planned | 29 / 33 / 10 | 30 / 31 / 10 |
| Launcher word with an author: `payout` / `retry` | 2 | 62 / 60 | 53 / 56 |
| Launcher name glob `pay*` | none planned | 7 to 9 | 7 to 9 |
| Docs one character off the pages' names, every one counted (this pass): `r` / `p` | none | | 16 / 13 |
| The same in a space: `r` in SCLOPS (1,504 pages) / SCLPRD (332) | none | | 31 / 23 |

Of the 222 statements of the tour, two planned a `Gather` on the PC: the views' text counts and the one character
read whole. Fourteen (the inbox's rules and a context walk) could not be planned again from the log, their
parameters untyped; on the Mac, where `auto_explain` logged every plan of the tour, none of them held a `Gather`, and
the check below plans them through the app's own code. On the Mac the tour started 64 workers, all of them for the
views' text counts (32 openings of Tickets, two each).

**The views' text counts were work for nothing.** Each view's text was counted over every live ticket the text
admits, once per view: Björn's nine views hold one text, so the statement counted his 23,176 tickets nine times. And
all nine are saved Jira views, whose count is the one their source put on them (the saved views' own statement), so
every one of those nine counts was thrown away.

## Decided (Fable, xhigh, under Björn's delegation)

1. **Windows only.** On Windows the app's connections plan no worker (as before), and the search's two plans that ask
   for three workers ask for none there: `sql::matched_planning(windows)` and `sql::letter_planning(windows)`, the
   flag carried by the `Searcher` (`planned_as_windows`, set from `cfg!(windows)`), so a test plans the Windows
   statements on any platform. On macOS and Linux a worker is about 2 ms to start and the measured broad letters
   are faster with them; dropping them there would be a slowdown nobody asked for.
2. **The Docs search's one character is answered by the pages' names** once every item has its stored characters,
   as the launcher's one character is (`2026-11-15-one-letter-is-answered-by-names.md`): the pages a name of which
   holds a word beginning with it, those whose name begins with it first, then the newest
   (`sql::page_letter_sql`). Three conditions, all held by a test:
   - **The total is counted**, every such page, past the page's fifty: the page prints *N pages*, and a count of
     the rows the answer holds would say 50 for a letter thousands of names hold. Index entries counted, under the
     kept plan, never ranked.
   - **A character no page's name holds is asked of the bodies**, as every query was before, so *no pages* is never
     said of a letter a body holds.
   - **A search within a space keeps to it.** The space is a `project:` filter, which `letter_sql` refuses: the
     Docs statement asks it of each name's item.
   Read whole, `r` was 80 ms of backend on the PC before anything drew (67 and the hits' rows), over the key's line
   at 4x, for an order by how many such words a page holds. Off the names it is 16 ms, 3,019 pages counted (the
   launcher's uncounted names read is 2.2), and 23 to 31 ms in a space, where each name's item is asked its
   space.
3. **The launcher's one character while the names fill is worker-free** on Windows: 169 ms for `r` on the PC, for the
   seconds of the fill, once after an upgrade. With workers it was 95, still over the key's line, and rare letters
   slower (`x` 38 against 14). The fill-time answer was ruled as *the same answers, a longer wait*; the list keeps
   what it drew and Enter waits for the answer, so the wait shows nothing wrong and opens nothing wrong.
4. **The views' text counts** are read only for views whose count is not their source's, and a text is counted once
   however many views hold it. On his data the statement is no longer sent.

## How it is held

`knobas-app`'s `tests/it/read_plans.rs`:

- **`on_windows_no_read_a_person_waits_on_plans_a_parallel_worker`**: every read the file's plan test plans (the
  board in its four shapes, the inbox's rules, the board's statuses and signals, the launcher's newest, the Docs
  tree, the tray, the Builds reads, the alert roster, the saved views' counts), every inbox rule, and the search's
  statements (the one character read whole, filled and not, the Docs one character off the names with and without a
  space and read whole, a word's first characters, a word's count and its bounded answers, a Docs word and a
  launcher word with an author ranked whole, the views' text counts) are planned on a pool opened as the app's is
  on Windows (`app_session_settings(true)`), under each statement's own settings as on Windows, with a worker then
  made to look free for any scan of any table, on the mirror analyzed and never analyzed. None may hold a
  `Gather`. The same pool opened as elsewhere must plan the views' text counts and the one character read whole on
  workers: the mirror is one a worker would be started for.
- **`only_the_listed_statements_set_the_worker_cap`**: every place in the crates' source that sets
  `max_parallel_workers_per_gather` is listed with what it sets. On Windows the session cap covers every statement
  on the app's pool except one that sets the cap itself, so that is the one way a new read escapes the list above.

Mutants, each failing those tests: the Windows session default taken out (both fail); `letter_planning(true)` or
`matched_planning(true)` asking for the other platforms' three (the first fails). And the Docs search's test
(`docs_ipc.rs`, `one_character_is_answered_by_the_pages_names_counted_and_in_the_space`) fails with the names not
asked, with the total cut to the rows held, with the space not asked, and with no fall back to the bodies.

The windowless backend (`test-util` builds only) plans every read as on Windows with `KNOBAS_PLAN_AS_WINDOWS=1`
(`knobas_db::embedded::plans_as_windows`), so the page tour and the keys can be run on a Mac with the server's own
count of workers started as the answer.

## What it costs

- **The Docs search's one character answers differently**: by names, then the newest, where it ranked bodies by
  how many words begin with the letter. A page whose body alone holds such a word is no longer one of its hits
  while some page's name holds one. Two characters and more are ranked as before.
- **The fill-time letter on Windows is slower for broad letters** (169 ms for `r` against 95), for seconds once per
  upgrade, and faster for rare ones.
- **A word with an author on Windows** loses nothing measured (faster without).

## What would reverse it

A machine where a worker's start is cheap on Windows (a future PostgreSQL that keeps workers), or a read a person
waits on that the PC measures faster with workers than without: the check names the statement, and the closed list
is where it would be let through.
