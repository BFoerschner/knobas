# Development

Working notes for building, testing and releasing knobas. The README has the
quickstart, [how-it-works.md](how-it-works.md) has the architecture and its
pictures, and `testenv/README.md` covers the live suites against real products.

The commands you will use most:

```bash
just deps     # the frontend's packages, when they are missing or stale
just dev      # the app, on your real data directory
just demo     # the app on fixture data, in a profile of its own
just front    # the fast loop for frontend work
just check    # the whole gate
```

## Toolchain

- `rust-toolchain.toml` pins Rust 1.99. The `just` recipes that call cargo run
  it with `RUSTUP_TOOLCHAIN` unset, so an exported override cannot beat the
  pin. `just dev` and `just demo` go through `tauri dev` and inherit the
  variable. Raw `cargo` commands need `env -u RUSTUP_TOOLCHAIN` for the same
  effect.
- Node 20.19+ or 22.12+, which Vite 8 requires. `just deps` runs `npm ci` in
  `app/` when `node_modules` is missing or older than the lockfile.
- Nothing in the repo installs `just`: `brew install just` or
  `cargo install just`.
- The gate also needs `python3`, `jq`, and either shellcheck 0.11.0 or Docker
  to run its pinned image. `just readme-diagrams` needs d2 0.9.0 or newer, and
  `just shot` needs Chrome.

The first test run or `just dev` downloads PostgreSQL 18.6 once into the shared
`~/.theseus/postgresql/<version>` and runs `initdb`. Later runs reuse it.

## The gate

`just check` is the quality gate. Under the ticket loop the merge-manager runs
it once, on the head it merges (`just gate`, below). It first
fails on a table below that lost step with the tree (`scripts/doc-lists.py`) and on
a Markdown link that names nothing (`scripts/doc-links.py`), then runs three halves
side by side:

- `front`: `svelte-check`, the Vitest suite and `vite build`.
- the scripts: `shell`, `test-layout`, `witness-unit` and `conflict-markers`.
- the cargo chain: `fmt`, `clippy`, `clippy-libs`, `test-build`, `ipc-mirror`
  and `test`. `test` takes a slot under `$TMPDIR/knobas-gate-slots` (one test
  run at a time since #1892, four libtest threads per binary) and starts its
  tests against one shared test server, not a Postgres per test.

Each half's output is captured and printed when it finishes. A failing half
prints its whole log. A passing scripts or cargo half prints each recipe's
wall time, and the cargo half adds `test`'s build and run times and its ten
slowest test binaries. A passing `front` prints the totals of `svelte-check`,
Vitest and the build.

`just gate` runs `check-front` instead on a frontend-only head: the
doc-list and doc-link checks, the conflict-marker scan, `svelte-check`, the Vitest
files the change can reach (`just front-changed`) and `vite build`. On a head that
changes only scripts under `scripts/` or `testenv/` (and prose) it runs
`check-scripts`: the doc-list and doc-link checks and `check`'s script half (`shell`, `test-layout`,
`witness-unit`, `conflict-markers`), no cargo. `scripts/gate-scope.sh` says
which, and why.

`just front` alone is the fast loop for frontend work. `just pre-gate` formats,
refreshes the IPC mirror and runs a few quick checks. It does not replace the
gate. `just docs-check` is the three cheap checks alone (doc lists, doc links,
conflict markers) for a docs-only change.

Timings stay out of the gate, because a wall-clock assertion inside it would
measure the machine. They have recipes of their own: `just search-perf`,
`just builds-perf`, `just responsiveness-probe` and `just keep-alive-budget`,
and `just frame-budget` and `just open-budget`, which `testenv/perf/README.md`
describes.

