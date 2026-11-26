# 2026-11-23: An XML file is read as a document, a value through a pipeline, and JSON the same way

**Asked (Björn, on PR #2479):** "Schema yes, Checked doesn't feel right. Maybe XML-View? Honestly, maybe extend the XML-View by Code-Folding, <optional> schema check (automatically enabled if found schema) and like ability to open links in the browser, or opening images in a modal, or decrypting base64 files and displaying them in a modal? That'd be nice. Maybe have something similar to json and jq." And on the base64 modal: "maybe add the ability to render fields per pipeline, like in general? … maybe as a rightclick context? or a button."

The judgment calls went to Fable (xhigh) under Björn's standing instruction, and Opus built them. The architecture of values is [ADR-0059](../adr/0059-a-value-inside-a-file-is-read-like-a-file.md); this records the rulings. The schema explorer's design, which the check builds on, comes with its own note and ADR when the schema is built.

**Björn's answers (2026-11-23, late evening).** He chose Fable's default on all ten of the design's questions, so each is his decision, not a provisional one: *XML view* and *JSON view* in *Render as*; every XML file opens in the XML view, schema or not; a schema problem is shown at the end of its line; a query name matches as the file writes it; images from a kept host load without asking; a downloaded schema is kept in the profile, with *Download again* in Settings; folds last while knobas runs, not across restarts; `.json` files open in the JSON view; the keys as designed; a kept chain is a chip you press, and nothing decodes by itself. Each is kept in one place in the code (the registry's entries, `NAME_AS_WRITTEN` in `render/xml-xpath.ts`, `doc-keys.ts`, `KEPT_APPLIES_BY_ITSELF` in `render/value-kept.ts`), so a later answer is a one-line change.

## What was there

An `.xml` and a `.json` file opened as Source. A `.xsd`, `.wsdl` or `.csproj` had no colour. File pipelines had base64, the decompressors and jq, on whole files. Nothing folded, nothing in a file opened, and no image loaded from the web.

## The rulings

1. **An XML file opens in the XML view**: by an XML declaration at any name, or an XML ending and a start tag. It is the file's own lines in the explorers' pane, and the default.
2. **Everything the view adds stands in room held from the first frame**: the caret's column, the line's end, the rail.
3. **Folding is by line**: the caret, `←` and `→`, `Shift` for everything under. *Levels* is a stepper, `−` and `+`, as deep as the document goes, with `0` to `9` for an exact level. A fold counts what it hides. Folds are kept by element while the window lives.
4. **The schema check runs when a schema is found**: what the person pointed to or downloaded, beside the file, the catalog, by name, by namespace, another synced repository where exactly one declares it, the schemas that come with knobas. An address is offered and never fetched by itself.
5. **A departure is said at the end of its own line**, listed in the rail, and explained by the declaration beside the document. No row is put under a line. A finding is put on its line by libxml2's path to the node, which has no bound, not by its line, which is 16 bits.
6. **An address opens through `openWeb`**; `W` and `Shift W`. A path is a link only where the repository holds a file there. A namespace is a name, not a page.
7. **An image in the repository opens in the file viewer; one at an address is offered** and loads on a press, by one `GET` with no cookie, kept in memory. A host can be kept.
8. **A value is looked at in the file viewer through a pipeline**: a chip where knobas takes it for something, `Space`, or right-click. The chain is the file pipeline's bar, with seven new steps.
9. **A value pipeline is a file pipeline whose first step takes the value out of the file.** Nothing new crosses IPC.
10. **A value looks like base64 only where what it decodes to is something knobas knows**, unless the file or its schema says what it is. The bytes overrule a media type beside them.
11. **A chain can be kept for a field**, named by how its names end and a file pattern.
12. **The query is XPath 1.0, read by knobas over the outline, under a budget.** libxml2 is the witness. A name matches as the document writes it.
13. **What was asked is kept in memory while knobas runs**, and is in no address.
14. **A JSON file opens in the JSON view**, the same view, with jq. The jq is jaq, kept running for the open document.
15. **A file written on one line is laid out**, and says so.

## Built in this pass, and what waits

Built (2026-11-23): Source colours every XML ending (X0); the XML view, the document (X1); the query (X4); values and their pipelines, general across renderers (V1); links and images (X3); a file written on one line, laid out (X5).

Rulings Fable added while it was built:

