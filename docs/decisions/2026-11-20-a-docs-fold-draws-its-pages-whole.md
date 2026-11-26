# 2026-11-20: A Docs fold draws its pages once it holds them all

**Found.** The Windows pass of v3.2.9-ux24 (Björn's PC, knobas's own WebView2 window, the ux23-era scale copy, real
presses on a fold's arrow, every rail row's place read each frame). On a page loaded afresh, the Docs home with
*Scale test: Operations Runbooks Home* open, a press on *Archive* drew the one page of its four the outline held,
then the other three when its read landed: 12 ms later at CPU 1x, 25 ms later at 6x. The 11 rows under it moved
twice, 27 px and then 80 px. At 4x the read beat the first frame and they moved once. This is what
`2026-11-20-a-docs-fold-reads-all-its-pages.md` left: that fix reads the rest of a fold the outline cut short as it
opens, but the fold drew what it held first, so the rest still arrived after it.

**Now.** A fold that holds fewer pages than its `children` says, and is asking for them, draws none of them until its
read has answered (`TfDocTree`: `need` settles when the read is drawn or has failed, `answered` per fold). Then it draws
them all in one frame. If the read fails, it draws what it holds. A fold read whole, or one that holds all its pages,
draws at once as before. A fold closed and opened again asks again if its pages are still missing.

**In the window** (this front end served over CDP into ux24, the same copy, the page loaded afresh each time): *Archive*
drew its four pages in one frame, 14 and 18 ms after the press at 1x, 50 at 4x, 47 to 62 at 6x, and the rows under it
moved once.

**Opening a fold, timed** (ux24, full screen 5120x1369, a Gitea sync every 45 s, 99 to 123 rail rows, a fold not read
yet): its rows drawn 56 to 64 ms after the press at 4x and 85 to 99 ms at 6x (lines 100 and 200). `docs_nodes` answered
in 23 to 49 ms.

**Tested.** `TfDocs.parts.test.svelte.ts`: a fold the outline cut short (24 of 30 held) draws none of them while its
read is out and all 30 when it lands, red on the code before; a fold whose read fails draws the 24 it holds.

**Reversing it** is the `!waits(node)` test in `TfDocTree.svelte`'s fold going away.
