# 2026-11-17: What the ux17 Windows pass left over: the start's fills, Compare's second read, a repository's first build, a forge read's hold, the zoom's line and its notches, Builds at full screen, a drag's first frame

**Asked (Björn, through the orchestrator):** the ux17 Windows pass (v3.2.9-ux17, PR comment 6943892580) left seven
things over a line or unexplained on his PC. Fix what can be fixed, with tests; a decision goes to one Fable agent
(xhigh); measure before and after on the PC.

**How it was measured.** Björn's PC, knobas's own WebView2 window, CDP from a portable Node on Windows, under the PC's
timing lock, Björn idle throughout (over two hours at the start, the idle guard before every start). A scratch profile
restored from the saved scale dump, migrated by ux17, Gitea synced four times and Forgejo and Kuma once, then copied:
every run starts from a fresh copy, so each first open is a first read. *Before* is v3.2.9-ux17 from its MSI (unpacked,
never installed); *after* is this pass's head built for Windows on the Mac (`cargo xwin`, release, the front end
embedded) beside the MSI's PostgreSQL. The inner-pages runs had a Gitea sync every 45 s through them (`sync-loop.mjs`),
as the Windows passes do. The numbers are single runs at each throttle unless said; the PC's noise between two runs of
the same build is 10 to 20 % (a 1x small file read 104 then 70 ms on the same build).

## 1. Every start walked the fills: now one read each

**Why.** Bring-up's card, list-row and run fills (`knobas_core::fill::walk`, behind Ready) walked every item a batch at
a time, 78 + 80 + 17 batches, and wrote nothing on a mirror that is whole: every write to `sync.item` keeps its rows in
its own transaction, so only a mirror from before a migration or a declaration change owes any.

**Taken.** Each fill first asks one anti-join, whether any item it would write a row for has none
(`fill::walk_owed`, the `OWED` statement of `item_card`, `item_row` and `item_run`), and walks only then. The row
functions give every item of a declared kind a row (a ticket a card while any adapter declares cards), so the
statement is exactly where a batch would write; a false never leaves an item without its row. No mark is stored, so
nothing has to be cleared when a declaration changes or a migration rewrites rows. Tests: a fill with every row written
runs no batch, and one row taken away is walked and written again (cards, rows, runs).

| On the PC, scale mirror, a fresh copy started offline | cards | rows | runs |
|---|---|---|---|
| ux17, two starts | 1,055 / 1,546 ms, 78 batches | 696 / 615 ms, 80 | 150 / 143 ms, 17 |
| after, two starts | 103 / 102 ms, 0 batches | 33 / 33 ms, 0 | 30 / 24 ms, 0 |

The cards' 100 ms is the whole-cards mark (`mark_cards_whole`) the board reads, unchanged. On the loaded Mac the three
checks cost 41 ms warm (131 cold) where the walks took 2.65 s.

## 2. Compare's first open, not cloned: the lines come with the list

**Why.** The list read Gitea's whole compare, kept it, and answered without lines; only once the page had drawn the
list did the view ask the first files' pages, which the backend served from the compare it had just kept. That second
round was no forge read but an IPC hop, a draw and a parse, under the throttle all main thread.

**Ruled (Fable, xhigh).** An additive optional field, `RangeFiles.first_pages`, filled only where the lines are in hand
as the list answers (the source's compare just read, or the one knobas kept or held between the list's two whole
shas); the page seeds its kept pages from it, so the view's first asks find them and the list and its first lines draw
in one frame. In the tree's draw order (the root's files, then folders by path, names within), binary files, files with
no changed line and files over 1,500 changed lines (*Load diff*) passed over, at most 40 files, 4,000 lines and 256 KiB
of text, a file that does not fit passed over and the next looked at (200 at most). A checkout's list (`--numstat`, no
text) and a prediction carry none: there a first page would put a patch read in front of the list, and that path is
not over. Also: a press (pointer or Enter) on a compare's link asks its list at once as a person's read; the 80 ms rest
still asks ahead of it. Why not a new command: it would duplicate the list's four paths, the early ask and the held
list for what one optional field does. Why not asking the pages as the list lands: under the throttle the hop is the
page's own main-thread time, which that keeps.

**Taken** as ruled (`range_files::first_pages`, `fill.ts`' `seedPages`, `TfRepo`'s `pressOnCompare`). Compare, *Open as
diff* and a pull request's files seed the pages; what Compare keeps between visits holds no lines.

