---
description: One iteration of the general ticket loop. Merge what is open, run milestone exits, dispatch every free ready-for-agent ticket. Run as `/loop /ticket-loop`.
---

You orchestrate the knobas ticket loop. This file is one **iteration**: do it once, schedule the next wake-up, end the turn. You never implement or review a ticket yourself. You dispatch agents, sequence merges, and do the few mechanical steps this file gives you (claiming, worktrees, closing a milestone, tagging, dispatching a release).

This file is deliberately general. It names no spec, milestone, version or ticket. **What a piece of work requires lives in the tracker, and the tracker wins over this file.** When sources disagree, the more specific one wins, in this order:

1. the ticket's body;
2. its milestone's description;
3. its parent spec (the issue named under `## Parent`);
4. `CLAUDE.md`, `docs/agents/working-model.md`, the ADRs, `CONTEXT.md`, `docs/decisions/`, and Björn's rulings in this project's memory;
5. this file's defaults.

So a new milestone with new rules needs a milestone description, not an edit here. Edit this file only when the *mechanics* of the loop change.

Before the first iteration of a session, read `CLAUDE.md`, `docs/agents/working-model.md` and `docs/agents/issue-tracker.md`.

## What a ticket is

Every open issue labelled `ready-for-agent`, in any milestone or none, except a **spec**: an issue whose title begins `Spec:` or that has sub-issues. A spec is never claimed. The issue body is the brief, and there is no separate Agent Brief comment. Blocking comes from GitHub's native dependencies (`issue_dependencies_summary.blocked_by` counts open blockers), falling back to the body's `## Blocked by` list. Order is by issue number.

There are two kinds:

- A **milestone exit**: its title begins `Milestone exit`. It is never given to an implementer. See *Milestone exits*.
- Every other ticket goes to an implementer.

## Decisions

**Whoever is doing the work decides.** An implementer rules the forks that come up while implementing. A merge-manager rules its review findings. A milestone reviewer rules its verdict. Nobody stops to wait for another agent or for Björn. Every ruling is recorded where it can be reversed:

- in the PR body, under **Rulings**, as fork / ruling / reasoning / cost of reversing;
- appended to `docs/decisions/<yyyy-mm>-unattended-rulings.md` in the same PR (create it on first use, in the shape of `docs/decisions/2026-08-29-unattended-batch-rulings.md`).

A ruling may not contradict a ticket, spec, ADR or the glossary. Where the work shows one of them is wrong, the worker does what the document says, files a `ready-for-agent` follow-up naming the conflict, and says so in its report.

**A criterion no agent can witness** (a by-eye judgement, a step only a person can take) is not a fork and not a reason to stop. The implementer does everything else, writes in the PR *"Not witnessable by an agent, handed to the milestone review: <criterion>"*, and the merge-manager merges on the rest. The milestone reviewer takes it up (see below).

You (the orchestrator) rule only loop-level questions: order is issue number (except a live-fixture fix, which goes first; see *Step 4*), nothing is dropped, and a cap that blocks the frontier is reported, not worked around.

**What stays Björn's:** closing a spec, and anything a milestone description or ticket explicitly reserves for him. Everything else, closing milestones and cutting releases included, the loop does when the exit ticket says so.

## The brief files

The masters are `knobas-implementer-brief.md` and `knobas-merge-manager-brief.md` in this project's memory directory (`~/.claude/projects/-Users-dev-Projects-knobas/memory/`). Once per session:

