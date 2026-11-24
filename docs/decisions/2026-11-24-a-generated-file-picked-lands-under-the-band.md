# 2026-11-24: A generated file picked in a comparison lands under the band, not behind it

**Reported (Björn, v3.2.9-ux40):** "folder renderer in comparison: the top line where the current filename is shown hides single line files".

## The cause

A folder renderer's generated files in a comparison are keyed `<folder>\u0000<path>` (ux26), so they never collide with the change's own files. The diff view found a file's card with the selector `[data-file="${CSS.escape(key)}"]`, and `CSS.escape` writes U+0000 as U+FFFD, so no generated file's card was ever found:

- A pick (the file list, `j`, `k`) fell back to the card's `scrollIntoView({ block: "start" })`, which puts the card at the scroller's top, under the band stuck there. The card's own head then sticks at the band's foot, over the card's first rows: 38 px at 2560x1440@1.25, the whole of a one-line file's diff. The next file's head showed below it.
- The rail's scroll spy never saw a generated card, so scrolling through them kept marking the file before the folder.

The compare-jump pass of 2026-11-17 fixed the same landing for the change's own files; generated files came later (ux26) with the new key.

## The fix

`cardIn` (`tf/diff/diff.ts`) finds a card by its key: by selector where the key has no U+0000, else by comparing each drawn card's `data-file`. The pick and the spy both use it, so a generated file lands with its head at the band's foot and the rail marks it, as every other file does. Nothing else changes: no new box, no new scroll.

## Checked by

- `TfDiffView.folders.test.svelte.ts`: a generated file of one line picked lands at the band's foot and the rail marks it while the reader scrolls. It fails without the fix (`scrollTop` 0, not 2,956).
- `?fake-rerender-short` (new): generated files of 0, 1 and 2 changed lines at the top, middle and end of the list, long ones between. Each of the 16 files picked in headless Chrome at 2560x1440@1.25: before the fix 12 landed 38 px under the band (the 4 at the scroll's end were stopped lower by it); after, all 16 at the band's foot, marked, their first line uncovered.
