---
status: accepted
---

# The Tickets view may say "Board"

Amends [ADR-0009](0009-board-never-appears-unqualified.md).

The ticket-first Tickets view has a layout toggle reading **Board / List** and a view group called **Jira boards**. Decided 2026-10-15 (Björn, D1): **inside the Tickets view, *Board* is the ticket board**, a view's tickets in columns, one column per group of statuses the view sets. The surface qualifies the word, as a tab labelled "Board" inside the Assets view reads as the assets board. *Jira board* is the source's word and always carries "Jira" in front.

The rest of ADR-0009 stands. Domain docs, specs and issues write *ticket board*, every other use stays qualified, and the mini board keeps its name for the room's Tickets tile.

## Rejected

- Rename the toggle to *Columns / List*. *Board* is the word Jira users already have, and the surface says which board it is.

## Consequences

- `CONTEXT.md` has **Ticket board**, and the *Mini board* entry lists the ticket board and the Jira board among the qualified boards.
- A ticket board's columns are the view's own, not the mini board's one-per-status groups (requirements R-3.15).
