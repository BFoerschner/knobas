# 2026-11-18: Every face is yours, and so is its size

**Asked (Björn, for the ux18 cut, through the orchestrator):** "for the ux18 cut I'd like a settings pane for being able
to switch all the different fonts, not just the code one and also being able to change their individual font sizes
(relative, since some fonts turn out smaller etc.)"

**Decided by Fable** (`claude-fable-5-2`, effort xhigh, on the facts and options below), followed as written. The one
rule under all of it: **a size evens one face out against the others, and Zoom makes things bigger.** Björn asked for
sizes because "some fonts turn out smaller", so text changes and the layout tuned around it holds.

## The rulings

1. **Four roles.** *Interface* (`--sans`, Archivo), *Headings* (`--head`, Archivo drawn narrow), *Reading* (`--serif`,
   Source Serif 4), *Code* (`--mono`, JetBrains Mono). Headings is its own role with its own family and size: page
   titles, section headings, every Markdown heading level and the display figures, so every rule that narrows with
   `"wdth"` belongs to it and a new Interface face never meets a width-tuned rule. Markdown's text (READMEs, pull
   request and ticket bodies) stays with Interface: it draws in Archivo today, a size belongs to a face, and moving it
   to Reading would turn every ticket and pull request serif unasked. The brand is a mark and stays Archivo. Mermaid
   diagrams, renderers' frames and PDFs are not roles.
2. **True size, not glyph size.** Glyph-only scaling (`font-size-adjust`, `@font-face size-adjust`) still changes
   advances, so lines rewrap and rows re-measure anyway, and at 130 % it crowds a 1.45 line. So every rule that sets a
   role's family sets the inherited `--fs` to that role's size, and every font size is `calc(<token> * var(--fs))`.
   Glyphs, numeric line heights and code views' rows follow; control heights, tuned row heights, padding, gaps and icons
   hold, both ways. A box that clips at 130 % would take `max(tuned, tuned × scale)`; the sweep below found none.
3. **Windowed lists:** the zoom's path, once a change. A list of lines of text (a run's log, a file's source, a diff's
   lines) takes every height it holds and its estimate times the new size over the old, onto the device's grid, and
   reads its drawn rows again: exact, because each row is a line box. Other lists keep what they hold: their
   single-line rows did not change, and a wrapped row is put right when drawn.
4. **Columns.** A column that holds one line of text in one role scales with that role, both ways: `--tf-label` is
   `calc(132px * var(--size-ui))`, line-number and key columns are times code's size. Rails, panels and dragged widths
   hold, and other text ellipsizes as a long name does today. No `ch` or `em` conversion, which cannot be pixel-same at
   100 %.
5. **Width.** Every rule that sets `"wdth" N` also sets `font-stretch: N%` (held by a test, as ligatures are): the one
   property that reaches an installed face's width axis and a family's condensed members. Archivo's `@font-face`
   declares `font-stretch: 62% 125%`, so it draws the same either way (pixel-same below). The backend says which
   families narrow (a `wdth` axis in `fvar`, or a condensed member); Headings lists them first and the rest under
   *Normal width only*. A face that cannot narrow draws at its normal width and full size: keeping Archivo for the
   figures would look like a bug to someone who just chose a heading face. The job key ellipsizes and never pushes its
   card wider.
6. **Range:** 85 to 130 % in fives, 100 % the default, the same for every role. At 130 % a 13 px line at 1.45 is
   24.5 px and still fits a 26 px row; below 85 % the 11 px step drops under 9.4 px. X-heights run from .42 em (Courier
   New) to .55 (JetBrains Mono), a ratio of 1.3, so evening needs about this much.
7. **Interface has a size too,** the face most likely swapped for a system font that draws small. It is no second
   Zoom, because only text follows it, and the pane says so once: *Size evens a face out against the others; to make
   everything bigger, use Zoom in Appearance.*
8. **No minimum hit target** is needed: no tuned height changes at any size. Code views' lines follow their text as an
   editor's do; their gutter buttons keep their size.
9. **Settings › Fonts,** a pane beside Appearance, which keeps Theme, Zoom and State colours. Kept in `localStorage`
   and on `<html>` before the first draw (`main.ts`), as the code font was: `knobas.code.font` and
   `knobas.code.ligatures` keep their keys; `knobas.ui.font`, `knobas.head.font`, `knobas.read.font`, and
   `knobas.<role>.size` a whole percent, absent for 100. The size marks the step where the chosen face's x-height
   (`OS/2.sxHeight`, which the backend now reads) matches the bundled face's; nothing sets itself.

Also from Fable: Archivo's word spacing (`.006em`, `.06em`) is two tokens a chosen interface face sets to `normal`, and
the two places script measures text (a run's step bar, the day strip's labels) take the role's size and measure again
when it changes.

## What a person sees

At the top, **the proof**: one word in each role's face (*Review*, *Sprint 14*, *Release notes*, `git rebase`) at the
display step times the role's own size, on one baseline, under a dashed rule at Archivo's x-height. Evening a face out
is making its lower case meet that rule, so the proof shows what the sizes below do, at once. It is the pane's one
loud thing.