| Compare's first open, not cloned, drawn ms | 1x | 4x | 6x |
|---|---|---|---|
| ux17 (this pass) | 133 | 195 | 323 |
| after | 110 to 119 | 168 to 173 | 275 to 282 |
| useful (the list and its first lines on screen), ux17 / after | 121 / 93 to 94 | 159 / 86 to 90 | 268 / 131 to 136 |

The only read before the page is useful is now the list itself, sent at the press (1 ms before the page's first frame)
and answered by Gitea's compare at 98 ms at 1x; there is no second read. At 1x the drawn time is that compare plus the
frame: the forge's.

## 3. A repository's first open: no number format, no layout of the half-built page

**Why.** Profiled on the PC with an unminified build at 4x: the landing is one task of about 126 ms from the address,
and two parts of it were no part of the page. `TfRepoLastCommit` formatted the commit count with
`toLocaleString("en-US")`: 19 ms of self time, the locale's data loaded on the page's first frame. And the leaving
page's `keepScroll` read its boxes' scroll after the new page was in the DOM, laying the half-built page out (19 ms of
forced layout). The landing itself is small (176 elements); the 4,074 nodes the ux17 pass counted are the document's
kept pages.

**Taken.** `thousands()` groups a whole number as en-US does without `Intl`, and every en-US count uses it (diff cards,
a run's log, Compare's commits, the folder renderer). The shell notes each box's place as the address changes, before
anything is built, while the last frame's layout is clean (`noteScrolls`), and a page the move hides takes its place
from the note; a hide no move noted reads its boxes as before. Tests: `thousands` draws what en-US does for every
magnitude; a hide during a move reads no box and the place comes back.

| A repository's landing, first open, drawn ms | 1x | 4x | 6x |
|---|---|---|---|
| ux17 (this pass) | 64 | 182 (longest task 75) | 292 (119) |
| after | 56 | 174 (73) | 289 (114) |

Within every line. What is left of the task is the page's own build (`TfRepo`, its tab, the files tab: about 35 ms
inclusive at 4x) and Svelte's effect flush.

## 4. Files at 1x: the forge's read, and the hold no longer in front of the answer

**Why.** A file's first open at 1x is the read itself (66 to 91 ms of `entity_view`) and 6 to 10 ms of page after it;
Gitea answers the two requests a file takes (`contents` and `raw`, side by side) in 33 to 53 ms from Windows. Of
knobas's share, one part was on the answer's path for no reason: a never-read view was serialized and written to
`sync.held_read` (a file's whole text among it) before the page got it.

**Taken.** The hold goes behind the answer, counted with the reads behind (`LiveSources::hold_behind`, `settled()`
waits for it); the session's next ask is answered from memory meanwhile, and a hold never replaces a newer one. Test:
a read answers while the store is locked against row writes, and is held once it is not. The page's share (6 to 10 ms
at 1x) is the file's draw.

| A file's first open, drawn ms, ux17 / after | 1x | 4x | 6x |
|---|---|---|---|
| small | 104 / 70 | 126 / 112 | 193 / 143 |
| large | 80 / 72 | 113 / 116 | 143 / 129 |
| a PDF | 87 / 105 | 90 / 95 | 84 / 85 |

The rest is Gitea's, and the PC's noise is as large as the change (the same build read the small file in 176 ms once).

## 5. A zoom step is held to a page's line; three notches lay the page out twice

**Ruled (Fable, xhigh).** A zoom step is held to a page's line (100 / 200 / 300 ms at 1x / 4x / 6x), not a key's. A
key's line is for an input that changes one thing on a page that otherwise stands: a row picked, a menu opened, a
filter's letter. A zoom step changes the size of every CSS pixel, so every run of text is shaped at a new size and
every line wraps anew: it is the page drawn again, the same work as a page shown again or a window resized, which the
rulings of 2026-11-16 (a sidebar's drag) and 2026-11-17 (a return to a page) already cost as a page. The trace agrees:
on Builds at 4x the step is 68 to 127 ms of layout, forced by the first list's read but owed by that frame anyway, and
the other 36 listeners and observers take under 4 ms together, so there is no script to take out. The key itself is
answered at once: the plate and the status bar's level are drawn from the press, before the webview is told, and that
answer keeps the key's line. Making the layout cheaper was tried and refused: `contain: layout paint` on rows snaps a
row at a fraction of a pixel to a whole one and moves its text (2026-11-15, 2026-11-16), which is the fault the zoom's
own grid work exists to prevent; `content-visibility` gives a skipped row a stand-in height, so a windowed list would
measure rows wrong and move them (2026-11-11, removed 2026-11-14). Containment only keeps a change from spreading, and
a zoom changes every box from the root. The lists already draw only the rows near the screen, so what a step lays out
is what is on screen. Measured on the PC (the ux17 pass), every page is inside the page's line at every throttle, the
worst being Builds at 36 / 162 / 256 ms.

**Taken.** `zoom.mjs` prints each step against the page's line and the plate (key to the frame that draws the new
level) against a key's. And the candidate Fable named, checked first: three Ctrl+wheel notches 25 ms apart on Builds at
6x landed as three levels (110, 125, 150 %), each a layout of 215 to 335 ms, 0.75 to 1.1 s in all. A level set on the
webview now holds the steps that come meanwhile until its resize is heard and a frame drawn (600 ms at most), then the
newest lands once (`zoom.svelte.ts`, `settled`). The wait starts as the level is sent: WebView2 lays the page out and
its resize is heard before `setZoom` returns, so the first version, listening after the call, waited out the whole
bound and landed the newest level 1.3 to 1.6 s after the first notch. Tests: two steps while a level is on its way land
once, at the newest; a layout heard before the webview's call returns lets the next land, each level sent once.

