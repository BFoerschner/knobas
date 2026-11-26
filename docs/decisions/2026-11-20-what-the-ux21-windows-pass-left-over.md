# What the ux21 Windows pass left over (2026-11-20)

The ux24 overs pass, PR #2479, on `ux-polish` from `cfd8cd01`. The four items the ux21/ux22 Windows pass left
(`2026-11-20-no-layer-lies-over-the-lists.md` and its record): a guard against a layer taking a list's scroll off the
compositor, the asset tree's wire layer while a wire shows, Monitors' Backspace at full screen, and Compare's and a
repository's first open at 4x and 6x.

Björn's PC numbers are knobas's own WebView2 window (Edg/157), ux22's backend with the branch's front end served into
it over CDP `Fetch`, on a copy of the upgraded scale profile, no sync running, CPU slowed through CDP. Headless Chrome
numbers say which machine.

## 1. A layer over a list can't take its scroll off the compositor again

**Why the frame budget caught neither layer.** Its `scroll` cases set `scrollTop` from script, which hit-tests nothing,
and `run.mjs`'s `wheel` is `Input.synthesizeScrollGesture`: one gesture, one hit test at its start, then the compositor
scrolls. A mouse wheel begins a scroll at each notch, and Chromium hit-tests each. Under a layer that draws nothing
there (`pointer-events: none` or not), it can't trust its own hit test and posts it to the main thread
(`PostingHitTestToMainThread`, in the `input` trace category). Headless Chrome on the Mac does the same as WebView2 here:
real wheel notches over the board, with the chip's layer back, posted every one (40 of 40), and none on the head.

**The guard** is in `scrollers.mjs`, which `frame-budget` runs with `--strict`, now on the fixture with
`?fake-assets=300` (a long asset column), the asset stops before the launcher's (which stays open):

- `layers()`: each fixed or absolute element over a box that scrolls, sampled on a 12 x 12 grid, let take presses for
  the question alone, above the list there and drawing nothing there. Seven bare points fail.
- `wheels`: a real wheel notch at five points of each list, under an `input` trace; any posted hit test fails.

With each fix reverted on its own (headless Chrome, Mac, 1440x868):

| Build | `layers()` | `wheels` |
|---|---|---|
| `8ab2aed9` reverted (the chip's layer over the window) | 8 layers: the board's two lists, the list, Inbox, Builds' two, an asset column, the pane | 6 lists: the board's two, the asset column and the pane at two stops |
| `cfd8cd01` reverted (the wire layer with no wire) | the svg over the asset column | the asset column at two stops |
| head (`cfd8cd01`) and this pass | none | none |

The DOM check also names the chip's layer over Inbox and Builds, whose notches Chromium still hit-tests on the
compositor; WebView2 agreed (the ux21 pass saw those lists on the compositor with the layer there). It fails them
anyway: which lists an engine spares is not ours to count on. `keys.mjs` now hears `hidden` change as a key's answer
(for item 3).

## 2. The wires lie over no column

**Cause.** With a wire showing, the layer was still absolute over the whole tree. In the window, the perf estate's VM
of 300 with a route to its first container (the pane scrolled to the route's row, so its wire draws over the 300-row
column): **12 of 12 notches over the column were hit-tested on the main thread, at 1x and at 6x.**

**Fix.** The lines are drawn in a layer over the gap between the last column and the pane, where nothing scrolls; each
far end (its dot and the line's last few pixels inside the column) in a layer of its own, the size of what it draws.
The two meet on the device's pixel grid, so no part is drawn twice. In the window: the gap layer 236 px wide, the ends
18 x 15 and 11 x 11, and **0 of 12 notches on the main thread** at 1x and 6x. Screenshots of the wired assets and of
Monitors at 1440x868@1, 2560x1440@1.25 and 5120x1440@1 are the same to the pixel, head against this pass.

**Windowing the long column** was not taken out: the column is windowed already (`Windowed`'s `from` is 300, the
column holds 300, `overscan: 1800`), and its first frame draws the rows on screen and a quarter view below. The VM's
open in the window, window size, drawn ms first / return: before 62 / 62, 190 / 192, 285 / 292; after 38 / 51, 177 /
185, 280 / 311 at 1x / 4x / 6x. Within the lines but the 6x return; the ux21 pass's 306 / 329 had syncs running.

## 3. Monitors' Backspace

**Cause.** The typed words took rows out of the DOM, so a Backspace that brought every monitor back built all 218
again. The two options the pass saw were keeping filtered rows mounted at the cost of every letter, or drawing in
stages, which the redesign took out on purpose (`2026-11-19-monitors-is-a-quiet-board.md`).

**Fix, the first option made cheap.** The scope, the state chips and the type still decide which rows are drawn. The
typed words set `hidden` on the rows they leave out, on a block of 16 none of whose rows shows, and on the list when
nothing matches (the no-match line stands beside it). A hidden row's marks are not redrawn on later letters, a block's room
is its shown rows' (so `content-visibility` keeps skipping blocks off screen), and the first shown row draws no rule
(`tf-mon-top`, not `:first-child`). So a letter is cheaper too: it flips an attribute instead of tearing rows down.

In the window, full screen (5120x1369), median of 7, rows ms at 1x / 4x / 6x:

| Key | before | after | line |
|---|---|---|---|
| a letter into the empty filter | 10.8 / 61 / 102 | 5.6 / 34.9 / 52 | 100 / 100 / 200 |
| Backspace back to every monitor | 11.7 / 84.4 / 156.1 | 5.3 / 54 / 105.4 | 100 / 100 / 200 |

The "before" run's front end was the window's own ux22 bundle, the same Monitors code as the head's (the served one
had stopped); with syncs running the ux21 pass read 104 / 205 for the Backspace. One "after" letter run fell in a
minute when another agent loaded the WSL box and was repeated; the table has the repeat. `moves` (fixture, 1440x868@1
and 2560x1440@1.25, 1x and 4x): nothing moved on Monitors, a monitor, Assets or an asset.

## 4. Compare's and a repository's first open: profiled, not cut

In the window at 6x, a first Compare of a repository the session had not compared read its parts in four tasks, 105,
94, 86 and 60 ms. The first is the press: the scroll each kept page puts back (`keepScroll`) forces the new tab's style
and layout (17 and 28 ms); the next two are the reads landing, each laying the page out again, forced by the windowed
lists' placement (`clipsOf`, `read`: 38 and 27 ms). On the fixture, where every read answers at once, the first open is
one task of 200 ms at 6x: 126 ms of script with no function over 31 ms (`landInPlace` 31, the scroll put back 18,
Svelte's answer copy 14, `checkVisibility` 14), 37 ms of layout and 14 of style. The first frame waits on building the
whole page, form, commits, the file list and the first file's lines, and the lists' placement is already one style
pass, one write and one layout a flush (the compare-jump and ux21 overs passes). No cut that keeps every row where it
first draws came out of it here; the next step is drawing Compare's diff after the frame that shows its form and file
list, with its room held, which is the staging the branch has avoided elsewhere and wants its own ruling.