- **Find in file** has no box until ⌘F, then a bar at the top of the lines that takes no room, on Source's own engine (its hits counted in the file's text, so *n of m* is exact in a windowed file); the query field is never ⌘F's.
- **Until the check is built**, the rail's head says only what knobas read: *Not checked* with the schema or the namespace the document names and *knobas does not check a document against a schema*; *No schema*; or *Not well formed* in the error ink with where. The switch shows *Findings* and *Results*; *Findings* lists the break of a broken file, else *Nothing was checked*.

Found by the probes on the built app, and fixed:

- **The view drew no lines in the app.** Its first line that reads like a path asked for the repository's paths while the lines rendered, and the repository page writes its state when asked: Svelte refused the write (`state_unsafe_mutation`). The view's tests had a repository that wrote nothing. The ask now runs after the draw, and a test whose repository writes state, as the page's does, failed before.
- **Three levels drew every row.** Source draws a file of 300 lines or fewer whole; a fold makes a long document that short, so the `3` key drew all 96 rows of the order's three levels in its frame. The view's lines now window past 64 rows, about a screenful at the largest window (`TfLines`' `wholeUpTo`; Source keeps its 300), and a folded row's caret is drawn by CSS, not an SVG. On the PC the key went from 24 / 106 / 165 ms at 1x / 4x / 6x to 13 / 46 / 82.
- **After *Keep for this field*** the dialog still offered to keep, and said nothing of the chain kept. It now says *Kept as …* and offers no second keep.

Waits for the next pass: the schema's check in the view (X2, with the schema explorer's S1 to S5), the JSON view and jq (J1, J2), comparison by element (C1), the Docs block and the file viewer's inline form (D1).

## The check, built (X2, 2026-11-24)

With the schema explorer's model (`docs/decisions/2026-11-23-an-xml-schema-is-explored-across-its-files-and-an-example-is-made-from-it.md`, S1, S2 and S5), an XML file that names its schema opens checked, by itself (sections 10 to 12).

- **The document's schema is read as the explorer reads a set**, from an entry that imports the root's namespace from the document's folder (or includes its no-namespace location), so every step of the order is the explorer's own (`tf/render/doc/doc-check.svelte.ts`). It starts once the outline has landed: started beside it, its reads and copies slowed the outline's own paint at 6x, and the verdict, which needs the outline to place its findings, came no sooner. (Since 2026-11-24 a check whose set the worker likely keeps whole starts beside it, reading nothing: `2026-11-24-the-check-keeps-its-schema-set-whole.md`.) Its session sends no declarations back to the page.
- **The rail's head** says each state of section 11 in the room it held: *Checking…*, *Valid* in `--ok`, *5 departures* in `--err`, *Not checked* in `--warn` and why, *Schema not found here* with *Point to a file…*, a schema at an address, *No schema*, checking off with *Check it*. *Schema ▾* holds *Stop checking this file* (until knobas quits), *Open the schema* and *Point to another file…*.
- **A departure stands at the end of its line**, in knobas's words, the line in the error edge and ground; the rail's *Findings* lists them, `N` and `P` step through them, and from 1,480 px a stepped-to departure stands whole beside the document over the declaration of its element, found by walking down by name (`declarationOf`). A finding is put on its line by libxml2's path to the node (`places`), never by its 16-bit line.
- **Names in a finding are written as the document writes them**: the document's prefixes, its root's namespace bare.
- **Settings › Pipelines** gains *Schemas*: *Check an XML file against its schema when one is found* (`xml.check`, on) and the schema files pointed to (`xsd.locations`). Commands `xml_check`, `set_xml_check`, `xsd_locations`, `xsd_set_locations`; no migration.

Measured on the built app (`testenv/perf/doc.mjs` with `?fake-xsd`, headless Chrome on the WSL box under the pc ticket, median of five, ms at 1x / 6x): at 1440 x 868 the order opened to its check's verdict, five departures, 61 / 239, `N` to the first departure 18 / 132; at 2560 x 1440 at 1.25 the verdict 77 / 320, `N` 20 / 142; at 1920 x 1080 at 2 the verdict 282 and `N` 153 at 6x. **Over its line**: the verdict at 6x on 2560 x 1440 at 1.25, 320 against 300 for a page; the page itself is usable from its first lines (178), and the verdict is paint on lines that stand, like the carets, which land at 269 to 327 there on the same build. `N` at 6x on 1920 x 1080 at 2 is 153 against 150. No line moved when the marks landed.

## The JSON view, built (J1, 2026-11-24)

Section 25: a JSON file opens in the same view.

