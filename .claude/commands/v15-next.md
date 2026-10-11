---
description: One iteration of the ticket loop — merge what is open, rule open forks, then dispatch the next free ready-for-agent tickets. Run as `/loop /v15-next`.
---

You are the orchestrating session for the knobas ticket loop. This file is one **iteration**; `/loop /v15-next` re-runs it after every wake-up. Do exactly one iteration, schedule the next wake-up, and end the turn. Never implement or review a ticket yourself: you dispatch and you merge-sequence. Every decision the loop would otherwise hand to Björn is ruled by a **deputy** subagent on Opus, as *Decisions* says; you never rule yourself and you never wait for Björn.

Read `CLAUDE.md`, `docs/agents/working-model.md` (Process, Concurrency), and `docs/agents/issue-tracker.md` before the first iteration of a session.

**The tickets are every open issue labelled `ready-for-agent`**, in any milestone or none, except a spec: an issue whose title begins `Spec:` or that has sub-issues is the parent of tickets, never a ticket itself, and is never claimed. Each ticket carries a `## Blocked by` list in its body and, usually, a `## Parent` section naming its spec issue. There is no Agent Brief comment: **the issue body is the brief**, and the parent spec is its context. Order is by issue number.

## The two brief files

Every implementer and merge-manager is handed a copy of a common rules file. The masters live in this machine's memory directory for the project (`knobas-implementer-brief.md` and `knobas-merge-manager-brief.md` under `~/.claude/projects/-Users-dev-Projects-knobas/memory/`). At the start of each session, once:

1. Copy each into the scratchpad with a unique name (`implementer-brief-loop.md`, `merge-manager-brief-loop.md`).
2. In each copy, replace the attribution block (the `Co-Authored-By:` and `Claude-Session:` trailers, and the PR-body footer) with **this session's** attribution from the system reminder. The masters carry a stale session URL.
3. Append to the implementer copy, verbatim:
   - "There is no Agent Brief comment on these tickets; the issue body is the contract, and the spec named in its `## Parent` section is its context."
   - "A live recipe against TeamCity, Jira or Confluence needs `source testenv/hetzner/env` and `testenv/hetzner/tunnel up` first; the Hetzner fixture is shared, so leave it as you found it and never stop a shared container."
   - "A desktop witness (`just desktop-witness <driver>`) takes over this Mac's screen and its one signed bundle: run it once, in the foreground, and never while another agent's witness is running — the orchestrator dispatches at most one such ticket at a time."
   - "Every ADR in `docs/adr/` is in force. A PR that contradicts one is wrong by construction."
   - "When the ticket, its parent spec, `CONTEXT.md`, the ADRs and `docs/decisions/` leave a choice open that changes what you build, do not guess and do not pick: stop at the fork, end your turn with a final message whose last section is `**Fork:**` — the question, the options, which one you lean to and why — and you will be resumed with a ruling. A ruling posted on your issue as a comment beginning `Ruling (deputy for Björn` is final: act on it, and append it to `docs/decisions/2026-10-unattended-rulings.md` (create it on first use, in the shape of `docs/decisions/2026-08-29-unattended-batch-rulings.md`) in your PR in the same fork/ruling/reasoning/reversal-cost shape the comment has."
4. Append to the merge-manager copy, verbatim: "A review finding that needs a ruling — a frozen-surface touch the ticket did not name, a criterion that cannot pass on a correct implementation, a spec sentence the code contradicts — is not yours to settle and not a reason to merge or to refuse: end your turn with a `**Fork:**` section as the implementer brief describes, and you will be resumed with the ruling. A §10.8 change the ticket names is already signed off by Björn; merge it on review like any other."

Pass the copy's path in every dispatch. Never pass the memory path.

## Decisions

Björn (2026-10-11): the deputy no longer runs on Fable — *"it's too expensive"*. **The deputy runs on Opus.** Björn also signs off any frozen-contract change a ticket names by approving the ticket; such a merge waits for nobody. What stays Björn's and is never done by the loop: closing a spec or a milestone, tagging, and dispatching a release.

**What is a decision.** Any of these, and nothing the documents already answer:

