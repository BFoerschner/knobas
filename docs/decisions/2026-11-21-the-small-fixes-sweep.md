# 2026-11-21: The small fixes sweep (for ux27)

Four small things left over from the renderers-columns pass and the ux26 Windows boot check, each fixed or explained in
its own commit.

## *Add a default* stands on the table's columns

**Found.** Settings › Renderers' *Add a default* row was a settings row: its label on the left, and on the right the
pattern, two dropdowns and *Add*. A dropdown is as wide as its longest option (`flex: none`), so with a long pipeline or
renderer name the two ran to about 1,370 px, past the default renderers' table above and past the window. The table's
own dropdowns stay in their columns (`max-width: 100%`).

**Taken.** The row is the table's next row. Its label goes above, as every settings row's does below 720 px, and its
controls stand on the table's column template (`.tf-rnd-addcols`: the rest, 24 %, 24 %, 100 px at the UI size, inside the
table's 1 px border and its cells' 10 px). The pattern fills *Files*, each dropdown starts under the table's dropdowns
and is no wider than them, in their type size, and *Add* stands where *Remove* does. A long name cuts in its box and says
itself whole in the dropdown's title. Two 24 % dropdowns could not fit the right half of a settings row (about 418 px at
the section's 804 px), so the label had to move for them to have the table's widths.

**Held by.** `testenv/ui-checks/long-paths.mjs` gains `under`: the case `settings-renderers-add-default` holds each
control of the row to the table's column at its place, at the four sizes. Before: the pattern 138 px out of *Files*, the
dropdowns 433 and 749 px out of theirs, *Add* 700 px.