- **Which files**: `*.json`, `*.geojson`, `*.webmanifest`, `*.har`, `*.ipynb`, `*.jsonl` and `*.ndjson` whose text starts with `{` or `[` (`render/json-sniff.ts`); `*.jsonc` and `*.json5` stay Source. The registry's *JSON view* stands after the API explorer, so an OpenAPI spec in JSON opens as *API* with *JSON view* next, and ahead of Source, so it is every other JSON file's default.
- **The outline** (`render/json-outline.ts`, the spike's `scanJson`) is the XML outline's columns where they mean the same, row 0 the document and the root under it; its nesting is walked with a stack of its own, so a deep file cannot overflow the reader. A line-delimited file holds a value a line. A file that is not JSON is drawn, and the head says where it stops, as for XML.
- **The model's JSON half** (`render/doc-json.ts`): an object folds counting its members, an array its items; the way down is a jq path's steps (`.`, `orders`, `[2]`, `attachments`, `[0]`, `content`); folds are kept by their jq paths; the picked value says *Object*, *Array*, *String*, *Number* or *Literal*, an object its members, an array its items by kind. **A string is a value**: its bytes inside its quotes, read as a JSON string (`TakeAs::JsonString`), its sibling members `type`, `contentType` or `mime` saying what it is, `fileName` what it is called; a field is named by its last two members, an array's items `[]` (`attachments[].content`). A one-line file is laid out, every value written as it stands.
- **The head says *Well formed* and *JSON is not checked against a schema*.** No schema is looked for; the XPath field is not drawn (jq is J2's).
- **A pipeline's JSON answer** (a `.json.gz` through *gunzip* and *jq*) is drawn by the JSON view too, the default for its name.
- **The export** (`fixtures/doc/files/tidewater/ledger-api/exports/orders-2026-11.json`, the clickdummy's) is in the seed's step 4c and `?fake-xml`.

Measured on the built app (`testenv/perf/doc.mjs`, the same setting as X2), ms at 1x / 6x: at 1440 x 868 the export opened from the README to its carets 25 / 99, its 48 orders folded by their caret 8 / 21, three levels by the key 14 / 63; at 2560 x 1440 at 1.25, 39 / 155, 6 / 26, 14 / 95. Nothing moved (`moves.mjs`, json-export, at 2560 x 1440 at 1.25, 1x and 6x).

## jq in the JSON view, built (J2, 2026-11-24)

Section 26: the JSON view asks with jq where the XML view asks with XPath, in the same field, rail and marks.

- **A helper is kept running for the document** (`crates/knobas-app/src/file_pipeline/session.rs`): the app's own binary as a child in `--knobas-jq-session` mode, the jq step's jaq, so a filter that builds a value too deep for the stack ends the child and never the app. It reads the document once; each filter after costs the filter alone. A filter has 1.5 s (20 s for a child just started, which reads the document first); past that the child is ended, the answer says so, and the next filter starts another. A session ends with its view, and after two minutes with nothing asked. Commands `json_query_open` (a file by entity and path, or text: a pipeline's answer), `json_query`, `json_query_close`; no setting, no migration.
- **What a filter answers**: its first 500 values compact, how many it gave, and the paths of what it picked from `path(filter)`, at most 5,000. A filter that picks (`.orders[] | select(.total > 2000)`) is marked as an XPath query is: *22 values on 22 lines*, the rows in jq's order, each line marked, `Enter` and `Shift Enter` stepping through them. A filter that makes values (`.orders | map(.total) | add`) has no paths: the rail lists its values and says *The filter makes new values, so nothing is marked in the document.* jq's refusals are said in its words.
- **The field is drawn only where a session can be opened**: a repository's file (read by the backend from the entity) or a pipeline's JSON answer (its text). The field says *jq*; the rail offers filters made from the document's own first list of objects and its first number.
- ***Copy ▾*** holds *Copy the filter* and **Use as this file's pipeline**, which adds the filter as a jq step to the file's pipeline, so the file opens as its answer.
- A row that holds others counts what it holds in its own words: an array its items, an object its members.

Measured on the built app (`testenv/perf/doc.mjs`, the same setting as X2; the dev fake answers in the helper's place, its session warmed by `.`), ms at 1x / 6x, lines 100 / 150 for a key: at 1440 x 868, a key that picks (`.orders[] | select(.total > 2000)`, to its 22 values marked) 28 / 66, a key that makes a value (`.orders | map(.total) | add`) 10 / 22; at 2560 x 1440 at 1.25, 28 / 39 and 11 / 20. J1's cases on the same build: the export to its carets 40 / 144 and 63 / 123, its orders folded 6 / 42 and 6 / 32. Nothing moved (`moves.mjs` at 2560 x 1440 at 1.25, 1x and 6x): `json-export`, and `json-jq-late`, a key whose answer lands 400 ms late, its rows and marks drawn in the room they hold. The helper itself (`tests/it/json_query.rs`): one child answers a picking filter, a making one and a refused one in turn; a filter that loops is stopped at 1.5 s and the next answers; a value too deep ends the child and the next filter starts another.

## Not in this pass

An XPath step in file pipelines. XML and JSON compared by element. JSON Schema. YAML. Folds across restarts. A query in an address. Thumbnails.

## Measured

On a prototype with the app's stylesheets in headless Chrome on the WSL box, the page opened as a file, median ms at 1x / 4x / 6x at 1440 x 868: an order of 1,812 lines opened 49 / 104 / 156, a fold 6 / 19 / 28, three levels 11 / 22 / 30, a line picked with its declaration 21 / 36 / 56, a departure stepped to 23 / 35 / 54, a query key 6 / 24 / 37, the attachment decoded and painted 15 / 22 / 33. A made order of 50 MB: the outline in 852 ms and 97 MB, `//ORDER_ITEM[QUANTITY > 10]/LINE_ITEM_ID` in 157 ms. 6,178 of 6,181 XPath expressions answered as libxml2 answers them; the three others are libxml2 leaving XPath 1.0. Of 2,387 values in the order, 6 are taken for something, all six rightly, two of them with nothing declared.

The build, `testenv/perf/doc.mjs` on the built app (`?fake-ipc&fake-xml`) in headless Chrome on the WSL box under the pc ticket, median of five, ms at 1x / 4x / 6x, lines 100 / 200 / 300 for a page and 100 / 100 / 200 for a press. At 1440 x 868 at 1: the order opened from the README to its carets 37 / 86 / 107 and to its first lines 24 / 84 / 120; opened again 1 / 3 / 5; the ORDER_HEADER folded by its caret 12 / 61 / 100; `−` 9 / 12 / 18; the `3` key 13 / 46 / 82; a query key 27 / 25 / 30 (`//ORDER_ITEM[QUANTITY > 10]/LINE_ITEM_ID`), 27 / 36 / 61 (`contains(., 'https://')`), 26 / 47 / 72 (`//*`); the attachment's chip to the PDF's first page 39 / 70 / 103. An arrow key held 30 lines: worst frame 17 / 18 / 17, no task over 50 ms. At 2560 x 1440 at 1.25 the same within the lines: the order to its carets 46 / 115 / 189, the `3` key 19 / 92 / 142, `//*` 31 / 77 / –. **Over its line:** the made 50 MB order opened to its first lines, 495 / 1,909 / 2,654 at 1440 (512 / 1,875 at 2560); its carets, the worker's outline, land with them (483 / 1,860 / 2,708). Scrolled end to end it holds 60 frames a second, no task over 50 ms. The 6x cases at 2560 after `//*` were not run (the pc ticket's turn ended).

**The real app.** The Linux build under its own Xvfb display on the WSL box, a scratch profile with the live Gitea as its source, real input through XTest: `orders/order-4711.xml` in `tidewater/ledger-api` opened in the XML view, its schema named and *Not checked*; *Levels* folded by the stepper and by `3`, `−`, `0` and `Shift →`; `//ORDER_ITEM[QUANTITY > 10]/LINE_ITEM_ID` answered 2 nodes on lines 706 and 1,274 in 1 ms; `Space` on the attachment showed the PDF, and *Keep for this field* kept the chain as *MIME_DATA as PDF*, the chip at the line's end then saying so; the undeclared thumbnail opened as a PNG and the audit value as the gzip's text; the image at an address was offered, and *Load it* said the host could not be found (`.example`); `W` on a line with an address and a path offered both, and the address went to the system's opener; *Zen* drew the view alone.

## Reversing it

Each part stands alone. The view: take `XML` and `JSON_VIEW` out of the registry and the files open as Source. The check in it: the setting, or `xml.check` off. Links and images: the view draws plain text where `doc-links.ts` answers none. Values: the chips and the menu rows are drawn only where `value-guess.ts` and the value thing answer; the new `PipeStep` variants can stay. The query: the rail's field. jq's helper: the three commands; without `json_query_open` the JSON view draws no field.
