# 2026-11-17: A row a list draws restyles that row, not the list

**Asked (orchestrator, from the moves follow-up pass, for ux18):** Tickets' list scrolled at 48 fps at CPU 6x at
1440 x 868, and 50 / 41 at 4x / 6x at 2560 x 1440 at 125 %, against 58 / 50 before, with about 590 elements restyled
a style pass. It came with the ux18 list changes (`bfd581ef` on: the preview removed, the list's 1,640 px cap). Find
the rule, make a scroll restyle nothing that did not change, at every size and throttle, with a test that fails before.
Check the board and the laned board.

## What restyled the list

A Chrome trace of the 4 s scroll with the invalidation categories (`testenv/perf/invalidations.mjs`' tally, on the
saved scale database): every row swap restyled the list's whole `tbody`, *Invalidation set invalidates subtree*
(`allDescendantsMightBeInvalid`) on `TBODY`, 400 to 530 elements a pass at 1440 and 580 to 690 at the wider sizes,
where a swap's own rows are about 70. Deleting rules in the running page found the cause: every rule with a `+` or
`~` beside a compound that names no class, id, tag or attribute.

- **A subject that names nothing** (`.tf-md > * + *`, `.tf-tkstack > * + *`, `.tf-headed > header + *`,
  `.tf-fhead.tf-unread.tf-holdsbelow ~ *`) is tried against every element on the page. Trying its `+` or `~` marks
  the element's parent as having children that sibling rules read, so every parent was marked: the list's `tbody`, a
  board column's cards.
- **A left side that names nothing** (`* + *` again, and `:not(.tf-rowacts) ~ .tf-rowacts`) goes into the one sibling
  invalidation set Chromium cannot key on a class.
- **A row inserted into a marked parent** schedules that set on the parent, for its whole subtree.

Any one of the first three subjects is enough to mark every parent: deleting one at a time changed nothing, and
deleting all of them left only the rows a swap drew. The oldest, `.tf-md > * + *`, is from `668610ff`
(2026-10-31), so the list has restyled its whole body on each swap since before ux18. `bfd581ef` made each pass
bigger: without the preview the list is 1,210 px wide at 1440 instead of 870, its rows wrap less, and more of them are
drawn (405 elements a pass on its parent against 439 on it, on the same data). A style pass runs only in a frame that
swaps rows, about 38 of the 240 frames of the probe's scroll, so most frames restyled nothing before either. The
swaps' 13 to 19 ms at 6x were what dropped them.

The board's columns had the same restyle, at most 31 elements a pass (a column holds few cards), and a laned board
draws all its lanes' cards and restyles nothing on a scroll.

## Taken

Each rule says the same with a named or positional subject, at the specificity it had, so nothing looks different:

| Before | After |
| --- | --- |
| `.tf-md > * + *` | `.tf-md > :where(:not(:first-child))` |
| `.tf-tkstack > * + *` | `.tf-tkstack > :where(:not(:first-child))` |
| `.tf-headed > header + *` | `.tf-headed > :where(:nth-child(2)):not(header)` (a headed panel opens with its header, `style-cost.test.ts`) |
| `.tf-rowend > :not(.tf-rowacts) ~ .tf-rowacts`, and the Inbox's | `.tf-rowend > .tf-rowacts:not(:first-child)` |
| `.tf-fhead.tf-unread.tf-holdsbelow ~ * { visibility: hidden }` | `.tf-heldbelow`, set on each part under a run's held header while it is held (the banners, the steps bar, the stale line, the runs' failure, the body) |

The run's parts get the class each, not a wrapper with `display: contents`: `.tf-bmain .tf-sbody > .tf-banner` styles
the banners as the scroller's own children.

`style-cost.test.ts` refuses a `+` or `~` beside a compound that names nothing, in every stylesheet and every
component's `:global` style (a scoped one carries Svelte's class), and fails on the head before this with the six rules
above. The frame budget holds Tickets' list at 200 elements a style pass at every throttle, where its 700 line held 1x
only and the restyle passed under it (`budget.json`, both corpora), and `run.mjs` gains the laned board's scroll,
`tickets-laned`, at the board's lines.

## Measured

On the Mac (M2 Pro), headless Chrome, the windowless backend (release build) on a fresh clone of the saved scale
database, `run.mjs`' scroll for the before (`f85f296c`) and `just frame-budget --real` for the after, load average 5
to 9. Frames a second; elements a style pass, the most of the run.

| Tickets' list | before | after |
| --- | --- | --- |
| 1440x868@1, 1x | 60, 526 | 60, 71 |
| 1440x868@1, 4x | 60, 526 | 60, 71 |
| 1440x868@1, 6x | 57.8, 526 | 60, 71 |
| 2560x1440@1.25, 4x | 60, 690 | 60, 71 |
| 2560x1440@1.25, 6x | 53, 683 | 60, 71 |
| 5120x1440@1, 4x | 60, 690 | 60, 61 |
| 5120x1440@1, 6x | 55.5, 690 | 59, 61 |

At 6x the list's worst frame went from 28.6 / 36.3 / 32.2 ms at 1440 / 2560 / 5120 to 18.8 / 20.9 / 35.6 ms, and
its longest task from 36 / 42.5 / 39.3 ms to 23.4 / 29.3 / 41.6 ms. The board held 58.3 to 60 fps before and 59 to 60
after; the laned board 60 before and 59.8 to 60 after; no task over 60 ms in any of them.