| Three Ctrl+wheel notches 25 ms apart, Builds, 6x, window size | layouts | the last level laid out |
|---|---|---|
| ux17, 5 runs | 3 every time (110, 125, 150 %) | 876 to 884 ms |
| after, 4 runs | 2 in 2 runs (110 then 150 %), 3 in 2 | 578 to 607 ms where 2 |

A wheel's events reach the page aligned to its frames, so a notch that comes one frame after the first level is laid
out is a level of its own; the hold takes the ones that come while a level is on its way. A single step costs what it
did.

## 6. Builds at full screen at 6x: why it stays

**What it is now.** Measured at full screen (5120 x 1369) at 6x with the wheel probe (`run.mjs --kinds wheel`, the runs
list), three runs each on the same scale copy: ux17 moved at 149 to 151 frames a second with worst gaps of 30 to 54 ms;
this head at 151 to 155 with 30 to 54 ms. The same, run to run as wide apart as between the builds; the ux17 pass's 25
to 38 ms were a quieter run of the same thing. The list stays on the compositor (main-thread work p50 0.9 ms).

**Tried, on the PC, with the page's CSS changed over CDP:** the runs list capped at 1,800 px (painted rows a third as
wide): worst gap 48 ms against 54 and 24 without it in the runs around it, so no measurable change. The ux14 check had
already shown the burst is Chrome rastering the tiles ahead of a scroll at its start, not the rows' swap.

**Found, for a later pass:** Builds at full screen draws 8 composited layers, 35.7 megapixels, five screens' worth: the
runs' scroll content (4,820 x 3,055, 14.7 Mpx) as expected, but also the app's root `div.tf` (7 Mpx) and a 4,829 x 1,290
layer (6.2 Mpx), both promoted only because they "overlap other composited content", and the document's own. Layers
promoted for overlap are what *fewer layers* would remove; which composited box below them causes it was not found in
this pass. Not changed: no change measured better, and a capped list is a visible change that bought nothing.

## 7. A drag's first slow frame, and the code tree at 4x: why they stay

**The first slow frame** is how a drag learns its page cannot be laid out at a frame's pace: the first written frame
over 19 ms makes that drag a ghost (Fable's ruling, 2026-11-17, `side-pace.ts`). A marked sidebar starts its next drags
as a ghost at once, and the ux17 pass saw that hold at 6x; at 4x the marks cleared, because a ghost's landing (one
layout at rest) comes in under 19 ms there while a live drag's frames do not. A stricter clear (more landings, or a
lower line) cannot be measured by frame gaps on a 60 Hz panel, whose every gap is 16.7 ms at least: it would hold a
fast laptop's sidebars as ghosts for good. So a first drag pays one slow frame of 21 to 50 ms, by design.
**The code tree at 4x** stays live 231 ms with three frames of 21 to 25 ms, never two in four: that is a drag keeping up
at 40 to 48 frames a second, which the rule leaves live on purpose (a ghost is the fallback for a page that cannot
follow, not for one that follows a little under the display's rate).

## The PC run

Scripts under the pass's scratch folder (`knobas-perf18` on the PC: the ux17 pass's kit, plus `first-open-prof.mjs`
(a first open's CPU profile and long animation frames), `trace-open.mjs` (a first open's main thread by kind),
`regions.mjs` (a page's nodes by region), `layers.mjs` (composited layers and why), `css-exp.mjs` (a CSS experiment on
the live page) and `wheel3.mjs` (three Ctrl+wheel notches)). One overlap with another agent: a profile of about a
minute ran while the movement audit held the PC lock (a lock check that failed did not stop the script); every number
above is from runs under this pass's own lock.
