# 2026-11-16: Every sidebar drags, and a page too slow to follow gets a ghost

**Asked (Björn, on PR #2479, through the orchestrator):** "I want everywhere where I have a sidebar it to be possible
to move it side-to-side dragging it with my mouse". His standing rule: directness, performance and responsiveness over
data scarcity; a drag at 239 Hz and at CPU 6x without visible stalls, full screen and high DPI included. One question
went to Fable on xhigh under his delegation (below).

## What was built

One shared edge (`TfSideEdge.svelte`, `side-width.svelte.ts`, `side-pace.ts`, `css/side-edge.css`) on all fourteen
sidebars: Tickets' views and its selected ticket, the ticket page's card (its own handle of #2402 retired into it, its
fit on double click kept), Builds' pipelines, a repository's file tree and its Builds tab's branches, Compare's side, a
diff's file list (Compare, a pull request's *Files changed*, a commit, local changes, the diff page), Docs' page tree and
outline, Settings' sections, a pull request's side, a link set's side, the asset pane. The Inbox, Monitors and the top
bar have no sidebar.

- **Where it sits.** An out-of-flow child of the sidebar's grid, placed on the column line by `grid-column` (a grid's
  absolutely placed child takes its grid area as its box), 8 px to grab, centred on the line or on the middle of the
  gap. It takes no cell and scrolls with neither column.
- **How it looks** (the frontend-design pass): nothing at rest, so every page at its default widths draws as it did
  (`pixel-same.py`, below). Under the pointer, after 120 ms, a 2 px line in `--ink-2` and the resize cursor; held, the
  line is `--ink` and a cover over the window keeps the cursor and takes the page's hover; focused from the keyboard,
  `--focus`. Held wins over focused. No band, no grip, no motion: the boldest thing on these pages stays the content.
- **What it does.** A drag sets the width; a double click or Enter returns the default (the card fits instead); ←/→
  step 16 px, 64 with Shift; Home and End go to the floor and the cap. `role="separator"`, `aria-valuenow` and its
  bounds, a label per sidebar (*Resize the pipelines*).
- **Limits.** A floor per sidebar (160 to 240 px) and a cap that is its share of the grid (30 to 60 %), checked in the
  drag and again in CSS (`clamp(floor, width, share)`), so a width holds at 5120 px and a window made narrower keeps the
  main column. Docs' outline also leaves the page its 660 px reading column and gap (`keep`), and `readerFit` folds the
  outline by its kept width.
- **Where the width is kept.** Per sidebar in the webview's `localStorage` (`knobas.side.<id>`; the card keeps
  `knobas.ticket.side`), read when the shell mounts, as the chosen job is: across page switches and restarts. A sidebar
  nobody resized keeps nothing, and its page's breakpoints still narrow it.
- **Cheap per frame.** Pointer moves are coalesced into one `requestAnimationFrame`; each frame writes one custom
  property (`--side-<id>`) registered with `@property { inherits: false }`, so a write restyles the grid alone; nothing
  Svelte draws hears of the drag until the pointer lets go, when the width is kept once. Windowed lists re-measure
  through their own observers.

## The ruling: a ghost where the page cannot keep up (Fable, xhigh)

Live reflow everywhere held 60 fps at 1x on every page and size, but at 6x the busy pages could not lay themselves out
at a new width in a frame: at 1440 x 868 Builds 37 fps (frames about 40 ms), a pipeline 39, Tickets 44, Docs' tree 54;
at 2560 x 1440 at 125 % and 5120 x 1440 Builds 27 fps with frames of 64 to 68 ms. A Builds drag frame was about 6 ms of
main-thread work at 1x against under 1 ms idle (paint 2.6, layout 1.7, pre-paint 0.9, layerize 0.4), spread over both
columns' rows; no single CSS feature in them cost more than a tenth of it. It is the page laid out at a new width, as a
window resize is.

