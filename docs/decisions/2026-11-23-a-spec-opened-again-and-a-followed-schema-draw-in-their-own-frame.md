# A spec opened again, and a followed schema, draw in their own frame (2026-11-23)

The ux30 end gate measured two API explorer cases (ADR-0057) over their Mac lines in `testenv/perf/openapi.mjs`, at
1440x868@1 and 2560x1440@1.25, every run: *a 5,000-operation spec opened from the README* (55.6 / 107.9 / 155.3 ms at
1x / 4x / 6x against 50 / 100 / 150) and *a ref followed to its schema's page, on TeamCity* (82.0 / 130.2 ms at 4x / 6x
against 50 / 100). What was slow, from traces of the built app in headless Chrome, and what changed. No line changed,
and nothing drawn looks different once drawn.

## The open of a 5,000-operation spec

- **The filter's list was made on the open, through a deep proxy.** The explorer held the worker's answer in `$state`,
  so `found` (the 5,000 rows the filter ranks) made a proxy of every row the first time it read them, on the open's
  frame: 10 ms at 4x. The answer is now `$state.raw` (it is replaced whole, never changed in place), and the list is
  read only once the filter has words.
- **The worker indexed the spec again on every open.** It kept the parsed document of a text read lately, but walked
  its 5,000 paths again each time. The index is now kept with the document (`openapi.ts`, `indexed`).
- **The rows crossed as 5,000 objects.** The page's thread spent 8 ms at 4x taking them. They now cross as columns
  (`openapi-pack.ts`) and are made rows again on the page's side, the same.
- **A spec opened again waited for the worker's round trip, and for a task more.** The page's side of the port now
  keeps the last four indexes by their text (`openapi-port.ts`, `openedLately`): a spec opened again takes its index in
  the press's own task, and the worker is still told, so its session has the document the later calls read. The file
  view's `{#await import(…)}` of the explorer waited a task (often behind a frame) on every spec, loaded or not; it now
  gets the module itself once loaded (`oas-module.ts`), so the explorer mounts in the same task.
- **The file view split the whole text to say its line count.** The header now counts the lines (`countLines`), and
  the highlighter's focus effect, which made the windowed source list size 75,000 lines, runs only under *Source*.

## A ref followed to a schema's page

TeamCity's `build` has 86 properties; the page drew 80 of them in the click's own task (32 ms at 4x) and laid them
out (31 ms; each row a subgrid of the list's grid), though some 4 to 7 rows fit in the column.

- **The page draws a screenful first** (`TfOasSchemaPage`, `firstRows`): its head, and the properties a guess at their
  heights says fill the pane (`OasState.paneTall`). The rest come 12 a frame from the frame after, below what is drawn;
  the example, *Used by*, *Part of*, a choice and *Show the other* come with the last property.
- **The columns hold their width from the first frame.** The list's name, notes and *required* columns are sized by
  every row. Each row not drawn yet that could widen one (`columnHolders`: the longest name, since a name is the code
  face and breaks anywhere; a row of each set of notes; a *required*; a name past ASCII) stands as a row of no height,
  unseen, holding just those cells. So a row drawn in the first frame never moves, sideways or down, as the rest come.
- **A page gone back to at a scroll draws whole at once** (`OasState.drawWhole`), so the scroll it had is there.
- **The inline form draws a schema whole** (`screenful` is the page column's only): there, rows stand under the page
  and would move as it grew.

## Measured

On the Mac, headless Chrome, `openapi.mjs`, the parent build (`6a31a1af`) alternated with this one in the same queue
turn, load 2 to 3.5, two sessions. Medians at 1x / 4x / 6x, the range over the runs.

| case | size | before | after |
|---|---|---|---|
| a 5,000-operation spec opened from the README (line 50 / 100 / 150) | 1440x868@1 | 64 / 94-95 / 139-141 | 31-36 / 67-72 / 98-100 |
| | 2560x1440@1.25 | 64-66 / 98-103 / 145-150 | 36-37 / 71-75 / 102-105 |
| a ref followed to its schema's page, on TeamCity (line 50 / 50 / 100) | 1440x868@1 | 27-38 / 79-81 / 116-117 | 18-26 / 38-40 / 56-62 |
| | 2560x1440@1.25 | 40-42 / 84-86 / 127-128 | 16-29 / 46-49 / 69-70 |

Every other case of the probe stayed within its line at both sizes. A follow's first frame drew 8 to 13 rows and one
held row on TeamCity's `build`, `buildType` and `project`, and no row drawn in it moved, sideways or down, as the rest
came in (a check of each row's top and its type cell's left and width, the first frame against 2 s later).