1. A `**Fork:**` section in an implementer's or merge-manager's final message.
2. A frozen-surface touch (§10.8) that the ticket did not name.
3. A witness gap: a criterion that cannot pass on a correct implementation, a live recipe red for a reason in the fixture rather than the code, a witness that can only see one direction.
4. A live recipe red twice in a row.
5. A contradiction between a spec, a ticket, `CONTEXT.md` or an ADR found mid-ticket.
6. Reordering or dropping a ticket, or a cap that blocks the frontier for more than two iterations.

**The deputy.** One subagent per decision (Agent tool, background, general-purpose, `model: "opus"`). Its prompt names: the issue number and the fork text verbatim; the ticket's parent spec issue; the paths of `docs/specs/2026-08-23-knobas-design.md`, `CONTEXT.md`, `docs/adr/`, `docs/decisions/`, `docs/contract.md`; the tracker rule that an issue is read before it is written to; and these instructions verbatim:

> "You are the deputy for Björn on the knobas ticket loop. First look for an answer that already exists — in the ticket, in its parent spec, in the glossary, in an ADR, in a decision record — and if one exists, quote it and rule that; a ruling that re-decides a settled question is wrong. Otherwise rule the fork the way the spec and ADRs would, preferring the reading that keeps the frozen surface smallest, the witness real (ADR-0013), and the ticket's scope unchanged. Post the ruling as a comment on the issue, beginning `Ruling (deputy for Björn, <date>):` and carrying four parts — **The fork**, **Ruling**, **Reasoning**, **If you disagree, the cost of reversing this is** — in the shape of `docs/decisions/2026-08-29-unattended-batch-rulings.md`. Where the ruling changes the glossary or needs an ADR, say so in the ruling and name the entry; the implementer writes it. Never implement, never review code, never merge. Your final message is the comment's text verbatim and nothing else."

**After the deputy reports:** resume the waiting agent with `SendMessage` carrying the ruling verbatim and the sentence "Act on this ruling; it is final." If the waiting agent is dead (a 429 mid-turn, a killed task), dispatch a fresh one with the ruling in its prompt. A ruling that reorders or drops a ticket is applied by you in the next Step 3. Never message a dead agent.

**Björn's reversal.** Every ruling is a comment on its issue and a section in `docs/decisions/2026-10-unattended-rulings.md` (appended by the PR that acted on it), so he can read them in order and reverse any one with a follow-up ticket.

## Ceiling (the machine's, not the plan's)

**Every subagent runs on Opus** — implementers, merge-managers and deputies: pass `model: "opus"` on every Agent call. Never pass `model: "fable"`.

Three Rust agents at once; a merge-manager counts as one, and a deputy counts for as long as it runs, which is minutes. At most two gates run at once; the recipe enforces it. Keep the mix at **two implementers plus one merge-manager** whenever a PR is open, so tickets close and the frontier keeps moving; three implementers only when no PR is open.

Two further caps, because the witnesses are shared machines:

- **At most one tunnel ticket running at a time.** A tunnel ticket is one whose acceptance criteria name a live suite against TeamCity, Jira or Confluence (the Hetzner fixture). Their live suites spend the same fixture.
- **At most one desktop-witness ticket running at a time.** A ticket whose criteria name `just desktop-witness`. Each drives the one signed bundle on this Mac's screen.

## Step 1 — read the state (jq only; never print issue bodies into context)

```sh
# open PRs from the loop
gh pr list --state open --json number,headRefName,closingIssuesReferences,isDraft \
  --jq '.[] | "\(.number)\t\(.headRefName)\t\([.closingIssuesReferences[].number]|join(","))"'
# claimed tickets
gh issue list --state open --label in-progress --limit 50 --json number,title --jq '.[] | "\(.number)\t\(.title)"'
# candidates (specs excluded by title; also skip any with sub-issues)
gh issue list --state open --label ready-for-agent --limit 100 --json number,title,body \
  --jq '.[] | select(.title | startswith("Spec:") | not) | "\(.number)\t\((.body|capture("## Blocked by\\n(?<b>[\\s\\S]*)")|.b|[scan("#(\\d+)")[]]|join(",")))\t\(.body|test("live (\\w+ )?(suite|recipe)|_live|-live"; "i"))\t\(.body|test("desktop-witness"))"'
```

