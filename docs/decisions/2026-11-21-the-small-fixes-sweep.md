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

## A folder renderer saved in Settings already marks the open tree

**Found (the ux26 Windows boot check).** "After a page reload its glyph stood on `docs`, because the page had read the
renderers before I saved this one." The Files tree reads the app's one copy of the folder renderers
(`render/folders.svelte.ts`), and every save in Settings › Renderers goes through that copy and holds what the backend
stored. The tree's glyphs and its marks (for a renderer that holds a file glob) follow the copy, so they follow every
save. In the built app under `?fake-ipc`, a folder renderer added in Settings while the Files tab was kept had its glyph
on the tab at once. The boot check drove the app with JavaScript in the page and most likely called
`set_folder_renderers` itself, past the copy, which no person can do.

**Taken (Fable's ruling, option B of three).** No change to the app. A backend event for this one list would leave the
other three saved lists (pipelines, command steps, defaults) as stale under the same call, and nothing else writes it.
A pin holds the behaviour: `TfRepoFiles.folders.test.svelte.ts` saves a renderer by a folder's name and then one by a
file glob while the tab is open, and each glyph comes at once. It passed before; a mutant that stops the marks from
following the list fails it. A boot check that saves through a command call reloads before it looks at the page.

## The launcher's first `Esc` takes the words

**Found (the ux26 Windows boot check).** After typing, the first `Esc` left the launcher open and the second closed it.

**Not changed.** It is the rule: the design system says `Esc` takes the words and then closes, so an opening with no
words closes in one press, and that `Esc` never loses typed words in one press (*Keys*, `filterKeys`).
`Launcher.svelte`'s `Escape` does that (the chip stays with the words gone, and the next press closes).