Then the four roles, each a row: its name and what it draws; the family field (the code font's combobox made general:
installed families, the role's own kind first under a heading, a typed name taken as it is, the option under the keys
or the pointer drawn in the specimen before it is chosen, `Esc` puts the choice back), *Use Archivo* when another is
chosen; the size, a slider of ten notches with 100 % taller, the chosen one a filled bar and the evening one ringed,
the arrows stepping it, `Home`, `End` and `0`, the figure beside it, *Back to 100 %* when off it, and under it *The ring
at 110 % draws Georgia's lower case as tall as Archivo's* when that step is not the chosen one. Across the row, the
specimen, drawn by the role's own rules so it shows what the app now shows: a list row with a chip and a button for
Interface, the job key and a section heading for Headings, a Docs paragraph for Reading, three numbered lines of source
for Code with *Ligatures in code* under them. Its caption names the face and size, says when a name is not installed
and draws as the bundled face, and when a Headings face has no narrow width.

## How a change stays cheap

A change writes one custom property on `<html>` (`--size-<role>`, or `--<role>-chosen`): one style recalculation and
layout of what is shown, no page rebuilt. A page kept hidden lays out when shown again, as after a window's resize. The
lists of code's lines rescale what they hold through `onCodeSize`. At 100 % every computed size is what it was:
`calc(13px * 1)` is 13 px.

## Measured

