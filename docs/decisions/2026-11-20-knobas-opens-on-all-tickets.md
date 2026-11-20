# knobas opens on All tickets (2026-11-20)

The ux26 pass, PR #2479. Björn: "the knobas app should open in the all-tickets view. It looks kind of broken when it
starts in the 'no job has been selected' and a button to show all tickets when first starting knobas".

## What decided the start

`App.svelte` replaced a blank address with home, the bare `#/job` (`tf/home.ts`, spec #2227), before the shell
mounted. A bare `#/job` drew the chosen job's ticket page (`knobas.job.chosen` in the webview's storage, spec
#2396). With none chosen, `TfPage.svelte` drew a page headed *Job* saying how a ticket becomes the job, with an
*All tickets* button. The webview opens on no address at every start, so knobas keeps no last address: the chosen
job is the only place it remembers.

## What it is now

- **No chosen job: All tickets.** A blank address becomes `#/tickets` instead (`homeOf`), still before the shell
  mounts, so its first frame is the Tickets page with nothing to replace and no step back.
- **A chosen job: unchanged.** Home stays the bare `#/job` drawing it.
- **A chosen job knobas no longer holds** (deleted at its source, the source removed, another database): the
  ticket page at home reads it, gets "not held", forgets the choice, and the shell puts `#/tickets` in home's
  place. That one case paints *Opening KEY…* first, because only the read can tell. A read that fails keeps the
  job and says why: a failure is not a gone ticket. The ticket's own `#/ticket/<id>` address forgets nothing.
- **A bare `#/job` later, with none chosen** (typed, a bookmark, Back to an old entry): All tickets too. The shell
  replaces it with `#/tickets` in an `$effect.pre`, before anything under it draws. The empty *Job* page is gone,
  not hidden: nothing reaches it any more. The choice can only end by being forgotten as above, so that empty page
  had no other way in.

The first-run wizard is unchanged (it lands on Tickets with the launcher open).

## Checked by

`home.test.ts`, the shell's and the app's tests (a shell mounted on `#/job` with none chosen is at `#/tickets` at
once; nothing draws under a bare `#/job`), and `ticket-page.test.svelte.ts` for the gone, failed and own-address
cases. Eight mutants, each killed. The press trips' `cold-start` kind (`cold-no-job`, `cold-chosen-job`,
`cold-gone-job`) opens the app cold in Chrome and records every frame from the shell's first: it fails if the empty
page or *knobas does not hold this ticket* ever paints, or the first frame is not at All tickets. On the build before
this change it fails `cold-no-job` and `cold-gone-job`.

## What it costs the first window

Headless Chrome on the WSL box (Ryzen 5600X), the perf build on the fixture at scale, medians of five cold loads, ms
from navigation start:

| | 1x | 4x | 6x |
|---|---|---|---|
| No job, before: the empty page / All tickets' cards (button pressed the frame it appeared) | 251 / 505 | 790 / 1745 | 1136 / 2662 |
| No job, after: the first frame, All tickets' cards in it | 455 | 1668 | 2526 |
| Chosen job, before / after: its card | 384 / 413 | 1415 / 1391 | 2092 / 2074 |

The same at 2560x1440@1.25 within 40 ms of these. The chosen job's 1x, measured again over nine loads, is 387 before
and 388 after: the 29 ms above is noise. The first frame comes later than the empty page did, because it is
the board rather than a sentence; the board itself arrives earlier than it could through the button, and nothing
paints before it.

## Reversing it

Make `homeOf(null)` return `#/job` and bring back `TfPage.svelte`'s empty branch, and drop the shell's
`homeInstead` effect.
