# 2026-11-19: The Docs rail holds its space

**Asked (Björn, on PR #2479):** "for the docs page, I really want you to not switch to the space when I'm just clicking
through the tree. Only if i actually search from the filter or manually select the space should it jump there"

## What switched it

The rail's view was `picked`, and `null` meant *follow the address*: the home showed every space, a space's own page
that space, and a page its own space. Only the home and a space's page reset it to `null`, and nothing ever set it
from a press. So a page opened from the list of every space turned the rail into that page's space alone. The list,
its filter and the other folds went, *For you* arrived on top, and the rows under the pointer jumped. Inside one
space's tree, the tree opened the folds around the open page and did not keep them (`TfDocTree`'s `holding`). A press
on a page in another fold closed the folder of the page being left, and everything under it moved up. At the space
level the list did the same: a space fold opened because it held the open page closed once the page was elsewhere.

## What each way in does now

Ruled by a Fable decision agent (xhigh), from the facts and options in the brief:

| Way in | The rail |
|---|---|
| A row of the tree, from the list of spaces or from one space | Holds: its space, its folds and its scroll. |
| A link in a page's body, to a page of another space too | Holds. The tree then marks no row, the crumb names the page's space, and the closed fold holding the page is marked. |
| The home's own lists (*Continue reading*, a tile's pages, *For you*) | Holds the list of spaces. |
| The Space picker | Switches: a space goes to its own page, *All spaces* lists them and holds that through later presses. |
| A space's own page (a tile's head, the crumb's space, the picker) | That space. |
| A search (`#/docs/search/…`) | The hits; opening one keeps them; *Clear* lands on the open page's space with the folds around it open. |
| The launcher's page, a notification's (`goLaunched`, `goWithoutOpener`) | Follows the page to its space, with the folds around it open. The launcher is a search. |
| A page opened from another section (a ticket's link, the inbox) | Follows. The rail was off screen, so nothing drawn moves. The Docs entry back to the place left keeps the rail as left. |
| ⌘[ and ⌘] | The rail as that entry had it: each entry keeps it beside the shell's stamp (`knobasDocsRail` in `history.state`). An entry without one follows its page. |

**No fold opens on a hold.** The tree opens the folds around the open page only while the rail follows it (`follow`).
The first press in Docs after that keeps those folds open as the reader's own, so nothing closes. A page whose row is
in a closed fold, or in a closed space of the list, marks that fold's head (`tf-holds`): the open row's `--fill` as a 2 px
inset edge and its words in `--ink`, which takes no room.

**For you sits over one space's tree only.** It was drawn on every place but the home, so opening a page from the home
pushed the list down by its height. It now goes with the rail's view: none over the list of spaces, the shown space's
over its tree. A press no longer changes the view, so it never arrives or leaves on one.

## How

- `TfShell` tells `opener.ts` how each address change came (`noteArrival`: what it was told and whether it is a return),
  and Docs reads it for the address it is at (`arrivalAt`). A test that moves the address without the shell sees a plain press.
- `picked` is settled as soon as the address says what the rail shows. From then on only the picker, the home, a space's
  page, a search, a following arrival and a return set it. A press leaves it, so the next new way into a page holds by default.
- `docs/specs/2026-10-15-ticket-first-design-system.md` (Docs' page tree) carries the rule and the mark.

## Tested

- Vitest `TfDocs.rail.test.svelte.ts`, nine tests: a press from the list keeps the list, its filter, the same fold
  elements, its scroll and every row, and adds no *For you*. A press in a space keeps the folds opened for the page left.
  A link to another space holds and marks the closed fold. A link from the list marks the closed space. The picker switches,
  and *All spaces* holds. The launcher and a notification follow, with the folds open. A search's hit, cleared, lands on
  its space. Another section's link follows, and the entry back keeps the rail. Back and forward restore the entry's rail.
  On the code before this change six of the nine fail (the three that pass are the switches, which already worked).
- Mutants, each failing at least one of them: a press that follows, folds not kept, *For you* back on every page, the tree
  always opening around the page, the launcher holding, no restore on a return, another section holding, nothing
  settled, the space fold opening for the page, no mark in a tree, no mark on a space, no stamp, and *Clear* holding.
- Press trips `docs-tree-walk-home` and `docs-tree-walk-space` (`tree-walk`): by real pointer through the tree, from the home and
  from a page, every row of the rail, the picker and the rail's scroll must stand where they stood.
- `just moves`: the new `docs-tree` place, a press on a rail row from a space's page, beside `docs`, `docs-space` and
  `docs-page`.

## Reversing it

Drop the arrival block in `TfDocs.svelte` (`arriveRail`, the settling effect, the stamp), `TfDocTree`'s `follow` and `path`,
and `noteArrival` in the shell; `picked` then follows the address again.
