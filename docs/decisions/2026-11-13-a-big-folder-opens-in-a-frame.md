# 2026-11-13: A big folder opens in a frame

**Asked (Björn, on v3.2.9-ux9, built from `1f84766e`, on Windows):** "even though I have the repo cloned, opening a
folder with a lot of subfiles in the tree on the code page still takes a long time. like a second or so, even if
cloned." The lines: a folder's rows on screen in the next frame or two, under 100 ms at 1x and 200 ms at 4x, for any
size with a clone; nothing moving after; the rest of a huge folder drawn as it scrolls.

**How it was measured.** A scratch repository on the live Gitea on the WSL box (folders of 300, 3,000 and 12,000
entries, and one of 800 ten directories down; 16,126 entries in all), synced by the windowless backend
(`examples/headless_ipc.rs`, a release build) over a copy of the scale database, and pressed with real input in
headless Chrome at CPU 1x and 4x (`testenv/perf/folders.mjs`): from the press to the end of the frame that shows the
folder's rows, the main thread's time by kind from a trace, the IPC calls between. Four states: cloned by knobas,
cloned by hand (another folder name and URL scheme, found by the clones root's scan), a clone behind the mirror's
commit, and no clone. The repository was removed afterwards.

## Where the second went

- **With a clone, all of it was the page drawing every row.** The press makes no IPC call: the clone's whole tree is
  in the page from the tab's open. `git ls-tree` of 16,126 entries took 40 to 50 ms on the Mac, once, at the open, and
  the backend keeps it. The tree drew each open folder with a plain `{#each}`, so a press mounted every entry of the
  folder, and the listings were deep reactive state, so each entry read went through a proxy:

  | Folder | 1x | 4x | main thread at 4x (script, style, layout, paint) |
  |---|---|---|---|
  | 300 | 37 ms | 68 ms | 65 ms |
  | 3,000 | 123 ms | 539 ms | 506 ms (263, 97, 88, 48) |
  | 12,000 | 722 ms | 2,927 ms | 2,777 ms (1,680, 414, 403, 219) |
  | 800, ten deep | 45 ms | 186 ms | 181 ms |

  Folding it again took up to 470 ms at 4x, and scrolling the open 12,000-entry folder ran at 15 frames a second at
  4x (67 ms a frame). WebView2 on a slow laptop is the 4x column or worse, which is Björn's second.
- **A clone behind the mirror's commit put the page on the forge for good.** knobas fetches the missing commit into
  the clone behind the read (ADR-0041), and it landed within about 15 s. The tab took the clone's tree on that event
  only while the source had not drawn yet, which it always had (its top answers in about 0.6 s). After a push the
  first answer is moreover the app's kept read at the commit before (ADR-0042), and the tab never noted the commit the
  read behind it moved to, so the clone could not have matched it anyway. A page kept alive stays so for the session,
  and every folder then went to Gitea.
- **Without a clone, Gitea's `contents` route is the floor.** It works out each entry's last commit: a folder of 300
  took 0.5 to 0.6 s, of 3,000 about 9 s, and of 12,000 never answered within the backend's 45 s. A folder read once is
  kept by the app, so a later press costs what the page does.

## Taken

- **The tree's rows are windowed** (`TfRepoFiles.svelte`, `treeRows`, `shell/windowed.svelte.ts`). The tree draws
  its flat rows (every listing read, an open directory's entries after it) through the app's windowed list from 120
  rows: the rows in view and a screenful each side, the rest room above and below at 24 px a row, so the tree scrolls
  exactly as far as before and the rows draw as it scrolls. The listings are raw state, replaced whole. The keys
  (#2311) reach a row past the window by drawing it first, and a row the window takes out while it has focus leaves
  focus on the tree, the keys going on from it.
- **A clone that holds the commit takes over in place.** `checkout:fetched` reads the clone's tree whenever the tab
  has none, the tab keeps the commit the source's tree stands at after each read behind it, and where that commit
  changed it reads the clone again; either order of the two ends with the clone's tree where it stands at the same
  commit. The rows stay where they are (the forge and the clone order a listing the same way), the file drawn at that
  commit stays drawn, and directory reads still under way are dropped so they do not write the forge's sections over
  the clone's.

## After (the same presses, head `757f40f4`)

| Folder | clone, 1x | clone, 4x | hand-made clone, 4x | clone behind, after the fetch, 4x |
|---|---|---|---|---|
| 300 | 28 ms | 28 ms | 20 ms | 19 ms |
| 3,000 | 27 ms | 27 ms | 26 ms | 28 ms |
| 12,000 | 29 ms | 26 ms | 28 ms | 28 ms |
| 800, ten deep | 29 ms | 26 ms | 28 ms | 28 ms |

Every press is 18 to 32 ms, two frames of the probe's 60 Hz, with 47 rows in the DOM; the main thread spent at most
30 ms on it at 4x. Folding is the same. Scrolling the open 12,000-entry folder holds 60 frames a second at 4x with no
gap in the rows (72 drawn). No row moved after the first draw in any press. Without a clone the first press of a
folder is still the forge's: 609 ms for 300 entries and 9.1 s for 3,000 at 1x and at 4x, then 21 to 32 ms on every
press after.

## Kept

`just open-budget --inner` makes the big-folder repositories for its run and removes them after it
(`testenv/perf/big-folders.mjs`, README *Big folders*): `folder-300`, `folder-3000`, `folder-12000` and `folder-deep`
in a repository knobas cloned, and `forge-folder-300` and `forge-folder-3000` in one with no clone, at 1x and 4x, on
the first press, a return and a return after every source's run. The lines (`budget-open.json`, `inner`) hold the
aim on every round with a clone, and `drawnMs`, the end of the frame that shows the rows, beside `usefulMs`, which is
taken at the start of that frame and so read 12 ms for the old tree's 2.7 s. Against the old tree the lines fail 7
presses.

## Left

- **A folder with no clone waits on Gitea's `contents` route**, seconds for thousands of entries and never for
  12,000. The whole tree the source reads for *Go to file* (`git/trees`, recursive, paged) holds every directory's
  entries; drawing a folder from it where it is read already is the next step, and it is a read of the forge's tree,
  so a change to the Gitea adapter's view or the tab's model, not taken here.
- **A repository of more than 20,000 entries.** The clone's tree read stops at 20,000 entries as the source's does
  (`checkout::local::ENTRY_CAP`), and the tab draws a cut tree as if whole: the directories past the cut are missing
  from their parent's listing and the one the cut fell in is short. Found reading the code, not measured. The fix is
  the backend answering a directory of the clone's kept tree on demand, uncapped, or the tab reading the directories
  the cut touched from the source.
- **The clone's whole tree crosses the IPC at the tab's open**: about 1 MB of JSON for 16,126 entries. Not part of a
  folder's press; on WebView2 it is the tab's open that pays it.
