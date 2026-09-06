---
description: One iteration of the M4 ticket loop — merge what is open, then dispatch the next free tickets. Run as `/loop /m4-next`.
---

You are the orchestrating session for milestone M4 of knobas. This file is one **iteration** of the ticket loop; `/loop /m4-next` re-runs it after every wake-up. Do exactly one iteration, schedule the next wake-up, and end the turn. Never implement or review a ticket yourself: you dispatch and you merge-sequence.

Read `CLAUDE.md`, `docs/agents/working-model.md` (Process, Concurrency), and `docs/agents/issue-tracker.md` before the first iteration of a session. The spec is issue #427; the tickets are #428–#455 across milestones M4.0, M4.1 and M4.2, each with a `## Blocked by` list in its body. There is no Agent Brief comment on these tickets: **the issue body is the brief.**

## The two brief files

Every implementer and merge-manager is handed a copy of a common rules file. The masters live in this machine's memory directory for the project (`knobas-implementer-brief.md` and `knobas-merge-manager-brief.md` under `~/.claude/projects/-Users-dev-Projects-knobas/memory/`). At the start of each session, once:

1. Copy each into the scratchpad with a unique name (`implementer-brief-m4.md`, `merge-manager-brief-m4.md`).
2. In each copy, replace the attribution block (the `Co-Authored-By:` and `Claude-Session:` trailers, and the PR-body footer) with **this session's** attribution from the system reminder. The masters carry a stale session URL.
3. Append one line to the implementer copy: "There is no Agent Brief comment on M4 tickets; the issue body is the contract."

Pass the copy's path in every dispatch. Never pass the memory path.

## Ceiling (the machine's, not the plan's)

**Every subagent runs on Opus** (Björn, 2026-09-06): pass `model: "opus"` on every Agent call, implementer and merge-manager alike. The session model is Fable; an unspecified model inherits it and burns Fable credit.

Three Rust agents at once, and a merge-manager counts as one. At most two gates run at once; the recipe enforces it. Keep the mix at **two implementers plus one merge-manager** whenever a PR is open, so tickets close and the frontier keeps moving; three implementers only when no PR is open.

## Step 1 — read the state (jq only; never print issue bodies into context)

```sh
# open PRs from the loop
gh pr list --state open --json number,headRefName,closingIssuesReferences,isDraft \
  --jq '.[] | "\(.number)\t\(.headRefName)\t\([.closingIssuesReferences[].number]|join(","))"'
# claimed tickets
gh issue list --state open --label in-progress --limit 50 --json number,title --jq '.[] | "\(.number)\t\(.title)"'
# candidates
gh issue list --state open --label ready-for-agent --limit 50 --json number,milestone,body \
  --jq '.[] | select(.milestone.title|startswith("M4.")) | "\(.number)\t\(.milestone.title)\t\((.body|capture("## Blocked by\\n(?<b>[\\s\\S]*)")|.b|[scan("#(\\d+)")[]]|join(",")))"'
```

A candidate is **free** when every number in its blocked-by list is a closed issue (`gh issue view N --json state -q .state` is `CLOSED`); "None" is free. Also count agents still running from a previous iteration (your own background tasks).

## Step 2 — merge before dispatching

For every open PR that has no merge-manager running, in ascending PR number and **one at a time**: dispatch one merge-manager (background, `model: "opus"`) with the merge-manager brief path, the PR number, the originating issue number, the worktree path `.worktrees/issue-<N>`, and the instruction to squash-merge against an explicit SHA it records first. If a previous merge-manager died mid-turn, dispatch a fresh one; never message the dead one.

After a merge-manager reports a merge: if the PR changed `app/package-lock.json`, run `npm ci` in the root `app/` before creating any further worktree (stale root `node_modules` propagates into every clone). Confirm the issue closed; if not, close it with a comment naming the PR.

## Step 3 — dispatch the frontier

Take free candidates in ascending issue number until the ceiling is reached. For each ticket N:

1. Claim: `gh issue edit N --remove-label ready-for-agent --add-label in-progress`.
2. Record the branch point: `SHA=$(git -C /Users/dev/Projects/knobas rev-parse origin/main)` after a `git fetch`.
3. Worktree: `git -C /Users/dev/Projects/knobas worktree add .worktrees/issue-N -b issue-N "$SHA"`, then `cmp app/package-lock.json .worktrees/issue-N/app/package-lock.json && cp -Rpc app/node_modules .worktrees/issue-N/app/node_modules`.
4. Dispatch one implementer (Agent tool, background, general-purpose, `model: "opus"`) whose prompt names: the brief path, the issue number, the worktree path, the branch-point SHA, the scratchpad directory, and these three sentences verbatim: "Run the full `just check` once at the end, in the foreground, with the Bash timeout at its maximum; never background it. The PR body's `Closes #N` line goes outside backticks; verify with `gh pr view --json closingIssuesReferences`. Stop when the PR is open; you never merge."

A ticket whose title says **droppable** is dispatched only when nothing else is free in its milestone.

## Step 4 — wake-up and stop

- If you dispatched or merged anything: `ScheduleWakeup` with `delaySeconds: 1800`, `noop: false`, `prompt: "/m4-next"`, reason naming the tickets out. Agent completions wake you sooner; the wake-up is the fallback.
- If nothing changed and agents are still running: `ScheduleWakeup` 1800 s, `noop: true`.
- If no candidate is free, no ticket is in-progress and no PR is open — either the milestone is done or everything left is blocked on a gate that is Björn's — `ScheduleWakeup` with `stop: true` and say which.

## Sub-milestone gates

M4.1 and M4.2 tickets are blocked by #440 (the M4.0 exit ticket), and Björn gates at that merge. When #440's merge-manager reports, **stop the loop** and hand the gate to Björn instead of dispatching into M4.1; he restarts `/loop /m4-next` after the gate. The same holds at #450 (M4.1's exit) and #455 (M4.2's exit).

## Report each iteration, in one short message

Merged (PR, issue, merge commit) · dispatched (issue, worktree) · still running · blocked and on what · the next wake-up. Never paste agent output.
