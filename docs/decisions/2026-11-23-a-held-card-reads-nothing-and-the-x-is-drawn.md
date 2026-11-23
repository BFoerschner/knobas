# 2026-11-23: A held card holds the board's reads back, and the ✕ is drawn

**Found by** the ux30 end gate's drag probe (`testenv/perf/card-drag.mjs`, ADR-0054, ticket-edit decisions section 14)
on the Mac: a frame while a card was carried took 83 ms at CPU 4x and 97 ms at 6x against the 34 ms line on a board as
it opens; the worst drop took 106 ms at 4x and 170 ms at 6x; the `→` of a carry once took 504 ms.

## What the traces showed

None of it was the drag's own work. A move cost 15 to 22 ms at 4x in every case. The long frames were work that landed
in the gesture:

- **The board read again under a held card.** The previous round's move lands (600 ms later in the fixture), and its
  refresh (`item:refreshed`) reads the board again: 65 ms at 4x in the fixture's `mini_board`, then the board drawn
  anew under the copy, its columns' boxes no longer the ones read at pickup.
- **The answer cache warmed in the middle of a drag.** It waits a second past the last key, press, wheel or scroll, and
  a drag is none of those: a card held past a second let it ask `ticket_views` (400 ms at 4x in the fixture) between
  two moves, or under the carry's first key.
- **The first drop drew the window's first ✕**, the toast's dismiss. Inter has no U+2715, so Blink searched the
  system's fonts for one: 49 ms of a 60 ms layout at 4x, once per window.

## What changed

1. **While a card is held, the board reads nothing.** A move's echo and a run's end ask the board again through the
   drag (`afterDrag`): at once when nothing is held; otherwise once, after the drop or the put back, 200 ms past its
   frame, so the drop's card, chip and focus draw first. The page going drops what it held back.
2. **A pointer moving with a button down is input** to the answer cache: its warm waits a second past the drag's end.
3. **A move writes only what changed**: the scroller's box is read with the columns' (at pickup and after a scroll),
   never on a move; the line's width, the reason's words and both their `hidden` are written when they change; a
   column's `data-drop` and its head's statuses only where the answer that lands mid-drag changes them.
4. **The hand is a cover, not a style of the board.** `.tf-board-wrap[data-held]` set the cursor and `user-select`
   on the board, and both are inherited: the pickup and the drop each restyled every element of it, 2,530 on a board
   of 221 cards, 31 ms of each at 4x. The grabbing hand (and the refusing one over a column that will not take the
   card) is now a cover over the window in the float host, under the copy, for the pointer's drag only, and the wheel
   over it still scrolls the board. Nothing in the styles reads `data-held` now.
5. **A drop hands every other column the array it already draws.** The held board was regrouped into new arrays for
   every column on each drop, and each column's stack windowed its cards again: 66 of them on a laned board. A column
   whose cards are the same cards in the same order keeps its array (`keepColumnArrays`), behind a `{@const}` of its
   own, so its stack hears nothing.
6. **The ✕ is drawn** (Fable's ruling, below): `TfXIcon`, one inline SVG in all seven places that drew U+2715 (the
   toasts, the filter box's and the assets search's clear, both find bars, the view editor's removes, a pull request's
   failed check). The glyph's box, so no button changes size: .762 em wide (the glyph's advance) and .7 em tall (its
   ink), on the baseline; two strokes in the text's colour with round ends, a tenth of an em thick, a little heavier
   where the text is bold (a toast's button, a check's mark).

## Decided by Fable (xhigh) while Björn was away

The first drop's font search, with four ways out (leave it, since the line is on the median; warm the search at
startup; draw the ✕; use Inter's ×): **draw it**. Leaving it leaves a 102 ms first drop on a person's first action,
slower on Windows; warming only moves the search and breaks if the warmed font differs; × reads weaker as a target.
Also: the probe holds the first drop after a page opens to the drop's line, so the median cannot hide it again; and the
app was swept for other characters Inter lacks.

## The sweep, and what is left

Inter's files cover Latin, Cyrillic, Greek and Vietnamese and the general punctuation block; every other character a
component draws costs the same search the first time a window draws it. Besides ✕ the app draws arrows (← → ↵ ⇄ ⇧),
the Mac's key signs (⌘ ⌃ ⌥ ⌫ ⏎), ✓, ●, ▶ ▸ ▾, ⋯, ≤, ⟨ ⟩, box drawing in Settings › Fonts and two CJK characters in the
languages list. ⌘ is on the shell's first frame (the top bar), so its search is part of the first paint. None of them is
in the drag's path; they are left for a pass of their own (draw them, or bring a face that has them).

## The probe

`card-drag.mjs` failed on the plain fixture: each round moves two cards out of To Do, and its To Do holds six. A round
now takes the first card of To Do, then In Progress, In Review and Done, and drops on In Progress or the first column of
its lane that takes it with one status. Its cases cover the sizes Björn's design named: `small` (the plain fixture, 20
cards), `forty` (42 at 1440 x 868, 47 at 2560 x 1440), `open` (87), `typed` (91, 176), `full` (94, 159) and `lanes`
(221 in lanes by type). Its `typed` case never typed (the filter box's selector matched nothing), and now does.
`--attach <port>` drives knobas's own WebView2 window, and `--trace <prefix>` traces the first two rounds with the CPU
profile.

## In knobas's own window on Windows (2026-11-23, the ux30 end gate)

WebView2 157 on Björn's PC, the probes' build served into a release window (`?fake-ipc` there since de1a3d30), the glide
at the display's rate (`--hz 170`; the window idles at 170 to 176 Hz). Every case met its lines at 1440 x 868 and full
screen (5120 x 1369) except `lanes` (221 cards at 1440, 296 full) at 4x: drop medians 103 to 126 ms and first drops 126
to 153 ms against 100, and at 6x first drops 207 to 238 ms against 200 in some runs. Moves while held stayed at 11 to
22 ms at 4x on the board as it opens (line 34).

A trace of a laned drop at 4x: the release's task runs the board's whole read (the overlay, the bands, the held places
of every card, the columns' card arrays and windows) twice before its frame, once for the move and once more in a
microtask after it, with a style and layout forced inside the first. Found and fixed here: a move's write answering
replaced its echo with one carrying the write's id, and the board was laid over its echoes again for a status that had
not changed. The overlay now reads the moved statuses and field edits from values that keep their identity while they
draw the same (`TicketsPage.svelte`, held by a test). In the app, where the answer lands in a task of its own, that pass
was a second frame's work after the drop. In the window the second pass in the drop's own task remains: its trigger
lands with the focus the drop gives the moved card, and is not yet named. Owed: name it, and take the forced layout out
of the first pass.
