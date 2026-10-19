# Merge-manager brief — common rules (knobas)

You review, fix, gate, squash-merge and clean up the ONE pull request named in your dispatch (plus a batch partner's merge, see *Merge tail*).

## No permission prompts

The loop runs unattended; one prompt stalls it until Björn is back. Never: chain `cd` into a compound command (use absolute paths, `git -C`, `npm --prefix`); write, delete or move under `~/.claude/` or any `.git/` from Bash (git commands are fine); edit shell config. Skip such a step and name it in your report.

## Before you start

Read `docs/agents/working-model.md`, `CLAUDE.md`, `docs/agents/issue-tracker.md`, `CONTEXT.md`. In the worktree from your dispatch: `git fetch && git status`; the branch must be pushed and clean, else **report, never force**.

**Pre-build** unless `scripts/gate-scope.sh` says `front`: `nice -n 10 env -u RUSTUP_TOOLCHAIN CARGO_BUILD_JOBS=4 cargo test --workspace --no-run` in the background, log in the scratchpad. Wait for it or kill it before taking the lock.

## Decisions

**You rule your review findings**; nobody waits for another agent or for Björn. Record each ruling in the PR under **Rulings** (fork / ruling / reasoning / cost of reversing) and in `docs/decisions/<yyyy-mm>-unattended-rulings.md` in the same PR. A ruling may not contradict a ticket, spec, ADR or the glossary; where one is wrong, follow it, file a `ready-for-agent` follow-up, and say so.

A step **reserved for Björn** that the implementer left open: on Opus, rule it ("Reserved for Björn, ruled by merge-manager on Opus"), tick the criterion, name the ruling beside it. On Sonnet, finish the review but **stop before taking the lock** and report the step; the orchestrator sends an Opus merge-manager.

A PR line "Not witnessable by an agent, handed to the milestone review" is not a blocker: merge on the rest and list it under *Milestone review* in your report. A criterion the implementer marked as unsatisfiable on correct code: check the claim; if right, it is ticked with that note, never satisfied by bending the code.

## Review

Start as soon as the PR opens, even as a draft.

1. `gh pr view <n> --json body,comments`; `gh issue view <N> --json body,comments`. The issue body is the spec; parent spec and milestone description are context.
2. `mattpocock-skills:code-review`, **Spec axis** (criteria, out-of-scope list); the implementer did Standards. Add a **deep pass** against ADR and contract (and the required §10.8 entry, which must match the diff exactly) when the diff touches a frozen surface: §10.8 IPC, migrations, write queue, source adapters (`crates/knobas-source/src/**`, `crates/knobas-http/**`), concurrency or locking.
3. Verify claims. Don't redo the implementer's mutants; add one only for a claim you find untested. Ask whether the fixture can witness the claim, and which direction is unwitnessed. A finding about a ticket you haven't read is a hypothesis: read it first.
4. Don't open screenshot PNGs; review the named differences and the code. An image only to settle a doubt the code can't answer.
5. Fix on the branch yourself (`git status --short`, named paths). Out-of-scope fixes: report with the finding quoted. Cap: 3 rounds, then stop and report.
6. Re-run a live suite only if your fixes or the rebase touched what it exercises.

**Follow-ups:** `ready-for-agent` if Björn would notice it in the app, else `backlog`.

## The gate

Yours is the ticket's only full gate: `just gate` on the **final merge head** (after fixes and rebase), **foreground**, Bash timeout 600000, `export CARGO_BUILD_JOBS=4` first. It picks `check-front` for a frontend-only head, else full `just check`; report which. Expect 150 s median, ~290 s p90 with two gates running. Red is yours to diagnose.

- A full gate runs `just pre-gate` itself first (fix commits and the rebase land after the implementer's pre-gate; 32 of 112 failed gates in Oct 10–19 were clippy or rustfmt). If it ends with "pre-gate changed files", commit them before the merge: the gate ran on them.
- Start any piped test or gate command with `set -o pipefail;`, or `| tail` turns a red run into exit 0. Never poll with `sleep N; cat log`; foreground or Monitor.

- Contention, not the diff: only `Failed to start forks worker` / `Timeout waiting for worker`, or `initdb` / `pg_ctl` / `shmget: No space left on device` (SysV `shmmni` is 32; each embedded postmaster holds one segment), with zero assertion failures. Check `ipcs -m`; after a killed run, `ipcrm -m` segments with no live postmaster; wait a minute, re-run once, say so.
- After the rebase, if `app/package-lock.json` changed and now equals the root's, `cp -Rpc` the root's `app/node_modules` over the worktree's before gating. No frontend half, no merge.
- Raw cargo: `env -u RUSTUP_TOOLCHAIN cargo …`; one build of yours at a time. `gh pr checks` "no checks reported" is normal (Actions are off). No pagers; macOS has no `timeout` binary.

## Merge tail (under the lock)

Lock script from your dispatch. Take the lock only once the PR is **ready** and your fixes are pushed.

1. `bash <lock> ready <PR> $(git rev-parse HEAD)`, then `bash <lock> take <PR>` (foreground). **Exit 3**: a partner merged your PR in a batch; confirm the issue closed, delete branch and worktree, `bash <lock> release <PR>`, report the merge SHA it printed.
2. Holding the lock: rebase onto `origin/main`; `MAIN=$(git rev-parse origin/main)`. Merge only against SHAs you recorded.
3. `bash <lock> partner <PR>`. Nothing printed: gate your rebased head, merge alone. `<B> <B-sha>` printed:
   1. `git fetch origin "pull/<B>/head"`; if `gh pr view <B> --json headRefOid` ≠ `<B-sha>`, merge alone.
   2. `git switch -c batch-<PR>-<B>` from your rebased head; `git cherry-pick $(git merge-base $MAIN <B-sha>)..<B-sha>`. Any conflict: `--abort`, back to your branch, merge alone.
   3. `just gate $MAIN` once on the batch head, foreground.
   4. Green: push your rebased head, merge yours `--match-head-commit`, `git fetch`, then `git rebase --onto origin/main <your rebased head>` the batch branch; its `HEAD^{tree}` must equal the gated batch tree. Push `git push --force-with-lease=<B branch>:<B-sha> origin HEAD:<B branch>`, merge B `--match-head-commit`, `bash <lock> merged <B> <merge SHA>`. Trees differ or push refused: stop, B's merge-manager carries on alone.
   5. Red: gate your own head alone, merge if green, say which PR failed if you can tell. B's cleanup stays with B's merge-manager.
4. `merged` is only for a partner; for your own PR only `release`.
5. `bash <lock> release <PR>` right after the last `gh pr merge` succeeds, or when you give up. Cleanup needs no lock.

## Merge and cleanup

1. Push fixes; `gh pr view <n> --json closingIssuesReferences` must list the issue (`Closes #N` outside backticks; `gh pr edit` if not).
2. After a force-push: `until [ "$(gh pr view <n> --json mergeable --jq .mergeable)" = MERGEABLE ]; do sleep 5; done`, give up after 2 min.
3. `gh pr merge <n> --squash --delete-branch --match-head-commit <sha>`, short imperative subject. `fatal: 'main' is already used by worktree` on exit 1 means it merged; confirm `gh pr view <n> --json state,mergedAt`.
4. Release the lock.
5. `git ls-remote --heads origin <branch>`; if present, `git push origin --delete <branch>`.
6. `gh issue view <N> --json state`; if open, close with a comment naming PR and commit.
7. `git -C /Users/dev/Projects/knobas worktree remove .worktrees/<name>` (the one git command allowed outside your worktree, post-merge only; `target` goes with it). Dirty: **report, never force**.

Scratch files in the scratchpad from your dispatch, PR number in every filename.

## Report

Exactly this shape, one line each, `none` where empty:

```
Merged: #<PR> → <sha> (closes #<N>)[; batched #<B> → <sha>]
Gate: <check|check-front> exit <n>, <s> s, Rust <passed>/<failed>, front <tests>
Review: <what the Spec axis and deep pass found>
Fixes: <commits>
Rulings: <one line each>
Follow-ups: <#n>
Milestone review: <criteria handed on>
Notes: <one line>
```

## Attribution

(The orchestrator replaces this block with the session's.)

Commit messages end with `Co-Authored-By: <model> <noreply@anthropic.com>`. PR bodies end with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.
