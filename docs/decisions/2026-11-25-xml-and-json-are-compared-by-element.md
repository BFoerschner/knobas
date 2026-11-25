# 2026-11-25: XML and JSON are compared by element

**Asked (Björn, 2026-11-24):** "yes", to finishing the XML, XSD and JSON design with comparisons by element. Step C1 of the compare-docs pass, as the XML view's design has it in its section 28 ([2026-11-23](2026-11-23-an-xml-file-is-read-as-a-document.md)), on S7's list ([2026-11-25](2026-11-25-a-schema-is-compared-by-what-it-declares.md)).

## What it is

- **An XML or JSON file in a comparison offers *Rendered*** beside *Lines*, which stays the default. The XML view and the JSON view now draw comparisons (`diff: true`). A file is planned from its name, as a schema is, except where the diff ends at the checkout and the side is read to know it for an API spec first.
- ***Rendered* is S7's list** (`tf/render/doc/TfDocDiff.svelte`, the same rows and classes): *1 element changed, 0 added, 0 removed, 0 moved. 1,581 read the same, however they are written.* Then a row a part, at its path: `/ORDER/ORDER_ITEM_LIST/ORDER_ITEM[1]/.../QUANTITY` (XPath steps, as the document writes its names) or `.orders[0].status` (a jq path). *Changed* says both values; *Added* and *Removed* name the topmost part that came or went; *Moved* says its place among its siblings (*3rd → 1st*). Opened, a row says each value before and after and its lines.
- **Formatting is no change**: indentation and white space, spaces around text, attribute order, quotes, a prefix spelled differently (a name is its namespace and local name), `<a/>` against `<a></a>`, a JSON object's key order and spacing, a number written `25.0` or `25`. A re-indented POM says *The same document on both sides* where its lines said every one changed.
- **`n` and `p`** step through the rows, opening each.
- **A side that is not well formed** (or not JSON, or over 8 MB) says so, and Lines has the change.

## How the parts are matched

`render/doc-compare.ts`, in the document worker, on both sides' outlines:

1. Level by level, a part's children are matched: a JSON member by its key; then by what names a part (an element's `id`, `key`, `name` or `code` attribute, an object item's field of those names); then whole, a subtree the same on both sides wherever it stands; then what is left, of each name, in order.
2. *Moved* is a matched element or array item outside the longest run that kept its order. An object's members never move.
3. The rows stand in the document's order on the side after; what went comes after what stands.

## Departures from the design

- **XML and JSON are one step.** The brief had them as C1 XML and C1 JSON; they share the comparison and the list, so they were built and checked together.
- **No *stricter* or *looser*** on a document's change: XML Schema decides those for a schema, and nothing decides them for a document.
- **The rows are cut at 600** and the comparison at 2,000 answered: *And N more: Lines has the whole change.*

## Checked by

- `doc-compare.test.ts`: formatting in both kinds, a changed value and attribute with both values, a part added or removed at its top, moved, repeated parts matched by id, whole and in order, ids that tell items apart that each changed, a removed attribute, a side not well formed, a file only after.
- `TfDocDiff.test.svelte.ts`: a re-indented POM the same, a changed one by path opened to both values, JSON by jq path, `n`, a side that is not JSON.
- `TfDiffView.rendered.test.svelte.ts`, `TfDiffRendered.test.svelte.ts`: an `.xml` and a `.json` offer *Rendered* by their names; a JSON file read for a spec's sniff is never read by a forge's request.
- Probes in `testenv/perf/doc.mjs` (`compare-order`, `compare-pom`, `compare-json`) on `?fake-doc-diff`; `moves.mjs` `diff-doc-rendered`.
