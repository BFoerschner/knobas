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
`press-reads.test.ts`, and the board's "the cards the window's height holds and one more". The Windows pass after them is
owed: Björn took the PC back (the evening block) before the head's build with these changes could be measured in the
window. Owed there: `open.mjs --attach` (rounds first, return, late) at 1440x868 and full screen at 1x, 4x and 6x,
every place, and `keys.mjs --attach` at the same sizes and rates, on a Windows build that includes 483d78a8.
