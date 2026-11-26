# The Windows first opens, third pass: the board, the repository's gap, reads at the press (2026-11-23)

After the second pass (`2026-11-23-windows-first-opens-second-pass.md`) three first opens were still over the PC's line
(300 ms at CPU 6x) in knobas's own WebView2 window on Björn's PC: a repository (383 and 385), the board of every ticket
at full screen (320) and a ticket at 1440 (308). Fable's ruling ordered the rest: the board's landing share, the
repository's gap between the full run and opening it alone, the pages' reads at the press, and only then an on-screen
part deferred.

## What the traces showed

Traces of each first open at 6x in the window (`Tracing` over the DevTools protocol, the CPU profile mapped through a
source-mapped build of the same commit; the build's code is byte for byte the shipped build's), on a ux29-era scale
profile, offline, the head's own Windows build. Tracing slows the page about a third; the numbers below are traced.

- **The repository.** Its press's reads left at `pointerdown`, 48 ms before the click, and `get_entity`'s answer was
  back 29 ms after it; but every answer's body is handled on the main thread, and the page's build held it for 162 ms
  (52 ms of that the first parse of its templates) and its first frame 57 ms more. Only then did the header's pull
  requests go out: their read is built from the row `get_entity` answers, so it was a second round, 140 ms more. The
  header is held until its pull requests and its checkout have answered (`held-head.svelte.ts`), and the page asked the
  checkout once it was built.
- **The ticket.** The page found its ticket in three reads in a row (`list_sources`, `entity_briefs` over the key's
  candidates, `get_entity`), the first asked inside its 102 ms build, the other two after its first frame: the card drew
  210 ms after the press, then built for 111 ms, and its frame laid out for 66.
- **The board of every ticket at full screen.** Its read went out 64 ms into its 84 ms build; once its answer was in,
  the 64 cards built in 73 ms and the windowed columns' first read of where they sit laid out the page in 81 ms (most of
  it shaping the cards' text), then the frame painted. Eight cards a column is what a 1369 px window shows, so the
  floor-count share draws eight there too, as Fable expected.
- **The repository's gap (step 4).** Not found. Opened alone, the repository's first open was 333 ms (the head's build)
  and 378 ms (ux33's exe, the head's front end served into it), in the range of the full run's 383. Its time is its own
  chain above, not hidden kept pages working: a fresh page holds none.

## What changed

- **The board's landing share** (step 3, `TicketsPage.svelte`'s `landingShare`): in the frame a show-every-ticket board
  lands, a column draws the cards the window's height holds by `cardFloor` and one more, at most eight; a column wholly
  past the window's right edge draws one. The rest draws the frame after, below them, as before.
- **A repository's press asks its header's last two reads** (step 5, `hover-reads.ts`): its pull requests, with the
  page's own filter (`repoPrsFilter`) from the row the Code list holds (`heldRow` in the answer cache), and its checkout
  (`readCheckoutAhead`), which the page's first `entityCheckout` takes.
- **A ticket's press asks its page's reads** (step 5, `job/press-reads.ts`): the sources, then the key's candidates'
  briefs, the ticket, its card's sections and its sub-tasks, each with the page's arguments (`jobCandidates`,
  `cardFilters`), at `pointerdown` or `Enter` on a card or row; the page finds them in the answer cache.

- **A repository's page and its tabs share the window's listeners** (`ipc/shared-listen.ts`'s `windowListen`, which
  Builds' pages already used): its parts heard the same events with a listener each, nine listen messages in the task
  that built it.
- **The landing share reckons the height below the board's top, and a column past the window's right edge from where
  the board starts** (Fable's second ruling, step 3 finished), both as the scroller's observer reads them after layout:
  counted from the window's top and left edges, a 1440 window's columns, the ones past its right edge too, still drew
  seven or eight cards each.

Nothing on screen draws differently: the same parts draw in the same frames, the reads they wait for are on their way
sooner. No on-screen part is deferred, so Björn's "drawn whole" reads as it did. A landing that waited one frame more
before the rest of its columns (a changed reading) was tried and bought nothing, since at 1440 the rest already drew
after the drawn frame; it was taken out. The board's own read sent at the address change (step 3 of Fable's second
ruling) waits: its arguments come from the view's filter as the page builds it from its suggestions, and a copy made
ahead that differed would read the board twice.

## Proof and what is owed

Each change has a test that failed before it: `hover-reads.press.test.ts` (the pull requests and the checkout),
`press-reads.test.ts`, `shared-listen.test.ts` (one listener for the window), `fbar-height.test.ts` (the board's top and
left heard after layout), `tickets.test.ts`'s landing count, and the board's "the cards the window's height holds and
one more".

In the window, 6x, drawn, base is round two's head (9385d922):

- **Repository:** 1440 went from 304 and 344 to 232 to 293; full screen from 318 and 343 to 232 to 296. One 1440 gate
  run read 313, and one full-screen run that started at load 2.8 read 357. The page's build varies from run to run (its
  longest task 66 to 125 ms), and about 50 ms of it is first-use template parsing.
- **Ticket:** 1440 went from 292 to 260 to 278; full screen from 295 and 302 to 257 to 278.
- **Board of every ticket:** 1440 stayed at 300 to 334 (base 305 and 306); full screen read 294 to 361 (base 288 and
  298). It did not move: the answer's task builds the landing cards and lays them out (about 65 ms of text shaping at 6x).

The gate shape on the final Windows build (0cba19d2, the backend of ux-polish at the time, the front end before
a4d99f06, 955a4968, f00d8c05 and c44bf4e5), at 1440 and full screen:

- **Opens at 1x:** 0 over at 1440; at full screen 1 over, the ticket list's first open (104.6).
- **Opens at 6x:** 2 over at 1440 (the board 316, the repository 313). At full screen, 4 over in the quiet run (a docs
  page 309, a run from a repository's Builds tab 301, Monitors 322, the long asset column 317). In each run a different
  two to four places sat 1 to 7 % over.
- **Opens at 4x, before 4x was dropped:** 0 over at 1440; 3 over at full screen (the board 224, the ticket list 216,
  the long asset column 207).
- **Keys at 6x, against 150:** every median within at both sizes; 1 press of 189 over at 1440 and 4 of 189 at full
  screen (152 to 176), each a case's slowest of seven.

Round two's head was over the same way on Builds (302 to 322), the run from a repository (317 and 334) and the long
asset column (307 and 312) in this pass's comparison: those predate these changes. Builds' first open is the style and
layout of its page as it is shown, about 270 ms traced.

Not run: the keys at 1x at both sizes. Björn stopped the end gate; they go to the nightly. Fable's second ruling holds
the rest: the board's own read at the address change, then a landing cap of seven, then six, named as a changed
reading, only if the board is over on its own.
