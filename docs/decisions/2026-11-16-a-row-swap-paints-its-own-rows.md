# 2026-11-16: A row swap paints its own rows: Monitors at 6x on a 1440p window at 125 %

**Asked (Björn, issue #2508, through the orchestrator):** *Monitors*' scroll at 6x on `2560x1440@1.25` held 52 to 54.5
frames a second against its line of 55, the one red cell of the sizes pass (`2026-11-15-every-window-size.md`), ruled
to stay red until the page was fixed. Make a row swap cost only the rows it adds and removes, at every size, keep what
is drawn the same, then check every other long windowed list at the budget sizes at 6x.

**How it was measured.** Headless Chrome on the Mac (M2 Pro, on AC power) at `1440x868@1`, `1920x1080@2` and
`2560x1440@1.25`, the windowless backend (release build) on a fresh APFS clone of the saved scale database with its
secrets moved out, the front end built as the probes build it, base 98892a5e against this head, alternated, at a load of
2.5 to 4.5. `run.mjs` for the frame rates, and Chrome traces of the 4 s scroll with the invalidation categories: each
layout's dirty objects, each `Paint`'s rect, `PaintImage` (a strip drawn again), `PaintInvalidator` and
`InvalidateDisplayItemClient` (what was invalidated, and why), split into frames that swap rows and frames that do not.

## Why a swap painted the whole table

Not the table's layout: a swap laid out 277 of 2,108 objects, 4 ms at 6x, at every size, and the sticky heads, the
table's clip and the strips' mask each changed nothing. **The rows that stayed moved.** At a display scale of 1.25 a row
is 51 device pixels, 40.8 CSS pixels, and Chromium lays out in 64ths of a device pixel; a `ResizeObserver` tells a
height cut to a 64th of a CSS pixel, 40.796875. The room above the drawn rows, the spacer's height, was the sum of those,
so each swap that moved eight rows into it came out 2/64 of a device pixel short and moved every row that stayed by
-0.025 CSS px (all 56 drawn rows, every swap, read back in the page; at scales 1 and 2 none moved). A row that moves is
painted again: on a swap frame 123 strips were drawn again where 16 are new, and paint took 23 to 29 ms at 6x against
about 8 at the other sizes. The taller the window, the more rows were painted again, so the cell missed at 1440p and
not at 1440 x 868.

**And every frame of a scroll walked the drawn rows.** The scrollbars are styled (`::-webkit-scrollbar`), so the main
thread paints them; a scroll moves a thumb (`InvalidateDisplayItemClient`, *scroll control*, twice a frame, the only
thing invalidated on a frame without a swap) and the scroller's layer is painted again, and with the page's content in
that layer every drawn row was walked again: 0.6 ms a frame at 1x on Monitors at 1440p, 3 to 4 ms at 6x, on every
frame. With the content in a layer of its own it replays as one cached piece.

## Taken

- **Heights and room on the device's grid** (`shell/windowed.ts`, `windowed.svelte.ts`). A drawn row's height is read
  from its box, which carries it whole, onto the grid (`laidOut`, `onGrid`; the observer's where a transform scales the
  box), and the room is written so it lands on its step (`gridLength`). Chromium takes a length to single precision
  and cuts it down to a step (measured on the room's own shape, 2026-11-16), so the length aims at the middle of the
  step and moves by the least single precision can where that carries it off; whole and power-of-two scales write the
  plain length. That holds up to about 2^17 device pixels of room (105,000 CSS px at 125 %); past it single precision
  cannot hit every step and a swap there may move the rows by one, painted again as before. Every windowed list
  shares it: Monitors, the Inbox, Tickets, Code, Builds, Docs.
- **A scroller's content is a paint layer of its own** (`chrome.css`, `tickets.css`): the shown page in `.tf-main`,
  `TfScroller`'s clip (`.tf-sclip`), and each board column, `position: relative` with no z-index, so no stacking
  context and no change in what draws over what (the filter bar's z-index 5 and the heads' 3 keep them above). Every
  page was checked for an element placed against a box outside its wrapper: one, a screen-reader label on Code, 1 px
  and clipped. Not `isolation`: a stacking context would put a dialog, menu or toast drawn inside a page under the
  shell's positioned chrome.

**Not taken.** Containing each row (`contain: layout paint`): rejected for Builds' runs for moving text by sub-pixel
snapping, and with rows that never move there is nothing left for it to save. A smaller overscan: the issue's
acceptance asks for a swap whose cost does not grow with the window, which this is. Standard `scrollbar-color` in
place of the styled bars: the compositor would move the thumb with no main-thread paint, but the bars would look
different, which is the design's to rule.

## Before and after

*Monitors*, `run.mjs` on the scale database, base and head alternated: frames a second, frame work p50 / p99, and a
frame's main-thread paint.

| Size | CPU | before | after |
|---|---|---|---|
| 2560x1440@1.25 | 1x | 59.8, 0.6 / 4.9 ms, paint 1.37 ms | 60, 0.3 / 3.2 ms, paint 0.54 ms |
| 2560x1440@1.25 | 4x | 60, 2.2 / 17.4 ms, paint 4.07 ms | 60, 1.1 / 12 ms, paint 1.66 ms |
| 2560x1440@1.25 | 6x | 56.5, 3.5 / 26.9 ms, paint 7.75 ms (52.8 to 57.3 in six runs at a load of 2.5 to 6) | 60, 1.7 / 18.1 ms, paint 3.08 ms (59.8 to 60 in seven runs at the same loads) |
| 1920x1080@2 | 1x | 60, 0.5 / 2.8 ms, paint 0.86 ms | 60, 0.3 / 3.8 ms, paint 0.54 ms |
| 1920x1080@2 | 4x | 60, 2 / 11.1 ms, paint 2.64 ms | 60, 1.1 / 11.1 ms, paint 1.51 ms |
| 1920x1080@2 | 6x | 60, 2.9 / 17.7 ms, paint 4.86 ms | 60, 1.6 / 18.8 ms, paint 2.86 ms |
| 1440x868@1 | 1x | 60, 0.5 / 2.9 ms, paint 0.8 ms | 60, 0.3 / 3 ms, paint 0.45 ms |
| 1440x868@1 | 4x | 60, 1.8 / 10.6 ms, paint 2.36 ms | 60, 1 / 11 ms, paint 1.44 ms |
| 1440x868@1 | 6x | 60, 2.7 / 15.5 ms, paint 4.43 ms | 60, 1.6 / 16.9 ms, paint 2.55 ms |

A swap at 6x, from the traces (paint of a swap frame, strips drawn again, pre-paint; paint of a frame with no swap):

| Size | before | after |
|---|---|---|
| 2560x1440@1.25 | 22.8 ms, 123 strips, 3.8 ms; 4 ms | 8.4 ms, 16 strips, 1.5 ms; 1.1 ms |
| 1920x1080@2 | 8.3 ms, 16 strips, 1.7 ms; 3.7 ms | 8.1 ms, 16 strips, 1.8 ms; 1 ms |
| 1440x868@1 | 6.9 ms, 17 strips, 1.3 ms; 3.1 ms | 7.8 ms, 17 strips, 1.5 ms; 0.9 ms |

A swap now costs the same at every size: its eight rows' style (170 elements, 6.2 ms at 6x), layout (4 ms) and paint.
At 1440 x 868 the page's composited layers went from 13 to 9 and their area from 16.3 to 29.6 device megapixels (the
compositor rasters only the tiles near the view; GPU time a frame went down, 1.5 to 1.2 ms); at the other sizes
the same.

**The other long lists**, 6x on `2560x1440@1.25`, base, then the grid alone, then this head: frames a second, frame
work p99, a frame's paint.

| List | before | grid alone | after |
|---|---|---|---|
| Tickets' board | 59.5, 16.5 ms, 9.56 ms | 59.5, 16.4 ms, 8.62 ms | 60, 10.7 ms, 7.13 ms |
| Tickets' list | 56.8, 28.6 ms, 6.27 ms | 60, 22.4 ms, 5.44 ms | 60, 22 ms, 2.25 ms |
| Inbox | 60, 14 ms, 1.77 ms | 60, 14.6 ms, 1.78 ms | 60, 15.6 ms, 1.95 ms |
| Code's pull requests | 60, 14.5 ms, 5.06 ms | 60, 13.4 ms, 5.11 ms | 60, 16.2 ms, 2.29 ms |
| A Docs space | 60, 9.8 ms, 1.9 ms | 60, 9.3 ms, 2.1 ms | 60, 11 ms, 1.04 ms |
| A Docs page | 56.3, 14.8 ms, 5.7 ms | 56.3, 16.8 ms, 5.78 ms | 56.3, 15 ms, 2.51 ms |
| Builds' runs | 59.3, 23.8 ms, 8.61 ms | 59.5, 17.1 ms, 7.07 ms | 60, 16.7 ms, 4.57 ms |
| Builds' pipelines | 59.5, 14.7 ms, 6.4 ms | 60, 10.3 ms, 6.09 ms | 60, 9 ms, 2.85 ms |
| A pipeline's history | 59, 28.2 ms, 8.75 ms | 60, 17.7 ms, 7.36 ms | 60, 18.8 ms, 5.47 ms |

None was over its line before; each paints less a frame now. A Docs page's 56.3 is a 40 ms task of its own in the
scroll, the same on the base. Builds keeps about 0.75 ms a frame of layerize at 1x and a running run's pulse (an SVG
circle restyled every frame) that paints whatever the scroll does.

