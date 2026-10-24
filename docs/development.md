# Development

Working notes for building, testing and releasing knobas. The README has the
quickstart. `testenv/README.md` covers the live suites against real products.

## Toolchain

- `rust-toolchain.toml` pins Rust 1.99. The `just` recipes run cargo with
  `RUSTUP_TOOLCHAIN` unset, so an exported override cannot beat the pin. Raw
  `cargo` commands need `env -u RUSTUP_TOOLCHAIN` for the same effect.
- Node 20.19+ or 22.12+, which Vite 8 requires. `just deps` runs `npm ci` in
  `app/` when `node_modules` is missing or older than the lockfile.
- Nothing in the repo installs `just`: `brew install just` or
  `cargo install just`.

The first test run or `just dev` downloads PostgreSQL 18.6 once into the shared
`~/.theseus/postgresql/<version>` and runs `initdb`. Later runs reuse it.

## The gate

`just check` is the quality gate. Under the ticket loop the merge-manager runs
it once, on the head it merges (`just gate`, below). It first
fails on a stale contract index (`scripts/contract-index.py --check`) or a
table below that lost step with the tree (`scripts/doc-lists.py`), then
runs three halves side by side:

- `front`: `svelte-check`, the Vitest suite and `vite build`.
- the scripts: `shell`, `test-layout`, `witness-unit` and `conflict-markers`.
- the cargo chain: `fmt`, `clippy`, `clippy-libs`, `inventory`, `ipc-mirror`
  and `test`. `test` takes a slot under `$TMPDIR/knobas-gate-slots` (one test
  run at a time since #1892, four libtest threads per binary) and starts its
  tests against one shared test server, not a Postgres per test.

Each half's output is captured and printed when it finishes. A failing half
prints its whole log. A passing one prints each recipe's wall time, and the
cargo half adds `test`'s build and run times and its ten slowest test binaries.

`just gate` runs `check-front` instead on a frontend-only head: the
contract-index and doc-list checks, the conflict-marker scan, `svelte-check`, the Vitest files the change can reach
(`just front-changed`) and `vite build`.

`just front` alone is the fast loop for frontend work. `just pre-gate` formats,
refreshes the inventory and the contract index, and runs a few quick checks.
It does not replace the gate. `just docs-check` is the three cheap checks alone
(contract index, doc lists, conflict markers) for a docs-only change.

`scripts/doc-lists.py` holds four hand-written tables to the tree, both ways:
*Crates* and *Frontend* below (every directory under `crates/` and
`app/src/lib/`, and every path a row names), and in `testenv/README.md` the
live-suite recipes (every `<x>-live` recipe) and the `live-up` products (every
product `testenv/live-lib.sh`'s `_live_main` knows). A crate added without its
row fails the gate.

Clippy runs twice on purpose. `clippy` uses `--all-targets`, which pulls in
every crate's dev-dependencies. Cargo unifies features across one invocation,
and `knobas-db` dev-depends on itself with `test-util` on, so that pass lints
the library with a feature nothing ships. `clippy-libs` uses `--lib`, which
drops the dev-dependencies and checks each library the way a dependent builds
it.

Under `just test` (and so `just check`) every test binary shares one throwaway
Postgres, `knobas-test-server`, and gets a database of its own on it; a
`cargo test -p <crate>` by hand, with `KNOBAS_TEST_DB_URL` unset, starts one per
binary. `KNOBAS_DB_URL` points the
app at a server you manage. It has no effect on the tests.

## CI

Every workflow in `.github/workflows/` runs on `workflow_dispatch` only.
Nothing runs on push, pull request or tag, so `gh pr checks` reports no checks.
The local `just check` is the gate. `docs/agents/working-model.md` says who
runs it and when.

- `check.yml` runs the same `just check` on Linux. It does not restate the
  gate, so the two cannot drift.
- `testenv.yml` lints the test environment's scripts and compose file without
  starting containers. Run it after touching `testenv/`.

Both are disabled since 2026-10-24 (Björn: the runner is too slow; the
checks run on the Mac). `gh workflow enable <file>` brings one back
(`docs/decisions/2026-10-24-bjorn-rulings.md`).

```bash
gh workflow run check.yml --ref <branch>
gh workflow run testenv.yml --ref <branch>
```

## Releases

`release.yml` bundles the app on each platform's own runner. A Tauri bundle
links the platform's webview, so a Mac cannot cross-compile the others. Tag
first, then dispatch the tag:

```bash
git tag -a v1.2.0 -m "..." && git push origin v1.2.0
gh workflow run release.yml --ref main -f tag=v1.2.0 -f platforms=all
```

| `platforms` | runner | artifacts | billed at |
|---|---|---|---|
| `macos` | `macos-latest` | `.dmg` (arm64 only) | 10x |
| `windows` | `windows-latest` | `.exe` (NSIS), `.msi` (WiX) | 2x |
| `linux` | `ubuntu-latest` | `.deb`, `.rpm`, `.AppImage` | 1x |

Name one platform when only one is in question. macOS minutes cost ten times
Linux minutes.

The workflow creates the release as a draft. Publish it once the legs you
wanted have finished. Nothing is signed. macOS reports the app as damaged until
you run `xattr -dr com.apple.quarantine /Applications/knobas.app`, and Windows
shows a SmartScreen warning. The Linux artifacts carry the runner's glibc
floor.

## Profiles

`just dev` runs the default profile: the real data directory, database and
keychain service. `just demo` runs the same app with `--demo`. The demo profile
has its own data directory, its own embedded server on its own port and its own
keychain service, so fixture data never mixes with a real corpus and a demo run
never sees real credentials. Spec §14a has the reasoning.

Only the demo profile can load demo data. The load registers the compiled-in
`mock` source, syncs the Tidewater Freight fixture
(`fixtures/tidewater/work.json`) and imports `testenv/estate.json`. Loading
again rewrites the same rows. Outside the demo profile the command refuses and
says to start knobas with `--demo`.

`KNOBAS_SECRET_STORE=memory` keeps credentials out of the OS keychain for a
throwaway run. They then last as long as the process, and a source's page
says *Stored for this session only* instead of naming the keychain.

## Sync

Each source syncs on an interval measured from the end of its previous run.
The default is five minutes. *Sync now* returns as soon as the run is recorded.

Every run holds its own database connection, outside the pools the window
uses, and at most five runs go at once. A slow source cannot take a
connection the UI needs. A failure backs the source off 1, 2, 5, 15, then 60
minutes. A 401 does not retry at all: only a new credential fixes it, and
retrying a rejected credential on a timer can lock the account.

Queued writes follow the same rules. A write to a source that did not answer
is tried again on the same ladder. A write whose credential was rejected
waits until a credential is saved for that source, its address or
configuration is edited, or the reader presses *Flush now*.

## Shutting down

Quitting the app (Cmd-Q, or closing the last window) stops a running clone and
removes its half-made folder, stops the sync scheduler, cancels in-flight runs
and stops the embedded server, also while the boot screen is still up. Every
step is bounded, so a quit does not wait out a slow remote call.

A write knobas is sending when you quit gets up to three seconds to finish, so
it is marked sent. This includes the write you wait on after *Flush now*, *Send
mine anyway* or *Edit and send*. A write you make after the quit began is
queued and not sent; the next start sends it. If a send is still going after
three seconds, knobas abandons it and holds the write, because the source may
already have it. You check the source, then choose *Send again* or *Discard*;
the next start does not send it again on its own. A write that a second send
cannot duplicate, such as pausing or resuming a monitor, stays pending and goes
at the next start. A start-work flow works the same way: the step being sent
gets the three seconds, the flow stops before its next step, and a *Run*
pressed after the quit began starts nothing. *Run* at the next start carries
the flow on.

A signal or crash (Ctrl-C under `just dev`, `kill`, a reboot) leaves the
postmaster running. Start knobas again. The next start adopts the running
server and takes ownership of it, so the next clean quit stops it.

A second launch on the default profile focuses the running instance and exits
before it touches the database. A second `--demo` launch is not caught that
way, because the guard would also keep a demo from running beside a real
instance. It opens a window, and the boot screen says the demo profile is in
use and names the knobas process to quit. It touches nothing on the way. Any
other second launch that gets past the guard on a live profile ends the same
way.

If the postmaster dies while knobas runs (a crash, a `pg_ctl stop` by hand),
the window notices within a few seconds and shows the boot screen with
*Retry*. *Retry* starts the server again on the same port and data directory.
The window keeps the profile meanwhile, so a second launch is still refused.

To stop an orphaned server by hand, use its own `pg_ctl`. It is not on `PATH`:

```bash
~/.theseus/postgresql/18.6.0/bin/pg_ctl \
  -D "$HOME/Library/Application Support/dev.knobas.desktop/db/data" stop
```

The demo profile keeps its server under `dev.knobas.desktop/demo/db/data`.

## Frozen surfaces

The `Source` trait, the migration baseline and the IPC schema are frozen.
Changing any of them takes an orchestrator decision, a spec update and an entry
under `docs/contract/10.8/`. `docs/contract.md` is the record.

## Crates

| Crate | What it is |
| --- | --- |
| `knobas-core` | The domain: entity addressing (`<namespace>:<key>`), links, the activity stream and their stores. |
| `knobas-db` | Embedded PostgreSQL lifecycle (start, adopt, stop), the connection pool and the migrations. |
| `knobas-source` | The adapter SPI: the `Source` trait, the contract battery and the serde data adapters exchange. It has no database dependency. |
| `knobas-spi-types` | The types the adapter SPI shares with the core: payload declarations, entity references, tones and how a write is delivered. No database, no async runtime: it and `knobas-source` build for `wasm32-wasip2`, so an external adapter can depend on them. `knobas-core` re-exports it under the old paths (#1234). |
| `knobas-http` | The HTTP stack the adapters share: one reqwest client, retry classes, rate limiting and the status to `SourceError` mapping. The write queue in `knobas-sync` also reads its local-quota mark (#1729). |
| `knobas-source-jira` | Jira Data Center adapter, with status, comment and create writes. |
| `knobas-source-confluence` | Confluence Data Center adapter, with page writes. |
| `knobas-source-gitea` | Gitea adapter: repositories, branches, pull requests, commits. |
| `knobas-source-teamcity` | TeamCity adapter: build configurations and builds, with trigger and re-run. |
| `knobas-source-kuma` | Uptime Kuma adapter: monitors from `/metrics`; pause, resume and create over socket.io with an account. |
| `knobas-source-mock` | The reference adapter. It serves the Tidewater fixture and talks to nothing. UI and engine tests use it. |
| `knobas-wasm-host` | The WASM host (ADR-0028): loads an external adapter, a component built against `wit/adapter.wit`, runs it under wasmtime with a deadline and memory cap on every call and `wasi:http` held to its manifest's hosts, and hands it to the rest of knobas as a `dyn Source` (#1198). |
| `knobas-adapter-sdk` | The Rust SDK of the adapter standard (#2028): the `knobas:adapter` bindings, the SPI types re-exported from `knobas-source`, and the `Adapter` trait an external adapter implements, with the WIT conversions and the `export!` macro inside. Its version is the interface version it binds (1.3.0), not the workspace's. |
| `knobas-wasm-fixture` | Test only. The fixture external adapter the WASM host is tested with: a fixed corpus with a cursor, built to a component by `just wasm-fixture` (#1198), on `knobas-adapter-sdk`. No external adapter ships with knobas. |
| `knobas-mockd` | Deprecated and frozen (ADR-0013). An HTTP mock of Jira and TeamCity, kept as a dev-dependency for older tests until the live suites cover them. |
| `knobas-live-cleanup` | Dev-dependency only. The one runner every live suite's `Drop` guard hands its cleanup to: a thread and runtime of its own, a bound on the whole sequence, and a report that cannot abort a test that is already unwinding (#1913). |
| `knobas-secrets` | The OS-keychain credential store behind a trait, with an in-memory store for tests. No secret reaches Postgres or crosses the bridge. |
| `knobas-search` | The launcher's read side: full-text search over the mirror in the `SearchQuery` to `SearchResponse` shape. |
| `knobas-sync` | The sync engine and scheduler: one run per transaction, cursors, backoff, credential health, the run log, and the write queue. |
| `knobas-app` | The Tauri shell: window, app state, database lifecycle, profiles and the IPC commands. |

## Frontend

`app/` is Svelte 5 runes on plain Vite (no SvelteKit), TypeScript strict with
`exactOptionalPropertyTypes` and `noUncheckedIndexedAccess`.

| Path | What lives there |
| --- | --- |
| `app/src/App.svelte` | The root: the boot gate, the first-run wizard, then the shell. |
| `app/src/lib/tf/tf.css`, `app/src/lib/tf/css/tokens.css` | The global stylesheet and the design tokens (`tf/css/` holds one stylesheet per page). |
| `app/src/lib/tf/` | The shell since #881: `TfShell.svelte`, and one folder per page. Routes are `tf/route.ts` (`parseTfHash`, old addresses rewritten in `tf/address.ts`), each folder's `page.ts` registers its page, `tf/pages.ts` collects them. |
| `app/src/lib/shell/` | Lifecycle, keyboard, toasts, the write queue and shared primitives. Its `router.ts` is the pre-#881 parser, still used by the asset tree and notification clicks; new routes go in `tf/route.ts`. |
| `app/src/lib/detail/` | Link helpers and dialogs for an item's detail (`LinksPanel`, `LinkDialog`, `links.ts`, `relations.ts`) and the `view/` folder; there is no slide-over component. |
| `app/src/lib/sources/` | `FirstRun.svelte` (the first-run wizard) and the helpers behind it: account, connection, diagnostics, schema-form and fixtures. There is no sources view. |
| `app/src/lib/launcher/` | The ⌘K overlay. It binds its own hotkey and handles its own `Esc`. |
| `app/src/lib/ipc/` | Hand-written mirrors of the command surface, one file per Rust command module in `crates/knobas-app/src/commands/`. |
| `app/src/lib/{assets,capture,inbox,standup,start-work,time}/` | Feature views. Notes and settings are pages under `tf/` (`tf/note/`, `tf/settings/`). |

Three module-level runes hold state several trees read:
`shell/health.svelte.ts` (live credential health), `shell/kind-registry.svelte.ts`
(what installed adapters declare about their kinds) and
`shell/toasts.svelte.ts`. Each exports a `create…` factory so a test can build
its own without a Tauri bridge.

### House rules

`lib/shell/house-rules.test.ts` and `lib/shell/a11y.test.ts` enforce these by
scanning the sources on disk, so they cover code no test mounts. Each one
breaks only in a bundled build: `tauri dev` loads the Vite server over
`http://`, which gets no CSP header.

- No inline styles: no `style="…"`, no `style:` directive, no
  `setAttribute("style", …)`. `style-src 'self'` blocks them in a bundle. Use a
  class or a native `<progress>`.
- No `{@html}`. All displayed text comes from source systems, and
  `ts_headline` output is not XSS-safe.
- No network at runtime. `default-src 'self'`, and fonts are bundled from npm.
  Loopback and the RFC 2606 reserved names are the only exemptions.
- Every `listen()` is cleaned up, including when the component unmounts before
  the promise resolves. `lib/shell/residue.test.svelte.ts` mounts each
  component with an effect twice and fails on leftovers.
- No data call before `db.state === "ready"`. Commands that need `AppState`
  reject with `not_ready` during start-up, and the shell gates on the
  lifecycle.
- Secrets are never displayed, logged or read back. No command returns one.
- Contrast is at least 4.5:1 and reduced motion is respected (spec §14).
  `lib/tf/tf-css.test.ts` checks the palette and the motion rules.

### QA without Tauri

Any screen runs in a browser with a fake behind `invoke`. `just shot` starts
the dev server on its own port (`5300 + slot`) and captures a route under
`?fake-ipc`:

```bash
just shot 3 /tmp/shots '#/tickets'
```

`?fake-ipc` turns the fake on. It only exists in the dev server: a production
build drops it, and `vite preview` then sits on "Starting the local database".
`?fake-db=starting|migrating|failed` holds the boot screen on one state. This
checks layout and interaction. `just dev` or `just demo` checks the bridge.

### Mockups

`mockups/ticket-first/` holds the clickable HTML mockups, `prototype-1.html`
to `prototype-6.html`, with the brief in `requirements.md`; `mockups/shared/`
holds the shared notes (`assets.md`, `capabilities.md`, `dataset.md`).
`prototype-6.html` is the behaviour and layout reference. Its CSS became the
global stylesheet. Its rendering approach (string templates and one
`innerHTML` swap per frame) did not carry over.
