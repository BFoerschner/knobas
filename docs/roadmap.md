# knobas — roadmap: MVP, milestones, parallel workstreams

**Status:** proposed 2026-08-24 (Claude), adopted, and amended per milestone since (M2.5 added 2026-08-30, M2.6 2026-08-31). Everything here follows from the revised design doc (`docs/specs/2026-08-23-knobas-design.md`) and the 2026-08 technology research (summarized in §Stack below). M0 through M2.5 have shipped, M2.6 is at its exit sweep as of 2026-09-01, and the GitHub milestones carry the live state.

This is the decomposition document: the project is too large for one plan, so it is cut into milestones. Since 2026-08-28, task-level planning runs the mattpocock-skills flow (grilling → spec → tickets in GitHub Issues); the executed superpowers-era plan files (M0/M1) remain in git history — see §6.

---

## 1. What "MVP" means here

Björn's stated pain: **bad source-system search (JQL/Confluence) and credential juggling.** The earliest build that beats the status quo at that is the MVP:

> **MVP = end of M1: sources connected once (credentials in the keychain), everything synced locally, one `⌘K` box that finds any ticket/PR/build/page in <100 ms, context rooms and detail views to read it all in one window.**

That build is read-only toward the sources, has no links UI, no timer, no assets — and is already worth opening every morning. (Since no real Jira/Confluence/TeamCity instance is available during development, every milestone is developed and accepted against the local test environment — §3 — and "start using it daily" begins at the deferred real-system gate, the day real credentials exist.) Everything after M1 makes knobas *knobas* rather than a fast index:

