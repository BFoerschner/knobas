# 2026-11-18: A file picked in a diff's list builds the screen it lands on, and nothing else in that frame

**Asked (through the orchestrator, for ux19):** a jump to a file picked in Compare's file list (and in *Files changed*)
took 89 to 171 ms at 4x and up to 296 at 6x on the Mac at the full-screen sizes, and 238 to 294 / 406 to 478 ms on
Björn's PC. Make its first frame build only the picked file's lines on screen and the room around it, the neighbours
after, nothing moving, within 100 / 100 / 200 ms on the Mac at 1x / 4x / 6x at the three sizes and close to it on the
PC. The ux18 gate added that the jump cost more than on ux17 (`9f0e236f`): two long tasks at 4x where ux17 had one,
each with forced layouts of 9 to 12 ms from the windowed lists, after the landing fix of 2026-11-17
(`2026-11-17-a-file-picked-lands-under-the-band.md`). Björn's rule: directness, performance and responsiveness.

## How it was found

`compare-moves.mjs` on `big-compare.mjs`'s cloned repository on the live Gitea (`jump-mid`, `jump-last`, the eleven
`jump-small` files among files of 400 changed lines, `pr-jump`, `pr-jump-small`), with Chrome traces and the V8 sampler
of each jump on an unminified build, so the profile names the functions. Then what each task of the jump built.

## Why a jump was slow

1. **Two screens built, one thrown away.** The pick called the file list's `reveal`, which scrolls a row only just into
   view: a card below the view came in at its foot. The list then drew the files around that place with its 600 px of
   overscan above and below, with their lines. The landing then moved the view a screen down to put the card under the
   band, and the list drew the files there as well and tore down the ones above. The landing's scroll also had every
   windowed list read the page again (`viewReads`), with the page half built: the forced layouts the gate saw.
2. **A big neighbour's lines, drawn twice.** A file's lines are a windowed list of their own. One the jump brings had
   not read where it sits yet, so it drew its first 600 px of rows from its top: the wrong rows for a card whose foot is
   what shows. Its read then drew the rows on screen and 600 px of overscan above them.
3. **Every changed pair of a file matched at once.** A card works out the changed words of each removed and added pair
   as it mounts: a 400-line file's 200 pairs, 5 to 6 ms of the jump at 4x, for a screen of lines.
4. **Twenty effects a line.** A line's code was a snippet: an `each` over its pieces, an `if` per piece, a snippet per
   token. A line of a dozen pieces made some twenty effects to build and as many to tear down, and a jump does both, for
   the screen it lands on and the screen it leaves.
5. **At the list's end**, the last files cannot reach the band (the scroll ends first). Each landing and each of the
   page's scrolls asked for a scroll the end refused, and each such ask made every list read the view again.

## Taken

- **`land`** (`windowed.svelte.ts`): the list scrolls straight to where row `index`'s top meets the band's foot and,
  until two frames have drawn after it, draws only the rows on screen under the band (and always the row landed on).
  The overscan both ways comes after that, in a task of its own (`growSoon`), held still by the landed card
  (`landOn`'s anchor, as before). `TfDiffView`'s `go` uses it for a card not drawn yet; `n`, `p` and `c` still use
  `reveal`. A reader's scroll before the overscan comes brings it at once.
- **Lists born in a jump** (`#bornIn`): a windowed list attached while a jump lands draws no row until it has read
  where it sits (it holds their room), then the rows on screen alone, and grows with the jump.
- **Changed words as drawn** (`changedWordsOf`): the pairs are found at once, their words matched when one of their
  lines is drawn.
- **A line's code in one effect** (`TfDiffCard`'s `code` attachment): its sign, tokens and changed words built as text
  nodes and spans, the same elements as before (`.tf-hl-<kind>`, `.tf-wd` around it), never markup.
- **No scroll the end refuses** (`reach`): a landing and a re-landing ask only for the scroll the scroller has room for.

The landing's rules stay: the head at the band's foot within 1 px, held until the reader touches the page, marked in the
list and on its card. The jumps' `offBand`, `drift` and `marked` were within their lines in every run below.

