# The frame probes

How smooth the app is, measured instead of felt, from the two smoothness reports of PR #2479 (2026-11-11): Windows in
WebView2 with the main thread slowed to stand in for a slower laptop, and the Mac in WKWebView at 120 Hz. Rulings in
`docs/decisions/2026-11-11-smooth-on-slow-laptops.md`.

## `just frame-budget`

Builds the app as the probes measure it, serves it on loopback, drives it in a headless Chrome at 1x and 4x, and
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

## The pieces

| File | Does |
|---|---|
| `run.mjs` | the scenarios: `scroll` (a script-driven scroll at 3,000 px/s for 4 s, under a trace), `wheel` (a wheel scroll the browser synthesizes), `type` (a filter typed a key every 120 ms) and `nav` (each page switch, first visit and return), at each `--throttle` |
| `probe.js` | the page half: every frame, the time from its callback to the end of its rendering (`frame work`), script by kind, long animation frames, event timings, IPC |
| `frames.mjs` | the trace readers: what reached the display (the compositor's frame reporters), how many elements each style pass computed, the main thread's time by kind, its longest tasks |
| `budget.mjs`, `budget.json` | the lines, per corpus (`fake`, `real`), and the check |
| `scrollers.mjs` | every box that scrolls on the app's pages, light and dark, with its own background and the colour under it; `--strict` fails on a transparent one |
| `invalidations.mjs` | why a scroll restyles what it does, from Chromium's own invalidation records: for a rule that restyles far more than the rows that changed |
| `profile.mjs` | a CPU profile of one moment (`--setup`, then `--act`), by function, for a slow key or page switch the frame probe names only by its task |
| `serve.mjs` | serves a built app on 127.0.0.1 where there is no Python (Windows) |
| `cdp.mjs` | the DevTools-protocol client: `launch()` a headless Chrome of its own, or `attach(port)` to one listening |
| `frame-budget.sh` | what `just frame-budget` runs |
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

## On a real database

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
