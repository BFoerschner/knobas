# Memory archive, 2026-10-19

Björn, 2026-10-19: agent memories get skipped, so standing rules now live where the agent reads them: `.claude/commands/ticket-loop.md`, `docs/agents/implementer-brief.md`, `docs/agents/merge-manager-brief.md`, `CLAUDE.md`, `docs/agents/working-model.md`. This file is the raw dump of every project memory that existed that day, for provenance: the rulings and their reasons, in the words they were recorded in. Nothing here binds on its own; where a rule still applies it is in one of the files above.

## a-stub-can-be-wrong-about-the-tool

_A hand-written stub encodes an assumption about the real tool; when it is wrong the fixture is structurally blind to the bug, and mutation testing cannot find it either_


#367's stub modelled `curl --max-time` as truncating its `-o` output file when a request times out. It does not: verified against curl 8.7.1 with a socket that accepts and never answers, a body written before the timeout survives intact. So a 500-then-hang sequence printed the *previous* attempt's refusal under "no reply in 240s" — one attempt's answer presented as another's — and every scenario in the harness passed, because each scenario gave one kind of answer and the body never crossed between attempts.

Found by #372's merge review, which tested the tool rather than the model.

**Why:** a stub is an assumption in executable form. Mutation testing proves the tests catch changes to *your* code; it says nothing about a shared premise the code and the tests both inherit from the stub.

**How to apply:** when a fixture stands in for an external tool, ask what the fixture asserts about the tool and verify that against the tool once, by hand. Suspect it especially where the fixture gives each scenario one kind of answer — the bugs live in the transitions between them. Same class as [[subagent-findings-need-quotes]]: the model was never checked against the thing.

## acceptance-criterion-that-cannot-pass

_"An acceptance criterion can fail in the other direction — unsatisfiable on a correct implementation, like a non-recursive grep for a file the ticket asks you to put in a subdirectory"_


The known failure class is a check that passes on broken code. There is a mirror image, and #398 shipped
one: a criterion that **cannot pass on a correct implementation**. Its wording was

> One shared module; `grep -n "fn day_window" crates/knobas-app/tests/*.rs` returns only the shared module.

The same brief asked for the module at `crates/knobas-app/tests/live_digest/mod.rs`, following the
`crates/knobas-source-gitea/tests/live_env/mod.rs` precedent. `tests/*.rs` does not descend into
`tests/live_digest/`, so on a correct implementation the grep returns **nothing**. The implementer and
the merge-manager each spotted it and said so; a less careful pair would have either "fixed" the layout
to satisfy the glob or quietly ticked a box the evidence did not support.

**Why it matters:** an unsatisfiable criterion pushes the agent toward changing correct work to match a
broken check, and it makes the ticked box meaningless either way.

**How to apply, when writing a brief:** run the acceptance command in your head against the *finished*
state you are asking for, not against today's tree. For anything under a directory, write `grep -rn`.
For a count, name the expected number and where it comes from. And ask the mirror of the usual question:
not only "could this pass while the code is wrong?" but "could this fail while the code is right?"
See [[triage-brief-fixture-claims]], [[knobas-implementer-brief]], [[contract-header-grep-anchor]].

## agent-cost-is-context-times-turns

_"Measured 2026-10-16 — implementer cost is ~85% cache reads (context × turns); reusing a finished agent for the next ticket costs more, not less"_


Measured 2026-10-16 on four Opus implementers (#852, #853, #863, #842 bundle) by summing `message.usage` in their subagent jsonl: cache reads 11–46 M tokens each vs cache writes 0.16–0.38 M and output 12–27 k. At Opus price ratios (read 0.1×, write 1.25×, output 5×) cache reads are ~84% of cost. The #853 agent's context went 17 k → 145 k in its first 3.5 min of reading, then crept to 275 k over ~100 more turns.

Björn asked whether to "cache the code" / reuse agents across tickets on one screen; agreed to test one ticket. The data showed a reused agent would start the next ticket at ~275 k context, so every turn pays for the old ticket: likely 1.5–2× a fresh agent. Not run.

**How to apply:** to cut cost, shrink early context and late turns, not reuse agents: narrow reads (grep -n then sed ranges), per-screen area notes, and moving mutants/Standards review off the implementer's fat tail to the fresh Sonnet merge-manager (proposed to Björn, awaiting his answer). Script: copy of `token-usage.sh` (jq sum of usage per subagent jsonl, optional since-timestamp). Related: [[loop-token-budget]].

## agent-scratchpad-filename-collision

_Concurrent subagents share one scratchpad dir; generic filenames collide and can publish one agent's content under another's name_


Concurrent subagents in one session share the same scratchpad directory. Generic
filenames collide silently: on 2026-08-28 the #37 agent wrote its PR body to
`scratchpad/pr.md`, the #52 agent wrote its own to the same path, and `gh pr edit`
then published #52's description onto PR #65. Caught and fixed by the #37 agent.

**Why:** the collision is invisible — no error, no overwrite warning. The failure
surfaces only as wrong content on a shared, outward-facing surface (a PR body,
an issue comment), after the write has already happened.

**How to apply:** when dispatching parallel agents, tell each to use a filename
unique to itself in the scratchpad (e.g. `pr-65.md`, not `pr.md`). Applies to any
scratch file, but especially anything that feeds `gh pr edit` / `gh issue comment`.
Relates to [[knobas-pr-agent-workflow]].

## agent-stuck-on-cut-off-stream

_"A subagent writing one huge file can loop on "response was cut off" for hours while ListAgents still says running; detect via transcript timestamps"_


2026-10-16: the #844 bundle implementer (Opus) spent 15:27–16:41Z retrying one giant Write; every ~16 min the transcript logged "Your response above was cut off mid-stream" and a fresh api-request, no tool calls. ListAgents said "running" the whole time; worktree files had not changed in 90 min. Its half-done refactor had already cut 480 lines out of `checkout.rs`.

**Why:** no notification fires for a stuck-but-alive agent; a quiet worktree is the only outside sign.

**How to apply:** when a worktree shows no file change for ~45 min, `jq` the subagent jsonl (`~/.claude/projects/-Users-dev-Projects-knobas/<session>/subagents/agent-<id>.jsonl`) for the last tool_use timestamps and "cut off" user entries. If looping: TaskStop, restore half-moved files with `git checkout --`, re-dispatch with "no single Write/Edit over ~250 lines". Worth adding that line to [[knobas-implementer-brief]].

## background-exit-code-swallowed

_A trailing echo/tee makes the harness report exit 0 for a command that failed; read the log, not the status_


`cmd > log 2>&1; echo "EXIT=$?" >> log` and `cmd | tee log` both report the
**last** command's status, so a backgrounded failure arrives as "completed
(exit code 0)". This happened twice in one session (2026-09-04, the v0.1.0
release): a failed `tauri build` and a failed `gh run watch` were both announced
to Björn as successes before the logs were read.

