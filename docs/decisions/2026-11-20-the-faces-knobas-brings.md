# 2026-11-20: The faces knobas brings are Inter and Cascadia Code

**Asked (Björn, for ux25, through the orchestrator):** "use Cascadia Code (with 105% i think what shows it as proper
hight right now, make it 100% for this then) for code, enable ligatures by default, and find proper modern ui fonts you
use for everything else. I want the app to look nice, and i think archivo and source serif right now are making the
software look older and more unusable than it needs to. Make sure we can include them in the app with the license,
commercially and non-commercially"

## What was decided

- **Code: Cascadia Code 2407.24**, Microsoft's own woff2 files from the release
  (`CascadiaCode-2407.24.zip`, SHA-256 `e67a68ee…c8474`), the variable upright and italic, weights 200 to 700, byte
  for byte. The italic draws comments.
- **Everything else: Inter 4.1**, the variable face with `wght` 100 to 900 and `opsz` 14 to 32, for Interface,
  Headings and Reading. One family: the optical size axis draws an 11 to 15 px list in Inter's text cut and the
  24 to 64 px titles and figures in its display cut by itself, so Headings needs no second family, and Docs in it read
  as the rest of the app does.
- **Archivo, Source Serif 4 and JetBrains Mono leave the bundle** and `package.json`. Settings › Fonts lists the fonts
  installed on the machine, never bundled ones, so keeping them selectable would have cost their files; anyone who
  wants them back installs them and picks them there.
- **Ligatures in code are on until turned off.** Turning them off is kept as `off`, so it stays off. Before, *on* was
  kept and *off* was nothing kept, so an *off* chosen by hand left no trace and reads as the new default: that is
  everyone, and the switch is one press.
- **Code's 100 % is Cascadia at 105 %.** The bundled Cascadia's `@font-face` says `size-adjust: 105%`: its lower case
  (.518 em) drawn at 105 % is .544 em, as tall as JetBrains Mono's .55, which is why 105 % looked right to Björn. Only
  the glyphs grow; a code view's line height is a number of its font size, so every row, gutter and windowed list keeps
  its height, and the Fonts slider reads 100 %. A Cascadia picked from the machine's fonts before this (Cascadia Code,
  Mono, NF, PL, or Nerd Fonts' CaskaydiaCove) becomes the bundled one on the next start, its size divided by 1.05 onto
  the nearest step: Björn's 105 % is 100 %, a Cascadia kept at 100 % is 95 % and draws as large as it did. Any other
  chosen face keeps its size; a person on the old default (JetBrains Mono) at a size gets Cascadia at that size, whose
  lower case is as tall. A CaskaydiaCove Nerd Font chosen for its icons is one pick away in the list again.

## Choosing the faces

Shown side by side on the real pages (Tickets' board, a ticket, a Docs page, a file's source, a diff, a run, Monitors,
Settings › Fonts), at 1440x868@1 and 2560x1440@1.25, light and dark, with Cascadia for code in all three:

- **A, Inter** for all three roles. **Chosen.**
- **B, Geist** for Interface and Headings, **Literata** for Reading.
- **C, Instrument Sans** for Interface and Headings (its width axis, 75 to 100, keeps the narrow headings and figures),
  **Literata** for Reading.

Why A: Inter holds its letters apart at 11 to 13 px with no hinting to lean on (the large x-height, open apertures and
wide spacing it was drawn for), which matters on Windows, where Björn reads at 125 % and web fonts get no hinting;
Instrument Sans ran its words together at those sizes ("2 repos,1 open PR"), and Geist is narrower and lighter in
lists. Literata in B and C read as Source Serif does, the look Björn named as old; Docs in Inter read as the rest of the
app. Inter has tabular figures (`.tf-num`), all of German with ß and ẞ (in its latin-ext part) and the rest of Latin,
Cyrillic, Greek and Vietnamese.

What it costs the design: the job key and the big figures were Archivo at width 62, weight 800, the one loud thing.
Inter has no width axis, so they draw at its normal width, in its display cut at weight 800: still the one loud thing,
by weight now. The `"wdth"` rules and their `font-stretch` twins stay, so a chosen Headings face with a width axis or
condensed members still draws them narrow (Settings › Fonts, 2026-11-18); a long key ellipsizes on its card, as it
did for a face that cannot narrow. Archivo's opened word spacing (`--ui-letter`, `--ui-word`) went with Archivo. The
type scale and line heights stay: Inter at 13 px / 1.45 in the shell, Docs at 17.5 px / 1.65, where Inter's lower case
is a step taller than Source Serif's and its lines in the 730 px column run about as long as before.

The shots: `shots/fonts/before-*`, `dirA-*`, `dirB-*`, `dirC-*` and `after-*` in the orchestrator's scratch folder.

## Licences

Both are under the SIL Open Font License 1.1, which allows bundling, use and redistribution, commercially and not, as
long as the licence and copyright notice travel with the font and a modified font does not use a Reserved Font Name.