`testenv/perf/fonts.mjs`: headless Chrome 156 on the WSL box of Björn's PC (its lock held; the PC and the box idle),
driven from the Mac over a tunnel, the windowless backend (release build) on the Mac on a fresh clone of the saved
scale database with its secrets moved out, the front end built as the probes build it. Per page, from a fresh load at
100 %: the role's size stepped to 120 % on its slider in Settings › Fonts by → as real input (from the key to the end of
the first frame after the figure changed, the median of four steps; a key's line is 100 / 100 / 200 ms); the page
shown again at 120 % against the same return at 100 % with nothing changed (a page's line, 100 / 200 / 300 ms), and the
rows that moved in the 600 ms after it; and its list scrolled by script at 3,000 px/s for 4 s at 100 % and at 120 %.

**1440x868@1**, at 1x / 4x / 6x:

| Page (role) | key ms | shown again at 100 % → 120 %, ms | scroll fps at 100 % → 120 % |
|---|---|---|---|
| Tickets' list (Interface) | 18.5 / 45.1 / 64.9 | 8.9 → 9.5 / 36.6 → 39.9 / 60.6 → 64.5 | 60 → 60 / 57.5 → 60 / 47.6 → 51 |
| Builds (Interface) | 26.4 / 49.9 / 68.3 | 12.2 → 13.2 / 61.7 → 68.7 / 97.5 → 138 | 60 → 60 / 48 → 52.4 / 34.4 → 27.9 |
| Code's tree (Interface) | 25.6 / 48.6 / 75.2 | 13.6 → 15.6 / 55.8 → 68.3 / 99 → 156 | 60 → 60 at all three |
| A Docs page (Reading) | 14.7 / 30.7 / 41.1 | 22.6 → 23.5 / 95.3 → 98.8 / 241.5 → 271 | 60 → 60 / 57.5 → 60 / 57 → 60 |

**2560x1440@1.25**, at 1x / 4x / 6x:

| Page (role) | key ms | shown again at 100 % → 120 %, ms | scroll fps at 100 % → 120 % |
|---|---|---|---|
| Tickets' list | 19.4 / 45.4 / 68.4 | 9.9 → 11.3 / 42.9 → 52.9 / 73.4 → 84.5 | 60 → 60 / 50.8 → 53 / 39.1 → 40.6 |
| Builds | 27.8 / 46.5 / 69.1 | 18 → 17.9 / 66.2 → 75.2 / 177.1 → 176.9 | 60 → 60 / 38.1 → 26.5 / 33.3 → 17.6 |
| Code's tree | 22.3 / 49.6 / 69.2 | 17.6 → 13.5 / 83.2 → 73.7 / 93.4 → 103.7 | 60 → 60 at all three |
| A Docs page | 29.9 / 32.9 / 43.9 | 37.3 → 39.1 / 224.7 → 244.7 / 346.4 → 375 | 59.8 → 60 / 54.5 → 60 / 49.5 → 59.3 |

Every key is within its line, the slowest 75 ms at 6x. Nothing moved after any page was shown again. A page shown again
lays out at its new size once: 1 to 57 ms more than the same return at 100 %. The Docs page's return at 2560x1440@1.25 is
over its line at 4x and 6x at 100 % as at 120 %, and as much on the commit before this pass (alternated, `--to 100`: 215
and 322 ms there, 220 to 228 and 304 to 337 here): it is the page's own, not this pass's.

**Nothing at 100 % costs more.** Base and head alternated, the same path with no size change (`--to 100`): Tickets'
list at 6x on 1440x868@1 scrolled at 49.8 and 47.3 fps on the base, 46.1 and 46.9 on this head; Builds at 33.8 and 37.1
against 33.3 and 33.8, its frame work p50 7.9 to 8 ms against 8 to 9.7; at 2560x1440@1.25 Builds at 4x 41.4 against 41.4
and 41.9, at 6x 30.7 against 32.6 and 34.9. Within the run-to-run spread.

**Builds' scroll at 115 % and up, before and after the list-scroll pass.** On 3f555b22, after the probe's sequence (a
scroll at 100 %, Settings and back, the slider stepped, Builds shown again), Builds scrolled at 26.5 and 17.6 fps at 4x
and 6x on 2560x1440@1.25 and 27.9 at 6x on 1440x868@1 (the tables above), where 105 % held 54. A trace named it: every
row swap restyled the runs table's whole body through `.tf-headed > header + *`, a sibling rule whose subject names
nothing, and slower frames swapped rows on more of them. The same live change traced alone did not show it, so its
start in the probe's sequence was never pinned down; letting every list drop its measured heights on a size change did
not move it and was not kept. The list-scroll pass's 01bba49f, which rewrites every such rule with a named subject,
landed under this pass; rebased on it (3d17be53), the same probe gives Builds at 120 %: 57.2 fps at 4x and 38.6 at 6x on
2560x1440@1.25 (50 and 37.7 at 100 %), and 60 and 57.8 at 4x and 6x on 1440x868@1 (60 and 41.1 at 100 %); Tickets' list
59.8 and 53.3 (59.3 and 52 at 100 %). Keys 44.5 to 71.2 ms, nothing moved.

## The sweep

Every page under `?fake-ipc` (Tickets, the Inbox, a job, a pull request and its diff, a file, a repository, Code,
Builds and a run, a Docs page and Docs' home, Monitors, Time and the standup, Settings › Fonts, Sources and
Appearance) in headless Chrome 156 on the WSL box of Björn's PC, at 1440x868@1, 2560x1440@1.25 and 5120x1440@1, with
every role at 85 % and 130 %, and with wide faces at 130 % (DejaVu Sans for the interface, Liberation Sans for headings,
which cannot narrow, DejaVu Serif, DejaVu Sans Mono) and narrow faces at 85 % (Liberation Sans Narrow, Liberation
Serif, Liberation Mono). A scan in each page listed every box whose text is clipped without an ellipsis, cut at its
foot, or spills a fixed height, against the same page at the defaults, and each page was looked at. New at 130 %:
three spills of 3 px, none clipped (a diff's *Rendered / Lines* buttons, a Docs page's *Unlink*, the specimen's job
key at a line height of 1), where the glyphs stand past a fixed box that does not clip. The first sweep found the job
card's *Sub-tasks* and *Code* labels running into their counts at 130 % with DejaVu Sans; that column, the ticket
card's facts, the Inbox's and Time's first columns now scale with Interface (ruling 4). The fake corpus names no
DejaVu or Liberation family among its installed fonts, so the captions there say the face is not installed; the
faces themselves draw, as the shots show.

## Gates

On the code head 3f555b22, in a clean clone: `just check-front` ok (svelte-check 1,554 files, 0 errors, 0 warnings;
vitest whole, 371 files, 5,638 passed, 1 skipped; again rebased on the list-scroll pass, 3d17be53: 5,640 passed); `just check-scripts` ok; `cargo fmt --all --check` ok; `just clippy`
clean; `cargo test -p knobas-app` 435 and 1,246 passed; the IPC mirror regenerated with the new `InstalledFont` fields
and committed. Thirty-four mutants, one for each family token, size, `--fs`, font size, narrowed rule, Markdown heading,
inline and bare code's size, column, the job key's ellipsis, Archivo's spacing, kept value, ratio, range, even step,
list order, specimen, slider key, ring, caption, the ligature switch's place, the Settings section, the lists of lines,
the two canvas measures, `main.ts`, the held heights and estimates, the list's re-read, and the backend's serif class and
condensed members: each failed a test. Press trips with `--strict` (the Mac's lock): 305 of 305 passing.
`testenv/desktop-witness-test.sh`: 305 checks ok. `just moves` (the fixture at 1x and 4x, 1440x868@1 and
2560x1440@1.25, 280 places with their first open and return): nothing moved. `pixel-same.py`, every page above shot at the
defaults on the commit before this pass and on this head at 1440x868@1, 2560x1440@1.25 and 5120x1440@1: only Settings ›
Fonts (new), Settings › Appearance (its code font rows gone) and the Settings rail (its new entry, which moves the rows
under it) changed; a run's step bar differed at 1440x868@1 by 4,166 px, which the commit before differs by from one
capture to the next (its labels are fitted before the bundled face lands; shot again, base and head the same); the rest
under 1,800 px with a colour step of 16 or less, or Time's and the Inbox's clock.