The last two columns flag tunnel and desktop-witness candidates; confirm a flagged one by its criteria. A candidate is **free** when every number in its blocked-by list is a closed issue (`gh issue view N --json state -q .state` is `CLOSED`); "None" is free. Also count agents still running from a previous iteration (your own background tasks).

## Step 2 — merge before dispatching

For every open PR that has no merge-manager running, in ascending PR number and **one at a time**: dispatch one merge-manager (background, `model: "opus"`) with the merge-manager brief path, the PR number, the originating issue number, the worktree path `.worktrees/issue-<N>`, and the instruction to squash-merge against an explicit SHA it records first. If a previous merge-manager died mid-turn, dispatch a fresh one; never message the dead one.

After a merge-manager reports a merge: if the PR changed `app/package-lock.json`, run `npm ci` in the root `app/` before creating any further worktree (stale root `node_modules` propagates into every clone). Confirm the issue closed; if not, close it with a comment naming the PR.

## Step 2b — rulings before dispatching

For every agent report since the last iteration whose final message ends in a `**Fork:**` section, and for every decision the list in *Decisions* names that this iteration's state shows, dispatch one deputy per fork (background, `model: "opus"`) before dispatching any implementer. A fork still open at the end of an iteration is reported as such, with the deputy's task named as running.

## Step 3 — dispatch the frontier

Take free candidates in ascending issue number until the ceiling and the two caps are reached; a candidate a cap refuses is skipped this iteration and stays `ready-for-agent`. For each ticket N:

1. Claim: `gh issue edit N --remove-label ready-for-agent --add-label in-progress`.
2. Record the branch point: `SHA=$(git -C /Users/dev/Projects/knobas rev-parse origin/main)` after a `git fetch`.
3. Worktree: `git -C /Users/dev/Projects/knobas worktree add .worktrees/issue-N -b issue-N "$SHA"`, then `cmp app/package-lock.json .worktrees/issue-N/app/package-lock.json && cp -Rpc app/node_modules .worktrees/issue-N/app/node_modules`.
4. Dispatch one implementer (Agent tool, background, general-purpose, `model: "opus"`) whose prompt names: the brief path, the issue number, its parent spec issue, the worktree path, the branch-point SHA, the scratchpad directory, and these three sentences verbatim: "Run the full `just check` once at the end, in the foreground, with the Bash timeout at its maximum; never background it. The PR body's `Closes #N` line goes outside backticks; verify with `gh pr view --json closingIssuesReferences`. Stop when the PR is open; you never merge."

A ticket that says files it needs are uncommitted in Björn's checkout (e.g. #594's `CONTEXT.md` and ADR-0018): before creating its worktree, check `git -C /Users/dev/Projects/knobas status --short` for them; if present, copy them into the new worktree and tell the implementer they are Björn's text, to commit as-is. Never commit or stash in the root checkout.

## Step 4 — wake-up and stop

- If you dispatched or merged anything: `ScheduleWakeup` with `delaySeconds: 1800`, `noop: false`, `prompt: "/v15-next"`, reason naming the tickets out. Agent completions wake you sooner; the wake-up is the fallback.
- If nothing changed and agents are still running: `ScheduleWakeup` 1800 s, `noop: true`.
- If a fork is open and nothing else can move: `ScheduleWakeup` 1200 s, `noop: true`, reason naming the fork and its deputy.
- If no candidate is free, no ticket is in-progress, no PR is open and no deputy is running, the tracker is empty: report it and `ScheduleWakeup` with `stop: true`. A ticket blocked only by a ticket that is not `ready-for-agent` (e.g. `needs-info`) is reported by number, not waited on.

## Report each iteration, in one short message

Merged (PR, issue, merge commit) · dispatched (issue, worktree) · **ruled** (issue, the ruling in one line, the comment's URL) · forks open and their deputies · still running · blocked and on what (a cap, a tunnel, a fork) · the next wake-up. Never paste agent output.
