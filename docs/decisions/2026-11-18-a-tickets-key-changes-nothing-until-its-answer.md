# 2026-11-18: A Tickets key changes nothing on screen until its answer draws

**Found in the ux19 Windows pass** (`key-frames.mjs`, knobas's own window at 5120 x 1369 on Björn's PC): in the frame
right after a letter typed into Tickets' filter, before any answer, 5 of the 48 cards on screen were swapped for others
in place, with the columns' counts still the empty board's. When the answer then differed from that frame, 6 to 21 of
its cards moved or went. The pass did not find what drew that frame.

## Why

The board's held order (`HeldOrder`, the still-rows pass: a card drawn keeps its place until the person acts) sorts
again when its *basis* changes, and the basis held the filter bar's own text, chips and dropdowns. So the key itself was
the act: in its own frame the held order was dropped and the board placed again from the board on screen, the one the
last read answered. A sync between keys (the pass ran one every 45 s) had brought cards that sort above drawn ones,
which wait behind *Show N new*, and cards that left, which stay drawn until the person acts. Placed again, the waiting
ones drew and the ones that left went: the swap. The answer then sorted the board a second time.

Two more things followed the bar and not the board: a *Done* column, which reads eight and draws three, drew all eight
in the key's frame because the bar had words (the fifty a column a typed filter draws), and a cleared filter cut every
column from fifty to eight before its answer came. And the cards' marks followed the typed text.

## Taken

- **The board reader keeps the filter its board was read with** (`drawnFilter`, set when a read's answer lands, the
  list's pages too). What the board draws by words (`typing`: fifty a column, the list's *Show N more* hidden) is that
  filter's, not the bar's.
- **The held order's basis is the drawn board's query** and the count of reads the person caused, not the bar: the
  board sorts again in the frame its answer draws, once.
- **The cards mark the drawn board's words.**

The read itself is unchanged: a key reads as soon as it is typed (or after the pause), with the bar's words.

## The check

`just moves` has a place for it, `tickets-key`: on the Tickets board, a letter typed into the filter as real key input.
The cards on screen before the key are held to their boxes until the columns' counts change (the answer's frame); one
that comes in, goes, moves or resizes before then is a move. After it, the place records for 5 s as every place does.
`just moves --only tickets-key --sizes "1440x868@1 5120x1440@1"`.

On the fixture (`?fake-scale`, every read 200 ms late), headless Chrome on the Mac, 1x and 4x: the old front end had 3
cards come in before the answer at 1440 x 868 and 5 at 5120 x 1440 (the *Done* column's), the new one nothing at either
size; nothing moved after the answer on either. The return and the 4x run answer from the answer cache within a frame, so
the first key at 1x is the one with a wait to hold.

## Tests

`board-reader.test.svelte.ts`: *a key draws nothing new until its answer lands* (the *Done* column keeps three, the
reader's `typing` and `drawnFilter` keep the last answer's, both ways). `TicketsPage.test.svelte.ts`: *a key changes
nothing drawn until its answer lands* (a sync brings a card that waits; the key's frames draw the same cards, unmarked,
until the answer, which draws it). Both fail on the old code.
