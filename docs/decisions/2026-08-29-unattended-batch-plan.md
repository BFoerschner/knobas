# Unattended batch, 2026-08-29 — what is running and why

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

- `49c3baf` (11:39) — "Retire autopilot; Björn reviews every PR before merge".
- `da61faf` (16:48, PR #75) — "Working model: agents merge their own PRs", whose body says in
  terms: *"Overrides the review-gate rule landed in 49c3baf earlier the same day."*

The later one wins, and it is the one checked into `docs/agents/working-model.md` today. So a
per-PR **merge-manager** agent reviews and squash-merges, serially, one at a time. **Björn keeps
the gate for exactly two things: milestone exits, and any change to a frozen contract** — the
`Source` trait, the migrations baseline, and the IPC schema. PRs in that class are left open for
him no matter how green they are.

(My session memory still carried the 11:39 rule and has been corrected.)

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
did not build one, and `app/src/lib/` is `detail`, `ipc`, `launcher`, `shell`, `sources`. Fable
ruled that #69 builds the minimum shell itself rather than waiting for a view nobody is building.

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

Three things arrived after the plan above was written and belong beside it rather than in a PR
body somebody may not open.

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

## One deliberate deviation from a Fable ruling, by a merge-manager

Fable's amended #81 ruling named three edits for PR #108. Two were applied as written. The third —
*"the module docs' '950 of 1,400' back to 1,000"* — was applied as **1,050**, not 1,000, and the
merge-manager recorded why in the PR body and the merge commit.

Its reasoning: that sentence describes what a run *walked* when the cap fired, and at 21 requests
× 50 records that is 1,050. Writing 1,000 would have restored the pre-PR number while making it
false — which is the exact failure the whole PR is about. Fable's own ruling uses the same
arithmetic elsewhere, saying the honest-server message becomes *"at least 1050"*, so the
merge-manager read item 3 as an oversight rather than a position: Fable recomputed the error
message but not the module doc.

I think that reading is right, and it is the kind of disagreement worth having in the open rather
than silently complying with a ruling into a false comment. Recorded here so Björn can overturn
it in one line if he disagrees.

**The same merge-manager also caught that the cap bump broke two tests neither Fable nor the
implementer had named** — `a_branch_listing_that_would_exceed_the_cap_fails_the_run` and its
repository twin both feed 1,001 records and assert the number in the message. Without that fix
the ruling's own edits would have failed CI.

## A second deviation, and this one is worth Björn's eye

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

I think that is right, and it is the same move Fable itself made against Björn's sketched option 1
on the same ticket. Recorded because it is the second time in this batch that a written instruction
was followed in spirit against its letter, and both times the letter was wrong for a reason the
writer could not have seen. If Björn wants the literal wording restored, item 6 has to come back
into scope with it.

**Also from that merge, reported and now filed as #124:** `fixtures.ts` still invents a `required`
Jira never declares, and gives TeamCity's `builds_per_config` a maximum 20× below the adapter's —
with three existing tests asserting the wrong bound. The fixture drift that caused #82's JSON-textarea
bug in the first place is a class, not an instance.

## A misattribution of mine, corrected by the last ruling

I told Björn, more than once, that **ADR-0005 calls the wizard's DONE sentence "the first sentence
knobas ever says to a new user."** It does not. That phrase is in **issue #84's body** and in
Fable's own #137 ruling language, and it reached `FirstRun.svelte`'s doc comment from there.

The distinction matters, and Fable is the one who caught it. ADR-0005 rules what *"mirrored N
items"* **means** — the corpus, not a run's `Upserted` — not **which panel renders it**. So #156's
fork 1 (make the DONE step reachable from a real sync) amends nothing, while fork 2 (re-aim the
rulings at the stats row) would be an ADR supersession and is Björn's.

Had the misattribution stood, #156 would have looked like a question about Björn's ADR when it is
mostly a question about a component. I repeated it because it read as settled; it was settled, but
not by the document I named.

## Fable's closing summary, for the reading order

Fifteen forks ruled across five batches, every one posted to its issue with the overturn line,
nothing frozen touched, and no label, milestone or merge moved by Fable itself. Its suggested
reading order for Björn:

- **#82** — its one divergence from a mechanism Björn sketched.
- **#91** — the most consequential.
- **#154** — the only ruling that alters what a delete does.
- **#156** — the ADR-quote correction above.

Explicitly left for Björn: #156's fork 2 and the DONE panel's voice; #106's option 3
(`SearchResponse`, frozen IPC); #146's battery-clause gate; #84's null-`finished_at` edge; #82's
descriptor-declared identity; and any battery tolerance, ever.

## Where the batch stopped, and why it stopped there

**GitHub Actions went down repo-wide for billing** between 12:09 and 12:16, and it needs Björn:

> The job was not started because recent account payments have failed or your spending limit needs
> to be increased.

Verified rather than relayed: the most recent run **on `main`** failed in one second with `steps: 0`.
Every run before the block was green; every run after fails instantly.

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

**Final state:** 33 issues merged across 29 commits to `main` today. Six PRs open — four gated on
Björn by the frozen-contract rule (#98, #117, #128, #132) and two waiting on CI (#158, #159).
Six worktrees, 217 GiB free. Every `ready-for-agent` issue that was open when the batch began has
either merged or has a PR.

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

**The consequence is worth stating plainly rather than leaving implied.** The backstop existed so a
merge did not rest on one machine's word, and today's batch has three separate cases where CI
caught something a local run did not: PR #126's merge-manager broke `tests/wiring.rs` in a way
`cargo test -p knobas-app --lib` could not see, because that guard is an integration target;
PR #142's manager could not run `svelte-check` or vitest at all in its scratch tree and said CI's
`front` job was the only thing that had ever looked at its edit; and PR #155's manager hit a
`-D warnings` failure on dead bindings only after a rebase. All three would now land on whoever
runs the local gate, which is why the two dispatches say a manager that *cannot* run the frontend
half must not merge.

Not an argument against the decision — it is Björn's call and the block is real. An argument for
the gate being run on the merged head rather than the reviewed one.
