# 2026-11-14: What a page costs to build, and the 6x line

**Asked (Björn, on PR #2479):** find out why each page is dear to build on a slow machine and make every element cheaper;
the bar for a slow work laptop is CPU **6x**: a page usable within 300 ms, a key within 200 ms, scrolling with no visible
stall, at the same weight as 100 ms at 1x and 200 ms at 4x. He confirmed 6x as a standing target, so it is written into
the spec (*The speed targets*), `CONTEXT.md`, `testenv/perf/README.md`, the implementer brief, and the three budgets.

**How it was measured.** Headless Chrome on the Mac (M2 Pro) at CPU 1x, 4x and 6x over fresh APFS clones of the saved
scale database (`final-home`, 20,007 tickets, 5,032 pages, 2,620 builds) through a release build of the windowless
backend, the front end built as the probes build it (`KNOBAS_PERF=1`). Opens with `open.mjs` (first open and return),
keys with `keys.mjs` (five presses, the median), scrolling with `run.mjs`, and each page's open under a Chrome trace
(`devtools.timeline`, the layout and style events with their stacks and invalidations, selector statistics) and a CPU
profile of an unminified build. The live products were not used, so the extremes were stood in for: a proxy in front of
the backend gave one Docs page 25 times its blocks (5.3 MB, the shape `docs_page` answers), a run 3,000 artifacts in ten
open folders, and a pull request twenty comments, one of 300 KB of Markdown with 157 fenced blocks.

## Why a page was dear to build

Per element, nothing in the engine was dear: a layout of the 1,400 objects of Builds is about 4 ms at 1x (3 µs an
object), the stylesheet's selectors cost at most 2 ms a style pass in Chrome's own selector statistics (no `:has()`,
descendant chain or `*` rule among the dear ones), and a page's paint is 3 to 8 ms at 4x. Svelte builds an element in
about 20 µs at 1x. The mystery layout of the Windows pass ("50 ms over 14 dirty objects") is a style read: the windowed
lists find the boxes that clip them by asking each ancestor's computed `overflow-y`, and inside a size container
(`container-type: inline-size` on `.tf-shost`, `.tf-runs`) a style read lays the page out first, so the "14 dirty
objects" were the new page's whole first layout (14 to 19 ms at 4x). What made pages dear was work done more than once,
or done for all of something when a part was needed:

1. **A font face landing after the page drew.** A browser loads a face only when text first needs it: JetBrains Mono
   400 the first time a page draws a run's number or a key (Builds, Tickets), Source Serif the first time Docs draws a
   page. It arrived a task after that page drew, and Chromium then laid out every text of the page again: 1,391 of
   Builds' 1,396 objects, 17 to 20 ms at 4x; 987 of a long Docs page's 997, 34 to 48 ms.
2. **Every kept answer deep-cloned twice.** The answer cache copies each read as it lands and again on each open that
   finds it, with `structuredClone`: 38.7 ms at 1x for the 5 MB page's answer, 1.6 ms for Builds' 571 KB of pipelines.
3. **A long Docs page worked out over all of itself at open.** Its reading time joined every block's words into one
   string and split it (56 ms at 4x on 5 MB), and it weighed all of its thousand sections for a first draw that
   reaches a few (19 ms).
4. **A long Markdown text built whole.** A 300 KB comment was parsed, sanitized and inserted at once: at 4x a 194 ms
   task, a 120 ms layout of 8,000 objects, then each of its 157 fences' colours landing laid all of it out again, frame
   after frame, up to 41,600 objects: 797 ms of main thread, 15,442 elements. A description or a README does the same.
5. **A scroller found while the page was being built.** What draws more as it nears the view (`whileNear`) found its
   scroller by the same computed-overflow read, as it was attached, so a long Docs page laid itself out half built in
   the task that built it (56 to 88 ms at 4x).

Not dear, and left: the windowed lists' own placement read (item 5's read, in `placeWindowsNow`) forces the layout the
frame would do anyway, once for all of a page's lists; moving it after the layout measured the same. A kept page's
scroll restore forces a layout as the page shows (21 ms at 4x on a pull request), but the place a return sets comes
after it and needs it done.

