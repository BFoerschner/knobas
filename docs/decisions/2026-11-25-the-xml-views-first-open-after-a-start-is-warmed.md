# The XML view's first open after a start is warmed, and its check reads its set in one round (2026-11-25)

The check's first verdict after a restart (`order-4711.xml` against openTRANS with BMEcat) landed at about 694 ms at 6x on
2560 x 1440 at 1.25 on the WSL box, its carets at about 520, against 300 for a page. Reopens and second documents were
within their line since the kept sets (`2026-11-24-the-check-keeps-its-schema-set-whole.md`). Björn said yes to a pass on
the first open (2026-11-25); its choices were Fable's (below). Nothing drawn looks different, and nothing moves.

## What the first open paid for

Traced in headless Chrome on the Mac (6x, 2560 x 1440 at 1.25, marks in the page and in both workers):

- **Each worker's start.** Chrome's CPU throttle does not slow a worker, but its start waits on the page's thread: the
  document worker and the schema worker each answered their first message some 190 ms after they were made at 6x
  (under 40 at 1x). The outline landed at about 300 ms, so the carets did too.
- **The view's code, run for the first time**: its mount alone an 85 ms task at 6x (a reopen draws its first lines some
  90 ms sooner).
- **The check's chain, after the outline**: the Settings read, the open, two rounds of reads (one a level of the set),
  libxml2's load (about 18 ms), the compile (24) and the check (10), and the departures placed. Each step waited for
  the page's thread, busy drawing the lines and the carets, 20 to 70 ms a step at 6x.
- **The probe's own polling** costs some 30 ms of the page's thread at 6x in every case alike; the numbers below carry it.

## What changed

