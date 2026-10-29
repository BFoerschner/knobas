# Unattended batch, 2026-08-29 — what ran, and why

Björn asked for "all the ready issues that are able to be implemented now in parallel by
subagents if possible and not blocked by anything else", said he would be away, and delegated
open decisions to Fable with the instruction to write them down. This file is the orchestration
half of that; `2026-08-29-unattended-batch-rulings.md` is the decision half.

## What "in parallel" is allowed to mean here

Not "all sixteen at once". `docs/agents/working-model.md` records a measured ceiling, learned by
killing five agents at load average 79.7 on this 12-core machine: **at most 2 concurrent Rust
implementers, plus a third only if it does no Rust compile.** Reviewers count as implementers.
The doc also says "never launch more than agreed — ask before scaling", and Björn is not here to
ask, so the ceiling is the answer rather than the floor. Parallelism therefore comes from
pipelining waves, not from starting everything.

## Merge authority — resolved from the record, not from recollection

Two rules landed on 2026-08-28 and they contradict each other:

- `49c3baf` (authored 11:08, committed 11:39) — "Retire autopilot; Björn reviews every PR before
  merge".
- `da61faf` (16:48, PR #75) — "Working model: agents merge their own PRs", whose body names the
  rule it replaces: *"Björn overrode the review-gate rule of 49c3baf on 2026-08-28: implementers
  still stop at PR-open, but a per-PR merge-manager agent now runs the review pass and
  squash-merges."*

The later one wins, and it is the one checked into `docs/agents/working-model.md` today. So a
per-PR **merge-manager** agent reviews and squash-merges, serially, one at a time. **Björn keeps
the gate for exactly two things: milestone exits, and any change to a frozen contract** — the
`Source` trait, the migrations baseline, and the IPC schema. PRs in that class are left open for
him no matter how green they are.

(My session memory still carried the earlier rule and has been corrected.)

## Triage of the sixteen `ready-for-agent` issues

**Ready now, no open fork:** #78, #86, #83.

**Ready, but carrying a design fork that should be ruled once rather than guessed four times:**
#81 (Gitea short-page paging), #91 (TeamCity watermark ceiling), #92 (TeamCity live suite),
#82 (PAT username contradiction), #93 (no engine+DB+adapter join test).

**#84 — I had this wrong, and Fable caught it.** Its body still carries the 2026-08-28 ruling
*"grill this before it becomes a ticket"*, so I routed it to Fable as a decision. The grilling had
already happened: Björn settled it on 2026-08-29 as **ADR-0005** (`5c850cd`), and a full agent
brief with acceptance criteria was posted to the issue at 03:01. It is implementable as written,
and it is a deep-pass change (sync engine, progress sinks) with no frozen surface.

**#69 — the blocker is void.** Its "Blocked by" wants a settings view to exist; #36 is closed and
did not build one, and `app/src/lib/` held only `detail`, `ipc`, `launcher`, `shell`, `sources`.
Fable ruled that #69 builds the minimum shell itself rather than waiting for a view nobody is
building. (`settings/` is there now because #69 built it.)

**Big M2 features, unblocked but gated:** #42 (write queue), #41 (suggestion engine),
#46 (notes). Each explicitly needs a migration, and migrations are single-writer (orchestrator)
*and* a frozen surface, so each lands as an open PR for Björn rather than a merged one.
#43 depends on #42 and says so outright ("If #42 has not landed, this cannot land either — say
so rather than adding a temporary direct path"); #44 and #45 depend on #43 and #42. Those three
are genuinely blocked until #42 lands and are not dispatched.

## Wave plan

| Wave | Rust slot A | Rust slot B | No-compile slot |
|---|---|---|---|
| 1 | #78 inventory-write | #91 → #92 (same crate, two PRs) | #86 + #83 (one PR) · Fable rulings |
| 2 | #84 (ADR-0005, deep pass) | #81 | #69 settings surface |
| 3 | #93 | #82 | — |
| 4 | #42 write queue (needs a migration from the orchestrator; PR left open for Björn) | #46 or #41 | — |

Each implementer works in its own worktree under `.worktrees/`, opens a PR, and stops. Merge-
managers run afterwards, serially, and only for PRs outside the frozen-contract class.

## Corrections and ratification requests, appended as the batch ran

Three things arrived after the plan above was written.

**A premise in my own dispatch was wrong, and the implementer built to the code instead.** #69's
brief told the implementer that "restore is the destructive one — it replaces the user's
database", and asked for a confirmation stating what would be lost. The landed engine does not
overwrite: `knobas_db::backup::restore` refuses with `TargetNotEmpty` when the database holds
knobas rows, because merge-with-a-preview is M4. A warning about losing existing data would have
described a case that cannot happen. The confirm therefore states what comes back, that the
keychain is untouched, that the mirror re-syncs, and that restore only works into a knobas
holding no data yet. **This wants Björn's ratification** — it is the right reading of the code,
but it is product semantics, and it was settled by an implementer correcting an orchestrator
rather than by anyone with the remit.

**Two follow-up tickets were filed from findings agents were told to recommend, not file:**

- **#103** — a component test can pass every assertion while leaking an unhandled rejection, with
  the only signal a bare non-zero exit at the end of `just check`. One file now guards against
  it; no other does. Same shape as #71 and #86: a gate that can be green while something is
  wrong.
- **#104** — three documents say the backup settings surface is blocked or absent, and PR #102
  makes all three false. Two sit inside the frozen `commands/` + `ipc/` paths, so the
  implementer correctly stopped rather than editing them. Labelled `ready-for-human` for that
  reason; the change itself is one comment line each.

**Wave 1 outcome:** #78 merged (`3dadc74`) — the merge-manager found, in review, that `mktemp`
plus `mv` was silently dropping the tracked `test-inventory.txt` from 0644 to 0600 on every
`inventory-update`, which git records nowhere and no gate would ever have shown. PRs #99 (#86,
#83), #102 (#69) and #98 (this record) open; #91 and #92 in flight.

## A deliberate deviation from a Fable ruling, by a merge-manager (PR #108)

Fable's amended #81 ruling named three edits for PR #108. Two were applied as written. The third —
*"the module docs' '950 of 1,400' back to 1,000"* — was applied as **1,050**, not 1,000, and the
merge-manager recorded why in the PR body and the merge commit.

Its reasoning: that sentence describes what a run *walked* when the cap fired, and at 21 requests
× 50 records that is 1,050. Writing 1,000 would have restored the pre-PR number while making it
false — which is the exact failure the whole PR is about. Fable's own ruling uses the same
arithmetic elsewhere, saying the honest-server message becomes *"at least 1050"*, so the
merge-manager read item 3 as an oversight rather than a position: Fable recomputed the error
message but not the module doc.

Recorded here so Björn can overturn it in one line. Complying silently would have written a false
comment, which is the failure the whole PR is about.

**The same merge-manager also caught that the cap bump broke two tests neither Fable nor the
implementer had named** — `a_branch_listing_that_would_exceed_the_cap_fails_the_run` and its
repository twin both feed 1,001 records and assert the number in the message. Without that fix
the ruling's own edits would have failed CI.

## A second deviation, worth Björn's eye (PR #110)

PR #110 (#82) merged as `8f2729b`. Its merge-manager departed from Fable's ruling on one point, for a
reason that is more interesting than the point itself.

Fable's ruling item 4 asked that `describe_missing_identity()`'s advice point the reader at
re-running *Test connection*. The implementer did exactly that:

> "No source has a username configured, so knobas cannot tell which items are yours. Run Test
> connection on a source to fill its username in, or type one yourself."

That message renders when a **saved** source has no username — and a saved source offers Re-enter,
Sync now and Delete. The only *Test connection* in the app is a step of the Add-source dialog, on a
draft. Fable's own item 6 put re-enter-path backfill explicitly out of scope, which is what removed
the path that would have made item 4's advice followable.

So the ruling's item 4 rests on a premise its item 6 falsified, and the fix for *a surface telling a
user to do something they cannot do* had replaced it with a more specific version of the same sin.
The merge-manager applied the ruling's stated intent — the advice has to be followable — over its
literal suggestion, reworded the message to say when the fill happens and admit the gap, and made
the test refuse the imperative.

It is the same move Fable itself made against Björn's sketched option 1 on the same ticket, and the
second time in this batch that an instruction was followed in spirit against its letter — both
times because the letter was wrong for a reason its writer could not have seen. If Björn wants the
literal wording restored, item 6 has to come back into scope with it.

**Also from that merge, reported and now filed as #124:** `fixtures.ts` still invents a `required`
Jira never declares, and gives TeamCity's `builds_per_config` a maximum 20× below the adapter's —
with three existing tests asserting the wrong bound. The fixture drift that caused #82's JSON-textarea
bug is a class, not an instance.

## A misattribution of mine, corrected by the last ruling

I told Björn, more than once, that **ADR-0005 calls the wizard's DONE sentence "the first sentence
knobas ever says to a new user."** It does not — the ADR contains no such phrase. That wording is
**issue #84's**, verbatim: *"It is also the first sentence knobas ever says to a new user, at the
moment they are deciding whether the tool works."* Fable's #137 ruling says it of a source rather
than a user (*"the first sentence knobas ever says about a new source"*), and that is the form that
reached `FirstRun.svelte`'s doc comment: *"this is the first sentence knobas ever says about a
source."*

The distinction matters, and Fable is the one who caught it. ADR-0005 rules what *"mirrored N
items"* **means** — the corpus, not a run's `Upserted` — not **which panel renders it**. So #156's
fork 1 (make the DONE step reachable from a real sync) amends nothing, while fork 2 (re-aim the
rulings at the stats row) would be an ADR supersession and is Björn's.

Had the misattribution stood, #156 would have looked like a question about Björn's ADR when it is
mostly a question about a component. I repeated it because it read as settled; it was settled, but
not by the document I named.

## Fable's closing summary, for the reading order

Fifteen forks ruled across five batches, every one posted to its issue with the overturn line, and
nothing frozen touched. Its reading order for Björn — **#82**, **#91**, **#154**, **#156** —
and the list of what it explicitly left for him are in the rulings file's closing section, which
is the copy to trust. They are not restated here, because two copies of a list drift.

## Where the batch stopped, and why it stopped there

**GitHub Actions went down repo-wide for billing** between 12:09 and 12:16, and it needs Björn:

> The job was not started because recent account payments have failed or your spending limit needs
> to be increased.

Verified rather than relayed: the most recent run **on `main`** failed in one second with
`steps: 0` (run `33252382037`, 12:23:38→12:23:39). Before the block, runs failed only on their own
merits — two did that morning, `m2/adr0005-followups` at 08:20 and `m2/fixtures-mirror-check` at
09:09 — and the last green `main` run was 12:08:43. After 12:16:27 every run fails instantly.

**PR #155 was merged on a locally-run gate rather than a green CI**, by its merge-manager, which
flagged the judgement call plainly instead of quietly. The case for it: test-only change, its
pre-rebase sha had been green on real CI for 8m24s, and the delta was a rebase onto a green `main`
plus doc edits and two dead-binding removals. The merge is `90a5276` if Björn would rather it not
stand.

**I have stopped merging there, deliberately.** PRs #158 and #159 are open, locally gated, and
**not** dispatched to merge-managers. One merge on a local gate, flagged, is a defensible call by
the agent in front of it. Making that the batch's standing practice while Björn is away — across a
scheduler guard on a destructive path and a change to what the first-run wizard shows — is not a
call I should make on his behalf. They wait for CI.

**State at that point** (revised twice below, as the day went on): 33 issues merged across 29
commits to `main`. Six PRs open — four gated on Björn by the frozen-contract rule (#98, #117,
#128, #132) and two waiting on CI (#158, #159). Six worktrees, 217 GiB free. Every
`ready-for-agent` issue that was open when the batch began has either merged or has a PR.

**The first thing to do on return is the billing settings**, because it unblocks everything else.

## CI is disabled; the gate is local now (Björn, 2026-08-29)

The billing block did not clear — a re-run of a blocked job still failed in one second with
`steps: 0`. Björn's instruction: *"just run the ci locally instead then and disable it on GitHub"*.

Done: `gh workflow disable check` and `gh workflow disable testenv`. Both now read
`disabled_manually`. Reversible with `gh workflow enable <name>` whenever he wants them back.

**This makes two statements in `docs/agents/working-model.md` false, and they should be amended
by whoever owns that doctrine:**

1. Under *Review economics*: **"The gate is CI's job, not the reviewer's. Reviewers check
   `gh pr checks` and run targeted tests plus their own mutations, rather than re-running the whole
   `just check`."** That rule assumed a CI that runs. Every merge-manager from here must run the
   full `just check` itself, on the exact head it merges — which is what the two dispatched for
   #158 and #159 were told, in those words.
2. Under the branching model: **"GitHub Actions runs `just check` on every PR as the
   machine-enforced backstop (plan 01 task 11), independent of anyone's worktree."** There is no
   longer a backstop independent of anyone's worktree.

The backstop existed so a merge did not rest on one machine's word, and today's batch has two
recorded cases where CI caught something a local run did not. PR #126's merge-manager broke
`tests/wiring.rs` and said so on the PR: *"CI caught it (run 33242918211); local
`-p knobas-app --lib` did not, because the guard lives in an integration target."* PR #155's
manager hit a `-D warnings` failure on dead bindings that appeared only after its rebase. Both
would now land on whoever runs the local gate, which is why the two dispatches say a manager that
*cannot* run the frontend half must not merge.

(An earlier draft of this paragraph named a third case, PR #142, whose manager was said to have
been unable to run `svelte-check` in its worktree. Nothing on #142 supports that: the PR carries
no comments and no reviews, and its own body reports `front` — svelte-check, vitest and
`vite build` — green. Removed rather than left standing on a merge-manager's session report
alone.)

(The PR #155 case does not survive checking either. That PR has no comments and no reviews, and
the only failed `check` run on `m2/gitea-live-second-push` — `33252091024` — had its job alive
two seconds with no steps: the billing failure, not a compile. The green run on that branch,
`33250680535`, was the *pre-rebase* sha, and the merge landed at 12:23:35Z, after the block, so
CI never ran on the post-rebase head at all — which is what this record already says two
sections up: *"PR #155 was merged on a locally-run gate rather than a green CI"*. The paragraph's
conclusion is unaffected: it rests on the #126 case, which does check out. Annotated by #169's
merge-manager, 2026-08-29.)

Björn's call, and the block is real. But it is an argument for running the gate on the merged head
rather than the reviewed one.

## Closing state

**35 issues merged when the batch closed**, with `main` at `b0b85e8`. The last two — #154 and #156
— merged under the local gate, and both merge-managers ran the full `just check` on the exact head
they merged rather than on the head they reviewed. #159's manager hardlink-cloned `node_modules`
into its own worktree rather than skip the frontend half, which is the standard the dispatches
asked for.

Four worktrees stood at that point, all belonging to open PRs Björn gated: `docs-batch` (#98),
`i41` (#132), `i42` (#117), `i46` (#128). 224 GiB free. The queue for him was #98 (this record)
→ #117 (write queue, migration 0005) → #128 (Notes, 0006) → #132 (suggestions, 0007), with
#43, #44 and #45 blocked until #117 landed — #43 says so in its own spec. The section below is
what then happened to that queue.

**One question escalated from the last merge, and it is one line of judgement.** #159's
merge-manager asks whether step 2's stats row still needs its tri-state now that a finished run
leaves that step. Two things there are now dead — `itemsReading`'s `—` branch and the `…` pending
state, and the `<progress max={finished ? 1 : undefined}>` beside them. Neither was removed,
deliberately: removing them retires the surface #137's ruling constraint 3 was explicitly placed
on, which is fork 2 and Björn's.

The manager added the fact that decides it: **the deadness is now unguarded.** It put loud sentinel
values in all three arms and the entire 22-test suite stayed green. So they are unreachable *and*
uncovered — a future edit breaks them silently. That is the cost of keeping them.

**Still open and unruled, all recorded on their issues:** #104, #112, #122 (the idempotency-key
SPI question), #141 (`SearchResponse`, frozen IPC), and two filed after this list was first
written — #161, from PR #132's merge-manager, and #162, from PR #160's. Both are traps laid for
the next person in the file rather than live defects. #157 was on this list and has since landed
as `29adcba`.

## The frozen-contract gate is delegated to Fable (Björn, 2026-08-29)

Björn's instruction: *"let Migration and ipc additions be merged by fable too."*

So the rule this record opened with — *"Björn keeps the gate for milestone exits and for any change
to a frozen contract"* — is now narrower. **Migrations and IPC additions are Fable's to merge**,
under the same merge-manager discipline as everything else: deep review, mutations re-run, the full
`just check` on the exact merged head, and a §10.8 ratified-exception entry recorded in
`docs/contract.md`.

**Two things this does not change, and I have not treated as delegated:**

1. **Milestone exits.** He named migrations and IPC additions; a milestone exit is neither, and it
   is the one gate in the working model that is about judging a body of work rather than a diff.
2. **The battery clauses.** Fable itself ruled on #146 that a contract-battery change is Björn's,
   citing the care taken by the one ratified exception that ever touched `contract.rs` —
   ADR-0004's entry in §10.8, which is in `docs/contract.md`, not in the ADR itself: *"nothing
   more -- no battery clause added, removed or reworded."* A delegation of *migrations and IPC*
   does not reach that, and Fable saying so about its own authority is the reason to keep it.

Recorded here because this record is itself one of the four PRs the delegation unblocks, and
because the doctrine it opens with would otherwise be stale on the day it merged.

**What the delegation unblocked, before this record merged.** All three frozen-contract PRs went
through the Fable gate: #117 (write queue, migration 0005) as `83519a3`, #128 (Notes, 0006) as
`8fdcf60`, #132 (suggestions, 0007) as `1ed92e2`, followed by #160 (#157) as `29adcba`, where
`main` stands. The day's total is **39 issues across 35 commits**. #43 is unblocked by #117 and in
flight; #44 and #45 follow it. Of the four PRs open at the delegation, this record is the last, and
it merged on a full local `just check` run on its rebased head — no CI, per the section above.
