# Unattended rulings — from 2026-10-11, Opus as deputy

The `/v15-next` loop now runs over every `ready-for-agent` issue, not one milestone, and its deputy is Opus. The shape is the 2026-08-29 batch's (`2026-08-29-unattended-batch-rulings.md`) and the v1.5 milestone's (`2026-09-v1-5-unattended-rulings.md`): each ruling is posted as a comment on its issue, beginning `Ruling (deputy for Björn, <date>):`, and the PR that acts on it appends the same four parts here, in the order ruled. The deputy first looks for an answer that already exists — the ticket, its spec, `CONTEXT.md`, the ADRs, the decision records — and quotes it; only an open fork gets a new ruling. Björn reverses one with a follow-up ticket, and a ruling that settles something architectural gets its ADR through the implementer, per this directory's README.

---

## #593 — `repo` directly before `pr`, or directly after it

Ruled 2026-10-11. Comment:
<https://github.com/BFoerschner/knobas/issues/593#issuecomment-6113041302>

**The fork:** #593 says "`repo` moves to directly after `pr`, ahead of `branch` and `commit`". The
same ticket also says "**a parent kind always comes before its child kinds**". A PR sits in a repo,
so the literal sentence breaks the rule for the repo→PR pair. PR #602 follows the rule and puts
`repo` directly *before* `pr`: `ticket, asset, repo, pr, build_config, build, page, note, commit,
branch, route, monitor`. Should `repo` go where the sentence says, or where the rule says?

**Ruling:** follow the rule. `repo` goes directly before `pr`, as PR #602 has it. This was already
decided, and the ruling only confirms it.

**Reasoning:** the answer is written down in three places, and all three agree:

- ADR-0018 (accepted): "a commit, branch or PR sits in a repo … In the fixed group order, a parent
  kind always comes before its child kinds."
- `CONTEXT.md`, **Parent**: "The entity you can open that an item sits directly inside: a repo for
  a branch, PR or commit; …"
- #591, user story 12: "As a developer, I want the repo group to come before the branch, PR and
  commit groups, so that the thing I navigate *to* is above the things I scroll *through*."

Story 12 names the PR group outright. "Directly after `pr`" is a slip in wording: #591's
Implementation Decisions has the same slip, and #593 copied it from there. The same bullet in #591
opens with the governing rule, and the ADR states that rule as a consequence of the decision. The
sentence describes one move, taking `repo` out of last place and putting it above the kinds that
sit in it, and one slot before `pr` does that. The ticket's scope and its five acceptance criteria
are unchanged, and #602 meets the first criterion's stricter reading too (repo above PR as well as
above branch and commit). `GROUP_ORDER` is not a §10.8 frozen surface (`docs/contract.md` does not
mention group order). Both of #602's tests are witnessed: mutant M2, the ticket's literal wording,
fails the unit rule test and the real-PostgreSQL `Searcher` test.

No glossary entry or ADR changes. The only text that disagrees is the bullet in spec #591; if
Björn wants the record clean, that bullet becomes "`repo` moves to directly before `pr`". That is
an edit to the spec issue and does not block #602.

**If you disagree, the cost of reversing this is:** trivial in the code: swap two entries in
`GROUP_ORDER` and drop the `("repo", "pr")` pair from the `PARENT_KINDS` test table. But the result
would contradict ADR-0018 and story 12, so a reversal means amending ADR-0018 (for example, "a
parent kind comes before its child kinds, except that a PR may precede its repo") and rewriting
story 12. It cannot be done in a comment.
