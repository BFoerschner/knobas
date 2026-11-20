# 2026-11-20: A run's answer lands in one frame

**Found** (`just moves`, the fixture, 1440x868 and 2560x1440 at 1x and 4x, while building ADR-0046): a folder's generated folder, opened from its glyph with nothing kept, drew its band and its made bar at once and its rows when the run ended, and three real rows below the folder moved down 168 px in that frame. *Run again* added a file near the top of the generated folder, and seven rows below it moved down 24 px. Each return, from the tab's kept tree, moved nothing; every repository place with no graft moved nothing.

**Ruled** (Fable, xhigh, under Björn's standing instruction): a generated folder's rows land in the frame its run ends, as the place's answer, and the movement gate reads them so. `moves.mjs` names where a place's answer lands and what says it did (`answer: { in, changes, hold }`); in the first frame the words of `changes` differ from those at arm, the boxes inside `in` are taken anew instead of compared, but for those of `hold`. The two places take `in` the tree, `changes` its generated rows, `hold` the folder's row and the band. A first open whose answer never lands fails.

**Held still:** the page while the run goes (the made bar is busy and says so); everything outside the tree in that frame; the folder's row and the band, and every row above the band, always (`swapIn` anchors on a first landing too, so with the band scrolled above the view nothing in view moves); everything after.

**Rejected:**
- Holding room for the rows while the run goes: their count is unknown until the tool ends.
- A box of fixed height for them that scrolls on its own: a scroller nested in the windowed tree.
- Holding a re-run's tree back behind *Show the new files*: on a first open it would leave the pane on a file the tree does not show (rejected already in the decision list's section 11).

**Standing:** it widens the 2026-11-18 rule that a key's place is drawn at its answer's frame (`docs/decisions/2026-11-18-a-tickets-key-changes-nothing-until-its-answer.md`) from a key to a run. Ruled by the design pass with Björn away: his to overrule.

**Tested:** `TfRepoFiles.made.test.svelte.ts`: the first rows land in one write (the band and the real rows keep their nodes, and the bar's busy clears in the flush that draws the first generated row), and they keep the view when the band is scrolled above it (mutant: the early return on a first landing). The press trips `rerender-glyph`, `rerender-real`, `rerender-run-key` and `rerender-run-pointer`; `just moves`' `rerender-graft` and `rerender-again` with the folder's row and the band held.

**Reversing:** drop `answer` from the two places, and land the rows on a press of the band instead of the run's end.
