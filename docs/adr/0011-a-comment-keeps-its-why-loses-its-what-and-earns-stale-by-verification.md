---
status: accepted
---

# A comment keeps its why, loses its what, and earns "stale" by verification

The repo leans on comments as agent-facing context: frozen-contract pointers, ADR references and protocol quirks are how one agent's finding reaches the next. Decided 2026-08-30 (Björn): **a comment is judged by whether it carries information the adjacent code cannot, and "stale" is a verdict reached by verification, never by reading alone.** Three verdicts, exhaustive, for comments in code and tests alike:

- **Keep** a comment that carries rationale the code cannot show: a constraint the code obeys, a rejected alternative, "this looks wrong but isn't" with the reason, a §10.8 or ADR pointer, a protocol quirk of an external system.
- **Delete** a comment that only restates the adjacent code. If removing it loses nothing the code already says, it goes; if any clause survives that test, trim to that clause.
- **Verify, then correct or file** a comment that contradicts the code it sits on. Reading cannot tell a wrong comment from a correct comment describing a bug. Run the code, the test or the query. If the comment is wrong, correct it in place. If the code is wrong, file an issue and leave the comment standing with a reference to it. Never silently delete a contradicting comment: it is the only recorded evidence that somebody once knew what the code was supposed to do.

Exclusions, each for a stated reason:

- `migrations/` SQL. sqlx checksums each applied migration; editing even a comment breaks every existing database. The area is also frozen under §10.8.
- `mockups/`. Throwaway design artifacts.
- Generated files. The next generation run reverts the edit.

## Rejected

- No standard, each sweep its own judgment. Seven agents, seven tastes, and review has nothing to cite.
- Delete by default. Strips exactly the pointers and quirks agents need, each re-derived later at the price of a bug.
- Keep by default. Restating lines are paid for on every bounded-context read, forever.
- Treat a contradicting comment as deletable noise. Half of them describe buggy code correctly, and those are the most valuable comments in the repo.

## Consequences

- A comment sweep is a comments-only diff. Where the code turns out wrong, the fix is an issue, not a functional change inside a comments PR.
- The Standards axis of code review inherits the three verdicts: a restating comment is a finding, a deleted rationale comment is a finding, and "this looks stale" obliges the reviewer to verify first.
