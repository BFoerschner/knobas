# 2026-11-24: Zen mode is one switch for the whole of Code

**Asked (Björn):** "also, I'd like zen mode to work everywhere in code".

Step Z1 of the compare-docs pass. What Zen hides on each page, how `Z` and `Esc` behave and whether it holds between pages went to Fable (xhigh) under Björn's standing instruction; Opus built it. It replaces the file-only Zen of ux31 ([2026-11-23](2026-11-23-zen-stands-first-and-a-held-block-is-not-drawn.md)).

## What it hides

On every Code page, four things wherever they stand:

- the page's crumbs;
- its header (the repository's head, the pull request's, the Code list's, a diff page's);
- its tab bar;
- the rail on its left, with its side edge: the Files tree, a diff's file list (a commit, Compare, Local changes, a pull request's *Files changed*, a diff page), the Builds tab's branches.

The content takes the rail's place and the page's width. What the page works with stays: a file's own header, the generated folder's band, a commit's header, a diff's toolbar, the compare form, filters, *Levels* and the query bar, threads. The top bar and the status bar stay: the top bar is the way out of Code, and the status bar says when a sync failed.

| Page | Zen hides | Stays |
|---|---|---|
| A file, Source or drawn by a renderer | crumbs, repository head, tabs, the Files tree | the file's header, the band, the view |
| Commits, a commit | the same, the diff's file list | filter, commit header, diff toolbar |
| Branches, Pull requests (the tab), Builds | the same, the Builds rail | filters, rows, runs |
| Compare, Local changes | the same, the diff's file list | the form or the checkout line, the diff toolbar |
| A pull request | its crumbs, header, tabs, *Files changed*'s list | the conversation, diffs, threads |
| The Code list | its header and tabs | the filter bar, rows |
| A diff page | crumbs, header, the file list | the diff toolbar, diffs |

A pull request's *Review* and *Merge* hide with its header; `Z` brings them back.

## Keys, the button, and how long it holds

- **`Z` on any Code page**, outside a field, with no dialog, menu or launcher open. The shell owns it.
- **One button** in the top bar, after the section nav, drawn only in Code, pressed while on. The file header's button is gone: a button in a header Zen hides would move under the pointer.
- **`Esc` leaves Zen first** (an inline panel's rung), before it leaves the page.
- **It holds while the reader moves between Code's pages** and ends on leaving Code. It is never kept across restarts.
- **Source and generated files too**: `Z` on a file no renderer draws no longer does nothing.
- **`T` (*Go to file*) in Zen** leaves Zen first, so the field it focuses is drawn.

## Nothing moves but what it hides

- The class is set on `main`, so a page drawn while Zen is on draws without its header from its first frame.
- The topmost row the reader sees keeps its place: measured before the flip, again after its layout, and the scroll set before its paint. At the page's top the content rises by what Zen hid, which is Zen's own move.
- The panes fitted to the window (the API and schema explorers, the XML view) fit again in the flip's frame.
- A focus inside what Zen hides goes to `main` first, so `Z` and `T` keep working.

## Rejected

- **A different hide list per page**: the reader would learn each page's Zen.
- **Hiding the top bar and the status bar**: the way out of Code and the sync's failures would go.
- **Lists left out**: reading history goes from a commit to all commits and on, and the header would come and go at each step.
- **A Zen button in each page's band**: the Code list and a pull request's conversation keep no band.
- **Zen ending on each page change**, and **Zen as a kept setting**.

## Checked by

`zen.test.ts` (the key; the row held through a flip; the focus moved; the stylesheet hides what `ZEN_HIDES` names), `TfShell.test.svelte.ts` (`Z` and the button on Code's pages, held from page to page, `Esc`, leaving Code; a field's letter, a dialog's keys), `TfTopBar.test.svelte.ts` (the button), `TfRepoFiles.test.svelte.ts` (no button of its own; `T` leaves Zen). `testenv/perf/zen.mjs`: on thirteen Code pages, scrolled and at the top, the row held, the button still, nothing moving in the second after, and the key's frame against its line.
