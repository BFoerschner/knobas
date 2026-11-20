# 2026-11-20: A Docs fold reads all its pages when it opens

**Found.** The Windows pass of v3.2.9-ux23 (Björn's PC, knobas's own WebView2 window, real presses), at full screen,
just after a start. From the list of all spaces, with eight folds of *Scale test: Engineering Handbook* open, the fifth
page press opened its page, and two rows arrived in the open *Archive* fold. Every row under them moved down 54 px. The
same happened on ux23's own front end, so the rail's foot (`2026-11-20-the-docs-rail-keeps-its-foot.md`) was not the cause.

**Why.** The Docs home's outline holds only the first 24 pages of each level (`OUTLINE_LEVEL_PAGES`, the instant-open
pass, 2026-11-12). A fold counted as read as soon as it held any page under it: `childrenMissing` in `tree-store.ts`
asked for a fold's pages only when it held none, and `readParts` skipped a fold that held one. So a fold the outline had
cut short drew the pages the outline held and read no more. The rest arrived with whatever read brought them later, here
the pressed page's way, which moved every row under them. `TfDocs.parts.test.svelte.ts` pinned the same thing: *Home*,
24 of its 30 chapters held, read nothing when opened, and drew 24.

**Now.** A fold is read until it holds as many pages as its `children` says (a node read whole, which carries no count,
is read as before). A fold the outline cut short reads the rest as it opens, about 15 ms, so its pages arrive with the
press that opened it, below the rows already there. A fold read once is not read again (`heldTree.folds`).

**Tested.** `TfDocs.parts.test.svelte.ts`: *Home* opened reads `c:home` and draws all 30 chapters; opened again it reads
nothing more. Red on the code before. In the window (the build served over CDP into ux23, the same scale copy, fresh
start): the open folds draw 43 rows where they drew 41, and five presses from the list of spaces change no row, at full
screen and at window size.

**Reversing it** is `childrenMissing` and `readParts`' fold test going back to "holds any page under it".
