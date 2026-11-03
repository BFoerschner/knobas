# Spec #2432's open calls: Docs draws whole pages, 2026-11-02

Björn ruled on the calls spec #2432 (*Docs draws whole pages*) left open, in a comment on #2432
on 2026-11-02, after the spec's final landing on `spec-2432`. The #2456 implementer records them
here, as that ticket asks. They are his rulings, not an agent's, and they bind as such.

## The rulings

1. **The projection deadline.** A projection gets 5 s (#2442 ruling 8), and that stands.
2. **`knobas adapter check` (story 50)** is not needed now. `docs_contract::check` stays in the
   code, with no command calling it, for later. Nobody deletes it.
3. **The not-mirrored include placeholder** opens the including page (#2439 ruling 3, whole-branch
   review F5). That is fine.
4. **URL embeds' links and popups, and the app's `frame-src http: https:`** (#2441 rulings 5 and 7,
   the #2441 merger's rulings 11 and 12) are fine. They are secure by default.
5. **A URL embed is a fixed 480 px** for now. Björn will see it and test it himself.
6. **The seeded instance (story 53)** is not accepted as it was. The seed must hold the
   constructs permanently, so Björn can browse them in knobas himself and not only the tests can.
   This became #2456, a sub-issue of the spec.
7. **The seed turns the `html` macro on itself** (#2440). Accepted.
8. **A pre-1.5 adapter's page `body-text` is kept** (#2442 ruling 3). Fine: no plugins exist yet.
9. **A table outranks `macro` beside an embed or an include** (whole-branch review, Spec 10). Fine.

## Why

Rulings 1 to 5 and 7 to 9 accept what the implementers ruled during the run. Each was recorded in
its PR under **Rulings** with its cost of reversing, and Björn read them before the spec's PR went
ready. Ruling 2 keeps a check that has no caller yet because the adapter SDK will want it once
plugins exist.

Ruling 6 is the one he turned down. The live tests made their pages over REST and deleted them
afterwards (`Litter`, #2436 ruling 1). That witnessed the reader, but it left nothing Björn could
open, and the include of a page "outside the synced spaces" pointed at a space that did not exist
(#2439 ruling 12). A reader he cannot browse is one he cannot judge.

## Cost of reversing

Rulings 1, 3, 4, 5, 8 and 9 each cost what their PR's **Rulings** entry says. Ruling 2: deleting
the unused check is one module and its tests. Ruling 6: the showcase pages are seed content
(`testenv/confluence-showcase/`, `testenv/seed-atlassian-content.sh`). Taking them out means
deleting those pages and the live tests' reads of them, and the tests then make their own pages
again.
