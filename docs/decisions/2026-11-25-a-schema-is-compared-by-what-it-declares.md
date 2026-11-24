# 2026-11-25: A schema is compared by what it declares

**Asked (Björn, 2026-11-24):** "yes", to finishing the XML, XSD and JSON design with comparisons by element. Step S7 of the compare-docs pass, as the XSD design's section 27 has it ([2026-11-23](2026-11-23-an-xml-schema-is-explored-across-its-files-and-an-example-is-made-from-it.md)). Björn agreed to keep the words *stricter*, *looser* and *different*.

## What it is

- **A `.xsd` in a comparison offers *Rendered*** beside *Lines*, which stays the default. It is planned from the file's name: a schema needs no hint from its hunks. A side that holds no schema says so.
- ***Rendered* is one list, as wide as the stage** (`tf/render/xsd/TfXsdDiff.svelte`), after a line of counts: *4 declarations changed, 0 added, 0 removed. 2 stricter, 2 looser, 1 different.* Then, quieter: *13 more changed through a type they name. 292 read the same, however they are written.*
- **A row** is a mark in words (*Changed*, *Added*, *Removed*, *Changed through*), the declaration's name, what changed in one line, and at its end which way. Its left edge is `--ok`, `--err` or `--warn`.
- **Opened**, it says each fact that differs, where and in words (*BANK_CODE: optional, now it must stand*; `250 → 100`; *the order changed: BANK_COUNTRY moved*), and under them the declaration on both sides as the explorer draws it.
- **`n` and `p`** step through the rows, opening each.
- **Which way** follows the design's table: what XML Schema itself decides. *Stricter* means a document that passed may fail now; *looser*, one that passes now may have failed before. Documentation and a default get no word.
- **Formatting is no change**: white space, attribute order, a prefix spelled differently, an enumeration's order.
- **Changed through**: a declaration whose own facts are the same but which names one that changed, one step. That includes a declaration of another file of the set, where that file changed too.
- **Nothing differs**: *The same schema on both sides.* A file on one side only lists everything, *283 declarations, all added*.

## How

- `render/xsd-changes.ts`: `factsOf` reads a declaration into facts at places a person reads, and `changes` compares two sets' declarations of the compared file (the spike's `xr-diff.js`, typed onto the app's model).
- Both sides are opened as sets on one schema engine, in the worker, and compared there (`compareSets`, a `changes` call naming both sessions).
- **The set's other files** are read through the diff's own reader (`TfDiffView`'s `setRead`). A file the change touches is read at each side; one it does not is read once, at the side after, for both.
- The rows are cut at 600: *And 1,234 more declarations: Lines has the whole change.*

## Departures from the design

- **The rows of an opened declaration are not marked** where they differ. The facts above them say exactly what differs, and both sides stand under them.
- **The sides' names** stand in the stage's caption, as every rendered comparison's do, not in a line of the list's own.

## Checked by

- `xsd-changes.test.ts`: each row of the design's table, formatting, an order changed, changed through in the file and in another file, a rename, the answer's rows and counts, openTRANS against itself.
- `TfXsdDiff.test.svelte.ts`: the counts' line, rows and directions, a row opened, `n`, the same schema, a side with no schema, a file only after, the set's file read at each side, no worker call for a side that is no schema.
- `TfDiffView.rendered.test.svelte.ts`: a `.xsd` opens as lines and offers *Rendered*; an untouched file is read once, a touched one at each side.
- Probes in `testenv/perf/xsd.mjs` (`compare`, `compare-n`) on `?fake-xsd-diff`; `moves.mjs` `diff-xsd-rendered`. At 1100 x 600 the mark *Changed through* was cut in its 104 px column; the column now holds it.
