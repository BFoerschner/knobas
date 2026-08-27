# knobas — handoff, paused 2026-08-25

A personal work cockpit (Rust + Tauri 2 + Svelte 5) that syncs Jira / Gitea / TeamCity
into one local Postgres and gives you one search box over all of it.

> **Update 2026-08-27 — the four unmerged branches are landed.** PRs #25 (teamcity, `baeaf00`),
> #26 (gitea, `5a78d98`), #27 (search-ipc, `ccd6a21`), #28 (testenv-compose, `5b2f175`), each
> through its adversarial review loop, all CI-green; worktrees and branches removed. New
> carry-overs from the landing reviews: `docs/superpowers/plans/2026-08-24-m1-carryovers.md`,
> section "M1 landing round"; contract amendments: interfaces doc §9, same date. Still open in
> M1: stream D tasks 15–23 and Gitea tasks 6–8 — both feed M2 planning. The "four unmerged
> branches" section below is historical.

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

**M1's headline exit criterion is met**: the `⌘K < 100 ms` gate passes at **64 ms worst-case p90**
over a 100k corpus (`m1/search-ipc`, unmerged). Getting there exposed two plan defects invisible to
every functional test — `q` was a materialised CTE, so the planner was blind to the tsquery and did
40,000 per-row `entity_pkey` probes; and the statement was a **cached prepared statement**, so the
first fix silently decayed on the sixth keystroke of every session. The second is the one to review
hardest: a plan that is correct five times and wrong thereafter passes any test that runs once.

### The four unmerged branches — all pushed, all clean, none has a PR

Each has a resumption report in `.superpowers/sdd/<plan>/` (git-ignored, local only). Read the
report before resuming its stream; each ends with a sentence its author wanted read first.

| Branch | Head | State | First thing on return |
|---|---|---|---|
| `m1/gitea` | `28abeef` | tasks 1–5 code-complete, 87 tests, 25/25 mutants | rebase, `just check`, open PR — **but settle the blocking ruling below first** |
| `m1/teamcity` | `8a5ef30` | tasks 1–7 complete, 82 tests, 15/15 mutants | `just check` has **never** been run on this branch — do that, then PR |
| `m1/search-ipc` | `71a3475` | tasks 8–10 complete, gate green, 27/33 mutants | rebase (expect one `generate_handler!` conflict, append-only — keep both lines), run the 6 perf mutants, PR |
| `m1/testenv-compose` | `e552800` | tasks 12–14 complete; **task 15 (CI) not started** | task 15, then PR |

Their authors' own warnings, worth more than any summary I could write:
- *teamcity*: the mutation harness first reported "15 survivors" because it merged cargo's stdout
  and stderr, parsed zero results, and an empty baseline passed its own green check.
- *search*: the 100 ms gate passes at 64 ms — but only because the benchmark found a **cached
  prepared statement** that made the fix decay on the sixth keystroke of every session.
- *testenv*: the Kuma healthcheck used to pass on a Kuma with no socket.io server at all —
  *"assume any other green light here is a proxy until you have mutated it."*
- *gitea*: a 403/404 on one repository is fatal during a cursor-less run, not a skip — the literal
  ruling would have let one refused repo lose its whole corpus to the sweep.

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
- **Build artifacts were reclaimed at pause** (~20 GB) — the first `just check` in each worktree
  rebuilds from scratch, roughly ten minutes. Source state is untouched. Disk left at 35 GiB free.
- All Postgres instances were stopped cleanly and no containers are running. The compose volumes
  are **kept and still seeded**, so `docker compose up -d` in `testenv/` returns a working Tidewater
  immediately (Gitea, Uptime Kuma and mockd images are already local; TeamCity/Jira/Confluence were
  deliberately never pulled).
- Each worktree will re-grow a 6–25 GB `target/`. Reclaim on merge; sweep for orphaned Postgres
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