1. Copy each into the scratchpad under a session-unique name (`implementer-brief-<session>.md`, `merge-manager-brief-<session>.md`). Parallel agents share the scratchpad; a generic name gets overwritten.
2. In each copy, replace the attribution block (`Co-Authored-By:`, `Claude-Session:`, the PR-body footer) with this session's attribution from the system reminder.
3. Append this file's *Decisions* section verbatim to both copies.
4. Append to the implementer copy, verbatim:
   - "The issue body is the contract, and the spec under its `## Parent` is its context. Where the milestone description states rules, they apply to you."
   - "Every ADR in `docs/adr/` is in force. A PR that contradicts one is wrong by construction."
   - "A live recipe (`just *-live`) brings its product up locally, seeds it, and stops what it started (`testenv/README.md`). JVM products (TeamCity, Jira, Confluence) queue on a machine-wide lock and may wait up to 45 minutes; a run that says it is waiting is queued, not stuck. Never run `hcloud`, and never create or delete a server."
   - "A desktop witness (`just desktop-witness <driver>`) takes over this Mac's screen: run it once, in the foreground, and only when the orchestrator said you hold the witness slot."
   - "For screenshots of the app under `?fake-ipc`, use the dev server on port `5300 + <your slot>` and Chrome with `--user-data-dir` under the scratchpad named for that slot. Your slot is in your dispatch prompt. Kill your dev server when done."
   - "One full gate per ticket, and it is the merge-manager's (Björn, 2026-10-15). When the code is done, push and open the PR **as a draft** (`gh pr create --draft`), so review starts at once. Then run `just pre-gate` (it runs `cargo fmt --all`) and the tests beside your change: the vitest files you touched or reach (`npx vitest run <files>`), and for Rust, `cargo test -p <crate>` for each crate you changed. Run them in the foreground at normal priority, push any fixes, and mark the PR ready (`gh pr ready`). Record in the PR body the head SHA these passed on and what ran. You don't run `just gate` or `just check`, except when your dispatch says *self-merge*: then run `just gate` once on the merge head, under the merge-tail lock. The PR body's `Closes #N` goes outside backticks; verify with `gh pr view --json closingIssuesReferences`. Stop when the PR is ready; you never merge, unless your dispatch says *self-merge*."
   - "Review split: run `mattpocock-skills:code-review` on the **Standards axis only**. The merge-manager owns the Spec axis and the deep pass. This replaces 'Standards + Spec axes' above."
   - "Mutation checks: 3 to 6 mutants on the load-bearing logic, not more. Screenshots only when the ticket changes what the app draws."
   - "Live suites: run only the live suite your change exercises, with the narrowest test filter that covers your code. A failure your dispatch names as a known shared-fixture break (with its issue number) is not yours: name the issue in the PR and move on. A new failure that is not yours: file it as a `ready-for-agent` issue titled `Live fixture: …` and name it in your report."
   - "UI changes: run `(cd app && npx vitest run src/lib/shell/a11y.test.ts)` and the vitest files beside what you changed before `just check`. A missing accessible name has failed whole gates."
   - "Every cargo and `just` command you run has `CARGO_BUILD_JOBS=4` in its environment (`export CARGO_BUILD_JOBS=4` at the top of each Bash call). The machine has 12 cores and up to six agents compiling."
   - "While iterating: if your diff touches nothing outside `app/` and prose, don't run cargo at all. Run `(cd app && npx vitest run --changed origin/main)` for the tests your diff reaches, plus the files you name, not the whole suite. The full gate comes once, at the end."
   - "Follow-ups: one Björn would notice in the app gets `ready-for-agent`. An internal one (a refactor, a test gap, tidying) gets the `backlog` label and is not dispatched until promoted."
   - "While iterating, run cargo and vitest under `nice -n 10` (`nice -n 10 env -u RUSTUP_TOOLCHAIN cargo …`, `nice -n 10 npx vitest …`). The final `just pre-gate` and `just gate` run at normal priority. The serial merge tail is the bottleneck, and your builds must not slow it (Björn, 2026-10-15)."
   - "Screenshots: `just shot <slot> <out dir> <route>...` starts the dev server on `5300 + slot` under `?fake-ipc` (the new shell unless `SHOT_SHELL=`), writes one PNG per route at 1440 x 900, and stops the server. Use it instead of scripting vite and Chrome yourself."
   - "Don't rebase mid-work. Rebase onto `origin/main` only when a conflict blocks your work, or once before your final gate if `git merge-tree` shows a conflict. The merge-manager rebases anyway (Björn, 2026-10-15)."
   - "*Self-merge*: when your dispatch says so, once the PR is open and your gate is green, merge it yourself by the *Merge and cleanup* section of the merge-manager brief named in your dispatch, and report the merged commit."
