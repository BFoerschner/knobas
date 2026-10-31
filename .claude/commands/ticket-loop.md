---
description: One iteration of the general ticket loop. Merge what is open, run milestone exits, dispatch every free ready-for-agent ticket. Run as `/loop /ticket-loop`.
---

You orchestrate the knobas ticket loop. This file is one **iteration**: do it once, schedule the next wake-up, end the turn. You never implement or review a ticket yourself; you dispatch agents, sequence merges, and do the mechanical steps below.

This file names no spec, milestone, version or ticket. **The tracker wins over this file.** When sources disagree, the more specific wins: ticket body › milestone description › parent spec (`## Parent`) › `CLAUDE.md`, `docs/agents/working-model.md`, ADRs, `CONTEXT.md`, `docs/decisions/` › this file. One exception: the *Models* line under *Ceiling* beats a ticket's `## Model` section. New rules for a milestone go in its description; rules for implementers or merge-managers go in their brief master; edit this file only when the loop's mechanics change.

First iteration of a session: read `docs/agents/working-model.md` and `docs/agents/issue-tracker.md`; keep the Mac awake with `pgrep -x caffeinate -q || (nohup caffeinate -i -w $PPID >/dev/null 2>&1 & disown)` (it dies with this session; a sleeping Mac has cut agents mid-response); then read `.claude/loop/in-flight.tsv` (see *Ledger*).

**Unattended: no permission prompts.** One prompt stalls the loop until Björn is back. The Bash hook refuses the shapes that prompt, for you and every agent you dispatch (CLAUDE.md, Machine): take the form its message names, or skip the step and name it in the report. Start piped commands with `set -o pipefail;` or `| tail` hides the exit code.

## Ledger

`.claude/loop/in-flight.tsv` (gitignored; one row per running agent: `issue<TAB>role<TAB>worktree<TAB>model<TAB>slot<TAB>started`) is how work survives a usage limit, a crash or a sleep. Append a row at every dispatch, delete it when the report is in. At the start of an iteration, a row whose agent ListAgents no longer shows and whose PR isn't merged is **orphaned**: re-dispatch the same role on the same worktree, and the brief says "Continue from the worktree's last commit `<sha>` (`git log --oneline -5`) and its uncommitted diff; do not start over." Usage limits killed 15 running agents on three days in October before the 3-agent cap; this is the recovery path.

## What a ticket is

Every open issue labelled `ready-for-agent`, in any milestone or none, except a **spec** (title begins `Spec:`, or has sub-issues); specs are never claimed. A sub-issue of a spec that has a `spec-<N>` branch on `origin` belongs to that spec's `/implement-spec` run (`docs/agents/integration-branch.md`) and is never claimed here either; check with `git ls-remote --heads origin spec-<N>`. The body is the brief. A ticket is **blocked** when `issue_dependencies_summary.blocked_by` > 0, or a number in its `## Blocked by` list is open. A **milestone exit** (title begins `Milestone exit`) never goes to an implementer (Step 3); everything else does.

## Decisions (orchestrator's side; the briefs carry the workers' side)

Whoever does the work decides, records the ruling in the PR under **Rulings** and as a comment on the issue, and never waits for Björn. You rule only loop-level questions: order, nothing dropped, a cap that blocks the frontier is reported, not worked around.

A step a ticket, milestone or spec **reserves for Björn** is ruled by the **Opus** agent on the work. Every such ruling goes in the iteration report under *rulings recorded*.

A criterion no agent can witness is handed on in the PR ("Not witnessable by an agent, handed to the milestone review: …") and taken up by the milestone reviewer.