- **Cascadia Code**: "Copyright (c) 2019 - Present, Microsoft Corporation, with Reserved Font Name Cascadia Code."
  (`LICENSE` at the tag v2407.24). A subset or re-encoded copy is a modified version and could not be called Cascadia
  Code, so the files are Microsoft's woff2, unmodified; `fsType` 0, no restriction on embedding.
- **Inter**: "Copyright 2016 The Inter Project Authors" (the package's `LICENSE`, and Inter's own `LICENSE.txt` at the
  tag v4.1), no Reserved Font Name, so Google Fonts' split by script keeps the name; `fsType` 0. The files are
  `@fontsource-variable/inter` 5.3.0's `inter-*-opsz-normal.woff2`.

Each face's files are in `app/src/lib/tf/fonts/<face>/` beside `OFL.txt` and a `SOURCE.md` naming the source, version
and every file's SHA-256; `tf-css.test.ts` fails when a file's bytes are not what `SOURCE.md` says, or a licence is
missing. The app carries the texts: Settings › Fonts ends in *Fonts that come with knobas*, a row a face (its version,
roles and copyright holder), each unfolding to its licence's full text, read from its own chunk when opened. There was
no third-party notices page to add them to.

## Nothing moves, nothing waits

- **Inter is preloaded** from `index.html` (Vite hashes the link to the same file the CSS names) and its `@font-face`
  says `font-display: block`, so the shell's first frame is drawn in Inter and no word is drawn first in a fallback
  and again in Inter. A face that never lands draws after 3 s in the fallback, as a broken bundle would.
- **Cascadia Code** keeps `swap` with stand-ins: `local("Menlo")` and `local("Consolas")` sized to its advance at
  105 % (`size-adjust` 102.19 % and 111.9 %) and its ascent and descent (the overrides), so a line of code drawn before
  it lands is as wide as after. `shell/faces.ts` loads it and the rest of Inter's scripts while the app is idle, as
  before.
- **Bundle:** the font files were 13 files, 750,240 bytes (Archivo's three, Source Serif's six, JetBrains Mono's
  woff2 and woff at 400 and 600); now 9 files, 718,840 bytes (Inter's seven parts, 340,256; Cascadia's two, 378,584),
  31,400 bytes less, and two licence chunks of about 4.5 KB each, read when opened.

## Measured

**The first window** (`testenv/perf/first-paint.mjs`, new: a cold load of the built app under `?fake-ipc` on Tickets,
the cache off, five loads a throttle; on the Mac under its lock, headless Chrome 156; the commit before this one,
b99dfbcd, against this one). The shell's first frame with its words in it, median ms, and the layout shifts after the
first paint over the five loads:

| Size | Throttle | Before: first text frame | After | Shifts before | After |
|---|---|---|---|---|---|
| 1440x868@1 | 1x | 129 | 134 | 0 | 0 |
| 1440x868@1 | 4x | 439, again 424 | 456, again 441 | 5, again 5 of 7 | 0, 0 |
| 1440x868@1 | 6x | 691, again 617, 634, 659 | 728, again 661, 659, 667 | 5, 7, 7, 7 | 0 each time |
| 2560x1440@1.25 | 1x | 134 | 134 | 0 | 0 |
| 2560x1440@1.25 | 4x | 459 | 432 | 5 | 0 |
| 2560x1440@1.25 | 6x | 711 | 637 | 5 | 0 |

First contentful paint is the same before and after (68 to 72 ms at 1x, 140 to 152 at 4x, 200 to 224 at 6x). The first
text frame moves with the run, 15 to 45 ms either way between two runs of one build; the 6x case at 1440x868@1 was run
four times alternating, and the middle of its four medians, 646 before and 664 after, is within that. In every load the interface face
was in the first text frame, before and after. **What changed is the shift:** before, every load at 4x and 6x shifted
its layout once after the first paint (Archivo, `swap`, landing a frame late); after, none did. The headless browser
measures here, not WebView2: owed on Windows, a look at the faces at 100, 125 and 150 % in knobas's own window.

## Gates

In a clean clone, rebased on b99dfbcd: `just check-front` ok (svelte-check 1,591 files, 0 errors, 0 warnings; vitest
388 files, 5,849 passed, 1 skipped); `just check-scripts` ok. On the Mac under its lock: `long-paths.mjs` 116 of 116
screens hold their paths; `testenv/desktop-witness-test.sh` 305 checks ok. `pixel-same.py` over the eight pages above,
before and after at 1440x868@1 and 2560x1440@1.25, light and dark: all 32 changed, as every page draws text, and each
was looked at (no text clipped, cut or overlapping that was not before). New tests: the vendored files against their
`SOURCE.md` hashes and licences, Inter's and Cascadia's axes, the ligature default and its kept *off*, the Cascadia
migration (five kept names and sizes, run once), another kept face's size untouched, the licences' list and a text
read when opened.
