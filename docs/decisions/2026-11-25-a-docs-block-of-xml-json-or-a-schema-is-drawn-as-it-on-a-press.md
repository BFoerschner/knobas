# 2026-11-25: A Docs block of XML, JSON or a schema is drawn as one on a press, and the file viewer folds XML and JSON

**Asked (Björn, 2026-11-24):** "yes", to finishing the XML, XSD and JSON design with the Docs blocks. Step D1 of the compare-docs pass: the XML view's design, section 28, and the XSD design's, section 28.

## What it is

- **A Docs page's code block** whose text is a schema gains ***Schema*** in its bar, beside *Copy*, at any length. One of 40 lines and more whose text is XML or JSON gains ***XML view*** or ***JSON view***. Its language must say so (`xml`, `xsd`; `json`, `jsonc`) or say nothing, and the sniff must agree; a block another language names is never read. An API spec keeps *API*, and a block offers one view, not two.
- **The block starts as the code its author wrote.** Pressed, the button reads *Source*.
  - ***XML view* or *JSON view***: the XML view's inline form, from E1. It has the lines with their folds and keys, *Levels*, links and value chips, but no rail, no check and no query: a Docs page reads no other file. It stands in a box as tall as the code was, 420 px at most.
  - ***Schema***: the block's own declarations in a box 420 px tall that scrolls inside itself (`TfXsdInline`). A row is a name and its kind, and opens what it holds in place, as the explorer's page draws it. A name from another file is drawn as written. There is no example and no check.
- **The file viewer dialog** draws text that is XML or JSON by the inline form: a ticket's `.xml` attachment folds, and so do a run's `junit.xml` and a value pipeline's XML.

## Departures

- **The inline form is the XML view's own document** (`TfDocView` with `inline`), with its worker session. The design named `TfDocInline` on an outline with no session of its own. One component keeps the keys and values the same everywhere.
- **A schema block of thousands of declarations** draws its first 400 and says how many more.

## Checked by

`doc-spec.test.ts` (the sniff: languages, the 40 lines, the schema at any length, the answer kept by language and text), `TfDocs.test.svelte.ts` (the five blocks' bars; *Schema* opened to a declaration; *XML view* in its box and back), `TfFileViewer.test.svelte.ts` (an XML and a JSON attachment fold, plain text stays lines), `TfRunArtifacts.test.svelte.ts` (a run's `junit.xml` in the inline form).

## The checks its build could not run (2026-11-25)

- **Fixtures.** The dev fake's *EDI message formats* (`confluence:edi-formats`, `fake-doc-blocks.ts`): UN/CEFACT's Unqualified Data Type D16B as published (160 lines, *Schema*), an export declaration and its status hook (61 lines each, *XML view* and *JSON view*) and an acknowledgement of 14 lines that stays code. PAY-228 carries a `junit.xml` and a JSON answer for the file viewer. `fake-features.test.ts` holds the page to those four answers.
- **The views' code loads ahead.** Pressed first in a session, *XML view* reached its carets in 283 ms at CPU 6x at 2560 x 1440 at 1.25 (305 under load), and 363 through real input in `moves.mjs`, against 162 once its code was in. A page whose block offers *XML view*, *JSON view* or *Schema* now loads that view's code once the reader has been still and the window is idle (`loadAhead`, `doc-module.ts`), and the file viewer loads it with the press that opens a file named as XML or JSON, beside the read of its bytes. After it: 263 and 295.
- **Probes** (`testenv/perf/docs-blocks.mjs`, headless Chrome on the WSL box under the pc ticket, median of five, ms at 1x / 6x; a view pressed open is a page, 100 / 300, a fold, a key or *Source* a press, 100 / 150). At 1440 x 868: the page opened first to its blocks' buttons 56 / 229, again 45 / 225; *Schema* 26 / 55; *XML view* to its carets 27 / 145, first in a session 59 / 258; *JSON view* 26 / 128, first 59 / 225; a declaration in place 29 / 38; *Source* 12 / 64; `←` on a picked element 11 / 32; `1` 12 / 30; a caret 12 / 30; the viewer's `junit.xml` to its carets 26 / 136, first 60 / 246; its suite folded 13 / 21; `2` 12 / 26. At 2560 x 1440 at 1.25: the page 71 / 228, *XML view* 43 / 162, first 79 / 263, *JSON view* first 67 / 251, the viewer first 61 / 246, the rest within the same lines. At 1920 x 1080 at 2, 6x: *XML view* first 256, *JSON view* first 234, the viewer first 258. Nothing watched moved.
- **Nothing moves** (`moves.mjs` at 2560 x 1440 at 1.25, 1x and 6x): `docs-blocks`, `docs-blocks-schema`, `-xml`, `-json`, `-decl`, and `viewer-junit`, `viewer-json` with the bytes read 300 ms late.
- **Long paths** at 1100 x 600 (`long-paths.mjs`): `docs-blocks` (the three views open at once), `docs-blocks-decl` and `viewer-junit`, every control whole.
- **Found, not D1's:** a Docs page opened from another keeps the other page drawn until its own read lands (`TfDocs.svelte` leaves `page` as it was when the target changes). With the read held 300 ms, the other page's crumb and headings stand where the new page's draw. The other Docs places in `moves.mjs` read at once, as the backend's local read nearly does, and so does `docs-blocks`.
