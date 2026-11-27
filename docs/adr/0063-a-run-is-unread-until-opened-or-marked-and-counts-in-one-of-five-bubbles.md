---
status: accepted
---

# A run is unread until it is opened or marked, and counts in one of five bubbles

Supersedes parts of `docs/decisions/2026-11-17-mark-builds-as-read.md` (*Who can be unread*, *Opening a run reads nothing*, *What all means*), which stays the record of the controls, the look and the Inbox's one notion. Decided 2026-11-27 (spec #2566, #2567; Björn's read model on #2555, his answers on #2554, and his rulings on #2567's open questions): **every run can carry a mark, got by opening it or by a *Mark … as read*, and the mark records whether the run had finished when it was written. A run counts, until read, in one of five bubbles by its state: blue running, green passed, yellow passed with warnings, red failed, grey cancelled. Opening a run reads it whatever it does after; a mark taken while it ran takes only its blue. Red and blue are cleared run by run; green, yellow and grey by an instant in this machine's clock.**

## The five bubbles

One place maps a run to its bubble: `run_bubble!` in `knobas-core`'s `run_read.rs`, over the run's resolved tone and its declared `started` and `finished`. Each adapter maps its source's statuses to tones in its declarations, so a source's words reach a bubble in two steps and in no third place.

| Bubble | The run | Tone |
|---|---|---|
| Blue | started and not finished: running | any |
| Green | finished, passed | `ok` |
| Yellow | finished, passed with warnings | `warn` |
| Red | finished, failed | `err` |
| Grey | finished, cancelled, or a status that fits none of the above | `muted`, `info`, or none |
| none | not started: queued | any |

- **Queued is not blue** (Björn, 2026-11-27). Blue is started and not finished. A queued run is not news until it runs.
- **A status that fits none lands grey.** A finished run whose state the declaration gives no tone, or `info`, has ended and said nothing good or bad: it is counted, not lost, and it is not red, which would ask for action nobody can name.
- **The reader's state colours move a run between bubbles** (spec #2463): the tone read is the resolved one, overrides merged in, so a status the reader recolours counts in the bubble of its new colour.
- **By source, today.** TeamCity (the one adapter that frames runs): `SUCCESS` green, `FAILURE` red, `UNKNOWN` grey (TeamCity reports a cancelled build so), `ERROR` grey (undeclared; it fits none). TeamCity has no passed-with-warnings status, so it never draws yellow. An adapter that frames runs later declares its own: Gitea and Forgejo Actions `success` ok, `failure` err, `cancelled` and `skipped` muted; Jenkins `SUCCESS` ok, `UNSTABLE` warn, `FAILURE` err, `ABORTED` and `NOT_BUILT` muted.

## What reads a run

