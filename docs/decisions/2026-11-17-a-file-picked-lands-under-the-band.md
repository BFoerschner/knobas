# 2026-11-17: A file picked in a diff's list lands under the band, and stays there

**Asked (Björn, on PR #2479, v3.2.9-ux17):** "In the compare view it doesn't scroll to some smaller diffed files for some
reason." His rule: directness, performance and responsiveness over data scarcity; a click answers within 100 / 100 /
200 ms at 1x / 4x / 6x, and nothing moves once drawn.

## How it was found

Compare on the fixture with its list replaced by a mixed one (180 files: files of 240 and 900 lines with, between them,
files of one, two and three lines, an added one, a pure rename, a binary file, a deleted one, a lock file, and three small
files at the list's end; each file's lines read 40 ms after it is asked), in headless Chrome inside WSL on Björn's PC.
Each file picked in the file list after the page was scrolled to the diff's top; where its head stood from the band's
foot, frame by frame for 1.5 s, which file showed under the band, and which the file list marked.

On the head before this pass every one of 21 picks missed:

- **Under the band.** The card was scrolled to the scroller's top, under the band that sticks there. A file's head is
  sticky under the band, but only within its own card: a card of a line or two cannot push its head out from under the
  band (a one-line file showed 26 px of its 44 px head), and a rename, a binary or a collapsed file showed nothing of
  it; what showed was the next file's head. The file list, reading the scroller's top, marked the file before.
- **Pushed away.** The windowed lists hold the topmost row on screen through each redraw. After the jump that was the
  file before the picked one, still showing under the band; its lines arriving grew it by thousands of pixels, and the
  picked file went with them: 4,400 px below the view after a file of 240 lines, 18,200 px after one of 900. Every
  small file, rename or binary file right after a big one was lost this way.
- **Nothing at all.** A card drawn once (near an earlier pick) and taken out by the window since leaves its binding
  `null`, not `undefined`; the view took it for drawn, brought nothing into the window and scrolled to nothing. A pick
  of any file that had been near the view before did nothing.
- **Cut back at the end.** Near the list's end the lines of the files around the pick come in as windowed lists. One
  placed after another list's read had laid the page out held no room yet for its rows past the first screen: the page
  was 5,300 px shorter for that layout, the browser cut the scroll back to the new end, and when the room came in the
  scroll stayed where it was cut. The last files landed a screen or more below where they were put.

## Taken

- **The landing** (`TfDiffView`'s `go`, `windowed.svelte.ts`'s `landOn`): the picked card's top goes to the band's foot,
  and that card is the row every windowed list holds through the redraws after it; every list reads the view it now
  shows in the same task. The file list and the `j`/`k` keys read the current file from the band's foot. A pick of a
  file whose card is not drawn draws it first, whatever its binding says.
- **Room before reads** (`placeWindowsNow`): the lists attached in one update all write their rows' room before any of
  them reads the page.
- **Held until touched** (`TfDiffView`'s `landed`): until the reader touches the page (a wheel, a press, a touch, a
  key), a scroll that is not the lists' own, or the files growing or shrinking around it (their lines coming in), puts
  the picked file back at the band's foot after the layout that moved it, before that frame is painted, drawing its card
  again first if the window took it out. *Back to top* and the next pick let go of it. A probe that moves the page
  itself sends a `wheel` first, as a reader's scroll starts.

Not taken: `scroll-margin-top` on the cards, which would have answered the band but not the three other causes.

## Before and after

**The fixture's mixed compare** (WSL Chrome on Björn's PC, 1440x868@1, 1x): 21 picks of 21 missed before (head under
the band, the file pushed 4,400 to 18,200 px away, or nothing drawn); after, 21 of 21 land with the head within 1 px of
the band's foot, marked, in 2 to 180 ms (the rest of the 180 ms wait on the fixture's 40 ms reads).

**`just open-budget --real --moves`' jumps** on the live Gitea's big compare with a clone (`compare-moves.mjs --only
jump,pr`, the Mac under its queue at load 3 to 10, the windowless backend built with debug assertions). The base build
never landed `jump-mid` at all, under the old probe's looser test of a head anywhere in the 120 px below the scroller's
top. After, every jump at every size lands within 1 px of the band's foot (the end files where the scroll's end stops
them, in view), moves at most 1 px after, and is marked; ms from the press to the painted frame that shows it with its
lines:

| Size | 1x | 4x | 6x |
|---|---|---|---|
| 1440x868@1 | 27 to 52 | 89 to 154 | 132 to 194 |
| 2560x1440@1.25 | 24 to 39 | 98 to 171 | 174 to 275 |
| 5120x1440@1 | 23 to 47 | 91 to 164 | 149 to 296 |

The cost of a jump is the cards it builds with their lines, as before: the frame that draws the picked card with its
lines came at 83 to 89 ms at 4x on this build and 86 to 96 on the base, alternated (it lands under the band in that
frame where the cards above it hold still, and a frame later, 56 to 76 ms at 4x, where their lines come in with it;
the profile is layout, 30 ms of it forced by reading where the cards stand). The probe now judges a frame as painted
(right after it), where it judged it in the frame's callback before layout; that adds the frame's layout and paint to
every move it times. Over the lines: 4x at all three sizes (up to 54 ms over at 1440, 71 at the full-screen sizes) and
6x at the two full-screen sizes (up to 96 ms over). The base was compared at 1440x868 only (above); the jumps' 4x and
6x need their own pass on the cards a jump builds.

What still waits: the same measure on Björn's PC (WebView2), owed with the next Windows pass.

## Gates

TBD
