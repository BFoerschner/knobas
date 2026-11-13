# 2026-11-13: A branch pick draws in a frame

**Asked (Björn, on v3.2.9-ux9, built from `1f84766e`):** "New pull request (Compare) is insanely slow on a branch with
many files changed. On ux9, every time I pick a branch in *From* or *Into* it waits a long time, and the whole UI
freezes while it does. I want picking a branch to feel instant, with a clone and without, at my scale and on slow
laptops." The lines: the branch's changed files (names, counts) and its commits on screen within a frame or two, under
100 ms at 1x and 200 ms at 4x; each file's lines only as it is reached, a long file in pages; nothing that blocks the
page's thread; a pick that supersedes another calling it off; the check against Gitea behind, as before; *Create pull
request* waiting on none of it; the same for a pull request's *Files changed* and *Open as diff*.

**How it was measured.** A scratch org on the live Gitea on the WSL box (`testenv/perf/big-compare.mjs`): a `main` of
10,000 files in 100 folders, branches that change 300, 3,000 and 10,000 of them in three commits each (and `-b` twins,
so the 4x run's first picks are reads knobas never made), and a second base for *Into*; one repository knobas clones
and one it does not. Synced by the windowless backend (`examples/headless_ipc.rs`, a release build) over a copy of the
scale database, and picked in headless Chrome at CPU 1x and 4x (`testenv/perf/compare.mjs`): the address set to a
branch's compare, then *From* picked three times and *Into* twice, each timed from the pick to the frame that lists the
branch's own file (`A-<branch>.md`) with the count line saying *against <Into>*, to the frame that draws that file's
lines with nothing dimmed, and to the page quiet for 700 ms; with every long task, every frame over 50 ms and every IPC
call between. A step that gets no answer from the page within 60 s is the page frozen: the browser is started again at
the next step's start. ux9's front end and backend were built from `1f84766e` in a scratch worktree.

**The machine.** Other agents' builds kept the Mac's load average between 40 and 130 for the whole pass (each line
says what it was); a start of `git` that takes 5 ms on a quiet Mac took 30 to 70 ms. The WSL box was idle (load under
2) until Björn needed it; the measurements after that point ran on the Mac alone, the forge replaced by a server that
accepts and never answers, and the ones that need the live forge are owed.

## Why a pick froze the page

On ux9 a pick of 300 files drew its list in 0.5 to 0.9 s at 1x and 1.5 to 2.5 s at 4x, and then kept the page's thread
busy: 5 to 10 s at 1x and 28 to 48 s at 4x before it was quiet. A pick of 3,000 or 10,000 files never drew: the page
gave the probe no answer for 60 s, at either rate, with a clone or without. The backend was not the wait: it answered
the whole diff in 0.15 s (300 files) to 0.28 s (10,000) with a clone and 0.14 to 0.74 s from Gitea. What froze was the
page:

- **One answer held the whole diff**: 0.3 MB for 300 files, 3.4 MB for 3,000, 5.6 MB for 10,000 (cut at the parser's
  50,000 lines), parsed on the page's thread.
- **The diff built a card for every file** (ux9 had no window over its files: 301 cards and 32,705 elements for 300
  files), and **a highlighting job for both sides of every file**. Each job's lines, as they came, copied the map of
  every file's tokens and woke every card. A CPU profile of a 300-file pick on ux9 spent 8.5 s of 20 s at 1x in
  Svelte's runtime re-running cards (`tokensOf` among them): quadratic in the files, so 3,000 never finished.
- **Every pick read the whole diff twice at once**: the effect that reads again when a branch becomes listed fired on
  every pick, since the listed id changes with the branch (two 3.4 MB answers for one pick of 3,000 in ux9's log).
- **The check behind a clone's diff read the source's whole diff again**, and that read was not one the answer cache
  counts as quiet, so every Compare emptied the cache of every page's kept reads.
- **The file list beside the diff** copied a folder's list for every file added to it, quadratic in a folder's size.

The extremes pass (earlier the same day) had already windowed the diff's cards and its file list on the branch's head;
the rest is this pass.

## Taken

- **A range is read as its list of files first, each file's lines as its card comes near the view** (`entity_range_files`,
  `entity_range_file`, `checkout/range_files.rs`; `TfDiffView`'s `fill`, `diff/fill.ts`). With a clone the list is
  `git diff --raw --numstat -z -M`: every file's paths, change, modes, blob and counts, no lines, and no cap (the whole
  diff stopped at 50,000 lines and dropped the files after). Two branch tips are one `git diff A...B` with the merge base
  and `git log A..B` read beside it, three gits at once; whole shas need no `rev-parse` first. The range's commits come
  with the list, so *Commits* draws with it instead of waiting for the branch's view (seconds on the forge for a branch
  knobas never read). The last four lists a clone answered are kept: a branch picked again, or *Into* picked back,
  answers without git. Without a clone the list is the source's compare, kept whole on the backend for its files' reads;
  Gitea has no call that lists a compare's files with their counts short of the diff itself (its commits listing names
  files per commit, without counts), so that read is the floor, and the page gets its list (1.5 MB for 10,000 files)
  and never the 5.6 MB.
