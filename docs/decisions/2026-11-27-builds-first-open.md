# 2026-11-27: Builds' first open: what Recent's first read costs, and what changed

**Asked (Björn, #2554).** "Builds › Recent takes a long time to open." Spec #2566 gave ticket #2569 the measurement
and the fix: measure first, change second, and record both here.

**Where.** The saved scale database (`testenv/perf/README.md`, *On a real database*: 39,770 items, 2,620 TeamCity
runs of 91 pipelines), cloned, secrets moved out, on the windowless backend (`headless_ipc`, release). Base
`396467603` (`spec-2566` after #2567). The machine was shared with other agents' gates throughout, so every number
carries its load (`vm.loadavg`, 1/5/15 min, 12 cores). None meets #533's quiet-machine standard.

## Before

### The backend, by part (`recent_runs`, 200 runs, no filter)

Timed inside `recent_runs_inner`; cold is the first call after the backend started, warm the median of the next 7;
three starts. Load 7.5 to 7.8.

| | `declare` | `declared_paths` | `identity_of` | `fillers` | the page (`RECENT_RUNS` + `RECENT_PIPELINES`) | total |
|---|---|---|---|---|---|---|
| cold (median of 3) | 1.28 ms | 1.21 ms | 0.42 ms | 0.68 ms | 24.4 ms | 28.6 ms |
| warm (median of 21) | 0.92 ms | 0.85 ms | 0.28 ms | 0.65 ms | 21.3 ms | 23.8 ms |

Cold is warm plus about 3 ms: the database files sit in the OS's cache, so a restarted PostgreSQL reads no disk.

### The query's plan

`EXPLAIN (ANALYZE, BUFFERS)` of `RECENT_RUNS` with the app's binds, third of three runs:

| limit | planning | execution | how `k` (the page's runs) was read |
|---|---|---|---|
| 60 | 1.7 ms | 1.8 ms | the head of `run_made_idx`, 60 runs probed |
| 200 | 2.2 ms | 17.0 ms | every run (2,620) joined to its pipeline, then a top-200 sort |

At 200 the plan read all 2,620 runs' `item_pipeline` rows (8,324 of 11,502 buffers, 13 ms) before keeping 200, a
cost that grows with the history. The cause is the join to each run's stored row (`run_join!`) through `run_keys`,
a CTE over `jsonb_each` that the planner estimates at 10,000 rows (it holds 16): priced per run, walking the index
looked dearer than sorting everything, though no filter that reads the stored row was set. With filters set
(*Failed*, *Running*, *Queued*, *Passed*) the plan read every run in both shapes: 10 to 23 ms.

`read_plans.rs`' mirror (1,500 runs) showed two more things: on tables never analyzed the live entities are estimated
at a few dozen, and the `exists` on `knobas.entity` was hashed, so the planner believed 12 runs would pass and sorted
them; analyzed, the outer select joined every item to `run_keys` before the page's runs.

### The open budget (`just open-budget --real`, 1440x868@1)

Load 7.2 at the start, 12.9 at the end. `useful` / `drawn` in ms; the probe's `useful` at 4x and 6x reads early
(#2590), so `drawn` is the number to read there.

| place | 1x | 4x | 6x |
|---|---|---|---|
| `builds` | 17 / 34 | 23 / 90 | 24 / 158 |
| `builds-recent` | 37 / 54 | 8 / 108 | 4 / 171 |
| `recent-run` | 51 / 68 | **211** / 228 | 368 / **384** |

Over: `recent-run` at 4x (useful 211 > 200) and 6x (drawn 384 > 300). Its reads are the run page's; they all resolve
together behind one main-thread task, not behind `recent_runs`.

`builds-recent` passed, so per the ticket the case Björn sees was looked for. A restarted backend is no slower (above).
What no place caught is the first open right after the app starts: `builds-recent` waits six seconds and a quiet page
before it presses. **`builds-recent-start`** (new in `testenv/perf/open.mjs`) goes to `#/builds/recent` as soon as the
nav is drawn, with the shell's start-up reads still out. Base, load 5.1 at the start, 19.5 at the end:

| place | 1x | 4x | 6x |
|---|---|---|---|
| `builds-recent-start` | 68 / 85 | 73 / 157 | 315 / **498** |

Over at 6x. Its `recent_runs` was in flight 42 to 325 ms after the press while the backend answered it in 8 to 14 ms
with nothing else in flight there: the time is the page's main thread, busy with the app's start, and the 129 KB
answer it then parses.

## What changed

1. **The query walks the head of `run_made_idx` whatever the limit** (`RECENT_RUNS`, `crates/knobas-core/src/builds.rs`).
   Each run's stored row is read only for a filter that reads it, in an `exists` that folds away when none is set
   (the statement is planned with its values, `.persistent(false)`). Liveness is a probe of the entity, never a join.
   The outer select probes each run's stored row by its key instead of joining `run_keys`. A filtered read builds its
   kept set from `run_made` (the runs), not every item. `read_plans.rs` holds the head of `run_made_idx` at 60 and 200
   in both statistics states, no whole read of `item_run`, and a filtered read with no per-row scan of the items.
2. **The first read is the window's rows** (`RECENT_FIRST = 60`, `app/src/lib/tf/builds/recent.ts`). With nothing held
   for the filter, *Recent* asks for 60 and draws them, then reads the rest of the 200 from that answer's cursor and
   lays it below them, so no drawn row moves (`TfRecent.test.svelte.ts`). A head read again still asks for 200.
3. **The four reads before the query run side by side** (`recent_runs_inner`): they are independent and still made
   per call, so a changed source or declaration is in the next read.

## After

### The query (same moment, same connection, base statement against head statement)

Load 8.8 to 10.0; execution of the third of three plans, each pair taken three times.

| read | base | head |
|---|---|---|
| 200, no filter | 20.7, 22.0, 17.7 ms | 18.3, 3.9, 3.3 ms |
| 140, no filter (the rest of the first page) | 18.9, 29.3, 18.1 ms | 2.8, 3.0, 2.9 ms |
| 60, no filter (the first read) | 2.2, 2.2, 2.6 ms | 1.9, 2.0, 2.6 ms |
| 200 *Failed* | 16.4, 17.1, 19.1 ms | 19.0, 21.0, 17.9 ms |
| 60 *Failed* | 16.8, 17.4, 15.3 ms | 21.3, 18.4, 21.7 ms |
| 200 *Running* | 13.3, 11.5, 11.0 ms | 17.1, 17.4, 17.4 ms |
| 60 *Passed* | 27.7, 25.4, 25.4 ms | 18.2, 16.5, 15.7 ms |
| 200 *Queued* | 10.3, 10.6, 13.8 ms | 16.6, 16.3, 16.8 ms |

A filter still reads every run, in both: the head builds the kept set once and walks the index until the page is full,
which is faster where the result is common (*Passed*) and about 5 ms slower where it is rare (*Running*, *Queued*: the
walk reaches the end of the history, probing each run's liveness). A filtered page is not the first open; #2570's scope
by pipelines is where a narrowed read's plan is checked next.

### The backend, by part

Load 8.7 to 10.9; three starts each.

| | the four reads, side by side | the page | total |
|---|---|---|---|
| 60 runs, cold (median of 3) | 3.6 ms | 18.2 ms | 21.7 ms |
| 60 runs, warm (median of 21) | 1.8 ms | 8.7 ms | 10.5 ms |
| 200 runs, warm (median of 21) | 2.9 ms | 14.3 ms | 17.2 ms |

The first page's warm median is 10.5 ms against 23.8 ms before. The page's time beyond the plan's 2 to 3.4 ms is the
second statement (`RECENT_PIPELINES`), planning each call, decoding and the machine's load: 24 calls in a row at load
14 ranged 8.3 to 24.3 ms, in two clusters around 9 and 18.

### The open budget

Two head runs, `useful` / `drawn` in ms, the first at load 25.7 falling to 9.5, the second at 11.9 rising to 44.1:

| place | 1x | 4x | 6x |
|---|---|---|---|
| `builds` | 26 / 43 · 25 / 59 | 30 / 130 · 19 / 102 | 20 / 187 · 32 / 282 |
| `builds-recent` | 40 / 57 · **164** / 180 | 10 / 160 · 4 / 121 | 41 / **407** · 6 / 222 |
| `builds-recent-start` | 36 / 53 · 57 / 74 | 68 / 235 · 165 / 249 | 243 / **376** · 231 / **448** |
| `recent-run` | 50 / 67 · 52 / 69 | **203** / 219 · 188 / 204 | 287 / **303** · 592 / **608** |

The same build moved by two to three times between the runs (6x `recent-run` drawn 303 and 608 ms, 1x
`builds-recent` useful 40 and 164 ms), and `builds`, which this change doesn't touch, moved with them (6x drawn 158 at
base, 187 and 282 here). These runs can't say whether the lines hold, in either direction. An interleaved run of the
two fronts against one backend (load 14 to 47) was stopped for the same reason: base's own 1x `builds-recent` read
322 ms in it.

## Ruled (implementer, Opus, unattended)

1. **The cost the measurement named is the query's plan** (16 of 24 ms, growing with the history), not the four reads
   (3 ms) or a cold start. The plan is fixed, and the four reads run side by side rather than being cached: caching
   them needs an invalidation for sources, declarations and tone overrides, for 1 to 2 ms.
2. **60 rows first** fill the window at the tallest screen the budget draws (about 50 rows at 1440 px); the rest of
   the 200 follows from the first answer's cursor. A head read again (a sync's news) still asks for 200, so a run
   below the first 60 keeps being read again as before.
3. **The new place presses at once.** A person who starts knobas to look at the runs; pressed later, the place is
   `builds-recent`. Its lines are `builds-recent`'s.
4. **Filtered reads stay a read of every run**, now about 5 ms slower for a rare result and 9 ms faster for a common
   one at 2,620 runs. Making them index-backed needs an index on a run's result (a migration) and is #2570's to weigh
   when it scopes the read by pipelines. Moving the liveness probe into the kept set for a filtered read was tried and
   not judged: the machine's load swung the runs from 18 to 163 ms.

## Not settled here

- Whether `builds-recent`, `builds-recent-start` and `recent-run` sit inside their lines at 6x on a quiet machine:
  every run here was on a machine at 2 to 4 times its cores, and the head's open-budget run shows them over. The
  backend half is measured above; the rest of a 6x first open is the page's main thread at the app's start (the run
  page's reads in `recent-run`, the shell's start-up in `builds-recent-start`), which the next quiet run should split.
- #2590: the probe's `useful` at 4x and 6x reads early.