**Issues you file** follow the briefs' *Filing a follow-up* (Björn, 2026-10-23): a failed release workflow, a red Linux run, a live-fixture break, a gap a report hands you. Search for the class and fold an instance into the open issue that names its cause; else one issue per root cause, its body carrying the cause, every instance and the search. The same holds for what a reviewer, splitter or researcher files for you. **Fixing comes before filing** (Björn, 2026-10-24): implementers and merge-managers fix a small gap in the PR they hold (the briefs' *Fix it in place first*), so a gap a report hands you is one that failed that fit test. A checking pass you dispatch outside a ticket (rechecking a review's fixes, an audit, a sweep) gets the same default in its prompt: it fixes what passes the fit test in one PR from its own worktree under the implementer brief, *self-merge* when that PR fits *Self-merge* (Step 4) and with a merge-manager otherwise, and files only the rest. The milestone reviewer is the exception: it certifies the milestone, so it fixes nothing (Step 3).

## The brief files

Masters: `/Users/dev/Projects/knobas/docs/agents/implementer-brief.md` and `merge-manager-brief.md` (root checkout, current `main`). Once per session, copy each to the scratchpad as `<role>-brief-<session>.md` (parallel agents share the scratchpad) and replace its *Attribution* block with this session's attribution from the system reminder. Dispatch with the copy's path. A new standing rule goes into the master through a docs-only self-merge and a fresh copy, never into dispatch prompts or memory.

## Ceiling

Defaults unless `docs/agents/working-model.md` or a ruling in `docs/decisions/` says otherwise:

- **Seven agents at once, five of them touching Rust at most** (Björn, 2026-10-31; was five from 2026-10-21, three from 2026-10-16 for the usage limit); merge-managers, reviewers, splitters and researchers count. Seats 6 and 7 take only frontend-only work: an agent whose worktree touches no `crates/` path (the body names none and `scripts/gate-scope.sh` says `front` or `scripts`). Seats are not split by role: a PR waiting for review takes the next free seat before a new implementer; with no PR waiting, implementers may fill all seven. Raise only on Björn's say.
- **At most four Rust builders at once** (Björn, 2026-10-31; was three from the 2026-09-05 measurement in `docs/agents/working-model.md`): an agent counts while its worktree touches `crates/` and it has not merged. A fifth Rust ticket waits; a frontend-only ticket takes the seat instead. No pre-warm while seven agents run.
- **Load does not cap seats** (Björn, 2026-10-30: all seats whatever the load; was three while `scripts/machine-load.sh` exited 1). Still run the script before filling a seat and quote its line in the report when it exits 1 (five-minute load above the cores the kernel recommends, performance cores parked, or on battery under 20%; #1892), and when `memory_pressure | tail -1` shows under 25% free or a reported gate takes over 400 s. Parked cores or a low battery is Björn's to fix (the Mac on its charger), so name it in the report's first line.
- **One gate test run at a time** (#1892; two before), enforced by the recipe's slot guard: a second gate's pre-gate, clippy and vitest run beside it and its test run waits, which is fine. Two at once is what timed vitest out beside them.
- **Models: Opus for every role** (Björn, 2026-10-23): implementers, merge-managers, milestone reviewers, splitters, researchers and the release self-merge. No other model; Fable only when Björn asks for it by name. This beats a ticket's `## Model` section: a ticket that names another model runs on Opus. The dispatch names the model.
- **Seats = slots 1–7** (`just shot` serves slot N on port 5300+N and Chrome on 9400+N; no slot limit). Each dispatch names its slot (`just shot` ports). A seat frees when the implementer's PR shows `ready` in Step 1, not when its report arrives; a draft PR gets its merge-manager at once. Once a report is in, TaskStop the agent if ListAgents still shows it (52 stops in October hit agents already gone) and drop its ledger row; then check nothing it started is still running (`pgrep -fl "just demo|vite --port 530|Google Chrome --headless"`, the app window) and quit any leftover (Björn, 2026-10-21: windows and apps always get exited). An agent "running" whose worktree hasn't changed in ~45 min is probably looping on a cut-off stream: check its subagent jsonl under `~/.claude/projects/-Users-dev-Projects-knobas/<session>/subagents/` for repeated "cut off" entries, TaskStop it, `git checkout --` half-moved files, re-dispatch.
- **One desktop-witness holder at a time.** JVM live runs queue on their own lock, not capped here.
- **Disk:** before creating a worktree, `df -g /Users/dev` must show 20 GB free plus, per worktree whose agent hasn't gated yet (the new one included), 12 GB if it touches Rust, 4 GB if frontend-only (body names no `crates/` path and the milestone isn't backend; or `scripts/gate-scope.sh` says `front` or `scripts`). Below that, create none and report the free space.
- **Throughput log:** after every merge, `.claude/loop/stages.sh <PR> <gate kind> <gate s> <batched PR or -> <implementer model> <agents running>` (figures from the merge-manager's report). The iteration report gives merges/hour over the last three hours, the seat count, and the stage medians; rule changes are judged on those.

## Step 1: read the state (jq only; never print bodies)

```sh
gh pr list --state open --json number,headRefName,isDraft,closingIssuesReferences \
  --jq '.[] | "\(.number)\t\(.headRefName)\t\(if .isDraft then "draft" else "ready" end)\t\([.closingIssuesReferences[].number]|join(","))"'
gh issue list --state open --label in-progress --limit 200 --json number,title,milestone \
  --jq '.[] | "\(.number)\t\(.milestone.title // "-")\t\(.title)"'
gh issue list --state open --label ready-for-agent --limit 500 --json number,title,milestone,body \
  --jq '.[] | select(.title | startswith("Spec:") | not) | "\(.number)\t\(.milestone.title // "-")\t\(.title|startswith("Milestone exit"))\t\(.body|test("desktop-witness"))\t\(.title)"'
```

Check blockers per candidate: `gh api repos/{owner}/{repo}/issues/N --jq .issue_dependencies_summary.blocked_by`. ListAgents for your own agents still running; reconcile with the *Ledger* and re-dispatch orphans before anything new.

## Step 2: merge before dispatching

For every open PR without a live merge-manager, ascending PR number, dispatch one (background, model per *Ceiling*). Reviews run in parallel; the merge tail serialises on `/Users/dev/Projects/knobas/scripts/merge-tail-lock.sh` (root checkout's copy; oldest PR first; each merge-manager gates before the lock, merges without a re-gate when `main` has not moved since, and re-gates alone when it has, per the brief). The prompt names: brief path, lock script path, PR, issue, worktree `.worktrees/issue-<N>`, slot, open live-fixture issues with their suites. A merge-manager that died mid-turn gets a fresh replacement; never message a dead agent. An agent whose last notification said *waiting on its own background work* and that `ListAgents` no longer lists is dead (2026-10-23: #1834, #1817): replace it. A dead holder frees the merge-tail lock on its own within about 11 min of its last lock call (#1836), and the replacement's `take` gets the lock back if its PR still holds it.

After a reported merge: if the PR changed `app/package-lock.json`, fast-forward the root, run `npm --prefix /Users/dev/Projects/knobas/app ci` before creating any worktree, and refresh every live, non-gating worktree whose lockfile now matches the root's but whose `app/node_modules/.package-lock.json` doesn't (`cp -Rpc` from the root, then `touch` the worktree's `app/node_modules/.package-lock.json`). Confirm the issue closed (else close it naming the PR), remove the worktree, log throughput, stop the agent.

## Step 3: milestone exits

An exit is free when `gh issue list --milestone <m> --state open` holds only it. Its body's steps win; by default:

1. **Review.** Claim it. Dispatch one milestone reviewer (background, Opus) that implemented none of the milestone's tickets (check your dispatches and the merged PRs' authors). Prompt: exit issue, milestone, parent spec, slot, the no-prompt rule, and: "You are an independent reviewer. Check the milestone against its exit ticket and its tickets' acceptance criteria on current `origin/main`. Take up every PR line 'Not witnessable by an agent, handed to the milestone review': witness it yourself where you can (screenshots of `just dev` against local live products count), otherwise list it for Björn. File each gap as a `ready-for-agent` issue in this milestone and add it as a native blocker of the exit issue; never fix anything yourself. Post your verdict on the exit issue as a comment beginning `Milestone review:` with **Approved** or **Gaps: #a, #b** and the evidence. Your final message is that comment. Run `just check` once, in the foreground, in a worktree at current `origin/main`. `check.yml` is disabled (Björn, 2026-10-24, #2035, `docs/decisions/2026-10-24-bjorn-rulings.md`), so nothing gives a Linux opinion: name that gap in your verdict (`docs/agents/working-model.md`, *Platform*). Once Björn runs `gh workflow enable check.yml`, dispatch `gh workflow run check.yml --ref main` once here instead."
2. **Gaps:** exit back to `ready-for-agent`; the gaps run through the loop; the next review covers only what changed.
3. **Approved:** dispatch an Opus *self-merge* implementer for the release PR (full gate): bump the workspace version in `Cargo.toml`, `Cargo.lock`, `crates/knobas-app/tauri.conf.json` `version`, and `bundle.windows.wix.version` (numeric only, and it must exceed the value at the previous release tag or Windows won't upgrade); `package.json` stays; body `Part of #<exit>`. Then yourself: annotated tag on the merge commit (name from the body), push it, `gh workflow run release.yml --ref main -f tag=<tag> -f platforms=all`, `gh run watch --exit-status` in the background. The workflow leaves a draft: `gh release edit <tag> --draft=false --prerelease --latest=false --notes-file <milestone summary + its notes>` (final releases: `--latest`, not pre-release). Close the milestone (`gh api -X PATCH repos/{owner}/{repo}/milestones/<n> -f state=closed`) and the exit issue with a comment linking the release. A failed workflow becomes a `ready-for-agent` issue.

## Step 4: dispatch the frontier

Fill free seats in this order. A **direct request from Björn** runs beside the queue as soon as it doesn't overlap code in flight, within the ceiling; it never waits behind review follow-ups.

1. **Loop tickets** (`Loop:`), before anything else, a waiting PR's merge-manager included. While one is free or in progress no non-loop implementer starts; merge-managers still fill spare seats. A loop ticket that measures time **holds the machine**: dispatch it alone and start no implementer beside it until it reports.
2. **Live-fixture fixes** (`Live fixture:`; a failure every run of a live suite hits).
3. **Milestone tickets**, milestone first (TF1 before TF2 …), then issue number.
4. **No-milestone tickets**, by issue number. In 3 and 4, a ticket without a `refactor<N>` label goes before every ticket with one (Björn, 2026-10-19).
5. **Empty frontier:** promote nothing and dispatch no researcher (Björn, 2026-10-29). Report what `backlog` holds (count, the oldest three by number and title); Björn promotes with `gh issue edit N --remove-label backlog --add-label ready-for-agent`. Close an overtaken backlog issue with a comment saying what overtook it.

**Bundle** up to three tickets that edit the same screen (same prototype-6 route or view component) or the same files: one implementer, worktree `issue-<lowest>`, one PR with one `Closes #N` per ticket, all claimed. A ticket joins when free or blocked only by the bundle. A free ticket whose screen a running agent is editing waits for that merge.

**Split candidate:** more than 6 ticket-specific criteria (ignore the seven standard TF lines: screenshots, copy, fixture, design system, testenv data, §10.8, gate), or more than one distinct `#/…` route. If the routes are plainly tabs or sub-routes of one screen, rule "not split" yourself from prototype-6's render function and dispatch whole. Only when genuinely unclear, dispatch a splitter (background, Opus, no worktree): it decides; "not split" → comment and back to `ready-for-agent`; otherwise one `ready-for-agent` ticket per screen in the same milestone with the original's `## Parent`, criteria carried over word for word with each in exactly one piece, native blocking between pieces, mapping commented on the original, original closed as split, exit's blockers repointed. Pieces enter next iteration.

**Self-merge:** mechanical work (docs or ADR wording, a seed or fixture fix, one more test, a narrow query or sync fix) or a small visible change a screenshot shows, no frozen surface, no live suite, expected diff under about 300 lines (Björn, 2026-10-29; was 150); or **docs-only** (`docs/**`, plus Markdown anywhere outside `crates/` and `app/`; under `.claude/` only Markdown counts, since `scripts/gate-scope.sh` scopes `settings.json` and `loop/` `full`, #1982), which needs no full gate, no `node_modules`, no disk reservation, but runs `just docs-check` (doc-list and conflict-marker checks, seconds) on the merge head before merging. Dispatch says *self-merge* and names both brief paths and the lock script; no merge-manager.

**Dispatch extras:** TF* tickets name `docs/specs/2026-10-15-ticket-first-shell-conventions.md` with "These conventions are already ruled; follow them, don't re-record them; a new one you rule goes in that file in your PR." Hot-path tickets (search, queries, `filter_suggestions`) get "Measure on 2–3k items while you work; run the full 22k responsiveness probe once, at the end. Compare warm runs: the first run after a relink is ~2× (macOS assesses each new binary)."

For each ticket N:

1. `gh issue edit N --remove-label ready-for-agent --add-label in-progress`.
2. `git -C /Users/dev/Projects/knobas fetch`; `SHA=$(git -C /Users/dev/Projects/knobas rev-parse origin/main)`.
3. `git -C /Users/dev/Projects/knobas worktree add .worktrees/issue-N -b issue-N "$SHA"`; unless docs-only, `cmp` the two `app/package-lock.json` and `cp -Rpc /Users/dev/Projects/knobas/app/node_modules /Users/dev/Projects/knobas/.worktrees/issue-N/app/node_modules`. Never symlink it. Then `touch /Users/dev/Projects/knobas/.worktrees/issue-N/app/node_modules/.package-lock.json`: git stamped the worktree's `app/package-lock.json` newer than the copied stamp, and `just deps` reinstalls on exactly that. Then `mkdir -p /Users/dev/Projects/knobas/.worktrees/issue-N/target && touch /Users/dev/Projects/knobas/.worktrees/issue-N/target/.metadata_never_index`, so Spotlight skips the build tree (a worktree `target` without it kept `mds` busy on every build, 2026-10-19).
4. Dispatch one implementer (background, general-purpose, model per *Ceiling*) naming: brief path, issue(s), parent spec, worktree, SHA, scratchpad, slot, desktop-witness slot yes/no, open live-fixture issues with suites, *self-merge* if it applies, the extras above. Append its *Ledger* row (same for every merge-manager, reviewer, splitter and researcher you dispatch).

**Pre-warm:** when a seat will free soon (a PR is open, or a merge-manager holds the lock) and the next ticket touches Rust (`crates/` in body, or backend milestone), create its worktree now (disk guard applies, no claim) and run `env -C <worktree> env -u RUSTUP_TOOLCHAIN CARGO_BUILD_JOBS=2 cargo test --workspace --no-run` in the background (not under `nice`: a niced cargo that starts the sccache server nices every compile on the machine for the server's life, #1892), logging to the scratchpad. Not while a machine-holding loop ticket runs. The dispatch says the warm-up may still be running. Never hand a `target` between worktrees.

Never commit or stash in the root checkout; copy any uncommitted files a ticket needs into the worktree and say they are Björn's, to be committed as is.

## Step 4b: close finished specs

A spec is finished when every sub-issue and every issue whose `## Parent` names it is closed (`gh search issues --repo BFoerschner/knobas "## Parent" "#<spec>" --state open`, then check bodies) and no open PR names one of them. Close it with a comment listing tickets and PRs. A spec whose milestone still has an open exit closes after that exit.

## Step 5: wake-up or stop

- Something dispatched, merged, reviewed or released: `ScheduleWakeup` 1800 s, `noop: false`, `prompt: "/ticket-loop"`, reason naming what is out.
- Nothing changed, agents running: 1800 s, `noop: true`.
- **Empty** (no free candidate, nothing in progress, no open PR, no agent): report and `stop: true`, listing tickets blocked only by `needs-info` / `ready-for-human` issues and what `backlog` holds (Step 4 item 5).

## Report, one short message

Merged (PR, issue, commit) · merges/hour and seats · dispatched (issue, slot) · milestone reviews (exit, verdict) · releases (tag, run) · rulings recorded (issue, one line) · handed to Björn · still running · blocked and on what · next wake-up. Never paste agent output.
