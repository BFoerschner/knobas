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