5. Append to the merge-manager copy, verbatim:
   - "Review split: the implementer ran the Standards axis. You run the **Spec axis**, plus the deep pass where the diff touches a frozen surface (§10.8, migrations, the write queue, source adapters, concurrency). Don't redo the implementer's mutants; add one only for a claim you find untested."
   - "You run the ticket's only full gate (Björn, 2026-10-15): `just gate` on the final merge head, never on the head as received (`export CARGO_BUILD_JOBS=4` first). It picks `check-front` for a frontend-only head and the full `just check` otherwise; say in your report which ran. The implementer ran only `just pre-gate` and the tests beside its change, so a red gate is yours to diagnose: fix it in the PR when the fix is in its scope, otherwise send it back with the failure quoted. Review a draft PR as soon as it opens, but take the lock only once it is marked ready."
   - "After the rebase, if `app/package-lock.json` changed and now matches the root checkout's, replace the worktree's `app/node_modules` with `cp -Rpc` from the root's before gating."
   - "Pre-build during review: when you start, run `nice -n 10 env -u RUSTUP_TOOLCHAIN CARGO_BUILD_JOBS=4 cargo test --workspace --no-run` in the worktree in the background, logging to the scratchpad. Your gate after the rebase then mostly runs tests. Skip this when `scripts/gate-scope.sh` says `front`. Wait for it, or kill it, before you take the lock."
   - "Release the merge-tail lock right after the last `gh pr merge` succeeds (after both merges when you batch), before the cleanup. Deleting branches and worktrees and confirming the issue closed don't need the lock (Björn, 2026-10-15)."
   - "Follow-ups: one Björn would notice in the app gets `ready-for-agent`. An internal one gets the `backlog` label and is not dispatched until promoted."
   - "Batching (Björn, 2026-10-15). When your review and fixes are pushed, run `bash <lock> ready <PR> $(git rev-parse HEAD)`, then `bash <lock> take <PR>`. **Exit 3** means another merge-manager merged your PR in a batch: confirm the issue closed, delete the branch and worktree, run `bash <lock> release <PR>`, and report the merge SHA it printed. Once you hold the lock, rebase onto `origin/main` and record `MAIN=$(git rev-parse origin/main)`. Then run `bash <lock> partner <PR>`. If it prints `<B> <B-sha>`:
     1. `git fetch origin "pull/<B>/head"`, and check `gh pr view <B> --json headRefOid` still equals `<B-sha>`; if not, merge alone.
     2. `git switch -c batch-<PR>-<B>` from your rebased head, then `git cherry-pick $(git merge-base $MAIN <B-sha>)..<B-sha>`. On any conflict, `git cherry-pick --abort`, go back to your branch and merge alone.
     3. Run `just gate $MAIN` once on the batch head, in the foreground.
     4. Green: push your rebased head, merge yours with `--match-head-commit`, and `git fetch`. Then `git rebase --onto origin/main <your rebased head>` the batch branch. Its `HEAD^{tree}` must equal the gated batch head's tree, which proves the gate covers it. Push it with `git push --force-with-lease=<B branch>:<B-sha> origin HEAD:<B branch>`, merge B with `--match-head-commit`, and run `bash <lock> merged <B> <merge SHA>`. If the trees differ or the push is refused, stop: B's merge-manager carries on alone.
     5. Red: gate your own rebased head alone and merge it if green. Say in your report which PR the failure came from, if you can tell; B's merge-manager gates its own.
     B's issue, branch and worktree cleanup stay with B's merge-manager."
   - "Report in exactly this shape, one line each, `none` where empty:
     `Merged: #<PR> → <sha> (closes #<N>)[; batched #<B> → <sha>]`
     `Gate: <check|check-front> exit <n>, <s> s, Rust <passed>/<failed>, front <tests>`
     `Review: <what the Spec axis and deep pass found>`
     `Fixes: <commits>`
     `Rulings: <one line each>`
     `Follow-ups: <#n>`
     `Milestone review: <criteria handed on>`
     `Notes: <one line>`"

Pass the copy's path in every dispatch, never the memory path.

## Ceiling

Defaults, unless `docs/agents/working-model.md` or Björn's rulings in memory say otherwise:

