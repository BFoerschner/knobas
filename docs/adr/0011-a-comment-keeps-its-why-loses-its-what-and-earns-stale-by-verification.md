---
status: accepted
---

# A comment keeps its why, loses its what, and earns "stale" by verification

Seven sweep tickets (#192–#198) are about to walk every comment in the workspace — roughly nineteen thousand comment lines across the backend crates and the frontend — and the Standards axis of every later code review judges new comments forever after. Without a ratified standard, each sweep agent invents its own taste, seven tickets produce seven inconsistent verdicts on the same kind of line, and a reviewer has no yardstick beyond "I would not have written that". The repo already leans hard on comments as agent-facing context: §10.8-style frozen-contract pointers, ADR references, and protocol quirks are how one agent's hard-won finding reaches the next one. A standard that deleted those to save tokens would be spending the repo's memory to tidy its files.

Decided 2026-08-30 (Björn): **a comment is judged by whether it carries information the adjacent code cannot, and "stale" is a verdict reached by verification, never by reading alone.** Three verdicts, exhaustive:

- **Keep** a comment that carries rationale — information a reader cannot recover from the code beside it: a constraint the code obeys but cannot show, a rejected alternative and why it lost, "this looks wrong but isn't" and the reason it isn't, a frozen-contract pointer (§10.8 style) or ADR reference, a protocol quirk of an external system. These are load-bearing for agents and humans alike; a sweep that touches one has left its brief.
- **Delete** a comment that only restates the adjacent code. It adds tokens and reading time with zero information, and its cost recurs on every future read. Restating *is* the whole test: if removing the comment loses nothing the code does not already say, it goes; if any clause survives that test, the comment is a keep (or a trim to the surviving clause, which is a correction, below).
- **Verify, then correct or file** when a comment contradicts the code it sits on. A contradiction means one of two things — a wrong comment, or a correct comment describing a bug — and reading cannot tell them apart; only behavior can. Verify against actual behavior first (run the code, the test, the query). If the comment is what's wrong, correct it in place. If the code is what's wrong, file an issue and leave the comment standing with a reference to that issue. **Never silently delete a contradicting comment**: deletion destroys the only recorded evidence that somebody once knew what the code was supposed to do.

**Scope: comments in code and in tests alike.** Test comments are held to the same three verdicts — a test's "why this assertion, why this shape" rationale is a keep, and a `// arrange` over an arrange block is a delete. Tests are where this repo records the most behavior rationale, so exempting them would exempt the densest area.

**Exclusions**, each for a stated reason, not taste:

- **`migrations/` SQL.** sqlx embeds a checksum of each migration file; editing so much as a comment in an applied migration breaks validation on every existing database. The area is also frozen under §10.8. No verdict is ever rendered there.
- **`mockups/`.** Throwaway design artifacts, not shipped code.
- **Generated files.** A tool rewrites them; an edit is churn that the next generation run reverts.

## Considered options

- **No standard — each sweep applies its own judgment.** Rejected: this is the status quo the tickets exist to end. Seven agents produce seven tastes, the same comment survives one sweep and dies in the next, and review has nothing to cite.
- **Delete-by-default (a comment must justify itself or go).** Rejected: the repo's comments are a working memory for agents — frozen-contract pointers and protocol quirks are precisely the lines a terse-code aesthetic would strip, and every one stripped is re-derived later at the price of a bug or a review round.
- **Keep-by-default (only fix provably stale comments).** Rejected: restatement comments are not harmless filler here; agents read whole files into bounded context, and pure-noise lines are paid for on every read, forever. The delete verdict is the half of the standard with recurring payoff.
- **Treat a contradicting comment as deletable noise.** Rejected explicitly: half of contradicting comments are correct descriptions of buggy code, and those are the most valuable comments in the repo. The verify-first rule is what makes the sweep safe to delegate.

## Consequences

- The seven sweeps (#192–#198) apply this ADR mechanically: every comment gets one of the three verdicts, and the only judgment left to the sweeping agent is *which* verdict fits — never what the verdicts are. PR descriptions report counts per verdict.
- Sweeps are comments-only diffs. The verify-then-file rule is what keeps them so: where the code turns out wrong, the fix is an issue, not a drive-by functional change inside a comments PR.
- The Standards axis of code review inherits the same three verdicts for new code: a restating comment is a finding, a deleted rationale comment is a finding, and "this comment looks stale" obliges the reviewer to verify before asserting it.
- The migrations exclusion is absolute and mechanical: no comment edit under `migrations/`, whatever its verdict would have been, because the checksum makes the cost a broken database rather than a style regression.
