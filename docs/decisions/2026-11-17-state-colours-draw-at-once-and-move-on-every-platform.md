# 2026-11-17: State colours draw at once, and a chip moves on every platform

**Asked (Björn, on v3.2.9-ux17, Windows, about 20,000 Jira tickets):** "State colours take forever to appear. Why. And I
can't change them", on Settings › Appearance.

## Why it was slow

To offer every status the mirror has seen, the *State colours* read walked every live Jira ticket's payload through the
state declarations, one ticket at a time (`SEEN_WORDS`). On the saved scale database (20,007 Jira tickets, four
statuses) `EXPLAIN ANALYZE` put that statement at 2.5 s on a quiet Mac and 11 to 14 s on a loaded one: a sequential scan
of all 39,770 items, and per ticket a correlated walk of the declarations with a sort. A set and a reset answered with
the same read, so each move waited as long. The Backup section hid its share ticks until that read answered, and the
launcher's ⌘? legend ran it again on every activity line while it was shown.

## Why a change didn't take

The board moved a chip by HTML5 drag, or by ← / → on a focused chip. On Windows, Tauri's `dragDropEnabled` (on by
default) makes wry revoke WebView2's drop target and register its own over it (`wry-0.55.1/src/webview2/drag_drop.rs`;
tauri-utils: "Disabling it is required to use HTML5 drag and drop on the frontend on Windows"), so the page's `drop`
never fired. A click on a chip did nothing. The keys worked, but each press waited the 2.5 s or more, and a press while
one was out was dropped without a word. A renderer pipeline's step grip used the same drag.

## Fable's ruling (xhigh)

1. **Pointer drag on both boards**, not `dragDropEnabled: false`: wry turns off WebView2's external drop only beside its
   own handler, so without it a file dropped on the window would be opened by WebView2, and nothing here builds for
   Windows to see the flag working first. `pointer-drag.ts` starts past a 4 px tremble, captures the pointer, finds the
   target by `elementsFromPoint`, puts it back on `Esc` or a cancelled pointer, and swallows the release's click. No
   `draggable` is left in `app/src`.
2. **A click on a chip's word opens a `TfMenu` of the six columns**, the current one checked; a pick lands it, and focus
   goes to the word in its new column. Pick-up-then-click (a hidden mode) and *Reset* in the menu were refused.
3. **A stored count of the seen words** (migration `0096`), not `sync.item_card` (tickets only, no category, whole only
   where the fill says so) and not a per-process cache (every launch would pay the scan). The read never scans: a source
   with no count, or one counted under other declarations, waits for the one counter's count (bring-up asks for it in the
   background), and a run that wrote or a landed write asks for its source again, one count at a time. The old walk stays
   as the oracle a test holds the count to.
4. **The board draws every move at once** and applies each answer with the moves still waiting drawn over it, so a chip
   never steps back; only a word's newest target is sent.

Staleness, accepted: a status a sync brings shows the next time the section opens. An event that added chips would move
a drawn board.

## Before and after

Measured on Björn's PC (Ryzen 5 5600X) in Chrome inside WSL, idle, under the PC queue, against the windowless backend on
the saved scale database (20,007 Jira tickets) running on the Mac and tunnelled in: before, the branch at `af10d21d`;
after, this change. `testenv/perf/state-colours.mjs`, 3 reps a case, median (worst), ms, from the input to the frame that
draws the chip where it lands. The PC's lines: a page 100 / 200 / 300, a click or key 100 / 100 / 200 at 1x / 4x / 6x.

| 1440 x 868 | 1x | 4x | 6x |
|---|---|---|---|
| open the board, before | 1,995 (2,026) | 1,921 (1,955) | |
| open the board, after | 43 (48) | 72 (107) | 73 (138) |
| → on a word, before | 1,971, 1 of 3 never landed | 2,008, 2 of 3 never landed | |
| → on a word, after | 14 (15) | 11 (12) | 16 (17) |
| *Reset*, before | 1,905 (1,912) | 1,905 (1,912) | |
| *Reset*, after | 32 (32) | 31 (31) | 32 (33) |
| a pick in the menu (new) | 24 (24) | 31 (31) | 29 (29) |
| a pointer drag (new), release to drawn, 5 drags | 14 (15) | 9 (11) | 16 (23) |

| 2560 x 1440 at 125 %, after | 1x | 4x | 6x |
|---|---|---|---|
| open the board | 59 (91) | 55 (86) | 81 (84) |
| a pick in the menu | 21 (25) | 29 (32) | 23 (32) |
| a pointer drag | 13 (15) | 10 (10) | 14 (14) |
| → on a word | 8 (16) | 11 (12) | 16 (17) |
| *Reset* | 22 (33) | 32 (33) | 31 (33) |

No chip moved after the board drew, in any run. The backend's own time per command (`--timings`, the Mac): before, every
`state_colours` read 1.8 to 2.2 s and every set or reset 1.8 to 2.4 s; after, 1 to 9 ms and 2 to 9 ms. A press made
while a write was out on the old board was dropped: the "never landed" above. **Applied:** with *In Review* at its
default the launcher over Tickets draws PAY-228's row in the active tone; once *In Review* is set to *Error* by its menu,
in the error tone (`data-tone`).

A first real-Chrome run found the drag never started: a real pointer leaves a small chip on its first move, before any
capture, so the chip's own listeners heard nothing (jsdom had sent every event to the chip). The helper listens on the
window from the press (`d0b5c570`), and its tests send the moves where a real pointer would be.

## Other Settings sections

Every read the Settings ports make, timed warm on the scale database: 5 to 90 ms under a load of 25 (`db_stats`, Logs,
the slowest). The one slow first draw beside *State colours* was Backup's share ticks, which waited on the same read
and now draw with it. The one other change that couldn't take on Windows was the pipeline's step drag, fixed with the
same helper.
