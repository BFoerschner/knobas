# knobas — your next steps (written 2026-08-27, after the M1 landing round)

State when this was written: all four paused M1 branches are merged (PRs #25–#28), the
M2 carry-over ledger is ported to GitHub Issues (milestone M2; the original
`m1-carryovers.md` is in git history), and the skills scaffolding (GitHub Issues tracker,
triage labels, single-context domain docs) is configured.

The structural rule underneath everything below: **grilling → spec → tickets happens in
one unbroken context window; every implement run starts from a cleared one.**

---

## Step 1 — `/clear`

Safe immediately. Everything durable is on disk: merged PRs, the carry-over ledger, the
interfaces §9 amendments, the HANDOFF update block, memory notes. The orchestration
transcript is noise for planning, and grilling wants a fresh window.

## Step 2 — the planning session (one sitting, ~1–2 h of your attention)

Paste this as the first message:

```
/mattpocock-skills:grill-with-docs Plan M2 for knobas. Read first, in order:
1. HANDOFF.md — the 2026-08-27 update block at the top
2. docs/superpowers/plans/2026-08-24-m1-carryovers.md — section "M1 landing round → M2 carry-overs"
3. docs/specs/2026-08-23-knobas-design.md §16 — every entry tagged "Rec 08-24" is a
   decision Claude made under delegation that I have never ratified; grill me on each one
4. docs/roadmap.md — what M2 was assumed to be

Open decisions to force: per-kind full_sync_exhaustive vs Sink reconcile; the TeamCity watermark
ceiling; structured status on SourceError; search E-Q1/E-Q2/lever 5; the Jira narrow-payload
backfill (must be early-M2 per the carry-overs). Still-open M1 scope to place: stream D tasks
15–23, gitea tasks 6–8. Output goal: a ratified M2 scope, with decisions recorded as ADRs and
vocabulary in CONTEXT.md.
```

During the interview:

- Facts are Claude's job; **decisions are yours**.
- This session creates `CONTEXT.md` and `docs/adr/` — that *is* the docs refinement.
- If a question needs a runnable answer (state model, a UI you have to see), let it
  detour through `/mattpocock-skills:prototype`.

## Step 3 — same window, immediately after, no clear between

1. `/mattpocock-skills:to-spec` — collapses the grilling into a buildable M2 spec.
2. `/mattpocock-skills:to-tickets` — splits it into tracer-bullet tickets on GitHub
   Issues with blocking edges.

If the window approaches ~150k tokens before to-tickets finishes, `/compact` at a phase
boundary — never mid-grilling.

## Step 4 — the build loop, one ticket at a time

For each ticket with no open blockers:

```
/clear
/mattpocock-skills:implement #<issue-number>
```

Repeat. Each run is self-contained (that is the fix for the erroring agents), drives TDD
internally, and ends with a code review before committing.

**PR-loop variant** (what landed the four branches: worktree + PR + adversarial reviewer,
keeps main GitHub-signed): start a session with plain Claude instead of `/implement` and
say *"land ticket #N through the PR loop"*. Both are legitimate — `/implement` is simpler;
the PR loop gives signed merges and an independent reviewer.

**Never more than two heavy Rust builds at once**, whichever variant.

## Anytime, outside the flow

- `! launchctl getenv RUSTUP_TOOLCHAIN` — hunt where `1.97.1` is exported; it silently
  overrides the repo's 1.94 pin for raw cargo (the justfile strips it; harnesses now do
  too). The one environment landmine left.
- ~~TeamCity swagger wizard~~ — **done 2026-08-27**: your hand-augmented 2026.1 spec is
  vendored at `testenv/specs/teamcity.json` and the fidelity gate is armed (PR #29,
  9/9 green; mockd conforms with zero violations). A re-fetch can no longer overwrite it.
- Your Jira/Confluence instance versions — needed to re-pin the vendored WADL and
  container tags. Still open.
- GPG: cache the key if you want orchestrator docs commits on main signed again.
- `/mattpocock-skills:triage` only for issues that arrive raw from outside — never for
  tickets that `to-tickets` created.
- `/mattpocock-skills:improve-codebase-architecture` in spare moments; feed what it
  surfaces into the next grilling.

---

**Short version:** `/clear` → grill-with-docs (prompt above) → to-spec → to-tickets, all
in one window → then `/clear` + implement per ticket.