**Why:** it is the repo's own named failure class — a check that measures a
representation of the thing instead of the thing (`working-model.md`, "Shifting
left"). The wrapper's status is a representation of the command's status.

**How to apply:** run the command bare under `run_in_background` and let the
harness report its real code; if a log is needed, redirect without a trailing
statement. Either way, confirm a success by reading the log tail and by looking
at the artifact the command was supposed to produce — never by the reported
status alone. Related: [[a-pgrep-watch-matches-itself]],
[[a-stub-can-be-wrong-about-the-tool]].

## baseline-first-run-after-relink

_"ADR-0017 baselines — first `just test` after a relink is ~2× (syspolicyd scans new binaries); deps size never shrinks in a working tree"_


Measuring #571's baselines for a crate merge (#574/#581, 2026-10-10): the first `just test` after `touch knobas-core` relinks everything ran 136 s vs 66 s warm, with syspolicyd at the top of `ps` scanning new executables. Compare the **warm re-run**, and report the load average. `du target/debug/deps` stays 30G after a merge because cargo never deletes old per-file test binaries. Only a fresh worktree or `cargo clean` shows the saving.

**Why:** an unwarmed number reads as a 2× regression and could wrongly refuse a merge PR ("gate must not get slower").
**How to apply:** for #575–#579, follow #581's PR body template. Run `just test` twice and use the second run. See [[knobas-gate-is-one-minute]].

## bash-tool-shell-is-zsh

_"Bash tool shell is bash 5.3 since 2026-10-19 (CLAUDE_CODE_SHELL); zsh quirks (`echo ====`, =cmd expansion) are history. Still true: no pagers, no `pgrep -c` on macOS, no `timeout` binary"_


Since 2026-10-19 the Bash tool runs `/opt/homebrew/bin/bash` (5.3): Björn set `CLAUDE_CODE_SHELL` in `~/.claude/settings.json` `env`. PATH comes from `~/.bashrc` (homebrew, ~/.cargo/bin, orbstack, `mise activate bash` above its `$PS1` guard). If a session shows zsh behaviour, or cargo/just is missing ("No version is set for shim: cargo"), check that setting and `~/.bashrc` first.

Before that it was zsh, where `echo ====` failed (`=cmd` expansion) and `$(...)` samplers misbehaved. Those quirks are gone.

**Still true, shell-independent:**
- Never call a pager (`man`, `less`, `git log` without `--no-pager`): a `man` call hung a review agent for 13 hours on 2026-09-06. Use `MANPAGER=cat`, `git --no-pager`.
- macOS `pgrep` has no `-c`; use `pgrep -f … | wc -l`. See [[pgrep-watch-matches-itself]].
- macOS has no `timeout` binary; use the Bash tool's timeout parameter.
- The no-`cd`-in-compound rule ([[no-permission-prompts]]) is a harness prompt rule, not a shell one; it still applies.

Related: [[background-exit-code-swallowed]].

## claude-md-globally-ignored

_knobas's CLAUDE.md is untracked because ~/.config/git/.gitignore ignores `CLAUDE.md`; worktrees under .worktrees/ have no CLAUDE.md, so agents there see none of the project instructions unless briefed_


Found 2026-09-05 by the #419 Standards reviewer: `git ls-files CLAUDE.md` is empty in the repo
root and `git check-ignore -v` names `/Users/dev/.config/git/.gitignore:1: Claude.md CLAUDE.md`.
The file exists only in the root checkout.

**Why:** every implementer and merge-manager works in a `.worktrees/<name>` checkout, which
therefore has no CLAUDE.md; the skills, tracker conventions and planning-flow pointers it holds
reach an agent only through the dispatch brief ([[knobas-implementer-brief]] tells them to read
it, and they cannot).

**How to apply:** point agents at `/Users/dev/Projects/knobas/CLAUDE.md` read-only, or ask Björn
whether to `git add -f CLAUDE.md` (or drop the line from the global ignore) so worktrees carry it.
Do not assume a worktree agent has read it.

## closing-keyword-in-backticks

_"`Closes #N` inside backticks does not link or auto-close — #209 stayed open after PR #233 merged and had to be closed by hand; brief implementers to put the keyword outside code spans and verify closingIssuesReferences"_


PR #233 (issue #209, 2026-08-31) wrote `` `Closes #209` `` inside backticks in its body. GitHub does
not parse closing keywords inside a code span, so the link never formed, the merge did not close the
issue, and the milestone listing showed #209 OPEN with its work already on `main`. #208 and #210
auto-closed from the same loop because their PRs wrote the keyword bare.

**Why it matters:** the milestone view is what Björn gates on; an open issue with merged work reads
as unfinished, and the fix is a manual close that has to explain itself.

**How to apply:**
- Every implementer brief: "reference `Closes #N` **outside backticks**".
- Implementers verify before reporting: `gh pr view <n> --json closingIssuesReferences`. The #238
  and #239 implementers did this after the brief said so, and both linked.
- When an issue is still open after its PR merged, check the PR body for a code span before
  assuming anything else went wrong. Close it with a comment naming the PR and merge commit.

Related: [[knobas-pr-agent-workflow]].

## contract-header-grep-anchor

_"The §10.8 enclosing-header lookup must anchor on `^#{1,6} ` — a bare `^#` also matches Rust attribute lines inside the contract's fenced sketches and can name the wrong header"_


The recipe for "which section is this `docs/contract.md` line in?" is:

```sh
grep -nE '^#{1,6} ' docs/contract.md | awk -F: -v L=<line> '$1 < L' | tail -1
```

The **space after the hashes and the 1–6 bound are load-bearing**. A bare `^#` also matches the
`#[derive(...)]` / `#[serde(...)]` attribute lines inside the file's fenced Rust sketches: measured
2026-09-04 on `docs/contract.md`, 89 lines match `^#` and only 47 are real headers. So the bare form
can report an attribute line as the enclosing header, or skip past the real one.

**Why:** the whole point of the lookup is deciding whether an edit sits inside §10.8, the
single-writer frozen section — a wrong answer there is the difference between "prose amendment to a
ratified entry" and "unratified change to a frozen surface", and Björn's gate cares which claim was
made. The unanchored version was in every implementer and merge-manager brief for this repo until
#390's implementer noticed it while doing exactly this check.

**How to apply:** use the anchored form in every brief and every PR body that claims a line is inside
or outside a section; quote the header line the grep returns rather than reasoning from the line
number. The related trap — "line 4597 is well past the frozen list, so it is outside §10.8" — is
wrong for a different reason: §10.8 is the file's *final* section and runs to the end. See
[[knobas-implementer-brief]], [[knobas-merge-manager-brief]], [[knobas-frozen-ipc-layout]].

## decide-dont-park-for-triage

_"Björn 2026-10-13 — don't park findings as needs-triage for him; rule on them as deputy and dispatch, unless it is a sign-off gate he kept"_


Björn, 2026-10-13, on #715 left as `needs-triage` "for your triage": "what's going on with 715, what do i have to do. i thought i said you can decide."

**Why:** under the 2026-10-12/13 delegations he expects the orchestrator to rule on follow-ups itself; a ticket waiting on him is a stall he did not ask for.
**How to apply:** when a review files a follow-up, rule on it (comment "Ruling (deputy for Björn …"), label it `ready-for-agent` and dispatch it. Only hand him what he explicitly kept: closing specs, milestones, tags/releases. Related: [[loop-triages-and-runs-to-empty]], [[loop-deputy-is-opus]].

## devtools-first-launch-scan

_macOS first-launch scan costs about 25-30 s per gate on fresh test binaries; Developer Tools exemption removes it; Ghostty is exempted since 2026-10-16 (0.4 s); restart tmux from Ghostty to inherit it_


#957 (2026-10-16): after a Rust change, each of the 53 freshly linked test binaries waits about 0.5 s on its first launch while macOS assesses it (syspolicyd), 23-32 s in total. The gate's `inventory` step pays it because it launches the binaries first. Running them in parallel doesn't help, because the assessments are serial.

- **Terminal.app** added to System Settings → Privacy & Security → Developer Tools: first launches 0.6 s.
- **Ghostty**: Björn exempted it on 2026-10-16. Its shells then took 0.4 s for the first launches, against 31.7 s before. The setuid `/usr/bin/login` in the chain does not matter. Earlier tests showed no effect only because no exemption was in place yet.
- **A tmux server started from Terminal.app** passes the exemption on to everything it spawns, even with Ghostty as the attached client (0.5 s). No longer needed now that Ghostty is exempted. A long-running tmux server still has to be restarted from the exempted terminal to inherit the exemption.
- **Check** with `scratchpad/probe957.sh` from session 8f6a23eb, or by timing first and second `--list` launches after a rebuild.

Related: [[baseline-first-run-after-relink]] (the first `just test` after a relink is about 2×; this is that cost's cause).

## dirty-tree-rides-across-branches

_"Uncommitted user work follows `git checkout` onto a feature branch, where `git add -A` silently commits it into someone else's PR"_


Björn's working tree on `main` is often dirty with in-progress docs (M2.5 ADRs,
CONTEXT.md edits). Uncommitted and untracked files **follow a `git checkout`**
onto a new branch, and the `git add -A` used to stage a feature commit then
sweeps them into that PR without a word.

This happened on 2026-08-30: ADR-0009 and a CONTEXT.md glossary entry — Björn's
M2.5 mini-board work, unrelated to any M2 issue — landed in PR #182's review-fix
commit. Caught only because `git status --short` came back empty on `main`
afterwards, which it should not have.

**Why:** `git stash push -u` at the start of a multi-branch session is not
enough. Popping the stash between branches, or popping it and then checking out
another branch, re-arms the trap. Each `git checkout` is a fresh chance to carry
the tree along.

**How to apply:** Before `git add -A` on a feature branch, run `git status
--short` and confirm every listed path belongs to the work at hand. Prefer
staging named paths over `-A` when the session started from a dirty tree. Keep
the stash stashed until every branch is pushed, and restore it only after the
final `git checkout main`. Verify with `git show --stat` after committing — the
file list is the cheapest check there is.

Recovery, if it already happened: save the strays out of the branch
(`git show <branch>:<path>`), `git checkout <base> -- <path>` plus `git rm` the
untracked ones, `git commit --amend --no-edit`, re-run the gate, then
`git push --force-with-lease`, and restore the files to `main`'s working tree.

Related: [[knobas-pr-agent-workflow]], [[agent-scratchpad-filename-collision]].

## fable-credit-exhaustion-substitute

_"Fable 429 "out of usage credits" kills every in-flight Fable subagent mid-turn; Björn's ruling 2026-09-01 is Opus merge-managers as the substitute, and re-dispatched agents must not message the dead review context"_


On 2026-09-01, during M2.6's last two PRs, Fable hit `HTTP 429 You're out of usage credits`. Three
Fable subagents died at once, mid-turn: the merge-managers for #243 and #244 and the long-lived
code-review context that had been carrying the stream since #209. Nothing was lost (all work was
committed, `main` untouched), but the delegation instruction had named Fable specifically.

**Björn's ruling:** "use opus merge managers." Not Sonnet, not wait-for-credits, not hand back.

**Why:** the model was the mechanism, not the point — the point was not blocking on him. Opus was
judged close enough in rigour for review-and-merge when the brief and the objective termination
rule do the heavy lifting. He kept the M2.6 milestone merge itself.

**How to apply:**
- When Fable dies with 429, do not ask a four-option question; re-dispatch on Opus and say so.
- Brief re-dispatched agents that the prior review context is dead and must not be messaged, and
  that any reviewer they spawn must pass `model: "opus"` or `"sonnet"` explicitly.
- The die-mid-turn shape looks like the stall in [[subagent-background-gate-stall]] but is not one:
  the task-notification says `failed` with the 429 in the summary. Check that before poking.
- Verify state first (`gh pr view` on each open PR, `git worktree list`, `git status`) — a dead
  merge-manager may have taken ownership of a worktree without changing anything.

Related: [[knobas-pr-agent-workflow]], [[gate-is-local-not-ci]].

**2026-09-03 addendum:** mid-run through the M3 loop Björn said "use opus for the remaining agents" — pass `model: "opus"` on every Agent dispatch (implementers and merge-managers alike) for the rest of that session; agents already in flight keep running as dispatched.

## frontend-only-gate

_Björn 2026-10-15 evening — batched merges + pre-warm (#917); `just gate` runs check-front for frontend-only heads; plus CARGO_BUILD_JOBS=4, oldest-PR-first merge lock, vitest --changed, seat handoff on PR open, spec digests_


Björn, 2026-10-15: "apply all of them, including the narrower frontend gate". Landed in PR #912: `just gate`, `scripts/gate-scope.sh`, `scripts/merge-tail-lock.sh`, the /ticket-loop skill and working-model rule 4.
- Gate: `just gate` = `check-front` (contract-index check + `front`) when every changed path is under `app/` or prose and nothing in `crates/` reads one outside a `//` comment; else full `just check`. The crates `include_str!` the IPC mirrors (`app/src/lib/ipc/*.ts`), `fake-tauri.ts` (only `fakeSmartLists`, which #908 was asked to move to its own file) and a few views, so those keep the full gate.
- `CARGO_BUILD_JOBS=4` on every cargo/just command (12 cores, up to six agents compiling).
- Frontend-only diffs: no cargo while iterating; `vitest --changed origin/main`.
- Internal follow-ups → `backlog` label (the rule was missing from the briefs).
- Merge lock: oldest waiting PR first (heartbeat queue, 120 s staleness).
- Seat frees once the implementer's PR is open with a recorded gate.
- Re-copy `node_modules` into matching worktrees after a lockfile merge.
- Milestone `## Spec digest` in TF descriptions (#913); dispatches point at it.

**Why:** load of 34 and 600 s gates at five seats; most TF merge tails ran a cargo half on a tree main had already passed.

**How to apply:** amends [[ticket-loop-is-the-loop]] and [[gate-is-local-not-ci]]; the briefs ([[knobas-implementer-brief]], [[knobas-merge-manager-brief]]) carry an "Amendments, 2026-10-15 (evening)" section.

**Same day, later** (Björn: "i approve all those edits"), PR #917: batched merges (the lock holder stacks the oldest `ready` waiter, gates the pair once, merges both; the waiter merges only if its rebased tree equals the gated tree; its `take` exits 3), a nice-19 pre-warm of the next Rust ticket's worktree when a seat is about to free, and a fixed eight-line merge-manager report.

**Night amendments** (Björn: "implement" 1–4): iteration cargo/vitest at `nice -n 10` (gates normal priority); release the merge lock right after the merge, before cleanup; implementers don't rebase mid-work; merge-managers pre-build `cargo test --no-run` at nice 10 during review. Not adopted yet: a per-recipe gate timing ticket and Sonnet merge-managers for small check-front PRs.

**One gate per ticket** (Björn, 2026-10-15 night, "apply these right now"): the implementer opens its PR as a draft, runs `just pre-gate` plus the tests beside its change, then marks it ready. The merge-manager runs the only full gate and reviews drafts at once. Self-merge implementers still gate. Also: `just shot <slot> <out> <routes…>`, and a warm `target` handed from a merged worktree to the next Rust worktree (`.worktrees/.warm-target`, rm incremental first). Revisit the warm target after #905. Supersedes the "gate runs twice" rule in [[gate-is-local-not-ci]].

**Late night** (Björn: "apply all of them"), PR #922: `.claude/loop/stages.sh` writes per-stage minutes (claim→draft→ready→merged) to `/Users/dev/Projects/knobas/.worktrees/.throughput.tsv` after each merge; a ticket with more than 6 criteria or more than one route goes to a Sonnet splitter first; small check-front PRs (under about 300 lines) get Sonnet merge-managers. Loop tickets filed: #920 (time each gate stage and the slowest test binaries, holds the machine; includes the `vitest --changed` check for check-front) and #921 (prefill the TF2–TF6 fixture modules from the digests).

**2026-10-16:** the warm target is suspended (PR #924). Per #920, handed-on targets keep path-baked build-script output, and 9 packages needed cleaning.

**2026-10-16 (Björn):** screenshots only when the ticket changes what the app draws; each PNG is opened at most once per route; merge-managers open none (PR #955). Images are expensive in an agent's context.

## gate-is-local-not-ci

_"Every knobas workflow is workflow_dispatch only; the merge gate is a full local `just check`, and a dispatched run is the only CI signal that exists"_


Amended 2026-09-05 (Björn: "make them only trigger manually"). `check`, `testenv`
and `release` are all `workflow_dispatch:` and nothing else, and `check`/`testenv`
are `active` again — enabled, because a disabled workflow cannot be dispatched at
all. **No push, pull request, merge or tag starts anything.** This supersedes the
2026-08-29 `disabled_manually` state (a billing outage), which is still the
history recorded in `docs/archive/2026-08-29-unattended-batch-plan.md`.

**Why:** the triggers used to sit in the files behind the disable switch, so
`gh workflow enable check` would have put a full build back on every PR. The
switch is gone now, not just off — so the cheap state no longer depends on
anyone remembering.

**How to apply:** the merge gate is still a full local foreground `just check`
including the frontend half (see [[subagent-background-gate-stall]],
[[knobas-rustup-toolchain-override]]). `gh pr checks` still answers "no checks
reported" on every branch — never read that as a verdict. But a run **you
dispatched** is real evidence: `gh workflow run check.yml --ref <branch>` buys
the Linux opinion a Mac cannot give, at 1x billing, and it is worth it before a
milestone exit. It found #406 the first time it ran. A release is now two steps —
tag, then `gh workflow run release.yml --ref main -f tag=<vX.Y.Z> -f platforms=both|macos|windows`;
macOS bills at 10x, so name one platform when iterating.
Related: [[knobas-windows-build-proven]], [[local-gate-is-blind-to-timezone]].

## handoff-lives-in-tmpdir

_"Check the handoff" means $TMPDIR/handoff-*.md (macOS /var/folders/.../T/), written by /mattpocock-skills:handoff — never the repo root_


`/mattpocock-skills:handoff` saves to the OS temp dir, not the workspace: on this Mac `$TMPDIR` = `/var/folders/ph/p63g3rb537b55_4cf_536s7r0000gn/T/`, files named like `handoff-583-checkout-ipc-flake.md`. `HANDOFF.md` at the repo root was retired in #171, so its absence proves nothing.

**Why:** 2026-10-11 I searched the repo, `/tmp` and old job dirs, concluded "no handoff", and implemented #583 without it. Björn: "there 100% is one". The handoff held a coupling the issue didn't mention (the twin pin in desktop-witness-test.sh) plus PR-body expectations.

**How to apply:** when told to check/read the handoff, first `ls -t "$TMPDIR"/handoff-*.md` and read the newest matching the task. Follow its PR/merge instructions; see [[knobas-pr-agent-workflow]].

## implementer-decides-forks

_Björn 2026-10-15 — during implementation the implementer rules its own forks; no deputy/other agent referral_


Whenever a decision comes up while implementing a ticket, the implementer decides it and records the ruling in its PR. Do not refer it to a deputy, merge-manager or another agent.

**Why:** Björn, 2026-10-15, when the ticket-first spec #808 was cut into tickets. Referral costs a round trip for no gain.

**How to apply:** put this in implementer briefs ([[knobas-implementer-brief]]). Supersedes the "pre-rule forks" / deputy-rules-forks parts of [[loop-efficiency-rules]] and [[decide-dont-park-for-triage]] for implementation-time forks. Björn's own gates (spec, frozen contracts) still stand.

## knobas-concurrency-ceiling-raised

_Björn raised the working-model's 2-Rust-implementers ceiling mid-loop on 2026-09-03 ("start 3 more rust subagents, the cpu is bored"); the limit is a guideline he adjusts by watching load_


During the M3 ticket loop (2026-09-03), with two Rust implementers running and load ~9 on the 12-core machine, Björn said: "you can start 3 more rust subagents. the cpu is bored." Five concurrent Rust agents were then run.

**Why:** the working-model's "at most 2 concurrent implementers" rule dates from a 79-load incident; Björn treats it as a guideline calibrated by the load he sees, not a hard cap. He watches `uptime` himself.

**How to apply:** start at the documented ceiling, report load in status lines, and scale up when Björn says so (or when an explicit ruling in the dispatch allows it); never scale up silently. Keep the Docker test environment single-owner regardless of how many agents run. See [[knobas-pr-agent-workflow]].

**Superseded 2026-09-06 (Björn, on #422):** three concurrent Rust agents, at most **two gates
(`just check`/`just test`) at once**, enforced by a slot guard in the `test` recipe (#425). The
"five when the CPU is bored" reading is dead: five gates exhaust macOS's 32 SysV shared-memory
segments regardless of CPU (see [[sysv-shm-caps-concurrent-gates]]). Reviewers still count as one
agent each.

**Superseded again 2026-10-15 (Björn, applied to `/ticket-loop`):** five agents at once (4 implementers + 1 merge-manager while a PR is open, 5 implementers when none). The two-gate slot guard is the only gate cap; a third gate waiting for a slot is expected. See [[ticket-loop-is-the-loop]].

## knobas-crossplatform-build-proven

_knobas builds and bundles on all three desktop targets with zero source changes (2026-09-05, v0.1.0); only macOS has ever been run_


The cross-platform question M3 left open is answered for *building*. `release.yml`
produced all six artifacts for v0.1.0 with **no source change** on any platform:

- **macOS** `.dmg` (arm64, ad-hoc/linker-signed, `CFBundleIdentifier` present)
- **Windows** `.exe` (NSIS) + `.msi` (WiX) — ~25 min, first compile ever
- **Linux** `.deb` + `.rpm` + `.AppImage` — first try; the deb declares only
  `libwebkit2gtk-4.1-0, libgtk-3-0`

So `windows-native-keyring-store`, `linux-keyutils-keyring-store`, `notify-rust`'s
non-macOS paths and `postgresql_embedded`'s theseus fetch all compile as declared.

**Why:** the M3 exit report lists Windows and Linux as unwitnessed, which reads as
"might not even build". They build. What is unwitnessed is *running*: no Windows
or Linux machine has launched the app, so the keyring, the notifier and the
embedded-Postgres download are proven only to typecheck there.

**How to apply:** scope "unwitnessed" to runtime, not to the build. Two gotchas
worth not rediscovering — the Linux leg needs `librsvg2-dev` and `patchelf` on top
of what `check.yml` installs (that file installs the non-bundling subset on
purpose), and the AppImage bundler needs `APPIMAGE_EXTRACT_AND_RUN=1` because a
runner has no FUSE. Artifacts carry the runner's glibc floor; `ubuntu-latest` is
the current choice and `ubuntu-22.04` is the wider-compatibility alternative.
Related: [[gate-is-local-not-ci]].

## knobas-design-by-mockup-process

_"knobas (Rust+Tauri work-integration app) is being designed via rounds of parallel clickable HTML mockups before any app code; round 1 = 5 paradigms × 5 designs, plan file and decisions recorded here"_


knobas = Rust + Tauri desktop app unifying Jira / Confluence / Gitea / TeamCity with local Postgres FTS, cross-source links, notes, write-back, time tracking, inbox, standup generation. Started 2026-08-22.

Process the user chose: **no app code until a UI is picked from mockup rounds**. Round 1 (2026-08-22) = 25 mockups, 5 paradigms (P1 Launcher, P2 Workbench, P3 Graph, P4 Context hub, P5 Journal) × 5 designs (D1 Signal, D2 Ledger, D3 Patchbay, D4 Atlas, D5 Index card), built by parallel agents in per-paradigm git worktrees (`.worktrees/r1-P<n>`, branches `mockups/r1-P<n>`), merged to `main`, index at `mockups/round-1/index.html` (regenerate: `node mockups/build-index.mjs round-1`). Briefs live in `mockups/shared/`. Later rounds repeat until the user says the design is good; then spec (now `docs/specs/`; planning flow changed 2026-08-28, see [[knobas-matt-skills-flow]]) → build.

Plan file: `/Users/dev/.claude/plans/i-want-to-create-witty-token.md`.

**Why:** online search in Jira/Confluence is bad and credential juggling is painful; the user wants to see interaction paradigms side by side before committing.

**How to apply:** keep using the shared dataset/screens briefs for every new mockup; parallelize by paradigm worktree; harness caps concurrent subagents at 20, so launch 25 in two waves. Don't start Rust/Tauri code before the user picks a design. See [[knobas-time-tracking-no-branch-awareness]].

## knobas-frozen-ipc-layout

_"contract §10.8 freezes the commands/+ipc/ module layout ("no new module") and requires a §10.8 exception entry per migration/IPC addition"_


`docs/contract.md` §10.8 freezes not just the IPC schema but the **module layout**: `crates/knobas-app/src/commands/` and `app/src/lib/ipc/` grow no new files — new commands go into the existing module of the owning stream (`entity.rs`/`entity.ts` for entity-stream features; #42's entry spells out "the frozen thing is the layout, not the existence of commands inside it"). Every migration or IPC addition also requires its own dated exception entry in §10.8, and the entry's claims must match the diff exactly (reviewers check). Björn's 2026-08-29 delegation ("let Migration and ipc additions be merged by fable too", widened same day to `knobas-source/**` and `knobas-http/**`) lets Fable merge-managers merge those; milestone exits and contract-battery clauses stay his.

**Why:** In #47 I created `commands/contexts.rs` and had to fold it back into `entity.rs` mid-review; the precedent was already written in the #42 entry.

**How to apply:** Before adding any Tauri command, migration, or event: read the newest §10.8 entries for the current next-free migration number and the delegation state, put commands in the existing module, append both barrels, and write the §10.8 entry in the same PR. Related: [[knobas-pr-agent-workflow]].

## knobas-gate-is-one-minute

_Since 2026-09-05 (#411, PRs #416/#417/#418/#420) `just check` is ~51 s idle and `just test` ~47 s; one shared Postgres per gate, xargs -P 6, front beside cargo; the old 3–6 min numbers and the 2-implementer ceiling derived from them are stale_


Measured 2026-09-05 on the merged main a9b436f, 12-core machine, warm, no source change:
`just check` 202 s → 51 s, `just test` 161 s → 47 s, scheduler_loop 30 s → 13 s,
postmasters per gate 61 → 1. Three `just check` under a concurrent serial `cargo test
--workspace` ran 53–64 s green with zero shmget/initdb markers (the #249 criterion).

**Why:** #418 gave `just test` one `knobas-test-server` per invocation (test binaries read
`KNOBAS_TEST_DB_URL`; unset = old per-binary server, so `cargo test -p` by hand is unchanged);
#420 runs binaries through `xargs -P 6` with per-binary capture under `knobas-gate.XXXXXX`
(never name scratch dirs `knobas-test-*`: the reapers sweep that prefix); #417 runs `front`
beside the cargo chain; #416 put scheduler timing behind `SchedulerDeps.timing`.

**How to apply:** briefs no longer need "3–6 min"; a Bash timeout of 600000 still covers a cold
worktree build. The working model's concurrency ceiling (2 Rust implementers) was measured
against the serial shape; #422 (ready-for-human) asks Björn to re-measure. A gate that takes
5+ min now means a cold `target/` or a stuck server, not normal. Known follow-ups: #421 (server
teardown ~10 s of the 47), #419 (two `time_ipc` tests red on Linux only: nanosecond `Utc::now()`
vs microsecond `timestamptz`). Related: [[gate-is-local-not-ci]], [[knobas-implementer-brief]],
[[knobas-merge-manager-brief]].

## knobas-implementer-brief

_The common rules file handed to every implementer subagent in the ticket loop; copy to the scratchpad, swap the Attribution block, pass the copy's path in the dispatch_


# Implementer brief — common rules (knobas)

You implement the GitHub issue(s) named in your dispatch on `BFoerschner/knobas`, in the worktree named there. Read this whole file first. Ticket body › milestone description › parent spec (`## Parent`) › this file.

## No permission prompts

The loop runs unattended; one prompt stalls it until Björn is back. Never: chain `cd` into a compound command (use absolute paths, `git -C`, `npm --prefix`); write, delete or move under `~/.claude/` or any `.git/` from Bash (git commands are fine); edit shell config. Skip such a step and name it in your report.

## The contract

- `gh issue view <N> --json body,comments`. The **body** is the contract; the parent spec is context. If the milestone description has a `## Spec digest`, read it first and only the spec sections it cites. Milestone rules apply to you.
- Every ADR in `docs/adr/` is in force; a PR that contradicts one is wrong by construction.
- Re-verify the ticket's diagnosis on the current tree before fixing; say in the PR if a sibling change overtook it.
- Read `/Users/dev/Projects/knobas/CLAUDE.md` (gitignored, absent from worktrees), `docs/agents/working-model.md`, `CONTEXT.md`, and `docs/contract.md` §10.8 if the ticket touches IPC or migrations.

## Decisions

**You decide the forks that come up while implementing**; nobody waits for another agent or for Björn. Record each ruling in the PR under **Rulings** (fork / ruling / reasoning / cost of reversing) and append it to `docs/decisions/<yyyy-mm>-unattended-rulings.md` in the same PR (create it in the shape of `docs/archive/2026-08-29-unattended-batch-rulings.md`). A ruling may not contradict a ticket, spec, ADR or the glossary: where the work shows one of them wrong, do what the document says, file a `ready-for-agent` follow-up naming the conflict, and say so in your report.

A step the ticket **reserves for Björn** (sign-off, approval, a choice): on Opus, rule it, headed "Reserved for Björn, ruled by implementer on Opus", tick the criterion and name the ruling beside it. On Sonnet, do everything else and name the step in your report; never rule it.

A criterion no agent can witness (by-eye judgement, a human-only step) is not a fork: do everything else and write in the PR *"Not witnessable by an agent, handed to the milestone review: <criterion>"*.

## How you work

Invoke `mattpocock-skills:tdd` before writing code.

- **Git only inside your worktree, `gh` only against your own PR.** Never touch the root checkout or `main`.
- Diff, reset and squash against the **SHA in your dispatch**; `origin/main` moves whenever a sibling fetches.
- **Commit per task** and before going idle: `git status --short`, stage named paths only, check `git show --stat`.
- **Don't rebase mid-work**; only when a conflict blocks you, or once at the end if `git merge-tree` shows one.
- **Builds:** `export CARGO_BUILD_JOBS=4` at the top of every Bash call that runs cargo or `just`; `nice -n 10` while iterating; raw cargo as `env -u RUSTUP_TOOLCHAIN cargo …`. One cargo build of yours at a time. No `just dev`/`just demo`/testenv unless the ticket says so. A diff that touches nothing outside `app/` and prose runs no cargo at all.
- **Frontend tests:** `npm --prefix app test -- <files>` (vitest must run in `app/`); for what your diff reaches, `npm --prefix app test -- --changed origin/main`; svelte-check `npm --prefix app run check`. Rune-driving tests need the `*.test.svelte.ts` suffix. UI changes also run `src/lib/shell/a11y.test.ts`.
- **Test inventory:** after adding or removing a test, `just inventory-update` and commit `test-inventory.txt`.
- **Scratch files** in the scratchpad from your dispatch, issue number in every filename (shared directory).
- **No pagers** (`man`, `less`, `git log` without `--no-pager`): they hang the tool.

## Live suites, witness, screenshots

- `just *-live` brings its product up locally, seeds it, and stops it (`testenv/README.md`). JVM products queue on a machine-wide lock, up to 45 min: "waiting" is queued, not stuck. Never `hcloud`; never create or delete a server.
- Run only the live suite your change exercises, narrowest filter. A failure your dispatch names as a known fixture break is not yours: cite its issue in the PR. A new failure that is not yours: file `Live fixture: …` as `ready-for-agent` and name it in your report.
- `just desktop-witness <driver>` takes the Mac's screen: once, foreground, only if your dispatch gives you the witness slot.
- **Screenshots only when the ticket changes what the app draws**; otherwise none, and the PR says the standard screenshot line doesn't apply. `just shot <slot> <out dir> <route>...` (slot from your dispatch; dev server on `5300 + slot` under `?fake-ipc`; `SHOT_THEME=light|dark`; quote `#` routes; `'#/a > #/b'` opens a first, `'#/x + click:<name>'` or `click:testid=<id>` captures a dialog). Never hand-script a browser; what `just shot` can't capture, skip and name. One theme, only the routes you changed; Read each PNG at most once per route to name differences from prototype-6 (read its source, never its PNGs); the PR links the images, it doesn't describe them.

## Testing what you claim

- **3 to 6 mutants on the load-bearing logic**, against a committed baseline: mutate one file, print the `-`/`+` diff (an unapplied substitution "passes" with nothing mutated), run the tests, restore with `git checkout -- <that file>`, never a directory. Proof (mutant, test that died) goes in the PR.
- Break the mechanism, not a downstream copy of its value; a constant the corpus also reads, or a cap no fixture reaches, makes a mutant that cannot die.
- State the scope of every result (binary, filter, test count). Ask whether the fixture can witness the claim, and which *direction* is unwitnessed.
- §10.8 is `docs/contract.md`'s last section and runs to the end of the file. Before claiming a line is outside it: `grep -nE '^#{1,6} ' docs/contract.md | awk -F: -v L=<line> '$1 < L' | tail -1` and quote the header.

## Finishing

The merge-manager runs the ticket's only full gate; you don't run `just gate` or `just check`.

1. Push (`git push -u origin <branch>`) and open the PR **as a draft** (`gh pr create --draft --title "<imperative subject>" --body-file <scratchpad>/pr-<N>.md`) so review starts at once.
2. `mattpocock-skills:code-review` against your branch SHA, **Standards axis only** (the merge-manager owns Spec). Fix findings.
3. `just pre-gate` (runs `cargo fmt --all`), then the tests beside your change: vitest files you touched or reach, `cargo test -p <crate>` per crate changed. Foreground, normal priority. Push fixes.
4. `gh pr ready <n>`.
5. PR body: what and why; acceptance criteria as a checked list; head SHA the checks passed on and what ran; mutation proof; **Rulings**; one `Closes #N` per ticket **outside backticks** (verify `gh pr view <n> --json closingIssuesReferences`); the footer from *Attribution*.
6. **Stop.** You never merge unless your dispatch says *self-merge*. Report: PR URL, what ran on which SHA, mutation summary, what the reviewer should look at first, anything left out and why.

**Self-merge** (dispatch says so): once ready, take the lock named in your dispatch, run `just gate` once on the merge head (foreground, Bash timeout 600000; docs-only needs no gate), then follow *Merge and cleanup* in the merge-manager brief named in your dispatch and report the merged commit.

**Follow-ups:** `ready-for-agent` if Björn would notice it in the app, else `backlog`.

## Attribution

(The orchestrator replaces this block with the session's.)

Commit messages end with `Co-Authored-By: <model> <noreply@anthropic.com>`. PR bodies end with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.

## knobas-links-local-exportable

_"In knobas, links are the core feature — manual + suggested, stored locally only (never written into Jira/Confluence/etc.), and everything knobas owns must be exportable/importable; monitoring is Uptime Kuma's, not knobas's"_


The user stated (2026-08-23): **links are the main thing** in knobas. They want suggestions *and* manual linking; links stay local (knobas never writes links into the source systems); everything knobas owns must be exportable and importable. Also: knobas does not own monitoring — Uptime Kuma is their monitoring system; knobas only syncs from it.

**Why:** the source systems' own linking/search is poor; the value is the local link graph and the ability to move/back it up. Monitoring already exists and works.

**How to apply:** design every surface with *Link to…* and suggestions; store links/contexts/notes/assets/time locally with stable entity ids; plan export/import (plain files, merge-by-id) as a first-class feature, not an afterthought; treat Uptime Kuma (and other systems) as adapters, never as things knobas replaces. Design doc: `docs/superpowers/specs/2026-08-23-knobas-design.md` §5a and §14. See [[knobas-design-by-mockup-process]].

## knobas-m4-grilling-outcome

_"2026-09-06 M4 grilling rulings not yet in the repo's spec issue — Orchestra not Flowrun, the estate is the real test infrastructure, three gated sub-milestones, Björn writes the spec"_


Grilled 2026-09-06 (`/grill-with-docs`, all four rounds ruled by Björn taking every recommendation). Written to disk in that session: `CONTEXT.md` **Estate** section (Estate, Asset, Route, Monitor, Alert, Import) + **Tree** entry, and ADR-0014 (containment is a parent field, not a link). The M4 spec issue is **#427** (written 2026-09-06 via `/to-spec` at Björn's request, milestone M4, `ready-for-agent`); GitHub milestones M4 (#9), M4.0 (#10), M4.1 (#11), M4.2 (#12) exist. #427 supersedes the bullet list below where they differ. Tickets #428–#455 cut 2026-09-06 (`/to-tickets`): M4.0 #428–#440 (exit ticket #440), M4.1 #441–#453 (exit witness #450; #451–#453 droppable), M4.2 #454–#455. No exit-gate issue: Björn gates at the exit ticket's merge (the M3 pattern). Frontier at cut time: #428 (asset model), #438 (estate file), #441 (Kuma seed, testenv only).

Rulings that live only in that conversation until the spec exists:
- **Flowrun is gone.** The real system is called **Orchestra**; **no adapter for it is planned** (the 2026-09-06 "later feature" booking was struck by Björn on 2026-09-07, PR #490: adapters are added by demand, none is booked; do not re-book it). Asset types `runtime` and `scenario` stay generic so the tree can hold one by hand. Port 8213 unreserved, ADR-0013's exception void.
- **The estate is the real test infrastructure** (three Hetzner servers, local Gitea and Kuma, the notebook, tunnel forwards as routes), not Tidewater fiction: `testenv/estate.json` with stable ids (`asset:knobas-jira`), loaded by an Import in the Assets view and by `--demo`. Env `dev`, owner Björn. Import adapters (hcloud/Docker) later.
- **Cut:** M4.0 Estate → (M4.1 Monitoring ∥ M4.2 Share export), each gated by Björn. M4.0 defers secret properties, "Depends on this", asset smart lists, status-page dialog.
- **Kuma:** one source, API key required (`/metrics`), account optional in the same keychain envelope; write-back (create/pause) is a droppable last M4.1 ticket. A poll is an ordinary sync run (exhaustive `monitor` kind, 60 s default). Own timeseries table, 90-day retention, in backup, out of share. Global warn threshold 1500 ms. Alert = sixth inbox category, keyed on the monitor entity, no own address; ack local; recovery closes it. Monitors: ping per Hetzner server (firewall allows only 22 + ICMP), HTTP via the tunnel (labelled), Gitea direct, plus a **canary** on a throwaway port the live recipe binds/releases — never stop a shared container for a test.
- **Share export** = the same pg_dump custom archive restricted by table; parts links/assets/contexts/sources on, notes/time off; no merge-restore in M4 (empty-DB restore only).
- **Assets tile:** All work shows the estate's top level; a source room shows assets its monitors attach to; project rooms show nothing.

**Why:** these are ratified decisions a later session could re-litigate or contradict while ticketing.
**How to apply:** treat as settled; if the spec issue exists, it supersedes this note. See [[knobas-real-products-on-hetzner]], [[knobas-frozen-ipc-layout]].

## knobas-m4-next-loop

_"M4 closed 2026-09-07 and v1.5 closed 2026-09-11 (main at 72cf54e5), but `/loop /v15-next` lives on: the exit's seven leftovers sit in milestone `v1.5 follow-ups` (14), a container whose description carries the order and the caps"_


Since 2026-09-06 the M4 tickets (#428–#455, spec #427) are driven by `/loop /m4-next`, a self-paced loop over the repo command file `.claude/commands/m4-next.md` (check `git log -- .claude/` before trusting it: the pinned agent roles were retired from that directory in `e9040ee`, and this file is the only thing meant to be there now). One iteration = merge open PRs one merge-manager at a time, then dispatch free tickets (every `## Blocked by` issue closed) up to the ceiling, then `ScheduleWakeup` 1800 s as the fallback; agent completions wake the session sooner.

- **Claim label `in-progress`** (created 2026-09-06): removed `ready-for-agent`, added at dispatch, cleared when the PR merges and closes the issue. Without it the next iteration re-picks the same tickets.
- **Briefs:** the masters are [[knobas-implementer-brief]] and [[knobas-merge-manager-brief]] in this memory directory; each session copies them to the scratchpad and rewrites the attribution block, which is stale in the masters.
- **M4 tickets have no Agent Brief comment**; the body is the contract, and the implementer brief's "Agent Brief comment" paragraph must be overridden by the appended line the command file specifies.
- **Gates:** the loop stops itself when #440 (M4.0 exit), #450 (M4.1 exit) or #455 (M4.2 exit) merges, so Björn gates; he restarts the loop afterwards.
- Frontier at creation: #428, #438, #441.
- **M4.0 landed 2026-09-06** (PRs #456–#469, main at 5ff4313); the loop stopped itself at #440's merge and the M4.0 exit gate is Björn's. Open rulings he was handed: #428's headless-Chrome-vs-`--demo` criterion (unsatisfiable, retire/move/rewrite), reachable-via reading both directions (glossary amended, spec #427 stories 13/31 and the design doc's Decided row not), no ticket owns editing environment/owner/status from the pane, contexts unreachable from Link to… (no context corpus, since #40), route URL path segments unsearchable, `env:`/`health:` chip wording stale. #441 (M4.1) was dispatched early because its body listed no blockers; only #442+ wait on #440.

- **M4 closed 2026-09-07** (Björn's ruling after #479 and #486 landed; main at f6898eca): every ticket #428–#455 closed, all three exits merged (#440, #450, #455 via PR #472), M4.2 was two tickets by design, #475 wontfix. Spec #427 and milestones M4/M4.1/M4.2 closed by this session. Running `/loop /m4-next` again is a no-op until a next milestone re-uses the command file with new numbers. Björn's own triage PRs (e.g. #487, branch `triage/...`, no closing issue) do appear in the loop's PR list and get a merge-manager like any other. The ADR-0013 consequence for faults no live instance can produce is PR #490, opened for Björn to merge.

**Why:** Björn asked for a loop that picks the next free tickets, spawns agents, waits, repeats, with the smallest possible context growth.
**How to apply:** do not run implementers or reviews in the loop session; keep tracker reads to jq one-liners; the ceiling and the in-progress claim are what keep two sessions or two iterations from double-dispatching. See [[knobas-concurrency-ceiling-raised]], [[fable-credit-exhaustion-substitute]], [[knobas-m4-grilling-outcome]].

**2026-09-07, v1.5:** the variant is `.claude/commands/v15-next.md` on branch `v1-5-glossary-and-adrs` (lands via #494); run `/loop /v15-next`. Differences from M4: spec #491, tickets #492–#510, one milestone `v1.5`, no sub-gates; the loop tags `v1.0.0` and dispatches `release.yml` itself after #492 merges and holds the frontier until that run is green; caps of one tunnel ticket (#495, #498, #509, #510) and one desktop-witness ticket (#500, #501, #503) at a time. Brief copies are `implementer-brief-v15.md` / `merge-manager-brief-v15.md` with four appended lines (no Agent Brief comment; tunnel first and leave the fixture; desktop witness foreground and solo; ADR-0015/0016 in force).

**2026-09-11, v1.5 CLOSED.** Milestone 13 and spec #491 closed; **`main` at the exit is `72cf54e5`**. Twenty-nine tickets: #492–#510 plus ten the loop filed on rulings (#518, #522, #525, #531, #533, #537, #547, #548, #551, #552). `/loop /v15-next` is now a no-op. What this run taught, beyond the M4 notes above:

- **The exit needs a live sweep on the exit head, and Step 4's wording does not say so.** It says *green on its last run*; the deputy read it as *green on the head being exited* and filed #551, because seven of eight recipes were last green on heads the milestone then changed under them (every adapter crate via #520, the sync engine's `build_source` via #539). Whether the wording gets fixed is flagged for Björn. Budget an exit sweep plus whatever it finds — here that was two more tickets and two merges after the last "milestone is done" looked true.
- **A ruling no PR acts on still needs a PR.** A runner ticket closes on transcripts and has no commit, so its ruling rides in the first PR the chain produces, or a docs-only PR at the terminal case. The rulings file header now says this (corrected in #553 and #555).
- **The file-list bound is what saves a second sweep.** Once a sweep's transcripts are taken, every later PR in the milestone must stay inside what those transcripts do not certify (`testenv/**` and docs here), and the merge-manager checks `git diff --name-only`, not the claim. Two PRs merged under that bound; one real defect (a stale `justfile` comment) was filed as #554 outside the milestone rather than widen the list, and the deputy confirmed that placement rather than letting it stand.
- **Björn's loop file said the candidate query is jq over `## Blocked by`** — it silently drops any ticket without that heading, which is every ticket the loop itself files. Read the milestone directly as well.

**How to apply:** the loop's shape carried unchanged; what cost iterations was the exit, not the tickets. See [[knobas-v1-5-grilling-outcome]], [[loop-deputy-is-opus]], [[live-suite-is-a-shared-fixture-with-a-budget]].

**2026-09-11, the follow-ups milestone re-uses the loop.** `/loop /v15-next` is *not* a no-op after all: the exit ruling left seven unplaced tickets, and the deputy put them in a new milestone **`v1.5 follow-ups` (number 14)** whose description says in as many words that it "is not a planned milestone in the M0–v1.5 sense, it is a container so the ticket loop can find its work". So read the milestone description before concluding a loop is finished — it can carry the next frontier, the running order and the caps.

- The command file's Step 1 jq filters `.milestone.title == "v1.5"` and therefore returns nothing here; query `ready-for-agent` without the milestone filter and read milestone 14's own ordering instead.
- **The order is in the description, not in `## Blocked by`** (most of these tickets have no such heading): wave 1 #515, #544, #524 (gate-only); then #556, then #541 which needs #556's recipe; then the two tunnel-class tickets **serially** — #516 (also §9, after #515) then #554. Sequencing is the lock: both re-mint a credential the sibling then 401s on.
- **The dispatch brief is each ticket's `Triage (deputy for Björn, 2026-09-11)` comment**, and where a body and its comment differ, the comment wins. #556 has no triage comment; its body is the brief.
- No spec, no grilling, **no frozen-surface touch** in the batch — so a ticket that seems to need a migration or an IPC addition is a fork, not a judgement call.

**2026-09-11 (later), milestone 14 CLOSED — ten of ten, `main` at `41c3573f`.** The follow-ups batch ran end to end unattended: #515, #544, #524, #556, #516, #554, #560, #561, #564, #541, merged as PRs #557–#570 over `72cf54e5..4bd1ca61` plus the docs-only exit PR. Three tickets the loop filed mid-batch (#560, #561, #564) were triaged by a Fable deputy *in the same session* under [[loop-triages-and-runs-to-empty]] and worked off, so the batch ended with an empty tracker rather than a queue. The deputy ruled the exit on #541; no tag, and whether v1.5.0 is cut stays Björn's.

What this run taught, beyond the earlier notes:

- **A milestone's description can carry the frontier.** Milestone 14's own text said it "is not a planned milestone … it is a container so the ticket loop can find its work", and held the running order, the serial tunnel line and the caps. The command file's Step 1 jq filters `.milestone.title == "v1.5"` and returns nothing here — read the milestone description before concluding a loop is finished.
- **Nine assertions that could not fail, caught across ten tickets — every one by mutation, never once by review.** Two shipped inside the very commit that removed another. Three of the failures were in the *instruments*: a `perl -0pi` substitution that silently did not match and printed an all-green "kill", an emptiness check comparing against the index rather than `HEAD`, and a mutant that left a stray `;` so the crate did not compile (a compile error is not a killed mutant). The rule that a negative control must print the diff proving it was applied is now in `docs/agents/working-model.md`, not just the brief.
- **The exit's load-bearing fact was tree identity, not judgement.** No live sweep was owed because the in-batch live re-reads sat on trees byte-identical to their squash commits (`f550fab8^{tree}` = `3830fa9c^{tree}`, `98ccdd86^{tree}` = `f0daedd8^{tree}`). Verified three times — implementer, merge-manager, and against the #552 bound. That is the cheap, exact form of the v1.5 exit-sweep lesson: compare trees, do not argue relevance.
- **A deputy will correct the orchestrator's own ticket.** #564 was filed by this session asking for a guard "of the same shape" as an existing one; the deputy ruled that shape too weak (whole strings coincide only by accident of the TeamCity version) and ordered the existing guard fixed too. Measured afterwards by the implementer: the old shape goes green where the new one goes red.

## knobas-matt-skills-flow

_"Björn retired the superpowers planning flow 2026-08-28 — mattpocock-skills only (grilling → spec → tickets); docs re-homed, task tracking = GitHub Issues milestone M2"_


Decided by Björn 2026-08-28: **do not use superpowers planning skills for knobas** (writing-plans, subagent-driven-development, etc.) — the flow is mattpocock-skills only: grilling → spec → tickets. `docs/superpowers/` was deleted; living docs re-homed: design spec → `docs/specs/2026-08-23-knobas-design.md`, frozen-contract record → `docs/contract.md`, roadmap → `docs/roadmap.md`, agent working model (old roadmap §3) → `docs/agents/working-model.md`. Executed M0/M1 plan files + the carry-over ledger live only in git history (`git show 736ac1f:docs/superpowers/plans/<file>`). Task tracking is GitHub Issues (milestone M2 = #31–#49, created 2026-08-28); triage labels per `docs/agents/triage-labels.md`.

Also that session: all twelve design-doc §16 "Rec 08-24" decisions were **ratified** (2026-08-27, two amendments: desktop notifications → M3; v1 = end of M4), and ADRs 0001–0004 + `CONTEXT.md` were created (per-kind full_sync_exhaustive = ADR-0003; structured SourceError status = ADR-0004).

**Why:** Björn wants one consistent skill toolchain; the superpowers plan-file style duplicated what issues + spec + ADRs now carry.

**How to apply:** plan milestone work by grilling Björn, then write specs/ADRs/CONTEXT.md entries, then GitHub Issues — never new plan files under docs/. The PR loop now runs without custom agents (all four roles retired 2026-08-28, commit `e9040ee`): ticket → mattpocock tdd → PR → mattpocock code-review at deep/standard scrutiny — see [[knobas-pr-agent-workflow]] and `docs/agents/working-model.md`.

## knobas-merge-manager-brief

_The common rules file handed to every merge-manager subagent in the ticket loop; copy to the scratchpad, swap the Attribution block, pass the copy's path in the dispatch_


# Merge-manager brief — common rules (knobas)

You review, fix, gate, squash-merge and clean up the ONE pull request named in your dispatch (plus a batch partner's merge, see *Merge tail*).

## No permission prompts

The loop runs unattended; one prompt stalls it until Björn is back. Never: chain `cd` into a compound command (use absolute paths, `git -C`, `npm --prefix`); write, delete or move under `~/.claude/` or any `.git/` from Bash (git commands are fine); edit shell config. Skip such a step and name it in your report.

## Before you start

Read `docs/agents/working-model.md`, `/Users/dev/Projects/knobas/CLAUDE.md` (gitignored, absent from worktrees), `docs/agents/issue-tracker.md`, `CONTEXT.md`. In the worktree from your dispatch: `git fetch && git status`; the branch must be pushed and clean, else **report, never force**.

**Pre-build** unless `scripts/gate-scope.sh` says `front`: `nice -n 10 env -u RUSTUP_TOOLCHAIN CARGO_BUILD_JOBS=4 cargo test --workspace --no-run` in the background, log in the scratchpad. Wait for it or kill it before taking the lock.

## Decisions

**You rule your review findings**; nobody waits for another agent or for Björn. Record each ruling in the PR under **Rulings** (fork / ruling / reasoning / cost of reversing) and in `docs/decisions/<yyyy-mm>-unattended-rulings.md` in the same PR. A ruling may not contradict a ticket, spec, ADR or the glossary; where one is wrong, follow it, file a `ready-for-agent` follow-up, and say so.

A step **reserved for Björn** that the implementer left open: on Opus, rule it ("Reserved for Björn, ruled by merge-manager on Opus"), tick the criterion, name the ruling beside it. On Sonnet, finish the review but **stop before taking the lock** and report the step; the orchestrator sends an Opus merge-manager.

A PR line "Not witnessable by an agent, handed to the milestone review" is not a blocker: merge on the rest and list it under *Milestone review* in your report.

## Review

Start as soon as the PR opens, even as a draft.

1. `gh pr view <n> --json body,comments`; `gh issue view <N> --json body,comments`. The issue body is the spec; parent spec and milestone description are context.
2. `mattpocock-skills:code-review`, **Spec axis** (criteria, out-of-scope list); the implementer did Standards. Add a **deep pass** against ADR and contract (and the required §10.8 entry) when the diff touches a frozen surface: §10.8 IPC, migrations, write queue, source adapters (`crates/knobas-source/src/**`, `crates/knobas-http/**`), concurrency or locking.
3. Verify claims. Don't redo the implementer's mutants; add one only for a claim you find untested. Ask whether the fixture can witness the claim, and which direction is unwitnessed. A finding about a ticket you haven't read is a hypothesis: read it first.
4. Don't open screenshot PNGs; review the named differences and the code. An image only to settle a doubt the code can't answer.
5. Fix on the branch yourself (`git status --short`, named paths). Out-of-scope fixes: report with the finding quoted. Cap: 3 rounds, then stop and report.
6. Re-run a live suite only if your fixes or the rebase touched what it exercises.

**Follow-ups:** `ready-for-agent` if Björn would notice it in the app, else `backlog`.

## The gate

Yours is the ticket's only full gate: `just gate` on the **final merge head** (after fixes and rebase), **foreground**, Bash timeout 600000, `export CARGO_BUILD_JOBS=4` first. It picks `check-front` for a frontend-only head, else full `just check`; report which. Red is yours to diagnose.

- Contention, not the diff: only `Failed to start forks worker` / `Timeout waiting for worker`, or `initdb` / `pg_ctl` / `shmget: No space left on device`, with zero assertion failures. Check `ipcs -m`, wait a minute, re-run once, say so.
- After the rebase, if `app/package-lock.json` changed and now equals the root's, `cp -Rpc` the root's `app/node_modules` over the worktree's before gating. No frontend half, no merge.
- Raw cargo: `env -u RUSTUP_TOOLCHAIN cargo …`; one build of yours at a time. `gh pr checks` "no checks reported" is normal (Actions are off). No pagers.

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

## knobas-pr-agent-workflow

_"knobas implementation flow since 2026-08-28: NO custom agents — a ready-for-agent GitHub Issue is implemented via mattpocock tdd in its own worktree, PR reviewed via mattpocock code-review at deep/standard scrutiny, then squash-merged by a per-PR merge-manager agent; migrations and IPC go to a Fable merge-manager; Björn keeps only milestone exits and battery clauses"_


Superseded 2026-08-28 (Björn, commit `e9040ee`: "only mattpocock-skills run here"): the four pinned agent roles (implementer, pr-reviewer, pr-reviewer-std, integrator) are **retired**; their definitions live in git history (`git show 06ed97f:.claude/agents/<name>.md`). Do not dispatch those agent types — they no longer exist.

Current flow: a GitHub Issue labelled `ready-for-agent` is the brief → implemented test-first (mattpocock tdd) in its own worktree/branch, `just check` green → PR → reviewed with the mattpocock code-review skill (Standards + Spec axes against the originating ticket) at two scrutiny levels: **deep pass** (frozen contracts, migrations, concurrency/locking, secrets, the write-queue/conflict engine — reviewed against ADR/contract, tests run in a throwaway worktree) and **standard pass** (everything else). The agent's work ends when findings are resolved AND `just check` is green; hard cap 3 review rounds, then Björn adjudicates. **Merging is delegated to a per-PR merge-manager agent** (Björn, 2026-08-28 16:48, commit `da61faf` / PR #75, which explicitly *overrides* the "Björn reviews every PR before merge" rule he set at 11:39 the same day in `49c3baf`). The *implementer* still never merges: it stops when the PR is open. A separate **merge-manager** agent then runs the review pass, drives the fixes, squash-merges with `--delete-branch`, verifies the remote branch actually went, and removes the worktree (reclaiming its ~6 GB `target/`). Merges stay **serial**, orchestrator-sequenced. **Björn keeps the gate for milestone exits** and for **contract-battery clauses** (ADR-0004: "no battery clause added, removed or reworded"; Fable ruled this his on #146). **The frozen surfaces are delegated to Fable** (Björn, 2026-08-29: "let Migration and ipc additions be merged by fable too", widened the same day to cover `WriteOp`/`crates/knobas-source/src/**` growth and `crates/knobas-http/**` when asked — "yes it does") — a Fable merge-manager reviews and merges them under the same discipline, recording a §10.8 ratified-exception entry in `docs/contract.md`. Unsigned local commits are fine (Björn 08-28).

**Why:** one consistent toolchain (mattpocock skills only); review termination stays objective; the machine-earned rules (2 concurrent Rust workers, worktree exclusivity, commit-before-idle) all survive unchanged in `docs/agents/working-model.md`.

**How to apply:** drive tickets with the tdd skill and review with the code-review skill; never recreate the agent roles without Björn asking — a session once partially resurrected them from a stale working tree (`c18c549`, reverted by `0060d90`), so check `git log -- .claude/` before trusting on-disk agent files. **An implementer stops at PR-ready**: once the gate/CI is green, push the branch, open the PR (mutation proof pasted per the standing rule), and stop — then **dispatch a merge-manager for it immediately, without asking** (Björn, 2026-08-30: "automatically start the merge manager whenever an issue is implemented"). One at a time, merges stay serial. Hand it the worktree the implementer just released, and route a PR touching a frozen surface (migrations, IPC, `WriteOp`/`crates/knobas-source/src/**`, `crates/knobas-http/**`) to a **Fable** merge-manager. **Two things to put in every merge-manager brief, both learned 2026-08-30:** (1) tell it to run `just check` in the *foreground* with the Bash `timeout` set to 900000 — otherwise it backgrounds the gate, stops, and reports "waiting for the gate" with nothing waiting on its behalf, once per dispatch; (2) tell it to assume `gh pr merge --squash --delete-branch` did **not** delete the remote branch — after the known cosmetic `fatal: 'main' is already used by worktree` exit 1, the deletion silently does not happen (3 for 3 across #184/#185/#186), so it must `git ls-remote --heads origin <branch>` and delete explicitly. The retired `just autopilot` unattended driver stays deleted. **`docs/agents/working-model.md` is the authority on this, not this memory** — it was amended three times on 2026-08-28 and the last amendment wins; check `git log -- docs/agents/working-model.md` with timestamps before trusting any recollection of the merge rule, including this one. **The gate is about the agent deciding to merge, not about who types it**: on 2026-08-28 Björn reviewed #62/#63/#64 himself and then said "look at the open pull requests and merge them" — an explicit instruction like that is his gate having already closed, so carry it out (squash, house style) rather than refusing it. See [[knobas-matt-skills-flow]].

## knobas-real-products-on-hetzner

_"SUPERSEDED 2026-10-12 (live suites go local on demand, #633; servers decommissioned, #634). Since 2026-09-06 TeamCity, Jira and Confluence run on three Hetzner cx23 servers, one product each; notebook reaches them via testenv/hetzner/tunnel and a PATH docker shim"_


On 2026-09-06 Björn had the opt-in real products moved off the notebook's 8 GB OrbStack VM
onto three Hetzner servers in nbg1: `knobas-teamcity` (cx23, ~EUR 6.50/month), `knobas-jira`
and `knobas-confluence` (cpx22 AMD each, ~EUR 23/month; Intel cx23/cx33 were 2-3x slower on
Jira's single-threaded start, cpx21/cpx31 are no longer offered). One product per server.
`hetzner/env` sets `KNOBAS_TESTENV_HETZNER=1` (parallel Jira+Confluence start in
`just atlassian-live`) and `KNOBAS_ATLASSIAN_KEEP=1` (pair kept between runs, no `down -v`).
Licences: a systemd timer on each Atlassian server runs `renew-licence.sh` every 150 min;
Jira's timebomb expiry = JVM start + 3 h (measured), so it is a restart; Confluence gets
restart + key re-applied via its licence form. Websudo is disabled on both via overrides.
Tooling lives in `testenv/hetzner/` (provision.sh, tunnel, env, bin/docker shim, README.md).
Token is `HETZNER_API_TOKEN` in the repo-root `.env`; the `hcloud` contexts on the machine
(`terra-prod`, `terra-backup`) belong to other projects and must never be written to.

**Why:** "so the notebook has more room". Gitea, Kuma and mockd stay local; TeamCity's VCS
roots reach the notebook's Gitea through a reverse forward carried by the TeamCity tunnel.

Ruling 2026-09-06: Björn wants the Atlassian live loop fast; the kept pair is the design.

**How to apply:** before any seed or `just *-live` recipe: `source testenv/hetzner/env`
(routes docker calls naming knobas-jira/teamcity/confluence to their contexts) and
`testenv/hetzner/tunnel up` (127.0.0.1:8111/8080/8090 forwards; dies with the network).
The local TeamCity pair is *stopped*, not removed; `seed-state.json`'s teamcity block now
points at the Hetzner instance. `atlassian-live`'s "teamcity is running" refusal checks the
local engine and no longer fires. Scripts must stay bash-3.2 clean (`${x^^}` and empty
arrays under `set -u` both broke on first run). See [[knobas-gate-is-one-minute]],
[[knobas-test-against-real-systems]].

**Superseded 2026-10-12 (Björn):** live suites run against real products started locally on demand, one JVM product at a time (#633); the Hetzner servers are to be decommissioned and estate.json replaced (#634). Never use the tunnel/ssh/hcloud from the loop. #634 landed (PR #639, 9ecc4a2d) and Björn confirmed 2026-10-12 that everything on Hetzner is torn down. The hcloud importer's witness is `ESTATE_LIVE_HCLOUD_SERVERS=N just estate-live` against the (empty) project; a `=1` run is Björn's, on demand, when the hcloud producer changes.

## knobas-rustup-toolchain-override

_"RUSTUP_TOOLCHAIN=1.97.1 is exported in this machine's Claude Code environment, silently overriding knobas's rust-toolchain.toml pin (1.94 then; 1.99 since 2026-10-14) for every local cargo/just run, including all subagents"_


Discovered 2026-08-27 during the M1 branch-landing session: the session environment (inherited by every spawned agent) exports `RUSTUP_TOOLCHAIN=1.97.1`, which overrides the repo's `rust-toolchain.toml` pin (1.94). Not set in ~/.zshenv/.zshrc/.zprofile — source unknown, likely the launch environment.

Refinement, verified empirically by a second reviewer (unset the var, re-ran the gate, cargo rebuilt nothing — a rustc change would invalidate every fingerprint): **the `justfile`'s cargo recipes strip `RUSTUP_TOOLCHAIN` themselves**, so `just check` results are on the pinned toolchain regardless. The exposure is **raw cargo invocations outside just recipes** — mutation harnesses, ad-hoc `cargo test` — which silently compile on 1.97.1 unless they strip the var too.

**Why:** a local green on the wrong compiler can hide a pin-only failure (clippy lints differ across versions), and the override is invisible in the command line.

**How to apply:** agent briefs that run raw cargo (not via just) must include `env -u RUSTUP_TOOLCHAIN` and verify `rustc --version` against the pin; `just` recipes are safe. GitHub CI is deliberately off ([[gate-is-local-not-ci]], 2026-08-31), so a local `just check` is the only place the pin is exercised — there is no CI backstop. Related: [[knobas-pr-agent-workflow]].

Update 2026-10-14: the pin moved to 1.99 (branch `toolchain-1.99`, 312327e9). In that session `RUSTUP_TOOLCHAIN` was **empty**, so the export is not always present; the rustup *default* is still 1.97.1. Keep the `env -u` habit anyway: whenever the var is set, it still wins over the pin.

## knobas-test-against-real-systems

_"Björn's 2026-09-02 ruling: adapters and write paths are tested against the real Jira/Confluence/TeamCity/Gitea containers, never certified against mockd; seed scripts and live suites are part of the deliverable"_


On 2026-09-02, during the M3 grilling, Björn revised the test strategy: "I want you to test against the actual confluence/jira/teamcity/gitea etc. I don't want you to test against the mocks. Every time something later happened that didn't work." Every adapter and every write path gets live example tests, seed scripts, and setup scripts against the real container; mockd is not a witness for any exit criterion.

**Why:** Features certified against mockd kept breaking later against the real thing. The mocks encoded assumptions, not behaviour.

**How to apply:** For any adapter or write-op ticket, the acceptance criterion names the real container (`testenv/docker-compose.yml` profiles `real-atlassian`, TeamCity, Gitea) and a `just <system>-live` recipe, following `just teamcity-live-seeded` and `seed-teamcity-builds.sh` (#266, #269). Do not build a mockd Confluence. The Atlassian timebomb licence lasts 3 hours from first apply, so live runs are stand up → seed → test → `down -v`. See [[gate-is-local-not-ci]] and [[witness-gap-ask-which-direction]].

## knobas-ticket-first-mockups

_"2026-10-14 UI redesign round "ticket-first": prototypes 1–5 + requirements.md in mockups/ticket-first/; Björn waived the capability sheet's layout constraints and asked for a fresh perspective"_


On 2026-10-14 Björn rejected the current room/tile UI and asked for a new one. The work lives in `mockups/ticket-first/` (committed 2026-10-15 on branch ticket-first-redesign): `prototype-1.html` (pick a ticket, then work in its "job"), `prototype-2.html` (scale, a filter bar on every view, Docs view, long threads), `prototype-3.html` (link sets + full Code view), `prototype-4.html` (Builds reworked TeamCity-style), `prototype-5.html` (Docs: search-first full text with live preview, Confluence-free reader, Markdown-ready model; the reference), `requirements.md` (R-ids, BUILT/LOW/NEW tags, decisions D1–D16).

**Why:** he wants to feel he works *through* the tool. Without a ticket no work happens. Links should stay out of the way, and suggestions should be easy to judge.

**How to apply:**
- He said "there's no layout constraints" and "I want a fresh perspective". Don't anchor on rooms, tiles or the capability sheet's section 0 for mockups. Still tag capabilities honestly.
- The open decisions D1–D11 in requirements.md are his to rule: Board vs ADR-0009, the "Job" term, an ADR for link sets beside ADR-0008, a `CommitFile` WriteOp for edits through Gitea (never the checkout, ADR-0016), and `git diff` amending ADR-0021.
- 2026-10-14 he dropped the job worklist ("Next on this job") and knobas notes entirely; Pages are the only documents (D10 covers the BUILT notes/capture feature).
- 2026-10-15 he called the job Map (Map 1, columns) not user-friendly. Six alternative drawings now sit as sub-tabs `#/job/<key>/map/2..7` (Trails, Outline, Layers, Distance, Ask, Where it runs), section 4c and D12 in requirements.md. Ask which ones he keeps before building on any of them.
- 2026-10-15 later: he picked Layers and asked for it as the asset page. Built as **Dependencies** (`#/deps`, and the job's Map tab scoped to the ticket), section 11d R-11.10 to R-11.24, D13 to D15. Configurable layers saved in views, click opens (assets open an editor), add in place, folding for 2 000+ items. He wants the real create/edit model (read `app/src/lib/assets/*Dialog.svelte`, `crates/knobas-core/src/asset.rs`), never the prototype's invented one, and everything new written down. Rulings 2026-10-15: D13 yes (Dependencies is the asset page), D15 yes (built-in type labels); D14: layers are one shared set with globally unique names (a name always means the same contents); views keep only show/fold/group, filters and tile size. D16 (2026-10-15): knobas clones on one click into the clones root, reversing ADR-0016's no-clone rule (needs an ADR amendment; R-8.25 holds the safety terms). Open in… is a single button that opens the menu. *This job* is opt-in on Code, Monitors, Docs and Dependencies and means "what the ticket reaches" (R-2.5, R-2.8).
- 2026-10-15 evening: he found ticket views too limited. Built section 3a (R-3.12 to R-3.16): a view editor with conditions, columns with status mapping and limits, lanes, card fields, and a live preview. He then **ruled that Jira boards come in automatically with the Jira source, not through an import dialog** (section 3b, R-3.20 to R-3.26). Jira decides membership (the board's JQL runs at sync, keys only), the columns follow the board, and lanes and cards are set in knobas. *Make an editable copy* turns one into an own view. Same day he ruled all of D1 to D17 (all as recommended; D11/D12: Dependencies only) and ADRs 0022 to 0026 were written (untracked). **Ticket views are their own term, not smart lists**: smart lists span every kind, so a ticket-only layout does not belong on them (D6, `ticket_view` table, no cap). He said to ignore the smart-list cap.
- 2026-10-15: `prototype-6.html` is the new reference: prototype 5 drawn as if every function existed (no BUILT/LOW tags in the UI, rulings applied), with Time (Day, Week, Standup, ported from DayReview/WeekTimesheet/StandupView) and Settings (per-source pages, D10 notes hidden) in full. It is built by `patch_p6.py` from prototype 5; edit prototype-6 directly from now on.
- 2026-10-15 ruling for the build: match prototype-6, no performance work in this round (existing perf gate stays green, "usable on testenv data" by eye, one follow-up ticket if not). Each UI ticket names its prototype-6 route; its PR shows real app vs prototype screenshots side by side. Recorded under "Scope rules" at the top of requirements.md. Next step: Björn types /mattpocock-skills:to-spec. All of it (mockups, requirements, ADRs 0022 to 0026, CONTEXT terms) was committed 2026-10-15 as ae7c5524 on local branch `ticket-first-redesign`, not pushed; on main those files do not exist.
- See [[knobas-design-by-mockup-process]] for the earlier round-1 process.

## knobas-ticket-first-spec

_Ticket-first redesign spec is issue #808 (2026-10-15); milestones TF1-TF6, seams, design-system file, landed on main 57d34af9_


Spec for the ticket-first redesign is GitHub issue #808 (published 2026-10-15, `ready-for-agent`). Whole redesign, slices as milestones TF1 Shell/Tickets/Job, TF2 Time/Settings, TF3 Inbox/Code/Builds, TF4 Docs/Monitors, TF5 Dependencies/link sets/Jira views (+ rooms UI removal), TF6 CommitFile/clone/diff. Mockups, requirements, ADRs 0022-0026 and `docs/specs/2026-10-15-ticket-first-design-system.md` were pushed straight to main (Björn's choice) at 57d34af9.

**Why:** Björn ruled the whole redesign in one spec; seams = the four existing ones (IPC command tests + live suites, component tests over the IPC mirror, `?fake-ipc` headless-Chrome screenshot vs prototype-6, real git in temp dirs), no new seam.

**How to apply:** next step is `/mattpocock-skills:to-tickets` (Björn types it). Builds on [[knobas-ticket-first-mockups]]; UI tickets cite the design-system file.

## knobas-time-tracking-no-branch-awareness

_"User rejected git-branch-aware time attribution for knobas; timer context must be any entity or an ad-hoc label, worklogs go to Jira"_


For knobas time tracking the user wants a manual timer plus passive suggestions (based on what is open in the app), but explicitly **not** git-branch awareness.

**Why:** only git has branches; Confluence, Jira, TeamCity don't, and much of the user's work (e.g. DB configuration) isn't code at all.

**How to apply:** timer context = any entity (ticket, page, note, repo) or an ad-hoc text label; never propose "switch timer when HEAD changes". Worklogs push to Jira (native worklog) with a local copy; day review must handle unattributed blocks and ticket-less labels. See [[knobas-design-by-mockup-process]].

## knobas-v1-5-grilling-outcome

_"2026-09-07 v1.5 grilling rulings (Björn accepted every recommendation): v1.0 tag first, one milestone, ten streams in order, four strikes, one hold, hand-witness rule, Flowrun/Orchestra scrub scope"_


Grilled 2026-09-07 after M4 closed (`/grill-with-docs`, 35 questions, Björn: "all your suggestions" three times). Glossary entries and ADR-0015/0016 committed on branch `v1-5-glossary-and-adrs` (62c40b2b); spec issue is #491 (milestone v1.5 = #13); tickets #492–#510 published 2026-09-07 (#492 gates all; prefactors #500 desktop witness harness, #508 origin key) ([[to-spec-skill-user-only]]).

**Rulings**
- v1.0.0 is tagged and released (dispatched workflow, as v0.1.0) from main before the first v1.5 merge. Spec §16 tier: v1 = end of M4.
- One GitHub milestone `v1.5`, no sub-gates; Björn gates frozen-surface touches and the exit.
- Order: (1) paperwork PR, (2) paste-URL → chip (TeamCity `web_url` first), (3) transitions read, (4) open in editor/terminal, (5) quick capture (+ fixes *New note* pre-linking), (6) asset smart lists, (7) "Depends on this", (8) save search as list, (9) hcloud importer, (10) Docker importer. Three streams at once.
- Struck: environment matrix (no version data without Flowrun), secret property type, status-page dialog. Held: merge-restore, until a second machine or colleague exists.
- Desktop notifications shipped in M3.3 (#290, #339); spec §13's v1.5 bullet is stale.
- Flowrun/Orchestra scrub: living docs and code lose both words (spec, roadmap, working-model, READMEs, testenv files, doc comments, mockd banner, search vocab alias `flowrun→asset`, tests using "flowrun-scenario"); records untouched (contract.md, ADRs, docs/decisions, mockups, migration 0001's comment — checksum). Types `scenario`, `step`, `connector` leave the table; `runtime`, `module` stay; dropped types read as `custom`, no migration.
- OS-level features (editor, terminal, hotkey) are witnessed by **desktop automation, not by Björn** (ruled after the grilling, same day: "I will not be there once the implementation runs"): the signed dev bundle (testenv README, identity `knobas-dev`) launched from its registered path and driven with osascript/System Events, the launched editor or terminal seen by process, output in the PR body, re-run by the merge-manager. The earlier `ready-for-human` checklist idea is superseded. macOS is the witnessed platform; others get command templates and no witness.
- URL resolver: exact match on normalised `web_url` (strip fragment, trailing slash, host case; keep the query — Confluence's identity is in it), new index; per-adapter parsers later if soak shows misses. Not-in-mirror → *Open in browser*, never a live fetch.
- Transitions read: one explicit trait method per adapter, no default impl (ADR-0006 spirit); select offers reachable statuses, falls back to corpus list; `WriteOp::Transition{status}` unchanged.
- Importers: Import dialog chooser; keychain namespace `importer:`; hcloud origin key `hcloud_id`, "land under" asked once per run; Docker via CLI per engine asset carrying `docker_context`, origin key context+name, containers land under that engine.
- Save-search-as-list needs a table + §10.8 entry + the share-export toggle; contract.md's "is M4" line is wrong and gets corrected.

**Why:** the whole design tree was settled in one session; the spec and tickets must transcribe it, not re-decide it.

**How to apply:** when writing or reviewing the v1.5 spec/tickets, check them against this list and the CONTEXT.md entries; anything that contradicts a line here is a regression of a ruling, not a new decision.

## knobas-v3-external-adapters-spec

_"v3 = external adapters + roles; spec #1192, milestone v3 (#22); every ticket cut from it gets label v3"_


2026-10-18 custom-sources grilling produced spec #1192 (milestone `v3`, GitHub milestone number 22), ADR-0028 (external adapter = sandboxed WASM component from a pinned release) and ADR-0029 (the shell draws a role, never a source), plus CONTEXT.md terms Role, External adapter, Adapter standard, Source action.

**Why:** Björn wants to add sources (GitHub, Forgejo, other CI/wiki/ticket systems) by pointing at a repo, with stated per-role requirements.

**How to apply:** every ticket `/mattpocock-skills:to-tickets` cuts from #1192 gets label `v3` and milestone `v3` (Björn's explicit request). Tickets #1193–#1210. Order: roles over built-ins → WASM host → SDK + `knobas adapter check` (uncut). **No GitHub adapter**: Björn 2026-10-18, "it was an example"; #1211–#1214 closed not planned. GitHub stays fine as an example and as where adapter releases are fetched from; never propose building a GitHub adapter. See [[milestones-in-order]].

## live-rerun-only-when-exercised

_Björn 2026-10-12 — a merge-manager re-runs a live suite only if its fixes or the rebase touched something that suite exercises; plus parallel review / serial merge tail and cargo fmt before every gate_


Björn, 2026-10-12, on how to shorten time-to-merge ("I want 4. aswell"): a merge-manager re-runs a live suite **only if something that suite exercises changed since the head the implementer's transcript was taken on**. That means its own fix commits or the rebase touching the adapter crate, `knobas-source`, `knobas-sync`, `knobas-http`, the seed/`testenv/` scripts, or app code an app-level live suite drives. Comments, docs, frontend and non-live test code do not trigger a re-run. A transcript whose head is not named is re-run. Recorded in `docs/agents/working-model.md` (PR #691).

Adopted the same day as orchestrator practice, with no objection from Björn:
- **Parallel review, serial merge tail.** Merge-managers review concurrently and stop at `READY TO MERGE #N`. The orchestrator resumes them one at a time for rebase → gate → merge.
- **`env -u RUSTUP_TOOLCHAIN cargo fmt --all` before every `just check`.** `just fmt` only checks. A third of the gate re-runs in the #660 wave were fmt-only failures.

**Why:** waiting on agents and the serial queues dominated wall time, not the gate (see [[knobas-gate-is-one-minute]]).
**How to apply:** put all three into the merge-manager and implementer brief copies at session start. Related: [[knobas-merge-manager-brief]], [[knobas-implementer-brief]], [[knobas-test-against-real-systems]].

## live-suite-is-a-shared-fixture-with-a-budget

_One live suite can spend a budget another suite asserts against; check before running a repeated live suite, and record the restore point first_


`crates/knobas-source-gitea/tests/live_gitea_capped.rs` sets `const HEADROOM: usize = 19` and asserts the seeded `payout-service`'s default-branch commit count and pull listing against it. `start_work_live` leaves **two commits and one permanent merged pull request per run** (its `Litter` only closes pull requests, and Gitea refuses to close a merged one). The recipe's own header claimed "nothing downstream reads that file or counts those commits" — false since #350.

Found 2026-09-04 when #358 wanted twenty consecutive runs and checked first: the fixture was sitting at exactly 19 of 19.

**Why:** the Docker environment is one shared fixture with no ownership between suites, so a budget declared in one file is spent by another that does not know it exists.

**How to apply:** before granting a repeated live run, make the agent verify what asserts on the fixture's counts and report the numbers. Have it record the restore point (default-branch head SHA, pull high-water mark, counts) *before* the first run, and verify the restore afterwards rather than assuming it. A targeted restore (force-push to the recorded SHA, `DELETE /issues/{index}`) is much cheaper than `testenv/reset`, which takes the TeamCity volumes with it. Note neither delete removes a merge commit from the default branch — only a force-push or a full reseed does. See [[knobas-test-against-real-systems]] and #373.

## local-gate-is-blind-to-timezone

_The frontend suite is pinned to Europe/Berlin (PR #408), so ambient TZ= does nothing; edit the pin to sample another zone_


Until 2026-09-05 `just check` ran in whatever zone the machine was in — Björn's is
`Europe/Berlin` — and no runner had executed the suite since 2026-08-29, so a
UTC-only failure could sit on `main` indefinitely. One did: issue #406, a test whose
expectation was `-new Date().getTimezoneOffset()`, which is `-0` at offset zero,
and `Object.is(+0, -0)` is false. Green in Berlin, red on every runner.

**Resolved by PR #408 (2026-09-05), and the resolution inverts the old advice:**
`app/vite.config.ts` sets `test.env = { TZ: "Europe/Berlin" }`, and
`app/src/lib/shell/test-setup.ts` throws if a worker starts in any other zone.

**Why:** the zone is now a decision rather than a property of whoever ran the
command. Björn chose Berlin over UTC deliberately: it is the zone the app is read
in, and its offset is positive, the sign the backend is given. The stated cost is
that no run sits at an offset of zero any more, so #406's own `-0` shape is not
sampled by accident by anything.

**How to apply:** `TZ=UTC npx vitest run` **no longer witnesses anything** — the
pin overrides the ambient variable, and a memory or brief that still recommends it
is stale. To sample another zone you must edit the `env` line in `vite.config.ts`
(and the guard's `PINNED_ZONE`) for the run. Vitest's `Start at` line still prints
in the ambient zone because the reporter is the main process, not a worker; do not
read that as the pin failing. The Rust half is unpinned on purpose — nothing under
`crates/` reads the local zone (`chrono::Local`, `Local::now`: no hits), since the
offset crosses IPC from the frontend. Still suspect any assertion that negates an
offset. Related: [[gate-is-local-not-ci]].

## loop-deputy-is-opus

_"Björn 2026-10-11: the ticket loop's deputy (rulings + triage) runs on Opus, never Fable — too expensive; /v15-next is now generic over every ready-for-agent issue"_


Björn, 2026-10-11: change `/v15-next` "into an implement step for all the next open tickets … that it doesn't use fable for decisions as it's too expensive."

- `.claude/commands/v15-next.md` (name kept, so `/loop /v15-next` still works) now loops over **every open `ready-for-agent` issue**, in any milestone or none. Issues with no `## Blocked by` section count as free. It stops when the tracker is empty.
- **The deputy runs on `model: "opus"`.** That supersedes the 2026-09-07 Fable-deputy rule (memory deleted) and the Fable triage in [[loop-triages-and-runs-to-empty]]. Every Agent call in the loop is Opus.
- **Björn's gates:** closing specs and milestones, tags and releases stay his, and the loop never does them. A frozen-contract (§10.8) change that the **ticket itself names** is already signed off by approving the ticket: for #594 he said "merge already gets my sign off, it's fine". Only an *unnamed* frozen-surface touch goes to the deputy.
- Rulings go to `docs/archive/2026-10-unattended-rulings.md`.

**Why:** Fable credit is too expensive for routine rulings.
**How to apply:** never pass `model: "fable"` in the loop; if an older note says the deputy must be Fable, this one wins.

## loop-drain-2026-10-16

_RETIRED 2026-10-16 ~16:30 — drain lifted when Björn typed /loop /ticket-loop in a fresh session; safe to delete this file_


Retired. Björn's 2026-10-16 drain ("no new agents for now") ended when he restarted `/loop /ticket-loop` after a context clear (~16:30). [[loop-token-budget]] still applies. This file can be deleted.

## loop-efficiency-rules

_"Björn 2026-10-13 adopted every efficiency suggestion from the #701 wave review — bundle overlapping tickets, pre-rule forks, split review axes, pre-gate, union merges, §10.8 one file per entry, fewer mutants, skip READY stop on empty queue"_


Björn, 2026-10-13: "I want to apply every suggestion you did." From the #701 wave timing review:
- Bundle tickets touching the same code into one ticket/agent; parallelise only non-overlapping ones.
- Rule foreseeable forks in the ticket body up front.
- Implementer: code review Standards axis only; merge-manager owns Spec + deep review.
- `just pre-gate` (fmt, inventory-update, fast table tests, svelte-check) before every `just check` (#736).
- `merge=union` for test-inventory.txt and the rulings log (#736); §10.8 entries one file each with a generated index (#737).
- Merge-manager skips READY TO MERGE when the queue is empty; a live suite green on the exact merged head is not re-run.
- 3–6 mutants on load-bearing logic, not 10–16.
- Live tests reuse the shared cleanup guard (`Litter`).
- Squash first when a rebase conflicts widely.
- Model: [[sonnet-for-small-tickets]]; at most three Opus agents at once.

**Why:** most wave time went to rebase conflicts, mechanical gate re-runs, double reviews and forks that waited on the orchestrator.
**How to apply:** put these in every brief copy at session start until #737 writes them into docs/agents/working-model.md; then that file is the source. Related: [[live-rerun-only-when-exercised]], [[knobas-pr-agent-workflow]].

## loop-speed-rules

_"Björn 2026-10-14 adopted all speed suggestions — diagnose perf before filing, measure small, follow-ups ready vs backlog label, smallest tickets self-merge, ≤15-line reports, set a measurable done, daily check-in, Opus cap 5"_


Björn, 2026-10-14: "I want all of these suggestions". The rules are written into docs/agents/working-model.md under "Loop speed rules" (PR #780), which is their source once merged. The brief copies in the scratchpad already carry them.
- His requests run beside the queue ([[user-requests-run-beside-the-queue]]).
- Every request gets a measurable "done". If he gives none, I set it in the ticket.
- Diagnose a perf ticket before filing it: up to 20 min of EXPLAIN on the probe database, in both statistics states.
- Measure on 2–3k items and run the full 22k once. #781 adds probe snapshots.
- A follow-up the user would notice gets `ready-for-agent`. An internal one gets the `backlog` label (created 2026-10-14). The orchestrator promotes backlog itself once the frontier is empty — see [[orchestrator-promotes-backlog]].
- Smallest tickets self-merge: Sonnet-sized, no frozen surface, no live suite, no visible behaviour change, a diff under about 150 lines. The dispatch says "self-merge".
- Agent reports run about 15 lines at most. PR bodies put plans and long tables in `<details>`.
- A daily check-in at the start of each of his sessions: what merged, what is running, the one question for him.
- Opus cap raised from 3 to 5. Gate slots stay at 2 (SysV shm; raising shmmni is still not wanted, #425).

**Why:** the overnight run spent its time on a wrong-premise ticket, full-size measurement runs and a self-refilling follow-up chain.

**How to apply:** check these rules at every dispatch and filing. Supersedes the Opus cap in [[loop-efficiency-rules]].

## loop-token-budget

_"Björn 2026-10-16 — slow the loop to stay under the 5-hour usage limit: 3 agents max (a 3rd implementer may take the seat when no PR waits), Sonnet merge-managers, no routine splitters"_


Björn, 2026-10-16 about 13:50: "slow down, can we somehow make sure we don't hit the token limit? at least not the 5 hour limit?"

Measured that day: Opus implementers used 200k–450k subagent tokens each, merge-managers 50k–125k, splitters about 45k. Eight merges in about three hours at 4–5 seats.

**Ruling (overrides the ticket-loop's ceiling defaults and [[knobas-concurrency-ceiling-raised]]):**
- At most **3 agents** at once.
- **Amended 2026-10-16 ~21:20** (Björn asked "why's there only 2 agents", picked option 1): the 3rd seat is not reserved for merge-managers. When no PR is waiting for review, a 3rd implementer may take it; a PR that opens then gets its merge-manager at the next free seat (it waits, it does not make a 4th agent). Previously: at most 2 implementers.
- Merge-managers on **Sonnet** unless the diff touches a frozen surface (§10.8 IPC, migrations, write queue, source adapters, concurrency).
- Opus only for implementers of UI screens and tracers; Sonnet for everything mechanical.
- **No splitter agent** when the routes are plainly tabs or sub-routes of one screen. The orchestrator rules from the route list and prototype-6's render function; a splitter only when genuinely unclear.
- Wake-ups 1800 s; short iteration reports.

**How to apply:** check this before every dispatch. A waiting PR still takes the next free seat before a new implementer (ticket-loop Step 2 order). The orchestrator cannot read the usage meter; Björn sees it with `/usage`. Raise the ceiling again only when he says so. Related: [[agent-cost-is-context-times-turns]].

## loop-triages-and-runs-to-empty

_"Björn 2026-09-11: the ticket loop triages its own findings with a Fable deputy and does not stop at a milestone exit — it stops when no issues are left"_


Björn, 2026-09-11, to the running `/loop /v15-next` session: *"you also triage with fable on your own if necessary. only stop once there's no issues left."*

Two changes to how the loop runs:

1. **The loop triages.** A ticket the loop itself files mid-batch — the #554/#560/#561/#564 class — no longer waits for Björn to place it. Dispatch a **Fable** deputy (never Opus: see [[fable-credit-exhaustion-substitute]], whose Opus substitute is for merge-managers only) to rule placement, labels, running order and caps, and to apply them with `gh issue edit`. One ruling comment per issue, the four-part shape.
2. **The stopping condition moved.** Not "the milestone is exited" but "the tracker is empty". A batch exit that leaves `ready-for-agent` or `needs-triage` tickets behind is not a stop — triage them and keep dispatching.

**Why:** the loop was filing real defects and parking them, so an unattended run drained one milestone and silently grew a backlog nobody had agreed to. Björn would rather the deputy place them and the loop work them off than come back to a queue.

**How to apply:** keep filing findings outside the batch rather than widening it — that part was right — then triage them in the same session. A deputy counts against the three-agent ceiling only while it runs, which is minutes. See [[loop-deputy-is-opus]], [[knobas-m4-next-loop]], [[knobas-concurrency-ceiling-raised]].

## milestone-release-mechanics

_"How the loop cut v2.0.0-m1/m2 (2026-10-17) — release PR self-merge, tag, release.yml leaves a draft, publish as pre-release not latest; MSI version mN = 2.0.(N-1), final v2.0.0 needs MSI > last milestone"_


Cut v2.0.0-m1 (TF1, tag on 18cf5539, PR #1035) and v2.0.0-m2 (TF2, tag on bb73b9ad, PR #1041) on 2026-10-17 overnight.

Steps that worked:
1. Release PR as a Sonnet **self-merge**: bump `Cargo.toml` workspace version, `Cargo.lock`, `crates/knobas-app/tauri.conf.json` `version`; full `just check` on the merged head; body `Part of #<exit>`; never batch it.
2. Orchestrator: annotated tag on the merge commit (`git tag -a -F <file>`), push, `gh workflow run release.yml --ref main -f tag=… -f platforms=all`, background `gh run watch --exit-status`.
3. `release.yml` creates a **draft** with unsigned-bundle notes; afterwards `gh release edit --draft=false --prerelease --latest=false --notes-file <milestone summary + original notes>`.
4. Close the milestone (`gh api -X PATCH repos/…/milestones/<n> -f state=closed`) and the exit issue with the release link.

**MSI rule (#658 + PR #1041 ruling):** `bundle.windows.wix.version` is numeric only and must rise for Windows Installer to upgrade: mN ships as `2.0.(N-1)` (m1 2.0.0, m2 2.0.1, m3 2.0.2 …). The final v2.0.0 must set the MSI version above the last milestone's (e.g. 2.1.0). 2026-10-18: v2.0.0-rc1 cut the same way (PR #1184, tag on becb70da, no milestone/exit) with MSI 2.0.6, so the final v2.0.0 needs MSI > 2.0.6. **Final v2.0.0 cut 2026-10-18** (PR #1221, tag on c6877a62, MSI 2.1.0, run 39161059247 green, published as latest, not pre-release). The next release's MSI must exceed 2.1.0.

`package.json` stays 0.1.0 (never bumped). Related: [[release-per-milestone]], [[milestones-in-order]].

## milestones-in-order

_"Björn 2026-10-16 ~22:15 — dispatch by milestone order first (TF1, TF2, TF3 …), issue number second, so a milestone closes and releases something he can test"_


Björn, 2026-10-16 ~22:15: "if possible, try to prioritize milestones in order, tf1 tf2 etc. so i can actually have something to test".

**Why:** the loop's default order (issue number across all milestones) spread work over TF3 and TF5 while TF1 and TF2 sat with 3 and 7 small tickets open, so no milestone reached its exit review and pre-release.

**How to apply:** in ticket-loop Step 4, sort free candidates by milestone (TF1 < TF2 < … < no milestone), then issue number. Loop tickets and live-fixture fixes still go first. A milestone exit is run as soon as it is free; a release is what he tests. Running agents on later milestones finish normally; nothing is pre-empted. Bundling and same-screen rules still apply within the order. Overrides the ticket-loop file's "order is by issue number". Related: [[release-per-milestone]], [[loop-token-budget]].

## mutation-check-committed-baseline

_commit before mutation-testing — git checkout restore silently wipes uncommitted fixes_


Mutation-check only against a committed baseline. The restore step (`git checkout <file>`) reverts to HEAD, so any uncommitted work in that file is silently destroyed along with the mutant.

**Why:** In #47 (2026-08-29) a second mutation round ran after review fixes but before committing them; the restore wiped three uncommitted fixes in `context.rs`, caught only because the test suite went red on a struct the tests referenced.

**How to apply:** Before the first mutation of a round: `git status --short` must be clean (commit first). This is the same rule as the working model's "commit per task, always", applied to the mutate/restore loop. Related: [[subagent-findings-need-quotes]].

## no-permission-prompts

_"Björn runs the loop unattended in bypass mode; never issue commands that still prompt (cd in compound commands, Bash writes under ~/.claude or .git)"_


Björn, 2026-10-16: "please stop asking me to approve commands, I didn't turn on dangerous mode for fun. I'm not at the computer 24/7."

**Why:** bypass mode is on, but a few things still prompt, and a prompt stalls the unattended loop for hours. That session hit two: `rm`/`mv` of a memory file via Bash under `~/.claude/`, and `cd .worktrees/issue-851; …` in a compound command.

**How to apply:**
- Never chain `cd` into a compound command; use absolute paths and `git -C`.
- Change memory files only with Write/Edit, never Bash `rm`/`mv`/`sed -i` under `~/.claude/`. To retire a memory, empty it via Write or ask; don't `rm` it.
- Nothing under `.git/` by hand.
- **Second complaint, ~22:25 the same day** ("it keeps asking me for permission … I run in permissive mode for a reason"). Agents' quick commands sat 1–3 min waiting: `cd $W; cat >> file <<'EOF'`, `sed -i ''` with `\n`, `rm -f $D/*.png`, `(cd $W && just …)`. So the subshell `( cd app && … )` form the briefs allowed is not safe either. v2 rules: no `cd` at all (use `git -C`, `cargo --manifest-path`, `just -f … --working-directory …`, `npm --prefix … exec -- vitest`); no `sed -i`/heredoc/redirect writes into the repo (Edit/Write tools); no `rm` with globs or `-r`; commit messages via `git commit -F <file>`, never multi-line `-m`. Full text: the session's `no-prompt-rules-v2.md`, appended to both brief copies; the masters still need it.
- Detect: in a subagent jsonl, a cheap tool_use whose tool_result arrives >45 s later is a likely prompt.
- Put the same rules in every agent brief (the session copies of [[knobas-implementer-brief]] and [[knobas-merge-manager-brief]] got a "No permission prompts" section on 2026-10-16; add it to the masters too).

## node-modules-symlink-wipes-root

_"A worktree whose app/node_modules is a symlink to the root's: removing the worktree empties the root's real node_modules (2026-10-17, #891)"_


2026-10-17 16:35: the #891 self-merge agent symlinked `/Users/dev/Projects/knobas/app/node_modules` into its worktree to make `pre-gate` resolve modules. Its cleanup (`git worktree remove`, plus a failed `rm` of the link) left the root's `app/node_modules` an empty directory. The next agent found nothing to `cp -Rpc` and ran `npm ci` itself; the orchestrator restored the root with `npm --prefix /Users/dev/Projects/knobas/app ci`.

**Why:** every new worktree clones the root's node_modules; an empty root silently breaks all later frontend gates.

**How to apply:** briefs say copy (`cp -Rpc`) or `npm --prefix <wt>/app ci`, never symlink (added to the session brief copies; add to the masters [[knobas-implementer-brief]] and [[knobas-merge-manager-brief]]). If an agent reports an empty root node_modules, reinstall the root with `npm --prefix … ci` before the next dispatch.

## orbstack-port-forward-dies-after-recreate

_"OrbStack's 127.0.0.1:3000 forward to knobas-gitea went dead after gitea-live-capped recreated the container; restart and a second recreate did not heal it, compose stop + up did; the seeded TeamCity build then fails "Unable to collect changes" through the tunnel's reverse forward"_


2026-09-06, first live table against the Hetzner servers (head 14d4e24): `gitea-live` passed, then `gitea-live-capped` recreated `knobas-gitea` under the capped overlay and the host port 127.0.0.1:3000 stopped answering (curl "Empty reply from server", `000`) while `docker exec` inside the container got 200 and the healthcheck said healthy. `docker restart` and the recipe's own uncapping recreate left it dead; `docker compose stop gitea && docker compose up -d --wait gitea` brought it back. Second pass of the same three recipes: green, port fine after each. Not reproduced, so a transient OrbStack (2.2.3) forwarder fault, not the recipe.

Knock-on: `start-work-live` failed with curl 52 and `teamcity-live-seeded` failed one test with build 414 `FAILURE "Unable to collect changes"`, because the TeamCity server clones Gitea through the tunnel's reverse forward into that same dead port. Neither is a code failure.

Also on this machine: the repo-root `.env` (edited 2026-09-06 08:55 CEST) sets `KNOBAS_TEAMCITY_URL=https://teamcity.jetbrains.com` without `/guestAuth` plus a token, so `just teamcity-live` gets 401 on 11 of 12 tests. The suite is green run directly with `KNOBAS_TEAMCITY_URL=https://teamcity.jetbrains.com/guestAuth KNOBAS_TEAMCITY_TOKEN=` as `.env.example` has it.

**Why:** a red live table after the Hetzner move looked like four defects and was one dead port plus one `.env` drift.

**How to apply:** when a Gitea-side live recipe fails with an empty reply or a TeamCity build says "Unable to collect changes", curl 127.0.0.1:3000 from the host and from inside the container before reading any code; heal with compose stop + up, not restart. Check `.env`'s TeamCity URL against `.env.example` before calling `just teamcity-live` red. See [[knobas-real-products-on-hetzner]] and [[gate-is-local-not-ci]].

## orchestrator-promotes-backlog

_"Björn 2026-10-17: the loop promotes and dispatches backlog issues itself when no milestone/ready ticket is free; never leave seats empty waiting for him to promote"_


Björn, 2026-10-17, after I left seats empty and asked him to promote backlog: "why should I promote them, I never said I would do it myself. I only ever said you should prioritize milestone tickets … write it into the loop … that you promote them accordingly yourself."

**Why:** "backlog, not dispatched until promoted" was read as "Björn promotes"; he only ever ruled an order (milestones first), not a gate.

**How to apply:** dispatch order is milestone tickets (TF1 < TF2 …, then issue number), then `ready-for-agent` with no milestone, then promote open `backlog` issues oldest first (flaky-gate and `Live fixture:` ones ahead, per the loop's existing priorities): relabel `backlog` → `ready-for-agent`, one-line comment, dispatch. Close a backlog issue that a merge overtook. Never ask him to promote. The order lives in GitHub milestone **Backlog** (#21): its description ranks every backlog issue (gate/live reliability → lost or wrong data → visible polish → speed → declarations/seeds → tidy or measure-then-close); follow it top-down, and place new backlog issues into it where they fit (he asked 2026-10-17: "read the tickets and prioritize as to what makes sense"). Written into `.claude/commands/ticket-loop.md` Step 4 (PR #1106) and the session brief copies. Related: [[milestones-in-order]], [[loop-triages-and-runs-to-empty]], [[decide-dont-park-for-triage]].

## perf-ticket-needs-machine-hold

_"A ticket whose criteria take #533-style quiet perf runs needs the orchestrator to hold the machine — dispatch nothing beside it"_


When a ticket's criteria require `just search-perf` runs under #533's conditions ("whole machine for the run", "the dispatcher must not start a second gate"), the loop orchestrator must hold the machine: no implementer, merge-manager or review with gates/cargo tests runs beside it.

**Why:** 2026-10-12, #629 was dispatched while PR #630's review ran cargo tests; one perf run was contaminated and condition (a) was unmet. A deputy admitted the runs only because the verdict was "not reproduced" (load only adds time) and said a red or thin reading taken that way would not count — the departure was the orchestrator's.

**How to apply:** treat a perf-measuring ticket like the desktop-witness cap: dispatch it alone, and hold other merges/dispatches until its runs are recorded. Related: [[knobas-concurrency-ceiling-raised]], [[sysv-shm-caps-concurrent-gates]].

## pgrep-watch-matches-itself

_"A `pgrep -f '<recipe>'` watcher can match its own shell and report a run that never happened; key the watch on the log file and the recipe's completion line instead"_


The orchestrator pattern for live recipes is: the agent launches the recipe in the background and stands
down, and the orchestrator watches the process and resumes the agent with the exit status. The obvious
watch is

```sh
until pgrep -f 'just atlassian-live' >/dev/null; do sleep 20; done
pid=$(pgrep -f 'just atlassian-live' | head -1)
while kill -0 $pid; do sleep 30; done
```

**`pgrep -f` matches any process whose full command line contains the string — including the watcher's
own shell, whose `-c` argument quotes the pattern.** On 2026-09-04 this reported `atlassian-live (#399)
started, pid 20614` and then an exit 90 seconds later, while no log file had been created, no Atlassian
container had ever existed, and the agent had not reached its live step at all. Acting on that would
have meant resuming an agent with a fabricated result.

**Why it matters:** the resumed agent takes the orchestrator's word for the run's outcome and writes it
into a PR body. A watch that can report a phantom run is the same class as any other instrument that
lies — it fails *green*.

**How to apply:** key the watch on artifacts the recipe itself produces, not on a process name. Wait for
the log file to appear, then for the recipe's own terminal line (`atlassian-live: pair torn down`, or the
`RECIPE EXIT=` line a driver script appends). Have the agent's launcher append its exit code to the log
so the watch reads a status rather than inferring one. If a process check is unavoidable, verify the pid
with `ps -p <pid> -o command=` before trusting it, and treat "no log file" as proof the run did not
happen. See [[subagent-background-gate-stall]], [[knobas-merge-manager-brief]].

## pin-of-markup-is-not-the-accessible-name

_A gate pin of a label's spelling in the source says nothing about what the accessibility API answers; knobas' desktop witness produced this class three times._


Pinning what the markup says is not measuring what the tree answers. `just witness-unit`
pinned the *Checkout* heading against the file that carries it — 55 checks, all green —
while `just desktop-witness open-in-editor` refused because no element in the AX tree
carried `CHECKOUT` as `AXTitle` or `AXDescription`, the only two attributes `ax.swift`'s
`matching()` reads. A `.lab` heading rendered as `AXStaticText` carries its text in
`AXValue`. Three instances of the class in one harness (2026-09-08, #547/#548): the
uppercased markup names #536 fixed, this pin, and `exactly_one 'Capture'` counting three
correct elements.

**Why:** the driver is the witness, so a check that measures a representation of the
label rather than the label the OS serves lets a red run look like a defect in the
feature at the one moment the witness matters — and pushes the next agent toward
changing correct app code to satisfy a broken check (the [[acceptance-criterion-that-cannot-pass]]
failure mode).

**How to apply:** when a driver waits on a name, require the two-direction showing on the
same tree — false before the step, true after — from the dump, not from the source. Never
add an `aria-label` so a driver can find something: fix the driver, per the deputy's
ruling of 2026-09-08 on #525. Related: [[a-stub-can-be-wrong-about-the-tool]],
[[knobas-test-against-real-systems]].

## release-per-milestone

_Björn 2026-10-15 — a TFn milestone closes on its own when all tickets are done AND an independent milestone review agent approves; then pre-release v2.0.0-mN_


A ticket-first milestone TFn closes automatically, without Björn, once all its tickets are done and an **independent milestone review agent** (one that implemented none of the milestone's tickets) says it's OK. Gaps the review finds become new tickets in the milestone, and the exit waits for them. Then cut a release `v2.0.0-mN` (N = the TF number): bump the version, tag, dispatch `release.yml` with platforms=all, mark it a pre-release. Each milestone has one *Milestone exit* ticket (review, close, release) blocked by all its other tickets.

**Why:** Björn asked for it on 2026-10-15 when spec #808 was ticketed. It overrides the older "Björn keeps milestone/tag/release" rule in [[loop-deputy-is-opus]] for these milestones.

**How to apply:** the loop picks up the release ticket like any other once its blockers close. Related: [[knobas-ticket-first-spec]].

## reserved-steps-ruled-by-opus

_Björn 2026-10-18 — nothing a ticket reserves for him waits for him; an Opus implementer or merge-manager rules it (never Sonnet); the loop also closes specs with no work left_


Björn, 2026-10-18: "Why are you waiting on me, I am not there" then "Approved and write into the skill and everything that the implementer can make his own decision (except sonnet)". PR #1218 (#1187's frozen-contract amendment, whose criteria said "Sign-off from Björn before merge") had sat six hours of hourly no-op wake-ups.

**Rule:** a step a ticket, milestone or spec reserves for Björn (contract sign-off, approval, a choice) is ruled by an **Opus** agent: the Opus implementer, else an Opus merge-manager. A Sonnet agent never rules one; it reports it, and the orchestrator dispatches an Opus merge-manager. Recorded under **Rulings** ("Reserved for Björn, ruled by <role> on Opus") and in `docs/decisions/<yyyy-mm>-unattended-rulings.md`. Same evening Björn added: "You can also close specs if no work is left on it", so nothing stays his: the orchestrator closes a spec once every sub-issue and `## Parent` ticket is closed and no open PR is on them (after its milestone exit, if any), with a comment listing tickets and PRs (*Step 4b*, PR #1220). Written into `.claude/commands/ticket-loop.md` *Decisions* (PR #1219).

**Why:** he runs the loop unattended; a reserved step parks the whole frontier (here the v2.0.0 release and all of v3) until he happens to look.

**How to apply:** never schedule hold-only wake-ups on a Björn sign-off again; dispatch Opus to rule it. Put the rule in every implementer and merge-manager dispatch. Extends [[decide-dont-park-for-triage]] and [[implementer-decides-forks]]; overrides the "reserved for Björn" clause in [[ticket-loop-is-the-loop]].

## seed-refuses-without-hosts-env

_"Björn's 2026-09-06 ruling that testenv/seed hard-refuses without hetzner/hosts.env; do not soften to skip-with-warning"_


`testenv/seed` exits with an error when `testenv/hetzner/hosts.env` is missing (PR #457, issue #441). Both review axes proposed a skip-the-pings-with-a-warning path; Björn ruled on 2026-09-06 to keep the hard refusal.

**Why:** the seed makes the live Kuma a witness, and a Kuma running with three of eight monitors silently missing is the "green while measuring less than it claims" class. Same reasoning as #351, where every live recipe was made to refuse without its gate variables. The hosts file is this recipe's gate variable.

**How to apply:** if a reviewer or implementer proposes softening the refusal, decline and cite this ruling. A Hetzner-free seed, if ever wanted, is an explicit flag in a follow-up ticket, not an absent file. See [[knobas-test-against-real-systems]] and [[knobas-real-products-on-hetzner]].

## sonnet-for-small-tickets

_"Björn 2026-10-13 — small, mechanical tickets (and their merge-managers) run on Sonnet; Opus stays for tracers, frozen-surface and UI-heavy work"_


Björn, 2026-10-13: "I like your suggestions for the small model already, use sonnet for that yes." Small, mechanical tickets (a narrow sync fix like #728, a query correction like #718, test-infra chores) get `model: sonnet` for the implementer and the merge-manager. Opus stays for tracers, anything changing a frozen surface (§10.8, SPI vocabulary, ADRs), and UI-heavy tickets.

**Why:** the #701 wave spent most agent time on Opus for work that did not need it, and three Opus agents were killed mid-run by the usage limit.
**How to apply:** decide the model per ticket when dispatching; say which and why in the dispatch. Supersedes the all-Opus default of [[loop-deputy-is-opus]] for small tickets only (the deputy itself stays Opus). Related: [[knobas-pr-agent-workflow]].

## stop-finished-agents

_"Björn wants finished background agents removed; a completed implementer stays registered until TaskStop, merge-managers clear themselves"_


After a subagent's completion notification arrives, call TaskStop on its id. Björn asked for this on 2026-09-07 during the M4 loop ("make sure to remove finished agents please").

**Why:** finished implementers stayed listed as running background agents after reporting (two of six lingered); they clutter the task view and can re-notify. Merge-managers in that run had already cleared on their own, so a "no task found" error on stop is fine.

**How to apply:** in the orchestrator loop, stop each agent right after handling its report, in the same turn. Related: [[knobas-m4-next-loop]].

## subagent-background-gate-stall

_"Subagents end their turn while `just check` runs in background; resume them, or pre-empt with "run the gate in the FOREGROUND" in the brief"_


During the 2026-08-31 comment-sweep batch (#191–#198), four of seven implementer subagents ended their run with "waiting for the gate to finish" while their backgrounded `just check` was still compiling — the orchestrator had to resume each one before it would push and open its PR.

**Why:** an agent that backgrounds its final gate has nothing left to do and stops; its completion notification then reads like progress but the deliverable (PR) never happened. The [[knobas-pr-agent-workflow]] loop stalls silently at the last step.

**How to apply:** put "run the full `just check` exactly once, in the FOREGROUND — do not end your run while it is going" in every implementer brief (added mid-batch; the later agents still slipped once). When it happens anyway: check the worktree's branch is pushed/clean, arm a background watcher — `until ! lsof -a -d cwd -c cargo -c just -c rustc -c clippy-driver -c npm -c node | grep -q <worktree-name>; do sleep 20; done` — and SendMessage-resume the agent when it fires, telling it the gate has exited and to read the result and open the PR.

**Addendum 2026-09-02 (#264):** the opposite failure also happens: an implementer that arms its own
monitor on the gate log wakes itself every ~20 s to re-report "still green, waiting", each wake a
task-notification and a turn of its budget (seven in a row on #264). Fix: one SendMessage telling it
to kill its monitor and end its turn, then a watcher of your own on the gate *process*
(`while kill -0 <pid>; do sleep 10; done` via Bash run_in_background) and a single resume with the
exit status. Brief implementers up front: the gate runs foreground in one call, or if it must be
backgrounded, do not poll it; the orchestrator resumes you.

## subagent-findings-need-quotes

_"A subagent finding that quotes no artifact is a hypothesis; verify before acting, especially before writing to a shared surface"_


A review subagent reasons about neighbouring artifacts it was never given — the follow-up
ticket, the parent spec, the file it only heard about. Those inferences arrive in the same
list as findings it genuinely verified, and the only thing separating them is that they
quote nothing, because there was nothing to quote.

**Why:** on knobas #50 (2026-08-28) the Spec-axis agent was briefed on #50 and #40 and
recommended adding an acceptance criterion to #52. I posted it to #52. #52 already carried
that criterion word for word; nobody had opened it. Every real finding in the same report
quoted its source. The tell was there and I read past it.

**How to apply:** before acting on a finding, ask what it quoted. If nothing, it is a
hypothesis — go read the artifact, and *always* read before writing to a shared surface
(`gh issue view` before `gh issue comment`/`edit`/`close`; the harness gates Read-before-Edit
on files but nothing gates the issue tracker). When dispatching, hand the agent every ticket
it may need to opine on, since it will form opinions either way. Scope the caution to
outward writes — a merge, a force-push, a comment — not to every inference.

Codified for this repo in `docs/agents/issue-tracker.md` (PR #57). Related: the ticket flow
in [[knobas-pr-agent-workflow]].

## sysv-shm-caps-concurrent-gates

_macOS kern.sysv.shmmni=32 caps concurrent `just check` runs at 2 safely, 3 at a stretch, never 5; one gate peaks at 13 SysV segments (gate server + lifecycle suite's transient postmasters); leaked segments need `ipcrm`_


Measured 2026-09-05 (#422 experiment, five warm worktrees): 1 gate = 70 s, peak 13 shm segments;
3 gates = 168 s each, all green, peak 17 segments, load 65; 5 gates = all five red after 454 s with
`shmget: No space left on device` in every worktree's `knobas-db` lifecycle tests. Every postmaster
holds one SysV segment; `kern.sysv.shmmni` is 32 and `sysctl -w` cannot raise it at runtime on macOS.

**Why:** the #249 false-red class is exactly this limit. The gate server (#418) removed the 61
per-binary postmasters, but `knobas-db`'s `tests/embedded.rs` still starts up to 12 real servers
per gate, so 5 × 13 > 32. CPU is the second wall: three concurrent gates take 2.4× the single
wall, so concurrency buys almost nothing once the gate itself is parallel.

**How to apply:** never run more than two gates at once; before a wave, `ipcs -m | grep -c '^m'`
and `pgrep -f 'postgres -D'` should both be 0, and after killing a run, `ipcrm -m <id>` the
segments owned by dev with no live postmaster (15 were left after one kill). A concurrent warm-up
of several cold worktrees is its own overload (18 min at load 57); warm them serially. A
`pgrep -fc` on macOS is invalid (`-c` is not a flag): use `pgrep -f ... | wc -l`. Björn's
amendment of the working model is #422 (ready-for-human). Related: [[knobas-gate-is-one-minute]],
[[knobas-concurrency-ceiling-raised]].

## ticket-loop-is-the-loop

_Since 2026-10-15 the loop is `/loop /ticket-loop` (general, tracker-driven); v15-next/m4-next are superseded_


`.claude/commands/ticket-loop.md` (written 2026-10-15) replaces `/v15-next` and `/m4-next`. It is deliberately general: rules come from ticket body > milestone description > parent spec > working-model/ADRs/memory rulings > the file's defaults. A new milestone with new rules needs a milestone description, not an edit to the loop.

**Why:** Björn asked for a loop he doesn't have to change all the time, after the ticket-first spec #808 brought in [[implementer-decides-forks]] and [[release-per-milestone]].

**How to apply:** no deputy any more. Workers rule their own forks and record them in the PR and in `docs/decisions/<yyyy-mm>-unattended-rulings.md`. A "Milestone exit …" ticket gets an independent reviewer, its gaps as blockers, then a release PR, and the orchestrator tags, dispatches `release.yml` and closes the milestone. A criterion no agent can witness is passed to the milestone review. Supersedes the deputy parts of [[loop-deputy-is-opus]] and [[knobas-m4-next-loop]].

**2026-10-15 amendment (Björn: "I want these suggestions applied to the current skill"):** five seats, Sonnet for small mechanical tickets and their merge-managers, implementer runs Standards axis only and 3–6 mutants, merge-manager runs Spec + deep and gates once on the final head (skips it when the PR's recorded green head is unchanged), smallest tickets self-merge, live-fixture fixes jump the issue-number order and are named in every dispatch. The file is globally gitignored; Björn had it force-added 2026-10-15 (PR #902), so edits now go through PRs like other tracked files (`git add -f`).

**Second 2026-10-15 amendment:** reviews run in parallel (≤2 merge-managers), only the merge tail (rebase, gate, merge, cleanup) is serial under `$TMPDIR/knobas-merge-tail.lock` (a per-session lock script in the scratchpad); same-screen tickets are bundled (≤3, one PR); search/hot-path dispatches quote "measure on 2–3k, full 22k probe once"; disk guard before a new worktree (20 GB + 12 GB per ungated worktree). Shared build cache was checked and not adopted: sccache already serves deps since 2026-10-13 (#743), workspace crates miss on any new path, and a shared target dir would serialise builds on cargo's lock.

**Third 2026-10-15 amendment (Björn):** at most four implementers (five agents total); a per-session throughput log decides future seat changes. `Loop:` tickets (#905–#908 filed that day: target-dir size, a11y in pre-gate, new-shell conventions file, fake-IPC/App.svelte split) have the utmost priority, "all the performance hangs on them": they take the next free seat before merge-managers, no non-loop implementer starts while one is free or running, and a timing-measurement loop ticket holds the machine. Docs-only tickets self-merge without a gate. Finding: `[profile.test]` already has `debug = 0`; a worktree's 13 GB target is 7.1 GB `incremental` + 5 GB `deps`.

## to-spec-skill-user-only

_"/mattpocock-skills:to-spec (and to-tickets, same family as triage) refuse Skill-tool invocation; Björn must type them"_


`/mattpocock-skills:to-spec` is `disable-model-invocation`: the Skill tool returns "cannot be used with Skill tool… Do not replicate this skill's workflow by other means". Same class as [[triage-skill-user-only]]; `to-tickets` sits in the same directory and should be assumed the same until proven otherwise.

**Why:** 2026-09-07, after the v1.5 grilling Björn said "use the /to-spec skill and fable to do it"; the call was refused mid-turn.

**How to apply:** when the flow reaches spec or tickets, finish the parts this session owns (CONTEXT.md entries, ADRs, on a branch) and then ask Björn to type the skill himself. Do not draft a spec issue by hand as a substitute.

## triage-brief-fixture-claims

_"A triage brief that says "the fixture already holds X" must be checked against the rule that reads X, not just the data rows; #237's brief was wrong"_


On 2026-09-02 the #237 triage brief stated that the `?fake-ipc` corpus "already has the pairs" for
an `exact_key` suggestion (PR/commit/build on `feat/idempotent-retry` beside PAY-231). The triage
had read the corpus rows but not the detector's rules: `branch_name_key` reads only `kind = 'branch'`
entities (the corpus has none) and the branch string carried no key. The implementer had to edit the
corpus and the merge-manager flagged the brief's claim back to the orchestrator.

**Why:** a brief is the contract; a false fact in it either gets implemented as-is (a fixture whose
reasons are untrue of its data) or costs the implementer a re-diagnosis. Same class as
[[subagent-findings-need-quotes]]: reading the data is not reading the rule that consumes it.

**How to apply:** before a brief claims a fixture can witness or already holds something, open the
code that would read it (the detector, the macro, the filter) and trace one row through it. State
the rule you traced in the brief. If it cannot be traced cheaply, write "verify: …" rather than
asserting. Related: [[witness-gap-ask-which-direction]].

## triage-skill-user-only

_/mattpocock-skills:triage is disable-model-invocation; the Skill tool refuses it and tells the agent not to replicate its workflow — Björn has to type it himself_


`mattpocock-skills:triage` cannot be invoked through the Skill tool (2026-09-02: "cannot be used with Skill tool due to disable-model-invocation … Do not replicate this skill's workflow by other means"). The other mattpocock skills (tdd, code-review, grilling, domain-modeling) invoke fine.

**Why:** the skill is reserved for explicit user invocation; a dispatch that says "run /mattpocock-skills:triage" as step one silently loses that step.

**How to apply:** when a run brief asks for triage first, do the tracker read with `gh issue list` directly and tell Björn the skill itself needs a `/mattpocock-skills:triage` from his prompt; do not hand-roll its workflow. See [[knobas-pr-agent-workflow]].

## user-requests-run-beside-the-queue

_"Björn 2026-10-14 — his direct request must not wait behind review follow-ups; run it in parallel if it doesn't overlap; perf tickets measure small, full corpus once"_


On 2026-10-14 Björn asked: "what did everything take so long for? this ran over the night and still not done?" He had asked, on 2026-10-13, to work on perceived quickness "when everything is done". I read that as "after the whole queue". Review follow-ups (#770 → #771 → #774) kept refilling the queue, so his request (#775) only started around 08:00. It touches no suggestion code and could have run alongside from midnight on.

**Why:** a direct request from him outranks the loop's own follow-ups. "When everything is done" means the work in flight at that moment, not every future follow-up.

**How to apply:**
- Start his direct request as soon as it doesn't overlap the code in flight, within the 3-Opus cap.
- A follow-up from a review never jumps ahead of it.
- Perf or probe tickets: iterate on a small dataset, and do the full 22k-item probe run once at the end, before and after.

Related: [[decide-dont-park-for-triage]], [[loop-efficiency-rules]], [[perf-ticket-needs-machine-hold]].

## witness-gap-ask-which-direction

_"M2.6 hit "a fixture that cannot witness the thing" four times, each caught only by mutating the wire between two well-tested endpoints; the sharper question for tickets and reviewers is which direction is unwitnessed, since a sibling PR can land a guard on one side and make a ticket's diagnosis stale"_


The failure class `docs/agents/working-model.md` names — a check that measures a representation of
the thing instead of the thing — showed up **four times in M2.6** (2026-08-31 to 2026-09-01), and
every one was found by deleting a wire and watching the suite stay green:

1. #209 mutant 8: no fixture item could tell a narrowed Docs tile from an unnarrowed one.
2. #209's rendered project-room corpus was single-source, so dropping `sources` from the filter
   rendered identically (found by the merge-manager, fixed with a second source sharing the key).
3. #210: hardcoding `Room.svelte` to `miniBoardLayout="columns"` killed 0 of 742 — both named
   seams were green, the wire between them was blind.
4. The exit sweep: deleting `projects.all` at `App.svelte:51` removed every project room and
   killed 0 of 743. A `= []` default made it type-check; every App-mounting test stubbed
   `listProjects` to `[]`. Filed as #238.

**The pattern:** both endpoints were thoroughly tested every time. The gap was always the wire, and
always hidden by a fixture too small to witness the difference (one source, one column, one
project, an empty census).

**The second lesson, from #238 itself:** the ticket claimed "move the mock's project one level and
every existing test still passes." That was true when drafted and false by implementation time,
because #236 had landed a fixture-side guard in between. The real gap ran the *other* way — the
read disagreeing with the fixture. Twice in the milestone a fixture-side guard was assumed absent
when present.

**How to apply:**
- Brief every implementer and merge-manager: "ask whether your fixture can witness what you
  claim; a board of one column cannot witness a six-column backstop." Then name the mutants.
- When a ticket asserts a gap, the implementer re-verifies the diagnosis on the current tree
  before fixing it, and says so on the PR if the ticket was overtaken. The #238 implementer did
  exactly this and its merge-manager verified the correction rather than accepting it.
- Ask which **direction** is unwitnessed, not whether the seam is covered.
- A milestone-exit sweep framed as "what only shows when the PRs are read together" is what found
  instance 4; the per-PR reviewers could only see inside their own diffs.

Related: [[subagent-findings-need-quotes]], [[mutation-check-committed-baseline]].

## worktree-reset-against-moving-origin

_In a multi-worktree repo, sibling agents' `git fetch` silently advance refs/remotes/origin/main for everyone; `git reset --soft origin/main` then stages a neighbour's merged diff as a revert_


Found by the #303 implementer (2026-09-03): worktrees share one `.git`, so another agent's `git fetch` moved `origin/main` under it; `git reset --soft origin/main` (meant to squash two commits) staged the whole of #279's just-merged diff as a revert. Recovered with `git reset --mixed <sha>`.

**Why:** with five to seven agents in parallel worktrees, `origin/main` is a moving target between any two commands; any command that names it as a base can pick up a sibling's merge.

**How to apply:** brief implementers to reset, squash and diff against an explicit SHA they recorded (`git merge-base HEAD origin/main` captured once), never against the bare `origin/main`; keep `git rebase origin/main` for the deliberate rebase step only. See [[knobas-implementer-brief]].

