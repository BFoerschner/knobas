# 2026-11-17: A hover at a sync's end, a drag's start, the web view's menu, and Docs' one character, on the PC

**Asked (the orchestrator, for Björn, after the ux16 Windows pass, `windows-pass-ux16.md`):** the front-end items the
pass left open on his PC. Hover spikes at a sync's end at 6x (single moves up to 227 ms). A sidebar drag's first
130 ms at 4x and 6x, with one gap of 21 to 58 ms before the ghost. WebView2's own *Import passwords* and *More tools*
in a field's menu. Right-click inside a Docs embed and a renderer's frame, not checked. Monitors' rows holding still
at 125 %, not checked. The Docs search's one character, not checked. His standing rule: directness, performance and
responsiveness over data scarcity; a judgment goes to Fable on xhigh. Targets on the PC at CPU 1x / 4x / 6x: a hover,
key or click within 100 / 100 / 200 ms (a hover a frame or two), a page within 100 / 200 / 300 ms, a drag and a scroll
without visible stalls at 239 Hz.

**How it was measured.** Björn's PC (Ryzen 5 5600X, 239 Hz, WebView2 157), idle for hours throughout, under the PC's
timing lock. knobas's own window: the v3.2.9-ux16 MSI unpacked into a scratch folder (never installed), a scratch
profile holding the saved scale database, its Gitea and Forgejo sources pointed at the live products in WSL, page
1440 x 868 at 100 %. A front end built from this branch was put into the same window over the DevTools protocol (every
`http://tauri.localhost/` request answered from the build, the document keeping the app's own headers and CSP), so
the before and after ran against one backend and one profile. CPU slowed through CDP (main thread only, an
approximation of a slower laptop). WebView2's menu was read in a scratch WebView2 window on the same runtime (the SDK
from NuGet in the scratch folder), its items logged by name from `ContextMenuRequested` and the drawn menu read by UI
Automation. Everything was removed afterwards.

## 1. The hover at a sync's end: the probe's own collections

