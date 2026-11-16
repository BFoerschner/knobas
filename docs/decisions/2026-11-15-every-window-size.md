# 2026-11-15: The speed targets at every window size: full screen and high DPI

**Asked (Björn, on PR #2479, through the orchestrator):** every speed check ran at 1440 x 868 CSS pixels at scale 1,
and nothing had timed another size. A maximised window draws more rows and paints a larger area, and a display scale
of 125 to 200 % multiplies the device pixels. Measure the opens, the keys and the scroll at the common full-screen
sizes and scales at 1x, 4x and 6x, fix what is over a line at any of them, and keep it caught. Narrowed the same day:
"i don't care about narrow layouts, but high dpi is a necessary use case. and also just full screen in general." So no
narrow window was measured or added to a budget.

**How it was measured.** Headless Chrome 156 on the Mac (M2 Pro, on AC power), `Emulation.setDeviceMetricsOverride` with
the size's CSS pixels and device scale, the browser's window that size and `--force-device-scale-factor` the scale, so
the compositor's surface is a full-screen window's (`cdp.mjs`, `--size WxH@scale`). A screenshot of `1920x1080@2` is
3840 x 2160. The windowless backend (release build) on a fresh APFS clone of the saved scale database with its secrets
moved out, the front end built as the probes build it. `run.mjs` (scroll, filter keys, page switches), `keys.mjs` (the
broad keys, 5 presses a case, the median) and `open.mjs` (first visit and return) at each size, one after another.
Headless Chrome draws at 60 Hz whatever the size: a frame rate here is against 60, and what a 240 Hz display asks of a
frame is under *Owed on Windows*.

The Mac was shared with other agents' builds and test runs: a load average of 5 to 8 through the before matrix, and
of 10 to 110 through the after runs (`uptime`, logged each minute). An open's `drawnMs` moved by 100 ms and more from
run to run under that load (Builds' first open at 6x, five runs at one size: 371 to 744 ms), so a single cell over a
line was run again before it was called over, and what the change does was also counted in elements, which no load
moves.

The sizes, each a maximised window:

| Size | What it is | Device pixels |
|---|---|---|
| 1440x868@1 | the reference, every earlier pass | 1.25 M |
| 1920x1080@1, @1.25, @1.5 | a 1080p screen at 100, 125 and 150 % | 2.1, 3.2, 4.7 M |
| 2560x1440@1, @1.25 | a 1440p screen at 100 and 125 % | 3.7, 5.8 M |
| 2560x1440@1.5 | a 4K screen at 150 % | 8.3 M |
| 1920x1080@2 | a 4K screen at 200 % | 8.3 M |
| 1728x1117@2 | a 16-inch MacBook Pro full screen | 7.7 M |

## Before

At 140103aa, the head before this pass, with the load at 5 to 8. Each cell is the worst of its kind at that size
and throttle: the six lists `just frame-budget` scrolls, the six broad keys, every nav place on its first visit and return.

| Size | CPU | scroll: lowest fps, longest task, worst frame work p99 | a frame's paint + GPU, worst | keys: slowest rows | opens: slowest useful (place) | opens: slowest drawn (place) |
|---|---|---|---|---|---|---|
| 1440x868@1 | 1x | 60, 11.8 ms, 7.4 ms | 3.7 ms | 95 ms (tickets-expanded-not-project) | 76 ms (tickets-board first) | 92.7 ms (tickets-board first) |
| 1440x868@1 | 4x | 60, 16.1 ms, 12.1 ms | 4.7 ms | 96.5 ms (everything-letter) | 72.3 ms (repo first) | 244.1 ms (builds first) |
| 1440x868@1 | 6x | 60, 28.3 ms, 17.8 ms | 7.9 ms | 103.6 ms (everything-letter) | 118.8 ms (repo first) | 245 ms (tickets-board first) |
| 1920x1080@1 | 1x | 60, 5 ms, 3.4 ms | 3.7 ms | 72.8 ms (tickets-expanded-not-project) | 85 ms (tickets-board first) | 101.6 ms (tickets-board first) |
| 1920x1080@1 | 4x | 60, 19.3 ms, 13.5 ms | 8.4 ms | 89.9 ms (everything-letter) | 122 ms (repo first) | 184.2 ms (builds first) |
| 1920x1080@1 | 6x | 59.8, 27.3 ms, 20.9 ms | 8.3 ms | 108.8 ms (everything-letter) | 123.2 ms (repo first) | 286.9 ms (builds first) |
| 1920x1080@1.25 | 1x | 60, 6.2 ms, 4.5 ms | 3.4 ms | 73.9 ms (tickets-expanded-not-project) | 95.6 ms (tickets-board first) | 112.3 ms (tickets-board first) |
| 1920x1080@1.25 | 4x | 59.8, 23.9 ms, 17.1 ms | 6.6 ms | 102.6 ms (everything-letter) | 92.7 ms (repo first) | 192 ms (tickets-board first) |
| 1920x1080@1.25 | 6x | 59.5, 31.8 ms, 23.5 ms | 9.1 ms | 117.5 ms (everything-letter) | 126.2 ms (repo first) | 287.3 ms (tickets-board first) |
| 1920x1080@1.5 | 1x | 60, 11.1 ms, 7.2 ms | 3.3 ms | 86.7 ms (tickets-expanded-letter) | 71.9 ms (tickets-board first) | 88.6 ms (tickets-board first) |
| 1920x1080@1.5 | 4x | 60, 20.1 ms, 14.8 ms | 6.3 ms | 90.6 ms (everything-letter) | 90.2 ms (repo first) | 192.6 ms (builds first) |
| 1920x1080@1.5 | 6x | 60, 30.6 ms, 22.8 ms | 9.2 ms | 105.8 ms (everything-letter) | 125.5 ms (repo first) | 272.1 ms (builds first) |
| 2560x1440@1 | 1x | 60, 4.4 ms, 3 ms | 2.8 ms | 89.7 ms (tickets-expanded-letter) | 75.7 ms (builds first) | 92.4 ms (builds first) |
| 2560x1440@1 | 4x | 60, 19.6 ms, 13.5 ms | 8 ms | 99.5 ms (everything-letter) | 99 ms (repo first) | 190.4 ms (builds first) |
| 2560x1440@1 | 6x | 59.8, 28.9 ms, 21.5 ms | 8.8 ms | 113.2 ms (everything-letter) | 147.2 ms (repo first) | 412.4 ms (tickets-board first) |
| 2560x1440@1.25 | 1x | 60, 11.9 ms, 7.7 ms | 4.1 ms | 85.9 ms (tickets-expanded-not-project) | 78.4 ms (tickets-board first) | 95 ms (tickets-board first) |
| 2560x1440@1.25 | 4x | 60, 21.5 ms, 16.7 ms | 6.6 ms | 105.9 ms (everything-letter) | 93.3 ms (repo first) | 183.6 ms (builds first) |
| 2560x1440@1.25 | 6x | 58.5, 33.7 ms, 24.6 ms | 9.7 ms | 116.9 ms (tickets-expanded-not-project) | 110 ms (pull-request first) | 400.8 ms (builds first) |
| 2560x1440@1.5 | 1x | 60, 11.6 ms, 8.2 ms | 4.8 ms | 76.6 ms (tickets-expanded-letter) | 74.7 ms (tickets-board first) | 91.5 ms (tickets-board first) |
| 2560x1440@1.5 | 4x | 60, 22.9 ms, 16.5 ms | 7.3 ms | 97.8 ms (everything-letter) | 99.6 ms (repo first) | 189.2 ms (builds first) |
| 2560x1440@1.5 | 6x | 58.3, 34.7 ms, 26.7 ms | 10.3 ms | 118.6 ms (everything-letter) | 137 ms (repo first) | 282.8 ms (builds first) |
| 1920x1080@2 | 1x | 60, 13.3 ms, 7.8 ms | 4.2 ms | 87.2 ms (tickets-expanded-letter) | 92.1 ms (launcher first) | 108.8 ms (launcher first) |
| 1920x1080@2 | 4x | 60, 20.6 ms, 15.8 ms | 6.9 ms | 106.2 ms (everything-letter) | 82.5 ms (repo first) | 203.6 ms (builds first) |
| 1920x1080@2 | 6x | 59.5, 29.5 ms, 23.4 ms | 10.6 ms | 131.6 ms (everything-letter) | 150.8 ms (pull-request first) | 256.1 ms (tickets-board first) |
| 1728x1117@2 | 1x | 60, 11.9 ms, 7.4 ms | 4 ms | 93.9 ms (tickets-expanded-not-project) | 74.6 ms (builds first) | 91.3 ms (builds first) |
| 1728x1117@2 | 4x | 60, 18.3 ms, 13.9 ms | 7.5 ms | 100.4 ms (everything-letter) | 101.5 ms (repo first) | 185 ms (tickets-board first) |
| 1728x1117@2 | 6x | 59.8, 27.2 ms, 20.8 ms | 8.9 ms | 115.7 ms (everything-letter) | 153 ms (repo first) | 291.6 ms (builds first) |

**Over a line**, of the 1,000 or so cells: Builds' first open at 4x at the reference size (drawn 244 ms, its line 220),
Builds' first open at 6x at 2560x1440@1.25 (401 ms, line 300), the board's first open at 6x at 2560x1440@1 (412 ms),
and one row moved on a run's first open at 1728x1117@2. The board's 412 is the load: its main thread from the press to
its frame was 326 ms at 1440 x 868 and 323 at 2560 x 1440 at 6x in a trace, the same work at both sizes, and every
other size drew it at 245 to 290. The moved row is the run page's banner that TeamCity could not be read (the saved
home has no live TeamCity, so the read behind fails) landing after the first frame, at any size. Builds' first open is the one that grows with the
window: 214 ms drawn at 6x at the reference, 270 to 290 at every larger size, its main thread from press to frame 316
ms at 1440 x 868 and 400 at 2560 x 1440.

**Scrolling** holds at every size and scale: 58.3 frames a second or more at 6x, no task over 35 ms. A frame's paint and
GPU time grows with the device pixels, 3 to 4 ms at 1x at every size and up to 10.6 ms at 6x on 4K at 200 % (the main
thread's paint record slows with the throttle; raster runs on the GPU process). **Keys** hold at every size: the
slowest at 6x 132 ms (*Everything*'s letter on 4K at 200 %), against 200. A tall window draws more cards a column, and
the board's first letter went from 65 ms at the reference to 82 to 98 at the larger sizes at 6x.

## Why Builds grows with the window

Its first open builds every row the window shows: the pipelines rail and the selected pipeline's runs, 887 elements at
1440 x 868 and 1,370 at 2560 x 1440, 153 of them below the fold either way (the overscan). A run row is 19 elements and
already lean. At 6x on a 2560 x 1440 window the main thread ran 430 ms without a break from the press: the kept page and
the rail (120 ms), the windowed lists reading where they sit (45 ms, a forced style and layout), the rail's keep-the-
selection-in-view effect forcing a layout of the half-built page (25 ms) inside a `flushSync`, the runs once their read
answered, the days' lists reading where they sit (55 ms), and the frame (80 ms).

## Taken

- **A first open builds about what the screen shows** (`shell/windowed.svelte.ts`). Every windowed list built 150 to
  600 px of rows below the fold in the tasks that hold a page's first frame. Now, when a list reads where it sits, it
  draws the view, the overscan above it, a quarter of the view's height below it and one row past that; the rest of
  the overscan below waits until two frames have drawn after the last list of the page read where it sits, and grows
  in a task of its own. The quarter is there because rows not yet measured stand at their estimate, which runs a sixth
  over the Inbox's: a span of the view alone fell short of the screen's bottom for a frame. The row past it keeps the
  last drawn row off screen, where the Inbox's `:last-child` border changes no row in view. The read keeps the rows
  drawn before it, by key, where the overscan would hold them, and the undrawn rows' room goes in before the read, so
  a box that clips the list and holds only it (the Inbox's sheet) is not taken for the view's bottom. The press trips'
  `rows-still` caught the first version on three pages (a row going under a clipping footer, the Inbox's border, the
  span falling short); all 16 pass now. Elements in the first frame that shows the rows
  (`main`, at 1x, the base 0998d62d against this head):

  | Page | 1440x868@1 | 2560x1440@1.25 | 1920x1080@2 |
  |---|---|---|---|
  | Inbox | 314 to 244 | 426 to 384 | 356 to 300 |
  | Monitors | 700 to 490 | 994 to 847 | 805 to 637 |
  | Builds | 844 to 824 | 1,327, the same | 1,036, the same |
  | Code | 1,075, the same | 1,075, the same | 1,075, the same |

  Builds gains little: nearly all its rows are on screen, and its rail's overscan is 300 px and a day's 150. Code's
  pull requests draw whole below 100 rows.
- **The probes take a size**, and the budgets run the two heaviest sizes (`testenv/perf/README.md`, *At every window
  size*): `just frame-budget`, `just open-budget --real` and `just keys-budget --real` run 1440x868@1 at 1x, 4x and 6x,
  then `1920x1080@2` and `2560x1440@1.25` at 4x and 6x against the same lines. `run.mjs` records a scroll's paint,
  raster, compositor, GPU and display-compositor time per presented frame and the composited layers' area.

**Not taken.** Builds' rail scrolling its pick into view in the frame's own callback, where its layout read would be
the frame's own rather than one of a first open still being built (16 ms at 6x on a 2560 x 1440 window): a frame
callback that reads the rows before it, as the press trips' recorder does, saw the rail scroll after it had read the
frame, and the scroll is the same either way on screen. Fewer elements a run row: it is 19 already, its actions
(*Log*, *Run again…*) are what the keyboard reaches, and the look is the established one. A smaller overscan: the scroll's own lines were tuned with it (the Windows
pass). The board's first open (245 to 290 ms at 6x at every size, the same at each, so not the window's): its 50 ms
layout of 234 objects is the same at every size and is left to a pass of its own.

## After

On this pass's head, rebased on fc6b7939, from the budget runs themselves (`just frame-budget --real`, `just
open-budget --real`, `just keys-budget --real`), at a load of 5 to 32 (another agent's open budget ran beside them for
part of it). The same cells as the table above.

| Size | CPU | scroll: lowest fps, longest task, worst frame work p99 | a frame's paint + GPU, worst | keys: slowest rows | opens: slowest useful (place) | opens: slowest drawn (place) |
|---|---|---|---|---|---|---|
| 1440x868@1 | 1x | 59.8, 5.3 ms, 3.9 ms | 3 ms | 78.8 ms (tickets-expanded-letter) | 44.2 ms (code first) | 60.8 ms (code first) |
| 1440x868@1 | 4x | 60, 25.3 ms, 15.9 ms | 6.4 ms | 89.4 ms (tickets-expanded-not-project) | 84.2 ms (repo first) | 363.4 ms (tickets-board first) |
| 1440x868@1 | 6x | 57.5, 68.2 ms, 26.5 ms | 8.9 ms | 98.7 ms (tickets-expanded-letter) | 180.9 ms (pull-request first) | 284 ms (builds first) |
| 2560x1440@1.25 | 4x | 59.8, 31.1 ms, 22.7 ms | 12.3 ms | 87.1 ms (tickets-expanded-not-project) | 84.1 ms (repo first) | 190.7 ms (builds first) |
| 2560x1440@1.25 | 6x | 45.3, 68.3 ms, 47.9 ms | 13 ms | 103.5 ms (tickets-expanded-not-project) | 115.3 ms (repo first) | 265.3 ms (builds first) |
| 1920x1080@2 | 4x | 58, 66.8 ms, 44.2 ms | 8.6 ms | 93.2 ms (tickets-expanded-not-project) | 263.6 ms (launcher first) | 280.3 ms (launcher first) |
| 1920x1080@2 | 6x | 55, 40 ms, 27.7 ms | 11.3 ms | 101.7 ms (tickets-expanded-not-project) | 126 ms (repo first) | 349.9 ms (builds first) |

What changed between the two tables is mostly the machine: alternated runs of the base (fc6b7939) and this head under
the same load measure the same scroll (Builds' runs at 6x on 2560x1440@1.25: 52.2 and 56 fps against 58 and 53;
Monitors 52.5 and 52.8 against 50.8 and 54.3) and the same keys. The opens the change reaches drew sooner at 6x,
three alternated runs each at a load of 10 to 20: the Inbox at a median of 86 ms against 147 on 4K at 200 % and of
96 against 144 on 2560x1440@1.25, Monitors 148 against 225 on 4K at 200 %, Builds about even (335 against 372, 360
against 368). A six-size after matrix ran on an earlier revision of the change at a load of 10 to 35 and is left out:
its overs (6x scroll at 33 to 51 fps on lists this pass does not touch) were the load's, and the base measured the same.

**The cells over a line in the budget runs, run again at a load of 5 to 7:** the 6x scroll of Inbox (1440x868, longest
task 68 ms), the tickets list (4K at 200 %, 67 ms at 4x; 2560x1440@1.25, 54.3 fps), Builds' runs (2560x1440@1.25, 45.3
fps) and *Monitors*' filter key (121 ms at 4x) all within; Builds' first open (350 ms at 6x on 4K at 200 %, at a load of
25) 245 ms, and the launcher's first open (264 ms at 4x) 65 ms. One cell stays over: *Monitors*' scroll at 6x on
2560x1440@1.25, below.

## Monitors at 6x on a 1440p window at 125 %: over, ruled to stay red

52 to 54.5 frames a second against the line of 55, longest task 30 to 44 ms (line 60), on the fixture and on the scale
database, and the same on the head before this pass (140103aa), on fc6b7939 and on this head, alternated at a load of
6 to 10; once 58.5 on the quieter night of the before matrix. At 1440x868 and on 4K at 200 % it holds 60.

```sh
node testenv/perf/run.mjs --app "http://127.0.0.1:<app>/?fake-ipc&fake-real=<backend>#/inbox" --out m.json \
  --throttle 6 --kinds scroll --only monitors --size 2560x1440@1.25
```

**Why.** The roster is a windowed table. At the probe's 3,000 px/s the window swaps rows about every sixth frame, and
each swap lays out and repaints the visible table: about 30 ms at 6x (5 ms at 1x) on the 1,361 px list of a 2560 x
1440 window, against about 18 on the 788 px list at 1440 x 868, so on the big window each swap drops a frame.
Main-thread paint is 61 % of the scroll's work, 8.6 ms a frame at 6x against 5.6 at 1440 x 868 (a trace: `Paint`
1,736 ms over the 4 s scroll against 1,012). Ruled out: the strip's SVG mask (`.tf-mbar`; without it, 52.3 and 53.3
fps), and this pass's change (the base measures the same). Containing each row was rejected for Builds' runs
(2026-11-15) for moving text by sub-pixel snapping.

**Ruled (Fable, xhigh, 2026-11-16):** ship both sizes as Björn named them and leave the cell red, the line and the size
list as they are. Dropping the size drops what Björn called necessary; a lower line moves his target, which only he
does; a fix now holds every other cell behind work of unknown size that may change pixels and needs the design review.
The follow-up is the next pass, not backlog, since a budget that stays red teaches everyone to ignore it, and its fix
must stop a swap's cost growing with the list's height rather than retune the overscan to scrape 55: issue #2508.
`just frame-budget` exits non-zero on exactly this cell until then; any other red cell is new.

## Owed on Windows

Only Björn's own window can confirm:

- **240 Hz.** A frame there is 4.2 ms. At 1x a list's scroll took 2 to 8 ms of main-thread work at its 99th percentile
  at every size on the Mac, about twice that on his PC, so a list the main thread scrolls (WebView2 at 100 and 125 %)
  drops frames at 240 Hz where at 60 it does not. Whether WebView2 scrolls it on the compositor at his scale, and at
  what rate the frames land, is his window's to show.
- **4K at 200 % and 150 %.** Headless Chrome on the Mac rasters on the GPU process: 0.6 to 1.8 ms a frame here. His GPU
  and WebView2's raster at 8.3 M device pixels are not measured.
- **Fractional scales** (125 %, 150 %): Chromium on Windows snaps differently at a fractional scale; a scroll's or a
  key's numbers at 1.25 here are the Mac's.

## Gates

On the code head 6123fcb0, in a clean clone rebased on fc6b7939: `just check-front` ok (svelte-check 1,469 files, 0
errors, 0 warnings); vitest whole, 335 files, 5,148 passed, 1 skipped; `testenv/desktop-witness-test.sh` 307 checks;
the press trips with `--strict` 271 of 272, the 16 `rows-still` among them, the one (`pointer-add-to`, a second press on
Time at 600 ms) passing 2 of 2 run again, as on the base; `just frame-budget` on the fixture 45 within at 1440x868 and
its long run within, 36 within on 4K at 200 %, 35 within and *Monitors* over on 2560x1440@1.25; `just frame-budget
--real` 64, 53 and 49 within with 1, 1 and 5 over at a load of 7 to 10, the over scenarios run again at 5 to 7: 36, 28
and 27 within, *Monitors* on 2560x1440@1.25 over; no box that scrolls without an opaque background; `just open-budget
--real` 525 opens, no row moved, every open within at 1440x868 and on 2560x1440@1.25, two over on 4K at 200 % at a load
of 25 and within run again; `just keys-budget --real` 42 of 42, the slowest at 6x 104 ms; `pixel-same.py` over Inbox,
Monitors, Builds, Code's pull requests, Tickets and Docs at 1440x900@1, 1920x1080@2 and 2560x1440@1 against the base,
18 of 18 the same; `just check-scripts` ok. No Rust changed.
