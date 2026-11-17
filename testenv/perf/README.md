# The frame probes

How smooth the app is, measured instead of felt, from the two smoothness reports of PR #2479 (2026-11-11): Windows in
WebView2 with the main thread slowed to stand in for a slower laptop, and the Mac in WKWebView at 120 Hz. Rulings in
`docs/decisions/2026-11-11-smooth-on-slow-laptops.md`.

## The speed targets (Björn, 2026-11-14)

Three throttles, one weight each. On the saved scale database in headless Chrome, with the page's main thread slowed:

| CPU | A page usable after its press | A key's rows drawn | Scrolling |
|---|---|---|---|
| 1x | 100 ms | 100 ms | the display's rate |
| 4x | 200 ms | 200 ms | 55 frames a second, no task over 60 ms |
| 6x, a slow work laptop | 300 ms, drawn | 200 ms | no visible stall: 55 frames a second, no task over 60 ms |

`just open-budget --real`, `just keys-budget --real` and `just frame-budget` run all three by default and hold them
(`budget-open.json`, `budget-keys.json`, `budget.json`); a pass that changes what the app draws runs them. At 6x an open is
held by when it is drawn (`drawnMs`), not only when its first useful frame starts, since a long task after the rows
holds the frame that shows them. The inner pages (`--inner`) run at 1x, 4x and 6x too, each throttle at items of its own
(`inner-items.json`; the 6x set since the inner-6x pass, 2026-11-15); the extremes (`--extremes`) stay at 1x and 4x,
their items living on the live products. Windows runs
the same pages in about twice the Mac's time (the Windows pass, 2026-11-14), so a line met with little room on the Mac
is a line missed on a slow Windows laptop; `docs/decisions/2026-11-14-what-a-page-costs-to-build.md` has the numbers.

### At every window size (the sizes pass, 2026-11-15)

The targets hold at every size a person runs the app at, full screen and at high DPI as much as at the 1440 x 868 the
probes always drew. Every probe that starts its own browser takes `--size WxH@scale`: the page's CSS pixels and the
display's scale (`cdp.mjs`), 1440x868@1 by default. A scale of 1.25 or 1.5 is Windows at 125 % or 150 %, a scale of 2 a
4K screen at 200 % or a Mac's Retina panel, so `1920x1080@2` is a maximised window on a 3840 x 2160 screen at 200 %.

| Budget | Sizes it runs | Throttles |
|---|---|---|
| `just frame-budget`, `just open-budget --real`, `just keys-budget --real` | 1440x868@1 | 1x, 4x, 6x |
| the same, by default after it | `1920x1080@2` (4K at 200 %, the most device pixels) and `2560x1440@1.25` (1440p at 125 %, the most CSS pixels at a fractional scale) | 4x, 6x |
| `just open-budget --real` and `just keys-budget --real`, a whole run, after those | `5120x1440@1` (a 32:9 screen at 100 %, Björn's display full screen): a repository's landing (`repo`) and the board's letter (`tickets-letter`), the pages that grow most with a wide and tall window (the ux14 pass, 2026-11-16) | 4x, 6x |

*Monitors*' scroll at 6x on `2560x1440@1.25`, the one red cell of the sizes pass, holds 60 since issue #2508
(`docs/decisions/2026-11-16-a-row-swap-paints-its-own-rows.md`): a row swap moved every row that stayed by a fraction of
a pixel at a fractional scale, and so painted the whole table again. A red cell now is new. When a list's scroll
misses, trace it with `disabled-by-default-devtools.timeline.invalidationTracking` and look at the frames that swap
rows apart from the rest: a swap that draws again more than its own rows (`PaintImage` on the main thread) is rows
that moved, and a frame with no swap that paints at all is something invalidated every frame
(`InvalidateDisplayItemClient` names it).

`--sizes "WxH@s ..."` names others and `--sizes none` runs the default size alone; the inner pages and the extremes run
at 1440x868@1 unless `--sizes` names sizes for them. The lines are the same at every size. `run.mjs`'s scroll records, a
scenario, what grows with the device pixels a window paints: each presented frame's main-thread paint (`Paint`,
`PrePaint`, `Layerize`), raster, compositor thread, GPU main thread and display compositor (`pixels`, `frames.mjs`'s
`pixelWork`), and the page's composited layers and their area in device megapixels (`layers`, from the `LayerTree`
domain). A headless Chrome draws at 60 Hz whatever the size; what a 240 Hz display asks of a frame (4.2 ms) only the
person's own window can show (`docs/decisions/2026-11-15-every-window-size.md`, *Owed on Windows*).

## `just frame-budget`

Builds the app as the probes measure it, serves it on loopback, drives it in a headless Chrome at 1x, 4x and 6x, and
fails when something is over its line in `budget.json` or a box that scrolls has no opaque background:

```sh
just frame-budget                         # the fixture at scale (?fake-ipc&fake-scale), the targets' lists: runs anywhere
just frame-budget --real 5411             # the windowless backend on a saved database (below)
just frame-budget --only monitors,code    # the scenarios whose name holds one of the words
```

It prints the table it checked and leaves the run, the scrollers and `budget.md` in the folder it names. The lines are
what the branch measured on the Mac (M2 Pro) with margin; a slower machine reads higher, so read a failure against the
line's measure, then measure the same commit before the change to see what the change cost.

**The build** is `KNOBAS_PERF=1 testenv/ui-checks/build-app.sh`: a development build, so `?fake-ipc` is there, with
Svelte compiled and its runtime resolved as the release's (`vite.config.ts`). Svelte's development mode records a stack
trace on every state write, which made the measured app's filter keys several times dearer than the shipped one's.

**A sidebar's drag** (`run.mjs --kinds drag`, the `drag-` cases; the sidebar-resize pass, 2026-11-16,
`docs/decisions/2026-11-16-every-sidebar-drags.md`): the edge of each busy page's sidebar (`TfSideEdge`) held with real
mouse events, a move every 4.2 ms as a 239 Hz pointer sends them, out 220 px and back twice in 2.4 s, then let go. It
reports the drag's frame rate, its worst frame gap, whether the drag tripped the ghost (`side-pace.ts`: a page that
cannot lay itself out at a frame's pace) and the rate from the trip on. The real lines hold no trip at 1x and 55 fps
with no gap over 60 ms at 4x and 6x. The moves are sent without waiting on each answer, since the protocol answers a
move once the page has handled it. `drag-fixture-compare` and `drag-fixture-compare-files` run on the fixture only
(`--app '...?fake-ipc#/inbox' --only drag-fixture`): the saved database's repositories hold one branch. `--attach` with
`--size` sets the attached page's size, so the cases run as well in a headless Chrome on another machine reached over
an SSH tunnel.

## The pieces

