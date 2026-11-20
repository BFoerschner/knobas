# What the ux23 gate left over (2026-11-20)

The ux23 timing gate, PR #2479, for ux26. Four things it found, and what each turned out to be.

## An asset's lower sections wait for its *Depends on this*

**Seen.** At 2560x1440@1.25 (4x first open, 1x return) *Linked items*, *History* and the import line moved 24 px down
17 to 67 ms after an asset's pane drew. *Depends on this* is a second read (`depends_on_this`), and its one-line
*Reading…* gave way to the *Everything inside it* fold.

**Taken.** What depends on an asset by a link can't be known before the walk, so the section has no final height to
keep before its answer. It and every section under it (*Monitoring*, the monitor names, *Create monitor*, a route's
history, *Linked items*, *History*) wait unseen in their room until the walk answers or fails, at most 500 ms
(`heldHead`, as a detail page's header, `2026-11-17-nothing-moves-once-drawn.md`), and show whole. What stands above
draws at once. The pane keeps its scroll from one asset to the next, which gave two more movers when it was pressed
scrolled to its end: the held room was *Reading…*, 4 px taller than a fold, so the scroll clamped back 4 px when the
answer drew; and Chrome kept a held *History* in place as the walk's rows came in above it, scrolling 75 px on. So
while held the panel draws no *Reading…* line (its room is the least any answer takes, and an answer only adds to
it), and a held section is never the pane's scroll anchor (`overflow-anchor: none`). Past 500 ms *Reading…* shows as
before.

**Held by** `moves.mjs`' `asset-depends-late` (an asset holding one thing that nothing links to, the gate's shape) and
`asset-depends-links-late` (one that also has a dependent by a link), on the fixture with the walk 250 ms after the
pane, pressed with the pane scrolled to its end (`scroll`, which puts the panel in view at 1440x868), and drawn once
the pressed asset's heading is (`ready`: on WSL's Chrome at 4x the press's first frame can still hold the asset it came
from). Before, at 1x
and 4x, first open and return: 8 of 8 opens moved at 1440x868@1 and 8 of 8 at 2560x1440@1.25 (4 px on the first, 56 px
on the second). After: nothing moved in 16.

## *Mark all as read*'s toast is not slower

**Seen.** runs-read's `mark-all-said` at 1x read 51.5 to 62.4 ms on ux23 against a line of 50, and ux20 49.3.

**Found.** A Chrome trace of each press (ux20 and the head, alternated, on the same backend over the saved scale
database): the write's request leaves 1 to 2 ms after the press, the backend's answer lands 17 to 25 ms after it, and
the page spends 2 to 3 ms on the main thread after that (script 1.3 to 2.1, layout 0.2 to 0.4, paint 0.6). The toast
draws in the next frame and the probe's mark lands a frame later, about 48 ms. The measure is three frames, and a write
that answers after the second frame's start (load on the backend) makes it four, about 63 ms: the gate's overs.
Alternated on a quiet Mac (load 2.5 to 3.9), seven presses each: ux20 49.2 and 49.1 ms, the head 49.9 and 49.5, both
within. Nothing in the page to cut; the toast's styles (`5ff3b6b3`, `8ab2aed9`) cost nothing measurable.

## Docs' spaces letters

`keys.mjs`' `docs-spaces-letter` counted `e`, `r` and `n` as drawing nothing. The rail's filter reads a space's name
and key, and on the saved scale database those are `Engineering ENG`, `Scale test: Engineering Handbook SCLENG`,
`… Operations Runbooks SCLOPS` and `… Product Notes SCLPRD`. The three letters are exactly the ones all four hold, so
every space stayed where it was and the probe had no frame to time: the probe, not the app. They are `g`, `d` and `p`
now, each held by some spaces and not others.

## `keys.mjs --attach` lets go

Attached to a window (knobas's WebView2 window) with no `--check`, `keys.mjs` wrote its results and never exited, and
the window kept the probe's 1440x868 size override while it lived. It now puts the CPU back, clears the size and focus
emulation, closes its socket and exits.

## Two tests that failed under the suite's load

- `folder_render::runs`' tests waited for a run with a 20 s clock and then answered whatever was drawn, so a slow second
  run read as the first one's drawing (`a_second_run_of_one_key_gets_a_folder_of_its_own`). They now wait on the run's
  told events. The wait also showed a race in the product: a printed drawing's run, and a failed one, left the slot
  before the drawing was remembered and written or the failure recorded, so an ask between saw neither a run nor its
  answer, and `a_drawing_outlives_a_restart_and_goes_with_its_source` read the disk too early (2 of 20 loaded runs). A
  run now ends once its answer is kept, as a generated folder's already did.
- The archive spool was one folder, `$TMPDIR/knobas-archives`, emptied by each process at its first use and filled with
  files numbered per process, so two knobas on one machine overwrote each other's spool files ("the archive is
  corrupt"; 14 of 40 loaded runs). Each process now spools under `knobas-archive-spool/<pid>-<nanos>`, and folders there
  untouched for twelve hours are swept at first use, as the folder renders' scratch is.

40 runs of `knobas-app`'s lib suite, four at once (load 39 to 47), after both: every run passed.

**Reversing** the first is `dependsDrawn`'s `answered` returning `true`.
