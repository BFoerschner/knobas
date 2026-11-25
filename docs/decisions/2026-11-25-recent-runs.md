# 2026-11-25: Builds' *Recent*: the newest runs on top, no folders

**Asked (Björn).** "I want a view in builds that puts the most recent builds at the top, hiding all the folders".

**Before.** Builds drew one pipeline at a time: a rail of every source's project tree (folds for TeamCity's projects
and subprojects), and the selected pipeline's runs on the right. To see what ran last across the estate, Björn opened
pipelines one by one; the rail's glyphs and times said only each pipeline's latest run.

**After.** A switch, *Pipelines / Recent*, is the first line of the rail's top. *Recent* (`#/builds/recent`) has no
rail: the newest runs of every pipeline, newest first, one table, day headings as on a pipeline's runs, each row the
pipeline's run row with one more cell naming its pipeline, its number, and its folders as quiet text.

## Ruled (Fable, xhigh, while Björn is away)

1. **Way in: a switch, an address.** *Pipelines / Recent* heads the rail; in *Recent* the same switch heads the filter
   bar where the rail stood. `recent` is a reserved word under `#/builds/`: every pipeline's id names its source
   (`tc:buildType:…`), so none can be it (`recent.test.ts`).
2. **Kept across restarts**, in the webview's `localStorage` (`knobas.builds.view`), written only by a press on the
   switch. A bare `#/builds` with *Recent* kept takes `#/builds/recent`; a link to a pipeline opens it and changes
   nothing; *Pipelines* goes to the last pipeline.
3. **One run page.** A row opens `#/build/<pipeline>/<run>` as before, rail included. `Esc` and the crumb say *Back to
   Recent* and land on the same rows and scroll, the opened run's number focused (what *Recent* held is kept for the
   session).
4. **The row** is a pipeline's run row (`TfRunRow.svelte`, now shared by both lists) with the number moved into a
   pipeline cell: the name in ink (never under 12 characters), `#number` (never cut), then the folders dim, cut at
   their start so the folder nearest the pipeline stays; with two or more CI sources listing runs, the source's name
   leads the folders. Then branch, who started it, tests, duration, start, and *Mark as read*, *Log*, *Run again…* on
   hover. As the table narrows the tests go first, then who started it. *Run again…* opens the run's page with its
   Run… dialog open: the dialog needs the pipeline's settings, presets and refusals that page reads.
5. **Filters.** The bar's scope (*This job* with a job, *Mine*, *All*), *Result* and, with two or more CI sources,
   *Source* narrow the read itself, so *Failed* answers the newest failures wherever they lie; typed words narrow the
   runs held. No *Branch* dropdown (hundreds across every pipeline; words find a branch). The launcher's `#b` offers
   *Builds › Recent runs* as its first row, and `>recent` finds it.
6. **Data: a read of its own.** `recent_runs(filter, before, limit)`: the newest runs across every pipeline CI's
   fillers draw, newest first by when each started or, not started, was queued, exactly as `runMade` orders every list
   (`2026-11-12-runs-newest-first.md`), a page of 200, older pages of 200 from the mirror, at most 2,000 held, then a
   line pointing at a pipeline's own page. Migration `0103` stores each run's instant (`sync.run_made`, indexed
   `(made desc, entity_id desc)`), written by triggers as a run's stored row (`0088`) is, so a page is the head of one
   index range. `sync.item_pipeline.at` was the wrong order (a TeamCity build's finish). The answer names each page's
   pipelines; *Recent* draws without the 571 KB `pipelines` answer, which Builds reads behind its first draw.
7. **Fresh.** The head is read again when a sync ends, and when a refresh moved a run it holds or a write landed (at
   most once in 5 s, the last always read). Its runs in flight are read at their source 100 ms after it draws, when
   shown again and when the window comes back (`runs-in-flight.ts`, at most once in 5 s). New runs wait behind *Show N
   new runs* while the pointer rests on the list (`HeldOrder`), are lit once as they draw, and take the top as soon
   as the pointer leaves. **One addition:** while *Recent* is shown and the window has focus, each CI source is asked
   for its own run (`sync_now`) once its last ended a minute ago: on show, on return and each minute. A source that
   syncs only by hand, is off, is running or is backing off is never asked. *Recent* is as wide as the source, so the
   source's own run is its unit (ADR-0047's rejection was of a read per configuration).
8. ***Mark all as read*** at the bar's end: unfiltered, every failed run of every pipeline (the rail's act); narrowed,
   the failed runs listed, its tip saying how many are unread.
9. **Keys and menu.** Up and Down step the runs by their numbers, Enter opens one, `m` marks a failure read or
   unread. A run's right-click menu offers *Open pipeline* (everywhere a run is right-clicked). The shell's section
   memory keeps `#/builds/recent` as a place of Builds.

**For Björn to reverse:** runs rather than pipelines; the view kept across restarts; the rail back on a run's page;
the minute's ask of each source while *Recent* is shown.

## Found in the real app

Run on the Mac (the debug app under a scratch `HOME`, its window through `mac-window-guard.sh`, driven through the
page's own events) against a real TeamCity 2026.1.3 with one agent under OrbStack (a compose project of its own, the
server capped at 2 GB and the agent at 1 GB, removed afterwards), with four configurations in nested projects
(*Payments › Payouts › Settlement and reconciliation*, *Operations*):

