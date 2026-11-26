# 2026-11-25: A run started in knobas moves on at once

**Found (Björn).** "Team city isn't syncing a started run immediately after it's started. Similar problem to the pull
request problem."

**Cause.** A run started from knobas (*Run*, *Run on <branch>*, *Run…*, *Re-run*, *Run again…*) drew a *queued* row on
the pipeline page from the queue's answer (the echo, #1449), and its read-back (ADR-0031) put the build into the mirror
as TeamCity queued it. After that nothing read the build again until the source's next scheduled run: 300 s on the live
sources. The run after the write caught it only if TeamCity's agent had started it by then. So the pipeline page, the
run page's header and the Builds rail said *queued* for up to five minutes while the agent ran it, then *running* for up
to five minutes after it had finished. The rail drew nothing of the run at all until the run after the write ended: it
read its list only when a run of a source ended, and drew no echo.

| Where | Before | After |
|---|---|---|
| Pipeline page, the run's row | *Queued* at the queue's answer; *running* at the next scheduled run (≤ 300 s) | *Queued* at the queue's answer; *running* within one read of the agent taking it (2 s for the first 30 s, then 5 s), *finished* the same |
| Run page header | as the row | as the row; the whole page (view, log, open tab) read again when it goes from queued to running or to finished |
| Builds rail | nothing until the run after the write ended; then stuck as the row | *Queued* at the queue's answer; then as the row, its own row read at most once in 5 s |

**Measured in the real app** (2026-11-25, the debug app on the Mac against a real TeamCity 2026.1.3 with one agent,
under OrbStack, a 25 s build; times from the press, TeamCity's dates to the second):

| Case | Queued | Running | Finished |
|---|---|---|---|
| Agent free: the row | 12 ms (*Sent* 42 ms) | 93 ms (the read-back caught it started) | 31.4 s (TeamCity: 30.2 to 31.2 s) |
| Queued behind another build: the row | 10 ms; the read-back's row 83 ms | 26.9 s (TeamCity started it 26.0 to 27.0 s in) | 61.5 s (finished 57.0 to 58.0 s in) |
| The same, on the run page's header | as the page opened (2.1 s) | 28.9 s (started 26.8 to 27.8 s in) | 61.5 s (finished 56.8 to 57.8 s in) |
| The rail | 9 to 12 ms | within 5 s of the row | within 0.4 to 3.7 s of the row |

Before, *running* and *finished* waited for the source's next scheduled run (300 s on the live sources). The first
real run found one more cause: the pipeline page's row stayed *running* after the build finished, because the answer
cache drops on its own listener of `item:refreshed`, in no set order with the window's shared one the pages hear, so a
page's read again could be answered the kept row. Whoever reads again on the event now drops what it moved first.

**Ruled (Fable, xhigh, while Björn is away): the write queue follows a run it started, and pages read what they draw
when shown.**

1. **The follow** (`crates/knobas-app/src/sources/follow_run.rs`, ADR-0031 amended): after a landed write whose
   read-back item is a run with no `finished`, `after_landing` refreshes it by id on a falling cadence (2 s for 30 s,
   5 s to 5 min, 15 s after), `Lock::Try` (a run in flight skips the tick), as a person's read for 30 s and then
   behind. Ends when finished or gone, after 2 h, 5 failed reads in a row, the source off or removed, or the quit. At
   most 5 per source; a sixth stops the oldest. Not taken up after a restart.
2. **A tick that changed nothing tells nothing**: a one-item refresh whose upserts rewrote no row is
   `RefreshOutcome::Unchanged`, as a walk with no news.
3. **`item:refreshed` with no write behind it drops what a run of the item's source drops** in the answer cache, not
   every answer; a failed one nothing. `refresh_entity` drops nothing as it is asked: its end says what moved.
4. **Pages read again on the event**: the pipeline page its runs when the event names a run it holds; the run page its
   row and item, and the whole page when the run went from queued to running or to finished.
5. **Pages refresh what they draw in flight when shown** (`runs-in-flight.ts`): 100 ms after the runs drew, when shown
   again and when the window comes back, the newest 5, at most once in 5 s, in the background lane. This covers a run
   started elsewhere or before a restart. It finds no new runs: ADR-0047's rejection for a TeamCity configuration
   stands.
6. **The rail** draws a run queued here at once (`withQueued`: glyph and *queued*, until its own read holds the run;
   drawn only, the pipeline page settles the echo) and reads the moved pipeline's runs again when the event names a
   write of its echoes or a run it draws: at once on the first, then at most once in 5 s, the last always read. Only
   that pipeline's runs, laid into its row: the whole list is 571 KB at Björn's scale.
7. **Moves nothing**: a drawn row takes its new state in place; no chip needs held room (nothing stands after it).

**Other writes (Fable's ruling 8).** A Jira transition and a Gitea or Forgejo close or reopen already draw at once
(their echoes), are atomic, and are done at the read-back, which stands a run aside since the pr-state fix: nothing now.
Cancelling a run needs a new write (*Stop* is drawn disabled); the follow already ends on a cancelled run. Later: a run
TeamCity starts from a push or merge made in knobas, a log that grows while the run page shows it, and a pipeline page
finding new runs by itself.

**For Björn to reverse:** a run he did not start does not move while he watches it; it moves at the next scheduled
run, on *Refresh*, or when the page is shown again or the window comes back.

**Reversing it** is `follow_run.rs` and its call in `after_landing`, `RefreshOutcome::Unchanged` for one item in
`knobas-sync/src/refresh.rs`, `runs-in-flight.ts` and its callers (`TfPipeline`, `TfRun`), `withQueued` and the rail's
listener in `TfBuilds`, and `cacheEvents.refreshed`.
