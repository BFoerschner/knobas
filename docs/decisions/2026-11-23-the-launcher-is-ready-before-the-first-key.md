# 2026-11-23: The launcher is ready before the first key

**Found by** the ux30 end gate's key probe (`testenv/perf/keys.mjs`, `everything-four-letters`: the fourth letter of a
word half the scale mirror holds, typed under the launcher's *Everything*) on the Mac at CPU 1x: 80.7 ms at 1920x1080@2
and 81 ms at 2560x1440@1.25 against the 70 ms line, 72.9 ms with the launcher's flattening fix (7214668b) in the front
end. The answer was most of it: 54 to 65 ms from the key to the answer. ux28's gate had it over too (76.1 ms at 1440).
Every other launcher key was within its line, and so were four letters at 4x and 6x.

## Why the fourth letter

A word more than 2,000 rows hold is counted before it is answered
(`2026-11-16-a-word-many-rows-hold.md`): 2,001 of its matches and every row it names, each off its own page of the
search copies (`sync.item_search`). One to three letters are answered by names and never count. So the fourth letter
is the first key of a word that reads those pages, and the probe's seven words (`payo`, `retr`, `conf`, `depl`, `ledg`,
`batc`, `stag`) each land on pages no key before them read. The fifth letters read the same pages a second time.

What the backend spent on the key, timed by phase in the windowless backend on a fresh copy of the scale home:

- **The count read its pages from disk.** `conf` read 2,525 pages of `sync.item_search` for its count: 122 ms on a copy
  nothing had read yet, 30 ms where only the system's file cache held them (any restart of the app), 5.6 ms from
  PostgreSQL's buffers. In the probe the count took 31 to 47 ms of `conf`'s and `batc`'s backend time, and 10 to 15 once warm.
  The gate copies the home afresh for every run, so its every four-letter word met cold pages, as a person's first
  broad words after a start do.
- **Autovacuum's analyze dropped the kept plans.** The launcher's statements are planned once a connection and kept.
  About a minute after a start, autovacuum analyzed the eleven tables the session before had written past its
  threshold, `sync.item_initial` among them, and an analyze throws away every plan that reads the table: the next
  key on each of the five connections planned the walk and the count again, 9 to 11 ms a key. On the gate's base the
  analyze landed in the probe's run.

The front end's share was 13 to 20 ms, as for the other launcher keys.

## What changed

Bring-up warms the launcher's reads behind the app (`knobas_search::warm::warm`, called from `bring_up_app` right after
the migrations, on a connection of its own so it holds none of the pool's five):

1. **The analyze autovacuum is owed is taken at once**: every table written past autovacuum's own threshold
   (`autovacuum_analyze_threshold` plus `autovacuum_analyze_scale_factor` of its rows, the counters autovacuum reads)
   is analyzed, so the plans the first keys make are not dropped under a later key. A table below the threshold is left
   to autovacuum.
2. **The search copies are read into PostgreSQL's buffers** with `pg_prewarm`: the names' and the words' indexes, the
   copies' key, the copies, then the rows each answer's page joins (`knobas.entity`, `sync.item_row` and their keys),
   in that order, while what is read stays within half of `shared_buffers`; a relation that does not fit is left out.
   `sync.item`, which holds every payload, is not read whole.

On the scale copy the eleven owed tables took 172 ms to analyze and the 112 MB read 93 ms from a cold disk, behind the
window. `pg_prewarm` is created by the warm (`create extension if not exists`), not by a migration: it ships with
PostgreSQL's contrib modules, as `pg_trgm` does, and a server without it warms nothing and starts as before.

## What it costs

- 112 MB of the 256 MB of buffers, filled at start with what the launcher reads; on a larger mirror less is read, never
  more than half.
- An analyze at start that autovacuum would have run a minute later, of the same tables.
- The first broad word typed in the first quarter second after the window opens still meets what it met before.

## Proof

- `knobas-search/tests/it/warm.rs`. `a_broad_words_count_reads_no_page_after_the_warm`: the count of a broad word with
  the search copies evicted from the buffers (`pg_buffercache_evict_relation`) reads pages; after the warm it reads
  none, and every relation was read whole, in order. `the_analyze_autovacuum_is_owed_is_taken_at_once`: a table
  written past the threshold is analyzed and the next warm finds nothing owed; one below it is left. Both failed with
  the warm's two steps stubbed out (pages read; nothing analyzed) and pass.
- On the Mac, headless Chrome, `keys.mjs --only everything-four-letters` on a fresh copy of the gate's base (the scale
  home with the files filled), the head before (85f1508f) and this one alternated, three runs each at each size, the
  median of seven presses per run, a load of 2 to 4:

| Size | Before | After |
|---|---|---|
| 1440x868 | 75.2, 70.4, 66.2 ms | 59.2, 53.4, 54.3 |
| 1920x1080@2 | 63.9, 72.4, 66.3 | 58.5, 53.3, 57.1 |
| 2560x1440@1.25 | 70.9, 65.4, 64.5 | 47, 55.6, 59.3 |

  From the key to the answer: 53 to 60 ms before, 36 to 45 after.
- ux29 (v3.2.9-ux29's own backend and front end on its base) read 63.1 and 71.8 ms at 1920x1080@2 and 69.5 and 67 at
  1440x868: at the line and over it, as ux28 was.

## Reversing it

Remove the spawned warm in `bring_up_app`. The extension stays in a profile that ran it and does nothing unless called.
