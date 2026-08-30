---
status: accepted
---

# "Board" never appears unqualified, and the Tickets tile's board is the *mini board*

By the time the ticket status board arrived (M2.5 planning, 2026-08-30), the word "board" was already spent twice: the launcher's empty-query panel is internally the `board` mode (spec §4: "an empty box is a board, not a query" — `LauncherMode` in `app/src/lib/launcher/rows.ts`, the `launcher_board()` read), and the M4 Assets view has a planned **Board** tab for the Miller columns (design doc §2, entity addressing `#/assets/{board|monitors|flowrun}`). Neither is a status board. The only user-visible "Board" today is none at all — the launcher's is a pure identifier — and the design doc already has a name for the third thing: the Room's Tickets tile is "Tickets (**mini board**)" (design doc §2, Shell table).

Decided 2026-08-30 (Björn, M2.5 grilling session): **"board" never stands alone.** In domain docs, specs, issues, and user-visible text, every use is qualified — the *launcher board*, the *assets board*, the *mini board* — where qualification may come from the surrounding surface (a tab labelled "Board" inside the Assets view reads as the assets board). The M2.5 feature takes the spec's existing name, **mini board**, and nothing shipped is renamed: `LauncherMode`'s `"board"` and `launcher_board()` are internal identifiers, not user-visible labels, and they stay.

## Considered options

- **The status board claims "Board" as its domain name**, renaming the launcher mode and the M4 tab. Rejected: it buys purity by churning a shipped M1 surface and a specified M4 label, to fix a user-visible collision that does not actually exist.
- **Call it "Kanban".** Rejected: the word appears nowhere in the repo outside two vendored OpenAPI specs — not in the mockups, not in the design doc, not in the roadmap. The spec named the feature *mini board* on day one; inventing a second name for it is the exact overloading this ADR exists to stop.
- **Live with three unqualified boards, disambiguated by context.** Rejected: the word was spent twice before the third use arrived, which is how this question got forced in the first place.

## Consequences

- The M2.5 spec addendum, its issues, and its code speak of the *mini board*; "kanban" stays out of the vocabulary.
- A future fourth board-shaped surface gets its own qualified name on arrival, not squatter's rights on the bare word.
- `CONTEXT.md` carries **Mini board** as a glossary term; the launcher and assets senses stay where they are specified (spec §4, design doc §2) rather than becoming glossary entries — they name surfaces, not domain concepts.
