# 2026-11-18: A ticket reads its card in one go, with the press

**Asked (ux19, from the nothing-moves pass's follow-up):** a ticket's page made five reads for its sections (`job_reach`
and a reach-scoped `list_entities` for Code, Builds, Docs and Monitors), and the movement pass holds the sections and
the actions under them until all have answered (`2026-11-17-nothing-moves-once-drawn.md`), so the slowest of the five
set when they show: 52 to 139 ms on a first open at 1x on the PC. `card_reads` answers the five in one walk and had no
caller since Tickets' preview went (`2026-11-17-a-view-that-opens-with-every-ticket.md`).

## What held the card back

| | Before | Now |
|---|---|---|
| The reach and the four sections | five requests, after the ticket's own read | one `card_reads`, sent with the press when the address names the ticket's id (every press does), beside the three reads that find the ticket |
| The sub-tasks (`mini_board`, `under:<key>`) | asked after the ticket answered; the last read the hold waited for | asked with the press, under the key the address names |
| `card_reads` on the backend | the walk, then each section's role fillers (the source list and its misses, read again a section) and rows, one after another: 11.7 ms against 6.9 for the five requests at once | the walk, the declarations and every role's fillers at once (the source list and misses read once, `role_scopes`), then the four sections at once on at most four connections |
| A visit (`visit_entity`, every open) | dropped the ticket, its reach, its card and its assets from the answer cache, and every read still on its way | keeps them (no detail's history holds a visit, `activity::recent`; a reach walks links), and keeps a read on its way that it would keep |
| The promotion (`promote_context`, every open) | dropped every answer the window held, the card's among them, so nothing of a ticket was ever drawn from what was stored | drops them only when it may have made the context: an anchor not seen this session whose context's `created_at` is under a minute old |
| A reached asset (`get_asset`) | not a kept read, so it too dropped every answer | kept, dropped by every run that changed something |

An epic's context reaches its sub-tasks, and the card's walk can run before a promotion that makes one commits (true
since `2026-11-12-a-page-asks-for-what-it-draws.md` let the reach stop waiting for it): an epic opened for the first
time drew its card without its sub-tasks' connections until the next open. The reader reads the card again when the
promotion answers an epic's context made in the last minute.

A card read that fails draws every section as having nothing, as a failed section read did alone; the page itself no
longer fails with it (a failed Code read used to fail the whole page).

## Same answers

On a copy of the saved scale database (`final-home`), every one of its 23,176 tickets reaches nothing, so the five
reads and `card_reads` answer nothing alike for all of them. On a second copy, 50 tickets linked to 445 pull requests,
repositories, branches, builds, pages and monitors (`create_link`), 160 tickets (the 60 reaching most, a spread of the
rest, and some reaching nothing) answered the same through `card_reads` as through the five reads, row for row, path
and state included: 917 rows, none different. `card_reads_answers_what_the_five_reads_answered` now also asks a role
twice. On that copy `card_reads` takes 20.2 ms at the median (p95 31.2) against 23.0 (37.8) for the five requests
sent at once, on the Mac at a load of 10 to 17.

## Measured

On Björn's PC: headless Chrome 156 in WSL drawing the built app, the windowless backend on the scale copy on the Mac
through an SSH tunnel (about 35 ms a read), `open.mjs --only 'ticket$'` (a card on Tickets pressed, the `held` mark
being the card's actions), under the PC queue. *Before* is `ecc33edd`. The ticket the probe opens reaches nothing on
this database, so its card's rows are the empty state; what is timed is when the held part shows. Reads on a first
open: **16 before, 12 now**.

The held part's first showing, ms (1x / 4x / 6x):

| Open | 1440x868@1 before | now | 2560x1440@1.25 before | now |
|---|---|---|---|---|
| First, the app never having read it (storage cleared) | 72-105 / 152-200 / 231-248 | 88-90 / 84-85 / 114-128 | 88-106 / 150-169 / 214-248 | 72-89 / 100-152 / 131-155 |
| First after the app started, its reads warmed from the last session | 72-88 / 135 / 198-199 | 17 / 31 / 33 | 55-56 / 134 / 181-199 | 22 / 17 / 32 |
| Return, after every other section, after a sync (the kept page) | 18-32 | 18-27 | 16-24 | 19-30 |

A cold first open's sections and actions now show in the frame the page is first usable at 4x and 6x (five runs of
six; one 4x run at 2560 four frames later), where before they came four to seven frames after it. At 1x a cold open is
as it was, a frame after the page is usable: the reads that find the ticket are the wait there. Warm, the held
part shows with the page's first frame: before, the visit and the promotion had dropped the warmed answers before the
sections were read. Every open is within its line (100 / 200 / 300 ms).

On the Mac (headless Chrome and the backend on one machine, at a load of 5 to 10 from other agents' builds; two
runs a case, so noisy), the held part's first showing, ms (1x / 4x / 6x):

| Open | 1440x868@1 before | now | 2560x1440@1.25 before | now |
|---|---|---|---|---|
| First, the app never having read it | 82-126 / 177-1219 / 273-339 | 62-93 / 120-158 / 204-274 | 108-142 / 238-280 / 418-738 | 108-124 / 152-284 / 365-542 |
| First at 4x and 6x after the 1x round in the same session | 140-194 / 198-742 | 17-26 / 12-18 | 190-387 / 314-593 | 16-46 / 31-98 |
| Return (the kept page) | 18-31 | 26-32 | 24-32 | 16-30 |

The cold 6x opens at 2560 over 300 ms are the page's own first mount on the loaded Mac, before and now: the held part
showed with the page's first useful frame in each of them now (365 to 542 ms), and 217 and 300 ms after it before.

`just moves` on Tickets, the ticket page and its card's menu (`moves.mjs --only ticket,ticket-menu`), first open and
return, 1x and 4x, 1440x868@1 and 2560x1440@1.25, on the fixture with every read late and on the scale copy: nothing
moved.

## Reversing it

`JobPorts.cardReads` left out reads the five as before; the cache's rules are the `promote_context`, `VISIT_BLIND`
and `get_asset` lines in `ipc/answer-cache.ts`.
