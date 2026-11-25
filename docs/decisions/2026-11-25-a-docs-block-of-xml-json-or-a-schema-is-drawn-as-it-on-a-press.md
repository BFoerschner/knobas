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
