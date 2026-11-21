# 2026-11-13: A file after a sync and a first compare wait on nothing knobas holds

**Asked (Björn, on PR #2479): "these should be fixed"**, about two findings of the day's live measurement block:

1. A file reopened after a sync moved its branch was read from Gitea again, 2.1 s for a large one, while the clone
   held the new commit (the big-folder pass saw it at `3bcaea7c` too). A file the clone holds at the new commit must
   come from the clone at once; one it does not hold yet must draw what knobas held, under the held line, and be
   replaced in place when the clone (after its fetch) or the forge answers.
2. Compare's first open without a clone waited 0.65 to 2.7 s on Gitea's compare (55 to 177 ms with a clone). It must
   feel instant without one too; nothing blocks the page or a key, and *Create pull request* never waits.

## Why each waited

- **The file.** *Files* draws the clone's tree at the commit the mirror names for the branch, and reads the source's
  top behind it (ADR-0041). After a sync, the source's top stood at the new commit and the clone's tree drawn at the
  old one. The tab took that as "the branch moved since the sync": it put the source's tree in place and never asked
  the clone again, so the open folders and the file were read on the forge at the new commit, which knobas had never
  read. The clone was not asked at the mirror's new tip either, so it was never fetched into behind the read, and
  `checkout:fetched` was heard only by a tab with no clone tree drawn. A page opened afresh after the sync was no
  better: the source's top that knobas held from before the sync answered first, its tree stood, and the file at the
  old commit was drawn until the read behind said the branch moved. A deep link's early guess at the file also stood
  for the clone's file at the new commit, so a page could keep the file from before the sync for good.
- **The compare.** Without a clone, a compare of two branch names is Gitea's whole compare, read when the page asks
  and never kept past the session's four (ADR-0042 as amended this morning: "it is not held, since a branch name
  moves"). Every first open of a session, and every first open of a branch, waited the forge's whole answer.

## Taken

- **The clone at the mirror's tip, asked first** (`TfRepoFiles.svelte`, `rereadLocal`, `land`). On a return or a
  refresh, and when a fetch behind lands, the tab reads the clone's top at the mirror's tip; where the mirror stored a
  newer tip than the one drawn, the clone's tree takes over in place at once, the open folders read from it and the
  file read there. Where the source's top moved past the clone's tree, the clone is asked at the mirror's tip before
  the source's tree is put in place, and takes over where it stands at the source's commit; where it lacks the tip,
  that read fetches it behind (ADR-0041) and the landing hands the tree back. A source's top that knobas held from
  before the sync the clone's tree stands at (its `held_at` before the tip's `as_of`) gives way to the clone's tree,
  and is checked by the read behind as before. A deep link's guess stands only for a section at its own commit. A
  sync's move is quiet: no *moved on Gitea since the last sync* banner, which stays for a branch that moved past the
  mirror.
- **The file knobas drew stays, undimmed, under the held line** while the file at the new commit is read from the
  source. The line takes the meta's place in the file's header (*As knobas read it 2 h ago, at c0ffee0. Checking
  Tidewater Git…*), so nothing under the header moves when it comes or goes; the answer replaces the file in place,
  and a source that does not answer says so there and leaves the file. A read from the clone (a few milliseconds)
  draws no line.
- **A compare's list is held** (`range_files.rs`, `held_list`, `hold_list`): every list of two branch names the
  source answers is held in `sync.held_read` (ADR-0042) under `range_files:<base>...<head>` of the repository, and a
  later visit or session answers it at once with `held_at` (`RangeFiles.held_at`, additive). Compare draws it under
  the held line (`TfHeldNote`, whose room stays) and asks the source's list behind it, as it asks behind a list the
  clone predicted; a different answer replaces the list in place, quietly. The whole diff, lines and all, is held too
  under the two shas the source settled (`range_diff:<base>...<head>`, fixed, so true for good) where it fits the
  store's 2 MiB row, so a held list's files draw their lines at once; past it, the lines are read from the source when
  the page reaches them, four files' reads sharing one read of the compare (`one_read`).
