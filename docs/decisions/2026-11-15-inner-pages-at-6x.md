# 2026-11-15: A repository's and a pull request's inner pages at 6x

**Asked (Björn, on PR #2479, through the orchestrator):** in knobas's own window on his PC at CPU 6x (ux12), a return
to a small file's code view drew at 310 ms and a return to a pull request's *Files changed* at 324; *Files changed*
first opened drawn at 340 to 580 ms. Windows runs about twice the Mac, so the Mac should leave room: about 150 ms at 6x
for a return and for a first open's drawing. Find how much of the first open is the forge and how much the page, and
fix any other inner page over a line at 4x or 6x. His rule: directness, performance and responsiveness over data
scarcity.

**How it was measured.** Headless Chrome on the Mac (M2 Pro) at CPU 1x, 4x and 6x, `open.mjs --set inner` over fresh
APFS clones of the saved scale home, one throttle a fresh copy and backend (a release build of the windowless backend
with debug assertions, so it reads the copy's Gitea credential), the front end built as the probes build it. The live
products were someone else's, so the forge was a stand-in serving the scale estate's own repositories, built again from
the seed with the same commits (`testenv/perf/README.md`, *The inner pages on the Mac*), at 30 ms a request, as the live
Gitea answered a file it had read. Before is `cf2a729c`'s front end, after this pass's, against the same backend and
stand-in, alternated; long tasks with `moment.mjs` (a Chrome trace of one moment, new) and `profile.mjs`. The machine
carried other agents' work (a load of 2 to 20), so a single cell moves 10 to 20 ms either side.

## The two returns Windows found

**A file's code view drew whole up to 2,000 lines.** The Windows pass's "small file" is cart-service's `go.mod`, and the
seed caps every file at 400 lines, so it is the shape of the large one. A return from the landing's README to it built
all 400 lines in one task (about 90 ms of script at 6x on the Mac), and in the middle of that task the effect that
brings another file's top into view read the file's box, which laid the half-built page out (2,031 objects) before the
rest of the build dirtied it all again (3,212 objects laid out in the next frame). Then the highlighter's answer drew
every line again. 152 to 158 ms of task at 6x; 310 ms on the PC.

- The code view windows past 300 lines (`WHOLE_UP_TO`), as every windowed list does (#2209's `from`), so a 400-line
  file draws the hundred or so lines near the view. Copying, a line link and *Find in file* already reached the lines
  not drawn (#2251).
- The scroll to a new file's top is measured in a resize observer's first call, after the frame's layout and before its
  paint, so the file still opens at its top with no jump.

**A pull request's *Files changed* was built afresh on every return,** in a task of 82 to 90 ms at 6x: within it, the
commit picker's width read as the bar was attached (a layout of the half-built tab, 38 ms), the rail's first look at
which file the scroll is in (`checkVisibility` and boxes: a style pass of 1,224 elements), and each of the diff's
highlight answers, two a file, redrew every card's lines, since all of them read one record of tokens.

- *Files changed* stays mounted for the visit to the pull request once opened, hidden while another tab shows, as a
  repository's tabs are (#2248, spec #2234 step 3): a return shows it as it was left and reads again behind it, and
  another pull request starts afresh. What a return still costs is the browser drawing a hidden subtree again.
- The picker's width and the rail's first look are read in a resize observer's first call; the picker's last width
  stands from the start, so the tab's first style holds it, since `--tf-pick-w` is inherited and each write restyled
  the whole diff.
- Each side's tokens are kept under their own key (`SvelteMap`, the entry made before the first answer), so an answer
  redraws the card it is for.

## *Files changed*'s first open: the forge and the page

The *Files* read is the same at every throttle, 140 to 155 ms here: Gitea's pull request, its commits and then its
`.diff`, its reviews, then the pull request again to pin the diff for keeping, about five rounds of a 30 ms forge. On
the PC the live Gitea answered in 271 to 321 ms. The rest is the page: about 25 ms at 1x, 100 at 4x, 160 at 6x, of which
a warm page is about 80 at 6x (a 44 ms build, then a 34 ms layout and paint of about 2,200 objects) and the rest is code
running for the first time in the app's life, which each probe round is. The longest task after the read went from 151
to 157 ms down to 91 to 94 at 6x; the drawn time moved less, as the cold code and the forge are most of it.

## Found on the way, fixed

**The stand-in.** It ran its git in line on its one thread, so one branch list held every other request: up to 5.8 s
here, which first read as knobas (a commit at 2.4 s, Compare at 11 s). Its git now runs off the event loop and a
branch's commit records come from one `git show`; with `--log` it prints every request's time, so the forge can be told
from the page.

**The header's *New pull request* look flooded the forge.** To find a branch with nothing open, the header reads up to
eight branches' whole views, four at a time, each about ten requests to Gitea, and it looked again on every visit to the
repository and every tab shown (the page reads its lists again, which made a new list). One run of the inner pages sent
2,179 requests to the stand-in, nearly all of them this, against a forge quota of ten a second; a person's own reads
waited behind them (Commits after a sync, 0.6 to 1 s). What the look found is now kept per repository for the branches
it asked and the writes asked since, five minutes at most, and dropped on a refresh. A run's end does not drop it:
looked for again then, its reads went out with the run's own and Commits waited for them; a pull request someone opened
from the branch since is what Compare says when the button is pressed ("PR #N is already open from this branch").

**The landing's README waited for the top listing**, which names its section: a second round of the forge. Where the
checkout has said it holds no copy, the README is asked by path beside the listing, as a deep link's file is (#2244), and
the listing's file takes that read; a listing that names another default drops the guess. Here it saves little (15 to
60 ms), since a read by path is three requests (the tip, the entry, the bytes) where the listing's is two.

## Before and after

Drawn ms after the press, first open and return, each throttle on a fresh copy and backend; before is `cf2a729c`'s
front end, after this pass's (one run each, so read a cell to within 10 to 20 ms):

| Place, round (drawn ms) | 1x before → after | 4x before → after | 6x before → after |
|---|---|---|---|
| repository landing, first | 272 → 214 | 300 → 267 | 336 → 321 |
| a deep file, first | 175 → 181 | 205 → 190 | 241 → 247 |
| *Go to file*, first | 87 → 82 | 97 → 91 | 99 → 98 |
| the small file, first | 100 → 73 | 195 → 188 | 194 → 192 |
| the small file, return | 33 → 18 | 30 → 18 | 28 → 28 |
| the large file (400 lines), first | 202 → 203 | 224 → 198 | 259 → 198 |
| the large file, return | 33 → 22 | 99 → 25 | **169 → 38** |
| the README, first | 36 → 49 | 53 → 39 | 70 → 88 |
| Branches, first | 44 → 44 | 85 → 90 | 102 → 102 |
| Commits, first | 40 → 46 | 60 → 62 | 99 → 91 |
| a commit, first | 136 → 120 | 145 → 163 | 180 → 175 |
| Compare, first | 54 → 71 | 146 → 185 | 228 → 224 |
| *New pull request*, first | 50 → 67 | 150 → 155 | 247 → 222 |
| a pull request's overview, first | 39 → 38 | 98 → 96 | 124 → 99 |
| Conversation, first | 34 → 50 | 30 → 44 | 36 → 47 |
| *Files changed*, first | 184 → 183 | 265 → 270 | 347 → 314 |
| *Files changed*, return | 37 → 42 | 80 → 63 | **130 → 82** |
| the larger pull request's *Files changed*, first | 185 → 172 | 265 → 252 | 322 → 330 |
| its return | 49 → 38 | 78 → 46 | **128 → 68** |
| the repository's Builds tab, first | - | 80 → 72 | 98 → 83 |

The small file at 4x and 6x is the `6` set's `Redis.java`, 97 lines (`inner-items.json`); at 1x it is `go.mod`, 400.
Commits after a sync was 0.6 to 1.2 s in both builds, waiting on the forge behind the header's look; the last change
above (the look kept across a run's end) came after these runs, and the gate run on the head is the measure of it.

## Still over, and why

- **First opens that only the forge can answer**, at 6x on the Mac: the landing 321 ms (useful at 288, of which the
  listing's 165 ms read), *Files changed* 314 to 330, a deep file 247. These wait on Gitea's rounds, and on the PC a
  live Gitea is slower than the stand-in. Two backend changes would take a round each: reading the `.diff` beside the
  commits (it waits for them to know whether a kept diff stands at their tip), and pinning a fetched diff after the
  answer rather than before it (the record is read again). Left: the adapter's diff cache rides on both.
- **Code running for the first time**: about half of *Files changed*'s page time at 6x on a first open.

## Owed on Windows

1. In knobas's window on ux13 at 6x: a return to `go.mod` (was 310 ms) and to *Files changed* (was 324), and *Files
   changed*'s first open, drawn; the `6` set's places (`open-win.mjs --set inner --throttle 6`).
2. Commits after a sync on the repository page, and the forge's request count over a minute on a repository page
   (the header's look kept).

## Owed on WSL

`just open-budget --real <port> --inner` on the live Gitea at 1x, 4x and 6x on this head (the 6x lines are new), the big
folders and compares included.