| File | Does |
|---|---|
| `run.mjs` | the scenarios: `scroll` (a script-driven scroll at 3,000 px/s for 4 s, under a trace), `wheel` (a wheel scroll the browser synthesizes), `type` (a filter typed a key every 120 ms) and `nav` (each page switch, first visit and return), at each `--throttle` |
| `probe.js` | the page half: every frame, the time from its callback to the end of its rendering (`frame work`), script by kind, long animation frames, event timings, IPC |
| `frames.mjs` | the trace readers: what reached the display (the compositor's frame reporters), how many elements each style pass computed, the main thread's time by kind, its longest tasks |
| `budget.mjs`, `budget.json` | the lines, per corpus (`fake`, `real`), and the check |
| `long.mjs` | the long-run probe (the monitors-builds pass, 2026-11-12): Monitors left open while every source's run ends every `--every` seconds, then kept hidden behind the Inbox; main-thread time a minute, the longest task, a small read's wait, DOM, listener and heap growth, and a hover's cost at 1x and 4x, against `budget.json`'s `long` lines. `--attach` joins a running app (knobas's own WebView2), whose own syncs end its runs: an emit from the page (`plugin:event|emit`) froze the app's window for good |
| `scrollers.mjs` | every box that scrolls on the app's pages, light and dark, with its own background and the colour under it; `--strict` fails on a transparent one |
| `invalidations.mjs` | why a scroll restyles what it does, from Chromium's own invalidation records: for a rule that restyles far more than the rows that changed |
| `profile.mjs` | a CPU profile of one moment (`--setup`, then `--act`), by function, for a slow key or page switch the frame probe names only by its task |
| `moment.mjs` | the same moment under a Chrome trace (the inner-6x pass, 2026-11-15): each long task after `--act` split into script, style, layout and paint, and each style or layout pass in it with its elements or objects and, where script forced it, the frame that did. A profile's "(program)" is this |
| `serve.mjs` | serves a built app on 127.0.0.1 where there is no Python (Windows) |
| `cdp.mjs` | the DevTools-protocol client: `launch()` a headless Chrome of its own, or `attach(port)` to one listening |
| `frame-budget.sh` | what `just frame-budget` runs |
| `open.mjs`, `open-probe.js` | how long a press takes to show a page's content, first visit to after a sync (below) |
| `budget-open.json`, `open-budget.sh` | the opens' lines and what `just open-budget` runs (`--inner`, `--forge`: below) |
| `inner-items.json`, `slow-forge.mjs` | the inner pages' items per throttle, and the proxy that slows the forge (below) |
| `extremes.mjs`, `extremes-items.json` | the extremes an `--extremes` run seeds, syncs, presses and removes (*The extremes*, below) |
| `big-folders.mjs`, `folders.mjs` | the big folders an `--inner` run makes, presses and removes, and one folder's press broken down (*Big folders*, below) |
| `syncing.mjs`, `lane-probe.mjs` | one source kept syncing for a run's length, and the felt waits a sync can hold timed at the backend per adapter (*With a sync running*, below) |
| `keys.mjs`, `budget-keys.json`, `keys-budget.sh` | how long a key that keeps most of a page takes to draw the rows it asks for, the lines, and what `just keys-budget` runs (below) |
| `soak.mjs`, `soak.sh`, `soak-repo.sh` | a repository's Code page browsed for a long while, then every other page, with what grows counted; what `just soak` runs and the repository it browses (*The soak*, below) |
| `git-starts.mjs`, `git-shim/git`, `git-starts.sh` | how many `git` processes a press at a cloned repository starts, the counting `git` the backend runs under, and what `just git-starts` runs (*Starts of git*, below) |
| `folder-render.mjs` | folder renderers' pages and presses (ADR-0044): a repository's landing with its folders' glyphs, a folder's render opened from its row and on return with its drawing kept, *Run again*, Settings › Renderers and a folder renderer's page, over the built app under `?fake-ipc&fake-folder-render`, five times a case at each throttle, against the page and press lines (`--machine pc` for the PC's whole lines) |
| `zoom.mjs` | a zoom step (⌘= with the window's viewport and device scale set as the webview's zoom sets them) on Tickets, Code, Builds and Docs at each throttle and level: from the key to the end of its first frame, that frame's work and the longest animation frame; and a scroll at the scale Chrome was started at, with the rows that moved. A device-scale override does not move Chrome's layout grid, so a level's scroll is measured with Chrome started at the product (`--size 2327x1309@1.375 --levels 1`); `docs/decisions/2026-11-16-zoom-is-the-window-s.md` |
| `runs-read.mjs` | *Mark as read* on a failed run's row and back, and the rail's *Mark all as read* (drawn, then said by its toast after the backend's write) and its *Undo*, as real presses at each throttle against the windowless backend (`docs/decisions/2026-11-17-mark-builds-as-read.md`); the marks are put back as found |
| `wk/` | WKWebView, which has no DevTools protocol: below |
| `win/` | the guard and display helpers for a run on a person's Windows PC |

## What the numbers mean

- **rAF fps, dropped**: the page's own `requestAnimationFrame` rate, counted against the display's interval (60 Hz in
  headless Chrome, 240 on Björn's PC).
- **frame work**: from a frame's callback to the end of its rendering on the main thread. Under 4x it is four times the
  script and style a frame does.
- **elements a style pass**: how many elements one style recalculation computed, from the trace. A row entering a
  list should restyle that row: 566 on Monitors at the branch's head, 3,543 before.
- **longest task**: the main thread's longest task in the window, ms.
- **moved fps** (wheel): the pictures a second in which the list was somewhere new, from the compositor's frame
  reporters, and which thread scrolled. Headless Chrome on the Mac scrolls every list on the compositor, since macOS
  keeps no LCD text; Windows at 100% or 125% scale scrolls a transparent box on the main thread. So the compositor's
  side needs Windows (below), and the main thread's side shows anywhere.
- **keys to paint**: from each synthetic input event to the frame that draws it.
- **switch ms**: a page switch's first frame or its longest frame after, whichever is longer.
- **throttle Nx**: `Emulation.setCPUThrottlingRate`, the page's main thread only; not the compositor, raster or GPU.
- **size WxH@s** (`--size`): the page's CSS pixels and device scale; a probe's results name it (`size`).
- **a frame's paint, raster, compositor, gpu, viz** (scroll, `pixels`): those threads' task time over the scroll
  divided by the frames presented, ms. Headless Chrome on the Mac rasters on the GPU process, so raster's cost shows
  under `gpu` more than under `raster`.
- **layers, Mpx** (scroll, `layers`): composited layers that draw, and their bounds' area in device megapixels; a
  scroller's contents layer is its whole height, of which the compositor rasters only the tiles near the view.

## Opening a page: `just open-budget`

How long a press takes to show a page's content, from the instant-open pass of PR #2479 (2026-11-12, rulings in
`docs/decisions/2026-11-12-opening-a-page-draws-what-the-mirror-holds.md`). `open.mjs` presses each place's nav
entry or row with real input over the DevTools protocol, at each throttle, in four rounds:

| Round | What it is |
|---|---|
| `first` | the first press after the app started and was up 6 s (`--first-after`) |
| `return` | left for one other page, pressed again |
| `late` | left for every other section first, so the kept pages have turned over |
| `synced` | left, every source ends a run that changed something (`?fake-hooks`'s `__fake.emit`), pressed again |

For each it records, in the page (`open-probe.js`): the first frame after the press, the first frame in which the
place's own rows are drawn and nothing in the shown page waits (no skeleton, nothing `aria-busy`), the page's DOM
quiet 700 ms after that, the rows that moved or went after that first useful frame, every IPC call between the press
and settled with when it started and ended, and the long tasks. `--check budget-open.json --lines real` fails a place
whose useful time or settling is over its line, or whose rows moved.

```sh
cp -cR <saved home> "$TMPDIR/home"            # as *On a real database* above, the secrets moved out
HOME="$TMPDIR/home" cargo run --release -p knobas-app --features test-util --example headless_ipc -- \
  --data-dir "$TMPDIR/home/Library/Application Support/dev.knobas.desktop" --port 5411 --timings "$TMPDIR/ipc.jsonl"
just open-budget --real 5411
curl -X POST http://127.0.0.1:5411/quit
```

`--timings` makes the windowless backend append one line a command (its name, its arguments, when it arrived and
left, the backend's time on it, the answer's bytes), which splits an open into what the backend spent and what the
page did. The places are the saved scale database's (`SCLENG`, a TeamCity pipeline); there is no fixture run. Two
things differ from the app: a headless Chrome sends at most six requests to the backend at once, where Tauri's IPC
has no such limit, and the backend is the example's mock runtime, whose answers are the app's.

## The inner pages: `just open-budget --inner`

A repository's landing and tabs, its files, a commit, Compare, *New pull request*, a pull request's tabs and a run's
tabs, from the inner-pages pass of PR #2479 (2026-11-12, `docs/decisions/2026-11-12-the-inner-pages-draw-what-knobas-holds.md`).
These read past the mirror, so they are measured against the **live** forge: the backend syncs and reads the live Gitea
on the WSL box (`testenv/README.md`, *Live products on a remote machine*), over the tunnel, through `slow-forge.mjs`.

```sh
cp -cR <saved home> "$TMPDIR/home"            # the sources as saved, with their credentials (not moved out)
# once: the Gitea source behind the proxy, so its address, and its held reads, are the same live or slowed
curl -X POST http://127.0.0.1:5411/invoke -d '{"cmd":"update_source","args":{"id":"live-gitea","patch":{"base_url":"https://127.0.0.1:3900"}}}'
just open-budget --real 5411 --inner                # the live forge: `inner` lines
just open-budget --real 5411 --inner --forge 1000   # every request held 1 s: `forge` lines
```

- **A backend that reads the forge** needs the source's credential. A release build reads it from the keychain only,
  and a scratch `HOME` has none, so build the example with debug assertions on, which keeps the dev profile's
  `dev-secrets.json` (`CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=true cargo build --release ... --target-dir <scratch>`,
  a target of its own so the shared one keeps its cache).
- **`slow-forge.mjs`** forwards bytes between knobas and the https front (`127.0.0.1:3443`) and holds the first chunk
  of each request `--delay` ms from when it came, as `crates/knobas-app/tests/slow_proxy` does for the Rust rigs; holds
  overlap, so requests sent together on knobas's one HTTP/2 connection each wait the delay once (until the WSL block of
  2026-11-14 they queued, each behind every hold before it); `--control <port>`
  takes `POST /delay?ms=` and answers `GET /count`. Over TLS a new connection's handshake is held too, as a far
  network would.
- **The items** are `inner-items.json`: one repository, files, a commit, a branch and pull requests per throttle, so
  each throttle's first open is a read knobas never made. A place's `first` round is the first open in that backend's
  life; to measure an open of a page read in an earlier session, start the backend again and run `--rounds first`.
  `firstUsefulMs` in a line bounds the first round alone, since only the forge answers a read never made.
- **Six requests at once.** Headless Chrome sends at most six requests to the backend at once, so with the forge
  slowed a page's local reads queue behind the page before's forge reads, which Tauri's IPC never does: a commit's page
  measured 14.2 s with a clone, its diff answered by the backend in 80 ms. Read a slow cell against the backend's own
  `--timings` before blaming the page.
- **What waits on the forge is named per call**: each IPC call in `open.json` carries its arguments (`what`), and
  `--timings` on the backend each command's time, so a slow open says which section of which entity it waited for.

### The inner pages on the Mac: the stand-in forge

From the inner-6x pass (2026-11-15, `docs/decisions/2026-11-15-inner-pages-at-6x.md`), with the live products someone
else's. The scale estate's Gitea repositories are built from a seed, so they can be built again locally with the same
commits: `testenv/scale/gitea.py`'s `History` for `scale-commerce/cart-service`, `payments-gateway` and `mobile-android`
(the three item sets), at the seed's count of 10,000 and each person's address `<first name>@tidewater.test`, gives the
very shas the saved database mirrors. `standin-forge.mjs --repos <dir>` then answers each repository from
`<dir>/<owner>-<name>.git`: its record, branches and tips, trees, files, a commit and its diff, a path's commits, a
commit's statuses, and with `--pulls <file.json>` (each pull request's record by `owner/name#n`, the mirror's payload as
`get_entity` gives it) a pull request's record, commits and `.diff`. Reviews and comments answer empty, the account is
`knobas`, an organisation's labels answer empty, and every other route 404 (`--log` prints every request with its status and time, so a slow page can be told
from a slow stand-in: its git runs off the event loop, and a branch's commit records are read in one `git show`).

```sh
python3 -I testenv/perf/standin-repos.py --out <dir> scale-commerce/cart-service scale-commerce/payments-gateway scale-commerce/mobile-android
cp -cR <saved home> "$TMPDIR/inner"           # keep only `source:live-gitea` in its dev-secrets.json
node testenv/perf/standin-forge.mjs --listen 3961 --delay 30 --repos <dir> --pulls pulls.json --real-compare --log &
HOME="$TMPDIR/inner" <headless_ipc built with debug assertions> --data-dir "$TMPDIR/inner/Library/Application Support/dev.knobas.desktop" --port 5472 &
curl -X POST http://127.0.0.1:5472/invoke -d '{"cmd":"update_source","args":{"id":"live-gitea","patch":{"base_url":"http://127.0.0.1:3961"}}}'
just open-budget --real 5472 --inner --no-big --throttle 6 --only repo-,files-,file-small,file-large,file-readme,new-pull-request,pr-
```

- **The pull requests' records** come from a backend on the saved database: `standin-repos.py --port <its port> --pulls
  pulls.json --pr scale-commerce/cart-service#124 --pr …` (the items' `pr` and `prLarge`).
- **One throttle a fresh copy and backend**, so each first open is a read the backend never made. 30 ms a request is
  what the live Gitea took for a file it had read (the Windows pass); `--delay` sets it.
- **The saved home holds writes queued on the live products**: a backend whose Gitea source answers sends them (the
  stand-in refuses them, `POST …/branches`). Never point such a copy at a live forge you mean to keep as it is.
- **The saved home stores no tree**: its sync never ran the stored-trees pass, so its landings are the first fill's,
  read at the mirror's tip (the forge-rounds pass, 2026-11-15). For the landing as an app that has synced draws it,
  make a base once: a backend on a copy (so its migrations run), then `stored_trees_fill --db <its url> --source
  live-gitea --base-url http://127.0.0.1:3961 --passes 2` (each pass stores the stand-in repositories' default branches
  and their READMEs first; `--until-done` never ends here, as the estate's other repositories answer 404), quit the
  backend, and take copies of that copy. A backend older than the base's newest migration refuses it.
- **Left out:** the big folders and compares (`--no-big`: they are made on the live Gitea for the run), the PDF
  (`tidewater/milestone-review` is seeded by `seed-gitea.sh`, not built here) and a run's tabs (TeamCity).

### A person's read beside a running sync, and a page left open

From the ux13 forge-load pass (2026-11-15, `docs/decisions/2026-11-15-a-persons-read-beside-a-running-sync.md`). The
stand-in can model a forge that answers in turn: `--workers 1` answers one request at a time, the rest waiting in the
order they came (`--log` prints each one's wait and how many were ahead of it), and `--heavy <ms>` holds it longer for
the listings a run walks than `--delay` for a page's routes. `--sync` answers `/repos/search` with every repository of
`--repos`, so a scale profile's Gitea source runs whole against it.

```sh
node testenv/perf/standin-forge.mjs --listen 3987 --delay 40 --heavy 100 --workers 1 --repos <dir> --pulls pulls.json \
  --real-compare --sync --control 3988 --log &
node testenv/perf/lane-probe.mjs --port <backend port> --probes probes.json --source live-gitea --mode backfill
node testenv/perf/page-requests.mjs --app "http://127.0.0.1:5980/?fake-ipc&fake-hooks&fake-real=<port>#/repo/<id>" \
  --control 3988 --seconds 120 --every 40 --news alternate [--hide 10]
```

`landing-requests.mjs` prints what a repository's landing asks the stand-in on its first open, in order (`--log`'s
lines from `--log-file`): the landing-rounds pass (2026-11-16) found the default branch's tip read three times and the
early compare in the landing's own round with it. `page-requests.mjs` opens a page, lets it settle, then counts what the stand-in answers while the page is told a run of
its source ended every `--every` seconds (`--news` says which ended with something), hidden from `--hide` on. Build the
before and after backends into **separate target directories**: cargo's dep-info names sources relative to the
workspace, so a second checkout built into the first's target directory takes the first's crates as fresh.

### Big folders

Part of every `--inner` run (the big-folder pass, 2026-11-13, `docs/decisions/2026-11-13-a-big-folder-opens-in-a-frame.md`).
`big-folders.mjs` makes two scratch repositories on the live Gitea for the run, `knobas-perf-big-folders/cloned` and
`/forge`, each with folders of 300, 3,000 and 12,000 entries and one of 800 ten directories down (made with `git
fast-import` at a fixed date, so every run pushes the same commit). It adds the org to the profile's `live-gitea`
source (`KNOBAS_PERF_GITEA_SOURCE` names another), syncs, and clones `cloned` into a clones root of the run's own. It
pushes with this worktree's seed token (`testenv/seed-state.json`, or `KNOBAS_GITEA_TOKEN`). When the run ends, or is
stopped, the org and the repositories are deleted on Gitea, and the source's owners and the profile's clones root are put
back. `--no-big-folders` on `open.mjs` leaves them out.

| Place | Press | Line |
|---|---|---|
| `folder-300`, `folder-3000`, `folder-12000`, `folder-deep` | the folder's row in the cloned repository (`deep` opens at a file beside it) | the aim on every round, the first too: no IPC, the rows in the next frame or two |
| `forge-folder-300`, `forge-folder-3000`, `forge-folder-12000` | the same in the repository with no clone; the 4x run presses twins of its own (`hundreds-2`), so its first press is a read knobas never made | the first press reads the folder's own level of Gitea's tree (about 0.1, 0.2 and 0.6 s on the WSL box; `contents` took 0.7 s, 9 s and never), within 2, 3 and 5 s; the rounds after it are held to the aim |

A place's line holds `drawnMs` too: the probe's frame after the one that found the rows, by which time that frame has
drawn. `usefulMs` alone is taken at the start of the frame the rows are in, so it read 12 ms for a folder whose 12,006
rows then took 2.7 s to lay out and paint at 4x. A return folds the folder and presses it again; a round that comes
back to the tab with it still open folds it first (`undo`). The repositories hold about 31,400 entries, so the
clone's folders past the 20,000th are pressed too (round two of the pass).

`folders.mjs` is the same press broken down, for a pass on one repository: the main thread's script, style, layout and
paint from a trace, the IPC calls, and how many rows are in the DOM, unfolded, folded and unfolded again.
`--fetched <ms>` tells the page the app's `view:fresh` and `checkout:fetched` that long after it drew, for a clone
behind the mirror's commit, since the windowless backend tells the page no event. `--by-key` presses by `Enter` with the pointer off the
tree, so no read is asked early, as resting the pointer on a folder asks it; `--open` first times the tab's own open
from Code to its tree drawn, with its IPC calls.

```sh
node testenv/perf/folders.mjs --app "http://127.0.0.1:5362/?fake-ipc&fake-hooks&fake-real=5411#/time" \
  --repo live-gitea:knobas-perf-big-folders/cloned --folders hundreds,thousands,huge --throttle 1,4
```

### A file after a sync, and Compare with no clone: `synced-file.mjs`, `compare-open.mjs`

From the forge-waits pass (2026-11-13, `docs/decisions/2026-11-13-a-file-after-a-sync-and-a-first-compare-wait-on-nothing.md`),
measured on the Mac while the live products were in use. A scratch copy of a saved home has every source turned off
but Gitea, pointed at `standin-forge.mjs` (`base_url` `http://127.0.0.1:3911`): Gitea's compare for any two refs after
`--delay`, and with `--git <repo>` the routes *Files* reads, served from that local repository. Turn the sources off in
the copy's database before the backend first starts: a saved home's sources point at the WSL box's tunnels. The
scheduler starts no run on the saved home (*On a real database*), but a page's reads behind still reach the products.

```sh
node testenv/perf/standin-forge.mjs --listen 3911 --delay 1000 --git <repo> --control 3912 &
node testenv/perf/synced-file.mjs --app "http://127.0.0.1:5373/?fake-ipc&fake-hooks&fake-real=<port>#/time" \
  --db <the copy's database url> --git <repo> --clone <its clone, or ''> --repo <entity id> --path <file>
node testenv/perf/compare-open.mjs --app "http://127.0.0.1:5373/?fake-ipc&fake-hooks&fake-real=<port>#/time" \
  --items <items.json> --rounds press,address --dwell 3000
```

- **`synced-file.mjs`** draws a file, moves its branch as a sync does (a commit on the stand-in's repository, fetched
  into the clone, the mirror's tip and `synced_at` moved), and times the return to the kept page and a page loaded
  afresh to the frame showing the new commit's text. The windowless backend emits no events, so the probe raises
  `view:fresh` once the stand-in has answered the new commit's tree (`--control`'s `/tree`); asking the backend
  instead would start reads behind of its own. Leave seconds between syncs: a read behind still out across a sync
  is kept as read after it.
- **`compare-open.mjs`** presses *New pull request* after looking at the landing for `--dwell` ms, and loads a
  branch's compare by address; run it again on a restarted backend for a second session. The address rows include
  the app's own start.

### Every branch's tree: `stored-trees.mjs`, `stored_trees_fill`

From the stored-trees pass (2026-11-14, `docs/decisions/2026-11-14-every-branchs-tree-is-stored.md`), on the Mac with
the stand-in. A scratch copy of a saved home with every source off, Gitea at the stand-in, and each Gitea branch of the
mirror at a commit of a local repository named by its per-repository alias (`standin-forge.mjs --distinct`: two
repositories share no object, as two real ones do not; `--tips` names each branch's tip for `branches/{name}`).

```sh
node testenv/perf/standin-forge.mjs --listen 3911 --delay 50 --git <local repo> --distinct --tips tips.json --control 3912 &
cargo run --release -p knobas-app --example stored_trees_fill -- --db <the copy's database url> --source live-gitea --until-done
node testenv/perf/stored-trees.mjs --app "http://127.0.0.1:5373/?fake-ipc&fake-hooks&fake-real=<port>#/time" \
  --repo live-gitea:<owner>/<name> --deep <a/b/c> --find <file name> --throttle 1
```

- **`stored_trees_fill`** runs the pass alone, on the background lane and at the adapter's quota, against the copy's
  database: one JSON line a pass (requests, time, branches and levels stored, branches left, the store's bytes).
  Against a live forge it needs the profile's credential: `--secrets <profile>/dev-secrets.json` (the live pass,
  2026-11-14). `--check live-gitea:<owner>/<name> [--check-branches 3]` fills nothing: it draws each stored branch's
  top, every folder and *Go to file* from the store and compares each with the forge's own read of the same section,
  one JSON line a branch with the sections that differ. Read both through the address the mirror was synced at: the
  folders' links carry it.
- **`stored-trees.mjs`** times a repository's Files tab with no clone: its first open from Code to the tree's rows, a
  deep folder's first press (the folders above it opened first), and *Go to file*'s first use to the frame listing the
  file with nothing left reading. Each is a read the backend has not made, so start a fresh backend (and a fresh copy)
  for each throttle.

### Each open pull request's *Files*: `held_files_fill`

From the held-files pass (2026-11-15, `docs/decisions/2026-11-15-the-files-a-pull-request-opens-are-held.md`): the
sync's pass that holds each open pull request's *Files*, run alone against a copy's database, one JSON line a pass
(requests, time, sections held, too large, refused, left, and the held store's size). Point the copy's Gitea row at
the forge first (the held rows are keyed by the row's address), and give the stand-in every open pull request's record
(`--pulls`, the mirror's payloads by `owner/name#n`; build their repositories with `standin-repos.py`). For the inner
pages as an app that has run a sync draws them, fill a base this way and take copies of it.

```sh
cargo run --release -p knobas-app --example held_files_fill -- --db <the copy's database url> --source live-gitea --until-done
```

### Big compares on the Mac: `compare.mjs` against the stand-in

From the stored-compare pass (2026-11-14, `docs/decisions/2026-11-14-a-compare-lists-its-names-first.md`), with the live
products out of reach: `big-compare.mjs`'s repository made locally (its `stream()` through `git fast-import`, plus a
`trunk` two commits past `main` and `topic-3000` forked from the first of them), put into a scratch copy's mirror
under `live-gitea` as `knobas-perf-big-compare/cloned` and `/forge` (a repository row and a branch row per ref, each at
the local tip), its trees stored with `stored_trees_fill`, and `cloned` pinned to a local clone with
`set_checkout_override`. The Gitea source points at the stand-in and **stays on**: the mirror's rows of a source turned
off are left out of `sync.live_item`, and Compare then predicts nothing at the clone (the first try of this rig measured
the forge at 0 ms as "with a clone").

```sh
node testenv/perf/standin-forge.mjs --listen 3931 --delay 0 --git <local repo> --real-compare --refuse-rest --control 3932 &
node testenv/perf/compare.mjs --app "http://127.0.0.1:<app>/?fake-ipc&fake-hooks&fake-real=<port>#/time" \
  --repo live-gitea:knobas-perf-big-compare/cloned --throttle 1 --twins 0 --moved
```

- `--real-compare` answers the compare as Gitea does from the local repository (the three-dot diff, the commits with
  their parents); `--refuse-rest` answers every other route 503, so the source's scheduled run fails as unreachable and
  sweeps nothing.
- `--moved` adds two steps: `files-10000` into `trunk`, and `topic-3000` from `trunk`, whose merge base is no branch's
  head.
- One throttle a fresh copy and backend (`--twins 0`): each step's first pick is then a read the backend never made.
  Run the cells with a clone at a forge delay of 0: with the forge slowed, headless Chrome's six connections are held by
  the page's forge reads and the clone's answer queues behind them, which Tauri's IPC does not do. The steps' times move
  by about 30 ms run to run on a loaded Mac: take the median of three.

### Moving through a big compare: `just open-budget --real <port> --moves`

From the big-compare pass (2026-11-16, `docs/decisions/2026-11-16-a-big-compare-moves-at-once.md`): `big-compare.mjs`'s
long-lived branch, `many-commits` (300 commits over 4,200 files of `main`, about 67,000 changed lines, titles of every
length) and its 4x twin, with a pull request open from each in the cloned repository. `compare-moves.mjs` presses, at
each throttle and size, from the press to the frame that shows the answer:

- **jump-mid**, **jump-last**: a file half way down the file list and the last, picked in the list after the page and
  the list were scrolled to it; `lines` is the frame its lines are drawn with its head at the view's top (in view, for
  the last), `drift` how far it moved after.
- **top**: *Back to top* from the last file, until *From* is on screen.
- **swap-from**, **swap-into**, **swap-from-2**: three pairs never drawn (the page reads the next picks' names ahead
  while idle); **swap-back**: *Into* back to the pair the moves started on.
- **open-diff**: *Open as diff*, until the diff page lists the branch's file and draws a card's lines; `reads` counts
  its range reads.
- **pr-jump**: in the pull request's *Files changed*, the middle file (the last is past the whole diff's 50,000-line
  ceiling).

```sh
just open-budget --real <port> --moves                     # make, press at three sizes, remove
node testenv/perf/compare-moves.mjs --app "<app>?fake-ipc&fake-hooks&fake-real=<port>#/time" \
  --repo live-gitea:knobas-perf-big-compare/cloned --pr 'live-gitea:knobas-perf-big-compare/cloned#1' \
  --pr-b 'live-gitea:knobas-perf-big-compare/cloned#2' --throttle 1,4,6 [--size 2560x1440@1.25] [--trace <dir>] [--profile]
```

- `--trace <dir>` saves a Chrome trace of each move; `--profile` prints its script time by function.
- **A Windows laptop's git**: put a `git` that sleeps 100 ms and then runs the real one first on the windowless
  backend's `PATH`. Every start of git then costs what one costs there, which is most of a per-file read's wait.

### The extremes: `just open-budget --real <port> --extremes`

The worst shapes a real team's data has (the extremes pass, 2026-11-13,
`docs/decisions/2026-11-13-pages-draw-the-worst-shapes-in-a-frame.md`; `testenv/README.md`, *The extremes*): a ticket
of 500 comments and 200 attachments, a pull request of 3,000 files, 500 commits and 300 comments, a repository of
5,000 branches and 2,000 open pull requests, a Confluence page of a 2,000-row table and 300 comments, a 5 MB page, a
space of 3,000 pages at one level, a run of 5,000 artifacts and 20,000 tests. `extremes.mjs` seeds what is missing on
the live products (`testenv/seed-extremes.sh`, each product under its live lock), adds the scopes to the profile's own
sources, syncs them, and `open.mjs --set extremes` presses each page's own link or tab, first and on return, against
`budget-open.json`'s `extremes` lines. When the run ends, or is stopped, the scopes are put back and the extremes
removed from the products (`--keep-extremes` leaves them, for the next run).

```sh
just open-budget --real 5411 --extremes                    # seed, sync, press, remove
node testenv/perf/open.mjs --app "<app>?fake-ipc&fake-hooks&fake-real=5411#/time" --set extremes --no-seed \
  --throttle 1,4 --rounds first,return --out x.json        # on a profile synced with them already
```

- The pages whose ids the products give (the Confluence pages, the TeamCity run) are read from the products; with a
  product down, `KNOBAS_EXTREMES_IDS` (JSON: `page`, `hugePage`, `run`) names them, and a place without its item is
  left out.
- **Scrolling them:** `run.mjs`'s `x-` scenarios (`--only x-`) scroll the same pages under a trace, with
  `KNOBAS_EXTREMES_IDS` for the ids the products give.
- A place's useful selector waits for its first rows on screen; a page that draws in pieces (the 5 MB page) is useful
  at its first section, and `settled` says when its pieces near the view are drawn.

## Nothing moves: `just moves`

The movement gate (the nothing-moves pass, 2026-11-17, `docs/decisions/2026-11-17-nothing-moves-once-drawn.md`; Björn:
"Check if any other page moves after it's drawn. That's not good behavior."). `moves.mjs` opens every place a person
can open by the press a person makes: every nav page and its tabs, a repository and each of its tabs, a file, a commit,
*Open in…* and *Add an application*, a pull request and its tabs, a pipeline, a run and its tabs, the Docs home, a space
and a page, Monitors, a monitor, Sets, Time and its tabs, the Assets tree and an asset, every Settings section, a source's
page and an application's page, the launcher, a ticket card's right-click menu and the dialogs (*Create monitor…*,
*Link something to it*, *Add a source…*). Each on its first open after the app started and on its return.

`moves-probe.js`, in the page, tracks from the first frame the place is drawn (the DOM changed after the press, or after
the `hashchange` a link's address brings, a task after the address itself) the box of every row, heading, button, link,
field, tab, option and the focused element in the shown page, any dialog or menu, and the top and status bars, every frame
for 5 s past the first useful frame (nothing visible waiting). A box that moves or changes size by more than `--px` (2) is
a mover, with when, by how much, and what came in or grew above it in its column or beside it in its row that frame.
Layout-shift entries are kept beside it. Left out: what a running animation or transition moves, and for 150 ms after it
ends; anything after the person's own input; a box held unseen (`visibility: hidden`) in its room; a heading whose box only
narrows or widens with its words in place; and the press's own swap, a control the page left drawn while it read that takes
the new item's words in the frame the pressed item lands, keeping one edge, with nothing else moving.

```sh
just moves                                   # the fixture, every read late, 1x and 4x, 1440x868@1 and 2560x1440@1.25
just moves --real 5411                       # and the saved scale database (*On a real database*)
just moves --real 5411 --sync                # every source ends a run that changed something every 1.5 s while each records
just moves --only repo,pull-request --sizes 1440x868@1 --throttle 4
```

`--workers 3` (the default) runs three browsers, each a share of the places: a move is the order of frames, not their
time. `--trace <selector>` on `moves.mjs` prints what a box's children are each frame they change, which names a mover's
cause. The fixture's late reads (`fake-late`) are each under the 500 ms a header waits for its reads
(`tf/held-head.svelte.ts`), as the saved scale database's are at 4x; a read slower than that draws the header anyway, and
what it brings must keep its own room. With the Mac's lock taken, Chrome on the WSL box serves as well: `--attach <port>`
on a headless Chrome there, its debug port and the app's reached through `ssh -L` and `-R`.

## A broad key: `just keys-budget`

How long one key takes to show the rows it asks for, from the broad-filters pass of PR #2479 (2026-11-13, rulings in
`docs/decisions/2026-11-13-a-broad-key-answers-in-a-frame-or-two.md`). The keys are the dear ones, those that keep most
of a page: the first letter typed into Tickets' filter, a `-project:` and a `-priority:` that leave one value out, and a
single letter under Everything in the launcher, with the second, the third, the fourth, the fifth and the last
letter of a word after it. `keys.mjs`
presses each as real input over the DevTools protocol, each
press a key the session never asked before so the answer cache cannot answer it, and records in the page, from the
key's own event time:

| Measure | What it is |
|---|---|
| `rows` | the first frame after the reads the key sent at once (every read sent before the first answered) have answered and their rows are in the DOM: the answer, seen. A page's whole read once the keys pause (the list after *Show N more*, an expanded board: fifty a column on the key, the rest after `LIST_SETTLE_MS`, since the ux13-overs pass, 2026-11-15) is not waited for; it is in `reads` and `settled`, and `moved` fails a row on screen it moves |
| `answered` | when that read answered, so `rows - answered` is the page's own work and `answered` the wait for the backend |
| `moved` | the frames after `rows` in which a row drawn on screen in it moved or went; a row drawn later below them is none |
| `late` | what changed after `rows`, by element, for a `moved` to be read against |

```sh
just keys-budget --real 5411                  # the windowless backend on the saved scale database, as below
just keys-budget --real 5411 --only tickets   # the cases whose name holds a word
```

`--check budget-keys.json --lines real` fails a case whose median `rows` is over its line or whose rows moved. The
backend's own `--timings` splits `answered` into what it spent and what the IPC took. A release build of the backend is
what the app runs, and its number, not a debug build's, is the one the lines hold.

The 1x lines are Björn's 100 ms since the broad-keys pass of the same day (90 for `-priority:`), the 4x lines 200,
and since the render-cost pass (2026-11-14) the 6x lines 200 too, over the median of seven presses a case. They were measured at 71, 83, 64 and 91 ms with other agents' test runs and
browsers on the machine, and a press feels that: a letter's backend read was 54 ms asked alone and 65 to 75 during a
run, and one letter under *Everything*, which ranks its matches on four processes, slows most when something else has
the cores. Read a miss of a few ms against the machine's load and the backend's `--timings` before the code.

`everything-letter` has lines of its own since the one-letter pass (2026-11-15,
`docs/decisions/2026-11-15-one-letter-is-answered-by-names.md`): 70, 100 and 130 ms at 1x, 4x and 6x, where it measured
42, 63 and 94. One character is answered by names off an index, in about 10 ms of backend where the ranking took 57,
and the list draws an answer's first 24 rows in the frame it lands. Björn's key targets are 100, 100 and 200 ms on his
PC, whose read is about 1.5 times the Mac's, so the Mac's lines sit under his. With other agents' builds on the cores
it read 74 at 1x. `knobas-search`'s own tests hold the statement to the index, so a miss here is the page or the
machine: read the backend's `--timings` for `search`, which is about 10 ms.

`everything-two-letters` and `everything-three-letters` (the two-letters pass, 2026-11-16,
`docs/decisions/2026-11-16-two-letters-by-names-and-keys-wait-for-their-answer.md`) press the next letter of a word
into a box that holds the letters before, with their answer drawn and the keys long paused, so the key is asked at
once. They hold one letter's lines, 70, 100 and 130 ms, and measured 34, 61 and 73 for the second letter and 31, 59
and 73 for the third, where the ranking of words took 96, 126 and 145 and 89, 103 and 118. A press must change what
the list draws: `keys.mjs` reads an answer off the change it makes, so a key that asks for the rows already drawn
(`d` after `le`, which name the same rows) counts as one that drew nothing.

`everything-four-letters`, `everything-five-letters` and `everything-word` (the long-words pass, 2026-11-16,
`docs/decisions/2026-11-16-a-word-many-rows-hold.md`) press the fourth letter, the fifth and the last of words the
scale seed's bodies are built from, each in 13,000 to 22,000 of the 39,770 rows, into a box that holds the letters
before with their answer drawn. Ranked over every match they took 48 to 59 ms of backend. Counted first and answered
by their names and each kind's newest text they take 11 to 15, under one plan a connection. Their lines are the
letters', 70, 100 and 130 ms. A later letter often asks for the rows already drawn (`payo` and `payou` hold the same
rows, `invoic` is `invoice`'s own lexeme), so the fifth letters and the last ones are those that change the list.
The backend makes each statement's plan once on each of its five connections, 10 to 30 ms, and keeps it for the
app's life, so `keys.mjs` asks a letter and three words twelve times each before its first case: the lines hold the
keys after that.

## Starts of git: `just git-starts`

How many `git` processes a press at a cloned repository starts, from the git-starts pass of PR #2479 (2026-11-16,
`docs/decisions/2026-11-16-a-clone-is-read-in-process.md`, ADR-0043). A start is a few ms on a Mac and tens of ms on
Windows, so a press that starts git several times in a row misses its line there while every number here looks fine.
`git-starts.mjs` presses each action with real input, as `open.mjs` does, at a repository that is cloned, and reads
what a counting `git` on the backend's `PATH` wrote (`git-shim/git`: one line a start, an optional sleep, then the real
git):

