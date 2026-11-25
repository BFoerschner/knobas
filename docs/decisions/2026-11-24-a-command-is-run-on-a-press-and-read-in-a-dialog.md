# 2026-11-24: A command is run on a press and read in a dialog

**Asked (Björn, on PR #2479):** "I'd also like to be able to run an open in renderer/application in a way that it opens a modal with the html/image/code/text for like linters that i could run on demand or sth on the files or folders but that don't render them in a different way but just are like single off commands"

The judgement calls went to Fable under Björn's standing instruction, and Opus built them. The architecture is [ADR-0060](../adr/0060-a-command-runs-in-the-checkout-on-a-press-and-knobas-reads-what-it-prints.md). This records the rulings.

## What was there

Three ways to run a program of your own. An *Open in* application is started at the checkout and let go, and nothing it prints is read. A renderer and a folder renderer run on stdin or on a copy knobas wrote, when a page opens, and what they make stands in the file's or the folder's place. A pipeline step turns bytes into bytes. None of them runs a linter: a linter needs the real checkout, and its answer is read once.

## The rulings

1. **A command is *Open in* that knobas waits for and reads.** Its own list, Settings › Commands, one settings row. It runs in the checkout with no shell, in the app's environment less every `KNOBAS_*` variable, and it never stands in a file's place.
2. **What a command is offered on is read from its placeholders**: `{file}` a file, `{dir}` a folder, neither the repository, `{sha}` a commit, `{base}` and `{head}` a comparison. Four glob lists narrow it, and a menu shows only the commands that apply.
3. **Fifteen placeholders**, each a path inside the checkout, a number, a git name held to ADR-0034's grammar, or a path knobas made. `{path}` is refused by name.
4. **A file's name is never shell text.** A path in a quoted script is refused at save, with the form that works: `sh -c 'eslint "$1" | head -40' _ {file}`.
5. **It starts** from *Run… ▾* in a file's header, the repository's header and a generated folder's bar, from the right-click menu's *Run ⟨first command⟩* and *Run… ▸* after the *Open in* group, from `X`, and from a file's or a repository's sheet in Search. `Shift X` runs the last one again.
6. **A folder gets the folder's commands.** A file's command on the files a folder holds, or on a diff's changed files, is offered by name as one run a file, four at a time, at most 200.
7. **The dialog is a viewer with a run line**: how the run stands with the exit code as a figure, the time, the command as it was run, and a stepper through its earlier runs. Its stage's height comes from the window. It paints in the frame of the press, before the program has started.
8. **Pipes, never a terminal.** knobas asks for colour through `FORCE_COLOR`, `CLICOLOR_FORCE`, `CARGO_TERM_COLOR` and `PY_COLORS`. The backend reads both streams into lines with colour runs and drops every escape that is not a colour.
9. **A program's colours are the theme's tones**: red is failed, green passed, yellow attention, blue running. No value a program prints becomes a colour.
10. **`path:line:col` is a link** where the file exists inside the checkout, in five shapes. A place a program said something about is a finding, and *Problems* lists them. A press opens the file at the line and keeps the run one press away.
11. **A result that is not lines** is the file under `{out}`, the files under `{outdir}` or a stdout that is not text, drawn as the file viewer draws it: a page with no script, an image, a PDF, JSON, Markdown, code, a diff. Several files are a list with the picked one beside it.
12. **Closing the dialog leaves a run running**, shown in the status bar. *Stop*, the timeout and the quit end it, with everything it started.
13. **A run keeps** 200,000 lines and 16 MiB, its first 2,000 lines and its newest. What it printed is in memory until quit and nowhere else. The list of the last 200 runs, without output, is a file in the profile that no backup carries.
14. **Nothing runs by itself, and a share export carries no command.**
15. **A repository's own commands are never run from the repository.** They can be copied into the person's list after a look: a repository's *Run…* menu says when its checkout's `.knobas/commands.toml` offers some, and a dialog shows each whole line, what it takes and why Save would refuse one. A copy is scoped to that repository and is the person's from then on; when the file later changes a line, the menu says so and the dialog shows both lines, and copying again keeps the PATH folders, variables and colours they set.
16. **On Windows** the kill is a job object, a path for `cmd` may not hold `%`, `!`, `^` or `"`, and `wsl` with a path needs `-e`. A tick gives a command its paths as WSL sees them.

## Not in this pass

Marks in the file view after a run. Picking several rows of the Files tree. Scripts in a page a command made. A terminal. A command on a save or a sync. Reading a tool's JSON or SARIF report. A renderer offered as a command.

## Measured

On the clickdummy, drawn on the app's stylesheets, in headless Chrome on the WSL box, at 1440 x 868 and 2560 x 1440 at 1.25. The press to the dialog painted: 2.2 and 2.5 ms at 1x, 15.4 and 14.4 ms at 6x. A kept run of 150 lines opened: 8 to 11 ms at 1x, 53 to 68 ms at 6x. While a run printed 20,000 lines a second, frames were 16.7 ms at 1x and at 6x with the lines read off the page, and 33 to 50 ms at 6x with the page reading them itself, which is why the backend reads. *Wrap* on 120,000 lines: 12 to 13 ms at 1x, 75 ms at 6x, 133 ms worst. In the app (`testenv/perf/oneoff.mjs`, the built app under `?fake-ipc&fake-oneoff`, headless Chrome on the WSL box, five presses, median, at 1440 and at 2560 at 1.25, on the build's last head): *Run…* to its menu 3.1 and 15.9 ms at 1x, 14.3 and 15.2 ms at 6x; a command pressed to the dialog painted 8.6 and 15.6 ms at 1x, 23.4 and 23.2 ms at 6x; a kept run of 200,000 lines opened 13.1 and 17.3 ms at 1x, 106.4 and 95.1 ms at 6x; *Wrap* on it 1.0 ms at 1x, 5.0 and 6.0 ms at 6x. While a run printed 20,000 lines a second, frames were 16.7 ms median at 1x and at 6x at both sizes (95th 33.4 ms at 6x, worst 50.1 and 66.6 ms); scrolling the 200,000 lines, 16.7 ms median, 33.3 ms 95th at 6x. Nothing moved in the dialog, Settings › Commands or a command's page at 2560 at 1.25 (`moves.mjs`, 1x and 4x), once *Follow* stayed in its place after a run ended and *Run again* held both its words' width. Owed on Björn's PC: the same in WebView2.

Real programs, in the Linux build against a clone of the live Gitea's `tidewater/ledger-api`: ESLint 9 on a TypeScript file (4 findings), Ruff (3), ShellCheck (5), `xmllint --noout --schema` on `order-4711.xml` (exit 3, 4 problems), a report script writing a page to `{out}` (drawn with its script stripped), `dot -Tpng` printing a 519 x 131 PNG to stdout, a folder's files copied to `{outdir}` (*Files 2*), xmllint on the two changed orders from *Local changes* (one run a file), a failing command (exit 2), a program that is not there, and a repository's `.knobas/commands.toml` copied after a look (a refused line not offered, a changed line said).

## Reversing it

Take *Commands* out of Settings' nav, `runIn` out of `context/items.ts` and the three *Run…* buttons out of their headers: nothing then starts a run, and the row `oneoff.commands` stays unread. The backend's `oneoff` module and its thirteen commands can stay or go with no other code leaning on them. `<profile>/command-runs/` is knobas's own folder to delete.
