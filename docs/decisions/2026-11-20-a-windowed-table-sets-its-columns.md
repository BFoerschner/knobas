# 2026-11-20: A windowed table sets its columns before a row draws

**Asked (Björn, on PR #2479):** "when i scroll the code repositories down the width of the columns keeps jumping."

## Why they jumped

Code's *Repositories* table was the one long table in the app left to the browser's automatic table layout. Each
column was as wide as the longest value in the rows drawn, so anything that changed what the rows hold changed every
column's width and every column after it moved:

- **A window of rows.** The list windows from 300 repositories (`shell/windowed.svelte.ts`), and then draws only the
  rows near the screen. A window holding a repository with a long name, two teams or *Jupyter Notebook* made the
  columns wider; scrolling on past them made them narrow again.
- **A value landing in a drawn row.** A repository's team and language (`code_facts`), its checkout (a path, then the
  branch and where it stands once its tree is read) and *Last push* (*3 minutes ago*, then *4 minutes ago*) each
  widen or narrow their column when they change, and a sync's end reads them again.

The table beside it, *Pull requests*, had its own widths since 2026-11-10 (`colgroup`, `table-layout: fixed`). So did
Tickets' list, a pipeline's runs, a repository's branches, pull requests and builds.

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

RESULTS

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
