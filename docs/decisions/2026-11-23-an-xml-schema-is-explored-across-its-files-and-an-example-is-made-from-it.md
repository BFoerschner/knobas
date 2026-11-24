# 2026-11-23: An XML Schema is explored across its files, an example is made from it, and a document is checked against it

**Asked (Björn, on PR #2479):** "I want Fable to build me a renderer for .xsd Files. It should work out references automatically between files (like the opentrans, bmecat etc.) if they're findable. How could a view like that look like, i.e. it would be incredible if I could generate examples with Faker for example like that too. I only want a design that opus later can implement, but I'd like to see it first." The same day he took two proposals into it: an `.xml` file checked against its schema, and a comparison by element and type.

The judgment calls went to Fable (xhigh) under Björn's standing instruction, Björn answered its ten questions the same day ("i love this design."), and Opus built them. The architecture is [ADR-0058](../adr/0058-a-renderer-may-read-the-files-its-file-names-and-asks-beyond-the-mirror-only-on-a-press.md); this records the rulings. It builds on the API explorer's ([2026-11-22](2026-11-22-an-api-spec-is-explored-sent-from-and-opened-in-a-client.md)).

## What was there

An `.xsd` opened as Source. Nothing read XML Schema, nothing parsed XML off the page's thread, nothing validated a document.

## What the real schemas hold

openTRANS 2.1 with the BMEcat 2005, xmldsig and xmlmime files of its package (4 files, 374 KB, 848 declarations), BMEcat 2005 as BME ships it alone, UBL 2.1's invoice with the 13 files it reaches (2.8 MB, 2,843 declarations) and UN/CEFACT's Cross Industry Invoice D16B (54 files). openTRANS names its three imports by a bare file name beside it. The BMEcat file in its package declares `…/bmecat/2005` and BME's own declares `…/bmecat/2005fd`, under one file name. BMEcat and openTRANS hold no annotation at all; their only prose is each file's first comment. BMEcat's booleans, dates and countries are patterns over strings, eleven patterns are a list of words and `\w{1,50}`, and one element is forbidden by `maxOccurs="0"`.

## The rulings

1. **A `.xsd` whose first tag is a schema opens as the schema explorer**, the API explorer's pane, rail, list of fields and keys with other contents. *Render as* says *Schema*.
2. **A schema is read as a set**: the file and every file it reaches by `include`, `import`, `redefine` and `override` are one model. A name from another file carries its prefix, and following it stays in the explorer.
3. **The rail lists as a tree, by element or by type.** A root is an element nothing holds that reaches at least a fifth of what the largest reaches.
4. **How often is the one loud thing**: `1`, `0..1`, `1..∞` in the headings face, heavy where an element must stand.
5. **What an element holds is drawn merged and in order**: an extension's base before its own rows, groups opened, a choice as *One of these* with every branch a row.
6. **A referenced file is looked for in this order**: what the person pointed to or had knobas download, beside its file, the repository's XML catalog, by its name, by its namespace, another synced repository where exactly one declares the namespace, the two W3C schemas that come with knobas. It must declare the namespace asked for. An address is never fetched by itself: it is offered, and downloaded on a press.
7. **The repository's schemas are indexed only when a reference needs it**, and again when a file's content changes.
8. **A file not found here is said in place**, with its reason. *Point to a file…* and, for an address, *Download it* keep the answer in one settings row.
9. **An example is an XML document made from an element, valid first.** Its parts are *Required only* or *Everything*, a tick puts one optional row in, what may repeat stands 1 to 3 times, a choice is picked on its own line, its values are German or English, and its seed makes it again.
10. **A value is chosen in a fixed order**: fixed and default, the expected value of an enumeration, what the name and type ask for if the type takes it, a string made from the pattern, the built-in type.
11. **XML Schema's patterns are read by knobas**, matched under a step budget and made from. randexp made a valid string for 20 of the 32 patterns in the test sets.
12. **An example says it was made**: in its column's head, in a comment on its second line, and in its saved name.
13. **The check is libxml2 compiled to WebAssembly, in the worker**: on a press for an example, by itself for an XML file. It reads the set from memory and from nowhere else. Where the set is not whole, or the schema is 1.1, it says it did not check.
14. **An `.xml` file is checked in the XML view**, which is its default and has a note of its own ([2026-11-23](2026-11-23-an-xml-file-is-read-as-a-document.md)). This design gives it the schema's lookup, the check and the words of a departure.
15. **Two versions of a schema are compared by what they declare.** Formatting is no change. A fact says *stricter*, *looser* or *different* where XML Schema decides it. No verdict for the whole.
16. **On a Docs page a code block that holds a schema gains *Schema***, the code first.

## Not in this pass

Fetching from an address by itself. Sums that add up. Identity constraints and assertions honoured by an example. XML Schema 1.1 checked. WSDL, RELAX NG, Schematron. A verdict for a comparison.

## Measured

On a prototype with the app's stylesheets in headless Chrome on the WSL box, the page opened as a file, median ms at 1x / 4x / 6x at 1440 x 868: openTRANS with BMEcat read 44 / 68 / 101 and drawn 8 / 28 / 38, UBL's invoice read 84 / 158 / 237, an element's page from the rail 7 / 30 / 41, a name followed into another file 9 / 52 / 53, a filter key 4 / 23 / 21, a row opened in place 7 / 40 / 56, an example of 32 lines 4 / 18 / 32 and of 1,784 lines 27 / 129 / 198, libxml2's judgment of those 1,784 lines 6 / 22 / 34. Every example of every element of the three large sets, with required parts and with everything, passed libxml2: 5,618 of 5,618. The build's own numbers are under *Built* below.

## Reversing it

Each part stands alone. The explorer: take `XSD` out of the registry and a `.xsd` opens as Source. Examples: the example column is drawn only where the worker answers `example`. The check: drop the `libxml2-wasm` dependency and the *Check* button with it. The check of an XML file is the XML view's to switch off. The comparison: `XSD.diff = false`. The settings row can stay; nothing else reads it.

## Built

**The explorer, S1 and S2 (2026-11-24).** The model in the schema worker (`render/xsd.ts`, `xsd-answer.ts`, `xsd.worker.ts`), read on the XML view's outline (`render/xsd-xml.ts`, with the internal subset's entities under a budget), the order of section 14 (`render/xsd-locate.ts`), the repository's index only when a reference needs it (`render/xsd-index.ts`), XML Schema's patterns read by knobas (`render/xsd-pattern.ts`), and the explorer (`tf/render/xsd/`): the registry's *Schema* ahead of the XML view, the rail by tree, element and type with the filter over a held list, the overview, a declaration's page with its merged rows, followed names and Back, addresses, Zen. The four families are fixtures in `fixtures/xsd/` with their notices; `?fake-xsd` serves them to the browser fake.