`soak.mjs` timed each pointer move with a frame callback and a `MessageChannel` message after it **on every frame**,
for as long as it watched (at 239 Hz, 239 of each a second). That allocation made the page run a full collection
every 9 to 10 s: in a trace, `MajorGC` of 46 to 49 ms at 6x (mostly the DOM's tracing, `V8.GC_HEAP_EMBEDDER_TRACING_
EPILOGUE`), every one landing on a slow move. With the watcher on and no sync at all, slow moves of 61 to 130 ms came
every ~10 s; with it off, through a whole sync, none. The long frames the pass saw had no script and no style in them
(long-animation-frame entries with empty `scripts`), which is a collection's signature.

Then what a sync's end does to a hover, without the watcher, at 6x, on the shipped ux16 front end:

| A Gitea run through the hover, 6x, a move every 80 ms | moves | p95 | max | frames over 50 ms |
|---|---|---|---|---|
| no run, 40 s | 431 | 6.4 ms | 15 ms | 0 |
| a run that changed nothing | 403 | 9.4 ms | 27 ms | 0 |
| a run that moved the shown repository's `main` (a scratch copy of cart-service, one commit a run, three runs) | 482 to 541 | 9.5 to 9.9 ms | 31, 34, 108 ms | 0, 0, 2 |

The one 108 ms move met the run's end itself: 17 reads asked in 45 ms (the status bar's counts, the repository's
view, its branches' pulls and pipelines, a folder), and one answer's continuation ran 52 ms at 6x (about 9 ms at 1x).
That is within the 200 ms line at 6x and was left. A 45 s trace of a sync with the pointer moving held two
collections, 23 ms the larger, neither under a slow move.

**Changed:** `soak.mjs`'s watcher asks for a frame only when a move waits on one. The soak again, 8 laps, a Gitea sync
every 45 s as the pass ran it, the same front end:

| The soak at 6x | hover through a run, p95 | max | hover at rest, p95 1x / 4x / 6x |
|---|---|---|---|
| before (the per-frame watcher) | 10.1 to 11.3 ms | 82 to 244 ms | 6.7 to 7.5 / 5.6 to 9.8 / 6.4 to 12.2 ms |
| after | 8.5 to 9.6 ms | 12 to 29 ms | 6.9 to 7.7 / 6.1 to 11.6 / 5 to 13 ms |

A return to the page still holds its own first frame (89 to 128 ms at 6x, the page shown again), within a page's
300 ms line.

## 2. A drag's start (Fable, xhigh)

What the pass saw is two slow frames before the ghost: every drag started live and turned into a ghost on the second
slow frame of four, and a sidebar that tripped started its later drags as a ghost only for the session.

**Fable's ruling,** taken in full (options weighed: a first frame tripping at 38 ms; at 19 ms; timing the first
frame's forced layout; a list of busy pages; only keeping the memory):

1. **This drag** turns into a ghost when its first written frame is over 19 ms, or two of any four written are.
2. **The sidebar** is marked slow on two slow frames in four, every frame that writes a width costed, a ghost's
   landings among them (at rest and on release). One slow first frame, a 1x hiccup on a 60 Hz panel, costs that one
   drag and marks nothing.
3. **The mark is kept** in `localStorage` (`knobas.side-pace.<id>`, present means a ghost from the first frame), read
   once, written on mark and on clear; a storage that throws keeps it for the session. A slow machine pays the first
   slow frame once per sidebar, not once a session.
4. **It clears** after two landings in a row at 19 ms or under, at rest or on release (before, only a release that
   changed the width counted, so a drag that rested before letting go never cleared it, and a kept mark would have
   lasted for ever).

Refused: a 38 ms first-frame line (a 4x Builds frame is about 24 ms, so it would change nothing at 4x), the forced
layout's time (under a third of a Builds frame, a different share on every page), a list of busy pages (the 11-16
ruling's *measured, not listed*).

| knobas's window, 1440 x 868, real mouse events every 4.2 ms | 1x | 4x | 6x |
|---|---|---|---|
| ux16: Builds, a pipeline, Tickets, Docs' tree | live, worst gap 8 to 13 ms | ghost after 179 to 193 ms, 2 gaps over 19 ms (21 to 25) before it; Docs' tree after 1,222 ms, 5 gaps up to 42 | ghost after 188 to 246 ms, 2 gaps over 19 ms (21 to 46) before it |
| this head, no mark kept | live, worst gap 4 to 9 ms | ghost at the first slow frame: 1 gap over 19 (21 to 25) before it, Tickets 2; Docs' tree after 1,191 ms, 4 gaps up to 38 | ghost at the first slow frame: 1 gap over 19 (21 to 42) before it; Code's tree 2 |
| this head, marks kept | | Builds, Tickets, a pipeline, Docs' tree: 1 gap of 21 to 25 ms (the press's own frame or a sidebar not yet marked) | Builds and a pipeline: none over 19 ms (worst 12.5 to 16.7); others 1 (21 to 33) |
| 2048 x 1152 at 125 % (2560 x 1440 device px, by CDP), no mark kept | live, worst 8 to 13 ms | ghost at the first slow frame, 1 gap (21 to 29); Docs' tree 2 of 21 | ghost at the first slow frame, 1 gap (29 to 50) |

After the ghost: 236 to 240 fps, but 197 to 227 on Code's tree, Builds and a pipeline at 6x and 180 on Builds at 125 %
(worst gap 12.6 ms). Docs' tree at 4x lays out at about the line, so its slow frames come one in four or wider and it
trips late, as before; Code's tree at 4x stays live with one 25 ms gap, as on ux16.
Still paid, as ruled: one frame of 21 to 50 ms on the first drag of a busy sidebar on a slow machine.

## 3 and 4. WebView2's own menu, in fields and in frames (Fable, xhigh)

**What WebView2 157 offered**, by item name (`ContextMenuRequested`, the scratch window):

| Where | Items |
|---|---|
| A field, spellcheck off (Tickets' filter) | emoji · other (*Import passwords*) · undo, redo · cut, copy, paste, pasteAndMatchStyle, selectAll · other (*Writing direction* ▸) · moreTools (*Share*, *Voice typing*) · inspectElement |
| A misspelled word in a field | spellcheck ×3 (the suggestions, *Add to dictionary*) · cut, copy, paste, pasteAndMatchStyle · moreTools · inspectElement |
| A renderer's frame (`sandbox=""`) and an embed (`allow-scripts`), text or space | back, forward, reload · saveAs, print · moreTools · inspectElement |
| A selection in a frame | copy, copyLinkToHighlight, print · moreTools · inspectElement |
| A link in a frame | openLinkInNewWindow · saveLinkAs, copyLinkLocation · moreTools · inspectElement |

So both frames showed *Back* and *Refresh*, which reloads the whole app and drops every kept page (item 4: confirmed,
and fixed below). `IsRequestedForMainFrame` read false for the main page's own fields too, so nothing here leans on it.

**Fable's ruling:** native, one allow-list wherever the event fires. WebView2 raises `ContextMenuRequested` only where
the page let the menu through (a field; anything in a frame), and Chromium offers only what applies to the press, so
one list serves both. A knobas-drawn field menu was refused: it loses the spelling suggestions (script has no access
to them) and cannot reach inside a frame.

- **Kept, by name:** `spellcheck` (and the documented `spellCheck`), `emoji`, `undo`, `redo`, `cut`, `copy`, `paste`,
  `pasteAndMatchStyle`, `selectAll`, `copyLinkLocation`, `copyImage`; `inspectElement` in a development build only.
- **Removed:** every other name, `other` included (how *Import passwords* got in; the next thing Edge adds comes the
  same way), and every submenu (*More tools*, *Writing direction*). No separator left first, last or doubled; no menu
  at all where nothing is left.
- **Synchronous:** the items go before WebView2 draws them, no read and no wait.
- **Fails soft:** without `ICoreWebView2_11` or a reachable web view, the menu stays as WebView2 draws it, one `debug`
  line. `AreDefaultContextMenusEnabled` is never turned off.
- **Every window knobas makes:** the main window and the capture window (which mounts no shell, so its page menu was
  WebView2's).
- **A `select` is no field** for knobas's own menu (`thing.ts`): it takes no typing, and the web views offered it
  their page menu; it now gets knobas's (nothing on nothing).

**The rule in the scratch window** (the same list and order as `webview_menu.rs`), the drawn menus read back:

| Where | Before | With the rule |
|---|---|---|
| Tickets' filter | Emoji, Import passwords, Undo, Cut, Copy, Paste, Paste as plain text, Select all, Writing direction, More tools, Inspect | Emoji, Undo, Cut, Copy, Paste, Paste as plain text, Select all |
| A misspelled word | receive, Cut, Copy, Paste, Paste as plain text, More tools, Inspect | receive, Cut, Copy, Paste, Paste as plain text |
| A field in an embed | as the filter | as the filter's, trimmed |
| A frame's text or space | Back, Refresh, Save as, Print, More tools, Inspect | no menu |
| A selection in a frame | Copy, Copy link to highlight, Print, More tools, Inspect | Copy |
| A link in a frame | Open link in new window, Save link as, Copy link, More tools, Inspect | Copy link |

**Not yet seen in knobas's own window:** `webview_menu.rs` is Rust, and nothing here builds knobas for Windows; it
compiles and passes clippy for `x86_64-pc-windows-msvc` (checked in a scratch crate on the same `webview2-com` and
`windows-core`), and its rule is the one the scratch window ran. The next pre-release's Windows pass owes the right
clicks above in the installed app.

## 5. Monitors at 125 % hold still

knobas's window at 1440 x 868 CSS px, the scale by CDP (1.25, and 1 and 1.5 beside it), 80 wheel steps from the top to
the end of Monitors' 218 rows: every drawn row's place in the list read after each step against where it was first
drawn. 22 row swaps, 3,637 comparisons, **0 rows moved** at each scale. (Windows' own display scaling was not changed:
it is Björn's setting.)

## 6. The Docs search's one character: a pause nobody types into

Right: `r` answers 3,019 pages by name, those whose title begins with it first (*Report exporter…*, *Retrospective…*,
*Release 26.10.1 notes*); `p` 2,204 (*Payments architecture overview*, *PUT /v2/…*, *Partner api…*); `s` 5,027;
`x`, which no name holds and no body either, *0 pages*. The read itself: 50 to 63 ms for `r` through the IPC, 34 to
40 for `p`, 14 to 21 for `x`.

Within target: it was not. From the search's address to its hits drawn, `r` took 203 to 208 / 269 to 274 / 318 to
320 ms, and even `x` 175 / 188 to 192 / 200 to 202 ms. The search waited 150 ms for a pause in typing
(`SEARCH_PAUSE_MS`, #2200) before it asked. That pause was for Docs' own search box, which #2425 moved to the launcher:
a search now reaches Docs whole (the launcher's Enter, a chip), so the pause only delayed every answer. **Removed:** a
search's address asks at once.

| Address to hits drawn, ms | 1x | 4x | 6x |
|---|---|---|---|
| `r`, before | 203 to 208 | 269 to 274 | 318 to 320 |
| `r`, after | 54 to 59 | 125 to 138 | 180 to 194 |
| `p`, after | 50 to 52 | 115 to 116 | 165 to 189 |
| `s`, after | 60 to 62 | 120 to 130 | 174 to 187 |
| `x`, after | 25 to 27 | 42 to 45 | 58 to 60 |
| `r` at 2048 x 1152 @ 1.25, after | 55 to 67 | 123 to 128 | 174 to 176 |

Opening the search is a page (the launcher's Enter): within 100 / 200 / 300 ms at every throttle and both sizes.

## Also

- The wiring test that pins the main window's grants (`tests/it/wiring.rs`) failed on the branch since the zoom merge
  (f6a71d58), which granted `core:webview:allow-set-webview-zoom` and ran the front end's gates only. The grant is
  documented in `capabilities/default.json`; the test now names it.
- `run.mjs`'s drag cases print when the ghost took over and the slow gaps before it, and `--pace cold` clears the kept
  marks first, so a first drag can be measured on a machine that has dragged before.
