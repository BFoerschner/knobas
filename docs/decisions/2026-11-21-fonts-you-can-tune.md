# 2026-11-21: Code rows are joined, each face is tuned, and the older Cascadia comes along

**Asked (Björn, for ux29, through the orchestrator):** on code rows at the face's own height, "Yeah fine"; then "maybe
add a setting for line height, spacing etc so I can tweak fonts to my liking? Also, maybe include also the older
version of cascadia code then, I think that'd be interesting".

Amends `2026-11-18-every-face-is-yours.md`, `2026-11-20-the-faces-knobas-brings.md` and
`2026-11-21-code-is-cascadia-mono.md`. Replaces the held box-drawing pass of the same day (`4be172fd`, never pushed):
its Settings split of bundled and installed faces is taken as it was; its face of knobas's own for box drawing, the
face measured to a chosen font and the specimen built on them are not (Björn: "wanky workarounds"). Two rulings: one
from Fable (accepted by Björn) on why boxes broke and the row height that joins them, one from Fable (xhigh) on the
knobs, written out below.

**Amended 2026-11-26** by #2516 (spec #2514: "The default code font and line height are not `Cascadia Code 2009.22`
at `1.60`"): with nothing kept, code draws in Cascadia Code 2009.22 with its ligatures, rows 1.60 of the size, which
`css/tokens.css` now draws itself. Ligatures are on until turned off (kept as `off`), the newer release is kept as
`2407.24`, and *Joined* as `joined`. A choice kept before stays as it was; a default that was kept as nothing before
reads as the new default.

## Why box drawing never joined

