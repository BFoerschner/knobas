---
description: One iteration of the v1.5 ticket loop — merge what is open, then dispatch the next free tickets. Run as `/loop /v15-next`.
---

You are the orchestrating session for milestone v1.5 of knobas. This file is one **iteration** of the ticket loop; `/loop /v15-next` re-runs it after every wake-up. Do exactly one iteration, schedule the next wake-up, and end the turn. Never implement or review a ticket yourself: you dispatch and you merge-sequence.

Read `CLAUDE.md`, `docs/agents/working-model.md` (Process, Concurrency), and `docs/agents/issue-tracker.md` before the first iteration of a session. The spec is issue #491; the tickets are #492–#510, all in the one milestone `v1.5`, each with a `## Blocked by` list in its body. There is no Agent Brief comment on these tickets: **the issue body is the brief.** There are no sub-milestone gates: #492 (the v1.0.0 release) blocks every other ticket, and the ordering of the rest is by issue number, which is the stream order of #491.

## The two brief files

Every implementer and merge-manager is handed a copy of a common rules file. The masters live in this machine's memory directory for the project (`knobas-implementer-brief.md` and `knobas-merge-manager-brief.md` under `~/.claude/projects/-Users-dev-Projects-knobas/memory/`). At the start of each session, once:

1. Copy each into the scratchpad with a unique name (`implementer-brief-v15.md`, `merge-manager-brief-v15.md`).
2. In each copy, replace the attribution block (the `Co-Authored-By:` and `Claude-Session:` trailers, and the PR-body footer) with **this session's** attribution from the system reminder. The masters carry a stale session URL.
3. Append to the implementer copy, verbatim:
   - "There is no Agent Brief comment on v1.5 tickets; the issue body is the contract."
   - "A live recipe needs `source testenv/hetzner/env` and `testenv/hetzner/tunnel up` first; the Hetzner fixture is shared, so leave it as you found it and never stop a shared container."
   - "A desktop witness (`just desktop-witness <driver>`) takes over this Mac's screen and its one signed bundle: run it once, in the foreground, and never while another agent's witness is running — the orchestrator dispatches at most one such ticket at a time."
   - "ADR-0015 and ADR-0016 are in force: an importer is not a source; a spawned command takes no argument from the mirror. A PR that widens the opener's capability scope or adds a `Source` implementation for an importer is wrong by construction."

Pass the copy's path in every dispatch. Never pass the memory path.

## Ceiling (the machine's, not the plan's)

**Every subagent runs on Opus** (Björn, 2026-09-06): pass `model: "opus"` on every Agent call, implementer and merge-manager alike. The session model is Fable; an unspecified model inherits it and burns Fable credit.

Three Rust agents at once, and a merge-manager counts as one. At most two gates run at once; the recipe enforces it. Keep the mix at **two implementers plus one merge-manager** whenever a PR is open, so tickets close and the frontier keeps moving; three implementers only when no PR is open.

Two further caps, because the witnesses are shared machines:

- **At most one tunnel ticket running at a time.** The tunnel tickets are #495 (TeamCity live), #498 (Atlassian live), #509 and #510 (estate live). Their live suites spend the same Hetzner fixture.
- **At most one desktop-witness ticket running at a time.** These are #500, #501 and #503. Each drives the one signed bundle on this Mac's screen; two at once fight over the frontmost app.

## Step 1 — read the state (jq only; never print issue bodies into context)

```sh
# open PRs from the loop
gh pr list --state open --json number,headRefName,closingIssuesReferences,isDraft \
  --jq '.[] | "\(.number)\t\(.headRefName)\t\([.closingIssuesReferences[].number]|join(","))"'
# claimed tickets
gh issue list --state open --label in-progress --limit 50 --json number,title --jq '.[] | "\(.number)\t\(.title)"'
# candidates
gh issue list --state open --label ready-for-agent --limit 50 --json number,milestone,body \
  --jq '.[] | select(.milestone.title == "v1.5") | "\(.number)\t\((.body|capture("## Blocked by\\n(?<b>[\\s\\S]*)")|.b|[scan("#(\\d+)")[]]|join(",")))"'
```