Rulings made while building:

- **Files are listed in the order they were found**, breadth first, not as a tree by who asks for whom, and the list's columns share the width by fixed parts. A tree inserted each round's files between rows already drawn, and a column sized by its text widened when *reading…* became *imported, beside it · 1,234*: UBL's overview moved 13 to 16 px as its files landed. Found by `xsd.mjs`'s check that nothing drawn moves.
- **What stands under *Files* waits for the whole set**: *Seen while reading* and the first comment, so a landing file never moves them.
- **A file still being read shows the namespace its import asks for**, so its row is as tall then as after.
- **A page draws a screenful of its rows first** (18) and the rest 12 a frame, as the design says; what stands under the rows waits for them. UBL's invoice page from the rail was 152.8 ms at 6x on 2560 x 1440 at 1.25 drawn whole, against 150.
- **The explorer's rail and field rows are the schema's own components on the API explorer's classes and its windowed list.** The pane's layout moved to `tf/render/explorer/layout.ts` and is shared; merging the two rails and field grids into one component was left, so the shipped API explorer is unchanged.
- **Comparing schemas (S7), the Docs block and the inline form (D1)** are not in this pass: `XSD.diff` is `false`.

Measured on the built app (`testenv/perf/xsd.mjs`, `?fake-ipc&fake-xml&fake-xsd`, headless Chrome on the WSL box under the pc ticket, median of five, ms at 1x / 6x; lines 100 / 300 for a page and 100 / 150 for a press). At 1440 x 868 at 1: openTRANS opened from the README to its overview 41 / 89, opened again 24 / 93, UBL's invoice 28 / 167; ORDER's page from the rail 24 / 54, UBL's Invoice page 28 / 144; a name followed into BMEcat 26 / 68 and Back 25 / 63; ORDER_HEADER opened in place 7 / 20; 157 values from the address 42 / 130; a filter key over UBL's 2,843 names 3 / 20; BMEcat's fold of 440 5 / 30; an arrow key held 30 rows in UBL's rail worst frame 17 / 17, no long task; the rail scrolled end to end 61 fps. At 2560 x 1440 at 1.25: the opens 44 / 98, 34 / 100, 27 / 187; UBL's Invoice page 25 / 132 (1920 x 1080 at 2: 26 / 115); the rest within the same lines. Nothing drawn moved in any open.