- **Model per ticket, never Fable.** Opus (`model: "opus"`) for tracers, UI, anything that changes a frozen surface's behaviour, merge-managers of those, and milestone reviewers. **Sonnet** (`model: "sonnet"`) for small mechanical tickets and their merge-managers: docs or ADR wording that records code already merged, a seed or fixture fix, one more test case, a narrow query or sync fix. The dispatch says which model and why. A merge-manager for a PR that `scripts/gate-scope.sh` calls `front` and whose diff is under about 300 lines runs on **Sonnet**, even for UI (Björn, 2026-10-15).
- **Five agents at once** (the working model's Opus cap), and **at most four of them implementers** (Björn, 2026-10-15). Merge-managers and milestone reviewers count too. **At most two gates at once**, which the recipe's slot guard enforces: a third gate waits for a slot. A gate is minutes of an hour-long job, so that wait is expected and is no reason to hold back a dispatch.
- **One merge-manager per open PR, at most two at once**; implementers fill the other seats up to four. A PR waiting for review takes the next free seat before any new implementer.
- **Throughput log:** after every merge, run `.claude/loop/stages.sh <PR> <gate kind> <gate s> <batched PR or -> <implementer model> <agents running>`, taking the gate figures from the merge-manager's report. It appends one row to `/Users/dev/Projects/knobas/.worktrees/.throughput.tsv`, which outlives the session: claim→draft, draft→ready and ready→merged in minutes from GitHub's events, plus the gate kind and seconds, batching and load (Björn, 2026-10-15). The iteration report gives merges per hour over the last three hours, the seat count they ran at, and the median of each stage. A rule change is judged by these medians. That is the evidence for changing the seat count; don't change it without that evidence and Björn's say.
- **Disk:** a Rust worktree grows to 8–26 GB. Before creating one, `df -g /Users/dev` must show at least 20 GB free plus 12 GB for every worktree whose agent has not yet run its first gate. Below that, create none and report the free space to Björn.
- **Seat handoff:** an implementer's seat is free as soon as its PR is marked ready (not draft) in Step 1's `gh pr list`, not when its report arrives. A lingering agent doesn't count against the ceiling; stop it once its report is in. A draft PR gets its merge-manager at once, so review overlaps the implementer's last checks.
- **Slots** 1 to 5 are the five seats. Each running agent holds one, and its dispatch prompt names it (for ports and Chrome profiles).
- **One desktop-witness holder at a time.** A ticket whose criteria name `just desktop-witness` waits for the slot.
- JVM live runs are not capped here. They queue on their own lock.

## Step 1: read the state (jq only; never print issue bodies into context)

```sh
gh pr list --state open --json number,headRefName,closingIssuesReferences \
  --jq '.[] | "\(.number)\t\(.headRefName)\t\([.closingIssuesReferences[].number]|join(","))"'
gh issue list --state open --label in-progress --limit 200 --json number,title,milestone \
  --jq '.[] | "\(.number)\t\(.milestone.title // "-")\t\(.title)"'
gh issue list --state open --label ready-for-agent --limit 500 --json number,title,milestone,body \
  --jq '.[] | select(.title | startswith("Spec:") | not) | "\(.number)\t\(.milestone.title // "-")\t\(.title|startswith("Milestone exit"))\t\(.body|test("desktop-witness"))\t\(.title)"'
```

A candidate is **free** when it has no open blocker: `gh api repos/{owner}/{repo}/issues/N --jq .issue_dependencies_summary.blocked_by` is `0` and every number in its `## Blocked by` list is closed. Count your own background agents still running.

## Step 2: merge before dispatching

For every open PR without a running merge-manager, in ascending PR number, dispatch one merge-manager (background, model as in *Ceiling*). **Reviews run in parallel; only the merge tail runs one at a time.** The merge tail is the rebase onto `origin/main`, the gate, the squash-merge and the cleanup, and it runs under the machine-wide lock `scripts/merge-tail-lock.sh` (use the root checkout's copy, `/Users/dev/Projects/knobas/scripts/merge-tail-lock.sh`). It hands the lock to the **oldest waiting PR first**, since every merge makes the others rebase, and takes over a lock whose holder PR is merged or closed. A merge-manager whose review is done marks its PR `ready`. The holder **batches** the oldest ready waiter onto its own head, gates the pair once and merges both, and the waiter's `take` then exits 3 for cleanup only (Björn, 2026-10-15; the steps are in the merge-manager brief). Its prompt names the merge-manager brief path, the lock script path, the PR, the issue, the worktree `.worktrees/issue-<N>`, its slot, and: "Review and fix now, in parallel with other merge-managers. Before the rebase, run `bash <lock script> take <PR>` in the foreground; right after the last merge succeeds (before cleanup), or when you give up, run `bash <lock script> release <PR>`. Squash-merge against an explicit SHA you record after taking the lock. Re-run a live suite only if the fixes or the rebase touched what it exercises." If a merge-manager died mid-turn, dispatch a fresh one; never message a dead agent.

After a reported merge: if the PR changed `app/package-lock.json`, fast-forward the root and run `npm ci` in the root `app/` before creating any worktree; then, for every live worktree whose `app/package-lock.json` now matches the root's but whose `app/node_modules/.package-lock.json` doesn't, and whose agent isn't gating, replace its `node_modules` with `cp -Rpc` from the root. Confirm the issue closed; if not, close it naming the PR. Remove the worktree. Stop each agent once its report is in.

## Step 3: milestone exits

An exit ticket is free when every other open issue in its milestone is closed (its blockers say so, and `gh issue list --milestone <m> --state open` holds only the exit). For a free exit, the steps are whatever its body lists. By default, in this order:

1. **Review.** Claim it (`in-progress`). Dispatch one **milestone reviewer** (background, Opus) that implemented none of the milestone's tickets. Check this against the agents you dispatched this session and the merged PRs' authors' sessions. Its prompt names the exit issue, the milestone, the parent spec, a slot, and: "You are an independent reviewer. Check the milestone against its exit ticket and its tickets' acceptance criteria on current `origin/main`. Take up every PR line reading 'Not witnessable by an agent, handed to the milestone review': witness it yourself where you can (screenshots of `just dev` against the local live products count), and otherwise list it for Björn. File each gap as a `ready-for-agent` issue in this milestone, add it as a blocker of the exit issue (native dependency), and never fix anything yourself. Post your verdict on the exit issue as a comment beginning `Milestone review:` with **Approved** or **Gaps: #a, #b** and the evidence. Your final message is that comment."
2. **Gaps.** If the verdict lists gaps, put the exit back to `ready-for-agent`. The gaps go through the normal loop, and the exit becomes free again when they close. The next review covers only what changed since the last verdict.
3. **Close and release** on **Approved**, as the exit body says. By default:
   - dispatch one implementer for the release PR (version bump and anything else the body lists), merged by a merge-manager as usual;
   - then, yourself: tag the merge commit (annotated, the name the body gives), push the tag, `gh workflow run release.yml --ref main -f tag=<tag> -f platforms=all`, mark the release a pre-release when the body says so, close the milestone, close the exit issue with a comment linking the release;
   - check the release workflow's result once it ends (`gh run watch` in the background, or a later iteration), and report a failure as a new `ready-for-agent` issue.

## Step 4: dispatch the frontier

Take free non-exit candidates in ascending issue number until the ceiling is full, with two exceptions, which go first in this order. A **loop ticket** is one whose title begins `Loop:`. It makes every later ticket cheaper (disk, gate time, conflicts). **Loop tickets have the utmost priority** (Björn, 2026-10-15: "all the performance hangs on them"). A free loop ticket takes the next free seat before anything else, a waiting PR's merge-manager included. While any loop ticket is free or in progress, no non-loop implementer starts. Merge-managers still fill seats loop tickets don't need. A loop ticket that measures time (a gate or probe comparison) **holds the machine**: dispatch it when no other implementer is running, and start none beside it until it reports its measurements. A **live-fixture fix** goes next. That is a ticket that repairs a failure every run of a live suite hits (a seed, a shared fixture, a product's state; titles often begin `Live fixture:`). Every implementer whose ticket runs that suite loses time to it until it lands. Name every open live-fixture issue, with its suite, in each dispatch.

**Bundle tickets that edit the same screen or the same files.** Don't run two of them side by side: each merge forces the other through a rebase. In the new shell, the same screen means the same prototype-6 route or the same view component. Give a bundle to one implementer, in one worktree named for its lowest ticket (`issue-<lowest>`), with one PR that closes each ticket (one `Closes #N` line per ticket). A ticket joins a bundle when it is free or blocked only by tickets in that bundle. Put at most three tickets in a bundle. Claim every ticket in it. A free ticket whose screen is already being edited by a running agent waits for that agent's merge rather than starting beside it.

**Measuring:** a ticket that touches search, queries, `filter_suggestions` or another read on the hot path gets this rule quoted in its dispatch: "Measure on 2–3k items while you work; run the full 22k responsiveness probe once, at the end."

**Self-merge** for the smallest tickets: Sonnet-sized, no frozen surface, no live suite, no visible behaviour change, an expected diff under about 150 lines. Its dispatch says *self-merge* and names both brief paths and the merge-tail lock script. It gets no merge-manager. A **docs-only** ticket (it changes only `docs/**`, `.claude/**` or Markdown outside `crates/` and `app/`) also self-merges, and needs no gate, no `node_modules` copy and no Rust build. Its worktree doesn't count against the disk guard.

**New-shell conventions:** every dispatch for a ticket in a ticket-first milestone (TF*) names `docs/specs/2026-10-15-ticket-first-shell-conventions.md` once it is on `main` (#907), and says: "These conventions are already ruled. Follow them, and don't re-record them as your rulings. A new convention you rule goes in that file in your PR."

**Split before dispatch** (Björn, 2026-10-15): a ticket whose body has more than 6 acceptance-criteria checkboxes, or names more than one route or screen to build, is not given to an implementer whole. Dispatch one **splitter** (background, Sonnet, no worktree, no slot beyond its seat) naming the ticket. It files one `ready-for-agent` ticket per route or screen, in the same milestone, each with `## Parent` naming the original's parent spec. It carries the original's criteria over word for word and splits them between the pieces, with every criterion in exactly one piece. It sets native blocking where one piece needs another, comments the mapping on the original, closes the original as split, and points the milestone exit's blockers at the pieces. The pieces enter the frontier at the next iteration.

For each ticket N:

1. Claim: `gh issue edit N --remove-label ready-for-agent --add-label in-progress`.
2. `git -C /Users/dev/Projects/knobas fetch`, then `SHA=$(git -C /Users/dev/Projects/knobas rev-parse origin/main)`. Branch from that SHA, never from a moving ref.
3. `git -C /Users/dev/Projects/knobas worktree add .worktrees/issue-N -b issue-N "$SHA"`, then `cmp app/package-lock.json .worktrees/issue-N/app/package-lock.json && cp -Rpc app/node_modules .worktrees/issue-N/app/node_modules`.
4. Dispatch one implementer (background, general-purpose, model as in *Ceiling*) naming: the brief path, the issue, its parent spec (and, when the milestone description has a `## Spec digest`, "read the milestone's digest first and the spec sections it cites; the rest of the spec is context only"), the worktree, the SHA, the scratchpad, its slot, whether it holds the desktop-witness slot, the known live-fixture issues, and *self-merge* when it applies.

**Warm target: suspended** (2026-10-16). #920 found that a `target` handed on from another worktree keeps build-script output that names the old worktree's paths, so 9 packages (tauri, aws-lc and others) had to be cleaned before it built. A merged worktree's `target` is deleted with the worktree; sccache still serves the dependencies. Don't run the `.warm-target` moves in Step 2's cleanup or Step 4's worktree creation.

**Pre-warm** (Björn, 2026-10-15): when a seat will free soon (an implementer's PR is open, or a merge-manager holds the merge lock), take the next ticket Step 4 would dispatch. If its work will touch Rust (its body names `crates/`, or its milestone is backend), create its worktree now, after the disk guard but without claiming it, and start `nice -n 19 env -u RUSTUP_TOOLCHAIN CARGO_BUILD_JOBS=2 cargo test --workspace --no-run` there in the background, logging to the scratchpad. Never pre-warm while a machine-holding loop ticket runs. At dispatch, the worktree is reused and the dispatch says the warm-up may still be running; cargo's build lock makes the first build wait for it.

Never commit or stash in the root checkout. If a ticket needs files that are uncommitted there, copy them into the worktree and say they are Björn's text, to be committed as is.

## Step 5: wake-up or stop

- Anything dispatched, merged, reviewed or released: `ScheduleWakeup` 1800 s, `noop: false`, `prompt: "/ticket-loop"`, reason naming what is out.
- Nothing changed, agents still running: 1800 s, `noop: true`.
- **Empty**: no free candidate, nothing in progress, no open PR, no agent running. Report it and `ScheduleWakeup` with `stop: true`. List by number any ticket that is blocked only by an issue that is not `ready-for-agent` (`needs-info`, `ready-for-human`). Don't wait on those.

## Report each iteration, in one short message

Merged (PR, issue, commit) · merges per hour and seat count (from the throughput log) · dispatched (issue, slot) · milestone reviews (exit, verdict) · releases (tag, run) · rulings recorded this iteration (issue, one line) · handed to Björn (by-eye items a reviewer could not witness, reserved steps) · still running · blocked and on what · next wake-up. Never paste agent output.