## What changed

- **Every face loads while the app is idle** after its first draw (`shell/faces.ts`, from the shell's mount): the one
  relayout it costs falls on the page the app opened on, before anything is pressed.
- **A kept answer is copied by walking its JSON** (`answer-cache.ts`, `copy`): five to eight times faster than
  `structuredClone` (4.7 against 38.7 ms at 1x for 5 MB, 0.3 against 1.6 for Builds); anything not plain data in it is
  still cloned.
- **A Docs page counts its words block by block** (the same count) and weighs a section when its draw first reaches it.
- **A long Markdown text draws in pieces** (`render/markdown.ts`, `markdownPieces`; `TfMarkdown.svelte`): over 24,000
  characters, it is parsed whole, so references, list numbering and task boxes come out as before, then rendered,
  sanitized and drawn about 12,000 characters of top-level blocks at a time, the first at once and the rest as the end
  of what is drawn nears the view, as a long Docs page's body is. A piece never ends inside raw HTML a block opens and a
  later one closes (a `<details>` around paragraphs). An in-page anchor draws the rest first, and the find bar counts a
  held body's matches as a floor (`data-md-held`), as it does over a windowed list. The 300 KB comment's open: 217 ms
  of main thread at 4x, its longest task 98 ms, 803 elements.
- **`whileNear` finds its scroller after the mark's first layout** (a resize observer's first call, where the read
  costs nothing), and the intersection observer it makes reports in the same frame.
- **The 6x line is in all three budgets** and in each recipe's default run: `budget-open.json` holds every nav place
  used and drawn within 300 ms at 6x, `budget-keys.json` every key within 200 ms, `budget.json` every list scrolling at
  55 frames a second or more with no task over 60 ms, a key painted within 200 ms and a switch within 300. `open-budget`
  gives its watchdog 45 minutes a throttle, since three throttles outran the 45 it had. The inner pages and the extremes
  stay at 1x and 4x: their items live on the forge and are named per throttle.

## The known slow spots

| Spot (stand-in data, 4x) | Before | After |
|---|---|---|
| a 5 MB Docs page's open: main thread / longest task | 428 / 379 ms | 270 / 193 ms |
| a 300 KB comment's Conversation: main thread / longest task / elements | 797 / 213 ms / 15,442 | 217 / 98 ms / 803 |
| a run of 3,000 artifacts scrolled | 60 fps, no frame dropped, longest task 12.5 ms (6x: 60 fps, 19 ms) | the same: the Windows pass's windowing had fixed it |

The 5 MB page's parse itself (the answer's JSON) is a 58 ms task at 4x, before any of the page's work; what was 536 ms
in one task was mostly the clone, the word count, the weights and the half-built layout above.

## Before and after

**Main thread to build each page**, from a trace of its open, both builds traced one after the other on the same
backend so the machine's load falls on both alike (one open each; a trace adds about 10% to every number):