- **A file's lines are read a page of 2,000 lines at a time**, four files at once, nearest the view first; a long file's
  next page as its end comes near. A card holds its lines' room with the design system's skeleton until they come, so
  the files below move little; a failed read says why, with *Try again*.
- **The diff highlights, runs pipelines and plans rendered views for the files drawn**, not for every file as it opens.
  The file list pushes into a folder's list instead of copying it; *Viewed* is a set.
- **A pick calls off what it superseded** (`cancel_read`): a read the page names runs as a task of its own, and calling
  it off drops its request to the source and kills its git. The diff calls off its files' reads when it goes. A late
  answer is dropped as before.
- **A pick reads its diff once.** The listed-again effect fires only when the listing changes under the same branch.
- **The check against Gitea runs behind as before** and keeps its line (*Read from your clone as of the last sync;
  checking Gitea…*), asking the source's list (`entity_source_range_files`), not its every line; a different answer
  replaces the list in place. *Create pull request* waits on none of it (pinned).
- **The answer cache counts the compare's reads as quiet**: `entity_range_files`, `entity_range_file`,
  `entity_source_range_files`, `cancel_read`, and `entity_source_range_diff`, whose every check had emptied it.
- **The same for *Open as diff* and a pull request's *Files changed*** where it reads a range: the clone's diff drawn
  before the source answers, and a pick of commits. The source's own *Files* section, with its review threads on each
  file, still comes whole; the diff draws and highlights only what is near the view of it.
- **Tried and reverted:** counting in four gits at once over a quarter of the paths (or of the folders) each. On this
  Mac one `--numstat` of 10,000 files took 107 to 111 ms and four at once 150 to 250 ms.

## Before and after

Each cell is *the list* / *the first file's lines*, in ms from the pick, then the long tasks on the page's thread. ux9 is
`1f84766e`; after is `29d5c0eb`. The Mac's load average is given per run; with the forge refusing, the load was lowest.

**With a clone, the forge refusing at once** (no source reachable; the Mac at load 7 to 20 after, 20 to 40 before):

| Pick | ux9 1x | ux9 4x | after 1x | after 4x |
|---|---|---|---|---|
| Open at 300 files | 467 / 467, settled 5.3 s | 1,543 / 1,543, settled 27.9 s, 30 long tasks (longest 1.3 s) | 71 / 888 | 209 / 976 |
| *From* 3,000 files | frozen (no answer in 60 s) | frozen | 100 / 184 | 160 / 294, one 71 ms task |
| *From* 10,000 files | frozen | frozen | 196 / 346 | 346 / 530, one 125 ms task |
| *Into* another base, 10,000 files | not reached | not reached | 197 / 314 | 374 / 607, one 124 ms task |
| *Into* back to `main` | not reached | not reached | 58 / 142 | 173 / 423, one 120 ms task |
| *From* 300 again | not reached | not reached | 30 / 113 | 89 / 222 |
| *From* 10,000 again | frozen | frozen | 63 / 146 | 204 / 387, one 132 ms task |

