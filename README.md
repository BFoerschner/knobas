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

Prerequisites:

- [rustup](https://rustup.rs) — the toolchain itself is pinned to **1.94** by
  `rust-toolchain.toml` and installed on first build.
- Node **20.19+ / 22.12+** for the frontend, which is what Vite 8 requires.
- [`just`](https://github.com/casey/just) — every command below is one of its
  recipes, and nothing in the repo provisions it: `brew install just`, or
  `cargo install just`.

```bash
just deps     # npm ci in app/, on a fresh clone and whenever the lockfile moved
just check    # fmt + svelte-check + vite build + clippy -D warnings + tests
              # (also what CI runs on every PR)
just dev      # the desktop app against its own embedded Postgres
just demo     # the same app in the demo profile (own directory, database,
              # keychain) — the only profile Load demo data works in
```

The first `cargo test` (and the first `just dev`) **downloads PostgreSQL 18.6
once** into the shared `~/.theseus/postgresql/<version>` and runs `initdb`;
expect that run to take a few minutes and to need network. Every later run
reuses it.

`just check` is the whole quality gate: `cargo fmt --check`, `svelte-check` and
`vite build`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo clippy --workspace --lib -- -D warnings`, and `cargo test --workspace`.
Nothing is pushed without it green.

The clippy pass runs twice on purpose. `--all-targets` pulls in every crate's
dev-dependencies and cargo unifies features across the invocation, so
`knobas-db`'s self-dev-dependency (which turns on `test-util`) silently enables
that feature for the *library* build too — linting it in a configuration
nothing ships. The `--lib` pass drops the dev-dependencies, and with them the
feature, so each library is also checked the way something that merely depends
on it will build it.

GitHub Actions runs this same `just check` on every pull request and on every
push to `main` (`.github/workflows/check.yml`). CI deliberately does not
restate the gate — it only provides a Linux machine to run it on, so the two
cannot drift apart.

Setting `KNOBAS_DB_URL` points **the app** at a Postgres you manage instead of
starting an embedded one — for developing against a server with real data in
it. It has no effect on `just check`: the tests always stand up their own
throwaway server, so it is not a way to skip the download above.

## Profiles

`just dev` runs the **default** profile: the real data directory, the real
database, the real keychain service. `just demo` runs the **demo** profile,
which is that same application pointed at its own data directory — and
therefore its own embedded server on its own port — with its own keychain
service, so fixture data can never mix into a real corpus and a demo run is not
even offered the real credentials. Only in the demo profile does **Load demo
data** work; elsewhere the command refuses and says to start knobas with
`--demo`. The reasoning is in the design doc (§14a) and the ruling is interfaces
§8 P13.

There is no source to configure yet, which is what the demo profile is for.
Press **Load demo data** in the title bar: it registers the compiled-in `mock`
source and syncs the Tidewater Freight fixture
(`fixtures/tidewater/work.json`) — the same dataset every mockup was drawn
against — then reports how many items were upserted. The button is idempotent;
pressing it twice rewrites the same rows. Afterwards, searching for `sepa retry`
finds `mock:PAY-231` in the `ticket` group.

## Sync

Each source has an interval measured from the **end** of its previous run
(default five minutes), and the scheduler picks up whatever is due. *Sync now*
returns as soon as the run is recorded — the UI never waits on a source.

Every run holds a database connection of its own, outside the pools the rest of
the app uses, and at most three run at a time; a slow source therefore cannot
take a connection the window needs, however long the remote system takes to
answer. A failure backs the source off 1 → 2 → 5 → 15 → 60 minutes. A 401 does
not back off at all: only re-entering the credential can fix one, and retrying a
rejected credential on a timer is how an account gets locked out.

`KNOBAS_SECRET_STORE=memory` keeps credentials out of the OS keychain for a
throwaway run — they then live for the length of the process.

## Shutting down

Quitting the app (Cmd-Q, or closing the last window) stops the sync scheduler,
cancelling whatever is in flight, and then stops the embedded server. Both are
bounded: a quit during a thirty-second remote call does not wait it out.

A signal does not: Ctrl-C under `just dev`, a `kill`, or a crash leaves the
postmaster running.

That orphan needs no cleanup — **just start knobas again**. The next start
finds the server already serving its data directory and *adopts* it rather than
fighting it for the lock, which also makes it a warm start. Since M1 it also
takes *ownership* of it: the adopting process holds the same lock the starting
one would, so the next clean quit stops the server for good. (A server a
**live** sibling instance owns is never stopped by an adopter — two windows on
one profile share it, and the one that started it is the one that stops it.)

To stop one by hand, the server's own `pg_ctl` is the one that works — it is not
on `PATH`, and it needs the data directory:

```bash
~/.theseus/postgresql/18.6.0/bin/pg_ctl \
  -D "$HOME/Library/Application Support/dev.knobas.desktop/db/data" stop
```

The demo profile keeps its server one level down, under
`dev.knobas.desktop/demo/db/data`; running both profiles leaves two postmasters.

A reboot does the same thing.

## Crate map

| Crate | What it is |
| --- | --- |
| `crates/knobas-core` | The domain: entity addressing (`<namespace>:<key>`), links, the activity stream, and the stores that read and write them. |
| `crates/knobas-db` | The database: embedded PostgreSQL lifecycle (start / adopt / stop) and the embedded migrations. |
| `crates/knobas-source` | The adapter SPI — the `Source` trait, the contract battery, and the plain serde data every adapter exchanges. No database dependency, on purpose. |
| `crates/knobas-source-mock` | The reference adapter: serves the Tidewater Freight fixture, talks to nothing. |
| `crates/knobas-secrets` | *(new in M1)* The OS-keychain credential store behind a trait, with an in-memory store for tests and CI. Nothing secret ever reaches Postgres, and nothing reads a secret back over the bridge. |
| `crates/knobas-search` | *(new in M1)* The launcher's read side: the FTS query over `sync.live_item`, in the `SearchQuery` → `SearchResponse` shape, with snippets as segments. |
| `crates/knobas-http` | *(new in M1)* The HTTP transport the three real adapters share: one reqwest stack, retry classes, rate limiting, and the status → `SourceError` mapping. |
| `crates/knobas-sync` | The sync engine **and scheduler**: one run in one transaction (pull from a `Source`, upsert into `knobas.entity` and `sync.item`, advance the cursor), the full-sync sweep, cursors, backoff, credential health, the per-run log, and the ticker that decides when. |
| `crates/knobas-app` | The Tauri shell: window, app state, database lifecycle, profiles, and the IPC commands. |
| `app/` | The frontend — Svelte 5 runes on plain Vite, TypeScript strict. `app/src/lib/ipc/` mirrors the command surface, one file per Rust command module. |

## Where the documents live

- **Design doc** — `docs/specs/2026-08-23-knobas-design.md`: the
  feature-by-feature walkthrough, and the thing to change before changing
  behaviour.
- **Roadmap** — `docs/roadmap.md`:
  milestones M0–M4, which streams may run in parallel, and the working model for
  parallel agents (§3).
- **Task tracking** — GitHub Issues (milestone `M2` onward); the executed M0/M1
  plan files live in git history (`git show 736ac1f:docs/superpowers/plans/<file>`).

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
