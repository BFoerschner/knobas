# knobas — roadmap

**Status:** every planned milestone, M0 through v1.5, is closed; `v1.0.0` was tagged at the M4 exit and v1.5 closed 2026-09-11. Ongoing work is GitHub Issues. The milestone narrative is archived verbatim in `docs/archive/roadmap-2026-08.md`; §4 stays here because code cites its gotchas by number, and the section numbers are kept so those citations resolve.

## 1. What "MVP" means here

Archived (`docs/archive/roadmap-2026-08.md` §1).

## 2. Milestones

Archived (§2): M0 to M4, v1.5 and its follow-ups, all closed.

## 3. Working model for parallel agents

`docs/agents/working-model.md`.

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

Archived (§5); every row is closed or carried by ADR-0001 and ADR-0013.

## 6. Where task-level planning lives now

GitHub Issues, through the ticket loop (`.claude/commands/ticket-loop.md`).
