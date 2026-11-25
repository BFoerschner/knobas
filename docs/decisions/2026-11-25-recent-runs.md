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

## Reversing it

Migration `0103` stays (released migrations are frozen); drop its reads: `recent_runs` (`knobas-core::builds`
`recent_runs_page`, the app command), `TfRecent.svelte`, `recent.ts`, `builds-view.svelte.ts`, the switch and the
`recent` branch in `TfBuilds.svelte`, the `isRecent` lines in `TfShell.svelte`, *Recent runs* in the launcher
(`actions.ts`, `rows.ts`), `run-again.ts` and its line in `TfRun.svelte`. `TfRunRow.svelte` can stay: the pipeline's
runs draw through it unchanged.
