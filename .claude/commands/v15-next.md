---
description: One iteration of the v1.5 ticket loop — merge what is open, then dispatch the next free tickets. Run as `/loop /v15-next`.
---

You are the orchestrating session for milestone v1.5 of knobas. This file is one **iteration** of the ticket loop; `/loop /v15-next` re-runs it after every wake-up. Do exactly one iteration, schedule the next wake-up, and end the turn. Never implement or review a ticket yourself: you dispatch and you merge-sequence. **Björn is not here.** Every decision the loop would otherwise hand to him is ruled by a **deputy** — a Fable subagent — as the section *Decisions in Björn's absence* says; you never rule yourself and you never wait for Björn.

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
   - "Björn is not here. When the ticket, spec #491, `CONTEXT.md`, the ADRs and `docs/decisions/` leave a choice open that changes what you build, do not guess and do not pick: stop at the fork, end your turn with a final message whose last section is `**Fork:**` — the question, the options, which one you lean to and why — and you will be resumed with a ruling. A ruling posted on your issue as a comment beginning `Ruling (deputy for Björn` is final for this milestone: act on it, and append it to `docs/decisions/2026-09-v1-5-unattended-rulings.md` in your PR in the same fork/ruling/reasoning/reversal-cost shape the comment has."
4. Append to the merge-manager copy, verbatim: "Björn is not here. A review finding that needs a ruling — a frozen-surface touch the ticket did not name, a criterion that cannot pass on a correct implementation, a spec sentence the code contradicts — is not yours to settle and not a reason to merge or to refuse: end your turn with a `**Fork:**` section as the implementer brief describes, and you will be resumed with the ruling."

Pass the copy's path in every dispatch. Never pass the memory path.

## Decisions in Björn's absence

Björn (2026-09-07): *"let a fable subagent decide whenever there's a decision to be made since I won't be there."* The precedent is `docs/decisions/2026-08-29-unattended-batch-rulings.md`, where Fable ruled fifteen forks under the same delegation; its shape is the deputy's.

**What is a decision.** Any of these, and nothing the documents already answer:

1. A `**Fork:**` section in an implementer's or merge-manager's final message.
2. A frozen-surface touch (§10.8) that the ticket did not name — the contract says those need an orchestrator decision, and here that is the deputy.
3. A witness gap: a criterion that cannot pass on a correct implementation, a live recipe red for a reason in the fixture rather than the code, a witness that can only see one direction.
4. The v1.0.0 release run red, or a live recipe red twice in a row.
5. A contradiction between spec #491, a ticket, `CONTEXT.md` or an ADR found mid-ticket.
6. Reordering or dropping a ticket, or a cap that blocks the frontier for more than two iterations.
7. The milestone exit.

**The deputy.** One subagent per decision (Agent tool, background, general-purpose, **`model: "fable"`** — this is the one role that does not run on Opus, because the point of it is Fable's judgement). Its prompt names: the issue number and the fork text verbatim; the paths of `docs/specs/2026-08-23-knobas-design.md`, `CONTEXT.md`, `docs/adr/`, `docs/decisions/`, `docs/contract.md`, and the memory file `~/.claude/projects/-Users-dev-Projects-knobas/memory/knobas-v1-5-grilling-outcome.md` (read-only: the grilling's rulings, which bind); the tracker rule that an issue is read before it is written to; and these instructions verbatim:

> "You are the deputy for Björn, who is absent, on knobas milestone v1.5. First look for an answer that already exists — in the ticket, in spec #491, in the glossary, in an ADR, in a decision record, in the grilling rulings — and if one exists, quote it and rule that; a ruling that re-decides a settled question is wrong. Otherwise rule the fork the way the grilling's rulings would, preferring the reading that keeps the frozen surface smallest, the witness real (ADR-0013), and the ticket's scope unchanged. Post the ruling as a comment on the issue, beginning `Ruling (deputy for Björn, <date>):` and carrying four parts — **The fork**, **Ruling**, **Reasoning**, **If you disagree, the cost of reversing this is** — in the shape of `docs/decisions/2026-08-29-unattended-batch-rulings.md`. Where the ruling changes the glossary or needs an ADR, say so in the ruling and name the entry; the implementer writes it. Never implement, never review code, never merge. Your final message is the comment's text verbatim and nothing else."

**After the deputy reports:** resume the waiting agent with `SendMessage` carrying the ruling verbatim and the sentence "Act on this ruling; it is final for v1.5." If the waiting agent is dead (a 429 mid-turn, a killed task), dispatch a fresh one with the ruling in its prompt. A ruling that reorders or drops a ticket is applied by you in the next Step 3. Never message a dead agent.

**Fable credit.** If the deputy dies on a 429 before ruling, re-dispatch it at the next wake-up; **never substitute Opus for the deputy** — the 2026-09-01 Opus-substitute ruling is for merge-managers, and a decision waiting an hour costs less than a decision made by the wrong model. The waiting implementer keeps waiting.

**Björn's reversal.** Every ruling is a comment on its issue and a section in `docs/decisions/2026-09-v1-5-unattended-rulings.md` (appended by the PR that acted on it), so he can read them in order and reverse any one with a follow-up ticket. Nothing the deputy rules is an ADR until the implementer writes one under the ruling's instruction.

## Ceiling (the machine's, not the plan's)

**Every implementer and merge-manager runs on Opus** (Björn, 2026-09-06): pass `model: "opus"` on those Agent calls. The session model is Fable; an unspecified model inherits it and burns Fable credit. **The deputy runs on Fable** (Björn, 2026-09-07) and is the only exception; a deputy counts against the Rust-agent ceiling for as long as it runs, which is minutes.

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

Then `gh run list --workflow=release.yml --limit 1` for the run id, and comment it on #492. The release run is the only CI signal there is; on a later iteration check its conclusion, and if it failed, reopen #492 with the run URL, dispatch the deputy on the fork "release run red: retry, fix forward under #492, or release without the failed platform" and act on its ruling. Until #492 is closed **and** the release run is green, or the deputy has ruled otherwise, nothing else is free.

## Step 2b — rulings before dispatching

For every agent report since the last iteration whose final message ends in a `**Fork:**` section, and for every decision the list in *Decisions in Björn's absence* names that this iteration's state shows, dispatch one deputy per fork (background, `model: "fable"`) before dispatching any implementer. A fork that is still open at the end of an iteration is reported as such, with the deputy's task named as running.

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
- If a fork is open and nothing else can move: `ScheduleWakeup` 1200 s, `noop: true`, reason naming the fork and its deputy.
- If no candidate is free, no ticket is in-progress and no PR is open, the milestone is done: dispatch the deputy once more on the fork "v1.5 exit: every ticket closed, every live recipe green on its last run, the release run green — exited, or what is missing" and, on a ruling of *exited*, close the milestone (`gh api -X PATCH repos/BFoerschner/knobas/milestones/13 -f state=closed`), comment the ruling on #491, and `ScheduleWakeup` with `stop: true`. On a ruling naming what is missing, file it as a ticket in the milestone with `ready-for-agent` and keep looping. Björn reads the rulings file when he is back.

## Report each iteration, in one short message

Merged (PR, issue, merge commit) · dispatched (issue, worktree) · **ruled** (issue, the ruling in one line, the comment's URL) · forks open and their deputies · still running · blocked and on what (a cap, a tunnel, the release run, a fork) · the next wake-up. Never paste agent output.
