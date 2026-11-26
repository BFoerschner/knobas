# 2026-11-19: A renderer's program keeps to its menu (before ux21)

**Found (Björn, v3.2.9-ux20 on Windows).** "The render as orchestra, when you click on it, and the path of the program is
long it gets out of bounds. Same problem as with the local checkout path back then. It's kinda mangled." *Render as*
listed each command renderer as its name over its whole command line, in the code face, which never wraps a span
(`tokens.css`). A Windows program under *Program Files* with a `--profile` path after it ran 1,300 to 1,400 px past the
330 px menu, and the menu scrolled sideways under it. The pipeline's *Add step…* drew a command step's whole command
the same way. The clone-path pass (`2026-11-16`, design system, *Paths*) had held every filesystem path to its box but
did not reach renderers, which `?fake-long-paths` had none of.

**Taken.** The paths rule, applied to a command: a menu shows a program, never a command line.

- *Render as*: an item is the renderer's name on one line, ending in an ellipsis when it is longer, over its program,
  the command's first word with quotes grouping it as the backend splits a command (`programOf`, `tf/path.ts`), drawn
  by `TfPath`, so it gives up its middle and keeps the program's file: `C:\Pro…orchestra-render.exe`. The whole name and
  command are the item's title, and `TfPath`'s own title is the whole command (`TfPath` takes a `title` now). A built-in
  still says *built in*. The face cuts a long name at 24 characters, whole in its title.
- *Add step…*: a command step is its name, up to 55 % of the row, and its program in the rest, cut the same way.
- A folder renderer's *Render with* select, as wide as its longest option before, stops at 18 rem and ends in an
  ellipsis, the renderer's name in its title.
- A failure that holds a command line (a command renderer's or a folder renderer's stderr, the line above it) wraps
  anywhere rather than run past its box. Settings › Renderers' lists, its command steps and Settings › Checkouts'
  applications already wrapped the command at spaces; they now wrap a long word too (`.tf-wrapall`).
- A right-click label over 36 characters (*Render with ⟨a long name⟩*) is its own title: the menu already ended it in
  an ellipsis.

The menu stays 330 px (and never wider than the window less 16 px): wide enough for a name and a program's file,
and placed inside the window by `TfMenu` at every size, as before.

**Held by the layout check.** `?fake-long-paths` adds two renderers of `*.rs` with long names whose programs are long
quoted Windows and Mac paths with spaces (one is the file's default, so `retry.rs` is drawn by it and fails with its
command on stderr), a long command step, and two folder renderers, one that draws and one that fails.
`testenv/ui-checks/long-paths.mjs` opens *Render as*, picks each long renderer, reopens the menu, opens *Add step…*, a
folder drawn and failed by a long folder renderer, a folder's right-click menu, Settings › Renderers, a renderer's
dialog and a folder renderer's page, at its four sizes (11 screens, 44 cases more). A screen step can now press by a
selector or by words an item holds, press more after it (`then`), right-click (`context`) and wait (`wait`);
`--shots` writes each screen, long and short. A renderer's name may end in an ellipsis; a path still may not. On the code
before this change the check failed *Render as* and its reopening at all four sizes (the program 1,310 to 1,418 px out
of the menu, the menu scrolling sideways); after it, 104 of 104 cases hold. The press trip `render-as-long-key` opens
*Render as* with the long renderers, requires the menu inside the window with no row wider than it (`fits`, new on
`enter-twice`), picks the long renderer by Enter and requires focus back on the face, which names it.

**Reversing it** is `TfRendererSwitch`'s and `TfFilePipeline`'s items back to `command.command` in a `.tf-mono` span, and
the `.tf-rsw-*`, `.tf-pmenu-nm` and `.tf-frender-as .tf-select` rules taken out.
