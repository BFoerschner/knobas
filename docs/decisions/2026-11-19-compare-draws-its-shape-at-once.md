# 2026-11-19: Compare draws in its final shape at once, from the mirror; *Commits* stops at the form's foot

**Asked (the ux20 Windows pass, for ux21):** `91e361e0` made Compare wait unseen for its branch's standing, at most 500
ms, so the form no longer drew as *New pull request* (385 px) and then as *Compare* (172 px) for a branch with a pull
request open, the diff under it jumping 175 px. Measure Compare's first open with that hold at 1x / 4x / 6x on the live
Gitea; if it is over a line, show the page at once with its room at its final height.

## Measured

A branch's first Compare (a fresh copy of the saved scale home for each run, so its standing was never read), the
windowless backend on the Mac against the live Gitea on the WSL box, a headless Chrome on the box, 5120x1440, by the
inner pages' probe (`open.mjs --set inner --only repo-compare`). The standing (`entity_view` of the branch, `behind`, a
round trip to the forge for the counts) answered at 696 to 739 ms. The hold ran out at 500 ms, the page showed at
**521 / 548 / 564 to 580 ms** at 1x / 4x / 6x, and when the standing landed the diff still moved (1, 2 and 3 movers, the
diff 175 px up). Over every line, and moving anyway.

## Fable's ruling

Asked with the options (drop the hold; a fixed room at the Commits panel's cap; the panel out of the row's sizing; a
hold for the mirror only), Fable chose the panel out of the row's sizing, with a short hold kept only for an address
opened straight onto Compare:

- **The form's shape from the mirror.** Whether a pull request is open from the branch is in the mirror, read locally
  by the repository's page before Compare opens (`repo_branch_pulls`, about 20 ms; `mirrorStanding`, which the header's
  *New pull request* already reads). The form draws from it until the standing answers, and the standing's word stands
  after. A pull request opened since the last sync reshapes the form once: a correction, taken.
- **The page held only while the mirror's read is out,** at most 100 ms (`FORM_HOLD_MS`); a failed read counts as one
  with no pull request.
- **Commits stops at the form's foot** (`branches.css`): the side stretches to the form column's height and the panel
  sits in it, as tall as its rows and no taller than the form, its rows scrolling inside. The form alone sizes the row
  the diff stands under, so commits that come after the page shows move nothing: about ten rows beside *New pull
  request*, three beside *Compare*.
- **The ticket link's room** (*Link the PR to KEY in knobas*) is held unseen while a branch's key is looked up, so the
  form is its final height when it shows.

## After

The same runs, the new front end (fresh copies, alternated with the old): Compare useful at **171 / 133 / 146 ms** and
1,088 / 134 / 163 at 1x / 4x / 6x, nothing moved in any. The one 1,088 is the backend's: on that cold copy three local
reads (`entity_range_names`, the branches' view) waited about 1,060 ms behind the forge's first read; on the other copy
they answered in 24 ms. What the 1x open still waits for is the forge's compare of a repository knobas has not cloned,
whose names it had not stored. `moves.mjs --real --only repo` (the repository and each tab, Compare among them, first and
return, 1x and 4x) at 1440x868@1, 2560x1440@1.25 and 5120x1440@1, on fresh copies: Compare moved nothing at any size.

## Seen, not this pass's

- On the same runs the repository's landing moved twice: at 4x on its first open the README's *Render as Markdown* and
  *Pipeline* moved 59 px left as *Clone* came in beside them (1440x868 and 2560x1440), and at 5120x1440 on its return the
  last commit's *History* became *438 commits* 50 ms after it drew.
- The inner probe's `compare-large` item never counts as useful: it waits for three diff cards, and its branch
  (`fix/ledger-writer-49` of `cart-service`) changes one file in the estate as it stands now.
