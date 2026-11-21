# 2026-11-21: Settings' nav has subtrees

**Asked (Björn, on PR #2479):** "I feel like it's time for subtrees? Or something because themes go into appearance. and
also, this is just themes for code anyways. full on editor themes is something i'll want to add later aswell".

Ruled by Fable (xhigh) under Björn's standing instruction.

## The rule

**A row of the nav is a section, and the tree is fixed in code** (`SETTINGS_TREE` in `tf/settings/settings.ts`). A
parent is a section with a page of its own, and a click on it opens that page. What a person makes (a source, an
application, a folder renderer, a theme) is never a row: its page lights its section's row and the crumbs carry its
name, as before. Nothing in the nav waits on a read, so no row arrives late and moves the others.

## The tree

Appearance holds two children: **Code colours** (`#/settings/appearance/code-colours`) and **Fonts**
(`#/settings/appearance/fonts`). Fonts was "a pane beside Appearance" only because the nav was flat (amending point 9 of
`2026-11-18-every-face-is-yours.md`); its row stands where it stood. `#/settings/fonts` still lands on it, drawn in
place. The app-wide theme page's slot is Appearance's first child, key `theme`, with no row until it is built. Every
other section stays at the top: Sources, Checkouts and Renderers keep their made things off the nav, and Performance,
Search and Standup have no parent among the sections. The window's side stays on Appearance's own page.

## Folds, clicks and keys

Every parent starts open. A fold is kept in `localStorage` (`knobas.settings.folded`, the folded parents), and a move to
a child's page opens its parent and keeps it. Clicks follow the Docs tree's rule (`2026-11-21-a-click-on-a-docs-row-opens-its-fold.md`):
a click on a parent's row opens its page and its fold and never closes it, a click on the open parent's row toggles,
the chevron folds alone, and `Enter`, a modified click and the middle button keep the link's own way. A folded parent
holding the open page takes Docs' edge mark. The nav takes Docs' tree keys (`treeKeysOf`): it is one Tab stop, the
open page's row, where it was seventeen; ↑ and ↓ walk the rows shown, → and ← fold and step, Home and End.

## The rest

The crumbs read *Settings › Appearance › Code colours*, Appearance linking to its page, and the window's title comes
from the last of them. Esc leaves Settings from a child as from any section. The launcher's rows carry the path
(*Settings › Appearance › Fonts*) and are found by their own name. The narrow rail folds on a press that opens a page and
stays open on one that only folds.

## Left for Björn

Sources as nav rows, Standup under Time, and Renderers split into child pages.