| Action | What is pressed |
|---|---|
| `code`, `repo-landing`, `folder`, `file-small`, `file-large`, `file-deep`, `go-to-file` | Code, a repository, a folder of its tree, three files, *Go to file* |
| `branch-switch`, `files-refresh` | another branch picked on *Files*; every source's run ending while a file is open |
| `commits`, `commit` | the Commits tab, one commit's diff |
| `compare`, `new-pull-request` | a branch's compare by address, *New pull request* |
| `pr-files`, `pr-files-large` | two pull requests' *Files changed* |
| `local-changes`, `launcher` | the *Local changes* tab, the launcher with four letters typed |

Each is pressed twice (`first`: the first press in the backend's life; `again`: left and pressed again), one item set
a throttle (`inner-items.json`), and printed with its starts (the subcommand and when it began after the press), how
many were at once, and the press's useful, drawn and settled times. With `--check`, which `just git-starts` passes, a
press that started git at all fails, but for `fetch`, `clone` and `worktree`, which are writes behind an answer.
`local-changes` is the one action that may, on its first press: the working tree is git's to read (ADR-0043), so the
check passes its `diff`, `diff --cached` and `status` there, three at most and at once (told apart from in a row where
the shim sleeps 50 ms or more), and none on the press again, while nothing changed. `--count` only counts, for a
before.

```sh
python3 -I testenv/perf/standin-repos.py --out <dir> scale-commerce/cart-service scale-commerce/payments-gateway scale-commerce/mobile-android
for r in cart-service payments-gateway mobile-android; do git clone -q <dir>/scale-commerce-$r.git <clones>/$r; done
cp -cR <saved home> "$TMPDIR/starts"           # its dev-secrets.json moved out for the first start
# once, with no secret in reach: the Gitea source at the stand-in, the other sources off, each repository's clone named
HOME="$TMPDIR/starts" <headless_ipc> --data-dir "$TMPDIR/starts/Library/Application Support/dev.knobas.desktop" --port 5811 &
curl -X POST http://127.0.0.1:5811/invoke -d '{"cmd":"update_source","args":{"id":"live-gitea","patch":{"base_url":"http://127.0.0.1:3981"}}}'
curl -X POST http://127.0.0.1:5811/invoke -d '{"cmd":"set_checkout_override","args":{"entityId":"live-gitea:scale-commerce/cart-service","path":"<clones>/cart-service"}}'
curl -X POST http://127.0.0.1:5811/quit        # then only `source:live-gitea` back in dev-secrets.json
node testenv/perf/standin-forge.mjs --listen 3981 --delay 30 --repos <dir> --pulls pulls.json --real-compare &
log="$TMPDIR/git-starts.log"
HOME="$TMPDIR/starts" PATH="$PWD/testenv/perf/git-shim:$PATH" KNOBAS_GIT_SHIM_REAL="$(command -v git)" \
  KNOBAS_GIT_SHIM_LOG="$log" KNOBAS_GIT_SHIM_DELAY=0.1 \
  <headless_ipc built with debug assertions> --data-dir "$TMPDIR/starts/Library/Application Support/dev.knobas.desktop" --port 5811 &
just git-starts --real 5811 --shim-log "$log" --delay 100
```

- **`KNOBAS_GIT_SHIM_DELAY=0.1`** holds each start 100 ms, a slow Windows laptop's process start, so the Mac shows what
  the starts cost there; `--delay 100` tells the probe, which then says which starts were at once and which in a row.
  Leave both out to count alone.
- **The pull requests' records** (`pulls.json`) come from a backend on the saved database, as for the inner pages
  (`standin-repos.py --port <port> --pulls pulls.json --pr scale-commerce/cart-service#124 ...`).
- **The source is pointed at the stand-in before its credential is back**, so nothing the saved home has queued
  reaches the live Gitea.

Two more instruments sit with the reader itself (`crates/knobas-git/examples`), and run on any machine with a clone:

| Example | Does |
|---|---|
| `parity -- <repository> [--commits 2000] [--pairs 300]` | every read of the reader against the `git` on `PATH` over the repository's history: each commit's diff in every form, trees, files, and merge bases, logs and range diffs of commit pairs. Prints each difference; any is exit 1 |
| `timing -- <repository> <old> <new>` | what each read costs in-process and as a start of `git`, medians of nine: the number a Windows machine owes this pass |

### With a sync running: `just open-budget --inner --syncing <source>`

The case a person's read used to queue behind (the priority lane, 2026-11-13, `docs/decisions/2026-11-13-a-persons-read-goes-ahead-of-the-sync.md`):
a sync keeps the server's quota full, and the forge is loaded by it. `--syncing live-gitea` keeps a backfill of that
source running for the length of the run (`syncing.mjs` asks for another each time one ends) and presses the inner
pages once it is into its reads, checked against the `syncing` lines. A regression in the lane (a person's read
waiting behind the sync's requests again) shows as first opens of seconds where the lines allow one. The big folders
are left out: their set-up waits for a run of its own to mirror two new repositories. The windowless backend emits no
events, so a view it answered from what knobas held is never replaced in the page by its read behind (no
`view:fresh`): a place that waits for that read reads *never* here, where the app's window would draw it when the
read lands. `lane-probe.mjs` times those reads.

`lane-probe.mjs` times the same thing for every adapter at the backend, without a browser: a probes file names each
page's or press's live reads (`entity_view`, `reachable_transitions`, `test_source`, `refresh_entity`, and
`submit_write` with `"until": "sent"`, timed until the write leaves the queue), sent together as the page sends
them, while `--source` is kept syncing. A view answered from what knobas held is followed until its read behind lands
(its `held_at` moves past the send). Start each measured backend on a fresh clone, so every probe is a first open.

```sh
node testenv/perf/lane-probe.mjs --port 5411 --probes jira.json --source live-jira --out jira-after.json
```

## The soak: `just soak`

What a page costs after it has been used for a while, from the hover-lag pass of PR #2479 (2026-11-16,
`docs/decisions/2026-11-16-a-refresh-reads-what-can-have-changed.md`). Every other probe here measures a page fresh:
its first open, its return, a scroll. Björn's report was of a page that got slow with use: "Hovering over the filelist
on the left side is incredibly slow ... it just happens when you browse the code-page for a longer time." A hover at
rest never showed it, at any throttle, after any amount of browsing. What grew was the work each run's end and each
return to the page started: every folder opened so far read again and the tree drawn again once a folder.

`soak.mjs` browses a repository's Code page as a person does. A lap opens two or three folders and two files, scrolls,
folds a folder now and then, and every few laps looks at Commits and a commit, at *New pull request*, at a pull request
and its files, at another section, and comes back. After each lap, at rest on Files, it records:

| Measure | What it is |
|---|---|
| `hover` | a sweep over the tree's rows on screen at each throttle: `felt`, from the mouse move's own time to the end of the frame that draws it, and `cost`, the main thread's share (the move's handlers and that frame's rendering) |
| `afterSync` | the pointer kept moving over the tree, a row every 80 ms, through the end of a run of every source that changed something (`sync:state`, told through `?fake-hooks`; every `--sync-every` seconds), at the slowest throttle: each move's `felt`, the main thread's time in the two seconds after it (`mainMs`), and the IPC calls it asked (`calls`) |
| `afterReturn` | the same through a return from another section (every second lap) |
| `nodesKept`, `listenersKept`, `heapMB` | every fifth lap, after a collection: DOM nodes, DOM listeners and the JS heap the page holds |
| `tauriListeners` | Tauri event listeners registered and not dropped |
| `idleMsPerMin` | the main thread's time while nothing is pressed |
| `hoverStyle` | every `--trace-every` laps, the sweep under a trace: the elements each style pass computed (two to four: the row left and the row entered) |

`--check budget.json --lines real` holds the `soak` lines: what the page keeps late in the run against early in it
(the median of the last third of the collected laps less that of the first: one lap against another reads which page
keep-alive happened to hold), the hovers at rest, and over the run's second half the hover through a run's end and a
return, the calls a run's end asks and how much they grew. `--tour` asks the same of every other page: each lap visits
the sections, a pipeline, a run, a space, a doc, the repository's other tabs, a pull request, its files and its
conversation, presses what a page offers more of (a board column's *Show 50 more*, *Older runs*, a diff's *Expand
all*), and keeps the pointer moving over its rows through a run's end; the `soak-tour` lines hold the page whose run's
end grew most between its second and third visits and its last, and the slowest hover through one. The tour's nodes
and heap are printed and not held: what it presses is kept by design, a hundred more cards a lap on the board and a
diff's context, so they grow until there is no more to press (a pull request's page levelled at 31,000 nodes after
eight laps; Tickets, 1,200 more cards on, still asked 17 calls a run's end and drew a hover in 22 to 59 ms).

```sh
testenv/perf/soak-repo.sh <dir>               # the stand-in's cart-service with 9,700 files on main, and a clone
cp -cR <saved home> "$TMPDIR/soak"            # keep only `source:live-gitea` in its dev-secrets.json
node testenv/perf/standin-forge.mjs --listen 3973 --delay 30 --repos <dir>/repos --pulls pulls.json --real-compare &
HOME="$TMPDIR/soak" <headless_ipc built with debug assertions> --data-dir "$TMPDIR/soak/Library/Application Support/dev.knobas.desktop" --port 5733 &
curl -X POST http://127.0.0.1:5733/invoke -d '{"cmd":"update_source","args":{"id":"live-gitea","patch":{"base_url":"http://127.0.0.1:3973"}}}'
psql <the copy's database url> -c "<the update soak-repo.sh printed>"       # the mirror's main at the big commit
curl -X POST http://127.0.0.1:5733/invoke -d '{"cmd":"set_checkout_override","args":{"entityId":"live-gitea:scale-commerce/cart-service","path":"<dir>/clone"}}'
just soak --real 5733 --repo live-gitea:scale-commerce/cart-service                  # ten minutes, then the tour
just soak --real 5733 --repo live-gitea:scale-commerce/cart-service --minutes 40 --size 2560x1440@1.25
```

- **The pull requests' records** (`pulls.json`) come from the backend itself, as for the inner pages:
  `standin-repos.py --out <dir>/repos --port 5733 --pulls pulls.json --pr scale-commerce/cart-service#124 …`.
- **A fresh copy a run.** The browsing writes visits, and a backend that has answered a tree keeps it.
- **Six requests at once.** Headless Chrome sends at most six requests to the backend at a time, so a burst of reads
  the app's own window would send together (300 folders read again) reaches the backend six at a time here. The
  page's side of such a burst, its main thread's time and the hover through it, shows in full; the backend's side does
  not. `crates/knobas-app`'s own tests hold that (`folders_asked_at_once_read_the_tree_once`).
- **The fixture's bridge kept every listener's callback** until this pass, and with it the page that listened: a soak
  over it grew by 2,500 nodes and 1.2 MB a minute that the app's own window never held. Tauri drops the callback with
  the listener, and so does `fake-tauri.ts` now; a soak's node and heap growth is the app's.
- **The lines.** The calls a run's end asks and their growth hold the fault itself whatever else the machine is
  doing (25 calls against 113 after ten minutes of the old re-read). A hover's `felt` lines are Björn's targets, 100, 100
  and 200 ms, and 300 through a return: a quiet Mac measures 18, 21 and 23 ms at rest, 27 through a run's end and 170
  through a return, and one at a load of 40 from other sessions 66, 24 and 33, 128 and 284. The `cost` lines hold the
  main thread's share of a hover at rest.
- **Read `afterSync` and `afterReturn`, not `hover`,** for a page that reads again too much: a hover at rest is two
  restyled elements whatever the page holds. A return's hover includes the return's own first frame, which
  `open-budget`'s `return` rounds hold.

## On a real database

**The saved home** is `~/Library/Caches/knobas-perf/scale-db/final-home`, Björn's scale (`testenv/README.md`,
*Björn's scale*): 39,770 items (20,007 Jira tickets, 5,032 Confluence pages, 62 Gitea repositories with 2,886 issues
and 1,707 pull requests, 2,620 TeamCity builds, 218 monitors), at the head's schema. Its sources keep the live
products' addresses, their credentials are in the profile's `dev-secrets.json` and none of them syncs by itself (a
ten-year interval). `<saved home>` below means that path. Its dump is beside it (`scale-final-home.dump`), and
`testenv/scale-db-restore.sh <dump> <new home>` rebuilds the home from the dump, migrated to the checkout's head, in
about a minute. That is the way to bring it to a newer schema, and the way back after a loss: the first copy, under
`/private/tmp`, was deleted by macOS's periodic clean-up on 2026-11-15.