## Before and after

Press to the painted frame that shows the picked file at the band's foot with its lines, the range over all fourteen
jump cases (`jump-mid`, `jump-last`, eleven `jump-small`, `pr-jump`, two `pr-jump-small`), and the longest task among
them. *Before* is `ecc33edd`'s front end, *after* this pass's, on one windowless backend (the Mac's, release with debug
assertions, a fresh copy of the saved scale home), alternated.

**The Mac**, headless Chrome under its queue (load 4 to 8):

| Size | Throttle | Before | After |
|---|---|---|---|
| 1440x868@1 | 1x | 23 to 44 (longest task 0) | 9 to 32 (0) |
| 1440x868@1 | 4x | 73 to 120 (72) | 30 to 109 (78) |
| 1440x868@1 | 6x | 111 to 188 (107) | 48 to 125 (71) |
| 2560x1440@1.25 | 4x | 92 to 166 (80) | 37 to 88 (74) |
| 2560x1440@1.25 | 6x | 154 to 376 (163) | 51 to 177 (91) |
| 5120x1440@1 | 4x | 84 to 132 (70) | 44 to 98 (53) |
| 5120x1440@1 | 6x | 125 to 199 (101) | 54 to 180 (92) |

The one 4x over, `zz-mix/d-two.ts` at 1440x868 (109 ms), took 68 ms in the traced run after it. The middle file at 4x
now lands with no long task at all at the three sizes. ux17's front end had a 97 ms task for it, and 60 + 79 ms
for the last file, on the same backend. (Its picks never pass the probe's test since the landing pass, a head within
2 px of the band's foot, so only its tasks compare.)

**Björn's PC**, a headless Edge 157 of the run's own on Windows from a portable Node, the page served on Windows
loopback, the Mac's backend forwarded through the WSL box, under the PC's queue:

| Size | Throttle | Before | After |
|---|---|---|---|
| 1440x868@1 | 1x | 23 to 32 (0) | 8 to 23 (0) |
| 1440x868@1 | 4x | 117 to 187 (104) | 36 to 127 (82) |
| 1440x868@1 | 6x | 192 to 284 (141) | 59 to 267 (135) |
| 2560x1440@1.25 | 4x | 142 to 218 (123) | 51 to 126 (74) |
| 2560x1440@1.25 | 6x | 225 to 432 (217) | 86 to 216 (139) |
| 5120x1440@1 | 4x | 135 to 216 (104) | 51 to 129 (90) |
| 5120x1440@1 | 6x | 242 to 341 (178) | 84 to 244 (181) |

The 1440x868 6x highs (`f-two` 229, `j-logo` 267 ms) were above their *before* in that run only; the small files run
again alone, alternated (after, before, after), gave 43 to 115 against 121 to 159 at 4x and 69 to 206 against 196 to 253
at 6x.

## What still waits

- **The PC at 4x and 6x**: the jumps next to a 400-line file and those at the list's end are 100 to 129 ms at 4x and up
  to 244 at 6x. What is left in their frame is the screen of lines itself, the cards the page left torn down, and the
  page's layout. Not dropping the old cards in the jump's frame would need a second list of cards, which this pass did
  not take.
- **The overscan's task after the landing** builds the files above and below: 50 to 80 ms at 4x on the Mac, 100 to 180
  at 6x on the PC. A press in that time waits for it.
- **In WebView2** (knobas's own window on Windows): this pass measured Edge, not the window.

## Gates

On the final code, in a clean clone, with the trimmed gates: `just check-front` passed (svelte-check 1,556 files, 0
errors; vitest 372 files, 5,653 passed, 1 skipped); `just check-scripts` passed; the press trips with `--strict` 305 of
305 and `testenv/desktop-witness-test.sh` 305 checks, under the Mac's queue; `just moves --only repo-commit,pull-request`
(a commit's diff, a pull request and its three tabs, 1x and 4x, at 1440x868@1 and 2560x1440@1.25) moved nothing.
`pixel-same.py` not run: what a line draws is the same elements as before, and nothing else drawn changes. The timing
is this pass's own cases above.