Options put: keep live reflow (A); an adaptive ghost (B); hold the main column and slide it (C); always a ghost (D).
**Fable chose B**: A breaks the 6x line, D throws away what holds at 1x everywhere, C still costs about 30 ms a frame on
Builds (half of it is the rail's own rows) and shows clipped content. As built (`side-pace.ts`):

- Every drag starts live. A frame that wrote a width is costed by the gap to the next frame; over 19 ms it is slow (the
  55 fps line is 18.2 ms; 19 sits between the vsync multiples of 60, 100, 120, 144 and 239 Hz panels).
- Two slow frames within any four written in a row turn the drag into a ghost until release; one alone never does, so
  a collection or a sync landing mid-drag does not.
- The ghost: the columns keep the last width written; the held line follows the pointer by a transform, clamped where
  the width clamps; the width lands when the pointer has rested within 2 px for 150 ms, and on release, the transform
  reset in the same frame. Same line, no tint.
- Memory: a sidebar that tripped starts its later drags as a ghost for the session. The frame that lands the width on
  release is costed the same way, and two releases in a row within 19 ms, each changing the width, clear the mark.
- Measured, not listed per sidebar: the same Builds rail is 60 fps at 1x and 27 at 6x.

Accepted: the two frames that trip the ghost, once a drag, and the frame that lands the width on Builds at 6x on the
large sizes (one 64 to 68 ms task), which comes while the pointer rests or after it lets go.

## Measured

Björn's PC, a headless Chrome 156 in its WSL box driven from the Mac over an SSH tunnel (the Mac's lock was held by
other agents' budgets the whole afternoon), the saved scale database through the windowless backend on the Mac; real
mouse events, a move every 4.2 ms, out 220 px and back twice in 2.4 s (`run.mjs --kinds drag`). Drag rate, after the
trip where it tripped:

| Case | 1440 x 868 @1, 1x / 4x / 6x | 2560 x 1440 @1.25, 1x / 4x / 6x | 5120 x 1440 @1, 1x / 4x / 6x |
|---|---|---|---|
| Code's file tree | 60 / 60 / 60, live | 60 / 60 / 60, live | 60 / 60 / 60, live |
| A pull request's side | 60 / 60 / 60, live | 60 / 60 / 60, live | 60 / 60 / 60, live |
| Settings | 60 / 60 / 60, live | 60 / 60 / 60, live | 60 / 60 / 60, live |
| Docs' tree | 60 / 58.5 live / 60 ghost | 60 / 60 ghost / 60 ghost | 60 / 60 ghost / 60 ghost |
| Docs' outline | 60 / 60 / 60 ghost | 60 / 60 ghost / 60 ghost | 60 / 60 ghost / 60 ghost |
| Builds | 60 / 60 ghost / 60 ghost | 60 / 60 ghost / 60 ghost | 60 / 60 ghost / 60 ghost |
| A pipeline | 60 / 60 ghost / 60 ghost | 60 / 60 ghost / 60 ghost | 60 / 59.1 ghost / 59.1 ghost |
| Tickets' views | 60 / 56.5 live / 60 ghost | 60 / 60 ghost / 60 ghost | 60 / 60 ghost / 60 ghost |
| Compare's side (fixture) | 60 / 60 / 60, live | 60 / 60 / 60, live | 60 / 60 / 59.7, live |
| A diff's file list (fixture) | 60 / 60 / 60 ghost | 60 / 60 ghost / 60 ghost | 60 / 60 ghost / 60 ghost |

No drag tripped at 1x at any size. After a trip the worst gap was 16.8 ms, and 33 ms once on a pipeline at the large
sizes; a live drag's worst gap at 4x was 33 ms. Three single gaps (one of 96 ms on Compare's side at 6x, one of 67 ms
after the trip on the file list at 4x at the two large sizes) did not come back on a rerun of that case alone. Before
the ghost the same cases at 6x ran 27 to 54 fps. `budget.json`'s `real.drag` lines (no trip at 1x; 55 fps and no gap
over 60 ms at 4x and 6x): 56 within, 0 over, at each of the three sizes.

Compare and a diff's file list are measured on the fixture's compare: the saved database's repositories hold one
branch each, and a compare of thousands of files needs `big-compare.mjs`'s stand-in forge, which was not set up. Both
the file list and the file cards are windowed (`Windowed`), so what a drag lays out does not grow with the count.
A pull request's side was measured where the page could not read the pull request (no keychain in a scratch home):
its edge and grid, not its panels.

**Pixels.** Twelve screens with sidebars (Tickets, a ticket, Builds, a pipeline, a repository's files, Builds tab and
Compare, a pull request and its files, a Docs page, Settings, the asset tree), base `39797c78` and the head, at
1440 x 868, 2560 x 1440 at 125 % and 1920 x 1080 at 200 %, `pixel-same.py`: 11, 10 and 10 the same; the rest under its
600 px line but one, Tickets' filter bar at 200 % (1,183 px, the same words a colour step apart in their anti-aliasing,
the crops side by side identical to the eye). Nothing changed at rest.

## Left

- A compare of thousands of files on the stand-in forge, on the Mac when its lock is free.
- In WebView2 and WKWebView themselves: these numbers are headless Chrome's on the PC's CPU. `side-pace.ts` measures
  each machine's own frames, so a fast machine stays live and a slow one gets the ghost.
