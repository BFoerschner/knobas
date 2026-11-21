# 2026-11-16: A big compare moves at once: a jump, the way back up, a swap, Open as diff, Files changed

**Asked (Björn, on PR #2479, through the orchestrator):** on his work laptop, a cloned repository, Windows WebView2, in a
compare of very many files and lines: (1) picking a file in the list and scrolling to its diff takes a long time;
(2) once far down there is no way back to the branch pickers, the file list and the actions ("a dropdown at the top or
a button that scrolls back up, but something has to change"); (3) many commits stack on the right in many different
lines and look bad; (4) changing *From* or *Into* in *New pull request* takes a long time and blocks the UI; (5) *Open
as diff* is slow although the diff is drawn below already. Folded in the same day from the ux14 Windows pass: a pull
request's *Files changed*, first open on the larger pull requests, took 217 ms at 4x and 390 at 6x on his PC, 530 ms of
main-thread drawing at 6x in two tasks of 220 ms. His rule: directness, performance and responsiveness over data
scarcity.

## How it was measured

`big-compare.mjs`'s repositories on the live Gitea on the WSL box, with a long-lived branch added for this pass,
`many-commits` (300 commits over 4,200 files of `main`, about 67,000 changed lines, titles of every length) and its 4x
twin, each with a pull request open into `main`, cloned and synced into a fresh APFS copy of the saved scale home, through
a release build of the windowless backend with debug assertions. `compare-moves.mjs` (new, `testenv/perf/README.md`,
*Moving through a big compare*) in headless Chrome at 1440x868@1 (1x, 4x, 6x), 1920x1080@2 and 2560x1440@1.25 (4x, 6x),
times each move from the press to the frame that shows its answer, with the long tasks and the range reads it made;
Chrome traces and CPU profiles of the moves said where the time went. *Before* is `2edf3eb7`'s front end, *after* this
pass's, alternated on one backend. Two rigs for git: the Mac's own, and git slowed by 100 ms a start (a `git` first on
the backend's `PATH` that sleeps and runs the real one), a stand-in for a Windows laptop, where most of a file's read is
starting git. The cells of record ran under the Mac's timing lock (`timing-locks.md`) at a load of 3 to 16, one run a
cell; the matrices before them ran beside other agents' builds at a load of 10 to 48 and are kept only for the slowed
git.

## Why each was slow

1. **A jump** drew the picked file's card at once, then read its lines: one start of git a file, the files of the
   overscan above it read first and four at a time, so the picked file's lines waited behind other files' starts.
   With git slowed, the lines came 170 to 400 ms after the press; at the full-screen sizes the base often never drew
   them at the view's top (the file moved as the cards above it filled in).
2. **No way back:** the diff's toolbar, the file list's filter and the form scrolled away; only the list stuck.
3. **The commits** stood beside the form two lines each, every one of them: 300 commits made a column about 13,000 px
   tall, and the diff began below it. The check against Gitea then replaced the clone's 300 commits with Gitea's 250
   (its compare stops there), even when it found the same diff.
4. **A swap** dims the diff and makes it inert until the new list lands (the ruling of #2246), and the new list is a
   round of git: a names read of two git starts side by side, then the counted list. With git slowed, 150 to 300 ms of
   a dimmed, inert page each time, on any pair, including one the page had drawn a minute before.
5. **Open as diff** read the list again and every file's lines again, though the compare had both: 190 to 1,600 ms to
   list and more for lines.
6. **Files changed** windowed its cards only from 300 files and a card's lines only from 300 lines, so a pull request
   of seven files built every card and every line's highlight spans in the tab's first frame (traced at 6x on the Mac:
   a 99 ms task of script building them, then a 110 ms frame laying them out).

## Taken

Designed with `/frontend-design` inside the design system (no new colour, type or shape; one new control):

- **The band** (`TfDiffView`, `code.css`): a diff with a file list draws its toolbar as a band that sticks to the top of
  the page's scroller on the paper it sits on, its 6 px of padding taken out of its margins so nothing moves where the
  diff starts. Once it has stuck it draws a hairline under it and ends in ***Back to top***, a small ghost button, out of
  the tab order until then, that takes the scroller to its start and the focus to the page. The file heads stick under
  the band, and the rail's *Filter changed files* and its list stick together under it (`.tf-dstick`), so a file is
  found by name from anywhere in the diff. A pull request's commit picker sticks with the band, level with its controls.
  `Home`, one of the shell's reading keys, already takes the page's scroller to its start; the button's title says so.
- **A jump lands with its lines** (`fill.ts`, `TfDiffView`): every page a diff reads between two whole shas is kept for
  the window (150,000 lines, the range used longest ago let go first), so a file read once draws with no read; the files
  near the view have their first pages read in one read, the reader's file first (`entity_range_file_pages`, below); and
  where the list was read at a checkout, the files not drawn are read ahead while the page is idle, 100 a read, out from
  the reader's file, up to 100,000 changed lines, a cursor walking that order once. A pick in the file list, a `j` far
  down and *Open as diff* draw the file's lines in the frame they land in.
- **One read for many files** (`entity_range_file_pages`, additive IPC; `checkout::range_files::range_file_pages`): the
  first page of up to 200 files in one `git diff -M -- <paths>` at the checkout, or out of the kept compare without one,
  each the page `entity_range_file` answers for that file alone; a file it cannot answer whole is null and read alone.
  Pinned: `many_files_read_at_once_are_each_the_file_read_alone` reads every batch of the fixture's files (all, halves,
  one at a time, reversed, whitespace left out; renames, a binary file, a mode change, names with a space and outside
  ASCII) as each reads alone; a read that drops the renamed files' old paths fails it.
- **Commits, one line each** (`TfRepoCompare`, `branches.css`): sha and title on one line, the whole title on hover, the
  count at the heading's end, in a box of about ten rows that scrolls and draws only the rows it shows (windowed from
  40). A check that finds the same diff keeps the clone's longer list.
- **A swap draws at once where it can** (`TfRepoCompare`, `compare-early.ts`): the list of each pair Compare drew this
  session is kept (16 pairs), and a pick of it draws in its own frame, nothing dimmed, under the clone's line, while its
  reads go out and fill it in place; the names or counted list of the same two commits lay nothing down again. Once the
  pair shown is drawn and the page is idle, the clone's names of the next picks are read one after another and kept the
  same way: the six newest other branches as *From*, three other bases as *Into*. Nothing is read ahead of the forge.
  While a pick's list is read, the diff kept dimmed reads nothing ahead (`calm`). A pair never drawn and not read ahead
  waits on git as before, dimmed (#2246).
- **Open as diff takes what Compare drew** (`keepList`, `TfDiffPage`): the counted list Compare drew between two whole
  shas, the last eight, and the pages kept, in the diff page's first frame, read no second time (two whole shas name a
  diff that never changes, so nothing is read again behind it).
- **Files changed draws what the screen shows first** (`TfDiffView`, `TfDiffCard`): the cards window from three files
  and a card's lines from 80, so the first frame builds the cards on screen and the overscan grows in its own task after
  it, as every windowed list's does.

**Not taken.** A dropdown file picker in the band: the rail's filter and list now stick under it, which is that picker,
already in the design system, with its keys. Not dimming a swap's old diff: a pair read ahead or drawn before no longer
dims at all, and for a pair never read the old diff is another branch's, which should not take a press. Making Gitea's
compare list more than 250 commits: its API's cap; the clone lists 500.

## Before and after

One run a cell. A jump's file counts once its lines are drawn with its head at the view's top (in view, for the last
file). *never*: not within the 60 s limit (the base drew the middle file at the top and then moved it, as the cards above
it filled in); *none*: there was no control. Each after-cell is within its line in `budget-open.json`'s `moves`.

**The Mac's git, under the timing lock** (ms from the press to the frame that shows the answer: a jump's file and *Open
as diff* with their lines drawn; the rest landed):

| Move | 1440 1x | 1440 4x | 1440 6x | 1440p@125% 1x | 1440p@125% 4x | 1440p@125% 6x |
|---|---|---|---|---|---|---|
| A file half way down, picked | 78 → 20 | 130 → 45 | 147 → 96 | never → 23 | never → 71 | never → 101 |
| The last file, picked | 36 → 16 | 48 → 31 | 91 → 45 | 26 → 9 | 91 → 35 | 82 → 56 |
| Back to top | none → 11 | none → 47 | none → 52 | none → 16 | none → 35 | none → 51 |
| *From*, a pair never read | 53 → 47 | 82 → 53 | 124 → 122 | 53 → 57 | 85 → 61 | 119 → 140 |
| *Into*, a pair read ahead | 36 → 9 | 62 → 41 | 74 → 62 | 43 → 18 | 80 → 59 | 102 → 84 |
| *From*, a pair read ahead | 62 → 18 | 107 → 59 | 193 → 129 | 54 → 27 | 135 → 104 | 187 → 152 |
| *Into* back to a pair drawn | 9 → 6 | 12 → 42 | 27 → 23 | 5 → 7 | 23 → 22 | 27 → 38 |
| Open as diff | 126 → 19 | 212 → 105 | 336 → 146 | 123 → 26 | 216 → 100 | 308 → 124 |
| Files changed, a file picked | 19 → 11 | 42 → 73 | 71 → 71 | 21 → 13 | 74 → 55 | 103 → 73 |

| Longest task | 1440 1x | 1440 4x | 1440 6x | 1440p@125% 1x | 1440p@125% 4x | 1440p@125% 6x |
|---|---|---|---|---|---|---|
| A file half way down, picked | 0 → 0 | 0 → 0 | 0 → 92 | 0 → 0 | 0 → 70 | 51 → 98 |
| The last file, picked | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 53 |
| Back to top | none → 0 | none → 0 | none → 53 | none → 0 | none → 0 | none → 64 |
| *From*, a pair never read | 0 → 0 | 0 → 0 | 0 → 50 | 0 → 0 | 0 → 0 | 50 → 56 |
| *Into*, a pair read ahead | 0 → 0 | 0 → 0 | 0 → 59 | 0 → 0 | 0 → 50 | 50 → 72 |
| *From*, a pair read ahead | 0 → 0 | 0 → 54 | 0 → 117 | 0 → 0 | 0 → 83 | 0 → 127 |
| *Into* back to a pair drawn | 0 → 0 | 0 → 0 | 64 → 66 | 0 → 0 | 0 → 0 | 56 → 73 |
| Open as diff | 0 → 0 | 0 → 87 | 64 → 123 | 0 → 0 | 0 → 78 | 0 → 97 |
| Files changed, a file picked | 0 → 0 | 0 → 71 | 68 → 0 | 0 → 0 | 72 → 53 | 101 → 71 |

**Git slowed by 100 ms a start**, a Windows laptop's git (load 17 to 48):

| Move | 1440 1x | 1440 4x | 1440 6x | 1920x1080@2 4x | 1920x1080@2 6x | 2560x1440@1.25 4x | 2560x1440@1.25 6x |
|---|---|---|---|---|---|---|---|
| jump-mid | 197 → 7 | 196 → 6 | 242 → 10 | never → 10 | never → 2 | never → 14 | never → 1 |
| jump-last | 161 → 12 | 182 → 5 | 298 → 7 | 356 → 8 | 401 → 0 | 169 → 10 | 334 → 5 |
| top | none → 1 | none → 0 | none → 14 | none → 14 | none → 7 | none → 14 | none → 14 |
| swap-from | 40 → 203 | 235 → 14 | 245 → 222 | 793 → 16 | 765 → 257 | 236 → 12 | 300 → 274 |
| swap-into | 26 → 17 | 221 → 12 | 184 → 6 | 237 → 10 | 298 → 1 | 274 → 11 | 231 → 0 |
| swap-from-2 | 42 → 13 | 216 → 12 | 212 → 1 | 286 → 13 | 424 → 8 | 360 → 17 | 292 → 11 |
| swap-back | 1 → 3 | 0 → 5 | 16 → 1 | 4 → 8 | 10 → 0 | 5 → 11 | 2 → 10 |
| open-diff | 243 → 4 | 334 → 6 | 393 → 20 | 591 → 18 | 756 → 14 | 375 → 2 | 483 → 13 |
| pr-jump | 13 → 2 | 12 → 6 | 11 → 7 | 11 → 1 | 6 → 15 | 8 → 3 | 8 → 10 |

| Longest task | 1x | 4x | 6x | 4x | 6x | 4x | 6x |
|---|---|---|---|---|---|---|---|
| jump-mid | 0 → 0 | 0 → 67 | 0 → 69 | 0 → 60 | 66 → 100 | 52 → 63 | 50 → 99 |
| swap-from | 0 → 0 | 0 → 73 | 71 → 71 | 164 → 85 | 127 → 93 | 0 → 141 | 102 → 77 |
| swap-from-2 | 0 → 0 | 0 → 74 | 101 → 120 | 59 → 125 | 114 → 167 | 51 → 103 | 82 → 186 |
| open-diff | 0 → 0 | 50 → 78 | 50 → 158 | 64 → 163 | 115 → 180 | 67 → 130 | 125 → 157 |

These runs took a frame's time from its timestamp, which a frame held back by a long task carries from before the task
ended, so their *after* reads a few tens of ms early where a move's own task is long; the table above takes it in the
frame's callback (`compare-moves.mjs` since 186a2812). The after build here is 1d17533a's, before the passes that
lay a swap's list down once and pause the read-ahead.

**Files changed of the larger pull request** (`payments-gateway#211`, seven files), first open, drawn ms and the long
tasks, under the lock:

| Size | 1x before → after | 4x before → after | 6x before → after |
|---|---|---|---|
| 1440x868@1 | 24 → 17 | 63 → 62 | 92 (tasks 56, 69) → 94 (task 57) |
| 2560x1440@1.25 | 25 → 17 | 61 → 62 | 89 (tasks 53, 65) → 99 (task 60) |

When the machine is quiet the first open draws as soon as before at these sizes; what changed is the work in it: one long
task where there were two (the cards below the fold are built after the first frame). Under other agents' load (24 to 29)
the same open at 6x at 1440x868 drew in 124 to 133 ms against 182 to 190, three runs each, alternated: the windowed open
gives a busy main thread less to do before its first frame, which is Björn's PC's case (530 ms of drawing at 6x on ux14).

## What still waits

- **A pair never read and not among the picks read ahead** waits on git, dimmed: one round of the names read (38 ms on
  the Mac at 1x, about 140 with git slowed). The Fable pass on in-process reads takes these paths next.
- **A jump's own task** builds the cards near the landing with their lines at once: under the lock its longest task is
  70 ms at 4x and 92 to 98 ms at 6x, inside the click's 100 and 200 ms lines; the work that came in later tasks before
  now comes in that one.
- **A pull request's Files changed** is the forge's whole diff (with its threads), so a pull request of 4,200 files and
  67,000 lines stops at knobas's 50,000-line ceiling and says so; the files past it have no lines. Reading it a file at a
  time at the clone, threads placed on it, is a pass of its own.
- **Gitea lists at most 250 commits** in a compare; without a clone, *Commits* shows those.

## Owed on Windows

1. Björn's own case on ux15: a compare of thousands of files with a clone, a file picked far down (its lines drawn as it
   lands), *Back to top*, *From* and *Into* changed back and forth (drawn at once after the first), and *Open as diff*.
