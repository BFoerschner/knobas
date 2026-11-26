# 2026-11-17: Mark builds as read, one or all

**Asked (Björn, on PR #2479, ux18):** "I want to be able to mark builds as read. Single ones as well as all. Since I might not care about old failed runs."

## Where a run asked for attention before

Nothing was read or unread. A failure asked everywhere until a later run passed:

| Place | What it drew |
|---|---|
| Top bar | *Builds* count: pipelines whose newest build failed (the Builds tile's `latest_per_parent` tally). |
| Builds rail | A failing pipeline's name and time in err; a fold's *N failing* in err, and the fold opened by itself; bare `#/builds` opened on the first failing pipeline; the *Failing* chip. |
| A pipeline's runs | Each run's state chip; nothing more. |
| Inbox | A *failed build* line per failed run on my work, until its pipeline went green, with *Done* and *Snooze*; desktop notifications come from new lines. |
| A repository's Builds tab | Each pipeline's newest run per branch: a table of pipelines, not of runs. |
| A ticket's *failing build* signal, a room's Builds tile | Facts about the work. |

## The ruling

The judgment calls went to Fable (xhigh); its rulings are followed here.

- **Who can be unread.** A finished run whose declared state is `err`, with no mark. Every other run is read by its nature: nothing new is drawn on a pass, a running or a queued run. A failure stays unread until it is marked, whatever its pipeline did since.
- **Stored** in `knobas.run_read` (migration `0098`): one row per run marked, by its entity id. Local only, never written to TeamCity, kept across syncs and restarts. A new run is a new entity, so **a new failure is unread**. *Mark all* writes a row per failed run in scope in one statement and answers the ids it wrote; *Undo* takes back exactly those. A time watermark was rejected: it depends on TeamCity's clock against this one, and makes *Undo* and a single *Mark as unread* awkward.
- **One notion with the Inbox, by writing.** *Read* is the row and nothing else. The Inbox's failed-build rule leaves out a run with a row, on both shelves; *Done* on a failed build writes the row in the same transaction; *Mark as unread* deletes the row and the line's *Done*, so the line comes back (a snooze stays). The migration turns every *Done* already given on a failed build into a row.
- **A pipeline's alarm is its newest run.** It asks while its newest run is a failure unread: name and time in err on the rail, counted in the top bar, a fold opening by itself, first for a bare `#/builds`. Read, the pipeline still fails: its glyph stays red, the *Failing* chip still keeps it, and a fold's *N failing* still counts it, toned err only while one under it asks. Reading an older failure quiets nothing on the rail; its dot goes from the run table.
- **Opening a run reads nothing.** Only a mark does: a glance would otherwise take a to-do off the Inbox and silence the count for a pipeline still red.
- **What *all* means.** The rail's *Mark all as read*: every failed run of the pipelines the rail lists now, its filter applied, so unfiltered every pipeline. A pipeline's header and its right-click menu: every failed run of it, older pages included (the backend resolves it by pipeline). Nothing on a repository's Builds tab, which lists pipelines, not runs. No *all older than this one*. Both are always enabled; a press that finds nothing says *No unread runs*.
  - *Amended 2026-11-26 (#2527):* on the Builds rail, a pipeline's right-click and a folder's say *Mark N runs as read* (*Mark 1 run as read*), and leave the item out at none. A pipeline linked anywhere else keeps *Mark all as read*.
  - *Amended 2026-11-27 (#2543): N is the whole history again.* #2527 first counted only the failures the rail holds (each pipeline's last twelve runs) and marked exactly those, so an older failure stayed unread after the press; its merger ruled that short of "its unread failed run count" and "marks them all". Now the Builds list names each pipeline's unread failed runs over its whole history (`PipelineRow.unread`, by the same query a pipeline's mark writes by, `failed_runs_of!`), N counts those against the window's read set, and the press marks by pipeline again, drawing the N read at once. The label, the toast and *Undo* still agree: the backend writes the N the label named (unless a sync brought a failure in between, which the toast then counts). The count stays fresh across marks with no read of the list: a mark or *Undo* moves it through the read set, and a failure marked unread that the list did not name (read when the list was read) reads the list again. The list's whole-history read costs about 2 µs a run: 26.6 ms of a 47.6 ms read at 13,000 runs (`builds_list_plan.rs`).
- **Undo** on *Mark all*: a toast, *Marked 312 runs as read*, with *Undo*. One run: no toast, the control's word flips.
- **Look.** An unread run: a 6 px err dot hung in its number cell's gutter, its hidden word *Unread*; the number keeps its weight (every run's is bold already), and marking moves nothing. No dot on the rail: an asking pipeline looks as a failure looked before. The design system says it (*Read and unread*, *Keys*).
- **Controls.** *Mark as read* / *Mark as unread* in a failed run's row actions, left of *Log* and *Run again…* (an icon button there, a tick to read and the dot to unread, the words its name and tip: as words the group ran over the duration and cut it, and a press on a hovered row's duration marked the run instead of opening it; the start time's column is as wide as the three), and on its page's header. `m` toggles it on the focused run row and on the run's page; not `r`, which reads as *Re-run*. *Mark all as read* in the rail's tools (an icon button there, two ticks with the words as its name and tip: as words it wrapped the tools onto a second line beside *Collapse all* and moved every row 32 px) and the pipeline's header. The right-click menu: a failed run's *Mark as read* or *Mark as unread*, a pipeline's *Mark all as read*.
- **Everything moves at once.** The window holds one set of read ids (`runs-read.svelte.ts`), read once the database answers and replaced whole by a mark, so marking hundreds of runs is one change that every list follows. It is not in `RunRow`, whose answers are kept and would go stale. Each mark writes one activity line, whose `activity:new` the Inbox and the top bar already re-read on.
- **Left alone.** Desktop notifications (a line *Undo* brings back was seen this session and notifies nobody), the ticket's *failing build* signal and the room's Builds tile, which are facts about the work.

## Tested

- Rust: `knobas-app`'s `run_read_ipc.rs` (a pipeline's mark writes its finished failures only and answers what it wrote, a second press writes nothing, *Undo*; the count drops when the newest failure is read and a new failure asks again; reading and *Done* are one answer both ways; a withdrawn run's mark is not answered), `knobas-core`'s `inbox.rs` (a read run leaves both shelves until marked unread, its *Done* with it). 7 mutants of the new SQL and Rust, each killed.
- Vitest: `builds.test.ts` (the rules), `runs-read.test.ts` (the store: drawn at once, put back on a failed write, *Mark all*'s *Undo* and *No unread runs*, the Inbox's *Done*), `TfPipeline`, `TfBuilds`, `TfRun` (the dot, the controls, `m`, *Mark all as read*, a new failure unread), `items.test.ts` and `thing.test.ts` (the menu), `inbox.test.ts` (*Done* reads its run). 34 mutants of the new code, each killed.

## Timing

On Björn's PC, headless Chrome in WSL, the backend the Mac's windowless one over the saved scale database (91 pipelines, 20 failing, 2,620 runs) through an SSH tunnel; real presses, 7 each, medians at 1x / 4x / 6x (`testenv/perf/runs-read.mjs`):

| Press, to the frame that shows it | 1440x868@1 | 2560x1440@1.25 | Line |
|---|---|---|---|
| *Mark as read* on a row (the dot gone) | 32 / 30 / 46 | 32 / 46 / 64 | 100 / 100 / 200 |
| *Mark as unread* | 33 / 32 / 48 | 33 / 42 / 66 | 100 / 100 / 200 |
| *Mark all as read* on the rail (no name in err, no dot) | 33 / 35 / 53 | 33 / 45 / 71 | 100 / 100 / 200 |
| its toast, after the backend's write | 83 / 98 / 99 | 83 / 99 / 114 | 100 / 200 / 300 |
| *Undo* (the failures back) | 33 / 35 / 55 | 33 / 44 / 67 | 100 / 100 / 200 |

The backend's `mark_runs_read` of every pipeline: median 20 ms, worst 54 ms. Builds' first open and return, a pipeline's and a run's (`open.mjs`, the open budget's places), on the PC: every one inside its line at both sizes; Builds first open drawn 46 / 113 / 151 ms (1440) and 46 / 139 / 209 ms (2560) at 1x / 4x / 6x.

A first Mac run drew *Mark all* only when the backend answered (65 ms at 1x, 103 ms at 6x): the open pipeline page's older failures were not among the rail's runs. A pipeline page now holds its failed runs with the store (`hold`), so they turn at once too.
