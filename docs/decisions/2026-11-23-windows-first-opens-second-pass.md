# The Windows first opens, second pass: the list, a run, the board (2026-11-23)

The ux30 end gate's Windows pass still had first opens over the PC's line (100 / 200 / 300 ms at CPU 1x / 4x / 6x) in
knobas's own WebView2 window on Björn's PC after the first fix pass: the list of every ticket (4x 200 to 215, 6x 298 to
342), the board of every ticket (6x 282 to 324), a ticket (6x 294 to 356), a run opened from a repository's Builds tab
(6x 302 to 337) and a long asset column at full screen (6x 303). Fable ordered the work: the list's mount, the run page
without its rail work first, the board, then the asset column.

## What the traces showed, and what changed

Each from a trace of the first open at 6x in the window (`Tracing` over the DevTools protocol, the CPU profile mapped
through a source-mapped build of the same commit).

- **The list restyled every row in the frame that paints them.** Tickets' scroller wrote the filter bar's height to
  `--tf-fbar-h` on each first layout, its stylesheet fallback of 47 px included; a custom property on the scroller is
  inherited by every row, so the frame's observer callback restyled the whole list (37 ms). It is written only when the
  height changes from what is in force (`tickets/fbar-height.ts`).
- **The list made the browser search the system's fonts.** Every row's status mark and the sorted column's head drew ▾
  and ▴ (U+25BE, U+25B4) through `content:`, which no bundled Inter file holds: three round trips to the browser's font
  service in one shape of 48 ms. The ticket page's status and facts drew the same ▾. All are CSS triangles now, in the
  text's colour and size. `fonts/content-glyphs.test.ts` holds every character a stylesheet draws to Inter's ranges; ✓
  and ↗, left by the card-drag note's sweep, are named there as left.
- **Builds' rail laid out a run's whole page in the task that mounted it.** Keeping the selected pipeline in view
  (`reveal`, `scrollIntoView`) forced style and layout of the new page (135 ms) while the run's reads waited behind it.
  It runs after the frame's layout now and before its paint (`shell/after-layout.ts`), so the rail is still painted
  scrolled to its pipeline.
- **A run's mount sent six listen and unlisten messages.** The pipeline page's three listeners went and the run page's
  three came for the same events. Builds' ports now share one bridge listener per event for the window
  (`ipc/shared-listen.ts`); a page leaving its set sends nothing.
- **A list that draws the rows on screen drew its overscan's rows before reading where it sits.** The board's columns
  drew 400 px of cards and the read changed them again in the same task (22 ms); an asset's column drew 1800 px of rows
  where a window of 900 shows 34. Before its read such a list draws a window's height of rows and one past
  (`shell/windowed.svelte.ts`, `onScreen`), unless a key's walk already moved its view.

## Proof

Each change has a test that failed before it: `fbar-height.test.ts` (three cases), `content-glyphs.test.ts`,
`shared-listen.test.ts`, and the windowed case "an on-screen list draws, before it reads where it sits, a window's
height of rows and one past". In the window, fresh app per run, alternated with the build before these changes, at 6x:
the list of every ticket 405 ms drawn (median) to 257 to 294; a run from a repository's Builds tab at full screen 367 to
about 300. The board's drawn time did not move: its landing frame's eight cards a column are the cost. Fable's ruling
on the rest is in the PR.
