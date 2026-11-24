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

## Not in this pass

An XPath step in file pipelines. XML and JSON compared by element. JSON Schema. YAML. Folds across restarts. A query in an address. Thumbnails.

## Measured

On a prototype with the app's stylesheets in headless Chrome on the WSL box, the page opened as a file, median ms at 1x / 4x / 6x at 1440 x 868: an order of 1,812 lines opened 49 / 104 / 156, a fold 6 / 19 / 28, three levels 11 / 22 / 30, a line picked with its declaration 21 / 36 / 56, a departure stepped to 23 / 35 / 54, a query key 6 / 24 / 37, the attachment decoded and painted 15 / 22 / 33. A made order of 50 MB: the outline in 852 ms and 97 MB, `//ORDER_ITEM[QUANTITY > 10]/LINE_ITEM_ID` in 157 ms. 6,178 of 6,181 XPath expressions answered as libxml2 answers them; the three others are libxml2 leaving XPath 1.0. Of 2,387 values in the order, 6 are taken for something, all six rightly, two of them with nothing declared.

The build, `testenv/perf/doc.mjs` on the built app (`?fake-ipc&fake-xml`) in headless Chrome on the WSL box under the pc ticket, median of five, ms at 1x / 4x / 6x, lines 100 / 200 / 300 for a page and 100 / 100 / 200 for a press. At 1440 x 868 at 1: the order opened from the README to its carets 37 / 86 / 107 and to its first lines 24 / 84 / 120; opened again 1 / 3 / 5; the ORDER_HEADER folded by its caret 12 / 61 / 100; `−` 9 / 12 / 18; the `3` key 13 / 46 / 82; a query key 27 / 25 / 30 (`//ORDER_ITEM[QUANTITY > 10]/LINE_ITEM_ID`), 27 / 36 / 61 (`contains(., 'https://')`), 26 / 47 / 72 (`//*`); the attachment's chip to the PDF's first page 39 / 70 / 103. An arrow key held 30 lines: worst frame 17 / 18 / 17, no task over 50 ms. At 2560 x 1440 at 1.25 the same within the lines: the order to its carets 46 / 115 / 189, the `3` key 19 / 92 / 142, `//*` 31 / 77 / –. **Over its line:** the made 50 MB order opened to its first lines, 495 / 1,909 / 2,654 at 1440 (512 / 1,875 at 2560); its carets, the worker's outline, land with them (483 / 1,860 / 2,708). Scrolled end to end it holds 60 frames a second, no task over 50 ms. The 6x cases at 2560 after `//*` were not run (the pc ticket's turn ended).

**The real app.** The Linux build under its own Xvfb display on the WSL box, a scratch profile with the live Gitea as its source, real input through XTest: `orders/order-4711.xml` in `tidewater/ledger-api` opened in the XML view, its schema named and *Not checked*; *Levels* folded by the stepper and by `3`, `−`, `0` and `Shift →`; `//ORDER_ITEM[QUANTITY > 10]/LINE_ITEM_ID` answered 2 nodes on lines 706 and 1,274 in 1 ms; `Space` on the attachment showed the PDF, and *Keep for this field* kept the chain as *MIME_DATA as PDF*, the chip at the line's end then saying so; the undeclared thumbnail opened as a PNG and the audit value as the gzip's text; the image at an address was offered, and *Load it* said the host could not be found (`.example`); `W` on a line with an address and a path offered both, and the address went to the system's opener; *Zen* drew the view alone.

## Reversing it

Each part stands alone. The view: take `XML` and `JSON_VIEW` out of the registry and the files open as Source. The check in it: the setting, or `xml.check` off. Links and images: the view draws plain text where `doc-links.ts` answers none. Values: the chips and the menu rows are drawn only where `value-guess.ts` and the value thing answer; the new `PipeStep` variants can stay. The query: the rail's field. jq's helper: the three commands.
