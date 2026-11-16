# 2026-11-16: What the ux14 Windows pass found over target: the board's letter at full screen, Builds' scroll start, READMEs after an upgrade, a repository's first open

**Asked (Björn, on PR #2479, through the orchestrator):** the ux14 Windows pass (v3.2.9-ux14, scale data, PR comments
6909856033 and 6912473584) found four things over target on his PC. A Tickets board letter at 4x at full screen
(5120 x 1440): 116 ms, the answer at 65. Builds at full screen at 6x: runs and a pipeline's history scroll at 210 and 201
frames a second with one gap of 46 and 42 ms each. READMEs after an upgrade: the first sync stored 3 of 63, its 300
requests spent on every branch's tree. A repository's first open: 275, 203 and 372 ms at 1x, 4x and 6x at window size,
325, 230 and 392 at full screen.

**How it was measured.** Headless Chrome on the Mac (M2 Pro, on AC power) at `1440x868@1`, `2560x1440@1.25` and
`5120x1440@1`, the windowless backend (release build) on fresh APFS clones of the saved scale database with their
secrets moved out, the front end built as the probes build it, the base (69a5180d) and the head alternated. The Mac
was shared with other agents' builds and browsers the whole pass: a load average of 25 to 55. Each change is also
counted in what no load moves (cards drawn, requests sent, the order reads go out). The first hour's board numbers were
thrown away: the backend this pass started could not bind its port, which another agent's backend held, and the probes
had measured that backend's database. Ports are checked before a backend starts since.

## 1. The board's letter at full screen

**Why.** The scale board is six columns; at 5120 x 1440 every one is on screen, eleven cards tall. A letter takes each
column from 8 cards to 50, so each windows (`TicketStack`, from 24). A column that windowed once kept its overscan
grown for the session, so every key after drew the view and the whole 600 px below it in the frame the answer lands:
88 cards at 5120 x 1440 where 66 were on screen, 59 at 1440 x 868 where 34 were. Then the cards themselves: an epic's
key was a `TfKey` component in each card (`TfKeyed`), a quarter of a card's script, and a line no word marks drew a
list of one run (`Marked`).

**Taken.**
- **Each new set of a column's cards draws the screen's first** (`shell/windowed.svelte.ts`, `onScreen`). A set is new
  when the list starts to window or a row it drew is no longer where it was; more rows after them (*Show more*) are
  the same set. A new set draws the rows on screen and one past them, and the overscan below two frames later
  (`growSoon`), as a list's first read does. Only the board's columns ask for it.
- **An undrawn card is reckoned at its floor** (`cardFloor`): 66 px and 18 a line of summary at 40 characters a line,
  never taller than a card draws (84.2 px for one line, 102.4 for two, 193.3 for seven at either column width,
  measured), so the column's foot is never empty for a frame. At its floor a card stands at about two thirds of the 136
  px it was reckoned at, so the column's overscan is 400 px, which holds as many cards as 600 did.
- **`TfKeyed` draws a key's span itself**, the same span and classes `TfKey` draws with no link, and **a line with no
  key, or no marked word, is its text alone** (`TfKeyed`, `Marked`). The DOM is the same.

Cards drawn in the frame the answer lands, after a letter (`e`), and in all once the overscan grew:

| Size | before | after |
|---|---|---|
| 5120x1440@1 | 88 (66 on screen), 91 | 70 (66 on screen), 84 |
| 1440x868@1 | 59 (34 on screen), 59 | 38 (34 on screen), 51 |