```sh
cp -cR <saved home> "$TMPDIR/home"                        # an APFS clone, never the original
mv "$TMPDIR/home/Library/Application Support/dev.knobas.desktop/demo/dev-secrets.json" "$TMPDIR/"   # no sync while measuring
HOME="$TMPDIR/home" cargo run -p knobas-app --features test-util --example headless_ipc -- \
  --data-dir "$TMPDIR/home/Library/Application Support/dev.knobas.desktop" --port 5411
just frame-budget --real 5411
curl -X POST http://127.0.0.1:5411/quit
```

The scenarios name the saved scale database's pages (`#/page/live-confluence%3A362334`, `#/docs/space/SCLENG`,
a TeamCity pipeline); on the fixture those find nothing and are left out of its lines.

## In Edge or WebView2 on Windows

What the Mac cannot show: whether a list scrolls on the compositor at 100% scale. On a person's own PC, from one
scratch folder, and only while they are away:

1. A portable `node.exe` in the folder (the official zip, unpacked), and this folder's files beside it.
2. The built app in the folder, served on loopback: `node serve.mjs <app> 5412`. Loopback only, so Windows asks no
   firewall question.
3. For real data, the Mac's windowless backend through the WSL box: `ssh -N -R 127.0.0.1:5411:127.0.0.1:5411 <wsl
   host>`; Windows reaches WSL's loopback.
4. An Edge of the run's own, on its own profile folder, as an app window with
   `--remote-debugging-port=9444 --user-data-dir=<folder>\edge-profile --app=http://127.0.0.1:5412/?fake-ipc&fake-real=5411`.
