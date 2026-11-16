# 2026-11-16: A word many rows hold is answered by its names, and the first start writes nothing

**Asked (Björn, on PR #2479, through the orchestrator):** the two things the two-letters pass left for a ruling
(`2026-11-16-two-letters-by-names-and-keys-wait-for-their-answer.md`, *Left*). Four letters and more, and a finished
word, that half the mirror holds were as slow as before: `payo` 55 ms, `retry` 66 and `payout` 61 ms of backend on
the Mac, 80 to 130 on his PC, against key targets of 100, 100 and 200 ms from a key to its rows at CPU 1x, 4x and 6x.
And migration `0094` held the first start after the upgrade for 3.4 s on the Mac, 5 to 6 s on his PC. His standing
rule: directness, performance and responsiveness over data scarcity. Decided by Fable under his delegation, and
carried out in the same pass.

Two decisions, in two parts.

# Part one: a word many rows hold

## What was there

A word was ranked over every row that holds it, to keep ten a kind. `payout` holds 18,553 of the 39,770 rows of his
scale and `retry` 22,394, and each match cost 1.8 µs: its row fetched, its rank worked out from how often and where
it says the word, its name compared. That was 34 ms of `payout`'s 54. Another 10 went on parsing and planning a
statement of 23 kB for each key, because how large the match is depends on the word and only a plan made for the
word knew.

And the answer it bought was this, for `payout` under *Everything*: ten tickets, the first a ticket named *Fix
refund service pagination tokens* whose body says the word 34 times; ten pages chosen the same way; and **no
repository at all**. Nine repositories hold the word and `tidewater/payout-service` is named by it, but the page's
forty rows were the forty best ranked of every kind, and a repository's description is short. Under *Titles* the
same: `repo: 0 of 2`. Three letters, `pay`, showed the repository. The fourth took it away.

## What a word many rows hold is for

Somebody typing `payout` into ⌘K is on the way to something named payout: the service, its pipeline, the runbook, a
ticket about the scheduler. A word that sits in half of all bodies says nothing by sitting in one more, and counting
how often it sits there is the same noise the one-letter ruling found in counting words that begin with `p`.

A word few rows hold is different. `iban` is in 548 rows, and the page that says it most is probably the page about
it. Story 26 (a strong body match beats a weak title match) was written for that, and holds there.

## Decided

1. **A query is counted before it is answered, no further than one past 2,000.** The rows its scope matches, after
   its filters: the names under *Titles*, the text under *Everything*.
2. **Few rows hold it: it is ranked whole, as before.** Up to 2,000 matches, every one is ranked and each kind's ten
   best are its answer, the same rows in the same order. Stories 25 and 26 stand. 2,000 is where ranking every
   match costs what the bounded read costs, 4 to 6 ms.
3. **Many rows hold it: its names come first.** The rows a word of whose title or key is the word, or begins with it
   while it is being typed: what *Titles* matches. They are ranked among themselves as *Titles* ranks them, by the
   whole row, so a name whose body is about the word stands above one that mentions it once.
4. **Under *Everything*, the text follows where the names leave a kind room.** That kind's newest rows whose text
   holds the word, newest first. They are not ranked: their rank is 0, and a body that says the word twenty times
   stands below a name that says it once.
5. **Names that fill a kind are that kind's answer.** Ten tickets named by the word leave the tickets no room, and
   their text is not read.
6. **A word that names more than 2,000 rows too is answered by the newest of them**, under *Titles* and under
   *Everything*: a project's key (`sclpay` names 5,600 tickets), a word in every title of a space (`scale`, 7,970).
   No name is ranked. The newest ticket of a project is a better first row than the one that mentions its own key
   most.
7. **No match is counted past the bound.** A kind's `total` is the rows the answer holds of it, as it is for one to
   three letters. No screen draws the count (#2421).
8. **The page is dealt a row a kind at a time, at every length.** Every kind's first row, then every kind's second,
   until the forty are dealt, where the forty best ranked of all were drawn. A rank compares rows of one kind. Across
   kinds it compares how long their bodies are. So `payout` draws seven tickets, seven repositories, seven pull
   requests, five pipelines, seven pages and seven branches, where it drew ten, none, ten, five, ten and five. Each
   kind's rows are still the first of its own order. This is the one-letter ruling's third point, which gave its
   reason for letters only.
9. **One to three letters are answered as they were.** Where they leave the answer to the text (no name holds them,
   or under *Everything* the names are few), the text is counted and answered by this ruling: `du` and `bec`, which
   the two-letters pass left at 22 and 28 ms.
10. **An exact name is not made a tier.** Between equal ranks a row named exactly what was typed still comes first,
    as before (#611). Above a stronger rank it would overturn story 26 for every word, and where names are read
    newest first (ruling 6, and one to three letters) it would take an index of each row's names that nothing reads
    yet. Not built; *Left* says what it would take.
11. **Untouched:** a query with an author, a parent, a reach, a name glob or a ticket key, which reads the mirror's
    own rows and usually holds few; a browse with no words; the Docs search, which reads pages alone.

What moves: nothing. One answer is drawn. The count is made before anything of it is drawn.

## What was weighed

- **Names first for every word, ranking none.** One rule at every length, and it gives story 26 up for `iban` too,
  where the ranking is cheap and says something. Not taken.
- **The same ranking made cheaper.** The floor is the row and its rank: 1.8 µs a match on one process, and on his PC
  a second process costs 31 to 37 ms to start. Halving it leaves `retry` at 30 ms and growing with the mirror.
- **A stored rank for each word of each row.** The rank of one word is a sum over its places in the row, so it could
  be stored: one row a word a row, five to ten million on his scale, rewritten with every edit of a body. And a
  query of two words is ranked by how close they stand, which no stored row holds.
- **The ranking over each kind's newest few hundred matches.** Close to today's answer where a kind holds few, and
  elsewhere a row is in or out by its date, in a way nobody could tell from the page. The names' rule is one a
  person can say.
- **Every match read for its kind and date, none ranked.** A third of the cost, 0.6 µs a match, and it still grows
  with the word: 13 ms for `retry`. It is what the answer falls back on for a kind the word is rare in, and how a
  word that names thousands is read.
- **An index of the search copy by kind and date**, for the walk. It would end the heap-only update of a date that
  moves (`0084` set the table up for it), and every moved date would write both word indexes again. The names'
  table already holds each row's kind and date in an index, so it got the row.
- **The walk of the mirror's own index** (`item_kind_updated_idx`). Its rows sit beside their payloads: 14 µs a row
  read cold, 387 ms for a kind the word is not in.

## How

**The count (`knobas_search::sql::probe_sql`).** One row: the matches, and the names where the matches are past the
bound. Each is `select count(*) from (… limit 2001)`: the index lists every place of the word whatever the limit
(2.6 ms for `payout`'s 19,146), and the limit bounds the rows fetched to be counted. The names are counted by their
own index alone, so a name whose body holds a word the query leaves out (`retry -queue`) is counted and never
answered: the count only chooses how names are read.

**Few (`kept_search_sql`).** The statement that always ran, with one change of text: `q`, the typed words, is
materialized.

**Many (`broad_sql`).** Three reads of the mirror in place of the one:

- `names`: every row the query names, off the index of the names (`item_search_fts_a_idx`), ranked. Where the word
  names more than 2,000 rows, the same rows with their kind and date and no rank.
- `walk`: for each kind the names do not fill, its rows newest first off `sync.item_initial`'s index at the empty
  start (below), each one's search copy fetched by its key and tested for the word, until ten are found. The match is
  spelled so that no index answers it (`coalesce(sx.fts @@ q.tsq, false)`): the walk is the read. For `payout` that
  is the repositories alone: 65 rows walked for the seven only whose text holds it.
- `late`: a kind the walk did not fill within 500 rows is asked of the word's index instead, which lists every row
  of the word once and keeps that kind's newest ten. So a kind the word is rare in is answered whole, and the walk
  of it costs 0.7 ms at most. No such kind, no read. The budget changes no answer, which a test holds at 1, 3 and
  500.

The stages after are the words' statement's own: the cut by kind, the page, its titles, path lines, state chips and
excerpts. Knobas' own corpora (notes, assets, routes, views, sets, lists) hold tens of rows and are ranked as before.

**One plan a connection (`fetch_kept`).** The three statements do work their own limits bound, so one plan serves
every word: `plan_cache_mode = force_generic_plan`, the statement prepared by name. Each read is spelled for the way
it must be made, an index's own predicate where the index is the read and a wrapped one where the walk is, so no
estimate chooses. A plan made without the words would parse them again for every row and every use if `q` were
inlined (6 µs a row, measured), which is why it is materialized.

**A connection is kept for the life of the app** (`knobas_db::embedded`, the app's pool). A plan is made once on
each of the pool's five connections, 10 to 30 ms a statement, and lives as long as the connection does. sqlx closes a
connection after 30 minutes, and after 10 idle, by default, which is the rule for a server across a network. So the
plans would have been made again at the next key five times every half hour: a letter 13 to 15 ms where the kept
read is 3, a broad word 30 to 50 where it is 12. The app's pool now sets no age and no idle time. A connection that
broke is still found and replaced when it is next asked for. What is left is the first use after the app starts:
the first five letters and the first five long words typed are that much slower, once (the words no slower than
they were before this pass).

**The same for one to three letters, where it was only meant.** `fetch_named` set `force_generic_plan` since the
one-letter pass and sent its statement unnamed, so the server parsed and planned it again for every key: 1.5 ms and
5 to 7 ms of a read that runs in 2. It is prepared by name now. One letter: 8 ms to 3.

**Migration `0095`: the empty start.** `sync.item_initial` takes one row more an item, under no characters at all
(`initial = ''`): every name begins with the empty string. So the index the names are read off
(`item_initial_newest_idx`) is, at `initial = ''`, each kind's rows newest first, one narrow entry a row. The same
two triggers write it, through `sync.name_prefixes`. No index was added to the search copy.

**The withdrawn entities** are read through a new index of the 291 that are (`entity_withdrawn_idx`), where every
ranked search scanned all 40,061 to find them: 1.6 ms to 0.02, on a read that is now 10.

## What it costs

- **Disk.** 39,770 rows more on his scale, one an item beside the 20 there were, about 9 MB with their index
  entries. And the table filled a batch at a time is 214 MB where the one pass left 181 (part two).
- **The sync.** A write of a mirror row whose name or date changed writes 21 narrow rows where it wrote 20.
  It was not dearer where it was measured. A date moved on 1,000 tickets, median of six on scale clones, the two
  builds alternated: 325 ms of the statement's 1,517 in the trigger before, 199 of 1,379 after. The fill writes into
  the index a batch at a time and leaves its pages room; the one pass packed them, so every rewritten row split one.
- **A query on the line.** One that 2,001 rows hold is answered names first where one that 2,000 hold is ranked
  whole. The line is in rows, not a share of the mirror, because the cost it bounds is in rows.
- **A word that names thousands** reads every one of its names for kind and date, 0.7 µs each: `scale` takes 24 ms
  under *Everything* and 16 under *Titles* where it took 27 and 26. It is the one read here that still grows with
  the word.
- **A count before every answer.** 1 ms for a word few rows hold, 3 to 5 for one many do.
- **The first use after a start.** Each statement's plan is made once on each connection: the first five letters
  typed after the app starts take 13 to 15 ms of backend where the head before took 8 and the kept read takes 3, and
  the first five long words 30 to 50, as before this pass.
- **Connections that stay.** The app's five are no longer closed at 30 minutes or after 10 idle. Each holds the
  launcher's prepared statements, a few megabytes of the server's memory in all.
- **The contract.** `SearchResponse.total` and `ResultGroup.total` are the rows the answer holds for a query more
  than 2,000 rows hold. The generated mirror's two doc comments say so. And a page of forty holds other rows than it
  did where more than forty were ranked across more than four kinds.

# Part two: the first start after the upgrade

## Decided

1. **`0093` and `0094` write no row.** Each made `sync.item_initial` and filled it in one pass before the window
   opened, 2.6 and 3.4 s on his scale. They make the table, its key, its index and its triggers, and nothing else.
2. **Bring-up fills it behind the app** (`knobas_core::item_initial::fill`), 150 items a batch, each batch its own
   transaction, after the other fills. A batch takes its items `for share skip locked`, as `0091`'s fills do, so it
   never waits on a sync's run, and a run that changes an item waits for the batch that holds it and writes over
   what the batch wrote.
3. **Until it has passed over every item, the launcher answers as it did before `0093`.** A word's first characters
   and a word many rows hold are ranked over every match: the same answers that build gave, a longer wait, for the
   8.6 s the fill takes on his scale, after the 10 s of the fills before it.
4. **The table says when it is whole** (`sync.item_initial_whole`, one row). The migration marks an empty mirror at
   once, since every item it comes to hold is written by the triggers, so a new install never waits. The fill marks
   a mirror that held items, after its last batch, once no item is without its row under the empty start. That row
   is also what tells the fill an item is done.
5. **Both files changed in place.** No release carried them. A profile the draft was run on recorded their first
   checksums, and the runner accepts those two and rewrites them (`knobas_db::migrate`, `SUPERSEDED`), as it accepts
   a file's other line endings. What the first files left, the table filled for three lengths, the fill reads as it
   reads an empty table: no item has its row under the empty start, so every item is written.

## What was weighed

- **Leave the files and add a faster pass in `0095`.** The time is spent inside `0093` and `0094`: a database that
  has not run them runs them first, whatever follows.
- **Can the fill be split?** It can, and a measurement says what it costs: written into its key and index a batch at
  a time the table takes 8.6 s where the two passes took 4.8 with the indexes built after, and it is 214 MB where
  they left 181 (the index 86 MB for 62). Behind the app nobody waits for the time, and the room in the index is
  what the next writes use.
- **Write only the missing row for an item `0094` had filled.** Half the fill's work for a profile of the draft, and
  a second path through the function to hold to the first. The draft's profiles are few.

# Part three: parallel workers

**Asked (Björn, during the pass):** on his PC a statement that runs on several processes pays for each one's start.
Can three workers be kept open? And if workers make a search faster, keep them.

## What a worker is

A process PostgreSQL starts for one query and ends with it. Seen on this server (18.6): three workers alive during
a parallel query, none after it, three new process ids for the same query run again. `max_parallel_workers` caps how
many run at once; nothing keeps one. On the Mac a start is about 2 ms. On his PC the Windows pass (2026-11-14)
measured 31 to 37 ms.

What can be kept is a connection, and the app's five now are (part one, *How*). Work could be spread over them by
the app itself, three statements at once on three connections, their rows merged in Rust, with no process started on
any platform. Not built: after this pass no read the key targets are held to needs it.

## Which statements start workers, on the scale mirror

Found by the server's own count (`pg_stat_database.parallel_workers_launched`: 0 on the saved home, 687 after the
open budget's tour of every page) and by sampling which statement the workers belonged to while each recorded
command was asked once.

| Statement | Workers | On the Mac, with / without | On his PC |
|---|---|---|---|
| The Tickets views' counts (`ticket_views`), on every opening of Tickets | 2, by the server's own rule | 33 / 44 ms, measured | about 115 / 70 to 90, **estimated** |
| One letter read whole: a single character in the Docs search, and the launcher while the names' table fills | 3, always (`LETTER_PLANNING`) | 73 to 90 / 133 to 139 ms at a load of 25, measured | `r` 94 / 157, `x` 46 / 7, measured by the Windows pass on the launcher's read as it was |
| A word ranked whole: a launcher query with an author, a parent, a reach, a name glob or a ticket key, and the Docs search's words | up to 3, seldom started (`MATCHED_PLANNING`) | 52 / 60, 60 / 57 and 10 / 10 ms for `payout`, `retry` and `iban`, measured: no difference | 31 to 37 ms more for each worker it does start, **estimated** |
| The launcher's letters, its count, a word few rows hold, a word many rows hold | none, on every platform | 3 to 17 ms | 5 to 30, **estimated** |

Of the 687 workers of the tour, 678 were the Tickets counts' (339 openings, two each). No other page's read started
one.

The estimates are the Mac's time at 1.5 to 2 times, plus 31 to 37 ms for each worker started. **Nothing in this pass
ran on Windows.**

## Decided

1. **On Windows the app's connections plan no worker unless a statement asks for some**
   (`knobas_db::embedded::app_session_settings`). The Tickets counts gain 11 ms from their two workers on the Mac
   and would pay 62 to 74 ms for them on his PC. **Not measured there.** It is one commit, and dropping it is one
   line.
2. **On macOS and Linux the server's rule stands.** Workers are worth their start there, and the Mac's numbers say
   so.
3. **The one-letter read of the whole table keeps its three**, on every platform: the Windows pass measured it
   faster with them for a broad letter and slower for a rare one, and ruled then. What would end it is answering
   the Docs search's single character the way the launcher answers one, off the names.
4. **A word ranked whole is left as it is.** Workers made no difference to it on the Mac, so asking for none would
   cost nothing here and could only save a start on Windows. Not changed without a number from the PC.
5. **The statements the key targets are held to plan none**, and a test reads each one's plan with a worker made to
   look free for any scan of any size (`long_words.rs`,
   `no_statement_a_key_is_held_to_plans_a_parallel_worker`).

## What would settle it

Three timings on his PC, against the scale data: `ticket_views` with the server's rule and with no worker; `search`
for `payout`, `retry` and one letter; a single character in the Docs search.

# Before and after

On the Mac (M2 Pro, on AC power), release builds of the windowless backend on fresh APFS clones of the saved scale
home, the head before (`98892a5e`) against this one, the two alternated. Other agents' runs were on the machine, at a load average of 5 to 9.

The backend's own time on the read (`took_ms`), median of nine, under *Everything*:

| Read | Before | After |
|---|---|---|
| `p` / `pa` / `pay` | 8 / 8 / 8 ms | 3 / 3 / 3 |
| `payo` / `retr` / `conf` / `depl` | 49 / 56 / 49 / 51 | 12 / 15 / 17 / 14 |
| `payou` / `retry` / `confi` | 52 / 57 / 49 | 13 / 15 / 15 |
| `payout` / `config` / `deploy` / `ledger` | 51 / 49 / 50 / 52 | 12 / 15 / 13 / 13 |
| `payout ` / `retry ` (finished, a space after) | 46 / 53 | 10 / 13 |
| `payout retry` / `retry -queue` / `retry payout config` | 62 / 30 / 67 | 11 / 13 / 22 |
| `iban` / `backoff` / `tidewater` / `PAY-231` (few rows hold them: ranked whole) | 13 / 16 / 10 / 9 | 6 / 11 / 3 / 2 |
| `du` / `bec` (no name: the text) | 18 / 26 | 6 / 12 |
| `ze` (nothing anywhere) | 13 | 2 |
| `sclpay` / `scale` (they name 5,600 and 7,970 rows) | 23 / 27 | 17 / 24 |

Under *Titles*: `payout` 11 ms to 5, `retry` 15 to 8, `conf` 12 to 6, `service` 13 to 7, `ledger` 11 to 5, `iban` 9
to 3, `sclpay` 18 to 9, `scale` 26 to 16.

Of `payout`'s 12 ms: the count 4.5 (the word's 19,146 places listed 2.6, 2,001 rows fetched 0.8, the names counted
1), the answer 4.8 (844 names ranked 2, seven kinds walked 0.5, the page's rows drawn 1.5), and the rest the
vocabulary and the round trips. One to three letters went from 8 ms to 3 because their plan is kept now.

From the key to its rows on screen, real input in headless Chrome (`keys.mjs`), median of seven presses, three
rounds, the same built page on the head before's backend and on this one's. The fourth letter is typed into a box
that holds three with their answer drawn, the fifth into one that holds four, and the last letter of a word into
one that holds the rest. No row moved in any press, and none drew nothing:

| Key | CPU | Before | After | Björn's line |
|---|---|---|---|---|
| the fourth letter | 1x | 75 / 84 ms | 49 / 47 | 100 |
| | 4x | 98 / 100 | 57 / 56 | 100 |
| | 6x | 119 / 116 | 78 / 76 | 200 |
| the fifth letter | 1x | 76 / 86 | 45 / 49 | 100 |
| | 4x | 83 / 91 | 52 / 53 | 100 |
| | 6x | 99 / 101 | 68 / 68 | 200 |
| a word's last letter | 1x | 72 / 77 | 36 / 40 | 100 |
| | 4x | 93 / 85 | 55 / 56 | 100 |
| | 6x | 103 / 100 | 69 / 60 | 200 |

Two rounds, at a load average of 2 to 6. On the head before, one press in seven of the fifth letter and of the last
drew nothing new, because its ranking gave the same forty rows a letter earlier, so those medians are of six.

The aim was half his line on the Mac. It holds at 1x and at 6x. At 4x the Mac is at 52 to 57 ms, under his line and
a little over half of it, as the letters are: the answer is in after 23 to 26 ms and the rest is the page drawing
its rows.

**The first start after the upgrade**, the same clone of the saved home (at `0085`) brought up by each build, from
the process starting to the database ready:

| | Before | After |
|---|---|---|
| Three starts, the two builds alternated | 5.03 / 5.09 / 5.20 s | 0.30 / 0.27 / 0.30 s |

`0088` to `0092` are in both. The fill then runs behind the app, with the launcher answering as before `0093`
meanwhile.

**Expected on his PC:** about 40, 70 and 150 ms at 1x, 4x and 6x for the fourth letter and later ones. The read is 12 to
17 ms on the Mac and so 20 to 30 there, where it was 80 to 130, over his line at 1x and 4x before the page drew a
row. The page's own share there was 11, 45 and 139 ms for one letter (the ux13 pass), and these keys draw into a
list that is already open. One to three letters should come out about 10 ms under what the passes before expected
(30, 60 and 140), since their read went from 8 ms to 3 here. Not measured there: the brief kept this pass off the
PC, and the 6x figure is the least certain, as it was for the letters.

`testenv/perf/budget-keys.json` holds `everything-four-letters`, `everything-five-letters` and `everything-word` at
the letters' lines, 70, 100 and 130 ms.

# Same answers

- `tests/it/long_words.rs` (knobas-search), ten tests. The decided answer on rows written for it: names ranked as
  *Titles* ranks them, then the newest text at rank 0, a loud body after a quiet name, the bound held to the row.
  Names that fill a kind leaving the text out of it. A word that names many rows answered by the newest of them. A
  kind the word is rare in answered whole at a walk of 1, 2, 4, 6, 8 and 500 rows. A withdrawn row, a source
  switched off, a hidden kind, a kind asked for and a window of days, in each of the three reads. The count, no
  further than one past the bound. **The bounded answer held to the unbounded reads it stands for**, over fourteen
  queries of the scale seed (a word being typed, a finished one, two words, a phrase, a word left out, a key) at
  every bound that changes the answer and three budgets of the walk: the ranking of every match within the bound,
  and past it the ranked or newest names and every text match by date, read with no bound at all. The page dealt a
  row a kind at a time, within the bound and past it. The plan: the kinds and the walk off the index and never
  sorted, the names through theirs, the index's read for a cut kind not run where none was cut. And a mirror not
  marked whole answered by the ranking alone, then filled, marked and answered by the bounded reads again.
- `sql.rs`'s own test holds the three statements to the builder's two rules, the typed words a bind and the
  literals the closed list, each one text whatever was typed, `q` materialized, each read spelled for its way.
- `tests/schema.rs` (knobas-db): the fill over three installs (no row, `0093`'s single characters, `0094`'s three
  lengths), an item that has its rows left as it is, the mark refused while an item has none; an item a sync holds
  stepped past; a profile that recorded the first checksums of `0093` and `0094` opening under this build, and the
  same checksum under another version refused.
- The 217 tests the crate had still pass, every word in them now counted first and ranked under the kept plan:
  `one_letter.rs` and `short_words.rs` hold a word's first characters, `ranking.rs` the ranking's witness.
- On the scale clones, with each kind's ten rows drawn, 38 queries under *Everything* and 43 under *Titles* that
  2,000 rows or fewer hold answer byte for byte as the head before.
- Twenty-six broken versions each fail a test. Nineteen of the search: the bound taken one short; the names never
  read newest, and always; the text's rows given a rank; the walk taking a name for text; a kind the names fill
  walked; the walk's last row not told; the index's read taking a name for text, and reading every kind; a cut
  kind's walk kept beside the index's rows; a withdrawn row walked; a kind's total left the names' count; the page
  not dealt; the count counting every match; the names counted for a query few rows hold; the typed words inlined
  under a kept plan; the newest names ranked; the names read, and the bounded reads run, before the table is whole.
  Seven of the stored rows: the fill writing an item that has its rows; no row under the empty start; the table
  marked whatever it holds; the fill writing an item a sync holds; `0094`'s first checksum refused; a superseded
  checksum taken for any version; an empty mirror not marked whole.

# Left, with what it would take

- **A word that names thousands of rows reads each of its names**, 0.7 µs a row: a project's key, a word a whole
  space is titled by. It is bounded by the names and no longer by the text, and it is the read here that still grows
  with the word. What would bound it is the names' stored order at every length of prefix, which is the table the
  two-letters pass weighed and left (one row a word, read by range).
- **The count reads the word's whole list of places**, 0.13 µs each, before it fetches its 2,001 rows: 2.6 ms for
  `payout`. PostgreSQL's word index gives a list whole or not at all.
- **Three broad words together** (`retry payout config`, 13,236 rows) take 22 ms: the count intersects three
  lists.
- **An exact name is first only between equal ranks.** Where names are read newest first, a row named exactly what
  was typed is among them by its date. It would take an index of `sync.item_search`'s three name columns (three hash
  indexes, about 1 MB each) and one more read in each statement; the stored order of one to three letters would take
  the same read.
- **A pipeline found by its run log** is a row the text found, after the names and by date, where it was ranked.
- **The Docs search** ranks every page that holds the word, as before. It reads 5,032 pages, and a single
  character typed into it is the one launcher-side read that still asks for three workers (part three).
- **A query with an author, a parent, a reach, a name glob or a ticket key** is planned for each key and ranked
  whole, as before.

# Gates

In a clean clone, on the pushed head `1354ba57` unless said otherwise.

- `cargo fmt --all --check`, `just clippy` and `just clippy-libs`: clean.
- `just wasm-fixture`, then `env -u RUSTUP_TOOLCHAIN just test`: 61 binaries and 19 doc-test crates, 4,973 passed, 0
  failed, 263 ignored, and `app/src/lib/ipc/generated/` unchanged after it. Run on `68561d1c`, the same Rust tree:
  the rebase onto the row-swap pass's four commits changed no file under `crates/`. Two earlier runs of the day each
  failed one plan test of knobas-search under the gate's load and nothing else:
  `item_search::a_search_with_words_reads_through_the_narrow_tables_indexes` once (it plans against whatever its
  module's database holds at that moment, and passed alone, with its module and in every later run), and
  `coverage_plans::a_probe_never_reads_a_table_to_prove_a_source_has_nothing` once, which the brief lists as flaky.
- `just check-front`: ok. vitest whole: 338 files, 5,190 passed, 1 skipped. `svelte-check --threshold error`: 1,473
  files, 0 errors, 0 warnings. The press trips with `--strict`: 272 of 272.
- `just check-scripts`: ok.
- `just open-budget --real`, on `e55d18b1`: every open within its line at 1440x868 and at 1920x1080@2 (three
  throttles, four visits a place). At 2560x1440@1.25 one cell of the tour's 525 was over, *Builds* after a sync at 6x,
  drawn in 331 ms with a longest task of 162, while another agent's build held the load at 13; the same cell drew in
  113 and 127 ms at the other two sizes. The cell was not run again: the machine stayed loaded (below).
- `just keys-budget --real`: **not held on the pushed head.** From the afternoon on, other agents' test runs, builds
  and browsers kept the machine at a load of 14 to 44, and a throttled press is the measure most open to that. The
  launcher's six cases were pressed all the same, at a load of 37 to 44: all 126 presses drew their rows and none
  moved, and every cell read two to three times its quiet figure, the three letter cases no slower read touched
  among them (one letter 50, 111 and 189 ms where it holds 37, 55 and 74). The measure that stands is the one in
  *Before and after*: both builds alternated at a load of 2 to 6, every new case under its line. To hold the lines
  on a quiet machine: `KNOBAS_PERF_PORT=5373 just keys-budget --real <port>` (another run may hold the default
  port). An earlier whole run, on a backend just started and before the probe asked each statement first, read the
  fourth letter at 70.5 ms against 70 at 1x, its first presses 136, 148 and 98 ms: each of the five connections
  making its plans. That is why the probe now asks them first and why the connections are kept.

# Reversing it

Part one: `Searcher::rows_by_words` answering through `sql::search_sql` whatever the count is the ranking of every
match, and `sql::build_search_sql`'s `picked` ordered by rank alone is the page as it was. `0095` stays; its rows
are then written and never read. Part two: nothing to reverse without a release that carried the first files.
