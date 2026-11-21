# 2026-11-20: Compare is built before its press, with its data in hand

The ux25 overs pass, PR #2479, on `ux-polish` from `dc367839`. Two items `2026-11-20-what-the-ux21-windows-pass-left-over.md`
left: Compare's and a repository's first open at 4x and 6x full screen (section 4 there), and the 300-container VM's
level in Assets at 6x.

Numbers are Björn's PC: knobas's own WebView2 window (Edg/157), ux24's backend, a scratch profile restored from the
saved scale dump and synced with the live Gitea (no clone), each front end built and served into the window over CDP
`Fetch` (base `dc367839` served the same way), CPU slowed through CDP. Compare at full screen (5120x1369), Assets at
window size (1440x868). `drawn` ms of `open.mjs --set inner` / `--set nav`, the frame after the first useful one.
Lines 100 / 200 / 300 ms at 1x / 4x / 6x.

## Fable's ruling (decision agent, xhigh)

Build Compare once, with its data in hand. Not a staged diff with its room held: it adds a task and a layout, shows a
waiting skeleton where the lines are already read (the hiding Björn called unnecessary), its room is not final
(`waitingRows`), and `compare-large` counts as useful only at its lines. Not "draw less" either. In order:

1. **No scroll write without a hide.** `keepScroll` (`while-shown.svelte.ts`) put every noted scroll back on a move inside
   a shown page, which nothing had hidden. The write laid out the half-built tab in the press's task (17 ms of style and
   28 of layout at 6x). It now writes only after a real hide.
2. **The tab switch's re-read leaves the open.** Each tab press read the branches, the pull requests, the branch pulls and
   the builds again in the press, and the answers landed inside the new tab's first open, each through `landInPlace`,
   whose `scrollTop` read laid the page out (31 ms of the fixture's 200 at 6x). They now go out two frames after the
   switch, once the window is idle, and an answer equal to what the page holds writes nothing (`TfRepo.svelte`,
   `sameAnswer`).
3. **Reads go out at the address, before the build.** An address on Compare that no press reached for (a launcher jump,
   a link a page sets) asks its pair as a press on its link does, and the early ask carries the clone's names beside the
   list (`compare-early.ts`). A repository's landing at an address no page holds reads its landing as a press does
   (`readLandingAtAddress`, from the shell's hash change); a press and the address it sets read it once.
4. **Compare is held ahead.** Once the landing's tree has drawn, the reader has been still a second and the window is
   idle (`quietAndIdle`, now exported from `ahead.ts` and hearing input for the whole session), the repository page
   mounts the Compare tab hidden at the bare `compare` address (`holdSlot` in `repo-tabs.ts`), on the branch *New pull
   request* proposes. Only on a page shown with more than one branch; without a clone only once the early compare has
   answered, so the forge is asked nothing new but the branch's standing, which goes in the background lane. Its first
   show is not a return that reads again (`ahead` in `KeptTabs`). An address on another pair builds Compare afresh
   (`compareGen`), so it never shows the other pair's diff dimmed while it reads its own. Nothing is staged: the tab is
   hidden until its press shows it whole, in one style pass and one layout.
5. Only if the large compare was still over: a clone's list carries `first_pages`. Not built: Björn's profile has no
   clone of the repositories measured, and the forge's list already carries them.

No ahead hold for repositories (nobody knows which comes next).

## Before and after (first open, drawn ms, 1x / 4x / 6x)

| Place | base | after |
|---|---|---|
| Compare's tab pressed (`repo-compare`) | 181 / 244 / 321 | 27 / 115-122 / 199-210 |
| Compare pressed 300 ms after the tree drew (`repo-compare-cold`, new) | | 34 / 166-169 / 249-250 |
| *New pull request* | 26 / 135 / 230 | 16-22 / 127-135 / 195-201 |
| A large compare by address (`compare-large`) | 35 / 299 / 379 | 24 / 186-259 / 337-373 |
| A repository's landing (`repo-landing`) | 53 / 161 / 274 | 35-48 / 183-197 / 269-302 |

Compare's first open is within its lines at every throttle, pressed after the hold or before it. Two stay at or over
6x's line, recorded rather than closed by staging, as the ruling says:

- **The large compare by address** (another pair than the one held) builds Compare whole in the address's task: 113 to
  127 ms of script, then its list's answer and a frame of 35 ms style, 52 layout and 28 paint at 6x. Its reads are in
  hand before the build (no call before useful).
- **The landing** is one build task of 167 ms at 6x with its reads in hand: 41 ms is the first parse of each template
  (`set innerHTML`), 46 ms the IPC sends of the page's own reads (`fetch`). Reading the landing at the address moved
  no number measurably.

## The 300-container level in Assets

- **No wire, no measuring.** `measureWires` read the tree's, the pane's and the strip's boxes after every redraw with no
  wire to draw, which laid out the column a press had just built (75 ms at 6x on the fixture) and ran on each column's
  scroll. With no wire it now reads nothing.
- **A level's first frame draws the rows on screen** (`onScreen` on the column's window); the rest of the overscan two
  frames later, below the column's clip.

| `asset-long` (window size, 6x) | base | after |
|---|---|---|
| first open | 271, 294 | 242, 239 |
| return | 283, 307 | 340, 275 |

The return is still at the line. It waits for two rounds of reads: the asset (`get_asset`, kept by the answer cache),
then all four columns of its path, the 300 rows last (8 calls before the useful frame). Two cuts were tried and taken
out:

- Asking a press's columns at the press: they then landed with the asset in one longer task, and 6x drew no sooner
  (287 / 291 ms).
- Keeping `asset_tree` and `depends_on_this` in the answer cache: the return drew in 157 to 159 ms at 6x, but a new
  selection's interim read of the top column alone then answered at once instead of late, the strip drew one column for
  a moment, and `→` onto a holder left focus on a row that was redrawn (trip `strip-walk-gitea`, F-556, red at all three
  widths). Holding the columns while the selection's read is out broke `↓` in the same trip. It wants an interim read
  that never lands, which is its own change.

## Tested

`while-shown.test.svelte.ts` (no scroll write on a move inside a shown page, a hide still restores),
`TfRepo.kept.test.svelte.ts` (no list read in a tab's open, an unchanged answer lands nothing, the address asks Compare's
pair, Compare built ahead hidden and shown with no read again, another pair's address builds it afresh, nothing built on
a hidden page), `repo-tabs.test.ts` (`holdSlot`), `hover-reads.test.ts` (the landing at an address, once with a press,
none for a page held), `AssetsView.test.svelte.ts` (no box read without a wire). Each red on the code before.

`moves.mjs` gains `repo-compare` (pressed after the hold) and `repo-compare-soon` (pressed 300 ms after the tree drew);
`open.mjs` gains `repo-compare-cold`. The soon case runs with `?fake-branch-pulls`, a fixture switch that answers
`repo_branch_pulls` from the fixture's own branches, as the real mirror does: by default the fixture's mirror lists no
branch's pull request, so only the branch's standing, 350 ms late on the fixture, said PR #142 is open, and the form
drew *New pull request* and shrank to *Open PR #142* (236 px), on the base code as well as this one.

**Reversing it**: the hold is the effect after `compareGen` in `TfRepo.svelte`; the reads at the address are the
`$effect.pre` before it and `readLandingAtAddress` in `TfShell.svelte`.