5. `node.exe run.mjs --attach 9444 --match http://127.0.0.1:5412 --throttle 1,4,6 --guard "powershell.exe -NoProfile
   -File guard.ps1" --out <file>`: the guard runs before every scenario and stops the run when the last input in the
   session is under five minutes old.
6. The soak. With no window at all (the hover-lag pass ran it so, 2026-11-16, under the PC's timing lock): the probe
   starts a headless Edge of its own from the portable Node where `KNOBAS_CHROME` names `msedge.exe` (through WSL
   interop, `WSLENV=KNOBAS_CHROME`). `launch()` cannot stop it on Windows, so stop the processes whose command line
   holds the run's profile folder afterwards. Or in the app window of step 4 (`--attach` loads the page afresh with
   `?fake-hooks`, at the size the window has):
   `node.exe soak.mjs --attach 9444 --match http://127.0.0.1:5412 --app "http://127.0.0.1:5412/?fake-ipc&fake-hooks&fake-real=5411#/code"
   --repo <entity id> --minutes 15 --out <file>`, the backend's repository built and pinned on the Mac as *The soak* says.

`win/` has the three PowerShell helpers, for a `scripts` folder one below the scratch folder: `guard.ps1` (go on only
if the last input is five minutes old or the run's own), `wake.ps1 -Do wake|hold|release` (a pointer move of zero
pixels to turn the display on, refused unless the person is away, and a hidden holder that keeps it on). The display
must be on: with it off Windows presents nothing and the page idles at about 167 frames a second on a fallback timer.
Remove the scratch folder at the end.