`scripts/doc-lists.py` holds four hand-written tables to the tree, both ways:
*Crates* and *Frontend* below (every directory under `crates/` and
`app/src/lib/`, and every path a row names), and in `testenv/README.md` the
live-suite recipes (every `<x>-live` recipe) and the `live-up` products (every
product `testenv/live-lib.sh`'s `_live_main` knows). A crate added without its
row fails the gate.

`scripts/doc-links.py` holds every Markdown link in the tree to a target that
exists: `[text](path)`, an image, a reference definition, and `src`, `href`
and `srcset` in HTML. A relative path must name a file or a directory git
lists, and `#anchor` on a Markdown file must be one of its headings, slugged
as GitHub slugs it, or an `<a id>` in it. A link inside a code span or a fence
is an example and is not read, and neither is a file under a `fixtures/`
directory. A glossary term in `CONTEXT.md` is bold text and makes no anchor
of its own, so each carries `<a id="...">`: a new term without one fails here
the first time something links to it.

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

The local `just check` is the gate. `docs/agents/working-model.md` says who
runs it and when. No workflow runs it for you: `check.yml`, `testenv.yml` and
`release.yml` run on `workflow_dispatch` only, so `gh pr checks` reports no
checks on most pull requests.

- `check.yml` runs the same `just check` on Linux. It does not restate the
  gate, so the two cannot drift.
- `testenv.yml` lints the test environment's scripts and compose file without
  starting containers.
- `windows.yml` is the one workflow with a trigger of its own (#2232). A pull
  request that changes `knobas-db`'s console, embedded-server or migration
  code, a migration, `migrations.sha384` or `.gitattributes` runs the
  Windows-only tests on `windows-latest`. A Mac or Linux gate cannot see what
  they cover: that a console program knobas starts opens no window, that the
  checkout kept the migrations' line endings, and that the server binds the
  loopback.

`check.yml` and `testenv.yml` are disabled since 2026-10-24 (Björn: the runner is too slow; the
checks run on the Mac; `docs/decisions/2026-10-24-bjorn-rulings.md`, #2035).
Until he enables them again, a step that says to dispatch one runs this
instead:

- for `check.yml`: `just check`. Nothing then runs the gate on Linux;
  `docs/agents/working-model.md` (*Platform*) says what that leaves open.
- for `testenv.yml`, after touching `testenv/`: `just check` covers its
  `just shell` step. It does not cover the compose and port steps; run those
  from `testenv/`: the three `docker compose ... config -q` commands in
  `.github/workflows/testenv.yml`, then `./check-ports.sh`.

`gh workflow enable check.yml` and `gh workflow enable testenv.yml` bring them
back. Dispatching is then:

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

The workflow creates the release as a draft. Its body is only the note on
unsigned bundles; the patch notes are written by hand. Before publishing, put
a `## What's new in vX.Y.Z` section above that note, with a `---` between
them: one bold heading per spec or theme that shipped since the last tag, with
bullets saying what changed for the user and the PR or spec numbers, then
**Fixes**. `git log <last tag>..<tag> --oneline` lists what shipped, and the
last release's body (`gh release view <last tag>`) shows the form. Write them
with `gh release edit <tag> --notes-file <file>`. Then publish
(`--draft=false --latest`) once the legs you wanted have finished. Nothing is
signed. macOS reports the app as damaged until
you run `xattr -dr com.apple.quarantine /Applications/knobas.app`, and Windows
shows a SmartScreen warning. The Linux artifacts carry the runner's glibc
floor.

## README screenshots

Every picture in the README is generated. `docs/screenshots/shots.txt` lists
them, one line per picture: a name, then a route and steps in the words of
`scripts/shot-steps.mjs`. Each line becomes `<name>.webp`, in light mode.

```bash
just readme-shots   # about 90 seconds, into docs/screenshots
```

A re-run on the same kind of machine draws the same bytes, so between two runs a
diff means the app changed. `scripts/readme-shots.sh` says how: the page's clock,
timezone and locale are pinned, each shot gets an empty Chrome profile, and the
fixture is asked for its calm state (`?fake-sources-healthy&fake-timer`). The
pictures only ever show that fixture, the fictional Tidewater, never a live source.

A Mac and the Linux runner rasterize text differently, so a Mac run rewrites every
file against what a release committed, by a few pixels of text edge. Use a Mac run
to look at a change, then `git restore docs/screenshots` and let the next release
commit it. Commit a Mac run only when the README cannot wait for a release, and
expect that release to rewrite every picture again.

`release.yml`'s `readme` job runs the same script on the tag once the bundles
are built, and `scripts/readme-shots-publish.sh` commits the result to `main`
with the tag in the README's stamp. It runs before the publish click, so the
README is right when the release goes out. It shoots only for the newest tag
with no suffix: a tag like `v3.2.9-ux7`, or one older than the newest release,
leaves the README alone. A new tag commits at least the stamp, and the same
tag published twice commits nothing. A full set is about 1.5 MB, and only the
pictures that changed reach the history.

To add a picture, add a line to `shots.txt` and an `<img>` to the README, then
run the recipe. Every line ends on `see:` and words from the fixture only that page
shows, because an address with nothing behind it still draws a page. The script
refuses a line with no `see:`, a line the README does not show and a picture the
README asks for that no line makes.

A step that finds nothing fails the run and names the step, and nothing is
replaced. So a PR that removes a page a line shows, or a control it clicks, should
edit the line and the README's section with it. One that does not fails the next
release's `readme` job, with the installers already uploaded. Fix the line, run
`just readme-shots` and commit the pictures.

### Diagrams

The three diagrams in `docs/how-it-works.md` are `docs/diagrams/*.d2`, laid out by
TALA, which ships inside d2 since v0.9.0 with no plugin and no key. They follow the
reader's theme, so `just readme-diagrams` renders each to `<name>-light.svg` and
`<name>-dark.svg`. Nothing runs it for you: edit the source and render when the code
a diagram shows changes, and look at the result. It refuses a d2 without TALA rather
than lay them out with another engine.

| Source | Shows | Change it when |
| --- | --- | --- |
| `system-map.d2` | Sync and reads coming in, the write path going out | A new path between a source and the database, or a new store |
| `write-queue.d2` | The states of `knobas.write_queue` and the queue's buttons | A state, or a button in `app/src/lib/shell/WriteQueue.svelte` |
| `crates.d2` | The crates in rows, each depending only on rows below | A crate is added or removed, or a `[dependencies]` line crosses a row |

`system-map.d2` gives every group a fixed place (`top`, `left`), because TALA left
to itself folded the groups into a square and moved the whole picture after a small
edit. A new box there needs a place of its own. All three were last checked against
the code at `f64718f7`.

## Profiles

`just dev` runs the default profile: the real data directory and database. Its
keychain service is not the installed app's. A debug build appends `.dev`
(`dev.knobas.desktop.dev`), so `just dev` cannot reach the credentials of an
installed knobas.

`just demo` runs the same app with `--demo`. The demo profile has its own data
directory and its own embedded server on its own port, so fixture data never
mixes with a real corpus and a demo run never sees real credentials. Its
credentials go to the `.demo` keychain service in a release build. Under
`just demo`, which is a debug build, they go to `demo/dev-secrets.json` in the
profile's folder, and the keychain is never asked (#2384). Spec §14a has the
reasoning.

`just live-demo` is `just demo` with every live product of the test
environment already added as a source (`testenv/README.md`, *Live products as
sources*).

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
configuration is edited, or the reader presses *Retry waiting writes*.

## Shutting down

Quitting the app (Cmd-Q, or closing the last window) stops a running clone and
removes its half-made folder, stops the sync scheduler, cancels in-flight runs,
stops the backup task and stops the embedded server, also while the boot screen
is still up. Every
step is bounded, so a quit does not wait out a slow remote call.

A write knobas is sending when you quit gets up to three seconds to finish, so
it is marked sent. This includes the write you wait on after *Retry waiting writes*, *Send
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

Five surfaces are frozen:

- every released migration under `crates/knobas-db/migrations/`, comments
  included, because sqlx checksums the whole file
- `crates/knobas-source/src/`: the `Source` trait, its types and the contract
  battery
- the IPC command and event schema
- `crates/knobas-http/`
- `crates/knobas-app/src/error.rs` and `profile.rs`

A change to one is announced in the PR body under **Frozen surface**, as
`docs/contract.md` §10.8 says; the merge-manager reviews it against the ADRs.

## Crates

[how-it-works.md](how-it-works.md#how-the-code-is-laid-out) draws these in rows: a
crate depends only on crates in rows below its own.

| Crate | What it is |
| --- | --- |
| `knobas-core` | The domain: entity addressing (`<namespace>:<key>`), links, the activity stream and their stores. |
| `knobas-db` | Embedded PostgreSQL lifecycle (start, adopt, stop), the connection pool and the migrations. |
| `knobas-source` | The adapter SPI: the `Source` trait, the contract battery and the serde data adapters exchange. It has no database dependency. |
| `knobas-spi-types` | The types the adapter SPI shares with the core: payload declarations, entity references, tones and how a write is delivered. No database, no async runtime: it and `knobas-source` build for `wasm32-wasip2`, so an external adapter can depend on them. `knobas-core` re-exports it under the old paths (#1234). |
| `knobas-http` | The HTTP stack the adapters share: one reqwest client, retry classes, rate limiting and the status to `SourceError` mapping. The write queue in `knobas-sync` also reads its local-quota mark (#1729). |
| `knobas-source-jira` | Jira Data Center adapter, with status, comment, create and worklog writes. |
| `knobas-source-confluence` | Confluence Data Center adapter, with comment, page-edit and new-page writes. |
| `knobas-source-gitea` | Gitea and Forgejo adapter: repositories, branches, pull requests and commits, and issues as tickets, so it fills two roles, code host and ticket system (ADR-0029). Its writes cover branches, file commits, pull requests, reviews, line threads and merges. |
| `knobas-source-teamcity` | TeamCity adapter: build configurations and builds, with trigger, re-run, run with parameters and set-parameter writes. |
| `knobas-source-kuma` | Uptime Kuma adapter: monitors from `/metrics`; pause, resume and create over socket.io with an account. |
| `knobas-source-mock` | The reference adapter. It serves the Tidewater fixture and talks to nothing. UI and engine tests use it. |
| `knobas-wasm-host` | The WASM host (ADR-0028): loads an external adapter, a component built against `wit/adapter.wit`, runs it under wasmtime with a deadline and memory cap on every call and `wasi:http` held to its manifest's hosts, and hands it to the rest of knobas as a `dyn Source` (#1198). `docs_contract::check` is the docs contract `knobas adapter check` would run on a component that claims the wiki role (#2444). |
| `knobas-adapter-sdk` | The Rust SDK of the adapter standard (#2028): the `knobas:adapter` bindings, the SPI types re-exported from `knobas-source`, and the `Adapter` trait an external adapter implements, with the WIT conversions and the `export!` macro inside. Its version is the interface version it binds (1.5.0, which adds `Adapter::project`, the docs projection, #2442, and the docs writes, #2443), not the workspace's. It re-exports the shared HTML reader with its hook (`html`, `Hook`, #2444), and its default new page body is the text as escaped plain HTML (`html_from_text`). |
| `knobas-wasm-fixture` | Test only. The fixture external adapter the WASM host is tested with: a fixed corpus with a cursor, built to a component by `just wasm-fixture` (#1198), on `knobas-adapter-sdk`. No external adapter ships with knobas. |
| `knobas-mockd` | Deprecated and frozen (ADR-0013). An HTTP mock of Jira and TeamCity, kept as a dev-dependency for older tests until the live suites cover them. |
| `knobas-live-cleanup` | Dev-dependency only. The one runner every live suite's `Drop` guard hands its cleanup to: a thread and runtime of its own, a bound on the whole sequence, and a report that cannot abort a test that is already unwinding (#1913). |
| `knobas-secrets` | The credential store behind a trait: the OS keychain, an in-memory store for tests and `KNOBAS_SECRET_STORE=memory` runs, and a file store for a debug build's demo profile. No secret reaches Postgres or crosses the bridge. |
| `knobas-search` | The launcher's read side: full-text search over the mirror in the `SearchQuery` to `SearchResponse` shape. |
| `knobas-sync` | The sync engine and scheduler: one run per transaction, cursors, backoff, credential health, the run log, and the write queue. |
| `knobas-app` | The Tauri shell: window, app state, database lifecycle, profiles and the IPC commands. |

## Frontend

`app/` is Svelte 5 runes on plain Vite (no SvelteKit), TypeScript strict with
`exactOptionalPropertyTypes` and `noUncheckedIndexedAccess`.

| Path | What lives there |
| --- | --- |
| `app/src/App.svelte` | The root: the boot gate, the first-run wizard, then the shell. |
| `app/src/lib/tf/tf.css`, `app/src/lib/tf/css/tokens.css` | The global stylesheet and the design tokens. `tf/css/` holds one stylesheet per area. `app/src/unported.css` is a second global stylesheet, for the components that predate the shell's look. |
| `app/src/lib/tf/` | The shell since #881: `TfShell.svelte`, and one folder per page. Routes are `tf/route.ts` (`parseTfHash`, old addresses rewritten in `tf/address.ts`), each folder's `page.ts` registers its page, `tf/pages.ts` collects them. |
| `app/src/lib/shell/` | Lifecycle, keyboard, toasts, the write queue and shared primitives. Its `router.ts` is the pre-#881 parser, still used by the asset tree; new routes go in `tf/route.ts`. `shell/dev/` is the fake backend of *QA without Tauri*. |
| `app/src/lib/detail/` | Link helpers and dialogs for an item's detail (`LinksPanel`, `LinkDialog`, `links.ts`, `relations.ts`) and the `view/` folder; there is no slide-over component. |
| `app/src/lib/sources/` | `FirstRun.svelte` (the first-run wizard) and the helpers behind it and behind Settings › Sources: account, connection, diagnostics, schema-form and fixtures. There is no sources view. |
| `app/src/lib/launcher/` | The ⌘K overlay. It binds its own hotkey and handles its own `Esc`. |
| `app/src/lib/render/` | Which renderer a file or a Markdown fence gets, and the engines that draw it. *How a file is drawn*, below, goes through them. |
| `app/src/lib/ipc/` | The command surface as the window sees it: hand-written `invoke` wrappers, one file per Rust command module in `crates/knobas-app/src/commands/`, over types generated into `ipc/generated/`. The gate's `ipc-mirror` step fails on a stale one. |
| `app/src/lib/{assets,capture,inbox,standup,start-work,time}/` | Feature views, stores and helpers. `inbox/` and `start-work/` hold no component: their pages are under `tf/`, as notes and settings are (`tf/note/`, `tf/settings/`). |

State that several trees read lives in module-level runes. Most stores export a
`create…` factory so a test can build its own without a Tauri bridge:
`shell/health.svelte.ts` (live credential health) and
`shell/kind-registry.svelte.ts` (what installed adapters declare about their
kinds) are two. `shell/toasts.svelte.ts` is one module-level state with a
`reset()` instead.

### How a file is drawn

Everything here is under `app/src/lib/render/` unless it names another folder.

- **Which renderer.** `registry.ts` holds what each renderer declares and which
  accept a file or a fence. `defaults.ts` picks one from the default per file
  pattern. The defaults are stored on the backend, `defaults.svelte.ts` holds
  the copy, and the switcher is `tf/render/TfRendererSwitch.svelte`. `port.ts`
  is the port over the engines.
- **Code.** Shiki highlights in a worker (`highlight.worker.ts`,
  `highlight-core.ts`).
- **Markdown.** `markdown.ts` is markdown-it with DOMPurify, links, and fences
  handed to the registry, drawn by `tf/markdown/TfMarkdown.svelte`. `image.ts`
  turns a relative image's bytes into a blob, an SVG sanitized first.
- **Diagrams.** `diagram.ts` draws into a sandboxed frame (`frame.ts`). Each
  engine runs where it can:
  - Mermaid (`mermaid.ts`) on the main thread.
  - Graphviz (`graphviz.ts`, WASM) in `diagram.worker.ts`. Tauri sends the CSP
    with `.html` assets only, so WASM runs there without loosening the
    document's `script-src`.
  - D2 (`d2.ts`) on the main thread of a hidden `sandbox="allow-scripts"` frame
    of its own (`d2-frame.html`, `d2-frame.ts`, held by `port.ts`'s
    `frameWorker`), because its Go WASM overflows a WebKit worker's stack. The
    frame is served on the `knobas-render` scheme under `renderers::ENGINE_CSP`
    (#2381).
- **PDFs and images.** `pdf-core.ts` is pdf.js with scripting and form actions
  off and links by the Markdown rules, bound to pdf.js's own worker in
  `pdf.ts`. `tf/render/TfPdf.svelte` draws it page by page into canvases in the
  document. A binary image is `tf/render/TfBytesImage.svelte`.
- **Your own commands.** `command.ts` (ADR-0033). They run on the backend
  (`crates/knobas-app/src/renderers.rs`), and `tf/render/TfCommandOutput.svelte`
  draws their output sanitized in a sandboxed frame.

### House rules

`lib/shell/house-rules.test.ts` enforces these by scanning the sources on disk,
so it covers code no test mounts. The first three break only in a bundled
build: `tauri dev` loads the Vite server over `http://`, which gets no CSP
header.

- No inline styles: no `style="…"`, no `style:` directive, no
  `setAttribute("style", …)`. `style-src 'self'` blocks them in a bundle. Use a
  class or a native `<progress>`.
- No `{@html}`. All displayed text comes from source systems, and
  `ts_headline` output is not XSS-safe.
- No network at runtime from the app's own document. `default-src 'self'`,
  and fonts are bundled from npm. Loopback and the RFC 2606 reserved names are
  the only exemptions. A page's embed is the one thing that reaches the
  network (spec #2432): the app's `frame-src` allows `http:` and `https:` so a
  URL embed can be framed (#2441), and an HTML embed runs from the
  `knobas-render` scheme under its own policy, `EMBED_CSP` in
  `crates/knobas-app/src/renderers.rs`, which lets its scripts load and
  connect over `http(s)` (#2440). Both frames are sandboxed
  (`app/src/lib/render/embed.ts`): an HTML embed has no origin of its own, and
  a URL embed keeps its page's origin, which is never the app's (#2460).
  The app's own documents load in no frame (`frame-ancestors 'none'`, and
  `app/src/lib/shell/own-frame.ts` for the dev server, #2461), so neither
  reaches the app.
- Every `listen()` is cleaned up, including when the component unmounts before
  the promise resolves. `lib/shell/residue.test.svelte.ts` mounts each
  component with an effect three times (used then unmounted, unmounted with
  reads in flight, unmounted between answers) and fails on leftovers.
- No data call before `db.state === "ready"`. Commands that need `AppState`
  reject with `not_ready` during start-up, and the shell gates on the
  lifecycle.
- No `invoke` outside the IPC barrel, and the dev harness is reachable only
  behind `import.meta.env.DEV`.
- Secrets are never displayed, logged or read back. No command returns one.
- Contrast is at least 4.5:1 and reduced motion is respected (spec §14).
  `lib/tf/tf-css.test.ts` checks the palette and the motion rules.
- Every icon-only button, text input, select and textarea has an accessible
  name, and a decorative icon is hidden from the accessibility tree.
  `lib/shell/a11y.test.ts` scans for both.

### QA without Tauri

Any screen runs in a browser with a fake behind `invoke`. `just shot` starts
the dev server on its own port (`5300 + slot`) and captures a route under
`?fake-ipc`:

```bash
just shot 3 /tmp/shots '#/tickets'
```

`?fake-ipc` turns the fake on. It only exists in the dev server: a production
build drops it, and `vite preview` then sits on "Starting the local database".
The fake lives in `app/src/lib/shell/dev/`. More switches go after `?fake-ipc`:

| Switch | What it does |
| --- | --- |
| `&fake-db=starting\|migrating\|failed` | Holds the boot screen on one state. |
| `&fake-first-run` | Opens the first-run wizard. |
| `&fake-sources-healthy` | Leaves no source that needs the reader, so no err tone draws in the status bar. |
| `&fake-timer` | Starts with a timer running. The README's pictures use this and the one above. |
| `&fake-scale` | The fixture at the size of a real mirror: 20,000 tickets, 5,000 pages. |
| `&fake-real=<port>` | Sends every command to a real windowless backend on that port (`crates/knobas-app/examples/headless_ipc.rs`). |

This checks layout and interaction. `just dev` or `just demo` checks the
bridge.

### Mockups

`mockups/` holds the clickable HTML mockups the app was drawn from.
`mockups/ticket-first/` has `prototype-1.html` to `prototype-6.html` with the
brief in `requirements.md`, and the later `ticket-page.html` and
`docs-reader.html`. `mockups/code-area/` has the Code pages, and
`mockups/shared/` the shared notes (`assets.md`, `capabilities.md`,
`dataset.md`).

They are historical and decide nothing new: UI work is held to
`docs/specs/2026-10-15-ticket-first-design-system.md` (its Controls section for
buttons and their placement), the dev-only `#/controls` specimen, and
before/after screenshots (#2238). One file still binds: `lib/tf/tf-css.test.ts`
holds the app's `--hl-*` highlighting tokens to `mockups/code-area/base.css`,
so a change there fails the gate.

Prototype-6's CSS became the global stylesheet. Its rendering approach (string
templates and one `innerHTML` swap per frame) did not carry over.
