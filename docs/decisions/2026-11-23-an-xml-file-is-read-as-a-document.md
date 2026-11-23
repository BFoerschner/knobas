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

Waits for the next pass: the schema's check in the view (X2, with the schema explorer's S1 to S5), the JSON view and jq (J1, J2), comparison by element (C1), the Docs block and the file viewer's inline form (D1).

## Not in this pass

An XPath step in file pipelines. XML and JSON compared by element. JSON Schema. YAML. Folds across restarts. A query in an address. Thumbnails.

## Measured

On a prototype with the app's stylesheets in headless Chrome on the WSL box, the page opened as a file, median ms at 1x / 4x / 6x at 1440 x 868: an order of 1,812 lines opened 49 / 104 / 156, a fold 6 / 19 / 28, three levels 11 / 22 / 30, a line picked with its declaration 21 / 36 / 56, a departure stepped to 23 / 35 / 54, a query key 6 / 24 / 37, the attachment decoded and painted 15 / 22 / 33. A made order of 50 MB: the outline in 852 ms and 97 MB, `//ORDER_ITEM[QUANTITY > 10]/LINE_ITEM_ID` in 157 ms. 6,178 of 6,181 XPath expressions answered as libxml2 answers them; the three others are libxml2 leaving XPath 1.0. Of 2,387 values in the order, 6 are taken for something, all six rightly, two of them with nothing declared. ⟨The build adds its probes' numbers here.⟩

## Reversing it

Each part stands alone. The view: take `XML` and `JSON_VIEW` out of the registry and the files open as Source. The check in it: the setting, or `xml.check` off. Links and images: the view draws plain text where `doc-links.ts` answers none. Values: the chips and the menu rows are drawn only where `value-guess.ts` and the value thing answer; the new `PipeStep` variants can stay. The query: the rail's field. jq's helper: the three commands.
