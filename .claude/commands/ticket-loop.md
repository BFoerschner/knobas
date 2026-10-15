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
   - "Run the full `just check` once at the end, in the foreground, with the Bash timeout at its maximum. Never background it. Run `just pre-gate` (it runs `cargo fmt --all`) first. Record the head SHA the gate passed on in the PR body. The PR body's `Closes #N` goes outside backticks; verify with `gh pr view --json closingIssuesReferences`. Stop when the PR is open; you never merge, unless your dispatch says *self-merge*."
   - "Review split: run `mattpocock-skills:code-review` on the **Standards axis only**. The merge-manager owns the Spec axis and the deep pass. This replaces 'Standards + Spec axes' above."
   - "Mutation checks: 3 to 6 mutants on the load-bearing logic, not more. Screenshots only when the ticket changes what the app draws."
   - "Live suites: run only the live suite your change exercises, with the narrowest test filter that covers your code. A failure your dispatch names as a known shared-fixture break (with its issue number) is not yours: name the issue in the PR and move on. A new failure that is not yours: file it as a `ready-for-agent` issue titled `Live fixture: …` and name it in your report."
   - "*Self-merge*: when your dispatch says so, once the PR is open and your gate is green, merge it yourself by the *Merge and cleanup* section of the merge-manager brief named in your dispatch, and report the merged commit."
5. Append to the merge-manager copy, verbatim:
   - "Review split: the implementer ran the Standards axis. You run the **Spec axis**, plus the deep pass where the diff touches a frozen surface (§10.8, migrations, the write queue, source adapters, concurrency). Don't redo the implementer's mutants; add one only for a claim you find untested."
   - "Gate once: run `just check` on the final merge head only, never on the head as received. If you change nothing, need no rebase, and the PR body records a green `just check` on that exact head SHA over the same `origin/main`, merge without re-running it."

Pass the copy's path in every dispatch, never the memory path.

## Ceiling

Defaults, unless `docs/agents/working-model.md` or Björn's rulings in memory say otherwise:

