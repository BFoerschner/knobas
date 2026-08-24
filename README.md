# knobas

knobas is a personal work cockpit: a desktop app (Rust + Tauri) that pulls Jira,
Confluence, Gitea, TeamCity, Uptime Kuma and Flowrun into one local Postgres, so
one full-text search covers all of them at once. On top of that index it keeps
the things no single source owns — links between tickets / branches / PRs /
builds / pages / notes, local notes, contexts, and time tracking whose worklogs
are assembled from what you actually did.

Status: **M0 (foundation) is complete.** The app starts an embedded Postgres,
runs its migrations, syncs the built-in Tidewater Freight fixture through the
`Source` SPI, and searches the result. Everything else in the paragraph above is
M1 and later — see the roadmap.

## Dev quickstart

Prerequisites: [rustup](https://rustup.rs) (the toolchain itself is pinned to
**1.94** by `rust-toolchain.toml` and installed on first build) and Node
20.19+ / 22.12+ for the frontend, which is what Vite 8 requires.

```bash
just deps     # npm ci in app/, only when the lockfile moved
just check    # fmt + svelte-check + vite build + clippy -D warnings + tests
just dev      # the desktop app against its own embedded Postgres
```

The first `cargo test` (and the first `just dev`) **downloads PostgreSQL 18.6
once** into the shared `~/.theseus/postgresql/<version>` and runs `initdb`;
expect that run to take a few minutes and to need network. Every later run
reuses it. Set `KNOBAS_DB_URL` to point the app at a Postgres you manage
instead of starting an embedded one.

`just check` is the whole quality gate and is what CI would run; nothing gets
pushed without it green.

## Demo mode

There is no source to configure yet. Press **Load demo data** in the title bar:
it registers the compiled-in `mock` source and syncs the Tidewater Freight
fixture (`fixtures/tidewater/work.json`) — the same dataset every mockup was
drawn against — then reports how many items were upserted. The button is
idempotent; pressing it twice rewrites the same rows. Afterwards, searching for
`sepa retry` finds `mock:PAY-231` in the `ticket` group.

## Shutting down

Quitting the app (Cmd-Q, or closing the last window) stops the embedded server.
A signal does not: Ctrl-C under `just dev`, a `kill`, or a crash leaves the
postmaster running. The next start **adopts** that orphan and reuses it as a
warm start rather than fighting it for the data directory — but an adopted
server is not owned, so from then on no clean quit stops it. Owning that
properly is M1 stream F; until then, `pg_ctl stop` or a reboot is the manual
escape.

## Crate map

| Crate | What it is |
| --- | --- |
| `crates/knobas-core` | The domain: entity addressing (`<namespace>:<key>`), links, the activity stream, and the stores that read and write them. |
| `crates/knobas-db` | The database: embedded PostgreSQL lifecycle (start / adopt / stop), the embedded migrations, and the FTS query. |
| `crates/knobas-source` | The adapter SPI — the `Source` trait and the plain serde data every adapter exchanges. No database dependency, on purpose. |
| `crates/knobas-source-mock` | The reference adapter: serves the Tidewater Freight fixture, talks to nothing. |
| `crates/knobas-sync` | The sync engine — one run in one transaction: pull from a `Source`, upsert into `knobas.entity` and `sync.item`, advance the cursor. |
| `crates/knobas-app` | The Tauri shell: window, app state, database lifecycle, and the M0 IPC commands. |
| `app/` | The frontend — Svelte 5 runes on plain Vite, TypeScript strict. `app/src/lib/ipc.ts` mirrors the command surface. |

## Where the documents live

- **Design doc** — `docs/superpowers/specs/2026-08-23-knobas-design.md`: the
  feature-by-feature walkthrough, and the thing to change before changing
  behaviour.
- **Roadmap** — `docs/superpowers/plans/2026-08-24-knobas-roadmap.md`:
  milestones M0–M4, which streams may run in parallel, and the working model for
  parallel agents (§3).
- **Implementation plans** — `docs/superpowers/plans/`, one file per milestone
  (`2026-08-24-plan-01-foundation.md` is M0).

From the end of M0 the **`Source` trait, the migration baseline, and the IPC
schema are frozen**: changing any of them takes an orchestrator decision plus a
design-doc update (roadmap §3).

## Mockups

`mockups/` holds the design rounds: clickable HTML mockups with shared example
data, used to choose the UI before the app is built. `mockups/shared/` is the
brief every mockup is built from.

`mockups/playground/` holds standalone idea pages outside the rounds (e.g.
`topology-explorer.html`: infinitely nested assets, click-to-create, routes as
wires).
