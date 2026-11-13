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

## Round two: a repository past 20,000 entries, and a folder with no clone (the same day)

**Asked (Björn): "fix both"**, the two this ruling had left.

**How it was measured.** Three scratch repositories on the live Gitea on the WSL box, removed afterwards:
`big-folders` (16,127 entries: folders of 300, 3,000 and 12,000 and one 800 ten deep), and `bigger` and its twin
`bigger-forge` (56,168 entries: forty folders of 1,000 first, then folders of 300, 3,000 and 12,000 and one 800 ten
deep, all past the 20,000th entry). The windowless backend (release builds of `f3f6a8c4` and of this round) over a
copy of the scale database, `bigger` cloned by knobas; each throttle on a backend started fresh, so every first press
is a read knobas never made; folders pressed by `Enter` with the pointer off the tree (`folders.mjs --by-key`), so no
read is asked early. No seeding ran on the WSL box meanwhile.

**A clone's tree, cut and drawn as if whole.** With `bigger` cloned, the tree showed `a-bulk` and the README and
nothing else: every folder past the 20,000th entry was missing, with nothing saying so. The backend now keeps the
clone's tree uncapped (up to two million entries), indexed by folder, and answers one folder at a time
(`entity_local_dir`, additive) and *Go to file*'s paths (`entity_local_paths`); the tab reads the top at its open and
a folder when it opens. A folder of a tree the backend itself had to cut says it is not whole, and the tab reads it
from the source.

| `bigger`, cloned | 1x before | 1x after | 4x before | 4x after |
|---|---|---|---|---|
| the tab's open: what crosses the IPC | 1.22 MB, cut at 20,000 | 435 B | the same | the same |
| the tab's open: to its tree drawn | 259 ms | 39 to 166 ms | 117 ms | 115 to 125 ms |
| 300, 3,000, 12,000, past the cut, by `Enter` | missing | 37, 46, 47 ms | missing | 52, 51, 61 ms |
| the same, pressed again | missing | 29 to 30 ms | missing | 26 to 28 ms |
| 1,000, before the cut | 28 ms | 24 ms | 21 ms | 19 ms |

A folder's read costs the backend 0.4 ms (300 entries) to 15 ms (12,000, 756 KB). With the pointer resting on the
folder first, the read is under way before the press and every folder draws in 25 to 39 ms, the 800 ten deep
included.

**A folder with no clone.** Gitea's `contents` works out each entry's last commit. A folder is now its own level of
Gitea's tree, read by the tree's sha (`git/trees/{sha}` without `recursive`: a request a thousand entries, no last
commit), kept by that sha: the top by the commit's, a folder below one read by the sha that level named, any other
walked level by level from the top. Where *Go to file* has read the whole tree, a folder it holds is drawn from it
with no request; one past its cut says so (`read_alone`, additive on `Block::Tree`), and the tab says so under the
tree. The tree draws no last commit, so nothing a listing drew is lost (pinned against `contents`' answer). A folder's
cap goes from 1,000 entries to 20,000: before, a folder of 3,000 drew its first thousand, with nothing in the tab
saying so; a folder the source cuts now says so under the tree, with its page there. A first try drew every folder
from the whole tree read first; the tab's top then waited for it, eighteen requests at the repository's quota, 2.1 to
3.3 s where it was 0.26 to 0.71 s. Reading a folder's own level put the top back at 0.17 to 0.38 s.

| No clone, first press | 1x before | 1x after | 4x before | 4x after |
|---|---|---|---|---|
| 300 (16,127 / 56,168 entries) | 654 / 666 ms | 153 / 76 ms | 717 / 671 ms | 97 / 85 ms |
| 3,000 | 8.9 / 8.9 s (its first 1,000) | 230 / 205 ms | 8.9 / 8.9 s | 222 / 227 ms |
| 12,000 | never (45 s) | 612 / 676 ms | never | 609 / 576 ms |
| any, pressed again | 18 to 31 ms | 23 to 30 ms | 27 to 29 ms | 21 to 29 ms |

| No clone, every request held 1 s (`slow-forge.mjs`) | 1x before | 1x after | 4x before | 4x after |
|---|---|---|---|---|
| 300 | 1.7 / 1.6 s | 1.1 / 1.3 s | 1.7 / 2.1 s | 1.5 / 1.1 s |
| 3,000 | 12.7 / 12.8 s | 3.7 / 2.5 s | 12.8 / 13.3 s | 3.3 / 2.2 s |
| 12,000 | never | 6.9 / 5.8 s | never | 4.7 / 5.7 s |

`slow_forge_live.rs`' new witness reads the same folders through its proxy (`KNOBAS_BIG_FOLDERS_REPO`).

## Left

- **The 12,000-entry folder with no clone** takes twelve level pages: 0.6 s on the live Gitea, 4.7 to 6.9 s with every
  request held 1 s. A repository read whole once (*Go to file*) draws every folder it holds at once after.
- Both items this ruling first left (a clone's tree cut at 20,000 entries; a folder with no clone waiting on Gitea's
  `contents`) are taken in round two above, and the clone's whole tree no longer crosses the IPC at the tab's open.