- **A mark** is one row in `knobas.run_read` (migrations `0098`, `0105`): the run's entity id, when, `how` (`opened`: the run's page was shown; `marked`: a *Mark … as read*, a scope's mark, or the Inbox's *Done* on a failed build) and `finished` (whether the run had finished, as the mirror held it, when the row was written). `0105` turns every row written before it into *marked after finishing*, which is what each was.
- **Read** (`reads!`, the one predicate): a mark that is `opened`, or one written after the run finished. A read run is in no bubble. The Builds list's unread failures (`PipelineRow.unread`), the Inbox's failed builds on both shelves, the top bar's count and the window's set all ask it.
- **A mark taken while the run ran** reads nothing. It takes the run out of the blue, and when the run ends it counts in the bubble its end says (question 1 on #2554: only opening counts as having looked). Marked again after it ended, the row is written over in place and reads it.
- **Opening a run reads it**, whichever way its page was reached (a row, the launcher, an address), whatever it does after, a failure's Inbox line with it (question 3: read and *Done* stay one row). The page writes the mark once per run per session and draws it at once; a page shown again writes nothing.
- **Opening writes no activity line** (Björn, 2026-11-27: only an explicit *Mark … as read* writes one, as before). The window that opened the run reads the top bar and the Inbox again on the open's answer. It is one switch, `OPENING_WRITES_A_LINE` in `commands/entity/run_read.rs`.
- **The Inbox's *Done*** on a failed build writes a mark in the same transaction, recording whether the run had finished like any mark; *Mark as unread* deletes the row, however it was got, and the line's *Done*.
- **Scrolling past does nothing.** Nothing else writes a mark.

## A mark by scope

`mark_runs_read` takes a scope: **everything** (every run of every live pipeline, which needs no list from the page), **a set of pipelines** (a folder is its pipelines), or **runs by id**, or a union of them. Of the runs in scope:

- **Blue and red are marked run by run.** Each running run gets a mark taken while it ran, each failure no mark reads gets a mark after it finished. A run named by id is marked whatever its state. The answer names the rows written, split into those that read their run and those taken while it ran, and *Undo* deletes exactly those rows. A row written over (marked while running, marked again after) goes too on *Undo*, which leaves the run as it was: a finished run's mark taken while it ran reads nothing.
- **Green, yellow and grey are cleared by moving the scope's instant** (below), not by a row per run: they are counts, not lists to tick off. *Everything* moves the one instant; a set of pipelines moves each one's own. *Undo* puts back the instant it moved, unless leaving Builds has moved it since.
- **What *all* means on the page.** *Mark all as read* at *All pipelines* sends *everything*; at a folder or a pipeline, its pipelines; with a filter narrowing the table, the runs it lists, by id. A rail row's right-click marks its pipelines.

## The instant, and its clock

- **Green, yellow and grey count the runs that ended after the instant and that no mark reads.** The instant is when Builds was last left (#2568 stores it, as a `knobas.setting` row, beside each pipeline's own a scope's mark writes), so what passed since the last look is what is counted, and looking clears it.
- **Both sides are this machine's clock.** A run's side is the local time the mirror first saw it finished, stamped when its stored row first carries a finish, never the source's `finishDate`: 2026-11-17 rejected a watermark on TeamCity's clock against this one, and comparing two local stamps has no skew. A run the mirror first sees already finished is stamped when it is first written. The stamps the mirror holds before it learns to stamp, and those a backup's restore rebuilds, are older than any instant, so nothing old turns green.
- **While Builds is in front** the page keeps the instant it read on arrival, so the green of the visit stays for the visit (question 2). A run that ends during the visit is stamped after the visit began and counts in no green, yellow or grey: you are looking at Builds. A failure still turns its blue red.

## Rejected

- **Only a finished failure can carry a mark** (2026-11-17). A running run already reporting `err` counted red and could not be marked (#2555's first bug), and nothing could say a run had been looked at while it ran.
- **A time watermark on the source's clock** for any bubble. TeamCity's clock is not this one; a skew of a minute turns a pass green twice or never.
- **A row per green, yellow and grey run.** The bubbles would be lists to tick off, and *Mark all as read* would write a row for every pass since the instant. The instant clears them in one write and *Undo* puts one value back.
- **A separate table for opened runs.** Two rows per run would need a rule for which wins; one row with `how` and `finished` says it.
- **An activity line per open.** Browsing runs would fill the log with views. Kept as a switch should Björn want it.

## Consequences

- Migration `0105` adds `how` and `finished` to `knobas.run_read`. The frozen IPC surface changes: `read_runs` answers `RunMarks` (`read`, `while_running`), `mark_runs_read` takes a `MarkScope` (`everything`, `pipelines`, `runs`) and answers `RunMarks`, and `mark_run_opened` is new (`docs/contract.md` §10.8).
- `knobas-core` exports `reads!`, `read_ids!`, `run_bubble!`, `runs_of!` and `failed_runs_of!` (red of `run_bubble!`), so every reader asks one predicate and one map.
- #2568 stores the instants, stamps each run's local first-seen finish, and counts the five bubbles per pipeline.