| Page | 4x main thread ms, before → after | 4x longest task | 4x layouts | 6x main thread | 6x longest task | elements |
|---|---|---|---|---|---|---|
| Builds | 240 → 157 | 104 → 87 | 67 → 39 | 315 → 285 | 161 → 158 | 777 → 777 |
| Code | 181 → 168 | 43 → 47 | 23 → 13 | 315 → 237 | 68 → 57 | 1062 → 1062 |
| Docs | 121 → 79 | 63 → 45 | 26 → 20 | 163 → 138 | 82 → 82 | 289 → 289 |
| a Docs page | 97 → 99 | 30 → 40 | 24 → 32 | 145 → 153 | 45 → 59 | 221 → 221 |
| a 5 MB Docs page (stand-in) | 348 → 208 | 170 → 84 | 161 → 75 | 398 → 327 | 186 → 137 | 497 → 497 |
| a space | 108 → 92 | 41 → 39 | 19 → 21 | 167 → 142 | 65 → 59 | 169 → 169 |
| Inbox | 125 → 71 | 92 → 51 | 15 → 4 | 113 → 116 | 80 → 82 | 282 → 282 |
| Monitors | 118 → 119 | 88 → 86 | 15 → 14 | 200 → 172 | 147 → 127 | 674 → 674 |
| a pipeline | 105 → 96 | 38 → 37 | 22 → 22 | 149 → 134 | 56 → 55 | 872 → 872 |
| pull request (its source down: one line) | 52 → 51 | 31 → 31 | 3 → 5 | 81 → 79 | 50 → 47 | 26 → 26 |
| repository (its source down) | 74 → 69 | 35 → 34 | 4 → 5 | 125 → 111 | 56 → 51 | 61 → 61 |
| a run | 55 → 52 | 36 → 32 | 3 → 3 | 81 → 77 | 53 → 50 | 330 → 330 |
| Settings | 55 → 46 | 42 → 36 | 5 → 5 | 71 → 69 | 57 → 53 | 90 → 90 |
| Standup | 88 → 84 | 40 → 39 | 12 → 11 | 130 → 129 | 56 → 58 | 520 → 520 |
| a ticket | 75 → 74 | 28 → 28 | 4 → 4 | 118 → 123 | 44 → 47 | 76 → 76 |
| Tickets board | 259 → 177 | 66 → 41 | 80 → 55 | 272 → 271 | 62 → 64 | 525 → 525 |