A candidate is **free** when every number in its blocked-by list is a closed issue (`gh issue view N --json state -q .state` is `CLOSED`); "None" is free. Also count agents still running from a previous iteration (your own background tasks).

## Step 2 — merge before dispatching

For every open PR that has no merge-manager running, in ascending PR number and **one at a time**: dispatch one merge-manager (background, `model: "opus"`) with the merge-manager brief path, the PR number, the originating issue number, the worktree path `.worktrees/issue-<N>`, and the instruction to squash-merge against an explicit SHA it records first. If a previous merge-manager died mid-turn, dispatch a fresh one; never message the dead one.

After a merge-manager reports a merge: if the PR changed `app/package-lock.json`, run `npm ci` in the root `app/` before creating any further worktree (stale root `node_modules` propagates into every clone). Confirm the issue closed; if not, close it with a comment naming the PR.

**The one merge you finish yourself: #492.** Its acceptance criteria name a tag on the merge commit and a dispatched release, which no implementer can make before the merge. When #492's merge-manager reports the merge commit `M`:

```sh
git -C /Users/dev/Projects/knobas fetch origin
git -C /Users/dev/Projects/knobas tag -a v1.0.0 -m "knobas v1.0.0: M4's exit, the v1 tier of spec §16" "$M"
git -C /Users/dev/Projects/knobas push origin v1.0.0
gh workflow run release.yml --ref main -f tag=v1.0.0 -f platforms=all
```

Then `gh run list --workflow=release.yml --limit 1` for the run id, and comment it on #492. The release run is the only CI signal there is; on a later iteration check its conclusion, and if it failed, reopen #492 with the run URL rather than dispatching into the frontier on a red release. Until #492 is closed **and** the release run is green, nothing else is free.

## Step 3 — dispatch the frontier

Take free candidates in ascending issue number until the ceiling and the two caps are reached; a candidate a cap refuses is skipped this iteration and stays `ready-for-agent`. For each ticket N:

1. Claim: `gh issue edit N --remove-label ready-for-agent --add-label in-progress`.
2. Record the branch point: `SHA=$(git -C /Users/dev/Projects/knobas rev-parse origin/main)` after a `git fetch`.
3. Worktree: `git -C /Users/dev/Projects/knobas worktree add .worktrees/issue-N -b issue-N "$SHA"`, then `cmp app/package-lock.json .worktrees/issue-N/app/package-lock.json && cp -Rpc app/node_modules .worktrees/issue-N/app/node_modules`.
4. Dispatch one implementer (Agent tool, background, general-purpose, `model: "opus"`) whose prompt names: the brief path, the issue number, the worktree path, the branch-point SHA, the scratchpad directory, and these three sentences verbatim: "Run the full `just check` once at the end, in the foreground, with the Bash timeout at its maximum; never background it. The PR body's `Closes #N` line goes outside backticks; verify with `gh pr view --json closingIssuesReferences`. Stop when the PR is open; you never merge."

For #494, add: "Branch `v1-5-glossary-and-adrs` (two commits: the six glossary entries and ADR-0015/0016, plus `.claude/commands/v15-next.md`) is the start of your work — cherry-pick or rebase it onto your branch point before anything else, and do not rewrite those files except where the ticket says so."

## Step 4 — wake-up and stop

- If you dispatched or merged anything: `ScheduleWakeup` with `delaySeconds: 1800`, `noop: false`, `prompt: "/v15-next"`, reason naming the tickets out. Agent completions wake you sooner; the wake-up is the fallback.
- If nothing changed and agents are still running: `ScheduleWakeup` 1800 s, `noop: true`.
- If the release run from #492 is still in progress and nothing else can move: `ScheduleWakeup` 600 s, `noop: true`, reason "watching the v1.0.0 release run".
- If no candidate is free, no ticket is in-progress and no PR is open — the milestone is done, or everything left is blocked on a gate that is Björn's — `ScheduleWakeup` with `stop: true` and say which. The v1.5 exit (every ticket closed, every live recipe green, the release run green) is Björn's gate.

## Report each iteration, in one short message

Merged (PR, issue, merge commit) · dispatched (issue, worktree) · still running · blocked and on what (a cap, a tunnel, the release run) · the next wake-up. Never paste agent output.
