# 2026-11-21: Two rows that moved in the ux28 gate, and three trips that only passed on Linux

The ux28 gate (stopped at Björn's word) left two rows-still trips failing some of the time, on WSL and alone:
`builds-rows-still` (1 of 6) and `code-rows-still` (4 of 8). Both are app bugs, fixed here. Three press trips also
failed in headless Chrome on the Mac and passed on WSL: `theme-slot`, `theme-swatch` and `builds-rows-still`. The first
two were the trips' fault.

## A pipeline with a job draws once the job's branches are read

**Found.** On the Mac, frame by frame: the pipeline page drew with *Run on main*, and about 450 ms later the job's
branches answered (`readJobBranches`: the sources, the ticket, then `list_entities`, three reads in a row). The button
became *Run on feature/PAY-231-sepa-retry*, the header's actions grew from 698 to 873 px and wrapped under the name, the
header went from 77 to 111 px, and every run row moved 34 px down. Whether the branches came before or after the first
draw was down to timing, so the trip failed some of the time.

**Taken.** With a job chosen, `TfPipeline` draws nothing until the job's branches have answered, or failed (`drawing`).
They decide *Run on <job branch>*, the job's chips on the rows and *This job*'s count, so the page draws once, as it
stays. `TfBuilds`'s `deciding` already holds a bare `#/builds` the same way until the job's pipelines are read. The read
is made once per job while the page lives, not on every pipeline.

**Held by.** `TfPipeline.test.svelte.ts`: with the branches' read still out, no header and no run row; once it answers,
*Run on feature/PAY-231-sepa-retry* and the job's chip, never *Run on main* first. A read that fails still draws the
page, *Run on* naming the default branch. And `builds-rows-still` in Chrome.

## Code's repository rows keep their height where nothing hovers

**Found.** Where nothing hovers (`hover: none`: a touch screen, and headless Chrome on Linux; headless Chrome on macOS
reports `hover: hover`), a row's actions stand in the row instead of floating beside it. The checkout read brings each
repository's *Clone* or *Open in…*, a small 26 px button, so a row drawn before it was 34 px high and grew to 41 px when
it landed. Under `hover: hover` the actions float and hold no room, so the Mac never showed it.

**Taken.** Under `@media not (hover: hover)`, the end of a Code repository row keeps a small button's height
(`--control-h-sm`) from the first draw. Every repository ends up with one of the two buttons, so the rows are the height
they always ended at. Branches' rows already reserve this room the same way. Pointer devices see no change.

**Held by.** `code/repos-layout.test.ts` holds the rule as the stylesheet writes it, since jsdom lays nothing out. The new
trip `code-rows-still-touch` turns on Chrome's touch emulation, which reports `hover: none` on every machine, and holds
the checkout read back 2 s, so it lands after the rows have drawn. Before the fix it failed every time, on the Mac and on
WSL (`fx-ui` went from 34 to 41 px at 515 ms); after it, it passes. The `rows-still` kind takes `touch` for it.

## The theme trips change things the way a person can on every platform

- `theme-swatch` chose the hex field's text with Ctrl+A. On macOS that isn't select-all, and Chrome edits nothing from a
  key event alone there, so the colour was typed after the old one. The step `{ selectAll }` sends the platform's
  select-all, ⌘A on macOS and Ctrl+A elsewhere, with Chrome's `selectAll` command.
- `theme-slot` pressed the `<select>` and then ↓ and Enter. On macOS a press or ↓ opens the native menu, which headless
  Chrome can't drive. It now focuses the select (`{ focus }`) and types "gi" (`{ chars }`), which chooses GitHub Light by
  its first letters on every platform.

## The coverage plans' fixture holds still under autovacuum

**Found.** `knobas-search`'s `coverage_plans::a_probe_never_reads_a_table_to_prove_a_source_has_nothing` read 305 child
rows of `gitea-swept-last` against its 257 once, in the ux28 gate's `just test` on WSL. Alone on the Mac it passed 5 of
5. The plan was the one meant: 256 rows from the far end, then the look from the head, which read 49 rows before its first
live one. The fixture writes that source's live rows first so they head the index. But an index orders equal keys by
their place in the table, and a vacuum between two of the writes (autovacuum vacuums after a thousand inserts) lets the
later withdrawn rows fill earlier pages, ahead of the live ones. A vacuum at that point read 270. With autovacuum made
eager (1 s naptime, an insert threshold of 50), the test failed 4 of 6.

**Taken.** The fixture turns autovacuum off on every `knobas` and `sync` table before it writes
(`knobas_db::plan_shape::never_analyzed`, the method #767 and #768 used, as `reach.rs` does), so it is what it says, and
"never analyzed" stays true. Under the same eager autovacuum it passed 6 of 6. The limit stays 257: it was right, and the
probe's code is unchanged.
