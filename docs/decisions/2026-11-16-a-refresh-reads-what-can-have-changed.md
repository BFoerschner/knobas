# 2026-11-16: The file list after an hour: a refresh reads what can have changed, not every folder ever opened

**Asked (Björn, on PR #2479, through the orchestrator):** "Hovering over the filelist on the left side is incredibly
slow, I can't reproduce it anymore, maybe have a fable agent look at it why? Maybe it's connected to the compare or
pullrequest initial lag. Oh, no it's not any action, but it just happens when you browse the code-page for a longer
time apparently? It just started lagging again. (work laptop, you cant see the logs). The project is cloned on the
disk if that helps narrow it down." With his standing instruction: find the cause, decide, write down why, fix it.
The targets (a hover or a key within 100, 100 and 200 ms at CPU 1x, 4x and 6x, a hover a frame) must hold after hours
of use, not only on a fresh start.

**How it was measured.** Headless Chrome on the Mac (M2 Pro, on AC power) over fresh APFS clones of the saved scale
database through a release build of the windowless backend, the front end built as the probes build it. The
repository browsed is the scale estate's `scale-commerce/cart-service` served by the stand-in forge, with one commit
more on `main` that brings in 9,721 files in 808 folders (this repository's own tree four times over), the mirror's
tip moved to it, and a clone of it pinned as the checkout: a cloned project, as Björn's is. A new probe,
`testenv/perf/soak.mjs`, browses its Code page as a person does for 30 minutes (two or three folders and two files a
lap, Commits and a commit, *New pull request*, a pull request and its files, other sections and back), ends a run of
every source every 30 seconds, and records after each lap what a fresh page never shows. Two builds side by side on two
backends: the branch's head with only the probe rig's own leak fixed (section 6), and the same with this pass, at
1440x868@1 and at 2560x1440@1.25. The machine was shared with other agents' builds throughout (a load of 4 to 9 for
the first pair, about 20 for the second and the tour), so read the second pair's fixed numbers as an upper bound.
Then the same pair for 15 minutes each on Björn's PC (Ryzen 5 5600X, Windows), in a headless Edge started by the
probe from a scratch folder, the PC idle and under its timing lock, the backend still the Mac's through an ssh tunnel
to the WSL box.

## 1. The cause

**Not the hover.** A hover at rest stayed one frame at every throttle after any amount of browsing: two to four
elements restyled a move (the row left and the row entered), 4.6 ms of main thread at 6x over the first ten laps and
5.2 over the last ten, 280 folders later. No listener, node, timer or heap growth explains it either (section 5).
What grew was the work started by two things that are no action of the reader's on the tree: **the end of a run of
the repository's source that changed something, and a return to the page** (from another section, or from another
tab of the repository to Files where it was left).

