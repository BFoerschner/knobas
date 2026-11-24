# A one-time code's type is made ahead, a picker's drag is the reader's input, and the laned drop's rest (2026-11-24)

The ux30 end gate's third WebView2 fix round, in knobas's own window on Björn's PC (5600X, about 170 Hz, the front end
of each build served into the window, `testenv/perf/win/`). Three overs were left after the second round: the Theme
editor's colour picker drag (its own section in `2026-11-21-a-theme-colours-the-whole-window.md`), Passwords at 2,000
entries with ↓ held at full screen and 6x, and card-drag's laned first drop at full screen and 4x.

## ↓ held in Passwords: the one-time code's type, made at its first use

The gate saw one frame of 61 and 60 ms (line 50, no long task) while ↓ was held through 600 rows at 5120 x 1369, 6x.
Traced in the window it is one frame, at the same row every run: the fifth `j`, the first entry with a one-time code.
Its key ran 17 ms of script where the others ran 9 (the code's component drawn for the first time: ten `ParseHTML`),
and the frame's layout took 26.6 ms where every other took 5 to 8, for 13 dirty objects of 569. The code is set in the
display face at weight 800 and display size; its first use made that instance of the face inside the layout.

The page now lays out a hidden sample in the code's type once it has drawn and the window is idle (`warm`,
`.tf-pw-warm`: absolute, no height, hidden, `aria-hidden`), so the face is made before anyone reaches a code. Tested:
the sample is there once the page is idle (fails before).

In the window, alternated, 5120 x 1369, ↓ held, worst frame: head 36 and 42 ms at 6x, 26 and 24 at 4x; with the sample
24 and 24 at 6x, 18 and 18 at 4x. The 61 ms frame the gate saw is the same frame on a slower moment. The Mac (headless
Chrome, 5120 x 1440): 17 ms at every rate on both.

A first idea, reading and drawing the entry pane once a frame instead of once a key, was measured and dropped: the
probe's keys, like a held key's repeats, land 1.15 to a frame, so there was nothing to fold.

On the way: a key that acts on the open entry (C, S, U, O, A, E) pressed while the entry just picked was still being
read acted on the pane, which still drew the entry before it, so S could show and C copy the previous entry's
password. Such a key now waits for the open entry's read and acts on it once its pane is drawn; another pick drops it.
Tested (fails before: the previous entry's password was shown).

## A colour picker's drag is the reader's input

The sections opened ahead of their press (`ahead.ts`) wait for a second with no key, press, pointer move, wheel or
scroll. A colour picker's drag happens in the system's own popup; all the page hears is an `input` a pick. So a page
built ahead could land in the middle of a drag, and it did in the probe: the Theme editor's drag at 4x on the Mac had
one frame of 50 to 117 ms, a major garbage collection (73 ms of it marking DOM wrappers) after other pages' rows were
built during the drag, while the DOM grew by about 110 nodes (11,530 to 11,640; head's stayed at 11,421 to 11,428,
whose drag left the window no idle time). A field's `input` is now the reader's input too. Tested with real timers:
inputs every half second keep the wait from ending, and it ends a second after the last (fails before).

## The laned first drop at full screen: measured, not fixed

In the window, alternated, three rounds, lanes at 5120 x 1369, 4x (`--hz 170`): head drop median 83.9, 78.8, 84.3 ms and
first drop 116.0, 114.1, 106.7; the variant below 84.7, 79.2, 94.0 and 111.2, 109.7, 121.0 (line 100). The median is
within; the first drop is over in every run.

Traced (4x, full screen), the first drop's task runs the pointer-up, the board's update and its forced work in one go:

- About 85 ms of script (traced), 44 of it the board's update.
- In it, about 25 ms of creating templates for the first time (Svelte's `innerHTML`): the card's write chip
  (`tf-wwords`) and its mark, the filter bar's held-order button (`tf-fheld`) and the toast's action button. Only the
  first drop has it; the second has none. That is most of the first drop's extra over later ones.
- Removing the focused card's old row blurs it, and Chromium works out style (216 elements, 12 ms) and layout (5 ms)
  there; focusing the moved card forces another layout (5 ms); after the release the pointer's boundary events force a
  third layout and a 14 ms PrePaint, then a 12 ms paint at 5120 px.

A variant that took focus off the card at a pointer's pickup removed the blur, but the layout moved into the focus
that follows (18 ms instead of 5 plus 5) and the numbers above did not move, so it was dropped. Owed: the template
creation on the first drop (warming those four parts ahead in idle time is the candidate), and the forced layouts,
which are the board's own size at 5120 px.
