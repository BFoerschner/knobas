# 2026-11-20: A diagram keeps its room

**Asked (#2510, the ux21 README-image pass's leftover, for ux26):** a Mermaid fence in Markdown showed its source until
the engine answered, then a frame sized to its drawing took the source's place at another height. On the fixture's
`payout-service` README the fences under its two diagrams moved 53 px about 400 ms after the text, on a first open and on
a return, at 2560x1440@1.25 and 5120x1440@1. Björn handed the open question to Fable: "let a fable agent decide on
what's best for ux".

## The ruling (Fable, 2026-11-20)

**A diagram's drawing that lands after the page under it has drawn stays in the room its source had: it is drawn whole
inside that room, scaled down where it is larger, and takes its own size only when the reader presses *Show at full
size* or the drawing itself. From its second open on, a diagram stands at full size in a room of its own from the first
frame.**

One refinement came out of the first screenshots. A tall D2 diagram of a four-line source stood whole at 14%, a speck
nobody could read as a diagram. So a drawing that whole would be under 40% is not scaled: its top stands in the room at
its own size, fades out at the room's foot, and the button reads *Show all*. Down to 40% the whole drawing still reads
as a picture (the fixture's sequence diagram stands at 42%).

The three ways out, and why this one:

- **It moves once** (the page under it shows, then jumps when the drawing lands). No press needed, but it breaks the one
  rule every page is held to (`2026-11-17-nothing-moves-once-drawn.md`), and it breaks it at the worst moment. A late
  drawing means a large diagram, a cold engine or a slow machine, so the reader has had time to start reading under it
  or to aim at a link. A top-down flowchart is often two to four times its source's height, so the jump is hundreds of
  pixels.
- **A room of a guessed height.** A drawing's height comes out of the engine's own layout (text measured, a graph laid
  out); a guess by kind and line count is tens of pixels off for a flowchart, so the drawing has to be fitted into the
  guess anyway, and a room that is neither the source's nor the drawing's shows the source cut or padded meanwhile.
- **The source's room, the drawing fitted into it, full size on a press** (taken). Nothing moves. What it costs is one
  press, on the first open of a diagram only, and only where its drawing was late *and* something in view stands under
  it. A reader gets the whole picture at once, small; a drawing that fits its source's room (most LR flowcharts) is at
  its own size with nothing to press.

What makes the late case rare, so the press is too:

1. **A diagram seen before never asks.** Its drawn size is kept between sessions (`render/diagram-sizes.ts`,
   `localStorage` `knobas.diagram-sizes`, the latest 400), by engine and engine version, text scale and a digest of
   its source. It stands as an empty frame of that size from the frame its text draws in, and the drawing lands in
   that frame. A drawing made earlier in the session is kept whole (the latest 64, at most about 12 MB) and drawn in
   the text's own task, so a return asks no engine and Mermaid costs the main thread nothing.
2. **Mermaid is warm before a README needs it.** While the app is idle after start it loads Mermaid and draws one small
   flowchart and one small sequence diagram, one an idle moment, thrown away (`warmDiagrams`). D2 and Graphviz stay cold
   until their first diagram: megabytes of WASM few READMEs ask for.
3. **A warm engine gets the text's hold.** For a diagram never seen whose engine has answered in this session, what
   stands from its fence down is held unseen until its drawing is in, at most the 300 ms an image of unknown size gets
   (`TfMarkdown`, `IMAGE_HOLD_MS`); the text above reads at once. A drawing that lands inside the hold has nothing drawn
   under it and takes its own size. A cold engine cannot answer inside 300 ms, so nothing is held for it: the page shows
   at once with the diagram's source.
4. **Where nothing seen stands under it, a late drawing takes its own size at once.** `drawDiagram` looks at what
   comes after the fence, and after everything the fence is inside of, up to the box that scrolls them. If none of it
   is painted in the window below the fence's bottom, in its column, the fence may change height. That covers a fence
   still held unseen with the text under it, a page kept hidden, the last block of a page, and every diagram whose
   followers are below the fold, which a first open at 1440x868 mostly is. By the time the reader scrolls to such a
   diagram it is drawn and its size is kept.

The room is the same rule in every case: the height a fence's body has when the page under it first shows is the height
it keeps until the reader does something. A kept size that proves wrong (a font gone, two sources of one digest) is
handled as a late drawing is, fitted into the room it was given, and set right for the next open. A parser's error that
lands late scrolls inside the room; one that lands inside the hold shows whole. A theme switch draws again in the room
the drawing has.

**A text drawn again keeps its rooms.** A page that reads again (a return to a kept page, a sync's end) hands
`TfMarkdown` the same text, and it builds the text anew. The probe showed what that did: on a return the base dropped
both drawings to source and drew them again, 261 px down and up, and the first cut of this change turned a fitted
drawing into a full one, 261 px down. Now each fence in the same place with the same code gets the room it was kept at
(`keptFenceRooms`, `keepFenceRooms`), and the session's drawing goes into it in the text's own task: a fitted diagram
comes back fitted, one the reader enlarged comes back large, and nothing under either moves.

## The keys, against the earlier order

The earlier order named four keys: source, Mermaid version, theme, and the width the SVG scales at. Two came out
otherwise.

- **The width.** knobas draws every diagram at its drawing's own size whatever the column's width (a wider one scrolls
  its holder, #2389), so no column width is in the key. What the engine does lay out by is the text scale (`--fs`
  where Mermaid measures its labels, `.tf-diagram-scratch`), and that is in the key.
- **The theme is left out.** All three engines are handed the theme as colours alone (`mermaidConfig`, D2's and
  Graphviz's palettes), so a drawing is the same size in both. With the theme in the key, a diagram seen all day in
  the light theme was "never seen" the evening the system went dark, and went through the hold or the fitted room
  again for nothing. Should an engine ever size by theme, the drawing is fitted and the size set right, as for any
  kept size that proves wrong.

The engine's version is a constant beside the key (`ENGINE_VERSIONS`), which a test holds to the installed packages.

## The look

In the existing fence, nothing new but one control (`/frontend-design`, kept inside the system): *Show at full size* is
a small ghost button in the fence's header, left of *Copy*, there only while the drawing is scaled down. The fitted
drawing is centred in the room on the sheet colour, 6 px clear above and below, and the whole of it is the same press
(the zoom-in cursor, as an attachment image that opens its preview). A cut drawing starts 6 px under the room's top,
centred, and its last 28 px fade to the sheet colour; its button reads *Show all*, the app's word where a list is cut
(*Show all runs*). No motion: the block takes its size in the frame of the press. A press on the button moves focus to
*Copy*, so a keyboard reader keeps their place.

## Not this pass's

- A diagram **file** in the Files tab and a diagram in a **rendered diff** (`TfDiagram`) keep no room: nothing of the
  file stands under the first, and a diff shows a before and an after that must stay at one scale. Both gain the kept
  size (an empty frame of the drawing's size from the first frame) and the session's drawings. A diff card under a
  diagram never seen still moves once when it lands.
- D2 and Graphviz fences get all of the above except the warm-up.
- A **pull request's** description and comments, a **diff's** comments and an inbox item's text hold nothing under
  their own text (only a ticket's description and comments tell their page to, `onheld`). A later comment is seen under
  a diagram there, so a diagram never seen is fitted on its first open even when its drawing is quick, and stands at
  full size from the second. Holding a windowed conversation's later rows for a comment's diagram is its own piece of
  work.

## Fixed on the way

*Copy* on a fence whose diagram was drawn copied nothing: the drawing had taken the `<pre>` the button read. A fence's
source is now kept beside its block (`fenceCode`).

## Proof

- **Tests**: 33 new (`diagram.test.ts`, `diagram-sizes.test.ts`, and in `frame`, `port`, `markdown` and `TfMarkdown`'s),
  which fail on 94 mutants. Among them: no stand-in for a kept size, a hold for a cold engine, a hold never ended, a
  late drawing that takes its own size, a fitted drawing scaled by one side only, the room not kept to the pixel, no
  *Show at full size* or one after *Copy*, a press that leaves the room locked, what is held unseen or stands beside
  the fence counted as under it, what stands outside its scroller counted, the fitted size kept in place of the
  drawing's, a size kept per theme, a stale engine version, the store never written or never read, a drawing nobody
  waits for thrown away, a text drawn again forgetting its rooms or handing one to the wrong fence, a speck standing
  whole, a readable drawing cut, the warm-up asking for everything at once or warming D2, and *Copy* reading the drawn
  block.
- **The failing test** (`moves.mjs --only repo-readme --throttle 1,4,6 --check`: `repo-readme-image` with its workers,
  and `repo-readme-diagram-late`, first open and return, 12 opens a size; headless Chrome on the WSL box). On the base
  `dbe1087a`: 11 of 12 opens moved at 2560x1440@1.25 and 10 of 12 at 5120x1440@1, the fences' *Copy* 54 and 56 px down.
  After, on the pushed `83210bd3`, a browser at a time: nothing moved in 36 opens, 12 at each of 1440x868@1,
  2560x1440@1.25 and 5120x1440@1, and the same again with the margin fix below.
- **The pages this touches** (`moves.mjs --only repo,ticket,pull-request`, 1x, 4x and 6x, on this change before its
  last two rebases): nothing moved in 228 opens, 114 at each of 1440x868@1 and 2560x1440@1.25.
- **What a reader waits for** (a probe of the README's first open after the app stood idle, 2560x1440@1.25, ms after the
  press, base then this change):

  | CPU | text | what stands under the diagrams | both drawings | fences under them moved |
  |---|---|---|---|---|
  | 1x | 62, 55 | 214, 207 | 176, 147 | 0 px, 0 px |
  | 4x | 302, 210 | 473, 536 | 828, 506 | 261 px, 0 px |
  | 6x | 360, 322 | 612, 791 | 1190, 748 | 261 px, 0 px |

  What stands under the diagrams shows 63 ms later at 4x and 179 ms later at 6x than on the base. It waits for both
  drawings, and Mermaid draws on the main thread, so at 6x the hold's 300 ms timer sits behind a 290 ms task and the
  hold lasts 470 ms. In exchange the page is whole at 791 ms, not 1190, and nothing jumps.
- **A return** to the README asks no engine: its longest task went from 65, 206 and 241 ms (1x, 4x, 6x: both diagrams
  drawn again, the fences 261 px up and down) to 0, 57 and 88 ms, with nothing moved.
- **Every drawing 900 ms late** (`repo-readme-diagram-late`, 2560x1440@1.25, 1x): the text at 53 ms, what stands under
  the diagrams at 349 ms with the source, the drawings at 1001 ms inside the source's rooms (678x142 as 534x111 in a
  124 px room, 450x316 as 189x132 in 145 px), each with *Show at full size*; nothing moved, on the return either. At
  1440x868 the same drawings took their own size: nothing in view stood under them.
- **The warm-up's cost**: while the app stands idle after start, one more long task than the base at 1x (50 ms, 0.9 s
  after load) and two more at 4x (70 and 88 ms). Mermaid's chunks are in 0.4 s after load at 1x.
- **The dark theme**: a new session with both sizes kept and the drawings held back 2 s stands with two empty rooms of
  678x142 and 450x316 on the sheet colour, the frames unpainted, no white box.
- **Press trips** `--strict` on `83210bd3`: 317 of 319 pass. `scroller-select-run` fails and `folders-list-return`
  cannot run, both the same on the parent commit.
- **`pixel-same.py`**, eight screens shot on the parent `d859dad5` and on `83210bd3` at 1440x868@1, 2560x1440@1.25 and
  5120x1440@1 (the README's first open and return, a diagram file, a broken one, Code, Tickets, a Markdown file with a
  D2 and a Graphviz fence, a pull request's conversation): 18 of 24 pairs the same or under the 600 px line. The six
  that changed are the two the ruling changes, at each size. The D2 fence, whose engine is cold and whose drawing is
  six times its source's height, shows its top with *Show all*, and the Graphviz graph under it stands at its own
  size in its source's room. The pull request comment's flowchart is fitted, since the comment box stands under it.
- **Found by those screenshots**: in the file view a diagram's holder carries the 12 px bottom margin that spaces a
  text's boxes, which a room of fixed height turned into 12 px of scroll and a scrollbar. A kept room's holder has no
  margin now; the fence's body measured 103 px with 103 px to scroll after it, from 115.

**Reversing it** is `drawDiagram` without its `room` (the drawing takes its size whenever it lands, as before),
`diagram-sizes.ts`, the warm-up in `TfShell`, `TfMarkdown`'s `hold.wait` handed to `drawFences`, and `noWorkers` back
on `repo-readme-image`: the fences under a README's diagrams then move when the drawings land.
