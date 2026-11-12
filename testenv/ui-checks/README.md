# UI checks

`testenv/ui-checks/` holds three checks the UX polish loop (#2479) ran on every round: the press trips, the screenshot
comparison and the real-window driver. `testenv/perf/` holds the frame probes. They caught regressions that the suite
and the screenshots missed. None is a gate, and none starts a product. They use the mock corpus under `?fake-ipc`,
except where a command below names a real database or `--live`.

## Run the press trips

`trips.mjs`, with `chrome.mjs` beside it, makes 238 trips a person makes, in headless Chrome against a built app: a
press twice by key and by pointer, a leave and a return while a toast is up, typing into a dialog, `Esc` in every field,
⌘F. The trips are a hand-written table at the top of the file, and its header names each kind and what fails it. From
the repository root, build the app in development mode, so `?fake-ipc` is there, and serve it for the one command:

```sh
testenv/ui-checks/build-app.sh . "$TMPDIR/knobas-built"
testenv/ui-checks/with-built.sh "$TMPDIR/knobas-built" 5340 node testenv/ui-checks/trips.mjs "$TMPDIR/trips" --port 5340 --strict
```

It writes `trips.json` and `trips.md` to the out folder and exits 1 if a trip fails or can no longer find its control.
`--only a,b` runs the trips whose id, kind, finding, page or route holds one of the words. `--workers n` sets how many
Chromes run side by side, 2 by default. `--strict` adds the trips marked strict, which is how the loop ran it.
`KNOBAS_CHROME` names another Chrome. `with-built.sh` serves the folder on 127.0.0.1 for the length of the command and
refuses a port that already answers.

## Check that rows hold still

The `rows-still` trips (2026-11-11) record every visible row's box on each frame, from a list's first draw until it
settles. They fail if a drawn row moves, grows or leaves, or if one comes in above it. Each page runs five ways: the
first visit, a return, a return after a sync, a sync while shown, and arriving by the launcher. Three fixture switches
feed them:

| Switch | Does |
| --- | --- |
| `?fake-late=<command>:<ms>,...` | Answers each named read that late, `*:<ms>` for every command, so it lands after the first draw |
| `?fake-hooks` | Puts `window.__fake` in the page (`corpus`, `endRuns`, `inboxNew`, `runNew`, `monitorDown`), so a trip adds rows and ends a run that changed something |
| `?fake-real=<port>` | Sends every command to the app's own backend running with no window (`crates/knobas-app/examples/headless_ipc.rs`), so the built front end draws a real database |

The windowless backend:

```sh
cp -cR <saved home> "$TMPDIR/home"                       # an APFS clone, never the original
HOME="$TMPDIR/home" cargo run -p knobas-app --features test-util --example headless_ipc -- \
  --data-dir "$TMPDIR/home/Library/Application Support/dev.knobas.desktop" --port 5399
# then open the built app at ?fake-ipc&fake-real=5399
curl -X POST http://127.0.0.1:5399/quit                  # stops its scheduler and its PostgreSQL
```

It opens the demo profile under the directory you name and nothing else, shows no desktop notification and opens no
window. Its scheduler syncs as the app's does. If you kill it without the `/quit`, the profile's PostgreSQL keeps
running, and `pg_ctl stop -D <profile>/db/data` stops it.

## Measure the frame budget

`just frame-budget` (2026-11-11) builds the app as the probes measure it (`KNOBAS_PERF=1`, which is `?fake-ipc` with
Svelte's runtime as released) and drives it in a headless Chrome with the main thread slowed 1x and 4x. It fails when a
list scrolls, a filter key paints or a page switches slower than its line in `testenv/perf/budget.json`, or when a box
that scrolls has no opaque background. It runs on the fixture at scale by default, or with `--real <port>` on the
windowless backend over a saved database. `just open-budget` does the same for how long a press takes to show a page.
[../perf/README.md](../perf/README.md) has each tool, what its numbers mean, and how to run it on Windows and in WKWebView.

## Compare two rounds of screenshots

`pixel-same.py` takes two folders of screenshots of the same screens on two heads, and the manifest that named them. It
lists the screens whose pixels changed that nobody meant to change, most changed first. "Meant to" is a screen with
`arrows` in the manifest, or one named as a whole word in a text passed with `--names-from`.

```sh
testenv/ui-checks/pixel-same.py <old shots> <new shots> <manifest.json> [--workers n] [--names-from notes.md ...]
```

It prints only and exits 0 whatever it finds. The same build shot twice differs by up to about 540 px on a screen (a
clock, a caret), so it draws its line at 600 px.

## Drive the real window

`real-window.sh` (macOS) runs the debug app in its own window, with its WKWebView and its menu, on the mock corpus, and
drives it with real CGEvent input: pointer moves with real `movementX`, presses, drags, wheels and keys. It reaches what
headless Chrome cannot, such as a hover, a press guard that reads `movementX`, macOS's own text checking in a field, and
which key the app's menu takes. A probe on the dev server's HMR socket reads the page back.

Before you start:

- A debug build at `target/debug/knobas-app` (`just demo` once builds it), `swiftc`, `app/node_modules`, an
  Accessibility grant for the terminal, and port 1420 free.
- No other knobas running. It refuses while one is.
- A Mac nobody is using. It takes the screen and the pointer while it runs.

```sh
testenv/ui-checks/real-window.sh start                 # dev server on 1420 and the debug app with --demo, window at 1440 x 900
testenv/ui-checks/real-window.sh start --live          # the same on the demo profile's own sources: run `just live-sources` first
testenv/ui-checks/real-window.sh js 'location.hash'    # a value from the page
testenv/ui-checks/real-window.sh ev move 400 225 click 1378 221 key f cmd type retry
testenv/ui-checks/real-window.sh keys '#/page/confluence%3Asepa' .tf-doc .tf-rd-main
testenv/ui-checks/real-window.sh tabs '#/inbox' 12
testenv/ui-checks/real-window.sh restart               # after changing a system setting such as Full Keyboard Access
testenv/ui-checks/real-window.sh stop
```

- Without `--live` the fake takes the place of the page's IPC bridge, so the window never talks to the demo profile's
  sources.
- `start --live` is `just live-demo` driven by the same input. The page keeps the real bridge and the profile's stored
  credentials, so the window shows what the live products synced, and its writes are real writes to them. Take back what
  a run wrote.
- `real-window/page-helpers.js` is what `js` can call as `window.__q`: where to press, what has focus, how far a box
  scrolled.