- **The view is warmed at idle (`tf/render/doc/doc-warm.ts`)**, after the sections built ahead (`aheadBuilt`) and once the
  reader is still and the window idle (`quietAndIdle`), each step waiting its turn, once a session:
  - *Every session:* the view's chunk is loaded and the view drawn off screen, hidden and inert, 240 px high, on a
    made-up 123-line order with no schema, so the document worker and the highlighter are up and the view's code has
    run. It comes down once its outline has painted, at the reader's first input, or when a document is opened for real
    (off the page at once, unmounted after that view's first frame).
  - *Where this profile checked a document in the last 30 days* (the hint store below): the check's Settings are read,
    so the first check starts on them (its verdict drawn only where they read the same again), and the schema worker
    checks a made-up order against a made-up schema held in memory, which loads libxml2. No file is read, nothing is
    kept, the session is closed. A profile that never checks keeps libxml2 cold, as D2 and Graphviz stay.
  - A document opened before a step has run cancels what is left. The module is not in the start's chunks.
- **The paths a set read are a hint across restarts (`render/xsd-hint.ts`)**, in the webview's storage under one versioned
  key: per repository, entry path and a hash of the entry's text; 32 entries, the least recently written dropped first;
  32 paths an entry, a larger set not hinted; 30 days. Only paths the walk makes are taken back (in the repository,
  `@repo/…`, `@download/…`, `@knobas/…`), never an address. It is written only where the reads were pinned and the set
  checked whole. The next first open reads the hinted files in its first round, through the same reads at the same pin;
  the worker holds them aside and takes one only when its own walk asks for that path, so any hint, right, stale, too
  wide or garbage, gives the same index, kept set and verdict as none (`xsd-hinted.test.ts`).
- **The document crosses to the schema worker once, with the open**, and the round that leaves the set whole answers the
  verdict (`Opened.checked`): no trip of its own. A kept set never holds the document it was checked with (found by
  `checked-other`, which drew order-4711's verdict on order-4710 before this was fixed).
- **A check starts on Settings held**, a kept set's or not; it still reads no file before the outline has landed.

Decided by Fable (2026-11-25): the two tiers and their gate, the hint's key, bounds and path rule, and that reads beside
the outline stay only at no caret cost. **Tried and dropped:** starting the check's reads beside the outline once the
schema worker is up. Twenty runs each, alternating, at 2560 x 1440 at 1.25: no sooner at 1x (verdict 122 against 124)
and within the runs' spread at 6x, so the reads stay behind the outline as the 2026-11-24 note had them.

## Measured

`testenv/perf/doc.mjs` on the Mac (WSL was down), the built app with `?fake-xsd` in headless Chrome, under the mac
queue; median of five, ms to first lines / carets / verdict, lines 100 at 1x and 300 at 6x. `checked-first` now models a
restart: the order checked once, knobas started again on the same storage, idle until warmed (6 s on a build without the
warm-up), then the order opened from the README. `checked-cold` opens the README straight after a start on empty storage
and presses as soon as it shows (before any warming); `checked-midwarm` presses while the hidden view stands.

| 2560 x 1440 at 1.25 | before (`c102851d`) | after (`e7dd44d6`) |
|---|---|---|
| First open after a restart, 6x, quiet Mac | 215 / 332 / 433 (a fresh page) | 180 to 187 / 245 to 253 / 285 to 296 |
| First open after a restart, 6x, Mac in use | 278 / 423 / 568 | 256 / 349 / 414 |
| First open after a restart, 1x, Mac in use | 57 / 106 / 193 | 50 / 56 / 133 |
| Pressed before any warming, 6x, Mac in use | 531 / 887 / 1172 | 550 / 938 / 1140 |
| Pressed before any warming, 1x, Mac in use | 87 / 141 / 263 | 93 / 150 / 263 |
| Pressed while the hidden view stands, 6x, quiet | about 295 / 414 / 566 at the same moment | 297 / 346 / 480 (Mac in use: 372 / 473 / 659) |
| Opened again, 6x, Mac in use | 186 / 256 / 256 | 177 / 238 / 238 |
| Opened again, 1x, Mac in use | 42 / 42 / 43 | 37 / 46 / 49 |
| The other order, 6x, Mac in use | 163 / 223 / 223 | 162 / 225 / 225 |
| The other order, 1x, Mac in use | 38 / 41 / 47 | 33 / 37 / 48 |

| 1440 x 868 at 1 (Mac in use) | before (`c102851d`) | after (`e7dd44d6`) |
|---|---|---|
| First open after a restart, 6x | 256 / 383 / 510 | 161 / 220 / 278 |
| First open after a restart, 1x | 48 / 82 / 172 | 37 / 50 / 133 |
| Pressed before any warming, 6x | 533 / 781 / 1201 | 458 / 776 / 1067 |
| Pressed before any warming, 1x | 59 / 134 / 239 | 76 / 132 / 237 |
| Pressed while the hidden view stands, 6x / 1x | — | 307 / 348 / 501 (1x: 33 / 68 / 163) |
| Opened again, 6x / 1x | 110 / 154 / 154 (1x: 25 / 38 / 41) | 114 / 154 / 154 (1x: 34 / 36 / 51) |
| The other order, 6x / 1x | 105 / 142 / 142 (1x: 32 / 46 / 46) | 105 / 145 / 145 (1x: 29 / 33 / 46) |

Twenty alternating runs at 6x on a quiet Mac put the 2560 x 1440 at 1.25 restart case at 182 to 187 / 248 to 249 / 290 to
292.

- **`xsd.mjs` before (`c102851d`) and after (`e7dd44d6`)**, at 1x and 6x, both 1440 x 868 and 2560 x 1440 at 1.25: all
  22 cases pass (`ok`, 0 over) at every size and speed. At 2560 x 1440 at 1.25 6x: `openTRANS` 83 → 79 ms, `UBL's
  invoice` 83 → 83 ms, `Check on ORDER's example` 34 → 26 ms, `Open in XML view` 200 → 202 ms, `bmecat:dtCURRENCIES`
  148 → 131 ms, rail arrow hold 17.8 ms (0 ms longest task), rail scroll 60.8 fps (0 ms longest task).
- **Nothing moved** (`moves.sh --fixture-only --throttle 1,6 --sizes 2560x1440@1.25 --only xml-checked,xml-plain,xsd-open`):
  nothing moved.
- **Long paths** (`long-paths.mjs` at 1100 x 600, all 8 XML/XSD/JSON/docs/viewer screens): 8 of 8 hold their paths.
- **Mutants** (34 across `xsd-kept.ts`, `xsd-hint.ts`, `xsd-answer.ts`, `xsd-port.ts`, `doc-check.svelte.ts`,
  `doc-warm.ts`): 34 of 34 killed.
- **Within the line on the Mac** at 6x on 1440 x 868 (even with the Mac in use: 161 / 220 / 278) and on 2560 x 1440 at
  1.25 on a quiet machine (180 to 187 / 245 to 253 / 285 to 296). The Mac runs this probe about 1.6 times faster than
  the WSL box, so on the WSL box 2560 x 1440 at 1.25 is about 290 / 400 / 470: **the carets and the verdict stay over
  300 there.** What is left is the view's own: its mount with its forced layouts, the highlighter's answers and the
  outline's second paint, then the set's parse and compile in the worker (some 50 ms, the same at every throttle).
  Keeping a parse or a compile across restarts was ruled out; the view's mount is the next target.
- **1x** stays over its 100 for the verdict (133, from 172 to 193): the worker's own parse, compile and check.
- **Memory** (12 s after inbox load, after GC, hidden view down): main JS heap 25.2 MB before vs 25.5 MB after (+0.3 MB)
  whether or not the profile checked lately; renderer RSS sum 665 MB before, 661 MB after on a profile that never checks
  (Tier A only), and 676 MB (+15 MB for libxml2) on a profile that checked lately (Tier A + Tier B).
- **Owed:** the real app check (WSL and live Gitea were down, and Björn was active on the Mac throughout so no visible
  window was launched through `mac-window-guard.sh`), and the WSL/PC 6x numbers.

## Reversing it

Drop the `warmXmlView` line in `TfShell.svelte` (the view loads with the first document again), and pass no `hint` and no
`xml` to `XsdPort.open` in `doc-check.svelte.ts`; the store's key `knobas.xsd-set-hint.v1` can stay, it is read nowhere
else.