The draw after the answer (main thread from the board's answer to the end of the frame that paints its cards), medians
of 12 keys, base and head alternated, at a load of 40 to 55:

| Size | CPU | before | after |
|---|---|---|---|
| 5120x1440@1 | 4x | 66.8 ms | 48.2 ms |
| 5120x1440@1 | 6x | 103.8 ms | 93.4 ms |
| 2560x1440@1.25 | 4x | 93.0 ms | 82.9 ms |
| 2560x1440@1.25 | 6x | 163.3 ms | 84.7 ms |
| 1440x868@1 | 4x | 37.5 ms | 29.0 ms |

These are before the `TfKeyed` change, which takes a quarter of each new card's script off. The main thread's work in
the 300 ms after the answer is about the same as before (79 and 81 ms at 4x on 5120 x 1440): the cards below the view
are built two frames later, not in the frame that shows the answer. On the PC the board's answer came at 65 ms and its
draw took 51, so the same share off is about 37: about 100 ms at 4x at full screen, at the line; the 6x key was within.

**Not taken.** Fewer cards a column on screen, or narrower ones: the look is the board's. Holding the cards' marks
until the answer lands (the key's own task is 20 to 30 ms at 4x): the marks appear as the key is typed, and that is
the design's.

## 2. Builds' scroll start at full screen

**What it is.** The runs list at 5120 x 1440 is 4,820 px wide. On the Mac at 6x a wheel scroll of it moves on the
compositor at 60 frames a second, and every 300 px a row swap costs the main thread 33 to 55 ms (script 10 to 18,
layout 7 to 14, paint 5 to 11, layerize 3 to 6). The first swap of a scroll is followed by 13 to 35 ms of the GPU
process's time, the later ones by 4 to 7; a 100 px scroll that swaps no row costs about 3 ms; a second scroll of the
same page pays it again (12 against 4). The Mac shows it as one presented frame missed (a gap of 21 ms) at the first
swap; at 240 Hz on the PC it is the one gap of 42 to 46 ms, and at window size, where the rows are a third as wide, of
21 ms. Ruled out by trace: the running run's animated pulse (hidden, the same), an image (Builds draws none; the strips'
mask images belong to the hidden, pre-built Monitors page), a font, a forced layout.

**Weighed:** a narrower runs column at full screen (fewer pixels a swap; a visible change, and at window size the
gap is still 21 ms, five frames at 239 Hz), a smaller overscan (more, smaller swaps; the first stays), every run drawn
with no window (no swap at all), and recording it for the PC's own trace.

**Ruled (Fable, xhigh, 2026-11-16):** draw every run row, filled in small idle slices after the first paint and capped,
but only if a check on the Mac passes first: with windowing off, no GPU burst over 7 ms follows the scroll's start.
If the burst survives, record it and let the PC's trace decide.

**The check failed.** With every row drawn (windowing off app-wide, a build for the check only), the row swaps' main-
thread tasks went (none over 20 ms in the scroll, where each swap was 33 to 88 ms), but the GPU process still spent 9
and 49.5 ms in the 100 ms after the scroll began, in two runs, against 5 to 6 a 100 ms after it. The burst is Chrome
rastering the tiles ahead of a scroll as it starts, at 4,820 px wide, not the swap. So nothing was built: a list drawn
whole would cost Builds' first open for a gap it does not remove. What the PC's GPU does in that first 100 ms at
5120 x 1440 is under *Owed on Windows*.

## 3. READMEs after an upgrade

**Why.** The stored-trees pass (`knobas_sync::stored_trees`) read each default branch's README after every owed
branch's tree, and a first sync after the upgrade to `0090` owes every branch: its 300 requests went to trees, and the
READMEs waited for the next run.

**Taken.** A default branch's README is read right after its tree, one request, before the next repository's tree or
any other branch: what a landing opens (the default branch's top and its README) is stored first, repository by
repository. The pass's budget is the same 300 requests; a README that does not fit waits for the next run's first
step, as before. Pinned by `stored_trees::a_default_branchs_readme_is_read_right_after_its_tree` (a budget of three:
`main`'s two levels and its README, the rest next run), which fails on the base.

**Measured** on the live Gitea of the WSL box, one pass from a home that stores no tree (the saved scale home):

| One pass, 300 requests | branches stored (default) | READMEs stored | left |
|---|---|---|---|
| before | 300 (62) | 0 | 906 |
| after | 238 (62) | 62 | 968 |

**The cost:** 62 of the long tail's trees wait one more run, one request each. Nothing more is read.

## 4. A repository's first open

**Why.** The landing sent its reads from its components' effects, so they went out after the page was built: 118 ms
after the press at 4x and 166 at 6x on the Mac, behind a build task of 100 to 200 ms. And the header read its own box
to see whether its actions stacked, laying out the half-built page inside that task. On a home whose sync stored the
trees and READMEs (item 3) nothing is read from the forge; the README is a read of the store.

**Taken.**
- **A press on a repository's link reads its landing** (`tf/repo/hover-reads.ts`, `readLandingAhead`): a pointer press
  or Enter starts the record, its code facts, its branches' pull requests, its pipelines (each answer-cached, joined in
  flight or taken kept) and its top listing, commits and branches views (read ahead, kept 3 s after they land for one
  ask to take, `readViewAhead`'s `keepMs`). Each is the call the page makes, with its arguments.
- **The header's read is the observer's first call** (`TfRepo.svelte`, `watchStack`), after the page's layout and
  before its first paint.

`open.mjs`, the first press after the app started, drawn, medians alternated with the base:

| Size | CPU | before | after |
|---|---|---|---|
| 1440x868@1 | 1x | 54.9 ms | 57.5 ms |
| 1440x868@1 | 4x | 130.6 ms | 120.3 ms |
| 1440x868@1 | 6x | 216.8 ms | 198.2 ms |
| 5120x1440@1 | 1x | 54.7 ms | 48.4 ms |
| 5120x1440@1 | 4x | 171.4 ms | 150.1 ms |
| 5120x1440@1 | 6x | 278.9 ms | 246.9 ms |

**What is left** is the page's own build, 100 to 190 ms of one task at 4x and 6x: the repository page and its Files tab
(about 45 ms of script at 6x), the leaving page's scroll noted (`keepScroll`, a forced style and layout of about 20 ms at
6x, since the new page is in the DOM when it reads), and the bridge's own calls. On the PC the README of a landing came
from the forge, then from a second round of IPC (69a5180d), and its first open at 6x was 372 ms; with the README stored
by the first sync and the reads out at the press, expect about 300 at 6x at window size and 330 at full screen, still
the build's. **Not taken:** `keepScroll` from the scroll events' own positions, with no read at the hide: its tests and
`return-place.ts` read the boxes' scroll as it stands, and a box scrolled with no event yet would come back wrong.

## Owed on Windows

- **The board's letter at 4x at full screen**, on his window: the draw's share off the Mac's was 28 %; whether it is
  under 100 there.
- **Builds' first 100 ms of a scroll**, on his GPU at 240 Hz, full screen and at 1456 x 907, at 1x, 4x and 6x, under a
  trace with the GPU process's raster: whether the one gap is the raster ahead of the scroll, as on the Mac, and how
  long it is (Fable's line: under 12.5 ms, three frames).
- **The press's reads in WebView2's IPC**, which has no six-at-a-time limit: a repository's first open at 1x, 4x, 6x.
- **The first sync after an upgrade**, on his profile: every README stored by its first pass.

## Gates

On the final head (in a clean clone):