**Drawn after the press** (`open.mjs`, the frame after the first useful one, first open / return, ms). Before: the
branch head `14279bb3`, from runs of the first and return rounds; after: this pass's head, from the gate run of `just
open-budget --real`, which presses every round. Read a first open at 1x with care: the gate's earlier rounds leave every
section's reads in the warm list, so Builds' and the board's first opens find their answers there, where the before
runs waited on the backend (Builds' `pipelines`, below). The machine was shared with other agents' builds on both
days, so a cell moves 10 to 20 ms either way between runs; the paired traces above are the steadier comparison.

| Place, drawn ms: first open / return | 1x before | 1x after | 4x before | 4x after | 6x before | 6x after |
|---|---|---|---|---|---|---|
| Inbox | 42 / 47 | 43 / 46 | 68 / 39 | 72 / 43 | 84 / 46 | 94 / 48 |
| Tickets board | 99 / 46 | 54 / 42 | 190 / 45 | 155 / 45 | 239 / 69 | 225 / 61 |
| a ticket's preview | 27 / 29 | 31 / 31 | 22 / 30 | 26 / 30 | 21 / 16 | 20 / 29 |
| a ticket | 40 / 45 | 49 / 46 | 70 / 40 | 78 / 42 | 100 / 40 | 110 / 40 |
| Code | 58 / 48 | 44 / 47 | 101 / 44 | 90 / 47 | 138 / 61 | 174 / 62 |
| a repository | 55 / 46 | 58 / 48 | 47 / 46 | 90 / 46 | 156 / 44 | 140 / 43 |
| a pull request | 43 / 48 | 42 / 47 | 81 / 42 | 76 / 47 | 101 / 52 | 110 / 33 |
| Docs | 38 / 48 | 46 / 47 | 86 / 44 | 66 / 46 | 122 / 46 | 118 / 37 |
| a space | 22 / 30 | 28 / 31 | 19 / 32 | 31 / 29 | 37 / 26 | 21 / 17 |
| a Docs page | 43 / 48 | 57 / 48 | 120 / 46 | 90 / 48 | 163 / 76 | 153 / 61 |
| a Docs page by Docs' entry | - / 45 | - / 44 | - / 43 | - / 47 | - / 41 | - / 44 |
| Builds | 110 / 46 | 40 / 37 | 220 / 46 | 155 / 44 | 267 / 67 | 207 / 73 |
| a pipeline | 26 / 30 | 22 / 31 | 32 / 25 | 23 / 29 | 29 / 24 | 18 / 27 |
| a run | 41 / 47 | 44 / 48 | 52 / 44 | 78 / 47 | 95 / 56 | 81 / 45 |
| Monitors | 42 / 46 | 47 / 45 | 116 / 42 | 101 / 46 | 172 / 44 | 150 / 44 |
| Standup | 51 / 47 | 43 / 47 | 37 / 60 | 76 / 62 | 122 / 93 | 128 / 78 |
| Time | 40 / 48 | 38 / 42 | 37 / 43 | 30 / 40 | 39 / 42 | 43 / 45 |
| Settings | 45 / 47 | 34 / 46 | 40 / 47 | 37 / 46 | 61 / 44 | 73 / 47 |
| the launcher | 44 / 24 | 78 / 24 | 68 / 22 | 71 / 5 | 70 / 31 | 77 / 24 |

**Keys** (`keys.mjs`, rows drawn after the key, median of five presses before and seven after, the after from the
gate run of `just keys-budget --real`; nothing drawn moved in any press). A key waits mostly on the backend's read
(`answered`, 49 to 97 ms), so the page's part was already small; every key is inside 200 ms at 6x on the Mac. The letter
under *Everything* went up at 6x with another agent's change to its ranking, merged between the two runs, and within its
line.

| Key, rows drawn ms (median) | 1x before / after | 4x before / after | 6x before / after |
|---|---|---|---|
| Tickets, a first letter | 70 / 68 | 81 / 78 | 95 / 89 |
| a `-project:` | 72 / 73 | 79 / 80 | 88 / 88 |
| a `-priority:` | 60 / 56 | 79 / 75 | 80 / 74 |
| the expanded board, a letter | 106 / 90 | 115 / 107 | 122 / 111 |
| the expanded board, a `-project:` | 117 / 89 | 111 / 109 | 129 / 121 |
| a letter under *Everything* | 80 / 74 | 108 / 94 | 110 / 127 |

**Scrolling at 6x** (`run.mjs`, a script-driven scroll of 3,000 px/s for 4 s that turns at the list's end: frames a
second, frames dropped, longest task). The line is 55 frames a second and no task over 60 ms.

| List at 6x | before | after |
|---|---|---|
| Tickets board (expanded) | 60, 0%, 23 ms | 56.5, 5.8%, 37 ms |
| Tickets list | 60, 0%, 24 ms | 58.5, 2.5%, 35 ms |
| Inbox | 60, 0%, 21 ms | 60, 0%, 22 ms |
| Code's pull requests | 60, 0%, 23 ms | 60, 0%, 18 ms |
| a space | 60, 0%, 32 ms | 60, 0%, 19 ms |
| a large Docs page | 58, 3.3%, 55 ms | 58.5, 2.5%, 45 ms |
| Builds, a pipeline's runs | 43.8, 27%, 81 ms | 44.3, 26%, 69 ms |
| Builds' rail | 60, 0%, 20 ms | 59, 1.7%, 38 ms |
| a pipeline's history | 56.5, 5.8%, 37 ms | 54.8, 8.8%, 51 ms |
| Monitors | 60, 0%, 29 ms | 60, 0%, 27 ms |

Nothing in this pass changes a scroll; the two columns differ by the machine's load (other agents' builds; the before
column was taken at a load of 4 to 6, the after at 8 to 15). Builds' runs, run on both builds side by side at a load of
about 5, scrolled at 57.8 frames a second on each, 3.8% dropped, longest task 34 and 36 ms.

## Still over, and why

On the Mac every open, key and scroll the three budgets hold is inside its 6x line (`just open-budget --real`: the
slowest of 75 opens drawn at 225 ms at 6x; `just keys-budget --real`: the slowest key 127 ms). Two are close enough to
miss on a slower machine or a busy one:

- **Scrolling a pipeline's runs at 6x** (Builds' main list, and a pipeline's history) is inside the line on a quiet
  Mac (57.8 frames a second) and outside it on a loaded one (44). Each time the window moves, a day's runs table adds
  and drops rows, and a table lays out every drawn row again: about 2 ms at 1x and 15 ms at 6x, with 6 ms of paint, so
  those frames take 30 ms at 6x. Containing the scroller saves nothing (measured: 1.5 against 1.7 ms at 1x); the cost
  is the table. Drawing a day's runs as rows that are not a table, or windowing them with fewer moves, is the fix, and
  it changes what is drawn, so it is a pass of its own through the design review. Not in `just frame-budget`'s default
  list, so the gate does not see it; `--only builds-main,pipeline-history` does.
- **The first opens with most to build**, at 6x on the Mac: Builds 207 ms, Tickets' board 225, Code 174, a Docs page
  153. On Windows, at about twice the Mac's time, those are about 300 to 450 ms: the next Windows pass will say.

Two things this pass found and did not change, both outside what a page costs to build:

- **Builds' first open in a fresh session waits on its reads.** `pipelines` takes 199 ms in the backend at the median
  (p90 287) for 91 pipelines with their last twelve runs, `pipeline_runs` 66. At 1x Builds is useful only at 95 ms,
  at 4x at about 150, because of that read, not the page. The app's warm list asks it at start from the second session
  on, so it bites a first start and a probe's fresh profile. It is a query to make cheaper in the backend.
- **The windowed lists' placement read** forces the page's first layout in the task that builds it. It is the same
  layout the frame would do, so it costs nothing extra; it only makes one task longer instead of two shorter.

## What only Windows can tell

- **The real times.** WebView2 on Björn's PC ran the same pages in about twice the Mac's time at the same throttle, so a
  Mac number at 150 ms at 6x is about 300 on his PC, and a slow laptop's 6x is the thing the line stands for. The 6x
  opens, keys and scrolls want measuring in knobas's own window there, as the Windows pass measured 1x and 4x.
- **Fonts.** Whether WebView2 lays a page out again when a face lands as Chromium does, and how long DirectWrite takes
  to shape a long Docs page's first screen (25 ms of a 56 ms layout at 4x on the Mac).
- **A big answer over Tauri's IPC.** The windowless backend answers over HTTP; in the app a 5 MB answer is read through
  Tauri's own channel and parsed in WebView2, which the Mac's headless Chrome does not see.
- **Scrolling on the compositor at 6x.** Headless Chrome on the Mac scrolls every list on its compositor; whether
  WebView2 at 100% scale does at 6x, and the frames a 240 Hz display shows, need the PC.
- **The pages the live products feed:** a repository's Files and Branches, a pull request's *Files changed*, a run's
  Log and Tests. They read the forge or TeamCity, which this pass did not use; the inner pages' set (`--inner`) at 6x
  is the next pass on the live products.

## Gates on the pass's head

On `fdb0062f` in a clean clone, over a fresh clone of the scale database through a release build of the windowless
backend at that head: `just check-front` ok (svelte-check 1,463 files, 0 errors, 0 warnings; vitest 330 files, 5,112
passed, 1 skipped); `testenv/desktop-witness-test.sh` 307 checks; the press trips with `--strict` 272 of 272, the 16
`rows-still` among them; `just open-budget --real` every open within its line, 75 a throttle at 1x, 4x and 6x, no row
moved; `just keys-budget --real` 18 of 18; `just frame-budget` (the fixture) 45 within and its long run within;
`just frame-budget --real` 53 within and 2 over at 4x on a loaded machine (a filter key at 105 ms, a return to Monitors
at 270 with one 154 ms task, both inside the line at 6x in the same run), both within on the rerun of those two;
`pixel-same.py` over 17 screens, the 7 that differ by 2 to 431 pixels, all under its 600-pixel line for one build shot
twice; `just check-scripts` ok. No Rust changed.