Both tell the open tab to read again (`TfRepo`'s `readShown`, the tab's `onRefreshed`), and Files' re-read
(`rereadTree`) asked for **every folder read so far**, open or folded, one IPC call each, and assigned `listings`
once per answer. Each assignment walks the whole tree again (`treeRows`), builds the windowed list's keys and its
offsets again (`Heights`' layout, a `Map` lookup a row), and hands every drawn row a new object. So a refresh cost
folders × rows, and both grow with every folder opened: after an hour's browsing, hundreds of answers each redrawing
thousands of rows, on every run's end and every return. It also read the open file again from the clone, drew it
again whole, coloured its lines again and read a README's every image again (12 `entity_file_bytes` a refresh on
this repository's landing), and it read the source's top twice.

The profile of one run's end at 6x with 300 folders opened (`Profiler` over 3.5 s, the pointer moving over the tree):
2,246 ms of main thread, 91.5 % script; 1,725 ms of it under Svelte's `flush`, of which the windowed list's
`slice` → `span` → the offsets' layout 1,029 ms (892 ms self), the tree's walk and the each block the rest; 301
`entity_local_dir` calls. A pointer moved over the tree meanwhile waited for it:

| A run's end at 6x, the pointer moving over the tree (`burst`, one run a cell) | 25 folders | 100 | 300 | 600 |
|---|---|---|---|---|
| before: IPC calls | 54 | 128 | 328 | 628 |
| before: hover, move to frame, max | 66 ms | 234 ms | 1,476 ms | 9,814 ms |
| after: IPC calls | 14 | 14 | 14 | 14 |
| after: hover, max | 27 ms | 23 ms | 26 ms | 22 ms |

| A return to the page at 6x | 100 folders | 300 | 600 |
|---|---|---|---|
| before: calls, hover max | 127, 274 ms | 327, 1,580 ms | 627, 7,401 ms |
| after: calls, hover max | 13, 24 ms | 12, 351 ms | 12, 59 ms |

(The 351 ms is the return's own first frame in one run, a 167 ms task while the page is shown again; the soak's
returns after the fix are in section 4, 22 to 190 ms.) Without a clone the same re-read went to the forge: at 138
folders a run's end asked the backend for 119 folder views, 35 of them requests to the forge, and a hover reached
183 ms; after, 3 views, 6 requests and 24 ms.

That is the report, word for word: it comes with browsing "for a longer time" (folders opened), it is "not any action"
(a run ends), it goes away (the burst ends) and comes back (the next run, the next return). WebView2 on Björn's laptop
runs the same script in about twice the Mac's time at 6x (the Windows pass, 2026-11-14), and Tauri's IPC sends the
whole burst at once where headless Chrome sends six calls at a time.

**The compare and pull request lag.** Not the same cause, but the same moment. Coming back to Files from Compare or
from a pull request is a return, so it started this burst; and a press in the seconds after a run's end or a return
(a file, *New pull request*, a tab) waited behind it like the hover did. A first Compare's own wait for the forge or
the clone is what `docs/decisions/2026-11-13-a-file-after-a-sync-and-a-first-compare-wait-on-nothing.md` and the
Compare passes cover, and is untouched here.

**The backend's half.** A folder asked at a commit is answered from the tree the backend keeps
(`checkout/local.rs`), eight trees, in the order they were read. A ninth tree read (another repository's landing,
the same repository at a new tip) dropped the oldest even while its page was in use, and then the burst's every call
found no tree and ran its own `git rev-parse` and `git ls-tree -r --long` over the whole repository: sixty folders
asked at once were sixty git processes and sixty parsed trees, 300 to 370 ms on twelve cores for these 9,721 files
with a read at 212 to 347 ms at the median, where one `git ls-tree` of them is 24 ms (72 to 108 ms for the sixty
with one read for all, section 2). Headless Chrome cannot show this (six at a time, the first keeps the tree); asked
of the backend directly it does. On a four-core laptop with a larger tree and Windows's process start it is seconds
of every core, the web view's included. Estimated, not measured there (*Owed on Windows*).

## 2. Decided

Björn's instruction for a decision in this pass is to ask Fable; these are that Fable's, with what each was weighed
against.

**A refresh reads the tree's top, and nothing under it that stands at a commit.** A clone's folder is read through a
section the tab makes from its commit, and a source's listing names its folders at a commit too (Gitea:
`files@<sha>:<dir>`), so such a listing cannot have changed; the top is where a branch that moved shows, and a top
that names new sections already reads the open folders again through them (`rereadTop`, `land`). This is the rule the
tab had for files since the WSL block (2026-11-14, `keepFile`), which the folders never got. A folder read by its
path alone (a source that names no commit, or one before its listing named it) may have changed and is still read
again; its answers are drawn once, together, and only where one differs from what is drawn.

- *Weighed: read every folder again, draw once.* Keeps the reads: 300 IPC calls a refresh, and without a clone as
  many asks of the backend's held views. The redraws were the larger cost, the reads the part that grows on the
  forge's side. Taken only for the folders that can have changed.
- *Weighed: read again only the open folders.* Still grows with use (a reader who folds nothing), and a folded
  folder's listing would need dropping to stay right. Not needed once what cannot change is not read.
- *Weighed: make one redraw cheap* (the offsets kept across a change of rows, the walk incremental). Worth having
  for a folder's own open at 8,000 rows, 11 ms at 6x now; not the fault, and not taken here.

**The clone's file at its commit is not read again either**, and a refresh reads the source's top once. A file from
the clone was never kept (it answers in milliseconds), so every refresh read and drew it again.

**A clone's takeover is one read a folder.** The clone's own read and the source's top both say a branch moved, a
moment apart, and each read every folder at the new commit: twice the calls. The second joins the first (`land`).

