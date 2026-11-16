# 2026-11-16: Zoom is the window's: ⌘ and + / − / 0, ⌘ and the wheel, one level kept on this computer

**Asked (Björn, for ux18):** "make it possible to also zoom in the content with like strg&+ or mousewheel and strg. I
feel that's kind of missing still. I don't know. Maybe also different zoom levels on sidebar/content, as per setting? I
have no idea how to properly make it customizable and what should be resized, use a frontenddesign with opus for it."

## What zooms

**The whole window, through the webview's own zoom** (Tauri's `Webview::setZoom`: WebView2's `ZoomFactor`,
WKWebView's `pageZoom`). The top bar, the page, its sidebars, the status bar, menus, dialogs, toasts and the frames
renderers draw in all grow together. The title bar and the window's buttons are the system's and stay as they are.

Why the window and not a region:

- **A browser zoom is a display scale.** Chromium lays the page out as at a display scale of the display's times the
  zoom (`devicePixelRatio` is the product), so every length the app reads and writes stays in one coordinate space:
  a windowed list's room and anchors, a menu placed from its opener's box, the sidebar edge's drag in client pixels,
  a canvas's backing size. CSS `zoom` on a region (`.tf-main`, a rail) puts that region's boxes in a second space:
  `getBoundingClientRect` answers in the window's pixels, a length written in the region's, and WebKit and Chromium
  disagree on which calls are scaled. Every windowed list (Tickets, Code's tree, Builds, Docs, the Inbox), every
  menu and the new sidebar edge would have to learn both, on two engines.
- **A text-only size breaks the rows.** The windowed lists stand on fixed row heights and estimates in pixels; text
  grown inside them wraps or clips, and every estimate is wrong at once.
- **One gesture, one meaning.** ⌘ and the wheel zoom the window wherever the pointer is, as in every browser, VS Code
  and the chat apps: a gesture that zoomed whichever column the pointer rests on would zoom something else on the next
  try. The one exception is a thing that has its own zoom: over a PDF, ⌘ and the wheel step the PDF's zoom, as a PDF
  viewer's does.

**Not taken: a level per region** (sidebar, content, code). Each would cost the second coordinate space above, and a
sidebar is now as wide as the reader drags it (`side-width.svelte.ts`), which answers most of "the sidebar is too
cramped" without making its words bigger. If code's text alone turns out too small at a level that suits the rest, the
next step is a *Code size* beside *Code font* (an IDE's editor size), scaling `--mono`'s size and the code rows'
heights together; that is its own pass, with the Source view's and the diffs' estimates.

## How a person controls it

- **Keys:** ⌘+ (also ⌘= and the keypad's +) zooms in, ⌘− zooms out, ⌘0 goes back to 100 %. Ctrl off the Mac; either
  modifier on either, as the shell's other keys. Read by the key, so a German board's + and − keys work; with ⌥ (AltGr
  off the Mac) it is typing, not zoom. In a field too: no field has these keys. A key a control already answered stays
  its own.
- **Wheel and pinch:** ⌘ or Ctrl with the wheel; a notch is a step, wheel up zooms in. A trackpad's small deltas add up
  to a step, so a pinch walks the steps. On the Mac a trackpad pinch arrives as WebKit's gesture events and walks the
  same steps.
- **Steps:** 80, 90, 100, 110, 125, 150, 175, 200 %, Chromium's own between those bounds. Not below 80: the type
  scale's smallest, 11 px, is then 8.8 px, and the small control 21 px. Not above 200: the window's minimum width,
  1100 px, is then 550, already the narrow layout.
- **What it shows:** each step shows the level for about a second, an ink plate under the top bar's middle with the
  figure in the job key's narrow heavy Archivo, the one place this pass spends anything, and *⌘0 for 100 %* beside
  it. It takes no pointer and no focus, and says the level to a screen reader. While the level is not 100 % the status
  bar ends in *Zoom 110 %*, which goes back to 100 % when pressed, so a zoom done by accident never goes unexplained.
- **Settings › Appearance › Zoom**, beside *Code font*: the eight steps as one segmented control, the one in use
  pressed, and a line naming the keys.
- **Kept on this computer** (`localStorage`, `knobas.zoom`), as the code font is: the level suits a display, and a
  backup carried to a laptop should not bring a 49-inch screen's level. It is set before the shell mounts, so the
  first frame is at the kept level; the wait is one call to the webview, only when the level is not 100 %. Not per
  page: a page that changed size as it opened would read as a fault.

## What stays fixed, and what composes

- The display's scale multiplies with it: 125 % on a 1440p screen at 110 % lays out as 1.375. Settings shows the
  zoom alone; 100 % is the system's size.
- A sidebar's width is in the window's zoomed pixels, as every other length is: zoomed in, a sidebar grows with its
  words and keeps the names it showed.
- Hit targets scale with everything else; nothing is held at a minimum, because nothing else would keep its size.

## Rows stay on the device's grid at every level

`2026-11-16-a-row-swap-paints-its-own-rows.md` keeps a windowed list's heights and room on the device's layout grid
at the display's scale. A zoom changes that scale, so each step is a scale change: every windowed list re-reads its
drawn rows once (each row observed again, so the observer reports it on the new grid), puts the heights it holds for
undrawn rows on the new grid, and writes its room again (`device-scale.ts`). A PDF's pages are drawn again at the
new scale so they stay sharp. Nothing else is rebuilt: a step is the webview's own relayout.

## Why the wheel costs scrolling nothing

A wheel listener that may cancel holds each scroll's start for the main thread (`return-place.ts` says so). The zoom
listens to the wheel passively, and adds a cancelling listener only while ⌘ or Ctrl is held, and takes it away when
the key comes up or the window loses focus: a scroll with no modifier never meets it.

## Measured

`testenv/perf/zoom.mjs`, headless Chrome 156 on the WSL box of Björn's PC driven from the Mac over a tunnel (the Mac's
timing lock was held), the windowless backend on a fresh clone of the saved scale database with its secrets moved out,
the front end built as the probes build it.

**How a step is made.** Headless Chrome has no webview zoom. A step is ⌘= pressed as real input together with
`Emulation.setDeviceMetricsOverride` to the window's size over the level at its scale times the level: the viewport
and `devicePixelRatio` a zoom gives, every windowed list's re-read, the page laid out again. **That override does not
move Chrome's layout grid** (a text row under an override of 1.375 laid out in 64ths of 1.25's device pixel), so it
times a step but cannot judge a scroll after one. A level's scroll is measured with Chrome started at the product, as a
zoom lays out: `--force-device-scale-factor` 1.375 and a 2327 x 1309 window is 2560 x 1440 at 125 % zoomed to 110 %.

**A step**, ms from the key to the end of its first frame, median of three steps out and back, at 1x / 4x / 6x (the
target is 100 / 100 / 200):

| Page | 1440x868@1, 90 % | 110 % | 150 % | 2560x1440@1.25, 110 % | 5120x1440@1, 110 % |
|---|---|---|---|---|---|
| Tickets' list | 30 / 80 / 150 | 25 / 82 / 119 | 22 / 80 / 115 | 45 / 92 / 144 | 31 / 74 / 122 |
| Code (a repository) | 39 / 54 / 70 | 33 / 45 / 129 | 15 / 75 / 71 | | |
| Builds | 49 / 117 / 194 | 37 / 139 / 190 | 34 / 105 / 154 | 29 / 53 / 85 | 35 / 66 / 97 |
| A Docs space | 19 / 59 / 83 | 34 / 58 / 80 | 34 / 56 / 82 | 17 / 37 / 56 | |

At 1x most of a step is the round trip to the browser and the next frame (the frame itself is 2 to 9 ms). Each step is
one frame: the frames after it did 0.2 to 2.4 ms of work, so the lists' re-read lands in the step's own frame. Builds at
4x on the smallest window is the one cell over its line (105 to 139 ms). That frame is Builds laid out again at the new
width, what any resize of the window costs it: its longest animation frame, 88 to 109 ms, has 15 ms of render and no
script of 5 ms or more. **The zoom's own work costs nothing measurable**: the same steps on a build whose lists do not
re-read at a new scale, alternated with this head, 110 % on 1440x868@1, step / longest frame in ms: Tickets at 4x 75.5,
76 / 72, 66.5 with the re-read and 69.5, 73.7 / 67.2, 66.6 without; at 6x 110, 111.5 and 105.5, 106.4; Builds at 4x
122.5, 114.6 and 110.4, 117.6; at 6x 168.9, 174.6 and 167.2, 182.4.

**Rows hold still at a zoomed scale.** A scroll of 4 s at 3,000 px/s, rows that moved between frames of all row-frames
seen, Chrome started at 1.25 (100 %) and at 1.375 (110 % of it): Tickets' list 40 of 14,208 and 39 of 13,507, Builds 8
of 1,879 and 9 of 9,254 at 1x; at 6x Tickets 39 and 39, Builds 6 and 14. The same few at both, the list's spacer rows
and the swaps the decision of 2026-11-16 leaves. Frames a second at 4x and 6x: Tickets 54.3 / 47.8 at 100 %, 56.5 /
49.8 at 110 %; Builds 52.9 / 44.7 and 58.8 / 56.0. Under the override instead, a third of Builds' rows moved each frame
and its scroll fell to 24 fps at 4x: what the lists would do if a zoom left the grid where it was, which is why the
grid is read again at each step.

**The room lands on its step at every product.** `gridLength`'s length for rooms of 1 to 4,000 rows of 51, 61.1875,
23.75 and 25.90625 device pixels, laid out in Chrome started at 0.9, 1.1, 1.25, 1.375, 1.5, 1.65, 1.75, 1.875 and 2.2:
every one read back on its step (72 to 176 rooms a scale, those under 25,000 CSS px, where a height read back in single
precision can tell one step from the next). `windowed.test.ts` holds the same arithmetic for every display scale times
every level.

## Owed

- **WebView2's own zoom on his PC.** Whether `ZoomFactor` moves the layout grid as `--force-device-scale-factor` does
  (Chromium's page zoom and device scale both feed its layout zoom, so it should): rows holding still under a scroll
  at 110 % in his window is the check. If they move, the lists should read the grid from a laid-out box instead of
  `devicePixelRatio`.
- **A trackpad's pinch.** On the Mac whether WKWebView sends the page its gesture events with magnification off, and
  on Windows whether WebView2 sends a pinch as a wheel with Ctrl while its own pinch zoom is off. Each needs a person's
  fingers.
- **WKWebView's step.** A zoom there is `pageZoom`; its cost was not measured (WKWebView has no DevTools protocol).

## Gates

On the code head rebased on 29ed5d0d (the docs after it change no code), in this pass's own worktree: svelte-check
1,515 files, 0 errors, 0 warnings; vitest whole, 356 files, 5,431 passed, 1 skipped; `doc-lists` and
`conflict-markers` ok; `just check-scripts` ok. Thirty-five mutants, one for each key, step, bound, kept level, wheel
path, listener's coming and going, the plate, Settings, the status bar, the shell's wiring, the lists' re-read, the
scale's watcher and the PDF: each failed a test. Press trips with `--strict` (the Mac's lock was held; WSL's headless
Chrome on the base b45492c5 head and this one, alternated): 284 trips, 279 passing, and the same three right-click
trips failing and two that cannot run (row-room) on the base build too: the right-click trips there predate the menu's
*Link to the job* row, and Linux Chrome cannot run the row-room trip. `testenv/desktop-witness-test.sh` on the WSL box
from this head's archive: 284 checks ok, its macOS helper checks skipped there. `pixel-same.py` not run: at 100 % the
only thing drawn differently is the new row in Settings › Appearance; the plate and the status bar's level draw only
while zoomed. Rust: `capabilities/default.json` only, which knobas-app's build script read and accepted (the release
build got past the crate itself before the disk filled); no Rust code changed.