2. *Files changed* first open of the larger pull requests at 4x and 6x (`open-win.mjs --set inner`): the 217 and 390 ms
   of ux14, and the band and its picker at 125 %.
3. The band's hairline and the stuck filter at 125 % and 150 % in WebView2.

## Reversing it

Each piece stands alone: `from: 3` and `from: 80` back to the default 300; the band's classes off (`tf-dband`, the
mark, `.tf-dstick`); `ahead` off in `rangeFill`; `keepList`/`keepDrawnCompare` unread; the commits box back to `.tf-rows`.
`entity_range_file_pages` is additive and can stay.

## Gates

On the final code (front end as pushed in 4cc83e24, Rust unchanged since adbb9825), in a clean clone, with Björn's trimmed
gates (`trimmed-gates.md`): `just check-front` passed (svelte-check 1,475 files, 0 errors; vitest 340 files, 5,204
passed, 1 skipped); `just check-scripts` passed; `testenv/desktop-witness-test.sh` 307 checks; the press trips with
`--strict` 272 of 272, the 16 `rows-still` among them; `pixel-same.py` at 1440x868@1, 1920x1080@2 and 2560x1440@1.25 over
24 screens of the fixture light and dark (Compare, Compare scrolled and from the default branch, *Open as diff*, *Files
changed* and scrolled, a pull request's Overview and Commits, a repository's Commits, Branches and Local changes, the
Inbox): changed only where meant (the band stuck with *Back to top* on the scrolled diffs, *Commits* one line with its
count on Compare), the rest the same or within the 600 px a build differs from itself, the Inbox's ages column apart by
up to 2,878 device pixels at 200 % (a minute between rounds); `just clippy`, `just clippy-libs`, `cargo fmt --all
--check` clean; `just wasm-fixture` then `just test` (the IPC types changed) 61 binaries, 4,973 passed, 1 failed, 263
ignored, the one the known flake `link_sets_ipc::the_name_roles_move_no_derived_row_either_way`, which passed alone, and
`app/src/lib/ipc/generated/` unchanged after it. The timing of record is this pass's own cases above, under the Mac's
timing lock at two sizes. The trips, the witness test and the screenshots ran before the lock rule (load 25 to 83);
`just frame-budget` and `just open-budget --real` ran in full before the trimmed gates replaced them, at a load of 16 to
83, and their overs (scrolls of Builds, pipeline history, Tickets, Monitors, the Inbox and Code's pull requests at 6x,
the launcher's and Docs' first opens) are all on pages this pass does not change.