**Pixels.** Sixteen screens (Monitors at its top, part way, after 120 scroll steps, at its end, a row hovered, the
unwatched tab; the Inbox, Code's pull requests, Builds, a pipeline, the board, the Tickets list, a Docs space and page,
Time, Settings) shot on the base and this head at the three sizes, `pixel-same.py`: at 1440 x 868 seven the same and
nine within a colour step of 3 (a positioned layer's anti-aliasing, invisible); at 1440p 125 % and on 4K at 200 % the
lists that had scrolled differ where the base's rows had drifted or its room came from the observer's cut heights,
the same rows and words a fraction of a pixel apart (the swaps' drift itself, now gone); at the top of each list the
same. Nothing changed that the eye can see, so nothing went to the design review.

## Owed on Windows

- **240 Hz.** A frame there is 4.2 ms. The scroll's paint a frame at 1x went from 0.8 to 1.4 ms to 0.45 to 0.54 ms on
  the Mac, about twice that on the PC: whether WebView2 at 125 % now holds 240 on Monitors is his window's to show.
- **WebView2's own cut.** The room's length was measured against Chrome 156 on the Mac; WebView2 is Chromium and lays
  out the same way, but a fractional scale's rows holding still under a scroll there is his window's to confirm.
- **The styled scrollbar on Windows.** Whether WebView2 paints its thumb on the main thread as Chrome on the Mac does,
  and so whether the positioned content saves there what it saves here.

**Found on Windows (the ux16 pass, 2026-11-17).** One clip may not be positioned: Builds' (`.tf-shost > .tf-bmain >
.tf-sclip`), which keeps a gutter and is painted in `--paper` for ClearType's sake (`0998d62d`). Positioned, WebView2
hit-tested every wheel event over it on the main thread (`FindScrollTargetOnMainThread`, 40 of 40), and Builds' runs
and a pipeline's history moved at 71 and 81 pictures a second under a wheel at 4x, 30 and 31 at 6x, where ux14 held
237 and 229. With that one clip static (`builds.css`), 0 of 40 and 236 and 237 at 4x, 224 and 229 at 6x, in
knobas's own window on the ux16 build. Every other clip stays positioned: their lists stayed on the compositor at
235 to 240 at 1x, 4x and 6x.

## Gates

On the code head (6ced3d31; the docs after it change no code), in a clean clone: `just check-front` ok
(svelte-check 1,473 files, 0 errors, 0 warnings; vitest whole, 338 files, 5,190 passed, 1 skipped);
`testenv/desktop-witness-test.sh` 307 checks; the press trips with `--strict` 272 of 272, the `rows-still` trips
among them; `just check-scripts` ok. `just frame-budget` on the fixture: 45 within at 1440x868 and its long run within,
36 within at 2560x1440@1.25 (*Monitors* at 6x within), 35 within and *Monitors* at 6x on 4K at 200 % at 52.5 at a
load of 7 to 10, run again alone at 3.6 to 5: 59.8 and 59 (the base 59.8 and 55.5). `just frame-budget --real`: 65,
54 and 52 within, with *Monitors* (53.8) and Builds' runs (49.2) at 6x on 2560x1440@1.25 over at a load of 5 to 6, run
again alone alternated with the base at 3.5 to 6: *Monitors* 60 and 60 (base 57, 57.3), Builds' runs 59.8 and 60 (base
57.7, 56.8). No box that scrolls without an opaque background. `just open-budget --real`: 525 opens, every one within
at all three sizes, no row moved. `just keys-budget --real`: every key within at all three sizes, the slowest at 6x
89 ms. `pixel-same.py` at the three sizes, above. No Rust changed.
