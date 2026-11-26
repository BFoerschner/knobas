# 2026-11-19: The Decompiler's line keeps its room (before ux20)

**Found (the ux19 release gate, `just moves --real`).** On Settings › Renderers, the Decompiler setting
(`tf/settings/TfDecompilerSetting.svelte`, `docs/decisions/2026-11-18-a-jar-is-drawn-as-its-contents.md`) moved *The
Java to use*, *Use this Java* and *Look again* 142 px across and 5 to 28 px down on first open and return, at 1x, 4x and
6x, at both sizes. Its cell was a settings row's control cell, a flex row, so the Java line and the reason stood beside
the path row and pushed it across as their words came; the reason came in above it, and a long path took a second
line. `javaRuntime()` answers after the page draws on the real backend (the first look starts each Java it finds) and
at once on the fixture, so `just moves` on the fixture never saw it.

**Taken.** What comes for sure keeps its room from the first frame (`2026-11-17-nothing-moves-once-drawn.md`), rather
than a hold: the first look in a session can take longer than the 500 ms hold, and the line has words to say while it
looks.

- The cell is a column (`.tf-stg-c.tf-dec-c`): the line, then the path row.
- The line and the reason share one room two lines high (`.tf-dec-st`, `--t-sm` at line-height 1.5, `height: 3em`),
  drawn from the first frame. The line takes both lines, or one with the reason on the second (`tf-dec-one`); each
  cut with an ellipsis says its whole in its title.
- The path is a field in a row of controls (`tf-stg-c tf-stg-row1`, `tf-stg-in`), as *Default renderers*' pattern
  is: it drew with no field's box before, *Found by itself* reading as loose text.

**The gate catches it now on the fixture.** A place in `moves.mjs` may land reads of its own late (`late`, `fake`):
`settings-renderers-java-late` opens Renderers with `java_runtime` 900 ms late, past any hold, and a Java too old
(`fake-jar=oldjava`), so there is a reason to say. On the code before this change it moved the field 142 px across
and the buttons 20 px down at 1x and 4x, first open and return; `--real` showed the gate's 142 px and 5 to 28 px.

**Measured (Mac, headless Chrome, the windowless backend on the saved scale database, whose Java line is a two-line
mise path with the reason *this build carries no decompiler*).** `just moves --real --only settings` at 1x, 4x and 6x,
1440x868@1 and 2560x1440@1.25: 408 opens, nothing moved. `just open-budget --real --only 'settings-renderers$'`:
usable at 27 / 57 / 82 ms first open and 31 / 47 / 45 ms on return at 1x / 4x / 6x at 1440x868@1, 24 / 70 / 80 ms and
29 / 45 / 44 ms at 2560x1440@1.25, each within its line. `pixel-same.py` at 1440x868, 2560x1440, 1920x1080@2 and
5120x1440: Renderers changed (found, none, too old), Appearance, Performance, Checkouts, Backup and Sources the same.

**Reversing it** is the cell's `tf-dec-c` and the room's `tf-dec-st` taken off; the line then stands beside the path
row again.