## WKWebView (`wk/`)

WKWebView has no DevTools protocol, and it scrolls on a thread of its own, so what it shows takes a recording of the
window. The Mac report's instruments:

| File | Does |
|---|---|
| `wkhost.swift` | a bare WKWebView window on a URL; `--uncap` turns off WebKit's 60 fps preference before the web view is made |
| `kn.swift` | real wheel input at the HID level, one event a display refresh, and the window's frames through ScreenCaptureKit: how far a column of pixels moved in each shown frame (`kn scroll <pid> X Y PX_PER_SEC SECONDS --hid --capture --strip`) |
| `serve-probe.mjs`, `probe-page.js` | the app served with the probe first in its `<head>`, evaluated over HTTP (`POST /__probe`), since there is no protocol to drive it with |
| `host.sh` | `start [--uncap] [HASH]`, `stop`, `js`: the fixture at scale in `wkhost` |
| `self-scroll.mjs` | the Tickets list with every row loaded under a real wheel scroll, several runs, with the list's row changes counted |

Build the tools with `swiftc -O wkhost.swift -o $WORK/tools/wkhost` and `swiftc -O kn.swift -o $WORK/tools/kn`. They
post real input and record the screen, so the Mac must be free: nothing else of yours on screen, and the pointer goes
where the scroll is. The report measured the native AppKit table at 109.5 to 112.7 shown frames a second with the
same input, which is this method's ceiling on a 120 Hz panel.
