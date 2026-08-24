# knobas — roadmap: MVP, milestones, parallel workstreams

**Status:** proposed 2026-08-24 (Claude), for Björn's review. Everything here follows from the revised design doc (`docs/superpowers/specs/2026-08-23-knobas-design.md`) and the 2026-08 technology research (summarized in §Stack below). Nothing is implemented yet.

This is the decomposition document the superpowers flow calls for: the project is too large for one implementation plan, so it is cut into milestones, each milestone gets its own plan written just-in-time via `superpowers:writing-plans`, and each plan is executed with `superpowers:subagent-driven-development` (or parallel worktree streams where the milestone allows it). Plan 01 (Foundation) is already written: `docs/superpowers/plans/2026-08-24-plan-01-foundation.md`.

---

## 1. What "MVP" means here

Björn's stated pain: **bad source-system search (JQL/Confluence) and credential juggling.** The earliest build that beats the status quo at that is the MVP:

> **MVP = end of M1: sources connected once (credentials in the keychain), everything synced locally, one `⌘K` box that finds any ticket/PR/build/page in <100 ms, context rooms and detail views to read it all in one window.**

That build is read-only toward the sources, has no links UI, no timer, no assets — and is already worth opening every morning. Everything after M1 makes knobas *knobas* rather than a fast index:

- **M2** is the identity release (links, suggestions, write-back, start-work, inbox).
- **M3** makes it the whole workday (time, standup, Confluence).
- **M4** adds the estate (assets, monitoring, Flowrun, export/import).

Ship order M3 vs. M4 can be swapped by pain; nothing in M4 depends on M3.

---

## 2. Milestones

### M0 — Foundation (one stream, sequential; plan 01, written)

The contract-freezing milestone. No parallelism yet — everything later builds on these interfaces, so one agent (or the session itself) does it carefully.

Contents: Cargo workspace + Tauri 2 + Svelte 5/Vite scaffold · embedded Postgres lifecycle (`postgresql_embedded`, PG 18.6 pinned, TCP localhost) · migration baseline (schemas `knobas` + `sync`, FTS with `GENERATED ALWAYS AS (...) STORED`) · `knobas-core` (EntityRef, links, activity) · `knobas-source` SPI (the `Source` trait + contract-test battery) · the Tidewater seed dataset as machine-readable fixtures + `--demo` loader · `knobas-source-mock` · minimal sync run (mock → `sync.item` → FTS) · IPC commands (`search`, `sync_now`, …) with typed TS bindings · `just check` gate.

**Exit criteria:** `just check` green; the Tauri window opens; `--demo` loads Tidewater; searching "sepa retry" over IPC returns PAY-231 grouped results from Postgres FTS.

**Freeze on exit:** (1) the `Source` trait, (2) the migration baseline, (3) the IPC command/event schema. Later changes to any of the three go through the orchestrator and a doc update, never unilaterally inside a workstream.

### M1 — Read cockpit ⇒ **MVP** (max parallelism: 6 streams)

