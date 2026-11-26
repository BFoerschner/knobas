# 2026-11-21: Code is Cascadia Mono; ligatures are Cascadia Code, off until turned on

**Asked (Björn, for ux27, dictated, through the orchestrator):** "I would like to use Cascadia Mono, because it seems
like Cascadia Mono looks better with like, esquito [ASCII] and boxes, for example, and just in general, seems like the
nicer fun to look at. So, and make it like 105%, I think. Because the public [default] is at 105%. So I guess it's
like meant to be, yeah."

Amends `2026-11-20-the-faces-knobas-brings.md` for code. Two calls went to Fable (xhigh), whose rulings are below.

**Amended 2026-11-26** by #2516 (spec #2514: "The default code font and line height are not `Cascadia Code 2009.22`
at `1.60`"): with nothing kept, code draws in Cascadia Code 2009.22 with its ligatures, rows 1.60 of the size, which
`css/tokens.css` now draws itself. Ligatures are on until turned off (kept as `off`), the newer release is kept as
`2407.24`, and *Joined* as `joined`. A choice kept before stays as it was; a default that was kept as nothing before
reads as the new default.

**Amended the same day** by `2026-11-21-fonts-you-can-tune.md`: code rows are the face's own line height (*Joined*) by
default, Settings › Fonts lists the bundled Cascadias apart from installed ones under CSS names of their own ("knobas
Cascadia Mono", "knobas Cascadia Code"), so "Both names are the bundled face" below holds for a name typed bare and
"Cascadia Mono (installed)" is the machine's copy, and the older Cascadia 2009.22 comes along.

## What was decided

- **Code is Cascadia Mono 2407.24**, from the release the bundled Cascadia Code came from
  (`CascadiaCode-2407.24.zip`, SHA-256 `e67a68ee…c8474`): `woff2/CascadiaMono.woff2` (210,484 bytes, SHA-256
  `7d4986a6…49799`) and `woff2/CascadiaMonoItalic.woff2` (153,708 bytes, `c00f68db…7116a`), variable, weights 200 to
  700, byte for byte, in `app/src/lib/tf/fonts/cascadia-mono/` beside `OFL.txt` (the tag's `LICENSE`: SIL Open Font
  License 1.1, "Copyright (c) 2019 - Present, Microsoft Corporation, with Reserved Font Name Cascadia Code") and
  `SOURCE.md`. `fsType` 0. Mono is Code without the programming ligatures: its metrics are Code's to the unit (2048 an
  em, x-height 1060, cap height 1420, ascent 1900, descent 480, advance 1200), so `->`, `=>`, `---`, `|--` and box
  drawing stay the characters the file holds, and nothing moves between the two.
