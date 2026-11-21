# 2026-11-21: A click on a Docs tree row opens its fold

**Asked (Björn, dictated, on PR #2479):** "On the [Docs] pages, in the tree view, when I click on a tree element, I want
it to always expand, when I click on it, And when I click on, click on it again, it should [toggle], basically. I don't
like to have to hit that letter or I can to expand and so on."

## Before

A page with pages under it was a `<details>` whose `<summary>` held the page's link. A click on the title opened the
page and left the fold as it was: a link inside a summary does not fold it. Only the chevron, or the free end of the row
right of the title, folded it, and → did by key.

## Now (`TfDocTree.svelte`, `pressed`)

- **A click on a page's row opens the page and its fold.** Anywhere right of the chevron: the title, or the row's free
  end, which goes through the link so the shell sees a plain press. A first click never closes a fold.
- **A click on the open page's row toggles its fold**, closed if open, open if closed. The page stays.
- **The chevron folds alone**, left of the title, and opens no page.
- **The keys stay as they were.** `Enter` on a row (a click with no count, `detail` 0) opens the page only; → and ←
  fold. Ctrl/⌘-click, ⇧-click and the middle button keep the link's own way.
- **Nothing moves above the row, and the fold's pages land with it.** The fold opens below the row. Its state changes in
  the press, not in the browser's later `toggle` event, so its rows draw in the frame the chevron turns. That holds for
  the chevron and for → too: before, both left the fold to the browser, and its rows came one frame after the chevron
  turned (the new press trip caught it on the chevron). A fold whose pages are not all read yet still draws none until
  it holds them all (the ux24 Windows pass), then all at once.

This keeps the 2026-11-19 rule (`2026-11-19-the-docs-rail-holds-its-space.md`): a press opens no fold *around* the page.
The one fold it opens is the pressed row's own, below it.

## The other trees

Checked, and left as they are:

- **Code's file tree, and the generated tree of a folder renderer** (`TfRepoFiles.svelte`): a folder's row is a button
  that only folds, so one click opens it and the next closes it already. Opening a folder's drawing is its own glyph
  beside the row.
- **Assets** (`AssetsView.svelte`): Miller columns. One click selects a row and draws what it holds in the next column;
  there is nothing to fold.
- **Docs' space list and *Hidden spaces*:** a space's head and the hidden fold have no page to open, so a click folds.
- **Settings' nav:** flat, nothing nests.

## Tested

`TfDocs.rail.test.svelte.ts` (first click opens and never closes, second click toggles, the free end, the chevron alone,
`Enter`, → and modified clicks), `TfDocs.parts.test.svelte.ts` (a click on a fold cut short still waits for all its pages).
Eleven mutants each fail a test. The press trip `docs-tree-click` (`--strict`) clicks by real pointer and fails on a row
moved at or above the pressed one, or a frame with the fold open and none of its pages.