| Stream | What | Depends on |
|---|---|---|
| A | **Jira adapter** (read: issues, epics, comments, worklogs; incremental; Cloud `/search/jql` + DC v2) | M0 SPI |
| B | **Gitea adapter** (read: repos, branches, PRs, commits; client generated from the instance's OpenAPI 3 spec or `gitea-sdk`) | M0 SPI |
| C | **TeamCity adapter** (read: builds, configs, queue; ~4 endpoints, `Accept: application/json`, `fields=`) | M0 SPI |
| D | **Frontend shell**: top strip, status bar, room + tiles (read-only), detail slide-over (read-only), sources view, first-run wizard | M0 IPC + seed data |
| E | **Search**: FTS corpus incl. ancestor paths, launcher (prefixes, chips, aliases, empty-query board), grouped results, built-in smart lists (read-only counts) | M0 IPC + seed data |
| F | **Sync engine**: scheduler (per-source interval, *Sync now*), cursors, backoff, 401 detection → credential health, keychain integration, diagnostics view | M0 SPI |

A/B/C develop against recorded HTTP fixtures + the shared contract battery; D/E develop entirely against the mock source and seed data; F uses the mock's simulated failures. **Integration checkpoint at the end:** connect a real Jira, Gitea, TeamCity; full initial sync; search everything.

**Exit criteria:** real credentials entered once, land in the keychain; initial + incremental sync works; `⌘K` < 100 ms over the synced corpus; rooms and details browsable; sources view shows sync health. **This build is the MVP — start using it daily.**

### M2 — Links & actions (the identity release; 4–5 streams)

Links UI everywhere (panels, *Link to…*, Tab action chains, `[[refs]]`) over the one link table · suggestion engine (keys in commits/branches/build params/page text, FTS similarity, native source links; reasons; dismissals persisted) + room tray · write-back: Jira status/comment/create, Gitea branch/PR/comment/approve, TeamCity trigger/re-run · write queue with conflict UI (re-read before flush, ask with diff) · **start-work flow** + reverse (PR merged → In Review) · inbox v1 (mentions, review requests, failed builds, assignments, credential expiry; actions; snooze with date) · notes (markdown, `[[…]]` chips, backlinks) · contexts complete (promote ticket, ad-hoc, 1-hop membership, per-context inbox filter) · scheduled **backup export** (`pg_dump` of `knobas`).

**Exit criteria:** ticket→branch→PR round-trip against real systems; a morning driven from the inbox; links/suggestions in daily use; a nightly backup archive exists.

### M3 — Time & the daily flow (3–4 streams)

Global timer (`⌘T`, any entity or ad-hoc label, switch-on-context-change) · worklog draft (concatenated intervals, activity checkboxes, generated comment) → Jira · ad-hoc block dialog · passive attribution (opt-in) · day review strip · week timesheet with correct *Log all* · standup digest (from the activity stream, traceable lines) + protocol (save as note / publish) · **Confluence adapter** (read + CQL search via v1, create-from-template, comment, macro-free section edit) · desktop notifications.

**Exit criteria:** a full real workday tracked, reviewed, logged to Jira; standup protocol published to Confluence.

### M4 — Assets, monitoring, estate (4–5 streams)

Asset model (infinite tree, typed+custom properties, routes, relations, env/owner inheritance, history) · Miller columns UI + spines + wires + pane (+ "Depends on this" panel) · **Uptime Kuma adapter** (v2: `/metrics` poll → own timeseries; `kuma-client` for config; create/pause write-back) · monitors tab, alerts → inbox with routing rule + ack semantics · **Flowrun adapter** (instances, scenarios, Run/Log/**Promote**) + tab · asset smart lists + "save search as list" · **export/import complete** (share export, merge-restore with preview) · seed-fixture extension for assets.

**Exit criteria:** the real estate modeled and browsable; an alert flows Kuma → inbox → ack; a scenario promoted from knobas; export restores on a clean machine.

### v1.5 — fast follows (any order, one stream each)

Open in editor/terminal · paste-URL → entity chip · quick capture hotkey · environment-matrix view · remaining §13 backlog by demand.

---

## 3. Working model for parallel agents

**Decided by Björn 2026-08-24: implementation runs as a PR loop between two pinned agent roles, orchestrated by the Fable session.** Agent definitions live in `.claude/agents/` (versioned in this repo):

| Role | Model / effort | Does |
|---|---|---|
| `implementer` | Opus 5 / **high** | one plan task per dispatch, own worktree + branch, TDD, `just check` green, opens the PR; addresses review findings on follow-up |
| `pr-reviewer` | Opus 5 / **xhigh** | checks the PR out into its own throwaway worktree, **runs the tests itself**, judges against the plan task + design doc + global constraints, posts findings as `gh pr review --request-changes` comments or approves |
| `integrator` | Opus 5 / **high** | on-demand only: rebases a stale branch onto current `main` when its original implementer is gone, resolves conflicts faithfully to both sides' intent, re-runs `just check`; never adds behavior |
| orchestrator | Fable (this session) | dispatches, relays review ↔ fix rounds (continuing the same agents so context is kept), adjudicates disputes, merges **serially**, syncs `main`, prunes worktrees, owns the frozen contracts and the migrations directory |

**Effort levels (rationale):** `high` is Opus 5's default and its cost/quality sweet spot for well-specified implementation tasks — the plans carry the hard thinking already. `xhigh` is reserved for the one place deeper reasoning demonstrably pays: adversarial verification. `max` is deliberately unused in the loop (large latency/cost for marginal gain on specified work); it's a break-glass option for one-off gnarly debugging. Effort is pinned per role (the Agent tool takes no per-dispatch effort override); if xhigh reviews prove too expensive on low-risk PRs (docs, fixtures, pure-UI), the escape hatch is a second `pr-reviewer-std` role at `high` for those — deferred until cost data says so.

**The loop per task:** dispatch implementer → PR opens → dispatch pr-reviewer → findings posted on the PR → orchestrator relays them to the *same* implementer (continuation, not a fresh agent) → fix commits pushed → *same* reviewer re-reviews the delta → repeat. **Termination is objective, not vibes:** merge when the reviewer approves AND `just check` is green. **Hard cap: 3 review rounds** — if agents still disagree, the orchestrator adjudicates with a written rationale or escalates to Björn. This prevents both infinite ping-pong and mutual rubber-stamping.

**Merging & signing:** `commit.gpgsign=true` is set globally, and subagents cannot serve pinentry prompts — so implementers set `commit.gpgsign false` in their worktrees, and the orchestrator merges with `gh pr merge --squash --delete-branch`: `main` stays linear (one commit per task, short imperative subject taken from the PR title) and every `main` commit is GitHub-signed/verified. Björn's own commits stay GPG-signed as before.

**Git rules (supersedes the earlier "agents never run git"):** agents run git **only inside their own worktree/branch** and `gh` only against their own PR; nobody but the orchestrator touches `main` or merges; the repo-root checkout belongs to the orchestrator. Human gate: Björn reviews at milestone exits and whenever a frozen contract (Source trait / migrations baseline / IPC) needs changing; day-to-day PRs merge on reviewer approval (he can watch them live on GitHub).

**Branching model (decided 2026-08-24): trunk-based with short-lived task branches.** What actually keeps parallel features from breaking each other is not the branches — it's three structural rules; the branches just carry the work:

1. **Streams own disjoint code.** The milestone streams are cut along crate/module boundaries (one crate per adapter, one component region per frontend surface), so concurrent PRs rarely touch the same files. Cross-cutting surfaces — the `Source` trait, the migrations directory, the IPC schema — are frozen and **single-writer (orchestrator)**; migrations are the #1 real-world collision source and are therefore requested from the orchestrator, never added inside a stream.
2. **Branches stay short-lived: one task = one branch = one PR to `main`**, named `m<milestone>/<stream>-<slug>` (e.g. `m1/jira-adapter-incremental-sync`), merged within its review loop — typically hours-to-a-day of divergence, so merge-back is trivial by construction. Long-lived per-feature branches are the *cause* of unmergeable code, not the cure; we use them only when a feature genuinely can't land in working slices, as `feat/<name>` integration branches fed by the same task-PR loop and merged to `main` after one final full review. Pre-1.0 there is no release to protect, so a half-built feature ships to `main` simply not wired into navigation rather than living on a stale branch.
3. **`main` must always pass `just check`, and merges are serial.** The implementer rebases onto `origin/main` before opening the PR and again whenever `main` moved during review (re-running `just check` after every rebase); the orchestrator merges one PR at a time and, when a merge conflicts with a still-open PR, has that PR's implementer rebase next (or dispatches the `integrator` if that agent is gone). GitHub Actions runs `just check` on every PR as the machine-enforced backstop (plan 01 task 11), independent of anyone's worktree.

Other standing rules (HANDOFF §6) stay in force: worktrees under `.worktrees/` (gitignored), harness cap 20 but **never launch more than agreed — ask before scaling** (suggested default: 4–6 concurrent; M0 is sequential anyway — one implementer + one reviewer alive at a time), agents report raw data. Cost note: xhigh reviews are the expensive step by design; re-reviews stay affordable because the continued reviewer only examines the delta.

What makes the streams independent (all built in M0):

1. **Contract-first.** The `Source` trait, the DB migration baseline, and the typed IPC schema are frozen at M0 exit. Adapter agents see only the SPI; frontend agents see only IPC types + seed data.
2. **The mock source is the frontend's backend.** Every UI stream runs `--demo` and never needs credentials.
3. **The contract battery is the adapter's spec.** `knobas-source` ships the test suite every adapter must pass (sync, incremental cursor, 401 handling, write-op mapping); an adapter stream is "done" when the battery and its own fixture tests are green.
4. **One migration directory, orchestrator-owned.** Workstreams request schema changes via the orchestrator so migration numbering never conflicts.

Per-task discipline (unchanged from superpowers): TDD, frequent commits, `superpowers:requesting-code-review` before merging a stream, verification-before-completion with command output.

**Test strategy by layer:** unit tests per crate (TDD) · adapter contract battery + recorded HTTP fixtures (wiremock-style) for Jira/Confluence/TeamCity · a `docker-compose.dev.yml` with **real Gitea and real Uptime Kuma v2** (both trivially self-hostable) for live integration tests, plus a tiny Flowrun stub server in-repo · frontend QA in headless Chrome against `--demo` (per-agent `--user-data-dir`/port — parallel agents have collided before) · milestone exit = manual checklist against real systems.

---

## 4. Stack (decided by research 2026-08-24; pins live in plan 01)

| Layer | Choice | Why (short) |
|---|---|---|
| DB | **PostgreSQL 18.6 via `postgresql_embedded` 0.21**, download-on-first-run, TCP 127.0.0.1; "existing PG URL" setting | 0.12 s start, 21 MB idle, ships `pg_dump`/`pg_restore` (export = free), proven (Retrom); sidesteps the open Tauri macOS `externalBin` notarization bug |
| DB access | **sqlx 0.9** (`runtime-tokio`, explicit features), embedded `migrate!` at startup | compile-checked raw SQL is right for FTS; diesel only wins on typed tsvector, not enough |
| App shell | **Tauri 2.11**, `tauri::async_runtime::spawn` for the scheduler, events for state changes, `ipc::Channel` for sync progress | events are explicitly not for high throughput |
| Secrets | **`keyring` crate 4.x directly** | `tauri-plugin-stronghold` is deprecated; `tauri-plugin-store` is unencrypted |
| HTTP | **reqwest 0.13** (+ middleware/retry + `governor` rate limiting) | rustls default now reads the macOS keychain roots — corporate certs work; skip `tauri-plugin-http` (pins reqwest 0.12) |
| Frontend | **Svelte 5 + Vite** (no SvelteKit), mockup CSS kept global, Bits UI for primitives, TanStack Virtual for columns/lists | runes ≈ the mockup's imperative logic; region-by-region port via `mount()` |
| Adapters | hand-rolled reqwest per source; exceptions: `gouqi` (Jira, handles Cloud-v3/DC-v2), Gitea client codegen from `/openapi3.v1.json`, `kuma-client` (Kuma config) | a sync app needs 5–15 endpoints, not 400 generated ones |

**Gotchas every implementer must know** (verified in research, encoded as constraints in the plans):

1. PG 18: `GENERATED ALWAYS AS (...)` without `STORED` silently creates an unindexable virtual column — always write `STORED`.
2. sqlx 0.9: dynamic SQL needs `AssertSqlSafe`; keep it in one reviewed query-builder module. Never bind `tsquery` — bind text into `websearch_to_tsquery('english', $1)`, compute the tsquery once as a FROM item. Never map `tsvector` to `String`.
3. macOS Unix-socket path limit (103 bytes) breaks PG sockets under `~/Library/Application Support` — TCP localhost only.
4. Jira Cloud `/rest/api/3/search` is **410 Gone** — `/search/jql` with `nextPageToken`, no `total`.
5. Confluence CQL exists only in the v1 REST API; content via v2.
6. Uptime Kuma v2 prunes raw heartbeats to ~24 h — knobas records its own timeseries from the first poll; metric absence = *unknown*, not down; response time `-1` is a sentinel.
7. `ts_headline` output is not XSS-safe — escape synced HTML before the webview renders it.
8. sqlx migrations need `build.rs` with `cargo:rerun-if-changed=migrations` or the embedded migrator goes stale silently.
9. Tauri: don't `emit` from the `setup` hook (webview not listening yet) — frontend signals ready first. Stop the scheduler and `pg_ctl stop` on `RunEvent::ExitRequested`.
10. Unsigned dev builds re-prompt the keychain on every run — sign locally.

---

## 5. Risks beyond the gotchas

| Risk | Mitigation |
|---|---|
| PG major upgrades (data dir incompatible) | pin `=18.6.0`; upgrade = sequenced dump → initdb → restore at startup on version change (we own it; export format is already a dump) |
| First-run binary download (GitHub rate limit / offline) | pinned version skips release lookup; `bundled` feature is the fallback if offline first-run ever matters |
| `rust_socketio` unmaintained (Kuma config channel) | socket.io isolated behind the adapter trait; `/metrics` alone still covers state + telemetry |
| Flowrun API unknown (internal system) | adapter stream starts with a discovery task against the real instance; the in-repo stub encodes whatever it finds |
| Full-page-rerender habits from the mockup leaking into the port | Svelte components own local state; the mockup is a *behavior* reference, its rendering strategy is explicitly not carried over |
| Scope creep before MVP | M2+ features need a milestone, not a slot in the current one; the design doc's tiering is the arbiter |
| Session/usage limits during agent waves | agreed concurrency before each wave; Opus for agents per established preference; worktree isolation so a killed wave loses nothing merged |

---

## 6. Plan index

| Plan | Covers | Status |
|---|---|---|
| `2026-08-24-plan-01-foundation.md` | M0 | **written**, awaiting review |
| plan-02 … | M1 streams (one plan per stream or one plan with parallel task groups) | write at M1 start |
| plan-03 … | M2 | write at M2 start |
| plan-04 … | M3 | write at M3 start |
| plan-05 … | M4 | write at M4 start |

Plans are written just-in-time so they never argue from a stale spec; each one re-reads the design doc first.
