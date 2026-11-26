# 2026-11-24: A schema's example is drawn as a small XML view, and opens as a whole one

**Asked (Björn):** "I would like to be able to open an example from the xsd in the xmlview, or in general have the xsd example be in a miniature xml view without the left sidebar."

Step E1 of the compare-docs pass. Where the opened example lives, and what *Save* does there, went to Fable (xhigh) under Björn's standing instruction; Opus built it. It builds on [the schema explorer's note](2026-11-23-an-xml-schema-is-explored-across-its-files-and-an-example-is-made-from-it.md) and [the XML view's](2026-11-23-an-xml-file-is-read-as-a-document.md).

## The example column

- **The document is the XML view's own** (`TfDocView` with `inline`), not numbered lines: the way down and *Levels*, folds and their keys (`←` `→`, `-` `+`, the digits), value chips and `Space`, addresses, and the check's departures at their lines' ends. It has no rail, no query and no side column. `Q`, `N` and `P` go to the explorer.
- **The check is the column's *Check***: its verdict is handed to the view, which places each departure by libxml2's path to the node, as the XML view places its own (`DocCheck.take`).
- **A value is looked at through a pipeline over the example's text**: `text_piped` and `text_piped_save`, new commands that run the file pipeline over bytes held in memory, as `entity_file_piped` runs it over a file's. No row links to a line: the example is at no address.
- **Nothing else changes**: seed, `A`, ticks, *Required only* and *Everything*, *Check*, *Copy*, *Save to Downloads*. The controls stand where they stood while the example is made again.

## Open in XML view

- **A place of its schema** (Fable): `at/example%2FORDER/<schema path>`. *Open in XML view*, beside *Save to Downloads*, pushes it. The explorer lands ORDER underneath, hidden and inert, and draws the whole XML view over it in its own pane's box: rail, document, side column from 1,480 px. Back and `Esc` show the explorer again as it was left, and nothing is read again. `Esc` on an address that came from elsewhere replaces it with ORDER.
- **Checked against the set it was made from**: the explorer's own port, borrowed and never closed by the view. The head says *Against opentrans_2_1.xsd, found where the example was made.* There is one verdict: the column's, so going back shows it with no press. A round of the set's files landing checks it again; its text stays.
- **It is the explorer's own example**, taken when the view opens and still while it shows. It has no seed and no *Another*, and `A` does nothing while it shows.
- **An example's settings are held until knobas quits**, by repository, path and content, as folds are. An address straight to an example makes the same document again; after a restart it is seed 4711.
- **The file header** names it after the schema's path (`schemas/opentrans/opentrans_2_1.xsd › ORDER-example-4711.xml`), says *Example made by knobas, seed 4711, required parts only. 212 lines. Not saved.*, and offers Zen, *Copy* and *Save to Downloads*. *Render as*, *Pipeline*, *Open in* and the file's menu are the schema's and do not stand for it.
- ***Save to Downloads* is unchanged** (Fable): `xsd_save_example`, the text with its *Made by knobas* comment, a free name in Downloads, no copy kept.

## Rejected

- **The file viewer dialog** (Fable): too narrow for the side column, the view's own dialogs would stack on it, and it has no Back.
- **A route of the Files tab that swaps the explorer out**: it would close the borrowed port, read the set again on Back, and lose seed, ticks and picks.
- **A tab of its own**: unsaved documents would need a list, a close and a lifetime, away from the repository their check needs.
- **A save dialog, or a save into the repository**: a second way to save; and an invented order must not be one press from a commit.

## Checked by

`TfDocView.test.svelte.ts` (the miniature form; a verdict placed and replaced), `TfXsdExample.test.svelte.ts` (the column's document; *Open in XML view*, its check, `A`, `Esc` by Back; an address straight to an example with held settings), `TfRepoFiles.example.test.svelte.ts` (the header and *Save to Downloads*).