- **A compare asked before its page** (`compare-early.ts`): without a clone, the repository page asks the list *New
  pull request* would open once its landing has drawn (or 1.5 s after it opened on another tab), and any compare whose
  link the pointer rests on for 80 ms (*New pull request*, the Compare tab, a branch's row), once a session each.
  Compare takes the answer or the read under way instead of asking again; one that failed is the page's own read.
  *Create pull request* waits on none of it.
- **Weighed and not taken.** A list stored in the mirror and refreshed by the sync (migration `0088`): every run would
  ask Gitea the whole diff of every branch that moved, for pages nobody may open, the cost ADR-0042 rejected for views;
  the held list removes the wait for every compare read once, and the early read for the one the reader is about to
  open. Drawing the branch's commits from the mirror: Compare's *Commits* already come from the branch's held view, and
  the mirror holds no file list for a commit or an open pull request (its `files` are not synced). A cheaper Gitea call:
  the compare pass found none that lists files with their counts short of the diff; not measured again, the live forge
  being in use.

## How it was measured

On the Mac alone: the WSL box was in use by the live block, and its tunnels were up, so a scratch copy of
`scale-db/final-home` had every source turned off but Gitea, and Gitea pointed at a local stand-in
(`testenv/perf/standin-forge.mjs`): Gitea's compare (300 added files, after `--delay`), and with `--git` the routes
*Files* reads (repository, branch, tree levels, a file's entry and bytes) from a local repository whose `main` the
mirror's tip was moved to. The windowless backend (`examples/headless_ipc.rs`, release with debug assertions) of the
branch head before this pass (`af3be52b`) and after it, each with its built front end, in headless Chrome at CPU 1x and
4x. The Mac's load average was 5 to 37 across the runs (other sessions' builds).

- **The file** (`testenv/perf/synced-file.mjs`): `cmd/server/main.go` (33 KB, 400 lines) drawn at the branch's commit,
  then a sync (a commit on the stand-in, fetched into the clone, the mirror's tip and `synced_at` moved), then the
  return (away to Time and back: the page is kept and reads again) or a reload at the file. The windowless backend
  emits no events, so the probe raises `view:fresh` when the stand-in has answered the new commit's tree, the read
  behind's last request. Timed from the step to the frame showing the new commit's text.
- **The compare** (`testenv/perf/compare-open.mjs`): *New pull request* pressed after the landing was looked at for
  3 s (pointer on it 150 ms first), and the branch's compare loaded by address; then the backend restarted (a second
  session) and both again. Timed from the press or the load to the frame listing the files with nothing waiting over
  them; the first file's lines followed within a frame in every row after.

## Before and after

**A file after a sync, with a clone that holds the new commit**, ms:

| Step | Stand-in | before 1x | after 1x | before 4x | after 4x |
|---|---|---|---|---|---|
| Return to the kept page | at once | 1,162 | 160 | 1,018 | 381 |
| Return to the kept page | 1 s a request | 6,709 / 6,753 | 1,058 / 1,067 | 6,862 / 6,994 | 1,138 / 1,164 |
| The page loaded afresh | at once | 481 | 266 | 748 | 583 |
| The page loaded afresh | 1 s a request | 3,439 / 3,570 | 263 / 289 | 3,717 / 3,778 | 563 / 600 |

Before, the return waited for the read behind (repository, branch and tree, three requests) and then read two folder
levels and the file on the forge; after, it reads nothing from the forge. The 1.1 s left at 1 s a request is the rig's:
headless Chrome sends six requests to a backend at once, and the four branch views the page reads were holding them;
the backend answered every clone read in 0.1 ms (a folder) and 15 ms (the file), and Tauri's IPC has no such cap. The
page loaded afresh includes the app's own start.

**The same without a clone**, 1 s a request: 6,808 and 6,631 ms before, 6,826 and 6,526 after at 1x (return, load);
6,841 and 6,808 before, 6,822 and 6,769 after at 4x. Unchanged: the file at the new commit is the forge's. What changed
is what is drawn meanwhile: the file knobas held, undimmed under the held line, where before it was dimmed.

**Compare without a clone**, the list in ms, with the stand-in's compare taking 650 ms and 2,700 ms:

| Open | 650: before 1x | after 1x | before 4x | after 4x | 2,700: before 1x | after 1x | before 4x | after 4x |
|---|---|---|---|---|---|---|---|---|
| *New pull request*, first session | 733 | 67 | 866 | 187 | 2,778 | 69 | 2,881 | 190 |
| By address, a branch never read | | | 1,215 | 1,165 | | | 3,216 | 3,215 |
| By address, read by the press before it | 898 | 233 | | | 2,949 | 232 | | |
| *New pull request*, second session | 748 | 90 | 854 | 188 | 2,826 | 112 | 2,881 | 201 |
| By address, second session | 915 | 247 | 1,181 | 548 | 2,946 | 265 | 3,198 | 548 |

At 1x the branch *New pull request* proposed is the one the address names, so its address row is a list read moments
before; at 4x it is not, so its first address row is a read knobas never made. The address rows include the app's own
start; with nothing to wait for that is 230 to 550 ms. The source is asked as often after as before (8 compares a
session): what knobas held is checked behind, and the early read is the page's read, sooner.

## Left

- **A compare never read, opened by an address** (a link from elsewhere, the launcher) with no clone and no pointer
  resting on a link to it first: the forge's whole compare, as before (1,165 and 3,215 ms above).
- **A file at a commit the clone does not hold**, with no clone or before the fetch lands: the forge's reads, which on
  a page opened after the sync start only once the read behind says the branch moved (three requests), then a folder
  level each down to the file and the file. Drawn meanwhile from what knobas held.
- **A list held past 2 MiB of lines**: its files' lines are the source's compare again in a later session, and that
  read and the check behind are two reads of one compare (the first by its shas, the second by its names).
- **Not measured here:** the live forge and the slowed proxy, the app's own window, Windows; owed on the WSL box.