Every Cascadia since 2106 cuts its line-spanning glyphs for the font's own line: ascent 1900 and descent 480 of 2048,
1.22 of the size at the bundled 105 %. Its `rclt` feature, required and so always on, swaps `│` and the rest for
`.stypo` glyphs 2430 units tall (1.25 of the size). knobas drew a file's rows at 1.6 of the size (20.8 px at 13 px),
a log's at 1.55 and a short log's at 1.5, so `│` left 4 to 5 px between rows in WKWebView, Chrome, Edge and knobas's
WebView2 window alike. Cascadia 2009.22 drew `│` 1.56 em tall with no swap, so it looked joined at 1.6. At the face's
own line height (16 px at 13 px) every box, double box, `├──` tree and block joins, in Mono and in Code with ligatures,
in WKWebView and WebView2 (the box-drawing agent's shots, `shots/box-drawing/compare-*`).

## What was decided

- **Code rows are *Joined* by default:** the drawn face's own line height, rounded to whole device pixels, in every
  area that stacks code lines: a file, a diff, a hunk (outdated and PDF), a fence, the editor, a run's log, a test's
  detail, a failed step, a pull request's log, a problem, a Docs block, an OpenAPI and a JAR block. For the bundled
  faces that is 16 px at 13 px, 15 at 12 and 17 at 14 at 1x; at 1.25 a 12 px log row is 18 device pixels (14.4 px).
  `rclt` stays on. `css/tokens.css` draws it as `round(calc(var(--code-lh-face) * 1em), calc(1px / var(--dpr)))`, the
  `1em` the area's own font size, `--dpr` written on `<html>` with each scale. An installed face's line height (and its
  `│`) is measured when it is picked and kept with its name, so a start never measures; an idle measure after a start
  keeps a changed font for the next one.
- ***Open* keeps the spacing before, pixel for pixel:** it sets `--code-lh` to `initial`, so each area's
  `var(--code-lh, 1.6)` (1.55, 1.5) is the number it was.
- **Each role has knobs** (Fable's rulings 1 and 2), each a notch slider like the size's, its default notch taller, its
  figure beside it and *Back to* when off its default:

  | Role | Line spacing | Letter spacing | Text weight |
  |---|---|---|---|
  | Interface | none (it would fight rows and controls tuned around it) | −0.03 to +0.06 em in 0.01, 0 | 350, 400, 450; 400 |
  | Headings | none | −0.03 to +0.06 em, 0 | none (its rules set 600 to 800 themselves) |
  | Reading | 1.40 to 1.90 in 0.05, 1.65 | −0.03 to +0.06 em, 0 | 350, 400, 450; 400 |
  | Code | *Joined* (default), *Open*, or a notch 1.10 to 1.80 in 0.05 | −0.03 to +0.06 em, 0 | 300 to 500 in 50; 400 |

  No word spacing: neither Inter nor a monospace face gains from it. A weight under 350 would let `bolder` give bold
  only 400. Each knob is one custom property on `<html>` (`--ls-<role>`, `--wt-<role>`, `--lh-read`,
  `--code-lh-face`), absent at its default, so one change is one style recalculation and one frame, and a default draws
  what it drew.
- **Code's line spacing** is two presets, *Joined* and *Open*, with the notches beside them; a notch pressed is the
  custom value, one ratio for every code area, rounded as *Joined* is. With a preset pressed no notch is filled, and an
  arrow steps from the ratio drawn. The ring marks the tallest notch at which the drawn face's `│` still joins at 12,
  13 and 14 px times code's size on this screen: 1.20 for the current Cascadia at 1x and 1.25 (1.25 leaves a 12 px
  row of 15 px against a `│` of 14.95), 1.60 for 2009.22. Its note is one line, always there: *Joined*, "Rows are as
  tall as Cascadia Mono itself, 1.22 times the size. Box lines and trees join."; *Open*, "The spacing before: 1.6 in
  files and diffs, 1.55 in logs, 1.5 in short logs. Cascadia Mono's box lines leave gaps."; a notch, "1.40 times the
  size. Cascadia Mono's box lines join up to the ring at 1.20."
- **Letter spacing** is added to a rule's own (`calc(-.01em + var(--ls-head, 0em))`), never put in its place. Code's
  reaches the code areas and the editor only: a key, inline code and `.tf-mono` keep `normal`, as do the line numbers,
  a log's times and the editor's numbers, so no gutter column moves. Chromium draws optional ligatures only at 0, so
  code's line says "Ligatures draw only at 0. Above 0, box lines show gaps."
- **Weight** moves regular text only: `.tf`, `body` and every rule in the interface's face that set 400 read
  `var(--wt-ui, 400)`; the Docs page reads `var(--wt-read, 400)`; the code areas `var(--wt-code, 400)`. Bold stays as
  drawn. A step's name on a run's bar and a plain label on Time's day strip are measured in script with the interface's
  weight and tracking.
- **Words inside a code area keep their ratio:** a diff's file head, its banner and a comment box at 1.6, a log's step
  summary at 1.55, so 13 px words are not set on 16 px lines. The `+` that starts a comment on a diff line is the row's
  height where the row is shorter than its 18 px.
- **The windowed code lists follow:** a file's source, a diff's lines, a run's log and a JAR's class take their estimate
  from the row height now (`codeRows`, `fonts.svelte.ts`) and hear every change of it (code's size, its line spacing,
  the drawn face's line, the device scale) with the new height over the old; a change of code's letter spacing is heard
  at 1, so a wrapped log line is read again. A JAR's class had no such listener before; it has now. A JAR's text box
  is its line count times the row (`var(--code-lh, 1.6em)`).
- **The older Cascadia comes along** (Fable's ruling 7): Cascadia Mono and Cascadia Code 2009.22, four rows under
  *Comes with knobas*: "Cascadia Mono", "Cascadia Code *with ligatures*", "Cascadia Mono 2009.22 *older release*",
  "Cascadia Code 2009.22 *older release, with ligatures*". A pick sets the release and the *Ligatures in code* switch;
  the switch flips Mono and Code within the release. It loads only when chosen (the idle loader leaves the three faces
  not drawn alone) and code turns to it once it has landed. The release has no italic, so comments stand upright in it
  (`font-synthesis-style: none` while it draws): a made-up slant would lean every `│` in a comment. A pick changes no
  line spacing; the ring moves to 1.60.
- **The bundled faces have names of their own** (from the held commit): "knobas Cascadia Mono", "knobas Cascadia Code",
  "knobas Cascadia Mono 2009" and "knobas Cascadia Code 2009" in CSS, so a bundled row always draws the bundled file
  even where a font of the plain name is installed; an installed Cascadia is listed as "Cascadia Mono (installed)"
  with its version (`InstalledFont.version`, read from its `name` table), and a Cascadia WebView2 draws that the
  backend's read missed (Windows Terminal's) is found by a canvas check.
- **Storage:** whole numbers, absent at their default, a read put on the nearest notch: `knobas.<role>.ls` (hundredths
  of an em), `knobas.<role>.wt`, `knobas.read.lh` (hundredths), `knobas.code.lines` (`open` or hundredths, absent for
  *Joined*), `knobas.code.release` (`2009.22`, absent for the current release), and `knobas.code.measure` (an installed
  face's line and `│` with its family). No one-time step: nobody's look changes but code rows going to *Joined*.

## The older Cascadia

Microsoft's release v2009.22, <https://github.com/microsoft/cascadia-code/releases/download/v2009.22/CascadiaCode-2009.22.zip>
(8,217,333 bytes, SHA-256 `801bcc94b79ae46bc573c9c07feb44d581e12d15f79b176be165f00f8fdf9846`): its
`woff2/CascadiaMono.woff2` (105,444 bytes, `6bcbea7c…5929a1e`) and `woff2/CascadiaCode.woff2` (112,440 bytes,
`eaaf96df…f58df956`), variable, weights 200 to 700, byte for byte, in `app/src/lib/tf/fonts/cascadia-2009/` beside
`OFL.txt` (the tag's `LICENSE`, the same text as 2407.24's: SIL Open Font License 1.1, "Copyright (c) 2019 - Present,
Microsoft Corporation, with Reserved Font Name Cascadia Code") and `SOURCE.md`. `fsType` 0. Metrics the same as
2407.24's to the unit. Settings › Fonts › *Fonts that come with knobas* lists it with its licence, and
`docs/third-party-notices.md` points there. **Bundle:** 217,884 bytes more on disk (13 font files, 1,300,916 bytes, from 11 and 1,083,032), plus one licence chunk of about 4.5 KB read when opened; nothing more loaded at run time unless it is
chosen.

## Measured and checked

- **In the real window on the Mac** (WKWebView at 2x, a scratch `HOME`, the debug `knobas-app` of this head driven by
  `testenv/ui-checks/real-window.sh` with real macOS input, `?fake-ipc&fake-ascii`): `docs/states.txt` drew at 13 px
  rows of 16 px (*Joined*, nothing stored), its boxes, double box and trees whole; a real click on *Open* set
  `--code-lh: initial` and the file drew 20.8 px rows again; *Joined* clicked back, then the field opened and *Cascadia
  Mono 2009.22* clicked in the list: the release was kept, `knobas Cascadia Mono 2009` loaded and code turned to it, rows
  16 px, comments upright; after a restart it drew at once and was the only Cascadia loaded. Shots
  `rw-*.png` in the orchestrator's `shots/font-tuning/`, the window alone.
- **`pixel-same.py`** (14 pages at 1440x868@1, 2560x1440@1.25 and 5120x1440@1, light, headless Chrome on the Mac, the
  commit before against this head): with *Open* stored, 35 of 42 the same, the 3 Settings › Fonts screens changed (the new
  knobs) and 4 under the 600 px line (a clock, a caret); with *Joined*, the default, 23 the same, 19 changed: every one a
  code area (the working tree's diff, the README's code fences, the Docs page's block, a file, a run's log) or Settings ›
  Fonts, and 1 under the line. A chip inside a Docs callout first lost the callout's inherited `.006em`; the rules that
  set no letter spacing of their own now say `var(--ls-<role>)` with no fallback, so unset it is inherited as before.
  The ASCII set (`?fake-ascii`: a file, its diff, run #1187's log, a Docs block, the Code and Reading rows of Settings ›
  Fonts) at 1440x868@1 and 2560x1440@1.25, light and dark, *Joined*, *Open* and 2009.22: `chrome-*.png` there.
- **`moves`** (the fixture at scale with late reads, 1x and 4x, 1440x868@1 and 2560x1440@1.25, on the WSL box): the code
  areas' places (a file, a commit's diff in its three modes and split, a pull request, Compare, a Docs block, each after a
  change of highlighter and after a change of line spacing, `lines-*`; the older Cascadia landing on a file; a run and its
  tabs; Settings › Fonts and Appearance; a Docs page, a rerendered file, the working tree's commits): nothing
  moved.
- **Tests:** the store, the pane, the CSS rules and the windowed lists' sources (`fonts.test.ts`, 45;
  `TfFontRole.test.svelte.ts`, 26; `code-face.test.ts` (n) to (q)); 30 mutants, each one line broken, all killed.
  Rust: `cargo fmt --check`, `clippy -p knobas-app --all-targets` clean, `code_font`'s 6 tests, the IPC mirror
  regenerated unchanged.
