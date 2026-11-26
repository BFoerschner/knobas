# 2026-11-12: The inner pages draw what knobas holds, and the forge's answer lands in place

**Asked (Björn, on PR #2479):** "did you check the inner pages as well, when opening a repository itself etc? that took a long time too." The earlier passes of the day measured the nav pages and a repository's landing with every source pointed at a dead port, so nothing that waits on a forge, git or the network showed.

**Measured.** The windowless backend (`examples/headless_ipc.rs`, optimized, with debug assertions on so it reads the dev profile's credentials; a release build reads the keychain only) on an APFS clone of the saved scale database, its Gitea source synced from and reading the **live Gitea on the WSL box** over the tunnel, through `testenv/perf/slow-forge.mjs` (no delay, or every request held 1 s), with and without the 65 repositories cloned into a scratch clones root, pressed in headless Chrome with real input by `testenv/perf/open.mjs --set inner` at CPU 1x and 4x. Each condition starts from the same synced profile, so a place's first open is a read knobas never made, and the backend's own sync at start loads the forge as the app's does. Rounds: the first open (1.5 s after start), a return, a return after every source ended a run, and the first open after the backend was started again (opened in an earlier session). Jira, Confluence and TeamCity were stopped on the WSL box for most of the pass (its memory budget), so a run's tabs were measured with TeamCity not answering.

Each cell: the first open · a return · a return after every source ended a run, in ms unless marked, before (`42913369`) → after; ↕ rows that moved after the first useful frame; *never*: not useful within the probe's 26 s. After is the final front end (`e0d4692f`) for the live forge at 1x, and `a9019327` (before the follow-ups: files kept across mounts, the header's first look deferred, the clone's tree taking over) for 4x, the slowed forge and the forge down. How to read them:

- **A first open with no clone** of what only the forge holds is the forge's time, and varies with its load: the after runs met a Gitea that had just rebooted (cold) and the start's own sync. The places the mirror now answers are the ones to compare: the landing (4.0 s → 0.5 s; 14.9 s → 0.1 s with a clone), and, with the forge down, the branch list and last commit drawn at all.
- **Seconds where the backend answered in milliseconds are the rig**, not the app: headless Chrome sends at most six requests to the backend at once, so with the forge slowed a page's local reads queued behind the page before's forge reads (a commit's page 14.2 s with a clone, its diff answered by the backend in 80 ms). Tauri's IPC has no such cap. The forge-slowed and forge-down columns of the after build carry this, more than before, because the header's branch search started at once there; the follow-up defers it.
- **Rows that move**: Branches' rows as each branch's standing lands (fixed in `228b97d9`, after these runs, not yet measured live) and one or two lines of *Files changed*, where the forge's diff replaces the clone's (not fixed).
- `run-*` before at 1x with no clone, and `pr-files-large` before at 1x with no clone, were measured with rules since corrected (a run tab's failure line counted, the second pull request's two files); `compare-large` and `pr-conversation` at 4x wait on a side panel that waits on each branch's whole view, longer than the cap under a sync's load; `pr-conversation` with a clone at 1x after is a round in which the window had not finished starting.
- The forge-down before run with no clone hit the probe's 45-minute watchdog in its 4x pass; at 1x it is complete.

**Live forge, CPU 1x** (first open · return · return after a sync; ms unless marked, before → after, ↕ rows moved)

| Page | no clone | with clone |
|---|---|---|
| repo-landing | 4.0 s → 487 · 8 → 3 · 8 → 10 | 14.9 s → 117 · 5 → 3 · 9 → 16 |
| files-deep | 1.5 s → 6.6 s · 202 → 3 · 12 → 244 | 4.3 s → 51 · 250 → 35 · 22 → 39 |
| files-go-to | 129 → 833 · 12 → 14 · 13 → 15 | 118 → 11 · 7 → 2 · 14 → 19 |
| file-small | 175 → 171 · 22 → 1 · 19 → 17 | 930 → 42 · 14 → 32 · 15 → 51 |
| file-large | 409 → 1.7 s · 172 → 3 · 13 → 651 | 54 → 27 · 38 → 44 · 52 → 51 |
| file-readme | 24 → 35 · 29 → 31 · 23 → 23 | 18 → 38 · 29 → 47 · 24 → 40 |
| file-binary | 353 → 371 · 93 → 13 · 23 → 226 | 436 → 46 · 99 → 40 · 16 → 36 |
| repo-branches | 27 → 27 · 28 → 30 · 24↕7 → 22 | 28 → 32↕14 · 30 → 22 · 25 → 30 |
| repo-commits | 17 → 17 · 24 → 28 · 27 → 24 | 23 → 32 · 22 → 23 · 24 → 20 |
| repo-commit | 5.0 s → 5.3 s · 9 → 14 · 5 → 2 | 114 → 88 · 15 → 4 · 3 → 21 |
| repo-compare | 1.3 s → 3.1 s · 24 → 25 · 24 → 22 | 90 → 87 · 28 → 24 · 27 → 24 |
| compare-large | never → never · never → never · never → never | never → never · never → never · never → never |
| new-pull-request | 255 → 203 · 45 → 45 · 30 → 27 | 83 → 105 · 29 → 32 · 21 → 30 |
| pr-overview | 28 → 30 · 2 → 9 · 9 → 12 | 29 → 22 · 3 → 5 · 4 → 16 |
| pr-conversation | 18 → 21 · 24 → 22 · 21 → 23 | 18 → never · 28 → never · 27 → never |
| pr-files | 326 → 454 · 29 → 26 · 26 → 24 | 89↕1 → 106↕1 · 22 → 22 · 25 → 29 |
| pr-commits | 13 → 2 · 12 → 5 · 8 → 6 | 2 → 5 · 6 → 10 · 7 → 12 |
| pr-files-large | never → 3.4 s · never → 22 · never → 28 | 73↕2 → 93↕2 · 22 → 26 · 26 → 27 |
| run-log | never → 1 · never → 5 · never → 14 | 5 → 2 · 4 → 9 · 13 → 14 |
| run-tests | – → 8 · – → 9 · – → 12 | 7 → 16 · 6 → 5 · 13 → 13 |
| run-artifacts | – → 4 · – → 2 · – → 14 | 5 → 5 · 7 → 7 · 13 → 14 |

**Live forge, CPU 4x** (first open · return · return after a sync; ms unless marked, before → after, ↕ rows moved)

| Page | no clone | with clone |
|---|---|---|
| repo-landing | 6.2 s → 897 · 11 → 8 · 5 → 17 | 233 → 940 · 14 → 1 · 4 → 16 |
| files-deep | 1.9 s → 456 · 3 → 10 · 6.7 s → 192 | 47 → 54 · 27 → 33 · 32 → 25 |
| files-go-to | 79 → 94 · 16 → 6 · 17 → 19 | 268 → 153 · 17 → 6 · 8 → 3 |
| file-small | 3.6 s → 409 · 231 → 10 · 11 → 1.5 s | 43 → 515 · 24 → 13 · 24 → 4.9 s |
| file-large | 943 → 406 · 930 → 4 · 13 → 165 | 2.4 s → 408 · 175 → 15 · 8 → 173 |
| file-readme | 49 → 62 · 31 → 30 · 28 → 28 | 50 → 53 · 46 → 28 · 45 → 18 |
| file-binary | 1.5 s → 391 · 535 → 12 · 7 → 225 | 0↕1 → 61 · 469 → 26 · 14 → 28 |
| repo-branches | 38↕14 → 51↕14 · 28 → 29 · 23 → 26 | 68↕13 → 72↕14 · 28 → 28 · 27 → 29 |
| repo-commits | 48 → 57 · 45 → 61 · 45 → 52 | 58 → 67 · 45 → 45 · 46 → 45 |
| repo-commit | 571 → 115 · 11 → 13 · 9 → 10 | 112 → 122 · 3 → 22 · 8 → 12 |
| repo-compare | 1.2 s → 217 · 28 → 28 · 28 → 28 | 135 → 143 · 29 → 20 · 28 → 29 |
| compare-large | never → never · never → never · never → never | never → never · never → never · never → never |
| new-pull-request | 6.7 s → 668 · 30 → 30 · 27 → 28 | 124 → 149 · 24 → 30 · 28 → 29 |
| pr-overview | 48 → 78 · 10 → 12 · 1 → 5 | 63 → 86 · 10 → 10 · 15 → 16 |
| pr-conversation | never → never · never → never · never → never | never → never · never → never · never → never |
| pr-files | 320 → 416 · 29 → 30 · 23 → 24 | 85↕2 → 123↕2 · 28 → 28 · 29 → 18 |
| pr-commits | 8 → 0 · 12 → 12 · 11 → 13 | 15 → 6 · 13 → 12 · 12 → 12 |
| pr-files-large | never → 383 · never → 30 · never → 23 | 85↕1 → 115↕1 · 29 → 27 · 27 → 18 |
| run-log | 3 → 6 · 12 → 11 · 12 → 10 | 4 → 10 · 12 → 10 · 11 → 10 |
| run-tests | 11 → 1 · 12 → 11 · 16 → 11 | 6 → 1 · 12 → 11 · 12 → 11 |
| run-artifacts | 0 → 0 · 12 → 11 · 12 → 11 | 1 → 12 · 11 → 11 · 2 → 10 |

**Forge holding every request 1 s, CPU 1x** (first open · return · return after a sync; ms unless marked, before → after, ↕ rows moved)

| Page | no clone | with clone |
|---|---|---|
| repo-landing | 8.8 s → 16.6 s · 11 → 16 · 8 → 14 | 6.0 s → 5.4 s · 12 → 11 · 9 → 12 |
| files-deep | never → never · never → 22.8 s · 10.2 s → 5.9 s | 29 → 54 · 33 → 28 · 59 → 43 |
| files-go-to | 11.7 s → 17.4 s · 16 → 3 · 9 → 10 | 16.0 s → 6 · 16 → 16 · 5 → 15 |
| file-small | 9.6 s → 11.7 s · 10 → 9 · 12 → 11 | 10.6 s → 39 · 7 → 33 · 9 → 42 |
| file-large | never → never · 24.7 s → 10 · 6.1 s → 13.3 s | 37 → never · 36 → 26.5 s · 24 → 7.9 s |
| file-readme | 21 → 17 · 31 → 30 · 28 → 27 | 45 → 21 · 48 → 30 · 44 → 22 |
| file-binary | never → never · 17.2 s → 13.7 s · 7 → 12 | never → 46 · 15.7 s → 40 · 10 → 37 |
| repo-branches | 23↕14 → 20↕14 · 26 → 23 · 24 → 28 | 33↕13 → 31↕14 · 22 → 25 · 27 → 28 |
| repo-commits | – → – · – → – · – → – | – → – · – → – · – → – |
| repo-commit | 29.1 s → 18.1 s · 12 → 5 · 3 → 11 | 95 → 14.2 s · 8 → 21 · 16 → 7 |
| repo-compare | 14.7 s → 17.6 s · 29 → 25 · 24 → 23 | 88 → 85 · 22 → 24 · 28 → 28 |
| compare-large | never → never · never → never · never → never | never → never · never → never · never → never |
| new-pull-request | 10.8 s → 9.8 s · 30 → 30 · 30 → 22 | 86 → 105 · 31 → 30 · 24 → 23 |
| pr-overview | 23 → 34 · 17 → 0 · 11 → 3 | 20 → 24 · 5 → 15 · 9 → 16 |
| pr-conversation | never → never · never → never · never → never | never → never · never → never · never → never |
| pr-files | 4.3 s → 6.4 s · 30 → 20 · 30 → 28 | 87 → 90 · 87 → 89 · 21 → 24 |
| pr-commits | 12 → 0 · 13 → 6 · 14 → 7 | 9 → 3 · 9 → 8 · 6 → 1 |
| pr-files-large | 4.3 s → 6.2 s · 31 → 24 · 30 → 26 | 90 → 90 · 89 → 106 · 20 → 29 |
| run-log | 5 → 9 · 7 → 4 · 14 → 14 | 0 → 5 · 8 → 6 · 13 → 13 |
| run-tests | 7 → 12 · 6 → 9 · 13 → 14 | 5 → 5 · 9 → 4 · 14 → 13 |
| run-artifacts | 6 → 3 · 7 → 5 · 12 → 13 | 7 → 4 · 5 → 8 · 14 → 14 |

**Forge down (refusing), CPU 1x** (first open · return · return after a sync; ms unless marked, before → after, ↕ rows moved)

| Page | no clone | with clone |
|---|---|---|
| repo-landing | never → never · never → never · never → never | – → 816 · – → 24 · – → 13 |
| files-deep | never → never · never → never · never → never | – → 48 · – → 54 · – → 47 |
| files-go-to | never → never · never → never · never → never | – → 6 · – → 35 · – → 15 |
| file-small | never → never · never → never · never → never | – → 57 · – → 53 · – → 34 |
| file-large | never → never · never → never · never → never | – → 42 · – → 44 · – → 30 |
| file-readme | – → – · – → – · – → – | – → 51 · – → 46 · – → 44 |
| file-binary | never → never · never → never · never → never | – → never · – → never · – → never |
| repo-branches | never → 27 · never → 28 · never → 30 | – → 32 · – → 24 · – → 22 |
| repo-commits | never → never · never → never · never → never | – → never · – → never · – → never |
| repo-commit | never → never · never → never · never → never | – → 77 · – → 2 · – → 10 |
| repo-compare | never → never · never → never · never → never | – → 114 · – → 31 · – → 31 |
| compare-large | never → never · never → never · never → never | – → never · – → never · – → never |
| new-pull-request | – → never · – → never · – → – | – → 94 · – → 32 · – → – |
| pr-overview | 27 → 32 · 1 → 6 · 12 → 8 | – → 21 · – → 24 · – → 4 |
| pr-conversation | never → never · never → never · never → never | – → never · – → never · – → never |
| pr-files | never → never · never → never · never → never | – → 109 · – → 108 · – → 80 |
| pr-commits | 4 → 8 · 7 → 8 · 13 → 14 | – → 10 · – → 14 · – → 14 |
| pr-files-large | never → never · never → never · never → never | – → 102 · – → 77 · – → 79 |
| run-log | 0 → 12 · 14 → 3 · 13 → 13 | – → 10 · – → 14 · – → 14 |
| run-tests | 9 → 2 · 13 → 10 · 13 → 14 | – → 10 · – → 13 · – → 13 |
| run-artifacts | 11 → 0 · 13 → 11 · 14 → 12 | – → 11 · – → 13 · – → 13 |

**Why the inner pages waited, page by page.**

- **A repository's landing**, first open: its branch list (2.8 s) and its last commit (3.9 s) came from the forge, which the header's *Branch*, *New pull request*, Compare and the tabs' bookkeeping all wait for, and the last commit's skeleton kept the landing waiting even where the clone drew the tree and the README in 70 ms. The mirror holds every branch's record, tip commit included, from the last sync.
- **A file opened by its address**, and a file in a folded directory: read by its path as a guess, then once its directory's listing arrived read again under the listing's section, which names the commit. A second round trip, 160 to 190 ms on the idle forge, seconds on a loaded one, on every return to it.
- **A file, a deep directory, a commit's diff, Compare and a pull request's *Files changed*** on a repository with no clone: what only the forge holds, read the first time and held after (ADR-0042).
- **Branches** drew its list, then each branch's standing (ahead and behind, its pull request, its actions) as that branch's whole view landed, a few at a time, 8 to 27 s each while a sync loaded the forge; each landing grew its row and moved the rows under it.
- **The held line** went when the fresh answer landed and took its line with it: the list under it moved up.
- **The repository page's tab bar** shifted 19 px on about one cold first open in two (the open budget's `1x repo first: 1 rows moved`): the header drew before the repository's row and its checkout, with an empty title and sub-line.
- **A branch list that failed** drew no tab at all.

**Taken.**

1. **The mirror draws a repository's branches and last commit** (`Source::mirror_listing`, ADR-0042 as amended): a repository never read answers its branch list from the branch records with the source's own rows, and its commits with the default branch's tip, under the held line, and the forge's answer lands behind it. Pinned row for row against the source's read, and live through the proxy holding every request (`slow_forge_live.rs`).
2. **Files keeps each file it drew under a listing's section** (`drawn-files.ts`, the newest 64, for the window's life, so a page mounted again has them too), and the listing's section stands for the guess's read; one read by its path alone is read again, as before; a refresh forgets them.
2a. **The clone's tree takes over when the source's top answered first** at the same commit (ADR-0041 as amended): the app keeps the top, so on a later open it beat `git ls-tree`, and every file was then read on the forge (3.8 s for one, with a clone on disk).
2b. **The header's first look for a branch to propose waits a second** on a repository's first open in the session: the branch list now comes at once, and its four branch views, each a compare on the forge, went out beside the page's own first reads and slowed the tree with no clone.
3. **Reads start while the pointer rests**: on a file or a folded directory in the tree, and on a link to a repository or a pull request's *Files changed*, 80 ms after the pointer stops, once; the press takes the read under way, and the page's own ask joins it while it is out (`readViewAhead`).
4. **Nothing moves when the forge answers**: the repository's header holds its title's and sub-line's room (the open budget's `1x repo first: 1 rows moved`, now 0 in four runs at 1x and 4x), Branches' columns are set by its header with one line a cell and its rows hold a small button's height, and the held line keeps its room once drawn.
5. **A failed branch list** draws the tabs at the mirror's default branch.
6. **`just open-budget --inner`** presses these places, live or with `--forge <ms>`, against `inner` and `forge` lines in `budget-open.json`.