**With a clone, the forge live on the WSL box** (before the pause; ux9 only): 684 ms for 300 files at 1x (settled
8.0 s) and 2,491 ms at 4x (settled 48.5 s, 540 long tasks, the longest 2.0 s); 3,000 and 10,000 frozen at both rates.

**With a clone, a forge that never answers** (a server on the Mac that accepts and writes nothing, the Gitea source's
address; Mac load 30 to 130 before, 10 to 19 after):

| Pick | ux9 1x | ux9 4x | after 1x | after 4x |
|---|---|---|---|---|
| Open at 300 files | 868 / 868, settled 10.6 s | 1,764 / 1,764, settled 36.9 s, 213 long tasks | 25.3 s / 56 s (below) | 25.5 s / 56 s (below) |
| *From* 3,000 files | frozen | frozen | 166 / 300 | 159 / 309 |
| *From* 10,000 files | frozen | frozen | 365 / 465 | 337 / 603 |
| *Into* another base | not reached | not reached | 623 / 740 | 308 / 607 |
| *Into* back | not reached | not reached | 68 / 201 | 164 / 464 |
| *From* 300 again | not reached | not reached | 44 / 144 | 75 / 175 |
| *From* 10,000 again | frozen | frozen | 74 / 291 | 180 / 413 |

The first open's 25 s is headless Chrome's, not the app's: the page's first six requests to the forge (the repository's
views, `refused_writes`) hold all six of its connections to the backend until they time out, and the list waits behind
them in the browser; the backend answered it in 84 ms. Tauri's IPC has no such cap. ux9's freeze is the same with every
forge, since it is the page's.

**Without a clone, the forge live** (ux9 only, before the pause): 950 ms for 300 files at 1x (settled 8.1 s); 3,000
frozen. The backend's own times for the whole diff from Gitea: 0.14 s (300), 0.28 s (3,000), 0.74 to 0.87 s (10,000);
for the list after: 0.18, 0.27 and 0.87 s, which then reaches the page as 47 KB, 0.47 MB and 1.56 MB instead of 0.34,
3.4 and 5.6 MB. The page's side after is measured with a clone above; with none it is owed.

No row of the file list moved after it drew in any step, before or after. After, the page holds six cards and about
1,100 elements at any size, against 301 cards and 32,705 elements for ux9's 300 files.

## Left

- **The first pick of a 10,000-file branch with a clone is over the line**: 196 ms at 1x and 346 ms at 4x. Of that, 170
  to 220 ms is the backend's `git diff --numstat` counting 10,000 files' lines (107 ms of git alone at load 7, in one
  process; four did not divide it), and at 4x one 120 to 130 ms task in the page parses the 1.5 MB list and builds the
  10,001-row file list. 3,000 files are within it at 1x (100 ms) and at 4x (160 ms), and every pick of a branch read
  before is within it at 1x (30 to 63 ms). Next steps, not taken: the list's names first and its counts behind (`--raw`
  alone is 30 to 40 ms; the cards' room depends on the counts, so they would wait for them), and a slimmer list on the
  wire.
- **The first file's lines** follow the list by 80 to 250 ms: one git per file, four at a time, on a busy Mac 30 to 90
  ms each.
- **Without a clone, the list is Gitea's whole compare**, read and parsed on the backend (0.18 to 0.87 s for 300 to
  10,000 files on the WSL box); the page's side of it is not yet measured, the box having gone back to Björn.
- **A pull request's own *Files***, with its review threads per file, still comes from the source whole.
- **Not measured:** the open budget's big compares against the live forge, and the slowed forge through
  `slow-forge.mjs` (owed on the WSL box); the app's own window; Windows.