**The backend reads a tree once for everyone asking, and drops the tree least recently used.** Asks for a tree being
read wait for that read and take its answer; each asker waits the read's own timeout at most. A tree found is the
newest again, so the tree a reader is in stays while they are in it. Additive, in `checkout/local.rs` only: no
command, type or migration changes.

- *Weighed: leave the backend, since the page no longer sends the burst.* The page was one caller; the next one
  (a deep link's folders, a feature not written yet) would find the same sixty processes. A cache that starts N reads
  of one value is wrong whoever asks.

**Not changed: the answer cache's warm.** After a run that changed something it asks again, one at a time, up to 120
recent reads, and a pointer moving over a page does not hold it back as a key or a wheel does (`ahead.ts` counts the
pointer; the warm does not). Suspected, measured, and left: with the fix in, a hover through a run's end at 6x is 22 to
25 ms at the 95th percentile with the warm running behind it, on a mirror of 39,770 items.

## 3. What changed

- `TfRepoFiles.svelte`: `rereadTree` skips a folder whose section names where it stands (`standsAt`) and draws the
  rest's answers once; `rereadFile` skips a clone's file and leaves the top to the tree's read on a refresh; `land`
  joins a takeover under way at the same commit.
- `crates/knobas-app/src/checkout/local.rs`: `READING`, one read a checkout and commit; `used`, a tree found moves to
  the newest end.
- `shell/dev/fake-tauri.ts`: a listener dropped drops its callback (section 6).
- `testenv/perf/soak.mjs`, `soak.sh`, `soak-repo.sh`, `just soak`, `budget.json`'s `soak` and `soak-tour` lines, and
  the README's *The soak*.
- Tests that fail before: `TfRepoFiles.local.test.svelte.ts` (two refreshes of a clone's tree with 40 folders open:
  82 folder reads before, 2 now; a moved branch: 15 reads for 5 folders before, 5 now), `TfRepoFiles.test.svelte.ts`
  (a forge's tree at a commit; folders read by path drawn together), `local.rs`
  (`folders_asked_at_once_read_the_tree_once`: 24 `git ls-tree` before, 1 now;
  `a_tree_in_use_outlives_the_trees_read_since`), `fake-tauri.test.ts`.

## 4. The soak, before and after

Thirty minutes each, the same seeded browsing, a run of every source ended every 30 s. Each cell is the median over a
five-minute window; a hover is from the mouse move's own time to the end of the frame that draws it, with the pointer
kept moving over the tree's rows, at CPU 6x.

| 1440x868@1 | 0 to 5 min | 5 to 10 | 10 to 15 | 15 to 20 | 20 to 25 | 25 to 30 |
|---|---|---|---|---|---|---|
| folders opened by then (before / after) | 59 / 59 | 106 / 108 | 152 / 155 | 201 / 204 | 247 / 247 | 278 / 282 |
| before: IPC calls a run's end | 58 | 113 | 144 | 206 | 256 | 292 |
| before: hover through a run's end, p95 / max | 31 / 313 ms | 144 / 704 | 238 / 478 | 581 / 839 | 935 / 1,580 | 1,162 / 1,583 |
| before: hover through a return, p95 / max | 118 / 281 ms | 223 / 322 | 301 / 458 | 563 / 791 | 845 / 1,497 | 1,411 / 2,531 |
| before: calls out at once, most | 72 | 122 | 166 | 210 | 267 | 298 |
| after: IPC calls a run's end | 24 | 25 | 25 | 25 | 25 | 25 |
| after: hover through a run's end, p95 / max | 22 / 58 ms | 25 / 52 | 22 / 35 | 22 / 34 | 24 / 51 | 23 / 26 |
| after: hover through a return, p95 / max | 25 / 188 ms | 147 / 182 | 23 / 163 | 22 / 141 | 27 / 170 | 50 / 190 |
| after: calls out at once, most | 18 | 18 | 18 | 18 | 18 | 18 |

| 2560x1440@1.25 (the machine at a load of 20) | 0 to 5 min | 5 to 10 | 10 to 15 | 15 to 20 | 20 to 25 | 25 to 30 |
|---|---|---|---|---|---|---|
| folders opened by then (before / after) | 47 / 47 | 75 / 77 | 124 / 129 | 165 / 165 | 199 / 199 | 206 / 214 |
| before: IPC calls a run's end | 59 | 92 | 127 | 180 | 201 | 226 |
| before: hover through a run's end, p95 / max | 74 / 157 ms | 181 / 242 | 374 / 463 | 636 / 933 | 886 / 1,085 | 1,122 / 1,612 |
| before: hover through a return, p95 / max | 195 / 287 ms | 244 / 306 | 372 / 700 | 732 / 1,443 | 1,118 / 1,537 | 1,149 / 1,662 |
| before: a read's wait in those two seconds, p95 | 725 ms | 530 | 889 | 1,226 | 1,539 | 2,062 |
| after: IPC calls a run's end | 25 | 25 | 25 | 25 | 24 | 20 |
| after: hover through a run's end, p95 / max | 40 / 112 ms | 33 / 56 | 42 / 56 | 32 / 82 | 27 / 66 | 26 / 56 |
| after: hover through a return, p95 / max | 131 / 266 ms | 142 / 265 | 144 / 304 | 140 / 302 | 118 / 298 | 126 / 166 |
| after: a read's wait, p95 | 648 ms | 373 | 202 | 253 | 292 | 160 |

| A hover at rest, move to frame / main thread's share, medians of the laps' 95th percentiles | 1x | 4x | 6x |
|---|---|---|---|
| 1440x868, before, first ten laps and last ten | 15.8 / 0.8 and 15.1 / 0.9 ms | 15.5 / 3.4 and 16.3 / 3.4 | 15.6 / 4.6 and 20.7 / 5.2 |
| 1440x868, after | 15.9 / 0.8 and 15.1 / 0.8 | 16.1 / 3.5 and 15.8 / 3.4 | 19.6 / 4.9 and 20.4 / 5.5 |
| 2560x1440@1.25, before, the run's 95th percentile | 19.8 / 3.3 | 25.4 / 8.9 | 31.6 / 12.4 |
| 2560x1440@1.25, after | 21.1 / 3.4 | 24.4 / 7.1 | 29.8 / 11.5 |

The frame's own wait is in *move to frame*: up to 16.7 ms at 60 Hz. A hover at rest is within one frame or two at
every throttle and size, before and after; through a run's end it is now too. A return's hover holds the return's
own first frame, a task of 60 to 150 ms at 6x while the page is shown again, which `open-budget`'s return rounds
hold (300 ms at 6x); it no longer grows.

| Edge on the PC, 1440x868@1, 15 minutes | 0 to 5 min | 5 to 10 | 10 to 15 |
|---|---|---|---|
| folders opened by then (before / after) | 57 / 57 | 105 / 106 | 147 / 150 |
| before: IPC calls a run's end | 59 | 110 | 145 |
| before: hover through a run's end, p95 / max | 25 / 106 ms | 101 / 276 | 272 / 428 |
| before: hover through a return, p95 / max | 142 / 305 ms | 260 / 354 | 324 / 458 |
| after: IPC calls a run's end | 25 | 25 | 25 |
| after: hover through a run's end, p95 / max | 13 / 31 ms | 14 / 36 | 15 / 35 |
| after: hover through a return, p95 / max | 109 / 179 ms | 20 / 208 | 19 / 152 |
| a hover at rest, 1x / 4x / 6x, before and after alike | 6.8 / 9.6 / 10.9 ms | 6.9 / 9.7 / 11.2 | 6.8 / 9.4 / 11.1 |

The PC's Edge shows the same curve as the Mac's Chrome: 176 calls and 428 ms by the fifteenth minute before, 27 calls
and 36 ms at most after, and a hover at rest that never moved. Its returns hold the return's own first frame there too
(a page's line at 6x is 300 ms).

**With a real sync running** (15 minutes on the fixed build, 1440x868, the Gitea source synced again each time a run
ended, 235 runs and about 10,000 requests on the stand-in, beside the other two soaks): 25 calls a run's end
throughout, a hover through it 35 to 53 ms at the 95th percentile and 108 at most, a read's wait 220 to 450 ms at 6x,
the heap 0.8 MB up.

**What the page keeps**, after a collection, the median of the last third of the collected laps less that of the first:

| | nodes | DOM listeners | heap | Tauri listeners | idle main thread |
|---|---|---|---|---|---|
| 1440x868, before | +3,269 (of about 28,000) | -62 (of about 600) | +4.9 MB (26 to 31) | 49 to 51 | 20 ms a minute |
| 1440x868, after | +3,299 | +122 | +5.3 MB | 49 to 52 | 18 |
| 2560x1440@1.25, before | +1,577 | +39 | +4.5 MB | 52 to 48 | 29 |
| 2560x1440@1.25, after | +2,799 | +28 | +5.3 MB | 52 to 49 | 33 |

The 5 MB over half an hour is the same before and after: 280 folders' listings, the answers kept, the pages held.
Nothing in it grows with time alone (the tour, section 5).

## 5. What else was looked for

The suspects the report left open, each ruled in or out by a count:

- **Listeners, effects, subscriptions not torn down.** Out. DOM listeners after a collection move with the page kept
  (500 to 800) and do not climb; Tauri's event listeners registered and not dropped stay at 48 to 52 over 150 laps.
- **Kept pages and the sections built ahead.** Out. The document holds 5,400 to 7,200 elements throughout, the nodes
  kept 23,000 to 33,000.
- **Caches without bounds.** Out for the page: the files drawn (64), the answers (400) and the recent reads (120)
  are capped. In for the backend's trees, which were capped at eight but dropped in the wrong order (section 2).
- **Detached DOM.** Out for the app; in for the probe rig (section 6).
- **Timers.** Out. Nothing pressed, the main thread works 15 to 60 ms a minute at every point of the run.
- **A repository page that asks the forge after each sync.** In: this is the trigger. What it asked for was the fault.
- **The clone's watcher or `git status`.** Out. The checkout is read once on a return (`entity_checkout`), on lap 2
  and on lap 150 alike. A return asked 24 calls on lap 2 and 288 on lap 150 before, 276 of them folders; 17 and 11
  after.
- **A style or layout invalidation that grows with the DOM, `:has()`.** Out. Under a trace a hover restyles two to four
  elements on lap 1 and on lap 140, at both sizes.
- **GC pauses.** Out. The heap is 26 to 34 MB after a collection.
- **The backend's IPC queue behind background fills.** Out as a cause, in as a symptom: before, 298 calls were out at
  once at a run's end and a read waited 2 s at 6x behind the page's own burst; after, 18 and 160 to 650 ms on a
  machine at a load of 20, 220 to 450 with a sync running.

**The rest of the app** (`soak.mjs --tour`, the fixed build, eight laps over 19 pages and three more runs of twelve
laps over nine each): the sections, a pipeline, a run, a space, a doc, the repository's Branches, Commits, Pull
requests and Builds, a pull request, its files and its conversation, each visited again and again with what it
offers more of pressed and a run ended on each visit.

- No page asks more at a run's end as it is used: every page's calls are level from its third visit (Tickets 17
  after 24 presses of *Show 50 more* and 1,200 more cards, Builds 18, a pipeline 18, a pull request's tabs 16;
  Branches 103 from its third visit, its branches' standings).
- A hover through a run's end at 6x, the slowest move of each visit from the third lap on: 21 to 29 ms at the median
  of all visits in the four runs of nine pages, 72 to 147 ms at worst (Code, a space and a pull request's overview,
  once each). In the run of 19 pages at a load of 20: 34 ms at the median, 133 at worst but for one 268 on Code in
  the last lap. Björn's line for a hover at 6x is 200.
- The nodes and the heap kept are level where nothing is pressed (the repository's tabs and five sections: 13,803 to
  13,997 nodes and 17 MB over eight laps; a space, a doc and Code's pull requests: 14,356 nodes on every one of
  twelve laps). Where the tour presses, the page keeps what was asked for: Tickets' heap rose 16 to 27 MB with the
  1,200 cards and no node more (the board windows them), and a pull request's page rose 18,800 to 31,200 nodes while
  its files' context was expanded and stayed there from the eighth lap, when nothing was left to expand.

Nothing else was found that grows with use. One thing found and left: Branches asks 103 calls at a run's end on a
repository of 57 branches (its standings, read again each time). It does not grow, and its hover held (66 ms).

## 6. The probe rig's own leak

The first soak, on the branch's head as it stood, grew by 2,500 nodes and 1.2 MB a minute (19,000 to 77,000 nodes and
19 to 45 MB in 21 minutes) with the document's elements level. The fixture's bridge (`fake-tauri.ts`) kept every
callback `transformCallback` registered after its listener was dropped, so every `listen()` a page had made held its
handler and, through it, the page. Tauri's own `unregisterListener` calls `unregisterCallback` for the listener's
handler, so the app's window does not hold them; the bridge does the same now. Every number above is with that fixed
in both builds.

## Owed on Windows

- Done on the PC in this pass: the soak in Edge, WebView2's engine, before and after (section 4). Not done: the
  app's own window, which needs a build with the fix in it.
- The app's own window on the work laptop, on the next pre-release: an hour on the cloned repository with its real
  syncs, the pointer on the tree when a Gitea run ends. Tauri's IPC sends a burst at once, which a browser's six
  connections never did; with the fix there is no burst to send, and that is the part no rig here can show.
- The backend's one read a tree on Windows, where a `git` start costs more: the Mac's 300 to 370 ms for sixty folders
  against 72 to 108 is a floor for what it saves there.

## Gates

Björn's trimmed gates of 2026-11-16 (the full timing matrix runs once a pre-release, not in every pass), on the
pushed head `b0f86759` in a clean clone. Timing and UI runs held the machine's timing lock, one run at a time.

- `just check-front`: ok. `svelte-check` 0 errors over 1,479 files, vitest 5,236 passed, 1 skipped, in 342 files.
  `just check-scripts`: ok.
- Press trips with `--strict`, under the Mac's lock: 272 of 272, the 16 `rows-still` trips among them.
  `bash testenv/desktop-witness-test.sh`, under the lock: ok, 307 checks. (Started first from a shell job, it
  failed 15: a job started with `&` hands its children SIGINT ignored, and the test stops its stand-in postmaster
  with SIGINT. Started with the signal at its default, it passes.)
- `pixel-same.py`: not run, nothing drawn changed.
- Rust: `just clippy` and `cargo fmt --all --check` clean; `cargo test -p knobas-app`, the one crate changed: 1,625
  passed, 0 failed, 83 ignored, in 16 targets. (Before the trim, on `2f2d713c`: `just clippy-libs` clean, and the
  whole `just test` 4,974 passed with 2 timeouts outside this change that pass alone, at a load over 50.)
- **Timing, for what this pass touched.**
  - The soak (`soak.mjs --check budget.json --lines real`, ten minutes, 1x, 4x and 6x) on Björn's PC, idle and under
    its lock, in a headless Edge, the backend the Mac's through a tunnel: every line held at both sizes.

    | The PC, final head | 1440x868@1 | 2560x1440@1.25 |
    |---|---|---|
    | folders opened | 112 | 91 |
    | IPC calls a run's end, and their growth | 25, -6 | 25, +4 |
    | hover through a run's end at 6x, p95 / max | 22 / 25 ms | 35 / 41 ms |
    | hover through a return at 6x, p95 | 197 ms | 231 ms |
    | hover at rest, move to frame, 1x / 4x / 6x | 7.1 / 10.4 / 12.6 ms | 7.2 / 10.5 / 12.9 ms |
    | its main-thread share, 1x / 4x / 6x | 1.0 / 4.6 / 6.9 ms | 1.1 / 4.6 / 7.8 ms |
    | nodes, listeners, heap kept, growth | +3,021, +87, +2.6 MB | +3,739, +219, +2.3 MB |

  - The Files tab's opens (`open.mjs --set inner`: the landing, a deep file, *Go to file*, a small, a large and a
    README file; first, return and after a sync; 1x, 4x and 6x) on the Mac under its lock, against the stand-in
    forge, a fresh backend a size: 54 of 54 within their lines at 2560x1440@1.25, 53 of 54 at 1440x868@1. The one
    over was *Go to file*'s first use at 4x, 2,882 ms with a 1.2 s task in it while the machine's load went from 9
    to 30 (98 ms at 1x and 142 at 6x in the same run); alone under the lock on two fresh backends, at a load of 5,
    it drew in 70 and 80 ms, line 200. The landing's first open, the forge's stand-in holding each request 30 ms:
    465, 166 and 524 ms at 1x, 4x and 6x at the first size; a return 0 to 12 ms.
- Before the lock existed (six sessions on the Mac at once, a load of 45 to 98), `just frame-budget`, its `--real`
  run, `just open-budget --real` and `just keys-budget --real` each ran once and read that load, the base commit going
  over on the same cells side by side (Monitors' scroll at 6x 41.9 frames a second on base and 42.6 on head, Tickets'
  48.8 and 48.3). Those runs are not this pass's gates any more; the pre-release's run of the whole matrix is.
