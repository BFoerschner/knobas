# Integration branch (`/implement-spec`)

How a whole spec is built on one integration branch with one PR to `main` at the end (Björn, 2026-10-30; first used for spec #2234, PR #2297). The orchestrator reads all of this. It hands implementers and mergers this file together with their brief, and the parts below beat the briefs wherever those talk about `main`, per-ticket PRs, the merge-tail lock or closing issues. Everything else in the briefs stands. `<N>` is the spec's issue number, and `<scratchpad>` is the orchestrator's session scratchpad, which every agent in the run shares.

## Orchestrator

- **The branch.** `spec-<N>` is created from `origin/main` and pushed. A draft PR to `main` is opened after the first landing: it closes #`<N>` and every sub-issue, and its body is rebuilt from the per-ticket notes before it goes ready. A body over 65,536 characters becomes a summary, with each ticket's notes posted as a PR comment.
- **Ceiling.** The *Ceiling* in `.claude/commands/ticket-loop.md` applies to this run as it does to the loop: seats, Rust builders, the load line, disk, the ledger and its orphan recovery. The caps are per machine.
- **Tickets.** Only `#<N>`'s sub-issues are claimed (`in-progress`), and never by `/ticket-loop`. A gap that a run finds in its own spec's scope joins the spec as a sub-issue (`gh api -X POST repos/{owner}/{repo}/issues/<N>/sub_issues -F sub_issue_id=<id>`), so the PR closes it.
- **ADR numbers.** Reserve a block for the spec at the start, and hand each ticket that writes an ADR its number in the dispatch. Two runs on different machines take disjoint blocks.
- **Landing counts.** Count the landings. Every fifth landing's merger gets *full gate* in its dispatch.
- **Before the PR goes ready**, in this order:
  1. Merge `origin/main` into `spec-<N>`.
  2. Run a whole-branch `code-review` (both axes) against the merge base, and give the fixes to one implementer.
  3. Run a final landing with the full `just gate origin/main` and the **whole** `just gitea-live` and `just gitea-live forgejo`, plus any other live suite the spec's tickets ran.
  4. Merge the PR under the merge-tail lock, as the merge-manager brief says.
- **Two specs at once on two machines.** The merge-tail lock lives in `$TMPDIR`, so it serializes only one machine. Land the two final PRs one after the other: after the first merges, merge `main` into the second spec's branch and gate it again.
- **Shared notes.** Agents write facts that a later ticket needs to `<scratchpad>/notes/<topic>.md`. These are API shapes, measured source behaviour and seams, not diaries. Dispatches point at them.

## Implementer

- **Base.** The worktree is on `issue-<T>`, made from `origin/spec-<N>` at the SHA in the dispatch. First, confirm it with `git merge-base --is-ancestor <SHA> HEAD`. If the worktree isn't on that base, `git reset --hard <SHA>` before any work.
- **No PR.** Push `issue-<T>`, and write the PR body's content to `<scratchpad>/pr-<T>.md`: what and why, the criteria checked, the head SHA and what ran, the mutation proof, **Rulings**, **Frozen surface**, **Fixed in passing** and **Evidence**. Rulings still go on the issue as a comment, and screenshots still go on `pr-assets/issue-<T>`.
- **Base for comparisons.** Before shots and diffs are taken against the dispatch SHA on `spec-<N>`, not against `main`. `scripts/gate-scope.sh origin/spec-<N>` says whether a change is front or full.
- **Before reporting**, run `git fetch origin && git merge --no-edit origin/spec-<N>` (a merge, not a rebase). Resolve, re-run the tests beside the change, push, and report the merged head.
- **Hands off.** Never close an issue, and never push to `spec-<N>` or `main`.

## Merger

- **Review** as the merge-manager brief's *Review* section says, over `git diff origin/spec-<N>...issue-<T>`, with `<scratchpad>/pr-<T>.md` as the PR body. Fix on `issue-<T>` and push.
- **Landing gate** (replaces *Merge tail* and *Merge and cleanup*; no lock):
  1. `git fetch origin && git merge --no-edit origin/spec-<N>`.
  2. Run `just pre-gate` and commit whatever it changed.
  3. Run `just check-front origin/spec-<N>`.
  4. For every crate the change touches: run `just clippy -p <crate> …`, then `cargo test -p <crate> …` (after `just wasm-fixture` when `knobas-app` is among them). A change to `knobas-source` or `knobas-http` also tests every crate that depends on it directly.
  5. When the dispatch says *full gate*, replace all of the above with `just gate origin/spec-<N>`.
- **Land.** Push with a plain `git push origin HEAD:spec-<N>`, never force. It is fast-forward only, so a rejection means `spec-<N>` moved: merge, run the landing gate again, push again. No tree lands that no landing gate saw. Then push `issue-<T>` to the same SHA.
- **After landing.** Comment "Landed on `spec-<N>` at `<sha>`; closes with the spec's PR." on each issue. Leave the issue open, and leave the worktree and branch for the orchestrator.
- **Report** in the merge-manager brief's shape, with `Landed: issue-<T> → spec-<N> <sha>` in place of `Merged:`, and name which gate ran.
