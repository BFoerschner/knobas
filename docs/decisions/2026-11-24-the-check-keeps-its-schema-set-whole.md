# The XML view's check keeps its schema set whole (2026-11-24)

The check's verdict (X2, `2026-11-23-an-xml-file-is-read-as-a-document.md`) landed at 320 to 350 ms at 6x on 2560 x
1440 at 1.25, against 300 for a page. Björn said yes to keeping the check's session (2026-11-24). What changed, and the
numbers before and after. Nothing drawn looks different, and nothing moves.

## The set kept whole (`render/xsd-kept.ts`)

- **The schema worker keeps the last whole sets** the check read, so a document opened again, or another document on
  the same schema, is answered whole by its open: no file is read, no round runs, and libxml2's compiled schema is the
  one already held.
- **Kept only where the reads are pinned.** The file view says what its reads stand on (`XsdReads.keep`): the
  repository and the entity, and the commit the file was read at, or on the working tree its head and git's list's
  stamp. Where there is no commit, or the file is a generated one, nothing is kept.
- **The key** is that pin, the entry (its path and text: the document's folder, its namespace, its location and its
  prefixes), the files pointed to (a download is named by its SHA-256, so *Download again* is another key), the
  namespace written bare, whether the session is lean, and the bundled schemas' version (a hash of their texts). Every
  file the set read is read at the pin, so the pin stands for them.
- **What moves under the pin is asked again** before a kept set is served: another repository's file (read at its
  branch's tip) and who else declares a namespace. An answer that differs reads the set again, and the stale set goes.
- **Kept only whole**: nothing still to read, and every file read (a read that failed may answer next time).
- **Bounded**: 4 sets and 32 MiB of their files' text, the least recently used dropped first, never one a session
  holds (each session counts as one hold; it lets go when it closes or changes its set, taking copies of what it
  shared). A commit that moves drops the sets kept for the commit before it. A pick, a download, *Download again* and
  *Forget* in Settings drop every kept set (`forgetKeptSets`).
- **The entry is named by its folder alone** (`.knobas-document-schema.xsd`, its prefixes in one order), so two
  documents of one folder that say the same of their schema ask for one set.
- **libxml2's compiled schema is kept by each file's path and text**, not by its length: a file changed to the same
  length was served the old compile before.

## Started with the view, where the set is likely kept (`tf/render/doc/doc-check.svelte.ts`, `TfDocView.svelte`)

The trace of a reopen at 6x showed the check waiting on the page's thread more than in the worker: two Settings reads
(some 50 ms), then each answer queued behind the outline's paint.

- **A check likely kept whole starts with the view**, the root read from the first tag: the same ask (pin, entry, files
  pointed to) was checked whole before. It starts on the Settings the last check read, and its verdict is drawn only
  where they read the same again; otherwise it starts again on them. Its check is asked right behind the open, so the
  worker runs libxml2 while the page draws, and it counts only where the open answers the set whole.
- **Nothing is read and nothing is drawn before the outline has landed**: the reads wait for it, and so do the head and
  the marks. A document that is not well formed draws nothing of the check.
- **Any other check starts once the outline has landed, as before.** Started beside it, the worker's start and the
  Settings reads slowed a first open's carets (523 to 569 ms at 6x on 2560 x 1440 at 1.25).
- **The departures' lines are kept by the document's text and their paths** (the last 8): the same text reads to the
  same outline, so a document checked again places its departures without asking its worker, and a valid document asks
  nothing. Its verdict and marks are drawn in the outline's own frame.
- Tried and dropped: holding the outline back until the check's lines stood. The page painted a frame of nothing new
  meanwhile, and the carets came some 100 ms later.

## Measured

`testenv/perf/doc.mjs`, the cases `checked-first` (a fresh page each run), `checked-again` (the order opened again from
the README after it was checked) and `checked-other` (order-4710, valid, after order-4711 was checked); the built app
with `?fake-xsd` in headless Chrome on the WSL box under the pc ticket, before (`6a8e9d08`) and after, one after the
other; median of five, ms to the verdict (the carets in brackets), lines 100 at 1x and 300 at 6x:

| | 1440 x 868, 1x | 1440 x 868, 6x | 2560 x 1440 at 1.25, 1x | 2560 x 1440 at 1.25, 6x |
|---|---|---|---|---|
| First open, before | 215 (84) | 573 (404) | 246 (112) | 718 (539) |
| First open, after | 212 (79) | 580 (404) | 231 (114) | 720 (548) |
| Opened again, before | 76 (29) | 271 (210) | 92 (40) | 413 to 416 (309 to 321) |
| Opened again, after | 43 (27) | 212 (212) | 51 (39) | 308 to 313 (308 to 313) |
| The other order, before | 73 (39) | 264 (168) | 92 (34) | 362 to 398 (266 to 271) |
| The other order, after | 40 (25) | 198 (198) | 42 (33) | 274 to 281 (274 to 281) |

- **Within its line now** at 6x on 2560 x 1440 at 1.25: the other order, drawn in its carets' frame. At 1440 both were
  within before, and land in their carets' frame now.
- **Still over at 6x on 2560 x 1440 at 1.25**: opened again, 308 to 313. The verdict lands with the carets now, and the
  carets are over by themselves, before and after (the outline's own paint).
- **The first open stays over** at 1x and 6x on both sizes, unchanged: it reads and parses the set (BMEcat's 250 KB),
  and loads and compiles libxml2. Nothing safe made it cheaper.
- Nothing moved (`moves.sh --fixture-only --throttle 1,6 --sizes 2560x1440@1.25 --only xml-checked,xml-plain,xsd-open`).
