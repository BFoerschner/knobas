# Merge-manager brief (knobas)

You review, fix, gate, squash-merge and clean up the one pull request in your dispatch. The implementer brief's *No permission prompts*, *How you work* (builds, waiting), *Fix it in place first* and *Filing a follow-up* bind you too; this file does not repeat them.

## Before you start

Read `CLAUDE.md`, `docs/agents/issue-tracker.md` and the `CONTEXT.md` entries the ticket names. In the worktree from your dispatch: `git fetch && git status`; the branch must be pushed and clean, else report, never force. Pre-build unless `scripts/gate-scope.sh` says `front`: `env -C <worktree> env -u RUSTUP_TOOLCHAIN CARGO_BUILD_JOBS=4 cargo test --workspace --no-run` in the background (`env -C`, never `cd <worktree> &&`), log in the scratchpad; wait for it or kill it before the pre-gate.

## Decisions

You rule your review findings; nobody waits for Björn. Record each in the PR under **Rulings** (fork / ruling / reasoning / cost of reversing) and as a comment on the issue. The implementer's limits apply: never contradict a ticket, spec, ADR or the glossary; a step reserved for Björn is ruled "Reserved for Björn, ruled by merge-manager on Opus"; a PR line "Not witnessable by an agent" is not a blocker, merge on the rest and list it under *Milestone review*; a criterion the implementer marked unsatisfiable on correct code: check the claim, and if right it stays ticked with that note, never satisfied by bending the code.

## Review

Yours is the ticket's only review. Start as soon as the PR opens, even as a draft.

1. `gh pr view <n> --json body,comments`; `gh issue view <N> --json body,comments`. The issue body is the spec; the parent spec and milestone description are context.
2. `mattpocock-skills:code-review`, both axes: Standards (repo standards) and Spec (criteria, out-of-scope list). Add a deep pass against the ADRs and `docs/contract.md`, with the PR's **Frozen surface** section held to the diff, when the diff touches a frozen surface: §10.8's list, the write queue, concurrency or locking.
3. Verify claims. Check that the mutation proof is present when the implementer brief says it is owed; add a mutant only for a claim you find untested. Ask whether the fixture can witness the claim and which direction is unwitnessed. A finding about a ticket you haven't read is a hypothesis: read it first.
4. Don't open screenshot PNGs; they are for Björn. Review the code.
5. Fix on the branch yourself (`git status --short`, named paths) per *Fix it in place first*; what fails its fit test you file and report with the finding quoted. Cap: 3 rounds, then stop and report.
6. Re-run a live suite only if your fixes or the rebase touched what it exercises (`docs/agents/working-model.md`, *Live runs*).

## The gate

Yours are the ticket's only full gates: `just gate` on the head you merge, rebased onto `origin/main`, once before the lock and again under the lock only when `origin/main` moved since. Foreground, Bash timeout 600000, `export CARGO_BUILD_JOBS=4` first; under the lock always as `env -C <worktree> bash <lock> hold <PR> just gate`. It picks `check-front` for a frontend-only head, else `just check`; report which. Expect about 200 s with the slot free and 300 to 350 s when another gate's test run holds it. Red is yours to diagnose.

- `just gate` runs `just pre-gate` first, which fixes formatting and the generated IPC mirror. If it ends with "pre-gate changed files", commit them before the merge: the gate ran on them.
- The gate waits at most 150 s for the test slot (`KNOBAS_GATE_SLOT_WAIT`) and exits 75 naming the holders. Exit 75 is a queue timeout, not a red gate: rerun with `KNOBAS_GATE_SLOT_WAIT=900` in the background (under `hold` when under the lock), its log ending in an exit line, and wait per the implementer brief's *Waiting*. A `just *-live` recipe that queues on a machine-wide lock runs the same way.
- Contention, not the diff: only `Failed to start forks worker` / `Timeout waiting for worker`, or `initdb` / `pg_ctl` / `shmget: No space left on device`, with zero assertion failures. Check `ipcs -m`; after a killed run, `ipcrm -m` segments with no live postmaster; wait a minute, re-run once, say so.
- After the rebase, if `app/package-lock.json` changed and now equals the root's: `cp -Rpc` the root's `app/node_modules` over the worktree's and `touch` its `.package-lock.json` before gating. No frontend half, no merge.
- `gh pr checks` saying "no checks reported" is normal; Actions are off.

