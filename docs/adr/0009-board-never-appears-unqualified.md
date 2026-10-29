---
status: accepted, amended by ADR-0024
---

# "Board" never appears unqualified, and the Tickets tile's board is the *mini board*

By the time the ticket status board arrived (M2.5), "board" was already spent twice: the launcher's empty-query panel is the `board` mode (`LauncherMode`, `launcher_board()`), and the Assets view has a Board tab for the Miller columns. Decided 2026-08-30 (Björn): **"board" never stands alone.** In domain docs, specs, issues and user-visible text every use is qualified: the launcher board, the assets board, the mini board, and since ADR-0024 the ticket board and the Jira board. Qualification may come from the surrounding surface (a tab labelled "Board" inside the Assets view reads as the assets board).

- The Room's Tickets tile is the **mini board**, the name the design doc gave it. "Kanban" stays out of the vocabulary.
- Nothing shipped is renamed. `LauncherMode`'s `"board"` and `launcher_board()` are internal identifiers.
- A future board-shaped surface gets its own qualified name on arrival.

## Rejected

- The status board claims the bare word, renaming the launcher mode and the Assets tab. Churns a shipped surface to fix a user-visible collision that does not exist.
- "Kanban." Appears nowhere in the repo outside vendored specs; a second name for a thing the spec already named.
- Three unqualified boards, disambiguated by context. The word was spent twice before the third use arrived.

## Consequences

- `CONTEXT.md` carries **Mini board** as a glossary term. The launcher and assets senses name surfaces, not domain concepts, and stay in the spec.
