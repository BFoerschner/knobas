# Builds rail as a tree

Mockup: [rail-tree.html](rail-tree.html) (2026-11-13). Its yellow bar switches between the new rail and today's.

## What changes

The pipelines rail on `#/builds` becomes a tree you can scan:

- **One line for each pipeline, 26 px.** The twelve-run strip is gone. The run history is still on the pipeline's page (the chart and the Runs tab). In the same height the rail shows about 31 rows where it showed about 20.
- **A status glyph column on the left.** A pass is a small green tick. A failure is a filled red disc, and the name turns red and bold. Running is a blue spinner in the mockup and a pulsing blue dot in the app, because the design system allows no other motion. Queued is a dashed ring, warnings an amber triangle, cancelled a struck ring, no runs a dash. Red is the only loud colour, so reading down the left edge finds the failures.
- **Guide lines for depth.** A 1 px line runs under each ancestor's glyph column, 14 px for each level as before. Hover and the selection start at the row's own indent, so the guides stay visible.
- **The shared prefix is dimmed.** When every pipeline in a folder starts with the same text up to a separator (`Payout_` in Payouts, `Payout_Deploy_` in Deploy), that part is drawn in the second ink colour and the part that differs is read first. A pipeline alone in its folder, and anything in *Pinned*, keeps its whole name in full ink.
- **A time only where it means something.** Failed rows show when they failed, in red. Running rows show when they started, in blue. Queued rows say *queued*. Passing rows show no time, and the row's title has it. The bar's *Times on every row* button shows the other option.
- **Folder counts.** A red pill for failing, a blue pill for running, then the total in the second ink colour.
- **Collapse all / Expand all** to the right of the chips. It sets every folder as if the reader had toggled it.
- **Tree keys.** Up and Down move between rows, Right opens a folder, Left closes it, Enter opens a pipeline.

These stay as they are: *This job* / *All*, the *Failing* and *Running* chips, *Pinned* first, a source's name over its tree, a chain of single children shown as one row, the rule for which folders start open, the inverted selection and the windowed list.

## Ported

The change shipped on `ux-polish` (2026-11-13), within the Builds files:

- **`builds.ts`** adds `sharedPrefix`, `railTime` and `railGlyph`. `ListRow`'s pipeline rows carry `pre`, and `Fold` carries `running`. `STRIP` and `stripOf` are gone.
- **`TfRunGlyph.svelte`** is the glyph (`.tf-rg`, because `.tf-st` was already the ticket table's state cell). Its words are the run's state as `stateWord` says it, the same word its chip shows.
- **`TfBuilds.svelte`** draws the rows, the folder counts, *Collapse all* / *Expand all* and the keys. The windowed estimate is now 26.
- **`builds.css`** replaces the two-line row, the strip and the 240 px rearranging. Changes from the mockup's block, after review:
  - the prefix of a failed or selected name keeps the name's colour and only drops the weight, because faded red text failed 4.5:1;
  - folder names are the second ink, as group labels are;
  - queued is `--info`.
- A failed row's time is when the run finished.
- The keys follow the docs tree's table: Home and End, and Left from a pipeline goes to its folder.

The app keeps the class names it already had (`.tf-pi`, `.tf-pi-nm`, `.tf-pi-why`, `.tf-pi-when`), so `chrome.css`'s shared selected-row rules still apply. The mockup's `.tf-tr-nm`, `.tf-tr-pre` and `.tf-tr-at` became `.tf-pi-nm`, `.tf-pi-pre` and `.tf-pi-when`, or `.tf-fold-nm` and `.tf-fold-at` on a folder. `.tf-tr` is the row geometry both share.

The mockup leaves out the stacked rail below 900 px (`TfRailBar`), because that part does not change.