## Merge tail

Lock script from your dispatch. Start once the PR is ready and your fixes are pushed. No merge lands a tree no gate saw.

1. **Pre-gate, no lock.** `git fetch origin`, rebase onto `origin/main`, `MAIN=$(git rev-parse origin/main)`, then `env -C <worktree> just gate`, bare (not under `hold`; you hold nothing). Red: fix, push, pre-gate again. Green: commit what pre-gate changed, `git push --force-with-lease`, then `bash <lock> ready <PR> $(git rev-parse HEAD) $MAIN`. `$MAIN` is the `origin/main` the gated rebase sat on, never a fresh fetch after the gate. After any later commit or rebase, gate again before calling `ready` again.
2. `bash <lock> take <PR>` in the foreground. Exit 0: held. Exit 4: still queued, call it again, as often as it takes, never in the background.
3. **Fresh or stale.** Holding the lock: `env -C <worktree> bash <lock> gated <PR>`. Exit 0: `main` has not moved and your head is what `ready` recorded, so the squash produces the gated tree; go to *Merge and cleanup* with `<sha>` = your `HEAD`. Any other exit (it says why): `git fetch origin`, rebase onto `origin/main`, `bash <lock> hold <PR> just gate`, merge if green. A stale pre-gate is not a red gate; it costs the re-gate, nothing else.
4. `bash <lock> release <PR>` right after `gh pr merge` succeeds, or when you give up. Cleanup needs no lock.

**Keeping the lock**: the lock is one holder slot on `origin` that every machine shares (`refs/locks/merge-tail`, #2335), so `take` may be waiting on a merge on the other machine; the queue order is per machine. It is a lease. Your claim dies 600 s after your last lock call (`KNOBAS_MERGE_TAIL_LEASE`) and the next waiter takes over. `assert`, `gated` and a re-entrant `take` refresh it; anything that runs over a minute under the lock runs as `bash <lock> hold <PR> <command…>`, which refreshes it every 30 s. Nothing that waits on another machine (a `gh run`, a live product's queue) happens under the lock. If `assert` says you lost it (exit 1), stop; a gate run meanwhile was wasted, nothing worse.

## Merge and cleanup

1. `gh pr view <n> --json closingIssuesReferences` must list the issue (`Closes #N` outside backticks; `gh pr edit` if not).
2. After a force-push: `until [ "$(gh pr view <n> --json mergeable --jq .mergeable)" = MERGEABLE ]; do sleep 5; done`, give up after 2 min.
3. `bash <lock> assert <PR> && gh pr merge <n> --squash --delete-branch --match-head-commit <sha>`, short imperative subject, in one Bash call. `assert` exits 1 when your PR no longer holds the lock: stop and report, never merge. `fatal: 'main' is already used by worktree` on exit 1 means it merged; confirm with `gh pr view <n> --json state,mergedAt`.
4. Release the lock.
5. `git ls-remote --heads origin <branch>`; if present, `git push origin --delete <branch>`. Only the PR's own branch: a `pr-assets/*` branch stays until Björn deletes it himself (implementer brief, *Screenshots*).
6. `gh issue view <N> --json state`; if open, close with a comment naming PR and commit.
7. `git -C /Users/dev/Projects/knobas worktree remove .worktrees/<name>`, the one git command allowed outside your worktree, post-merge only. Dirty: report, never force. Already gone is fine.
8. Leave nothing running (implementer brief, *Live suites*), also when you stop early.

Scratch files in the scratchpad from your dispatch, PR number in every filename.

## Report

Exactly this shape, one line each, `none` where empty:

```
Merged: #<PR> → <sha> (closes #<N>)
Gate: <check|check-front> exit <n>, <s> s, Rust <passed>/<failed>, front <tests>
Review: <what the review and deep pass found>
Fixes: <commits>
Rulings: <one line each>
Follow-ups: <#n>
Milestone review: <criteria handed on>
Notes: <one line>
```

## Attribution

(The orchestrator replaces this block with the session's.)

Commit messages end with `Co-Authored-By: <model> <noreply@anthropic.com>`. PR bodies end with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.