- **M2** is the identity release (links, suggestions, write-back, start-work, inbox).
- **M2.5** delivers the mini board (the Tickets tile's status-grouped rendering, plus a status select in the ticket detail).
- **M2.6** adds project rooms and gives the mini board a second layout, so the room a person stands in decides both what its tiles hold and how its mini board is drawn.
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
| A | **Jira adapter** (read: issues, epics, comments, worklogs; incremental; **DC REST v2 primary** — self-hosted is what Björn runs; Cloud `/search/jql` flavor later) | M0 SPI |
| B | **Gitea adapter** (read: repos, branches, PRs, commits; client generated from the instance's OpenAPI 3 spec or `gitea-sdk`) | M0 SPI |
| C | **TeamCity adapter** (read: builds, configs, queue; ~4 endpoints, `Accept: application/json`, `fields=`) | M0 SPI |
| D | **Frontend shell**: top strip, status bar, room + tiles (read-only), detail slide-over (read-only), sources view, first-run wizard | M0 IPC + seed data |
| E | **Search**: FTS corpus incl. ancestor paths, launcher (prefixes, chips, aliases, empty-query board), grouped results, built-in smart lists (read-only counts) | M0 IPC + seed data |
| F | **Sync engine**: scheduler (per-source interval, *Sync now*), cursors, backoff, 401 detection → credential health, keychain integration, diagnostics view | M0 SPI |
| T | **Test environment** (dispatched first): `knobas-mockd` HTTP mock server (Jira + TeamCity subsets to start) validated against the vendored specs (`testenv/specs/`, already fetched + pinned 08-24; TeamCity's spec extracted from its container per `testenv/specs/fetch.sh`), `testenv/docker-compose.yml` (real Gitea, real Uptime Kuma v2, mockd container, Flowrun stub), `testenv/seed` script that populates Gitea/Kuma with the Tidewater content via their APIs | M0 fixtures |

A/B/C develop against the shared contract battery + `knobas-mockd` run **in-process** in their integration tests (stream T delivers the Jira/TeamCity mocks first; adapters start on unit tests and the battery meanwhile — the vendored specs in `testenv/specs/` are their contract source, no real instances exist during development); D/E develop entirely against the mock source and seed data; F uses the mock's simulated failures. **Integration checkpoint at the end:** `docker compose up` the test environment on this machine and connect the app to it end-to-end — real-container Gitea/Kuma plus mockd Jira/TeamCity; full initial sync; search everything.

**Exit criteria (all against the test environment):** credentials entered once, land in the keychain; initial + incremental sync works; `⌘K` < 100 ms over the synced corpus; rooms and details browsable; sources view shows sync health. **This build is the MVP** — feature-complete for reading; it becomes the daily driver the day it's pointed at real systems (the deferred real-system gate).

### M2 — Links & actions (the identity release; 4–5 streams)

Links UI everywhere (panels, *Link to…*, Tab action chains, `[[refs]]`) over the one link table · suggestion engine (keys in commits/branches/build params/page text, FTS similarity, native source links; reasons; dismissals persisted) + room tray · write-back: Jira status/comment/create, Gitea branch/PR/comment/approve, TeamCity trigger/re-run · write queue with conflict UI (re-read before flush, ask with diff) · **start-work flow** + reverse (PR merged → In Review) · inbox v1 (mentions, review requests, failed builds, assignments, credential expiry; actions; snooze with date) · notes (markdown, `[[…]]` chips, backlinks) · contexts complete (promote ticket, ad-hoc, 1-hop membership, per-context inbox filter) · scheduled **backup export** (`pg_dump` of `knobas`).

**Exit criteria:** ticket→branch→PR round-trip against the test environment (real-container Gitea; mockd Jira transitions); inbox populated and actionable end-to-end; links/suggestions working over the synced corpus; a nightly backup archive exists.

### M2.5 — The mini board (serial, one stream; added 2026-08-30)

The Tickets tile becomes the **mini board** the design doc promised (§2 Shell: "Tickets (mini board)"): status columns over the context's live ticket items, each column headed by its status name and count, cards showing key, priority, and title, clicking through to the ticket detail — plus a status select in the detail slide-over, enqueued as a transition through the existing write queue (optimistic; the source refuses illegal moves by name and the refusal surfaces in the pending/held-write UI). One additive IPC read command is the milestone's only frozen-surface touch (§10.8 entry). Vocabulary per ADR-0009: *mini board*, "board" never unqualified.

**Gate:** opens only at M2 exit (Björn's gate); closes before M3 starts. Nothing in it is `ready-for-agent` before then.

**Exit criteria (against the test environment):** the tile renders the mini board from the granted read, with the empty state; a status move round-trips through the write queue and an illegal move surfaces in the conflict UI; the paperwork is merged (design-doc addendum, this roadmap insertion, ADR-0009, the §10.8 entry). Spec: issue #175.

### M2.6 — Project rooms and the mini board's two layouts (two streams, merged serially; added 2026-08-31)

M2.5 leaves a multi-project Jira with one room for all of it, drawing a mini board that is the union of every workflow's statuses in a tile too narrow to hold it — the problem ADR-0010 states, and the one this milestone answers twice over. A **project** (`CONTEXT.md`, **Project**; ADR-0010) becomes a scoping dimension a room can be built on: the switcher grows a room for every project a source's corpus shows, listed under that source's own room, and standing in one narrows every tile in the room rather than the Tickets tile alone. And the **mini board** gains a second layout, chosen by the room: a bounded room (a project room, a stored context) keeps M2.5's column layout, a definitionally unbounded room (*All work*, a source room) draws the same status groups stacked one under another and scrolling down, and a demote-only backstop at six columns keeps a bounded room that turns out to span two workflows off a horizontal scrollbar. Two frozen-surface touches — a project dimension on the room filter, and an additive read reporting the projects a corpus shows — carry a §10.8 entry each, written with the changes that make them. Vocabulary per ADR-0009 and ADR-0010: *mini board*, "board" never unqualified; *project* per the glossary, never a synonym for *context*.

**Gate:** opens only at M2.5 exit (Björn's gate); closes before M3 starts. Nothing in it is `ready-for-agent` before then.

**Exit criteria (against the test environment, except where the demo profile is named):** under every source room whose corpus shows projects — mockd Jira and mockd TeamCity both do — the switcher lists a room per project, labelled by the project's own name, and by its key where the project reports no readable name; standing in one narrows every tile in the room, and its mini board draws that project's workflow alone in columns, demoting to the stacked layout where that workflow turns out to draw more than six; *All work* and a source room show the same statuses, in the same order, with the same counts and cards, stacked vertically, and the terminal "No status" group is last in both layouts; a ticket whose mirrored record carries no readable project is still reachable in *All work* and in its source's room, appears in no project room, and there is no "No project" room; a bookmarked address for a project the corpus no longer shows lands in *All work*; `just demo` shows project rooms over the Tidewater dataset with nothing configured; the paperwork is merged (design-doc addendum, this roadmap insertion, the two §10.8 entries). Spec: issue #188.

### M3 — Time & the daily flow (3–4 streams)

Global timer (`⌘T`, any entity or ad-hoc label, switch-on-context-change) · worklog draft (concatenated intervals, activity checkboxes, generated comment) → Jira · ad-hoc block dialog · passive attribution (opt-in) · day review strip · week timesheet with correct *Log all* · standup digest (from the activity stream, traceable lines) + protocol (save as note / publish) · **Confluence adapter** (read + CQL search via v1, create-from-template, comment, macro-free section edit) · desktop notifications.

**Exit criteria:** a full workday tracked, reviewed, and logged end-to-end against the test environment (worklogs land in mockd Jira and read back; standup protocol published to mockd Confluence and read back).

### M4 — Assets, monitoring, estate (4–5 streams)

Asset model (infinite tree, typed+custom properties, routes, relations, env/owner inheritance, history) · Miller columns UI + spines + wires + pane (+ "Depends on this" panel) · **Uptime Kuma adapter** (v2: `/metrics` poll → own timeseries; `kuma-client` for config; create/pause write-back) · monitors tab, alerts → inbox with routing rule + ack semantics · **Flowrun adapter** (instances, scenarios, Run/Log/**Promote**) + tab · asset smart lists + "save search as list" · **export/import complete** (share export, merge-restore with preview) · seed-fixture extension for assets.

**Exit criteria:** an estate modeled and browsable (the Tidewater assets, plus Björn's real infrastructure where it exists locally); an alert flows from the real Kuma container → inbox → ack; a scenario promoted against the Flowrun stub; export restores on a clean machine.

### v1.5 — fast follows (any order, one stream each)

Open in editor/terminal · paste-URL → entity chip · quick capture hotkey · environment-matrix view · remaining §13 backlog by demand.

---

## 3. Working model for parallel agents

Moved to `docs/agents/working-model.md` (2026-08-28): the PR loop, concurrency limits, review economics, branching model, and the test strategy.

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
4. **The primary Jira/Confluence dialect is Data Center (self-hosted), not Cloud** — Jira DC REST v2 (`/rest/api/2/search`, `startAt` pagination; the Cloud `/search` 410-removal never applied to DC) and Confluence DC REST v1 (`/rest/api/content` + CQL; no v2 API exists on DC). The `flavor: datacenter|cloud` field in the source config selects the dialect; Cloud (`/search/jql`, `nextPageToken`, no `total`) comes later.
5. Confluence Cloud's CQL likewise exists only in its v1 REST API — on both flavors, CQL is the search path.
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
| Flowrun API unknown (internal system, no instance available during development) | the mockd stub *defines* the assumed contract and the adapter is kept deliberately thin; both get validated and adjusted at the real-system gate when an instance exists |
| No real Jira/Confluence/TeamCity during development — mocks could drift from reality | strongest available contract per API, vendored + checksum-pinned in `testenv/specs/` (Jira DC WADL, TeamCity swagger-from-container, Cloud OpenAPI for later); **Confluence DC has no published machine-readable spec** → validated against the real `atlassian/confluence` container (`--profile real-atlassian`); real-system gate re-runs every milestone checklist once real instances exist |
| ~~Björn's exact DC versions unknown~~ — **closed 2026-08-31 (#49)** | Björn ruled "use the latest datacenter versions", then "the ones demoable with timebomb licences" once it emerged the two differ: `real-atlassian` is pinned to Jira Software **10.3.24** and Confluence **9.2.21**, the newest LTS releases a published timebomb key starts (Jira 11.x is reported to reject it). Self-service DC trials ended 2026-03-30, so timebomb — 10 user, 3 hours — is the only free licence left. The WADL half turned out to be unavailable rather than unknown: Atlassian publishes none past 9.17.x, so `jira-dc-rest.wadl` stays at 9.17.0 and the container is deliberately newer (`testenv/pin-images.sh`) |
| Fidelity tests for the vendored specs cover the mocks, not the adapters' assumptions about *behavior* (ordering, defaults, permissions) | the `--profile real-atlassian` containers exist precisely to catch behavioral drift; run them at milestone exits |
| Full-page-rerender habits from the mockup leaking into the port | Svelte components own local state; the mockup is a *behavior* reference, its rendering strategy is explicitly not carried over |
| Scope creep before MVP | M2+ features need a milestone, not a slot in the current one; the design doc's tiering is the arbiter |
| Session/usage limits during agent waves | agreed concurrency before each wave; Opus for agents per established preference; worktree isolation so a killed wave loses nothing merged |

---

## 6. Where task-level planning lives now

The superpowers plan files were retired 2026-08-28 (mattpocock-skills flow only: grilling → spec → tickets). Task tracking is GitHub Issues, milestone **M2**, then **M2.5**, then **M2.6**. The executed M0/M1 plans and the M1 carry-over ledger remain readable in git history: `git show 736ac1f:docs/superpowers/plans/<file>`.