- **Model per ticket, never Fable.** Opus (`model: "opus"`) for tracers, UI, anything that changes a frozen surface's behaviour, merge-managers of those, and milestone reviewers. **Sonnet** (`model: "sonnet"`) for small mechanical tickets and their merge-managers: docs or ADR wording that records code already merged, a seed or fixture fix, one more test case, a narrow query or sync fix. The dispatch says which model and why.
- **Five agents at once** (the working model's Opus cap). Implementers, merge-managers and milestone reviewers all count. **At most two gates at once**, which the recipe's slot guard enforces: a third gate waits for a slot. A gate is minutes of an hour-long job, so that wait is expected and is no reason to hold back a dispatch.
- **One merge-manager per open PR, at most two at once**; implementers fill the other seats. A PR waiting for review takes the next free seat before any new implementer.
- **Disk:** a Rust worktree grows to 8–26 GB. Before creating one, `df -g /Users/dev` must show at least 20 GB free plus 12 GB for every worktree whose agent has not yet run its first gate. Below that, create none and report the free space to Björn.
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

For every open PR without a running merge-manager, in ascending PR number, dispatch one merge-manager (background, model as in *Ceiling*). **Reviews run in parallel; only the merge tail runs one at a time.** The merge tail is the rebase onto `origin/main`, the gate, the squash-merge and the cleanup, and it runs under a machine-wide lock. Once per session, write the lock script into the scratchpad as `merge-tail-lock-<session>.sh`: `take <PR>` loops on `mkdir "$TMPDIR/knobas-merge-tail.lock"` every 30 s, takes over a lock whose holder PR is merged or closed, and writes the PR into `holder`; `release <PR>` removes the lock if that PR holds it. Its prompt names the merge-manager brief path, the lock script path, the PR, the issue, the worktree `.worktrees/issue-<N>`, its slot, and: "Review and fix now, in parallel with other merge-managers. Before the rebase, run `bash <lock script> take <PR>` in the foreground; after cleanup, or when you give up, run `bash <lock script> release <PR>`. Squash-merge against an explicit SHA you record after taking the lock. Re-run a live suite only if the fixes or the rebase touched what it exercises." If a merge-manager died mid-turn, dispatch a fresh one; never message a dead agent.

After a reported merge: if the PR changed `app/package-lock.json`, run `npm ci` in the root `app/` before creating any worktree. Confirm the issue closed; if not, close it naming the PR. Remove the worktree. Stop each agent once its report is in.

## Step 3: milestone exits

An exit ticket is free when every other open issue in its milestone is closed (its blockers say so, and `gh issue list --milestone <m> --state open` holds only the exit). For a free exit, the steps are whatever its body lists. By default, in this order:

1. **Review.** Claim it (`in-progress`). Dispatch one **milestone reviewer** (background, Opus) that implemented none of the milestone's tickets. Check this against the agents you dispatched this session and the merged PRs' authors' sessions. Its prompt names the exit issue, the milestone, the parent spec, a slot, and: "You are an independent reviewer. Check the milestone against its exit ticket and its tickets' acceptance criteria on current `origin/main`. Take up every PR line reading 'Not witnessable by an agent, handed to the milestone review': witness it yourself where you can (screenshots of `just dev` against the local live products count), and otherwise list it for Björn. File each gap as a `ready-for-agent` issue in this milestone, add it as a blocker of the exit issue (native dependency), and never fix anything yourself. Post your verdict on the exit issue as a comment beginning `Milestone review:` with **Approved** or **Gaps: #a, #b** and the evidence. Your final message is that comment."
2. **Gaps.** If the verdict lists gaps, put the exit back to `ready-for-agent`. The gaps go through the normal loop, and the exit becomes free again when they close. The next review covers only what changed since the last verdict.
3. **Close and release** on **Approved**, as the exit body says. By default:
   - dispatch one implementer for the release PR (version bump and anything else the body lists), merged by a merge-manager as usual;
   - then, yourself: tag the merge commit (annotated, the name the body gives), push the tag, `gh workflow run release.yml --ref main -f tag=<tag> -f platforms=all`, mark the release a pre-release when the body says so, close the milestone, close the exit issue with a comment linking the release;
   - check the release workflow's result once it ends (`gh run watch` in the background, or a later iteration), and report a failure as a new `ready-for-agent` issue.

## Step 4: dispatch the frontier

Take free non-exit candidates in ascending issue number until the ceiling is full, with one exception: a **live-fixture fix** goes first. That is a ticket that repairs a failure every run of a live suite hits (a seed, a shared fixture, a product's state; titles often begin `Live fixture:`). Every implementer whose ticket runs that suite loses time to it until it lands. Name every open live-fixture issue, with its suite, in each dispatch.

**Bundle tickets that edit the same screen or the same files.** Don't run two of them side by side: each merge forces the other through a rebase. In the new shell, the same screen means the same prototype-6 route or the same view component. Give a bundle to one implementer, in one worktree named for its lowest ticket (`issue-<lowest>`), with one PR that closes each ticket (one `Closes #N` line per ticket). A ticket joins a bundle when it is free or blocked only by tickets in that bundle. Put at most three tickets in a bundle. Claim every ticket in it. A free ticket whose screen is already being edited by a running agent waits for that agent's merge rather than starting beside it.

**Measuring:** a ticket that touches search, queries, `filter_suggestions` or another read on the hot path gets this rule quoted in its dispatch: "Measure on 2–3k items while you work; run the full 22k responsiveness probe once, at the end."

**Self-merge** for the smallest tickets: Sonnet-sized, no frozen surface, no live suite, no visible behaviour change, an expected diff under about 150 lines. Its dispatch says *self-merge* and names both brief paths. It gets no merge-manager.

For each ticket N:

1. Claim: `gh issue edit N --remove-label ready-for-agent --add-label in-progress`.
2. `git -C /Users/dev/Projects/knobas fetch`, then `SHA=$(git -C /Users/dev/Projects/knobas rev-parse origin/main)`. Branch from that SHA, never from a moving ref.
3. `git -C /Users/dev/Projects/knobas worktree add .worktrees/issue-N -b issue-N "$SHA"`, then `cmp app/package-lock.json .worktrees/issue-N/app/package-lock.json && cp -Rpc app/node_modules .worktrees/issue-N/app/node_modules`.
4. Dispatch one implementer (background, general-purpose, model as in *Ceiling*) naming: the brief path, the issue, its parent spec, the worktree, the SHA, the scratchpad, its slot, whether it holds the desktop-witness slot, the known live-fixture issues, and *self-merge* when it applies.

Never commit or stash in the root checkout. If a ticket needs files that are uncommitted there, copy them into the worktree and say they are Björn's text, to be committed as is.

## Step 5: wake-up or stop

- Anything dispatched, merged, reviewed or released: `ScheduleWakeup` 1800 s, `noop: false`, `prompt: "/ticket-loop"`, reason naming what is out.
- Nothing changed, agents still running: 1800 s, `noop: true`.
- **Empty**: no free candidate, nothing in progress, no open PR, no agent running. Report it and `ScheduleWakeup` with `stop: true`. List by number any ticket that is blocked only by an issue that is not `ready-for-agent` (`needs-info`, `ready-for-human`). Don't wait on those.

## Report each iteration, in one short message

Merged (PR, issue, commit) · dispatched (issue, slot) · milestone reviews (exit, verdict) · releases (tag, run) · rulings recorded this iteration (issue, one line) · handed to Björn (by-eye items a reviewer could not witness, reserved steps) · still running · blocked and on what · next wake-up. Never paste agent output.
