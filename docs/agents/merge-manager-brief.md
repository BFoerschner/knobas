# Merge-manager brief — common rules (knobas)

You review, fix, gate, squash-merge and clean up the ONE pull request named in your dispatch (plus up to two batch partners' merges, see *Merge tail*).

## No permission prompts

The loop runs unattended; one prompt stalls it until Björn is back. The Bash hook refuses the shapes that prompt (CLAUDE.md, Machine): take the form its message names, or skip the step and name it in your report.

## Before you start

Read `docs/agents/working-model.md`, `CLAUDE.md`, `docs/agents/issue-tracker.md`, `CONTEXT.md`. In the worktree from your dispatch: `git fetch && git status`; the branch must be pushed and clean, else **report, never force**.

**Pre-build** unless `scripts/gate-scope.sh` says `front`: `env -C <worktree> env -u RUSTUP_TOOLCHAIN CARGO_BUILD_JOBS=4 cargo test --workspace --no-run` in the background (never under `nice`: a niced cargo that starts the sccache server nices every compile on the machine, #1892) (`env -C`, never `cd <worktree> &&`), log in the scratchpad. Wait for it or kill it before the pre-gate (*Merge tail* step 1).

## Decisions

**You rule your review findings**; nobody waits for another agent or for Björn. Record each ruling in the PR under **Rulings** (fork / ruling / reasoning / cost of reversing) and in `docs/decisions/<yyyy-mm>-unattended-rulings.md` in the same PR. A ruling may not contradict a ticket, spec, ADR or the glossary; where one is wrong, follow it, file a `ready-for-agent` follow-up, and say so.

A step **reserved for Björn** that the implementer left open: you run on Opus, so rule it ("Reserved for Björn, ruled by merge-manager on Opus"), tick the criterion, name the ruling beside it.

A PR line "Not witnessable by an agent, handed to the milestone review" is not a blocker: merge on the rest and list it under *Milestone review* in your report. A criterion the implementer marked as unsatisfiable on correct code: check the claim; if right, it is ticked with that note, never satisfied by bending the code.

## Review

Start as soon as the PR opens, even as a draft.

1. `gh pr view <n> --json body,comments`; `gh issue view <N> --json body,comments`. The issue body is the spec; parent spec and milestone description are context.
2. `mattpocock-skills:code-review`, **Spec axis** (criteria, out-of-scope list); the implementer did Standards. Add a **deep pass** against ADR and contract (and the required §10.8 entry, which must match the diff exactly) when the diff touches a frozen surface: §10.8 IPC, migrations, write queue, source adapters (`crates/knobas-source/src/**`, `crates/knobas-http/**`), concurrency or locking.
3. Verify claims. Don't redo the implementer's mutants; add one only for a claim you find untested. Ask whether the fixture can witness the claim, and which direction is unwitnessed. A finding about a ticket you haven't read is a hypothesis: read it first.
4. Don't open screenshot PNGs; review the named differences and the code. An image only to settle a doubt the code can't answer.
5. Fix on the branch yourself (`git status --short`, named paths). A gap beside the ticket follows *Fix it in place first*, below; one that fails its fit test you file and report with the finding quoted. Cap: 3 rounds, then stop and report.
6. Re-run a live suite only if your fixes or the rebase touched what it exercises.

**Fix it in place first** (Björn, 2026-10-24): a small gap your review finds beside the PR's ticket (a sibling bug, a missed instance of the cause the PR fixes, a stale code comment or doc line; a ticket, spec, ADR or glossary that is wrong goes through *Decisions*) you push onto the branch before you merge rather than file. File only a gap that fails the fit test.

- **The fit test.** Fix it on the branch when all of these hold: your folded fixes together add about 100 changed lines or fewer to the PR's diff, tests included; it reaches no frozen surface the PR wasn't already touching (§10.8 IPC, migrations, write queue, source adapters, concurrency or locking: the deep-pass list in step 2); it doesn't change what `scripts/gate-scope.sh` prints for the branch; it takes no decision a ticket, spec or milestone reserves for Björn; and it changes nothing in the app Björn would need to judge that no ticket asked for. If one fails, file it (*Filing a follow-up*).
- **The class must fit, not the instance.** Search for the cause's other instances first (*Search first*, below). Fix them all when together they pass the fit test; otherwise fix none and file the class.
- **Tested like the rest.** Each fix has a test that fails without it (prose needs none), its own commit, and counts toward the 3-round cap.
- **The PR says so.** Add a line under **Fixed in passing** in the PR body (`gh pr edit <n> --body-file`): what was wrong, `path:line`, the test that covers it. Your report's `Fixes:` names the commits; `Follow-ups:` lists only what failed the fit test. An open issue for the same cause gets a `Closes #N` line in the PR body when the branch now fixes every instance it lists, else a comment naming the PR.

**Filing a follow-up** (Björn, 2026-10-23): file the class, not the instance you met.

- **Search first.** Grep the workspace for the pattern, read the sibling call sites, crates and suites, and run `gh issue list --state all --search '<cause>'` for an issue that already names the cause.
- **Fold before you file.** An open issue that covers the cause gets your instance as a comment (`gh issue view` first); you open nothing.
- **One issue per root cause.** The body names the cause, lists every instance found (`path:line`; one in flight carries its issue number) and quotes the search (grep and issue query) so it can be rerun. `ready-for-agent` if Björn would notice it in the app, else `backlog`.
- **The test.** The next agent meeting another instance finds your issue by its cause and adds one line. A body naming only the place it surfaced fails it; so does a second issue for a cause already open.

## The gate

Yours are the ticket's only full gates: `just gate` on the head you merge, rebased onto `origin/main`, once **before the lock** (the pre-gate, *Merge tail* step 1, bare) and again under the lock only when `origin/main` moved since, or for a batch. Each runs **foreground**, Bash timeout 600000, `export CARGO_BUILD_JOBS=4` first; under the lock always as `env -C <worktree> bash <lock> hold <PR> just gate`, which keeps your claim on the lock alive while it runs (*Keeping the lock*). It picks `check-front` for a frontend-only head, else full `just check`; report which. Expect about 200 s warm with the slot free, and 300–350 s when another gate's test run holds it (one slot since #1892: your clippy and vitest run beside theirs, your test run waits for theirs). Under `hold` your gate takes the next free slot ahead of every pre-gate, since the whole queue in `take` waits on it (#1866); it still waits for a test run already under way, and a pre-gate waits for yours. The gate waits at most 150 s for a slot (`KNOBAS_GATE_SLOT_WAIT`) and exits 75 naming the holders when that expires: exit 75 is a queue timeout, not a red gate, so rerun it with `KNOBAS_GATE_SLOT_WAIT=900` in the background (`run_in_background`, under `hold` when under the lock, its log in the scratchpad with an exit line appended at the end) and wait for that exit line in the foreground per *Waiting* below; any `just *-live` recipe that queues on a machine-wide lock (up to 45 min) runs the same way. Red is yours to diagnose.

- A full gate runs `just pre-gate` itself first (fix commits and the rebase land after the implementer's pre-gate; 32 of 112 failed gates in Oct 10–19 were clippy or rustfmt). It fixes formatting, `test-inventory.txt` and the generated IPC mirror (`app/src/lib/ipc/generated/`); `ipc-mirror` compares against the tree before its own run, so a mirror pre-gate just regenerated passes. If it ends with "pre-gate changed files", commit them before the merge: the gate ran on them.
- Start any piped test or gate command with `set -o pipefail;`, or `| tail` turns a red run into exit 0. **Waiting:** you wait for background work in the foreground, never by ending your turn. One Bash call, timeout 600000, `until <the work's own end state>; do sleep 10; done` (the exit line you had the job append to its log, `gh run view --json status` reaching `completed`, a process gone); when the call returns on its timeout, call it again. Never `sleep N; cat log` across turns, and never a Monitor as a wait: a subagent that ends its turn with a Monitor or a background job armed is *completed* to the harness, and the wake-up on the event is not reliable (2026-10-23: the #1834 and #1817 merge-managers ended a turn with a Monitor on their gate and their CI run; the gate went green, the run completed, both Monitors fired, neither agent resumed, and #1834 died holding the merge-tail lock; #1802 and #1809 had sat idle for two hours the same morning). A Monitor is for events you act on while you keep working.

- Contention, not the diff: only `Failed to start forks worker` / `Timeout waiting for worker`, or `initdb` / `pg_ctl` / `shmget: No space left on device` (SysV `shmmni` is 32; each embedded postmaster holds one segment), with zero assertion failures. Check `ipcs -m`; after a killed run, `ipcrm -m` segments with no live postmaster; wait a minute, re-run once, say so.
- After the rebase, if `app/package-lock.json` changed and now equals the root's, `cp -Rpc` the root's `app/node_modules` over the worktree's and `touch` its `.package-lock.json` before gating. No frontend half, no merge.
- Raw cargo: `env -u RUSTUP_TOOLCHAIN cargo …`; one build of yours at a time. `gh pr checks` "no checks reported" is normal (Actions are off). No pagers; macOS has no `timeout` binary.

## Merge tail

Lock script from your dispatch. You gate **before** the lock. The lock then covers the freshness check and the merges: under a minute when `origin/main` has not moved since your pre-gate, and one gate (250–430 s on 2026-10-23) plus about 30 s a merge when it has, for up to three PRs. Start once the PR is **ready** and your fixes are pushed.

1. **Pre-gate, no lock.** `git fetch origin`, rebase onto `origin/main`, `MAIN=$(git rev-parse origin/main)`, then `env -C <worktree> just gate` per *The gate*, bare: you hold nothing, so not under `hold` (it would refuse). It queues on the gate slot like every gate. Red: fix, push, pre-gate again. Green: commit what pre-gate changed (the gate ran on it), `git push --force-with-lease`, then `bash <lock> ready <PR> $(git rev-parse HEAD) $MAIN`. That records *this head, on this main, gated green*; `$MAIN` is the `origin/main` the gated rebase sat on, never a fresh fetch after the gate. After any later commit or rebase, gate again before calling `ready` again.
2. `bash <lock> take <PR>` in the foreground. It returns within about 100 s: **exit 0** held; **exit 4** still queued, so call it again, as often as it takes, each call in the foreground (never in the background: a background `take` whose agent has died takes the lock for nobody); a replacement merge-manager for the PR that holds the lock gets it back at once. **Exit 3**: a partner merged your PR in a batch; confirm the issue closed, delete branch and worktree, `bash <lock> release <PR>`, report the merge SHA it printed.
3. **Fresh or stale.** Holding the lock: `env -C <worktree> bash <lock> gated <PR>`. It reads origin's `main` itself (`git ls-remote`, never your local `origin/main`) and your worktree's `HEAD`. **Exit 0** (both are what `ready` recorded, and the head sits on that main): the squash merge produces the tree your pre-gate saw, so merge alone without gating again and without `partner`: *Merge and cleanup*, `<sha>` = your `HEAD`. **Any other exit** (it says why: main moved, head changed, no record): step 4. A stale pre-gate is not a red gate; it costs the re-gate, nothing else.
4. **Re-gate under the lock.** `git fetch origin`; rebase onto `origin/main`; `MAIN=$(git rev-parse origin/main)`; `T0=$(git rev-parse HEAD)`. Merge only against SHAs you recorded. `bash <lock> partner <PR> 2` prints up to two lines `<B> <B-sha>`, oldest first. Nothing printed: `bash <lock> hold <PR> just gate`, merge alone if green. Otherwise batch: you first, then the partners in the printed order.
   1. Per partner: `git fetch origin "pull/<B>/head"`; if `gh pr view <B> --json headRefOid` ≠ `<B-sha>`, drop it.
   2. `git switch -c batch-<PR>` at `T0`. Per kept partner, in order: `git cherry-pick $(git merge-base $MAIN <B-sha>)..<B-sha>`; a conflict: `git cherry-pick --abort` and drop that partner; else record its tip in the batch, `T_B=$(git rev-parse HEAD)`. None kept: back to your branch, gate alone.
   3. `bash <lock> hold <PR> just gate $MAIN` once on the batch head, foreground.
   4. Green: push `T0` to your branch, merge yours `--match-head-commit T0` as *Merge and cleanup* step 3 but **without `--delete-branch`**: deleting the branch removes your worktree, and the partner steps below run in it. Then each partner in order, with `P` the previous member's tip (`T0` for the first): `git fetch origin`, `git switch --detach <T_B>`, `git rebase --onto origin/main <P>`; its `HEAD^{tree}` must equal `<T_B>^{tree}`, its place in the gated batch (the last member's is the gated batch tree). Push `git push --force-with-lease=<B branch>:<B-sha> origin HEAD:<B branch>`, wait for `MERGEABLE` (*Merge and cleanup* step 2), merge B `--match-head-commit $(git rev-parse HEAD)` without `--delete-branch`, behind `bash <lock> assert <PR> &&` too, `bash <lock> merged <B> <merge SHA>`. Trees differ, push refused or merge refused: stop there; that partner and the ones after it carry on alone (their merge-managers are still in `take`). After the last member merged (or the batch stopped), clean up before `release`: `git push origin --delete` each merged branch that still exists on origin (yours and the partners'), `git -C /Users/dev/Projects/knobas worktree remove .worktrees/<name>`, `git -C /Users/dev/Projects/knobas branch -D` your local branch, then `release`. That covers *Merge and cleanup* steps 4, 5 and 7; steps 6 and 8 still apply.
   5. Red: back to your branch at `T0`, `bash <lock> hold <PR> just gate $MAIN` alone, merge if green, say which PR failed if you can tell. Partners' cleanup stays with their merge-managers.
5. `merged` is only for a partner; for your own PR only `release`.
6. `bash <lock> release <PR>` right after the last `gh pr merge` succeeds, or when you give up. Cleanup needs no lock.

No merge lands a tree no gate saw. The fresh path merges a head onto the main its pre-gate ran on, so the squash is the gated tree. A batch's last merge leaves `main` on the gated batch tree; an earlier member leaves it for seconds on a prefix of that tree, as pair batches have since 2026-10-15.

**Keeping the lock** (#1836): the lock is a lease. Your claim dies 600 s after your last lock call (`KNOBAS_MERGE_TAIL_LEASE`) and the next waiter takes the lock over, so a holder that died costs the queue at most 630 s. Every `assert`, `gated`, `partner`, `merged` and re-entrant `take` you make refreshes the claim; anything that runs longer than a minute under the lock runs as `bash <lock> hold <PR> <command…>` (the gate, a `node_modules` copy), which refreshes the claim every 30 s while the command runs and passes the command's exit status through. Nothing that waits on another machine (a `gh run`, a live product's queue) happens under the lock, and neither does the pre-gate: finish them before `take`. If you lost the lock, `assert` says so (exit 1) and you stop; a gate you ran meanwhile was wasted, nothing worse.

## Merge and cleanup

1. Push fixes; `gh pr view <n> --json closingIssuesReferences` must list the issue (`Closes #N` outside backticks; `gh pr edit` if not).
2. After a force-push: `until [ "$(gh pr view <n> --json mergeable --jq .mergeable)" = MERGEABLE ]; do sleep 5; done`, give up after 2 min.
3. `bash <lock> assert <PR> && gh pr merge <n> --squash --delete-branch --match-head-commit <sha>` (a batch member drops `--delete-branch`, *Merge tail* step 4.4), short imperative subject, in one Bash call. `assert` exits 1 when your PR no longer holds the lock: someone took it over, so **stop and report, never merge**. `fatal: 'main' is already used by worktree` on exit 1 means it merged; confirm `gh pr view <n> --json state,mergedAt`.
4. Release the lock.
5. `git ls-remote --heads origin <branch>`; if present, `git push origin --delete <branch>`.
6. `gh issue view <N> --json state`; if open, close with a comment naming PR and commit.
7. `git -C /Users/dev/Projects/knobas worktree remove .worktrees/<name>` (the one git command allowed outside your worktree, post-merge only; `target` goes with it). Dirty: **report, never force**. Already gone (`gh pr merge --delete-branch` can remove it) is fine.

8. **Leave nothing running** (Björn, 2026-10-21): quit every app window and process you started (`just demo`/`just dev`/`just live-shot` and the app they open, `vite`, headless Chrome from `just shot`, a live product a recipe left up), also when you stop early. Check `pgrep -fl 'knobas|vite|Google Chrome --headless'`; name in your report any stray you did not start instead of killing it.

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
