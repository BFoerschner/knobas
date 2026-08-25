# knobas — handoff, paused 2026-08-25

A personal work cockpit (Rust + Tauri 2 + Svelte 5) that syncs Jira / Gitea / TeamCity
into one local Postgres and gives you one search box over all of it.

**Paused deliberately mid-M1. Everything is committed and pushed. Nothing is half-written
to disk that isn't also on the remote.**

---

## Read these first, in this order

1. `docs/superpowers/specs/2026-08-23-knobas-design.md` — what knobas is. §16 lists every
   decision and who made it; entries tagged **Rec 08-24** were decided by Claude under
   delegation and are yours to override.
2. `docs/superpowers/plans/2026-08-24-knobas-roadmap.md` — milestones, and §3 the working
   model (PR loop, review economics, concurrency limits). Read §3 before dispatching agents.
3. `docs/superpowers/plans/2026-08-24-m1-interfaces.md` — **the contract**. §8 rulings P1–P13,
   §9 amendments, §10 *as-built* (this is the truth; §2 carries superseded shapes, marked).
4. `docs/superpowers/plans/2026-08-24-m1-carryovers.md` — obligations owed to specific streams,
   including two that must be done **early in M2** and one that needs **you**, not an agent.
5. `docs/architecture.d2` / `.svg` — how it couples together (graphite = merged, amber = in
   flight at time of drawing, dashed = planned).

## Where the project stands

**M0 complete** (12 PRs). **M1: 24 PRs merged, ~59 of 82 stream tasks.**

| Stream | State |
|---|---|
| Contract | ✅ merged — migration 0002, SPI changes, `IpcError`, `knobas-search`/`knobas-http` seeds, demo profile |
| A Jira adapter | ✅ **complete and certified end to end** |
| F sync engine | ✅ **complete** — scheduler, dedicated-connection advisory lock, credential health, backoff, registry, sources IPC |
| T testenv | mockd Jira + TeamCity merged; compose/seeds **in progress** (`m1/testenv-compose`) |
| E search | grammar, SQL builder, grouped results, smart lists, board merged; IPC + launcher UI + 100k benchmark **in progress** (`m1/search-ipc`) |
| D frontend | phase 0 + rooms/details/entity read path merged; tasks 15–23 **not started** |
| B Gitea adapter | **in progress** (`m1/gitea`), pure layers + repo sync |
| C TeamCity adapter | **in progress** (`m1/teamcity`), all 7 tasks |

**Four branches carry unmerged work**, each with a resumption report in
`.superpowers/sdd/<plan>/` (git-ignored, local only — read them before resuming a stream):
`m1/gitea`, `m1/teamcity`, `m1/search-ipc`, `m1/testenv-compose`.

Merged branches are kept on the remote deliberately. They look "unmerged" to
`git merge-base --is-ancestor` because every PR was **squash**-merged; don't let that mislead you.

## Resuming

- `just deps && just check` — the gate. First run downloads PostgreSQL 18.6 once to `~/.theseus`.
- `just dev` (real profile) / `just demo` (separate profile, Tidewater fixture, own data dir,
  port and keychain service). Demo data cannot reach the real profile.
- Per stream: read its report, `git worktree add .worktrees/<name> <branch>`, dispatch an
  `implementer` with the remaining task briefs. Reviews go to `pr-reviewer-std`; `pr-reviewer`
  (xhigh) is for frozen contracts, migrations, concurrency, secrets.
- **Concurrency is bounded by the machine, not by task independence** — see roadmap §3. Two
  heavy Rust implementers is the safe default; a five-agent wave once drove load to 79.7 on
  12 cores and the watchdog killed three of them.
- Each worktree carries a ~6–25 GB `target/`. Reclaim on merge; sweep for orphaned Postgres
  before a wave (`ps aux | grep postgres`).

## ⚠ One blocking item before stream B merges

Gitea's per-repo budgets plus `full_sync_exhaustive: true` would meet the sweep now merged on
`main` and tombstone every commit past the cap on each full sync. **Ruled: budgets ⇒ declare
`full_sync_exhaustive: false`.** Written up at the top of the carry-overs doc; a reviewer should
treat `true` alongside any cap as a blocking finding.

## Needs you, not an agent

- **Vendor the TeamCity swagger**: `cd testenv/specs && ./fetch.sh --teamcity`. Needs ~10 GB
  free and a human at a browser (the first-start wizard is form endpoints, deliberately not
  scripted). Until then TeamCity is validated against golden fixtures only. The schema test is
  written and **self-arming** — it starts asserting the moment the file appears.
- **Your Jira/Confluence instance versions**, to re-pin the vendored WADL and container tags.
- **GPG**: the key wasn't cached at the end of the session, so the last few `main` commits are
  unsigned (`%G?` = `N`). Agent branches are unsigned by design; PR merges are GitHub-signed.

## The thing most worth carrying forward

Nine process rules live in `.claude/agents/{implementer,pr-reviewer,pr-reviewer-std}.md`, and
every one was earned by an agent discovering its own instrument had lied to it. They are the
reason the review loop found what it found; read them before writing new agent definitions.

The recurring defect all session was **a check that measures a representation of the thing
instead of the thing** — a lint satisfied by a comment mentioning it, a path substring standing
in for a resolved path, a mutation that never compiled, a restore that left a phantom mutant, a
benchmark timing an unvacuumed index, a fixture of zeros that cannot witness a swap. All of them
fail *green*. Four defects in Claude's own plans were caught the same way and corrected at source.
