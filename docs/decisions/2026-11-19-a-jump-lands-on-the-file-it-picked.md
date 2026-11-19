# 2026-11-19: A jump lands on the file it picked, not a file's height above it; Tickets' list lays out once to open

**Asked (the ux20 Windows pass, for ux21):** a jump to a file in *Files changed* and Compare was over the lines at 4x
and 6x on Björn's PC: small files up to 141 to 160 ms at 4x and 216 to 264 at 6x at full screen, the last file 137 to
150 / 236 to 263, the press's own task 87 ms in *Files changed* and 116 in Compare at 4x, "mostly forced layout of the
diff cards". And the show-every-ticket list's first open at 1x, 105 and 106 ms against 100 (ux19 100 / 93).

## How it was found

`mjump.mjs` (the Windows pass's `jump19.mjs` for a headless Chrome of its own) on a scratch repository on the live
Gitea, `main...jump`: 40 files, 30 grown by 1,500 lines and 10 changed by a line, in Compare (not cloned, Gitea's
compare) and in its pull request's *Files changed*. Each pick a real press on its row in the list, timed to the painted
frame with the file's head at the band's foot and its lines drawn; a 1x round first, as the Windows pass ran its
throttles one after another in one window. Then, for the slow picks, a log each frame of the cards drawn, the scroll,
the list's room, and every write to the scroller's `scrollTop` with who wrote it.

## Why a jump was slow

Not the layout of the cards in itself: the slow jumps were jumps to the wrong place, put right a frame later.

1. `land` scrolls to the picked file's top under the band. A file's top is fractional (its room is its lines', on the
   device's grid); a scroller holds whole pixels. The view's top fell 0.4 px above the file's top, and the window drew the
   file above it for that sliver, a file of 1,500 lines, under the band where nobody sees it.
2. That file's lines are a windowed list attached in the jump. Until it reads where it sits it holds no room for its
   undrawn rows, so its card was its first screen of lines tall: 654 px for a card of 31,616.
3. `go` then measured the picked card in that layout, found it 30,969 px above the band's foot, and scrolled there to
   put it right: the view now showed the big file, which drew a screen of its lines and tore down the screen it left,
   and the landing put the picked card back once that file's lines held their room: one task of 250 to 420 ms at 6x and
   a landing at 430 to 680 ms. Which picks did it turned on each file's fraction of a pixel, so the same run was fast on
   one small file and slow on the next.

Next to it, the lines each card brought read their styles after the list before had written its room, so each list's
read laid the page out again (`clipsOf`, 39 ms of a sixth-speed main thread in one jump).

## Taken

- **No row above the landed one in a landing's first frames** (`windowed.svelte.ts`, `#span`): while the landed row's top
  stands at the band's foot (within a pixel), the rows above it lie under the band and are not drawn.
- **A pick measures its card once the lines it brought hold their room** (`TfDiffView`'s `cardFor`): `placeWindowsNow()`
  after the cards are drawn, so the read of where every new list sits and the card's measure share one layout.
- **Every list's styles first, then every list's room, then every list's place** (`placeWindowsNow`, three phases): no
  list reads a style after another wrote.
- **Tickets' list reads no column width** (`windowed.svelte.ts`, `list`): a fixed table with a header row has its columns
  from its header, whichever rows draw. Reading their widths as the list began to window laid the page out in the task
  that opens it, and writing them back laid it out again; `tickets.css` gives the list `table-layout: fixed` with each
  column's width already.

## Before and after

A headless Chrome on the WSL box at Björn's PC (Linux, not WebView2), the windowless backend on the Mac on a copy of the
saved scale home through the box's tunnels, under the PC's queue. `df4e3a6e`'s front end against this pass's, alternated.
Press to the landed frame, ms, the small files and the last file, and the longest task of the round.

| Page, size | Throttle | Before: small / last / longest | After |
|---|---|---|---|
| Compare, 5120x1440 | 4x | 44 to 422 / 139 / 247 | 45 to 64 / 64 / 67 |
| Compare, 5120x1440 | 6x | 64 to 557 / 190 / 311 | 63 to 74 / 82 / 83 |
| *Files changed*, 5120x1440 | 4x | 41 to 449 / 119 / 238 | 48 to 74 / 79 / 77 |
| *Files changed*, 5120x1440 | 6x | 64 to 617 / 184 / 328 | 63 to 78 / 93 / 86 |
| Compare, 1440x868 | 4x | 34 to 112 / 88 / 72 | 33 to 47 / 48 / 55 |
| Compare, 1440x868 | 6x | 48 to 157 / 139 / 101 | 48 to 57 / 64 / 75 |
| *Files changed*, 1440x868 | 4x | 33 to 107 / 90 / 78 | 35 to 49 / 48 / 56 |
| *Files changed*, 1440x868 | 6x | 50 to 182 / 131 / 114 | 49 to 57 / 66 / 69 |

At 1x the jumps were 9 to 87 ms before and 9 to 46 after. Every pick lands under the band and stays (its head 0 or -1 px from the band's
foot at 0.2, 1 and 2.5 s; the last file where the scroll ends). One 1x pick in one round stood 69 px under the band at
0.2 s and was back by 1 s; three more rounds of both builds did not show it again.

**The show-every-ticket list's first open at 1x**, 1440x868, the same setup, alternated: before 129.5 / 123.8 ms useful,
after 98.5 / 91.6. Its rows now draw 4 to 5 ms after the read answers; the read itself crosses the tunnel to the Mac's
backend, which knobas's own window does not.

## A repository's first open

Traced at 4x, 5120x1440 (`moment.mjs`, `profile.mjs`): the press's own task is 89 ms, of it 71 ms of script spread over
building the page (no one function above 4 ms of self time), and one forced style pass and layout, 17 ms, from the
shell setting its scroller to the top for a page drawn the first time (`TfShell`'s `settlePlace`), a scroller that stood
at 0 already. It now sets the scroll only where the page it came from was left anywhere else (`notePlace` read that as
the page was left, laid out then). Opened from Code by its row (`open.mjs --only repo`, first, warm reads, alternated):
the longest task 87 to 94 ms before against 71 to 81 after at 4x, 125 to 154 against 109 to 126 at 6x, at 5120x1440 and
1440x868. The time to useful moved with the backend's reads through the tunnel (12 to 16 calls before useful), not
with the page: 69 to 86 / 129 to 178 / 203 to 333 ms before, 68 to 153 / 128 to 190 / 188 to 304 after at 1x / 4x / 6x.
What is left of the 4x first open is that script and the reads it waits on.

## Owed on Windows

These are a Linux Chrome's numbers; the jumps and the list want the next build's Windows pass in knobas's window.