- **A build started at TeamCity never reached *Recent*.** Its head was read again on the view's own *Sync now*
  (`syncRevision`) alone, not when a scheduled run or the minute's ask ended. It now reads its head when a CI
  source's run that changed something ends (`sync:state`), at most once in 5 s. After: a build queued at TeamCity
  drew at the top 27 s after it was queued, already passed (the minute's ask found it).
- **The minute's ask looked once a minute**, so a source was asked every two. It looks every 15 s.
- **The bar's switch sat on the window's edge and *Mark all as read* was cut at the other**: both stood outside the
  filter bar's padded box. One band holds them now, the mark after the count.
- A source whose builds carry no branch is not a CI filler (a role miss, ADR-0029), so a TeamCity configuration with
  no VCS root draws nothing anywhere in Builds; the check's builds were queued with a branch.
- Held: *Recent* replaces the rail; each row names its pipeline, number and folders (the long name cut, its folders
  gone first); a row opens its run with the rail; `Esc` says *Back to Recent* and lands on the same rows with the
  run's number focused; *Run again…* opens the run's page with its dialog; *Pipelines* goes back to the last pipeline
  and the pipeline's runs draw as before.

## Tested

- Rust: `knobas-app`'s `tests/it/recent_runs.rs` (6): the order held to `runMade` (a long build that finished last
  sorts by its start, a queued one by its queue time), pages that follow their cursor, the filters (results,
  branches, sources, *Mine* with no username), the stored instant following a queued run that starts, a time that
  moves and a delete, `sync.run_instant` reading exactly what `parse_instant` reads, and the statement staying
  unnamed (`the_read_is_planned_with_its_values_each_time`). 14 mutants of the statement, the read and the
  migration, each killed.
- Vitest: `recent.test.ts` (the rules), `TfRecent.test.svelte.ts` (12: the rows, a pipeline's place with two
  sources, a chip asked of the backend and words narrowing what is held, *Mine* and *This job*, *Show 1 new run* under
  the pointer, an older page, the way back, the keys, the minute's ask and who it never asks, a source's run ending),
  `TfBuilds` (the switch, no rail, the list read behind the first page, the kept view), the launcher's `#b` row and
  `>recent`, the run menu's *Open pipeline*. 25 mutants of the rules and the view, each killed (one survived first:
  its test started on an address the test before it had left).
- UI checks (`long-paths.mjs` and `trips.mjs --only builds --strict`, headless Chrome on the Mac): `long-paths` had
  zero misses across `build-recent`, `build-pipeline`, `build-run` and `build-run-unfolded` at `1100x600@1`,
  `1440x868@1`, `2048x1152@1.25` and `5120x1440@1`; all 24 Builds trips passed, including `builds-recent-rows-still`
  (first, return, sync, shown with *Show N new runs*, and launcher) and `builds-recent-open-key`.

## Timing

Headless Chrome on the Mac (WSL is down), the release windowless backend over a fresh copy of the saved scale database
(2,620 runs, 91 pipelines; migration `0103` filled `sync.run_made` at its first start), the front end built as the
probes build it. ms to the first useful frame / drawn, each on its first visit (`open.mjs`, `keys.mjs`):

| Case | 1440x868@1, 1x | 1440x868@1, 6x | 2560x1440@1.25, 6x | Line (1x / 6x) |
|---|---|---|---|---|
| *Recent*, first open | 48 / 64 | 11 / 211 | 170 / 287 | 100 / 300 |
| *Recent*, return | 13 / 29 | 12 / 78 | 10 / 110 | 100 / 300 |
| A run opened from *Recent*, first | 42 / 59 | 236 / 269 (alone; 283 / 299 beside the other cases) | 215 / 249 | 100 / 300 |
| A run opened from *Recent*, return | 45 / 61 | 194 / 210 | 192 / 209 | 100 / 300 |
| A letter in *Recent*'s filter (median of 5) | 18 | 34 | 38 | 100 / 150 |

Nothing moved after its first useful frame in any of them. The backend answered `recent_runs` (a page of 200,
129 KB) in 23 to 42 ms, median 27, over fifteen reads. **Found:** two of the first fifteen reads took 3.2 s each,
the rest 23 to 38 ms: from a connection's sixth run PostgreSQL kept a generic plan for the named statement, which
cannot read its `$n is null or …` filters. The statement is unnamed now (`.persistent(false)`, as `knobas-search`'s
suggestions are), and `the_read_is_planned_with_its_values_each_time` holds it. **Found too:** from a run opened in
*Recent*, a second press of the Builds entry went to the run's pipeline; while *Recent* is the kept view it goes to
*Recent* (`TfShell.svelte`).

**Nothing moves** (`moves.sh`, the fixture and the scale database, 1x and 6x, 2560x1440@1.25) found two, both fixed:
*Recent*'s header grew 5 px when its count arrived (the count now holds its room from the first frame, unseen until
read), and a run opened from *Recent* mounted the rail at its top and scrolled it to the pipeline a frame later, 43
rows moving 339 px (a rail mounted after *Recent* holds its rows unseen in their room until that scroll lands, the
probe's rule for a held room). After: nothing moved on *Recent*, a run opened from it, Builds, a pipeline or a run.

**Owed on Windows** (WSL and the PC are off): WebView2's first open of *Recent* at 6x, its keys, and its scroll.

## Reversing it

Migration `0103` stays (released migrations are frozen); drop its reads: `recent_runs` (`knobas-core::builds`
`recent_runs_page`, the app command), `TfRecent.svelte`, `recent.ts`, `builds-view.svelte.ts`, the switch and the
`recent` branch in `TfBuilds.svelte`, the `isRecent` lines in `TfShell.svelte`, *Recent runs* in the launcher
(`actions.ts`, `rows.ts`), `run-again.ts` and its line in `TfRun.svelte`. `TfRunRow.svelte` can stay: the pipeline's
runs draw through it unchanged.