**What still waits on the forge, and why.**

- **What only the forge holds, the first time it is asked** on a repository with no clone: the tree and its files, a commit's diff, Compare's diff, a pull request's *Files changed*. ADR-0042's rule stands: only a read knobas never made waits, and it waits without blocking the page or a key. The pointer resting on the link or the row starts it early; with a clone, the clone answers the tree, the files and the diffs.
- **Each branch's standing**: Branches' *Against main*, *Pull request* and actions, and the branch the header's *New pull request* proposes, are each branch's whole view on the forge (its compare against the default branch), 1 to 27 s apiece while a sync loads the Gitea on the WSL box. They arrive in place, in room held for them; the mirror holds no compare.
- **The rest of the latest commits**: the mirror holds the default branch's tip, not the source's list of the latest twenty or its count, so Commits draws the tip at once and the other nineteen join after it when the forge answers.
- **A run's Log, Tests and Artifacts** read TeamCity, which was stopped during this pass; with nothing held they said so at once.

**Reversing it** is `mirror_listing_kind` answering `None` in the Gitea adapter (the page waits for the source's branch list again), `keepFile` keeping nothing in `TfRepoFiles`, `readLocal` returning where the source's top answered first, `FIRST_SEARCH_AFTER_MS` 0 in `TfRepo`, and `installHoverReads` not called in `App.svelte`.

**Not measured live yet** (the WSL box went back to Björn): the Branches row height, the follow-ups at 4x and with the forge slowed, `just open-budget --inner` against its new lines, and the live witness of the mirror's branch list, whose first run read through a store-less session and was corrected; listed in the PR.
