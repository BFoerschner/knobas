---
status: accepted
---

# The Tickets view may say "Board"

Amends [ADR-0009](0009-board-never-appears-unqualified.md).

ADR-0009 says "board" never stands alone: there is the *launcher board*, the *assets board* and the *mini board*, and qualification may come from the surface around the word. The ticket-first redesign (`mockups/ticket-first/`, D1) has a Tickets view whose layout toggle reads **Board / List**, and a view list with a group called **Jira boards**. Björn waived the vocabulary rules for the mockups only, so the question was whether the app may keep the words.

Decided 2026-10-15 (Björn, D1): **yes.** Inside the Tickets view, *Board* is the **ticket board**: a view's tickets in columns, one column per group of statuses the view sets. The surface qualifies it, as a tab labelled "Board" inside the Assets view reads as the assets board. *Jira board* is the source's own word and always appears with "Jira" in front of it.

The rest of ADR-0009 stands. The bare word stays out of domain docs, specs and issues, which write *ticket board*. Every other use is still qualified, and the mini board keeps its name for the room's Tickets tile.

## Considered options

- **Rename the toggle to *Columns / List*.** Rejected: *Board* is the word everyone who uses Jira already has for this layout, and the surface already says which board it is.

## Consequences

- `CONTEXT.md` gains **Ticket board**, and the *Mini board* entry's list of qualified boards adds the ticket board and the Jira board.
- A ticket board's columns are the view's own, not the mini board's one-per-status groups (requirements R-3.15). The two are different things that share a layout.