- **The size stays: the slider reads 100 % at the size Björn calls 105 %** (Fable, Q1). Mono's `@font-face` says
  `size-adjust: 105%`, as Code's did, so Mono draws at 105 % of its own size, which is the size Björn had set an
  installed Cascadia to by hand and on 2026-11-20 asked to be called 100 % ("with 105% ... make it 100% for this
  then"). Where Björn's "the default is at 105%" most likely comes from: with an installed Cascadia Mono picked,
  Settings › Fonts marks the step where its lower case is as tall as the bundled face's, and said "The ring at 105 %
  draws Cascadia Mono's lower case as tall as Cascadia Code's": the bundled face was at 105 % already. Making the
  slider read 105 % was weighed two ways and turned down: dropping `size-adjust` and making `--size-code` 1.05 would
  have grown every code line height and every code-sized column by 5 % (a 20.8 px line to 21.84, off the 1.25 device
  grid) and changed the rounding of `.tf-mono` keys across Tickets and the chrome; relabelling the scale (90 to 135,
  `--size-code` the reading over 105) would have moved nothing but made code's numbers mean something no other role's
  do. If Björn wants the figure to read 105, the relabelling is the way, with `TfDay.svelte` and `TfRun.svelte`, which
  measure text at `fonts.size("code") / 100`, reading the new scale.
- **Ligatures in code: off until turned on; on draws Cascadia Code** (Fable, Q2). The switch stays: off draws
  Cascadia Mono and turns a chosen face's ligatures off; on draws the bundled Cascadia Code (kept, 378,584 bytes) and
  turns a chosen face's ligatures on. Dropping the switch would have taken a working control from anyone who chose a
  ligature face (Fira Code and the like), and Björn asked for ligatures one day before. The cost is disk only: `--mono`
  names just the Cascadia the switch draws (`--mono-bundled`, `css/tokens.css`), the idle loader (`shell/faces.ts`)
  leaves the other alone, and turning the switch waits until the face's upright and italic have loaded before code
  turns to it, so no line is drawn in the stand-ins and again in the face.
- **What a machine keeps** (`fonts.svelte.ts`, `toCascadiaMono`, once a machine, marked by `knobas.code.mono`):
  - The bundled face draws Cascadia Mono whatever an earlier build kept: on by default (2026-11-20), turned off, or an
    `on` kept before 2026-11-20 that the 2026-11-20 build never cleared. Björn asked for Mono, and he likely turned
    ligatures on by hand before 2026-11-20.
  - A chosen installed face keeps its ligatures as they drew: on unless turned off, kept as this build's `on`.
  - A Cascadia chosen from the machine's fonts (Code, Mono, NF, PL, CaskaydiaCove) becomes the bundled face, its size
    divided by 1.05 onto a step, as on 2026-11-20: an installed Cascadia Mono at 105 % is the bundled Mono at 100 %.
    From 2026-11-20 that ran on every start, so a Cascadia picked after it was taken away again at the next; now it
    runs once and a pick stays.
  - Every size stays as kept: code's 100 % is the same size as before.
  - From then on `knobas.code.ligatures` is `on` when on and nothing when off.
- **Both names are the bundled face.** The bundle declares Cascadia Mono and Cascadia Code, and a declared
  `@font-face` wins over an installed face of the same name, so a pick or a typed name of either is the bundled face,
  and sets the switch (Code on, Mono off). Settings › Fonts names the face drawn: the field's placeholder, *Use …*,
  the specimen's caption and the even-step hint say Cascadia Code while the switch is on. The code specimen's comment
  line shows `-> => != === |-- ┌─┐`, so the switch's difference shows there.
- **Licences:** Settings › Fonts › *Fonts that come with knobas* lists Inter, Cascadia Mono (Code) and Cascadia Code
  (Code, with ligatures on), each licence's text a click away; `tf-css.test.ts` holds the files to their `SOURCE.md`
  hashes and the notice's reserved name. There is still no third-party notices page beyond that list.

## Sizes, before and after

Nothing about the drawn size changed: the same `size-adjust`, the same metrics, the same rules. Computed from the
metrics (cap height 1420 and x-height 1060 of 2048, at the font size times 1.05) and the code views' line heights:

| Where | Font size, line height | Cap height | x-height | Line height | At 2560x1440@1.25 (device px) |
|---|---|---|---|---|---|
| A file's source, a diff (`.tf-code`, `.tf-diff`) | 13 px / 1.6 | 9.47 px | 7.07 px | 20.8 px | 11.83 / 8.83 / 26.0 |
| A Docs code block (`.tf-doc pre`) | 14 px / 1.55 | 10.19 px | 7.61 px | 21.7 px | 12.74 / 9.51 / 27.1 |
| A run's log (`.tf-logv`) | 12 px / 1.55 | 8.74 px | 6.52 px | 18.6 px | 10.92 / 8.15 / 23.25 |

Before (Cascadia Code) and after (Cascadia Mono) the same at 1440x868@1 and at 2560x1440@1.25.

## Bundle

Font files before: 9 files, 718,840 bytes (Inter 340,256; Cascadia Code 378,584). After: 11 files, 1,083,032 bytes
(Cascadia Mono 364,192 added), 364,192 more on disk, plus one licence chunk of about 4.5 KB, read when opened. Loaded
at run time with the switch off, as before: Inter and one Cascadia (Mono now, 14,392 bytes less than Code).

## Measured and checked

- **Screenshots** (the orchestrator's scratch folder, `shots/cascadia-mono/`): a file's source, a run's log, a Docs
  code block, a diff and Settings › Fonts, at 1440x868@1 and 2560x1440@1.25, light and dark, three ways: `before-*`
  (Cascadia Code with ligatures, the default until now), `mono-*` (Cascadia Mono, the default now) and `code-*`
  (*Ligatures in code* turned on). The fixture draws ASCII art, box drawing and arrows there under `?fake-ascii`
  (`shell/dev/fake-ascii.ts`: the file `docs/states.txt`, its working-tree diff, the head of run #1187's test step
  and a code block on *SEPA payout retry design*). In Cascadia Code `──────────>` drew as one long arrow, `<==========`
  as a double arrow and `|--` as `├──`; in Mono each is the characters the file holds. Box lines are the same in both.
- **`pixel-same.py`**, the commit before against this one, 14 pages (Tickets, a ticket, Inbox, Code, a file's source, a
  repository's README, the working tree's diff, Builds, run #1187's log, a Docs page, Monitors, Time, Settings › Fonts
  and Appearance) at 1440x868@1, 2560x1440@1.25 and 5120x1440@1, light, without `?fake-ascii`: 42 screens, 27 the
  same, 15 changed. Every change is text in the code face where Cascadia Code had joined or swapped characters
  (`-->` in a Mermaid diff, `##` in Markdown, `::` and `==` in the log, `->` in `retry.rs`, the `+2 −0` counts beside
  a diff's files) or Settings › Fonts (the specimen, the switch, the face's name, the licence rows); one screen,
  Appearance at 1440, differs by 12 pixels of colour step 1, below the noise line. Nothing outside the code face moved.
- **Gates** on the final head (rebased on ef234688), in a clean clone: svelte-check 1,781 files, 0 errors, 0 warnings;
  vitest 430 files, 6,407 passed, 1 skipped (before that rebase, one Tickets frame test failed once under the full
  run's load and passed 3 times of 3 alone); the fonts' own files 690 passed;
  `doc-lists`, `conflict-markers` and `just check-scripts` ok. Mutants on the setting and its carrying over: 16 of 16
  killed (the start reading nothing as on, the carrying over running every start or never marking itself done, a chosen
  face losing or keeping the wrong ligatures, a bundled face keeping a stale `on`, a Cascadia's size not divided, on not
  turning to Code or not kept, a face that lands after a later press applied, the turn not waiting for the face,
  picking Code by name not setting the switch or being a choice, the drawn face misnamed, the idle loader skipping the
  wrong face, reset leaving ligatures on). Press trips `--strict` on the Mac: 324 of 324 passing.
  `testenv/desktop-witness-test.sh`: 305 checks ok.
- **Nothing moves:** `just moves` on the fixture over the pages that draw code (a file, Compare, the working tree, a
  commit, a pull request, a run, a Docs page, a ticket, Tickets' keys and Settings, by `--only`), 1x and 4x, two
  workers: 1440x868@1, 156 first opens and returns, nothing moved, no column moved; 2560x1440@1.25, 156, nothing
  moved, no column moved (on the same perf build, served as `moves.sh` serves it). `long-paths.mjs`: 200 of 200
  screens hold their paths.
- **In the real app** (the Mac, a scratch `HOME`, a debug `knobas-app` built from this head's parent's Rust, the page
  under `?fake-ipc` in its own WKWebView, driven by `testenv/ui-checks/real-window.sh` with real macOS input): the
  first start drew code in Cascadia Mono and loaded only Mono (and Menlo's stand-in); with an earlier build's `on` put
  back and the marker cleared, the next start drew Mono and cleared the `on`; a real click on *Ligatures in code*
  loaded Cascadia Code's upright and italic, then turned code to it (`--mono` Cascadia Code, ligatures `normal`, kept
  `on`), and `docs/states.txt` drew `->` and `=>` joined; after a restart code was Cascadia Code, and only Code was
  loaded. Shots `realapp-mono-file.png` and `realapp-code-file.png`, cropped to the window.
- **Owed on Windows:** the same in WebView2 under ClearType at 100, 125 and 150 % (the PC was held by other runs).
