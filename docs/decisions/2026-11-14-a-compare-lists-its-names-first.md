# 2026-11-14: A compare lists its names first

**Asked (Björn's standing brief, on PR #2479):** live, with a clone, Compare's first open of a branch of 3,000 and
10,000 changed files took 118 and 204 ms at 1x and 244 and 324 ms at 4x (the stored-trees pass's live open budget). The
page waited for the clone's own list (`git diff --raw --numstat -M`), which reads every changed file's lines to count
them. "Make it instant": draw the changed files' names at once (from the stored trees, if they can say it), fill the
counts and lines in place with nothing moving, measured at 1x, 4x and 6x, with a clone and without.

## Why it waited

- **The counts.** The clone's list is `--raw --numstat`: git reads both sides of every changed file to count its lines.
  On this Mac, 40 ms for 3,000 files and 100 ms for 10,000, where `--raw` alone (names, changes, modes, renames) is 16
  and 25 ms. The page drew nothing until the counts were in.
- **The wire and the page.** The list is 0.47 MB for 3,000 files and 1.56 MB for 10,000, and the diff view did per-file
  work for every file as it landed (each file's room, its collapse rule, its default pipeline, a filter of nothing):
  one 126 to 132 ms task at 6x for 10,000 files.
- **Without a clone** the list is the forge's compare (Gitea: 0.8 to 4 s), unless it was read once and held
  (`sync.held_read`) or asked early from the repository page.

## Weighed: the list from the stored trees

The stored trees (`2026-11-14-every-branchs-tree-is-stored.md`) hold both branches' trees, each directory by its sha,
so the changed names between two trees are a walk that skips every directory both share. Built and measured
(kept on the local branch `stored-compare-walk`, d0c7bf93, with its tests): it lists what the clone lists for added,
deleted, modified, mode and type changes, a submodule moved on, a change five folders down and a rename with nothing
changed, pinned against the clone's list of the same range; the backend answered 10,000 files in 37 to 56 ms.

What the store cannot say:

- **The merge base.** A compare is against the merge base of the two tips, not *Into*'s head. The mirror keeps tips, no
  commit graph, so only a clone (`git merge-base`) or the forge can name it, and its tree is stored only where some
  branch stands at it. A base that moved since the fork (`topic-3000` below) had no stored tree to diff against; drawing
  head against *Into*'s tip lists *Into*'s own changes as the branch's, and they vanish when the real list lands.
- **A rename that changed the file.** git pairs it by content: one row in the real list, a deletion and an addition in
  the store's. Rows would come and go.
- **The counts**, and so whether a file is binary.

**Decided (the Fable decision agent, xhigh):** with a clone, draw first from the clone's own names, not the store's:
one `git diff --raw -z -M A...B` that reads no file, which pairs renames exactly as the counted read does, so no row
moves; the store's draw saves at most one start of git there. Without a clone, nothing names the merge base yet, so the
store's draw waits for a pass of its own that stores each branch's merge base with the default branch: with it, the
default tip it holds for and the branch's fork commits (the listed commits whose parents are outside the listing); when
the default branch moves, the commits it gained are read once, a branch none of whose fork commits are among them keeps
its merge base and takes the new tip, the rest are asked again; a draw only where that tip is *Into*'s mirror tip, and
never of a list holding both an unpaired addition and an unpaired deletion. A guard that refuses a list whose file has
the same blob at *Into*'s tip as at the head was weighed and rejected: *Into* merging a stacked parent and then changing
the same files again passes it.

## Taken

- **The names first** (`entity_range_names`, `range_files::range_names`): at the checkout, between the tips the mirror
  holds for the two branch names, `git diff --raw -z -M <base>...<head>` with `git merge-base` beside it, never in front;
  `None` where the clone cannot answer or the counted list is kept already. The names are the counted list less its
  counts, in its order, renames paired the same way (pinned:
  `the_names_are_the_counted_list_less_its_counts_in_its_order`, over an edit, an addition, a deletion, a rename with an
  edit, a pure rename, a binary file, a mode change, an empty file, names with a space and outside ASCII).
- **Compact on the wire** (`RangeNames`): the paths, one change letter a file, renames and mode changes beside them, no
  blobs: 0.24 MB for 10,000 files where the list is 1.56 MB. The page makes each file of it (`namesList`).
- **Read beside the counted list, drawn the moment it lands** (`TfRepoCompare`): under the clone's line (*Read from your
  clone as of the last sync; checking Tidewater Git…*), the line the counted list keeps; each file's lines are read
  meanwhile between the same two commits, so the first file's lines follow the names. The counted list fills the rows
  in place: the same rows, the counts in their empty slot, the commits, the lines read kept. A counted list that lands
  first wins; one that fails leaves the names. Design pass (`/frontend-design`): no placeholder in the count slot (it
  would flash for 100 ms), the row's height never changes, one line over the list throughout.
- **The diff view does less per file** as a list of thousands lands: folds and loads are raw state (a proxy made a
  signal for every file asked of it), each file's room reads its inputs once for all files, no filter takes every file
  as it is, and a path's collapse rule and its default pipeline are kept per path until the lists they come from change.

## How it was measured

On the Mac alone (the WSL box and the live products were the Windows pass's): a fresh APFS copy of
`scale-db/final-home` a cell, its Gitea source pointed at `testenv/perf/standin-forge.mjs --real-compare
--refuse-rest` serving `big-compare.mjs`'s repository made locally (a `main` of 10,000 files in 100 folders, branches
changing 300, 3,000 and 10,000 of them in three commits), plus `trunk`, two commits past `main`, and `topic-3000`, forked
from the first of them, so its merge base is no branch's head. The repository is in the copy's mirror as
`knobas-perf-big-compare/cloned` (pinned to a local clone) and `/forge`, its trees stored by `stored_trees_fill`
(124 requests). The windowless backend (`examples/headless_ipc.rs`, release with debug assertions) of `dc6fbcbe` and of
this pass, each with its front end, in headless Chrome (`testenv/perf/compare.mjs --moved`), one throttle a fresh copy
and backend. With a clone the stand-in answered at once; without one, its compare took 650 ms. Mac load 3 to 7.

A first try measured "with a clone" with the Gitea source turned off: a source turned off has its rows left out of
`sync.live_item`, so Compare predicted nothing at the clone and every cell measured the forge. The rig keeps the source
on and the stand-in refuses every route it does not serve, so the scheduled run fails and sweeps nothing (README,
*Big compares on the Mac*).

## Before and after

**With a clone**, ms from the pick to the frame listing the branch's own file with the count line saying *against
<Into>*, the median of three runs, each a pick the backend never read:

| Pick | before 1x | after 1x | before 4x | after 4x | before 6x | after 6x |
|---|---|---|---|---|---|---|
| *From* 3,000 files | 95 | 97 | 146 | 120 | 160 | 123 |
| *From* 10,000 files | 187 | 93 | 257 | 130 | 316 | 166 |
| *Into* another base, 10,000 files | 187 | 90 | 226 | 90 | 291 | 141 |
| *Into* a base that moved (`trunk`), 10,000 files | 184 | 95 | 254 | 100 | 280 | 139 |
| *From* `topic-3000` (merge base no branch's head) | 118 | 83 | 116 | 88 | 150 | 83 |
| Open at 300 files (the app's first compare) | 91 | 96 | 116 | 117 | 160 | 181 |

The backend answered the names of 10,000 files in 36 to 38 ms (the counted list: 133 to 135 ms) and of 3,000 in 30 ms
(61 ms). The longest task on the page's thread for a 10,000-file pick at 6x went from 126 to 132 ms to 116 to 120 ms (the
names drawn), with a second of 79 to 84 ms when the counts land. No row of the file list moved in any step, before or
after. Two after-runs of the 3,000-file pick read 1,355 and 1,721 ms: every call of the step, `cancel_read` among them,
waited about 1.3 s for one of headless Chrome's six connections while a branch's view retried the stand-in's 503s; the
backend answered the names in 30 ms. Tauri's IPC has no such cap; the runs are in the medians, which they do not move.

**Without a clone**, unchanged by design: 774 to 1,051 ms before and 757 to 1,423 ms after for a pair never read, the
stand-in's 650 ms compare and the rig's connections; a pair read once is held and draws at once, as before.

## Left

- **Without a clone, a pair never read waits on the forge** until the merge-base pass above stores what the store needs.
- **3,000 files at 1x** were already within the line (95 ms) and stay so (97); the open of the app's first compare
  (300 files) is the app's start more than the list.
- **The counted list's arrival** is a second task of 79 to 84 ms at 6x for 10,000 files: a key pressed then waits for it.
  The counted list is the existing `RangeFiles`; making it compact too changes what a pull request's *Files changed*
  and *Open as diff* read, for a later pass.
- **Not measured here:** the live Gitea and Forgejo, the app's own window, Windows (owed on the WSL box).
