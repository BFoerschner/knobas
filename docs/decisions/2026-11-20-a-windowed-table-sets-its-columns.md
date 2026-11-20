# 2026-11-20: A windowed table sets its columns before a row draws

**Asked (Björn, on PR #2479):** "when i scroll the code repositories down the width of the columns keeps jumping."

## Why they jumped

Code's *Repositories* table was the one long table in the app left to the browser's automatic table layout. Each
column was as wide as the longest value in its rows, so anything that changed what the rows hold changed every
column's width, and every column after it moved.

On the saved scale database (65 repositories) and the fixture (131) the list never windows: Code's repositories
window from 300. What moved them there is **a value landing in a drawn row**: a repository's team and language
(`code_facts`) and its checkout land after the rows draw, and each widened or narrowed its column. Measured in
headless Chrome, column widths before and after the reads landed:

| Size | Before the reads | After them |
|---|---|---|
| 1440x868@1 | name 601, Team 99, Language 149, Open PRs 151, checkout 207 | 550, 142, 149, 138, 244 |
| 2560x1440@1.25 | name 1040, Team 171, Language 257 | 992, 255, 269 |

A sync's end reads them again, *Last push* changes its words as time passes (*3 minutes ago*, *4 minutes ago*), and
on an estate of more than 300 repositories each window of rows brings its own longest values: each did the same. The
list does not have to window for it, and the reader is usually scrolling when it happens.

## What holds them now

- **Code's repositories** (`TfCode.svelte`, `code.css`): `table-layout: fixed` with a `colgroup`. The name takes the
  rest; *Team* 150 px, *Language* 112, *Open PRs* 88 and *Last push* 120, each times the interface's size (as Tickets'
  word columns); *Local checkout* 24 % of the table, so a wide window gives a path room; the row's actions column its
  padding, since *Open in…* hangs over *Last push*. A value longer than its column ends in an ellipsis with the whole
  of it in its title: the repository's name (its job chip and *on <source>* beside it are never cut), its teams, its
  language and a checkout's line; a path gives up its middle (`TfPath`).
- **A pull request's commits** (`TfPrCommitRows.svelte`): windowed from 40 commits with automatic layout, so its
  who-and-when column took each window's longest author. Now fixed: the sha its code width, who and when 210 px times
  the interface's size with an ellipsis and its title, the message the rest, wrapping.

## Every windowed list, checked

The scroll case below, on the fixture (every read late) and the saved scale database, at 1440x868@1:

| List | Columns | Before | After |
|---|---|---|---|
| Code's repositories | automatic | 276 cells moved on the fixture at 1x and 4x, 144 on the scale database at 4x (the name column 24 px narrower when the facts landed) | none |
| Code's pull requests | fixed since 2026-11-10 | none | none |
| Tickets' list | fixed, widths from its head | none | none |
| The Inbox, a Docs space | each row its own box | none | none |
| Builds' pipelines and runs, a pipeline's runs | rows; each day a fixed table | none | none |
| Monitors | one grid template for every row | none | none |
| A repository's branches, pull requests, builds | fixed | none | none |
| A pull request's commits | automatic | not windowed on either corpus (one commit on the fixture's first pull request) | fixed |
| A repository's commits, Compare's files and commits | drawn whole, or rows | none | none |

An asset column needs `?fake-assets`, and a repository's tabs, a pull request's commits and Compare need the live
forge on the saved scale database: there the case says so and goes on.

## The launcher's list, on the way

`scrollers.mjs --strict` on the saved scale database, at all three sizes, found one list whose wheel Chromium
hit-tests on the main thread: the launcher's (`div.tf-ln-list`, every point). In the page with the launcher open,
eight wheel notches over the list, each variant set on the element by hand:

| Variant | Notches hit-tested on the main thread |
|---|---|
| as it was | 8 of 8 |
| the dialog `overflow: visible` | 0 |
| the dialog without its `border-radius` | 0 |
| the body `overflow: visible` | 8 |
| no shadow, no scrim | 8 |

So it is the dialog's clip to its rounded corners. The launcher's dialog no longer clips (its body still clips
square), and its footer rounds its own fill; `tf-css.test.ts` holds both. Other dialogs keep their clip: their bodies
are short and none of them is a list a person scrolls through.

## The guards

- `app/src/lib/tf/windowed-columns.test.ts`: every table whose body is windowed (`{@attach <win>.list}` on its
  `tbody`, or `TfWindowedRows`) has `table-layout: fixed` from a rule that reaches it (the rule's classes on the table and
  its ancestors in the same component) and a `colgroup` or `thead` to take its widths from. On the head before this
  pass it names the two tables above.
- `testenv/perf/columns.mjs`, the scroll case of `just moves` (`testenv/perf/README.md`, *Nothing moves*): each
  windowed list wheeled to its end and back as real input, every frame the box of each header cell and each cell of
  every row drawn; a cell whose left edge or width changes by more than 1 px while it scrolls is a mover. The fixture's
  `?fake-code-rows` repositories now vary as a real estate's do (long names, two teams, a long language, together by
  last push), and the Code case reads 500 of them so the list windows.
- `just frame-budget` holds Code's repositories as a scroll (`code-repos`, at Code's pull requests' lines).
