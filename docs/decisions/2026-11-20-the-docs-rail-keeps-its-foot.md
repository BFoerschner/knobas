# 2026-11-20: The Docs rail keeps its foot

**Found.** The Windows pass of v3.2.9-ux23 (Björn's PC, knobas's own WebView2 window, real presses on rows under the
pointer) pressed five pages from the list of all spaces. Each opened its page, and the rail kept its four spaces, its 11
to 23 open folds and its scroll, with no *For you*. The first press did change the rail: the foot with *New page…*
arrived under it. That foot was drawn only while an open page allowed `create_page`, and the home has no open page. It took
47 px off the bottom of the rail's scroll box (full screen: the box ended at 1,339 px, then at 1,292 px), so the one or
two rows at the rail's foot left view. No row moved. Later page-to-page presses changed nothing. Going back to the home
took the foot away again.

**Ruled** by a Fable decision agent (xhigh), from four options: always draw the foot where the wiki can create pages,
reserve its room empty, overlay it, or leave it. **The foot belongs to the rail, not to the open page.**

- It stands whenever a space of the tree comes from a source whose adapter declares `create_page`. The test is the one
  `publishableSources` already makes, over `list_sources` and `list_adapters`. It is read once a session
  (`tf/docs/page-makers.ts`) and kept, so a later mount knows it before the tree draws. No IPC was added. Latching on a
  page that answered with `create_page` would latch on a press, which is this finding again. A new `DocsTree` field
  would touch a frozen surface.
- On a page that can be added under, *New page…* works as before. With no such page (the home, a space's page) it opens
  the dialog on the shown space, or, if that space's source cannot create, the first space that can, at its first page.
  The dialog's own *Space* and *Under* choose from there. The open page's refusal still disables it with its reason.
- A wiki none of whose sources can create pages has no foot anywhere. The page header's *New page…* still follows the
  page's own writes.
- While the sources' answer is still out (the session's first Docs, a few ms), the foot stands with the tree's first
  rows, so it never arrives after them; a wiki that cannot create pages then loses it once. If that read fails, or with
  no such port (as in older tests), the foot follows the open page's writes, as it did.

Not taken: an empty bar of the same height spends 47 px at the home on nothing. A bar laid over the box covers the same
two rows. Leaving it breaks the 2026-11-19 ruling that a tree press moves nothing in the rail.

**Tested.** `TfDocs.rail.test.svelte.ts`: the home draws the foot, and a page opened from the list keeps the same
element and every row, as does going back home; the foot's *New page…* on the home opens on the first space that can
create, at its first page; a wiki that cannot create pages draws no foot on the home or on a page that allows it; and
the foot stands with the first rows while the answer is out. All four fail on the code before. `page-makers.test.ts`: read once and kept; a failed read is asked again.

**Reversing it** is `footStands` in `TfDocs.svelte` going back to `page?.writes.includes("create_page")`.
