# 2026-11-20: A renderer's row is one line (for ux26)

**Found (Björn).** "In the renderers view, the command, the files, and so on completely explode the columns; maybe have it
render the same way as the local paths in code." Settings › Renderers' tables (command renderers, folder renderers,
command steps, pipelines, default renderers) let the browser size their columns from what the rows held, and the render-as
pass (`2026-11-19-a-renderer-s-program-keeps-to-its-menu.md`) had made a command wrap anywhere (`.tf-wrapall`) so it stayed
in its box. So a long Windows command took most of the table and broke over four to six lines, ten file patterns broke
over three, and every other column shrank to its longest word: a renderer's name stood one word a line beside them. A
default's deep pattern and a pipeline's steps did the same. The layout check held text to its boxes, which wrapped text
is, so it never saw it.

**Taken.** The paths rule, applied to a table: a cell is one line, as Code's *Local checkout* column is (design system,
*Paths*).

- A command (a renderer's, a folder renderer's, a command step's) is a `TfCommand`: its first word, quotes kept, is cut
  as `TfPath` cuts a path, so the program's start gives way first, all of it if need be, and its file stays
  (`orchestra-render.exe"`); then its arguments, in the muted ink, are cut at their end. The start takes only the room
  the file and the arguments leave (a flex basis of 0, growing to its own width at most), so no share of a shrink falls
  on what still fits and no ellipsis shows on a whole run.
- A list (a renderer's file patterns and fences, a pipeline's *Default for*) is a `TfWords`: its first three, cut at
  their end if even they are too long, and a muted *+N* that is never cut. Three, not as many as fit: a count that
  followed the width would need a measure after the first frame, and the row would change once drawn.
- A default's pattern is a `TfPath`, so `docs/…/*.orchestra.json` keeps its end. A name, a pipeline's steps and what a
  folder renderer takes end in an ellipsis (`.tf-cut`).
- Each says its whole in its title, and all of it is in the text (a list's rest visually hidden), so a selection of a
  cell copies it whole and a screen reader reads it.
- The tables lay out fixed (`.tf-rnd-tbl`): a name a fifth of the table, the lists a sixth to a quarter, the short facts
  (reads → writes, a timeout, the actions) a width that holds them at every UI size (`px * --size-ui`), and the command
  the rest. A long value cuts itself and moves no column, and the table is never wider than the page.
- The Decompiler's line says a found Java as one line with its path a `TfPath`, so a Java deep under a profile keeps
  `java.exe` and the reason, when there is one, keeps the second line.

The edit dialogs and a folder renderer's page hold commands and patterns in fields, which scroll inside themselves, and
were left as they were.

**Held by the layout check.** `?fake-long-paths` now gives the Windows renderer a 330-character command with eight
arguments, ten patterns and five fences, adds a folder renderer that makes a folder with a long command and long lists
of what it takes, a pipeline with a long name and five steps that eight patterns open through (one of them a deep path
with spaces), and a Java under a Windows profile. `testenv/ui-checks/long-paths.mjs` gains `tables`: on Settings ›
Renderers every cell of every table must be one line, every column as wide as with the fixture's short values, and no
table wider than its box; a command's program start and its arguments may give way (`tf-cmd-head`, `tf-cmd-args`), as a
path's start does, and a visually hidden run scrolls nothing. On the code before this change Settings › Renderers and the pipeline's dialog over it failed at all four sizes: every table wrapped, a renderer's command over 17 lines, a default's pattern over 27, the command renderers' columns changed from 100/108/136/85/173/201 px to 109/228/119/93/109/145, and the defaults table ran 237 px past its box. After it, the settings screens, *Render as*, *Add step…* and the folder render screens hold, 48 of 48 cases (Mac, headless Chrome).

**Reversing it** is the four tables' cells back to their text in `.tf-wrapall` and `.tf-mono` cells, the `.tf-rnd-*`
widths and `.tf-rnd-tbl` taken out of `settings.css`, and `TfCommand`, `TfWords` and their rules in `chrome.css` removed.
