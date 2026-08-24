# knobas M1 — Stream D: Frontend Shell Implementation Plan

> **For agentic workers:** Execute via the **PR loop** in `2026-08-24-knobas-roadmap.md` §3: one `implementer` agent (Opus 5, high) per task in its own worktree/branch (`m1/d-<slug>`) → PR → `pr-reviewer` (Opus 5, xhigh) → iterate (max 3 rounds) → squash-merge on approval + green `just check`. Steps use checkbox (`- [ ]`) syntax for tracking. Tasks inside a phase run **sequentially** — this is one stream, one file region, and later tasks build on earlier components. Phase 2 does not start before **Checkpoint 1** (interfaces §6.2: stream F's sources CRUD + scheduler merged).

**Goal:** Port the round-3 Signal shell (`mockups/round-3/signal-miller.html`) to Svelte 5 as the real knobas window — a window that appears *before* the database is up and says so, a hash-addressed room whose tiles read the synced corpus, a read-only detail slide-over that renders any kind (declared or not) from its raw payload, a sources view whose Add-source form is generated from each adapter's `config_schema`, credential health that offers *Re-enter* on a 401, and a first-run wizard that ends in a synced launcher.

**Architecture:** Svelte 5 runes, plain Vite, no SvelteKit. The mockup is a **behaviour reference only** — its rendering strategy (string templates + one `innerHTML` swap per frame) is explicitly *not* carried over (roadmap §5 risk row: "Full-page-rerender habits from the mockup leaking into the port"). Components own their state; data flows in as props or is fetched by the component that displays it; nothing re-renders a region because a sibling changed. The mockup's **CSS is carried over wholesale as a global stylesheet** (roadmap §4). Backend-side, stream D owns two thin command modules (`commands/app.rs`, `commands/entity.rs`) and their hand-written TS mirrors; everything else it consumes from the M1 contract.

**Tech Stack:** Svelte 5.56 + Vite 8 + TypeScript 5.9 (strict, `exactOptionalPropertyTypes`, `noUncheckedIndexedAccess`), Tauri 2.11 (`@tauri-apps/api` 2.11.1), Vitest + jsdom for component tests, `@fontsource/*` for self-hosted IBM Plex Sans/Mono + Barlow Condensed, `tauri-plugin-opener` for *Open in browser*, sqlx 0.9 runtime-checked queries on the Rust side.

**Spec:** `docs/superpowers/specs/2026-08-23-knobas-design.md` — §2 Shell (top strip, status bar, room, detail slide-over, entity addressing, split-flaps, keyboard), §2a Activity stream, §3 Sources and sync (add-source flow, credential health, diagnostics), §3a Adapter SPI (generated form, open kinds, generic detail view, raw payload), §5 Work items (read-only halves), §12.1 (change history per entity), §14 Non-functional (accessibility, secrets never shown), §14a First run + demo mode.
**Contract:** `docs/superpowers/plans/2026-08-24-m1-interfaces.md` — §2 (the IPC surface, consumed exactly as written), §6.1 (ownership), §8 (rulings P1–P13). **Roadmap:** `docs/superpowers/plans/2026-08-24-knobas-roadmap.md` §2 row D, §3 working model, §4 stack + gotchas. **Carry-overs:** `docs/superpowers/plans/2026-08-24-m1-carryovers.md` (stream D block).

---

## Global Constraints

**Inherited from plan-01 (M0), still in force:**

- sqlx: **runtime-checked queries only** (`sqlx::query`, `query_as` + `FromRow`) — no `query!` macros, no compile-time `DATABASE_URL`, no offline cache. Never pin sqlx 0.8.4 (yanked).
- Postgres runs on **TCP 127.0.0.1**; never Unix sockets.
- Every generated FTS column is `GENERATED ALWAYS AS (...) STORED`. (Stream D writes no migrations, but reads columns that depend on it.)
- Entity ids are strings `"<namespace>:<key>"`; the kind lives in `knobas.entity.kind`, not in the id.
- Frontend: Svelte 5 runes, plain Vite (no SvelteKit), TypeScript strict.
- **Commit style:** short imperative subject, no attribution footer. Task-branch commits are intentionally unsigned; signing is disabled **per-worktree only** — `git config extensions.worktreeConfig true` once, then `git config --worktree commit.gpgsign false` inside the worktree; never write `commit.gpgsign` to the shared repo-local config.
- **Quality gate:** `just check` = `cargo fmt --check` + `front` (`npm run check && npm run build`) + `cargo clippy --workspace --all-targets -D warnings` + `clippy-libs` + `cargo test --workspace`. Every task ends with it green.

**Roadmap §4 gotchas binding stream D (copied verbatim):**

1. > sqlx 0.9: dynamic SQL needs `AssertSqlSafe`; keep it in one reviewed query-builder module. Never bind `tsquery` — bind text into `websearch_to_tsquery('english', $1)`, compute the tsquery once as a FROM item. **Never map `tsvector` to `String`.**
   *For D:* `sync.live_item` exposes `fts` (a `tsvector`). Stream D's queries name their columns explicitly and never `select *` into a `FromRow` struct (interfaces §1 WARNING). Stream D writes **no dynamic SQL at all** — `EntityFilter`'s optional predicates are expressed as nullable-array/`is null` parameters in one static statement; the two `EntityOrder` values are two static statements chosen by a `match`.
2. > `ts_headline` output is not XSS-safe — escape synced HTML before the webview renders it.
   *For D:* **every value that came from a source system is rendered as text.** `{@html}` is forbidden anywhere in `app/src/lib/**` and `App.svelte`. That covers `title`, `body_text`, `author`, `payload` (interfaces §2.5: "It is **untrusted text**: the generic detail view projects it as text, never as markup"), snippet segments, and `IpcError.message`.
3. > Tauri: don't `emit` from the `setup` hook (webview not listening yet) — frontend signals ready first. Stop the scheduler and `pg_ctl stop` on `RunEvent::ExitRequested`.
   *For D:* `frontend_ready` exists precisely for this; the frontend registers its `listen()`s **before** calling it, and the backend replays the current `db:state` on that call.
4. > Unsigned dev builds re-prompt the keychain on every run — sign locally.
   *For D:* relevant when QA-ing the sources view (Phase 2); a re-prompt per run is expected, not a bug.

**M1 contract constraints (interfaces §2, §6, §8):**

- **Errors are `IpcError { code, message, source_id }`** with `code ∈ unauthorized | unreachable | not_found | conflict | invalid | not_ready | internal` (P1 GRANTED, all commands). The UI branches on `code`; it never string-matches `message`.
- Command names are snake_case in Rust; **arguments** are renamed to camelCase by Tauri (`sourceId`), **struct fields keep their snake_case spelling**. New enums serialize `#[serde(rename_all = "snake_case")]`, data-carrying ones `#[serde(tag = "state", rename_all = "snake_case")]`.
- **TS mirrors stay hand-written** and ship in the **same PR** as the Rust command they mirror. Mapping: `DateTime<Utc>` → `string` (RFC 3339), `Option<T>` → `T | null`, `i64/u64/f32` → `number`, `serde_json::Value` → `unknown`, `#[serde(flatten)]` → inlined fields, snake_case enums → string unions, tagged enums → discriminated unions.
- **Stream D may create/modify only:** `app/src/App.svelte`, `app/src/lib/shell/**`, `app/src/lib/detail/**`, `app/src/lib/sources/**`, `app/src/lib/ipc/app.ts`, `app/src/lib/ipc/entity.ts`, `app/src/app.css`, `crates/knobas-app/src/commands/app.rs`, `crates/knobas-app/src/commands/entity.rs`. Test files live **inside those paths** (`app/src/lib/shell/*.test.ts`), never in a new top-level directory.
- **Orchestrator-owned files D must request edits to, in the same PR:** `crates/knobas-app/src/lib.rs` (handler list, event constants, `run()` wiring), `crates/knobas-app/src/commands/mod.rs`, `app/src/lib/ipc/index.ts` (barrel), `crates/knobas-app/Cargo.toml`, `crates/knobas-app/tauri.conf.json`, `crates/knobas-app/capabilities/default.json`, `app/package.json`, `app/vite.config.ts`, `app/tsconfig.json`, `justfile`. Each such edit is **one line or one block**, described in the PR body under "Orchestrator edits requested".
- **Migrations: none.** `0002` is single-writer (orchestrator). A schema need becomes an open question, never a file.
- **Never invent an interface owned by another stream.** Types and functions from `app/src/lib/ipc/{sources,search}.ts` and `crates/knobas-app/src/commands/{sources,search}.rs` are *imported*, never declared, in D-owned files. If they do not exist yet, the task waits (Phase 2) or degrades (documented per task).

**Frontend house rules for this port (enforced by Task 1's lint test):**

- **No `style="…"` attributes and no `style:` directives** anywhere in D-owned Svelte. `app.security.csp` is `style-src 'self'` with no `'unsafe-inline'`, which blocks inline style *attributes* in CSP2+. Component `<style>` blocks are fine — `vite build` extracts them into `assets/index-*.css`. Dynamic geometry uses a native `<progress>` element or a class, never a computed inline style. **Desktop dev builds get NO CSP (carry-over) — a violation is invisible under `just dev` and fatal in a bundle.**
- **No network at runtime.** No `@import url(https://…)`, no `<link rel=stylesheet href=https://…>`, no remote images. Fonts are npm-vendored and bundled (`default-src 'self'`).
- **No `{@html}`.** See gotcha 7 above.
- **Every `listen()` is cleaned up.** `listen` resolves to an `UnlistenFn`; a component that subscribes returns a teardown from its `$effect` that calls it, and guards against the "unmounted before the promise resolved" race.
- **No data call before `db.state === "ready"`.** Commands that need `AppState` reject with `not_ready` while Postgres is coming up; the shell gates fetching on the lifecycle, it does not catch-and-ignore.
- **Reduced motion is respected**: split-flaps and slide-over transitions are disabled under `prefers-reduced-motion: reduce` (spec §2, §14).
- **Secrets are never displayed, never logged, never round-tripped.** There is no command that reads a secret back (interfaces §2.2) and no D-owned code that stores one outside the input element it was typed into.

---

## File map

| File | Responsibility |
|---|---|
| `crates/knobas-app/src/commands/app.rs` | `Lifecycle`, `Profile`, `app_status`, `frontend_ready`, `complete_first_run` |
| `crates/knobas-app/src/commands/entity.rs` | `get_entity`, `list_entities`, `recent_activity` |
| `app/src/lib/ipc/app.ts` | TS mirror of `commands/app.rs` |
| `app/src/lib/ipc/entity.ts` | TS mirror of `commands/entity.rs` |
| `app/src/app.css` | The Signal stylesheet: tokens, app frame, room, detail, sources, overlays |
| `app/src/App.svelte` | Root: lifecycle gate → `Shell`; owns the launcher slot |
| `app/src/lib/shell/lifecycle.svelte.ts` | Boot state machine: poll + `db:state`, `frontend_ready`, retry |
| `app/src/lib/shell/router.svelte.ts` | Hash → `Route`, `go()`, `Esc` unwind ladder |
| `app/src/lib/shell/Shell.svelte` | The 44px / 1fr / 24px frame |
| `app/src/lib/shell/TopStrip.svelte` | Context tabs, search field, sync monograms, sources gear |
| `app/src/lib/shell/StatusBar.svelte` | DB size, counts, sync cadence, latest change, clock |
| `app/src/lib/shell/Room.svelte`, `Tile.svelte`, `EntityRowLine.svelte` | The room and its read-only tiles |
| `app/src/lib/shell/Flap.svelte`, `Toast.svelte`, `Modal.svelte`, `Monogram.svelte` | Signal primitives |
| `app/src/lib/shell/kinds.ts` | Kind → tile bucket, label/monogram fallbacks |
| `app/src/lib/shell/errors.ts` | `asIpcError`, `errorMessage` |
| `app/src/lib/shell/dev/` | Dev-only fake Tauri IPC + fixture (never in a production bundle) |
| `app/src/lib/detail/Detail.svelte` | The slide-over frame (header, crumb, close, Esc) |
| `app/src/lib/detail/PayloadView.svelte`, `payload.ts` | §3a generic projection of `payload` |
| `app/src/lib/detail/HistoryPanel.svelte`, `LinksPanel.svelte` | §2a history, §5a links (empty in M1) |
| `app/src/lib/sources/SourcesView.svelte` | The sources view: rows, health, sync, diagnostics |
| `app/src/lib/sources/schema-form.ts` | `config_schema` → field model + validation |
| `app/src/lib/sources/SchemaForm.svelte` | Renders that field model |
| `app/src/lib/sources/AddSource.svelte` | The five-step add-source dialog |
| `app/src/lib/sources/FirstRun.svelte` | §14a wizard with `Channel<SyncProgress>` |

---

## Phase 0 — the window boots and says what it is doing

*No dependency on stream E or F. Everything here is testable with fixtures and the M0 backend.*

---

### Task 1: Frontend test harness and the headless-Chrome QA convention

**Files:**
- Create: `app/src/lib/shell/dev/fake-tauri.ts`, `app/src/lib/shell/dev/fixture.ts`, `app/src/lib/shell/test-setup.ts`, `app/src/lib/shell/house-rules.test.ts`, `app/src/lib/shell/greeting.svelte` *(one throwaway component the harness proves itself on — deleted by Task 5)*, `app/src/lib/shell/greeting.test.ts`
- Modify (orchestrator edits requested): `app/package.json` (devDeps + `check` script), `app/vite.config.ts` (vitest block, `assetsInlineLimit`), `justfile` (nothing — `front` already runs `npm run check`)

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `npm run check` = `svelte-check --tsconfig ./tsconfig.json && vitest run` — the gate every later task's tests ride on.
  - `installFakeTauri(fixture?: FakeFixture): void` in `dev/fake-tauri.ts` — defines `window.__TAURI_INTERNALS__.invoke` so the frontend runs in a plain browser.
  - `DEMO: FakeFixture` in `dev/fixture.ts` — a handful of Tidewater rows (`mock:PAY-231`, `mock:payout-service#142`, `mock:Payout_Build#1188`, `mock:sepa-design`, `mock:c90d11`) shaped exactly like the contract's `EntityRow`/`EntityDetail`.
  - The **QA convention** every later task's final step uses.

- [ ] **Step 1: Add the dependencies and scripts**

`app/package.json` — request from the orchestrator: add to `devDependencies` `vitest`, `jsdom`, `@vitest/coverage-v8` *(omit coverage if it drags the install)*, pinning the newest `vitest` whose peer range accepts **vite 8** (verify with `npm ls vite` afterwards: exactly one vite in the tree). Change:

```json
"scripts": {
  "dev": "vite",
  "build": "vite build",
  "preview": "vite preview",
  "test": "vitest run",
  "check": "svelte-check --tsconfig ./tsconfig.json && vitest run"
}
```

- [ ] **Step 2: Configure Vitest**

`app/vite.config.ts` — switch the import to `import { defineConfig } from "vitest/config";` and add, inside the returned object:

```ts
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts"],
    setupFiles: ["src/lib/shell/test-setup.ts"],
  },
  // Vitest must resolve Svelte's *browser* build, or `mount` runs the SSR
  // entry point and produces no DOM.
  resolve: { conditions: ["browser"] },
```

and inside `build`, add `assetsInlineLimit: 0` with this comment: *Vite inlines assets under 4 kB as `data:` URIs. `default-src 'self'` has no `data:` for fonts, so an inlined woff2 is a font that silently fails to load in a bundled build and works in dev.*

- [ ] **Step 3: Write the failing component test**

`app/src/lib/shell/greeting.test.ts`:

```ts
import { flushSync, mount, unmount } from "svelte";
import { expect, test } from "vitest";

import Greeting from "./greeting.svelte";

test("renders its prop and reacts to a change", () => {
  const target = document.createElement("div");
  document.body.append(target);
  const props = $state({ name: "knobas" });
  const app = mount(Greeting, { target, props });

  expect(target.textContent).toBe("hello knobas");
  props.name = "Tidewater";
  flushSync();
  expect(target.textContent).toBe("hello Tidewater");

  unmount(app);
  target.remove();
});
```

*(`$state` in a `.test.ts` file requires the svelte compiler on that file. If the runes syntax is rejected outside `.svelte`/`.svelte.ts`, rename the file to `greeting.svelte.test.ts` and add that glob to `test.include`; check which of the two the installed Svelte version accepts before writing the rest of the suite — every later task depends on the answer.)*

`app/src/lib/shell/test-setup.ts`:

```ts
// jsdom has no matchMedia; the shell asks it about reduced motion on mount.
if (!window.matchMedia) {
  Object.defineProperty(window, "matchMedia", {
    writable: true,
    value: (query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    }),
  });
}
```

- [ ] **Step 4: Run it and watch it fail**

Run: `cd app && npm run test`
Expected: FAIL — `greeting.svelte` does not exist.

- [ ] **Step 5: Write the component**

`app/src/lib/shell/greeting.svelte`:

```svelte
<script lang="ts">
  let { name }: { name: string } = $props();
</script>

hello {name}
```

Run: `npm run test` → PASS.

- [ ] **Step 6: Write the house-rules lint test**

`app/src/lib/shell/house-rules.test.ts` — this is the enforcement arm of the Global Constraints, and it runs on every later task:

```ts
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { expect, test } from "vitest";
import { globSync } from "tinyglobby"; // ships with vite; if absent, use node:fs readdirSync recursion

const ROOT = fileURLToPath(new URL("../../", import.meta.url)); // app/src/
const sources = globSync(["**/*.svelte", "**/*.ts"], { cwd: ROOT, absolute: true });

test("no inline style attributes or directives (CSP style-src 'self')", () => {
  const offenders = sources.filter((file) => {
    const text = readFileSync(file, "utf8");
    return /\sstyle=("|'|\{)/.test(text) || /\sstyle:[a-z-]+/.test(text);
  });
  expect(offenders).toEqual([]);
});

test("no {@html} (source text is untrusted — gotcha 7)", () => {
  const offenders = sources.filter((f) => readFileSync(f, "utf8").includes("{@html"));
  expect(offenders).toEqual([]);
});

test("no runtime network references (default-src 'self')", () => {
  const offenders = sources.filter((f) => /https?:\/\/(?!localhost|127\.0\.0\.1)/.test(readFileSync(f, "utf8")));
  expect(offenders).toEqual([]);
});
```

Run: `npm run test` → PASS (nothing violates it yet). Then temporarily add `style="color:red"` to `greeting.svelte`, re-run, confirm FAIL, and remove it — a lint test that has never failed is not known to work.

- [ ] **Step 7: Write the fake Tauri bridge and the fixture**

`app/src/lib/shell/dev/fixture.ts` — a hand-written subset of the Tidewater fixture. It is **duplicated on purpose**: `fixtures/tidewater/work.json` lives outside Vite's project root and importing across it needs an `server.fs.allow` change that would make the dev server read the whole repo. Five entities is all a screenshot needs.

```ts
import type { EntityRow } from "../../ipc/entity";

export const DEMO_ROWS: EntityRow[] = [
  { entity_id: "mock:PAY-231", kind: "ticket", source_id: "mock",
    title: "Retry failed SEPA payouts", updated_at: "2026-08-22T11:48:00Z",
    synced_at: "2026-08-22T14:28:00Z" },
  { entity_id: "mock:payout-service#142", kind: "pr", source_id: "mock",
    title: "SEPA retry with exponential backoff", updated_at: "2026-08-22T12:10:00Z",
    synced_at: "2026-08-22T14:28:00Z" },
  { entity_id: "mock:Payout_Build#1188", kind: "build", source_id: "mock",
    title: "Payout_Build #1188", updated_at: "2026-08-22T11:45:00Z",
    synced_at: "2026-08-22T14:28:00Z" },
  { entity_id: "mock:sepa-design", kind: "page", source_id: "mock",
    title: "SEPA payout retry design", updated_at: "2026-08-22T10:40:00Z",
    synced_at: "2026-08-22T14:28:00Z" },
  { entity_id: "mock:c90d11", kind: "commit", source_id: "mock",
    title: "PAY-231: jitter in backoff, cap at 5 attempts",
    updated_at: "2026-08-22T11:42:00Z", synced_at: "2026-08-22T14:28:00Z" },
];
```

*(`EntityRow` lands in Task 7; until then type the array as a local `interface` with the same fields and swap the import in Task 7's PR.)*

`app/src/lib/shell/dev/fake-tauri.ts`:

```ts
/**
 * Dev-only: answers `invoke` from a fixture so the frontend runs in a plain
 * browser (headless-Chrome QA, per roadmap §3). Never reached in a bundle —
 * the only caller guards on `import.meta.env.DEV`, so Rollup drops the branch
 * and this module with it. `app/src/lib/shell/dev/**` must never be imported
 * from a non-guarded site; house-rules.test.ts checks that.
 */
type Handler = (args: Record<string, unknown>) => unknown;

export function installFakeTauri(handlers: Record<string, Handler>): void {
  const internals = { invoke: async (cmd: string, args: Record<string, unknown> = {}) => {
    const handler = handlers[cmd];
    if (!handler) throw { code: "internal", message: `fake-tauri: no handler for ${cmd}`, source_id: null };
    return handler(args);
  } };
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: internals, configurable: true });
}

export function installIfRequested(): void {
  if (!new URLSearchParams(location.search).has("fake-ipc")) return;
  installFakeTauri(demoHandlers());
}
```

`demoHandlers()` returns `{ ping: () => "pong", app_status: () => ({ db: { state: "ready" }, first_run: false, demo: true, source_count: 1, app_version: "0.1.0" }), frontend_ready: () => null, list_entities: ({ filter }) => pageFor(filter), get_entity: ({ entityId }) => detailFor(entityId), recent_activity: () => [] }` — grow it per task, one handler per command the screen under QA calls.

Add to `house-rules.test.ts`:

```ts
test("the dev harness is only reachable behind import.meta.env.DEV", () => {
  const importers = sources.filter((f) => f.includes("/lib/shell/dev/") === false
    && readFileSync(f, "utf8").includes("shell/dev/"));
  for (const file of importers) {
    expect(readFileSync(file, "utf8")).toMatch(/import\.meta\.env\.DEV/);
  }
});
```

- [ ] **Step 8: Record the QA convention in the harness file's doc comment**

At the top of `dev/fake-tauri.ts`, write the exact commands (this is the convention every later task's QA step cites — per-agent port and `--user-data-dir`, because parallel agents have collided before):

```
# QA one screen without Tauri. PORT = 5300 + <your agent number>.
cd app && npm run build && npx vite preview --port $PORT --strictPort &
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
  --headless=new --disable-gpu --window-size=1440,900 \
  --user-data-dir=/tmp/knobas-qa-$USER-$PORT \
  --screenshot=/tmp/knobas-qa-$PORT.png \
  "http://localhost:$PORT/?fake-ipc#/ctx/all"
# attach the PNG to the PR; kill the preview server afterwards.
```

Note in the same comment: the **real** end-to-end check is `just dev` (Tauri + embedded Postgres + `--demo`); headless Chrome checks layout and interaction, not the bridge.

- [ ] **Step 9: `just check` and commit**

Run: `just check` → PASS.

```bash
git add app/package.json app/vite.config.ts app/src/lib/shell
git commit -m "frontend: vitest harness, house-rule lints, headless QA bridge"
```

---

### Task 2: The Signal stylesheet — mockup CSS as the global sheet

**Files:**
- Modify: `app/src/app.css` (replaces the M0 reset wholesale)
- Create: `app/src/lib/shell/app-css.test.ts`
- Modify (orchestrator edit requested): `app/package.json` (`@fontsource/ibm-plex-sans`, `@fontsource/ibm-plex-mono`, `@fontsource/barlow-condensed`)

**Interfaces:**
- Consumes: Task 1's harness.
- Produces: the class vocabulary every later component uses, copied from `mockups/round-3/signal-miller.html` lines 19–479: `.app .topbar .ctx-switch .ctx-name .tabs .tab .searchfield .tb-btn .sync .mg .flap .main .room .room-bar .kind .tiles .tile .tile-h .tile-b .empty .row .board .col .col-h .card .btn .chip .pill .inp .sel-inline .detail .d-h .d-b .d-title .d-meta .sec .sec-h .kv .cmt .log .view .view-b .filters .src .src-fix .dbbar .modules .mod .steps .radios .test-res .scrim .dlg .dlg-h .dlg-b .dlg-f .form .x .pop .toast .statusbar .tip .lab .mono .k .muted .faint .amber .fail .ok .link`, and the tokens `--bg --panel --raised --hair --hair2 --text --muted --faint --amber --fail --ok --link --disp --sans --mono --row`.

- [ ] **Step 1: Write the failing stylesheet contract test**

`app/src/lib/shell/app-css.test.ts`:

```ts
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { expect, test } from "vitest";

const css = readFileSync(fileURLToPath(new URL("../../app.css", import.meta.url)), "utf8");

test("carries the Signal tokens the components reference", () => {
  for (const token of ["--bg", "--panel", "--raised", "--hair", "--hair2", "--text",
                       "--muted", "--faint", "--amber", "--fail", "--ok", "--link",
                       "--disp", "--sans", "--mono", "--row"]) {
    expect(css).toContain(`${token}:`);
  }
});

test("fetches no font over the network", () => {
  expect(css).not.toMatch(/@import\s+url\(\s*["']?https?:/);
  expect(css).not.toMatch(/https?:\/\/fonts\.(googleapis|gstatic)\.com/);
});

test("disables the flap animation under reduced motion", () => {
  expect(css).toMatch(/@media\s*\(prefers-reduced-motion:\s*reduce\)/);
});
```

Run: `npm run test` → FAIL (the M0 `app.css` is a 15-line reset).

- [ ] **Step 2: Vendor the fonts**

Request the three `@fontsource` packages. At the very top of `app/src/app.css` (before any rule — `@import` must come first):

```css
@import "@fontsource/ibm-plex-sans/latin-400.css";
@import "@fontsource/ibm-plex-sans/latin-500.css";
@import "@fontsource/ibm-plex-mono/latin-400.css";
@import "@fontsource/ibm-plex-mono/latin-500.css";
@import "@fontsource/barlow-condensed/latin-500.css";
@import "@fontsource/barlow-condensed/latin-600.css";
```

If a `latin-<weight>.css` entry point does not exist under the installed version, fall back to the package's documented per-weight path (`@fontsource/ibm-plex-sans/400.css`); confirm by listing the package directory, do not guess.

- [ ] **Step 3: Port the stylesheet**

Copy `mockups/round-3/signal-miller.html` lines **20–479** into `app/src/app.css` after the imports, with exactly these edits:

1. Drop the `.a*` asset-board block (lines 481+) — that is M4's, and a selector with no markup behind it is dead weight a reviewer cannot check.
2. Drop `/* time view */` (`.strip .blk .axis .legend .blkdet .sheet`) and `/* standup */` (`.su .dg-* .proto .pub`) — M3.
3. Drop the launcher block (`.search .search-b .search-f .smart .sl .prefixes .pfx .chain-* .results .fbar .fchip .rlist .secl .res`) — `app/src/lib/launcher/**` is stream E's, and its styles ship with it. **Keep `.searchfield`** (the top-strip field, which is D's).
4. Keep `html,body{height:100%;overflow:hidden}` — the shell is a fixed 3-row grid, and a scrolling body would double-scroll every pane.
5. Add above `:root`: a header comment naming the source (`mockups/round-3/signal-miller.html`, round 3, D1 Signal), stating that later milestones append their own sections, and that **no rule here may depend on an inline style**.

- [ ] **Step 4: Run the tests**

Run: `npm run test` → PASS. Run `npm run build` → PASS, and confirm the emitted `dist/assets/*.woff2` files exist (fonts were bundled, not inlined and not fetched).

- [ ] **Step 5: QA**

Per Task 1's convention, screenshot `?fake-ipc` — at this point the page is still M0's markup on a graphite background; the check is that the fonts render (IBM Plex, not system-ui) and nothing 404s in the console.

- [ ] **Step 6: `just check` and commit**

```bash
git add app/src/app.css app/src/lib/shell/app-css.test.ts app/package.json
git commit -m "app.css: port the signal stylesheet, vendor the fonts"
```

---

### Task 3: Signal primitives — Flap, Toast, Modal, Monogram

**Files:**
- Create: `app/src/lib/shell/Flap.svelte`, `app/src/lib/shell/Toast.svelte`, `app/src/lib/shell/toasts.svelte.ts`, `app/src/lib/shell/Modal.svelte`, `app/src/lib/shell/Monogram.svelte`, `app/src/lib/shell/Flap.test.ts`, `app/src/lib/shell/Modal.test.ts`, `app/src/lib/shell/toasts.test.ts`

**Interfaces:**
- Consumes: `app.css` (Task 2).
- Produces:
  - `<Flap value={string} tone?: "plain"|"amber"|"fail"|"ok"|"dim" width?: "s"|"t"|"m"|"b"|"r" big?: boolean />` — a split-flap cell. Spec §2 Rec 08-24: **flaps only for values that change while you watch** (sync countdown, sync/health transitions, counts). Static readings are plain mono.
  - `toasts.push({ text, tone?, action?: { label, run } , ms? })`, `toasts.items` — a rune-backed store; `<Toast />` renders the stack into `#toasts`.
  - `<Modal title subtitle? onclose>{#snippet body()}…{/snippet}{#snippet footer()}…{/snippet}</Modal>` — focus-trapped `.scrim > .dlg`.
  - `<Monogram text="JI" tone?: "ok"|"err" />`.

- [ ] **Step 1: Write the failing Flap test**

`app/src/lib/shell/Flap.test.ts`:

```ts
import { flushSync, mount, unmount } from "svelte";
import { expect, test } from "vitest";

import Flap from "./Flap.svelte";

test("shows the value and flips to a new one", () => {
  const target = document.createElement("div");
  document.body.append(target);
  const props = $state({ value: "4:59" });
  const app = mount(Flap, { target, props });

  const cell = target.querySelector(".flap");
  expect(cell?.textContent).toContain("4:59");

  props.value = "4:58";
  flushSync();
  // Both halves settle on the new value; the leaves are transient.
  expect(target.querySelector(".ft")?.textContent).toBe("4:58");

  unmount(app);
  target.remove();
});
```

Run → FAIL.

- [ ] **Step 2: Implement Flap**

The mockup drives flaps through a global `FL` map and a post-render `settleFlaps()` sweep (`signal-miller.html:2200-2220`). **That does not carry over** — a Svelte component owns its own previous value:

```svelte
<script lang="ts">
  let { value, tone = "plain", width, big = false }:
    { value: string; tone?: "plain" | "amber" | "fail" | "ok" | "dim";
      width?: "s" | "t" | "m" | "b" | "r"; big?: boolean } = $props();

  let shown = $state(value);
  let leaving = $state<string | null>(null);
  const reduced = typeof matchMedia === "function"
    && matchMedia("(prefers-reduced-motion: reduce)").matches;

  $effect(() => {
    if (value === shown) return;
    if (reduced) { shown = value; return; }
    const previous = shown;
    shown = value;
    leaving = previous;
    const timer = setTimeout(() => { leaving = null; }, 360);
    return () => clearTimeout(timer);
  });
</script>

<span class="flap {tone === 'plain' ? '' : tone} {width ? `w-${width}` : ''} {big ? 'big' : ''}"
      aria-live="polite">
  <span class="ft">{shown}</span>
  <span class="fb">{shown}</span>
  {#if leaving}
    <span class="leaf top">{leaving}</span>
    <span class="leaf bot">{shown}</span>
  {/if}
</span>
```

- [ ] **Step 3: Write the failing Modal test, then implement**

`Modal.test.ts` asserts three behaviours the mockup's `dialog()` had for free and a port loses if nobody writes them down:

```ts
test("traps focus, closes on Escape and on scrim click, and restores focus", async () => {
  // opener button focused → mount Modal → first focusable inside is focused
  // → keydown Escape calls onclose exactly once
  // → click on .scrim calls onclose; click inside .dlg does not
  // → after unmount, document.activeElement is the opener again
});
```

Write it out concretely (mount with an `onclose = vi.fn()`, dispatch `new KeyboardEvent("keydown", { key: "Escape", bubbles: true })`, assert call counts). Implement `Modal.svelte` with the mockup's `.scrim.center > .dlg > .dlg-h/.dlg-b/.dlg-f` markup, a `role="dialog" aria-modal="true"` container labelled by the title, a keydown handler that cycles Tab within the dialog, and focus restore in the `$effect` teardown.

**`Esc` inside a modal must not reach the shell's unwind ladder** (Task 6) — the handler calls `event.stopPropagation()` after `onclose()`. That is the ladder's first rung, and the mockup's `if(box){…}` branch (`signal-miller.html:4186`) is what it replaces.

- [ ] **Step 4: Toasts**

`toasts.svelte.ts`:

```ts
export interface ToastAction { label: string; run: () => void }
export interface ToastSpec { text: string; tone?: "plain" | "err"; action?: ToastAction; ms?: number }
interface ToastItem extends ToastSpec { id: number }

let next = 0;
export const toasts = $state<{ items: ToastItem[] }>({ items: [] });

export function push(spec: ToastSpec): number {
  const id = ++next;
  toasts.items.push({ ...spec, id });
  setTimeout(() => dismiss(id), spec.ms ?? 6500);
  return id;
}
export function dismiss(id: number): void {
  toasts.items = toasts.items.filter((t) => t.id !== id);
}
```

Test: `push` adds, the timer removes, `dismiss` is idempotent (use `vi.useFakeTimers()`). `Toast.svelte` renders `toasts.items` into the `#toasts` container with `role="status"`, an optional action button, and a close `x` — text rendered as text (a toast can carry an `IpcError.message` straight from a source system).

- [ ] **Step 5: `just check`, QA, commit**

```bash
git add app/src/lib/shell
git commit -m "shell: flap, toast, modal and monogram primitives"
```

---

### Task 4: App lifecycle backend — `Lifecycle`, `Profile`, `app_status`, `frontend_ready`

**Files:**
- Create: `crates/knobas-app/src/commands/app.rs`, `app/src/lib/ipc/app.ts`
- Modify (orchestrator edits requested): `crates/knobas-app/src/commands/mod.rs` (`pub mod app;`), `crates/knobas-app/src/lib.rs` (three lines: `app.manage(Profile::from_args(std::env::args()))`, `app.manage(Lifecycle::new())` **before** the database starts, and the two handlers in `generate_handler!`), `app/src/lib/ipc/index.ts` (re-export)

**Interfaces:**
- Consumes: the contract's §2.1 shapes, verbatim.
- Produces:

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DbState { Starting { detail: Option<String> }, Migrating, Ready, Failed { message: String } }

#[derive(Debug, serde::Serialize)]
pub struct AppStatus { pub db: DbState, pub first_run: bool, pub demo: bool,
                       pub source_count: u32, pub app_version: String }

/// The database's bring-up state, managed at build time so `app_status` can
/// answer before `AppState` exists (interfaces §2.1).
pub struct Lifecycle { /* Mutex<DbState> */ }
impl Lifecycle {
    pub fn new() -> Self;              // DbState::Starting { detail: None }
    pub fn get(&self) -> DbState;
    pub fn set(&self, state: DbState); // stream F's bring-up writes here, then emits db:state
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Profile { pub demo: bool }
impl Profile { pub fn from_args<I: IntoIterator<Item = String>>(args: I) -> Self; }

#[tauri::command] pub fn ping() -> &'static str;                     // moved from commands.rs, unchanged
#[tauri::command] pub async fn app_status(app: tauri::AppHandle) -> AppStatus;
#[tauri::command] pub fn frontend_ready(app: tauri::AppHandle) -> Result<(), IpcError>;
```

- TS mirror `app/src/lib/ipc/app.ts`: `ping()`, `appStatus()`, `frontendReady()`, `type DbState`, `interface AppStatus`.

> **Open question D2 (see §Open questions):** `app_status` is declared `pub fn` in interfaces §2.1. It is written `pub async fn` here. The ruling's stated reason was that it must **not take `State<'_, AppState>`** — it does not; it uses `app.try_state::<AppState>()`, which returns `None` while Postgres is coming up, which is exactly the "still starting" case. `async` is what lets `source_count` and `first_run` be read *live* instead of from a cache that goes stale the moment a source is added. The TS mirror returns a `Promise` either way, so the frontend is unaffected. **Fallback if the orchestrator insists on `fn`:** keep the counts in `Lifecycle`, refreshed by a task spawned from `frontend_ready`, and document them as boot-time truth (the sources view is authoritative afterwards).

- [ ] **Step 1: Write the failing Rust tests**

`crates/knobas-app/src/commands/app.rs`, at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_state_serialises_as_the_contract_declares() {
        let json = serde_json::to_value(DbState::Starting { detail: Some("downloading".into()) }).unwrap();
        assert_eq!(json, serde_json::json!({ "state": "starting", "detail": "downloading" }));
        assert_eq!(serde_json::to_value(DbState::Ready).unwrap(), serde_json::json!({ "state": "ready" }));
        assert_eq!(
            serde_json::to_value(DbState::Failed { message: "no port".into() }).unwrap(),
            serde_json::json!({ "state": "failed", "message": "no port" })
        );
    }

    #[test]
    fn lifecycle_starts_starting_and_remembers_the_last_set() {
        let life = Lifecycle::new();
        assert_eq!(life.get(), DbState::Starting { detail: None });
        life.set(DbState::Migrating);
        assert_eq!(life.get(), DbState::Migrating);
    }

    #[test]
    fn the_demo_profile_comes_from_the_command_line() {
        let args = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert!(Profile::from_args(args(&["knobas", "--demo"])).demo);
        assert!(!Profile::from_args(args(&["knobas"])).demo);
        // A value that merely contains the word is not the flag.
        assert!(!Profile::from_args(args(&["knobas", "--db-url=demo"])).demo);
    }
}
```

Run: `cargo test -p knobas-app` → FAIL (module missing).

- [ ] **Step 2: Implement the module**

`commands/app.rs`: `Lifecycle` wraps `std::sync::Mutex<DbState>` and recovers from poisoning with `unwrap_or_else(PoisonError::into_inner)` (same pattern as `lib.rs`'s `shutdown_database`). `Profile::from_args` skips argv[0] and looks for an exact `--demo`.

```rust
#[tauri::command]
pub async fn app_status(app: tauri::AppHandle) -> AppStatus {
    let db = app.state::<Lifecycle>().get();
    let demo = app.state::<Profile>().demo;
    let app_version = app.package_info().version.to_string();

    // Before the pool exists there is nothing to count, and saying "0 sources"
    // while starting is *not* the same as "first run" -- so both stay false-y
    // until the database can answer.
    let (source_count, first_run) = match app.try_state::<crate::AppState>() {
        Some(state) => counts(&state.pool).await.unwrap_or((0, false)),
        None => (0, false),
    };
    AppStatus { db, first_run, demo, source_count, app_version }
}
```

`counts` runs two statements: `select count(*) from knobas.source_config` and `select exists(select 1 from knobas.setting where key = 'first_run.completed')`; `first_run = source_count == 0 && !completed`.

> `knobas.setting` arrives with migration `0002` (interfaces §1 item 6: "first-run completion"). If `0002` is not merged when this task runs, the `setting` half returns `false` behind a `// 0002` comment and Task 20 wires it — do **not** create a migration.

```rust
#[tauri::command]
pub fn frontend_ready(app: tauri::AppHandle) -> Result<(), IpcError> {
    // Gotcha 9: the backend may not emit before the webview listens. The
    // frontend registers its listeners, calls this, and gets the current
    // state replayed -- so a `db:state` fired during startup is never lost.
    use tauri::Emitter as _;
    let state = app.state::<Lifecycle>().get();
    app.emit(crate::events::DB_STATE, state)
        .map_err(|e| IpcError::internal(e.to_string()))?;
    Ok(())
}
```

`crate::events::DB_STATE` and `IpcError` are orchestrator-owned (contract PR). **Import them; do not declare them.** If the contract PR has not landed, this task stops and the orchestrator is asked — a locally-declared `IpcError` would be exactly the divergence §8 P1 exists to prevent.

- [ ] **Step 3: Run the tests**

Run: `cargo test -p knobas-app` → PASS.

- [ ] **Step 4: Write the TS mirror**

`app/src/lib/ipc/app.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";

/** `crates/knobas-app/src/commands/app.rs::DbState` (tagged on `state`). */
export type DbState =
  | { state: "starting"; detail: string | null }
  | { state: "migrating" }
  | { state: "ready" }
  | { state: "failed"; message: string };

/** `crates/knobas-app/src/commands/app.rs::AppStatus`. */
export interface AppStatus {
  db: DbState;
  /** No source configured and the first run was never completed (§14a). */
  first_run: boolean;
  /** The `--demo` profile: its own data dir, port and keychain suffix (P13). */
  demo: boolean;
  source_count: number;
  app_version: string;
}

export function ping(): Promise<string> { return invoke<string>("ping"); }
export function appStatus(): Promise<AppStatus> { return invoke<AppStatus>("app_status"); }
/** Arms event emission: the backend replays the current `db:state` (gotcha 9). */
export function frontendReady(): Promise<void> { return invoke<void>("frontend_ready"); }
```

Add a mirror-shape test `app/src/lib/shell/ipc-app.test.ts` that installs the fake bridge, calls `appStatus()`, and asserts the discriminated union narrows (`if (s.db.state === "failed") s.db.message`) — a compile-time guarantee made executable.

- [ ] **Step 5: `just check` and commit**

```bash
git add crates/knobas-app/src/commands app/src/lib/ipc/app.ts app/src/lib/shell
git commit -m "commands: app lifecycle, profile and status"
```

---

### Task 5: Async bring-up and the real loading state

**Files:**
- Create: `app/src/lib/shell/lifecycle.svelte.ts`, `app/src/lib/shell/Booting.svelte`, `app/src/lib/shell/lifecycle.test.ts`
- Modify: `app/src/App.svelte` (replaces the M0 demo/search page entirely), delete `app/src/lib/shell/greeting.svelte` + its test
- Modify (orchestrator edits requested): `crates/knobas-app/tauri.conf.json` (`"visible": true`), `crates/knobas-app/src/lib.rs` (bring-up moves off the `setup` hook onto `tauri::async_runtime::spawn`, writing `Lifecycle` at each transition), `crates/knobas-app/src/lib.rs`'s hidden-window test

**Interfaces:**
- Consumes: `appStatus`, `frontendReady`, `DbState` (Task 4); `EVENTS.DB_STATE` from the barrel.
- Produces: `lifecycle` — a rune-backed singleton with `{ db: DbState, status: AppStatus | null, ready: boolean, error: string | null }`, `start(): Promise<void>`, `stop(): void`, `retry(): Promise<void>`.

**Carry-over discharged:** *"Async DB bring-up with a loading state — the exit wave hid the frozen window (`visible: false` + show-when-ready), but first-run download/initdb still blocks the event loop; a real loading screen replaces that."*

- [ ] **Step 1: Write the failing lifecycle test**

`app/src/lib/shell/lifecycle.test.ts`:

```ts
import { expect, test, vi, beforeEach } from "vitest";

vi.mock("../ipc/app", () => ({
  appStatus: vi.fn(),
  frontendReady: vi.fn(async () => undefined),
  ping: vi.fn(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { appStatus, frontendReady } from "../ipc/app";
import { createLifecycle } from "./lifecycle.svelte";

beforeEach(() => vi.clearAllMocks());

test("polls until ready, then stops polling", async () => {
  vi.mocked(appStatus)
    .mockResolvedValueOnce({ db: { state: "starting", detail: "downloading postgres" },
                             first_run: true, demo: true, source_count: 0, app_version: "0.1.0" })
    .mockResolvedValueOnce({ db: { state: "migrating" },
                             first_run: true, demo: true, source_count: 0, app_version: "0.1.0" })
    .mockResolvedValue({ db: { state: "ready" },
                         first_run: true, demo: true, source_count: 0, app_version: "0.1.0" });

  const life = createLifecycle({ pollMs: 1 });
  await life.start();
  expect(frontendReady).toHaveBeenCalledOnce();   // listeners first, then arm the replay
  await vi.waitFor(() => expect(life.ready).toBe(true));
  const callsAtReady = vi.mocked(appStatus).mock.calls.length;
  await new Promise((r) => setTimeout(r, 10));
  expect(vi.mocked(appStatus).mock.calls.length).toBe(callsAtReady);
  life.stop();
});

test("a failed bring-up is surfaced, not swallowed, and can be retried", async () => {
  vi.mocked(appStatus).mockResolvedValue({ db: { state: "failed", message: "port 5432 in use" },
    first_run: false, demo: false, source_count: 0, app_version: "0.1.0" });
  const life = createLifecycle({ pollMs: 1 });
  await life.start();
  await vi.waitFor(() => expect(life.error).toBe("port 5432 in use"));
  expect(life.ready).toBe(false);
  life.stop();
});
```

Run → FAIL.

- [ ] **Step 2: Implement `lifecycle.svelte.ts`**

Two channels, deliberately: **the event makes it instant, the poll makes it correct.** A dropped event, an emit that raced the listener, or a backend that never got to `emit` all end in the same place — a poll that keeps asking until the answer is `ready` or `failed`.

```ts
import { listen } from "@tauri-apps/api/event";

import { appStatus, frontendReady, type AppStatus, type DbState } from "../ipc/app";
import { EVENTS } from "../ipc";

export function createLifecycle(opts: { pollMs?: number } = {}) {
  const pollMs = opts.pollMs ?? 500;
  const state = $state<{ db: DbState; status: AppStatus | null }>({
    db: { state: "starting", detail: null }, status: null,
  });
  let timer: ReturnType<typeof setTimeout> | undefined;
  let unlisten: (() => void) | undefined;
  let stopped = false;

  async function poll() {
    if (stopped) return;
    try {
      const status = await appStatus();
      state.status = status;
      state.db = status.db;
    } catch { /* the bridge is not up yet; the next tick asks again */ }
    if (!stopped && state.db.state !== "ready" && state.db.state !== "failed") {
      timer = setTimeout(() => void poll(), pollMs);
    }
  }

  return {
    get db() { return state.db; },
    get status() { return state.status; },
    get ready() { return state.db.state === "ready"; },
    get error() { return state.db.state === "failed" ? state.db.message : null; },
    async start() {
      // Listener first, then `frontend_ready` -- that ordering is the whole
      // point of the command (gotcha 9).
      const off = await listen<DbState>(EVENTS.DB_STATE, (e) => {
        state.db = e.payload;
        if (e.payload.state === "ready") void poll();   // refresh the counts
      });
      if (stopped) { off(); return; }                    // unmounted mid-await
      unlisten = off;
      await frontendReady().catch(() => undefined);
      await poll();
    },
    stop() { stopped = true; clearTimeout(timer); unlisten?.(); },
    async retry() { stopped = false; await poll(); },
  };
}

export const lifecycle = createLifecycle();
```

> `EVENTS` comes from the orchestrator's barrel (`app/src/lib/ipc/index.ts`). If it is not there yet, request the constant in this PR; do **not** hard-code `"db:state"` in a D-owned file.

- [ ] **Step 3: Write `Booting.svelte` and rewrite `App.svelte`**

`Booting.svelte` renders the four states over `--bg` with `--disp` type, mono detail, and no spinner-that-lies:

| `db.state` | Screen |
|---|---|
| `starting` | "Starting the local database" + `detail` (e.g. "downloading PostgreSQL 18.6 — first run only") in mono |
| `migrating` | "Bringing the schema up to date" |
| `failed` | The message as text, a **Retry** button (`lifecycle.retry()`), and the copyable line "knobas <version> · profile <demo\|default>" |
| `ready` | never rendered — `App.svelte` has switched to `Shell` |

`App.svelte` (Task 5 version — Task 6 fills the shell):

```svelte
<script lang="ts">
  import { onMount } from "svelte";

  import Booting from "./lib/shell/Booting.svelte";
  import Toast from "./lib/shell/Toast.svelte";
  import { lifecycle } from "./lib/shell/lifecycle.svelte";

  if (import.meta.env.DEV) {
    void import("./lib/shell/dev/fake-tauri").then((m) => m.installIfRequested());
  }

  onMount(() => {
    void lifecycle.start();
    return () => lifecycle.stop();
  });
</script>

{#if lifecycle.ready}
  <!-- Task 6 mounts <Shell /> here. -->
{:else}
  <Booting db={lifecycle.db} version={lifecycle.status?.app_version ?? ""}
           demo={lifecycle.status?.demo ?? false} onretry={() => void lifecycle.retry()} />
{/if}
<Toast />
```

Delete `greeting.svelte` and `greeting.test.ts` — the harness has real components to prove itself on now.

- [ ] **Step 4: Move the bring-up off the setup hook (orchestrator edits)**

Request in `crates/knobas-app/src/lib.rs`:

1. `tauri.conf.json`: `"visible": true`. **The window now appears immediately** — that is the point of the task — and `lib.rs`'s `the_main_window_is_declared_hidden_under_the_label_run_shows` test must be rewritten to assert `visible == true` *and* that `run()` no longer calls `.show()`. A test asserting the opposite of the new behaviour is worse than no test.
2. In `setup`: `app.manage(Profile::from_args(std::env::args())); app.manage(Lifecycle::new());` then `tauri::async_runtime::spawn(async move { … })` around the existing `start_database`, with `lifecycle.set(...)` + `emit(events::DB_STATE, …)` at `Starting`/`Migrating`/`Ready`/`Failed`. `block_on` disappears.
3. `generate_handler!` gains `commands::app::app_status, commands::app::frontend_ready` and `commands::ping` becomes `commands::app::ping`.

> **Stream boundary:** the events table (interfaces §2.3) assigns `db:state` emission to stream **F** ("F (bring-up), replayed by `frontend_ready`"). If F's bring-up PR lands first, this task only consumes it and skips edit 2. If D lands first, D requests the minimal spawn+`set` above and F's PR takes it over. Either order works because the frontend polls; note in the PR body which order actually happened.

- [ ] **Step 5: Verify end to end**

Run: `just check` → PASS.
Run: `just dev` — the window must appear within a second showing "Starting the local database", then switch to the (empty) ready state. On a machine with the PG binaries already downloaded this is fast; to see the slow path, `mv ~/.theseus ~/.theseus.bak` first and put it back after. Record the observed sequence in the PR body.
QA: headless screenshot of `?fake-ipc` is not useful here (the fake answers `ready` immediately); instead add a `?fake-ipc&fake-db=starting` switch to `demoHandlers()` and screenshot the loading screen.

- [ ] **Step 6: Commit**

```bash
git add app/src crates/knobas-app
git commit -m "shell: real loading state over an async database bring-up"
```

---

### Task 6: Hash router, the app frame, and the Esc unwind ladder

**Files:**
- Create: `app/src/lib/shell/router.svelte.ts`, `app/src/lib/shell/router.test.ts`, `app/src/lib/shell/Shell.svelte`, `app/src/lib/shell/TopStrip.svelte`, `app/src/lib/shell/StatusBar.svelte`, `app/src/lib/shell/keys.ts`
- Modify: `app/src/App.svelte`

**Interfaces:**
- Consumes: Task 5's `lifecycle`.
- Produces:

```ts
export type Route =
  | { view: "room"; ctx: string; detail: { kind: string; entityId: string } | null }
  | { view: "sources" }
  | { view: "first-run" }
  | { view: "unknown"; hash: string };

export const router: {
  readonly route: Route;
  go(hash: string): void;         // the only navigator
  back(): void;                   // one rung of the ladder
  start(): () => void;            // installs hashchange + keydown; returns teardown
};
export function parseHash(hash: string): Route;
export function hashFor(route: Route): string;
```

**Addressing (spec §2 "Entity addressing", adopted as the navigation contract):**

| Address | Meaning |
|---|---|
| `#/ctx/<id>` | a room. M1 ids: `all` (built-in) and, from Task 18, `src:<source_id>` |
| `#/<kind>/<entity_id>` | the detail slide-over over the current room, e.g. `#/ticket/mock:PAY-231` |
| `#/entity/<entity_id>` | kind-agnostic alias; resolves via `get_entity` then `replaceState`s to the canonical form |
| `#/sources` | the sources view |
| `#/first-run` | the §14a wizard |

> **Deviation from the mockup, recorded here:** the mockup addressed `#/ticket/PAY-231`. M1 ids are namespaced (`mock:PAY-231`) because P10 makes the instance id part of the entity id and two Jiras (`jira`, `jira-eu`) would otherwise collide on one key. `:` is legal in a URI fragment, so the address stays readable. `#/inbox`, `#/time`, `#/standup`, `#/assets/*`, `#/route/*`, `#/monitor/*`, `#/start-work/*` are **M2–M4** and parse to `{ view: "unknown" }`, which renders a one-line "arrives in M<n>" panel rather than a blank screen.

- [ ] **Step 1: Write the failing router test**

```ts
import { expect, test } from "vitest";
import { parseHash, hashFor } from "./router.svelte";

test("parses the M1 addresses", () => {
  expect(parseHash("#/ctx/all")).toEqual({ view: "room", ctx: "all", detail: null });
  expect(parseHash("#/ticket/mock:PAY-231")).toEqual({
    view: "room", ctx: "all", detail: { kind: "ticket", entityId: "mock:PAY-231" } });
  expect(parseHash("#/sources")).toEqual({ view: "sources" });
  expect(parseHash("")).toEqual({ view: "room", ctx: "all", detail: null });
  expect(parseHash("#/inbox")).toEqual({ view: "unknown", hash: "#/inbox" });
});

test("round-trips", () => {
  for (const h of ["#/ctx/all", "#/ticket/mock:PAY-231", "#/sources"]) {
    expect(hashFor(parseHash(h))).toBe(h);
  }
});

test("percent-encoded ids survive", () => {
  expect(parseHash("#/pr/mock:payout-service%23142").detail?.entityId)
    .toBe("mock:payout-service#142");
});
```

*(A `#` inside a key — Gitea's `owner/repo#142` — **must** be percent-encoded when building the hash, or the browser truncates the fragment. `hashFor` encodes; `parseHash` decodes. That is the one non-obvious rule in this module and it gets its own test.)*

Run → FAIL.

- [ ] **Step 2: Implement the router**

`parseHash` splits on `/` **after** stripping `#/`, takes segment 0 as the view word, `decodeURIComponent`s the rest, and treats any first segment that is not `ctx|sources|first-run|entity` and is not a known M2+ word as a *kind* (open kinds, §3a — the router must not carry a closed list of kinds). Detail routes keep the room they were opened over; the room id is remembered in the rune (`ctx` defaults to `all`) so `Esc` returns to it.

`go(hash)` sets `location.hash` (or re-parses when it is already there, matching the mockup's `go()` at `signal-miller.html:2241`). `start()` registers `hashchange` and the global `keydown`, and returns the teardown that removes both.

- [ ] **Step 3: The Esc unwind ladder and ⌘K**

`keys.ts` owns the ladder. The M1 rungs, from the mockup's `signal-miller.html:4184-4202` minus the surfaces that do not exist yet:

```ts
// 1. a modal or popover is open  -> the Modal closes it and stops propagation
// 2. the detail slide-over is open -> go(`#/ctx/${ctx}`)
// 3. a non-room view is open       -> go(`#/ctx/${ctx}`)
// 4. otherwise                     -> nothing (never exit the app on Esc)
```

`⌘K` / `Ctrl+K` calls `openLauncher()`, which Task 22 wires to stream E. Until then it pushes a toast: *"Search lands with the launcher (stream E)."* — a visible, honest stub, not a dead key.
`⌘T` is **M3** and is not bound; binding it now would train a habit the app cannot honour.

Tests: dispatch `Escape` with a detail route active → asserts `location.hash === "#/ctx/all"`; dispatch with a modal marker present → asserts the route did not change.

- [ ] **Step 4: The frame**

`Shell.svelte` is the mockup's `#app` grid verbatim (`44px 1fr 24px`, `min-width: 1100px`):

```svelte
<div class="app">
  <TopStrip />
  <main class="main">{@render main()}</main>
  <StatusBar />
</div>
```

`TopStrip.svelte` — **M1 members only**: the context switcher button + tabs (Task 9 fills them; a single "All work" tab for now), the `.searchfield` (`⌘K`), the sync monograms (Task 18; an empty `.sync` span now), and the gear that navigates to `#/sources`. The timer, inbox, Assets and Today/Day buttons of spec §2 are **M2/M3/M4** and are deliberately absent — reserving a slot for a button that cannot work is how a shell fills up with dead chrome.

`StatusBar.svelte` — a clock that ticks once a second (`setInterval` in an `$effect` with a teardown; **not** a `setInterval` at module scope, which is what leaks in a hot-reloaded dev session), the app version, and the profile word (`demo` when `status.demo`). Task 14 adds the rest.

`App.svelte` renders `<Shell>` with a `main` snippet that switches on `router.route.view`.

- [ ] **Step 5: `just check`, QA, commit**

QA: headless screenshot of `?fake-ipc#/ctx/all` — a graphite window with a top strip, an empty main area and a status bar; then `?fake-ipc#/sources` to prove navigation, and an `Escape` keypress driven through `--virtual-time-budget` or a follow-up screenshot after `document.dispatchEvent`. Attach both PNGs.

```bash
git add app/src
git commit -m "shell: hash router, app frame and the esc ladder"
```

---

## Phase 1 — the corpus is readable

*Depends only on stream D's own commands plus migration `0002`'s `sync.live_item`. Develops against `--demo` + the mock source.*

---

### Task 7: `list_entities` — the room's read

**Files:**
- Create: `crates/knobas-app/src/commands/entity.rs`, `crates/knobas-app/tests/entity.rs`, `app/src/lib/ipc/entity.ts`
- Modify (orchestrator edits requested): `commands/mod.rs`, `lib.rs` handler list, `ipc/index.ts` barrel

**Interfaces:**
- Consumes: `sync.live_item` (migration `0002`), `IpcError`.
- Produces, exactly as interfaces §2.5 declares them:

```rust
#[derive(serde::Serialize)] pub struct EntityRow { pub entity_id: String, pub kind: String,
    pub source_id: String, pub title: String,
    pub updated_at: Option<DateTime<Utc>>, pub synced_at: DateTime<Utc> }
#[derive(serde::Deserialize)] pub struct EntityFilter { pub sources: Vec<String>, pub kinds: Vec<String>,
    pub updated_within_days: Option<u32>, pub order: EntityOrder, pub include_deleted: bool }
#[derive(serde::Deserialize)] #[serde(rename_all = "snake_case")] pub enum EntityOrder { UpdatedDesc, TitleAsc }
#[derive(serde::Serialize)] pub struct EntityPage { pub rows: Vec<EntityRow>, pub total: i64 }

#[tauri::command] pub async fn list_entities(state: State<'_, AppState>, filter: EntityFilter,
    limit: u32, offset: u32) -> Result<EntityPage, IpcError>;
```

- [ ] **Step 1: Write the failing integration test**

`crates/knobas-app/tests/entity.rs`:

```rust
use knobas_app::commands::entity::{list_entities_inner, EntityFilter, EntityOrder};

async fn seeded() -> sqlx::PgPool { /* test_pool + migrate + demo_load_inner */ }

#[tokio::test]
async fn lists_the_newest_first_and_reports_the_unpaged_total() {
    let pool = seeded().await;
    let filter = EntityFilter { sources: vec![], kinds: vec!["ticket".into()],
        updated_within_days: None, order: EntityOrder::UpdatedDesc, include_deleted: false };
    let page = list_entities_inner(&pool, &filter, 2, 0).await.unwrap();

    assert_eq!(page.rows.len(), 2, "limit is honoured");
    assert!(page.total > 2, "total counts the whole filtered set, not the page");
    assert!(page.rows[0].updated_at >= page.rows[1].updated_at);
    assert!(page.rows.iter().all(|r| r.kind == "ticket"));
}

#[tokio::test]
async fn a_tombstoned_entity_is_absent_unless_asked_for() {
    let pool = seeded().await;   // demo_load runs MockSource::with_tombstone? see note
    let all = EntityFilter { sources: vec![], kinds: vec![], updated_within_days: None,
        order: EntityOrder::UpdatedDesc, include_deleted: false };
    let live = list_entities_inner(&pool, &all, 100, 0).await.unwrap();
    assert!(!live.rows.iter().any(|r| r.entity_id.ends_with(":PAY-198")));

    let with_dead = EntityFilter { include_deleted: true, ..all };
    let dead = list_entities_inner(&pool, &with_dead, 100, 0).await.unwrap();
    assert!(dead.rows.iter().any(|r| r.entity_id.ends_with(":PAY-198")));
}

#[tokio::test]
async fn title_order_is_a_second_statement_not_string_interpolation() {
    let pool = seeded().await;
    let f = EntityFilter { sources: vec![], kinds: vec![], updated_within_days: None,
        order: EntityOrder::TitleAsc, include_deleted: false };
    let page = list_entities_inner(&pool, &f, 100, 0).await.unwrap();
    let mut sorted = page.rows.iter().map(|r| r.title.clone()).collect::<Vec<_>>();
    sorted.sort();
    assert_eq!(page.rows.iter().map(|r| r.title.clone()).collect::<Vec<_>>(), sorted);
}
```

*(To reach the tombstone, seed with `knobas_source_mock::MockSource::with_tombstone()` through `knobas_sync::run_once` rather than through `demo_load_inner`; write that helper in the test file.)*

Run: `cargo test -p knobas-app --test entity` → FAIL.

- [ ] **Step 2: Implement**

The command is a shim; `list_entities_inner(pool, filter, limit, offset)` holds the behaviour and the tests point at it (same split as M0's `demo_load_inner`).

**Four static statements, no string building.** Optional predicates become nullable parameters, which is what keeps this out of `AssertSqlSafe` territory entirely (gotcha 2):

```rust
const LIVE_UPDATED: &str = r#"
select entity_id, source_id, kind, title, item_updated_at, synced_at,
       count(*) over () as total
  from sync.live_item
 where ($1::text[] is null or source_id = any($1))
   and ($2::text[] is null or kind      = any($2))
   and ($3::int is null or item_updated_at >= now() - make_interval(days => $3))
 order by item_updated_at desc nulls last, entity_id
 limit $4 offset $5"#;
```

`LIVE_TITLE` is the same with `order by title asc, entity_id`. `ALL_UPDATED` / `ALL_TITLE` read `sync.item i join knobas.entity e on e.id = i.entity_id` without the `deleted_at is null` filter (`include_deleted: true`). **Never `select *`** — `live_item.fts` is a `tsvector` and mapping it to a `String` is gotcha 2's second half.

Empty `Vec<String>` binds as `None::<Vec<String>>`, not as an empty array (`= any('{}')` matches nothing). `count(*) over ()` gives the unpaged total in the same round trip and is computed before `limit`.

Row mapping is manual (`sqlx::Row::get`) so the `total` column can be read off the first row without a `FromRow` struct that pretends it belongs to `EntityRow`; `total` is `0` for an empty page.

- [ ] **Step 3: Run the tests** → PASS.

- [ ] **Step 4: The TS mirror**

`app/src/lib/ipc/entity.ts` — mirrors of the four types plus:

```ts
export function listEntities(filter: EntityFilter, limit: number, offset: number): Promise<EntityPage> {
  return invoke<EntityPage>("list_entities", { filter, limit, offset });
}
```

Every field documented in one line; `updated_at: string | null` carries the note *"the source's own timestamp — a source that reports none leaves it null; never `now()`"* (interfaces §4.1 normalization).

- [ ] **Step 5: `just check` and commit**

```bash
git add crates/knobas-app app/src/lib/ipc/entity.ts
git commit -m "commands: list_entities over the live-item view"
```

---

### Task 8: `get_entity` and entity-scoped `recent_activity`

**Files:**
- Modify: `crates/knobas-app/src/commands/entity.rs`, `crates/knobas-app/tests/entity.rs`, `app/src/lib/ipc/entity.ts`, `crates/knobas-core/src/activity.rs` *(see open question D5)*
- Modify (orchestrator edits requested): `lib.rs` handler list

**Interfaces:**
- Produces, per interfaces §2.5:

```rust
#[derive(serde::Serialize)] pub struct SourceRef { pub id: String, pub display_name: String, pub adapter_kind: String }
#[derive(serde::Serialize)] pub struct EntityDetail {
    pub row: EntityRow, pub source: SourceRef,
    pub kind_info: Option<knobas_source::KindInfo>,
    pub body_text: String, pub author: Option<String>,
    pub payload: serde_json::Value, pub web_url: Option<String>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub links: Vec<knobas_core::link::LinkRow>,
    pub activity: Vec<knobas_core::activity::ActivityRow> }

#[tauri::command] pub async fn get_entity(state: State<'_, AppState>, entity_id: String)
    -> Result<EntityDetail, IpcError>;
#[tauri::command] pub async fn recent_activity(state: State<'_, AppState>, limit: u32,
    entity_id: Option<String>) -> Result<Vec<ActivityRow>, IpcError>;
```

- `knobas_core::activity::recent(pool, limit, entity_id: Option<&str>)` — the existing `recent` gains the filter (one caller, updated in the same PR). The global path uses `0002`'s `activity_recent_idx (at desc, id desc)`; the filtered path uses `0001`'s `(entity_id, at desc)`.

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn returns_the_row_its_source_and_the_raw_payload() {
    let pool = seeded().await;
    let d = get_entity_inner(&pool, "mock:PAY-231").await.unwrap();
    assert_eq!(d.row.kind, "ticket");
    assert_eq!(d.source.id, "mock");
    assert_eq!(d.source.display_name, "Tidewater (demo)");  // from source_config
    assert!(d.body_text.contains("SEPA"));
    assert_eq!(d.payload["key"], "PAY-231", "payload is the source record verbatim (§3a)");
    assert!(d.links.is_empty(), "links are M2");
    assert!(d.deleted_at.is_none());
}

#[tokio::test]
async fn an_unknown_id_is_not_found_not_internal() {
    let pool = seeded().await;
    let err = get_entity_inner(&pool, "mock:NOPE-1").await.unwrap_err();
    assert!(matches!(err.code, IpcErrorCode::NotFound));
}

#[tokio::test]
async fn a_malformed_id_is_invalid() {
    let pool = seeded().await;
    let err = get_entity_inner(&pool, "no-colon-here").await.unwrap_err();
    assert!(matches!(err.code, IpcErrorCode::Invalid));
}

#[tokio::test]
async fn a_tombstoned_entity_is_still_readable_and_says_so() {
    // §5a: links and notes point at entities that vanished upstream; the
    // detail view has to be able to show what the user linked to.
    let pool = seeded_with_tombstone().await;
    let d = get_entity_inner(&pool, "mock:PAY-198").await.unwrap();
    assert!(d.deleted_at.is_some());
}

#[tokio::test]
async fn activity_can_be_scoped_to_one_entity() {
    let pool = seeded().await;
    knobas_core::activity::record(&pool, "user", "opened", Some("mock:PAY-231"), json!({})).await.unwrap();
    knobas_core::activity::record(&pool, "user", "opened", Some("mock:c90d11"), json!({})).await.unwrap();
    let scoped = recent_activity_inner(&pool, 10, Some("mock:PAY-231")).await.unwrap();
    assert_eq!(scoped.len(), 1);
    assert_eq!(recent_activity_inner(&pool, 10, None).await.unwrap().len() >= 2, true);
}
```

- [ ] **Step 2: Implement**

`get_entity_inner`: `EntityRef::parse` first (→ `Invalid` on failure, which is how a bad deep link reports itself rather than as a 500), then one statement over `sync.item i join knobas.entity e ... left join knobas.source_config c on c.id = i.source_id` naming its columns — deleted rows included, with `e.deleted_at` returned. `SourceRef` falls back to `display_name = source_id`, `adapter_kind = source_id` when there is no `source_config` row (interfaces §1: `run_once` syncs unconfigured sources, so the join must be `left`). `links` = `knobas_core::link::links_of`. `activity` = `recent(pool, 20, Some(&entity_id))`.

`kind_info` is **`None` in this task** — resolving it needs stream F's adapter registry (`crates/knobas-app/src/sources/**`), which does not exist yet. Task 21 fills it. The field is in the contract, so it ships now, with a doc comment saying exactly that.

`web_url` is **`None` in this task** — see **open question D4**: P5 grants `SyncItem.web_url`, but migration `0002` adds no column to `sync.item` to persist it and `sync.live_item` does not select one. Until the orchestrator rules, `None` is the contract-conformant answer (P5: "adapters that cannot produce one leave it `None` and the button is absent").

- [ ] **Step 3: Run the tests** → PASS.

- [ ] **Step 4: The TS mirror + `recentActivity`**

```ts
export function getEntity(entityId: string): Promise<EntityDetail> {
  return invoke<EntityDetail>("get_entity", { entityId });
}
export function recentActivity(limit: number, entityId?: string): Promise<ActivityRow[]> {
  return invoke<ActivityRow[]>("recent_activity", { limit, entityId });
}
```

`payload: unknown` (per the contract's `serde_json::Value` mapping) with the comment: **"untrusted source text — project it as text, never as markup (interfaces §2.5, gotcha 7)."**

- [ ] **Step 5: `just check` and commit**

```bash
git add crates/knobas-app crates/knobas-core app/src/lib/ipc/entity.ts
git commit -m "commands: get_entity and entity-scoped activity"
```

---

### Task 9: Context tabs and the room bar

**Files:**
- Create: `app/src/lib/shell/contexts.ts`, `app/src/lib/shell/ContextTabs.svelte`, `app/src/lib/shell/RoomBar.svelte`, `app/src/lib/shell/contexts.test.ts`
- Modify: `app/src/lib/shell/TopStrip.svelte`

**Interfaces:**
- Produces:

```ts
export interface RoomContext {
  id: string;            // "all" | "src:<source_id>"
  label: string;         // "All work" | the source's display name
  kindWord: string;      // the `.kind` chip: "everything synced" | "source"
  filter: Pick<EntityFilter, "sources">;
}
export function builtinContexts(sources: { id: string; label: string }[]): RoomContext[];
export function contextById(id: string, contexts: RoomContext[]): RoomContext;  // falls back to "all"
```

**Scope, stated plainly:** spec §7 contexts (epic / ticket / ad-hoc, membership through links) are **M2** — they need `knobas.link`, which M1 never writes. So M1's context tabs are **read-only and derived**: one built-in "All work" room, plus one room per configured source once stream F can list them (Task 18). `+ new` renders disabled with the title "Contexts arrive in M2". This keeps the switcher on screen, honest, and one prop away from reading `knobas.context` when M2 lands.

- [ ] **Step 1: Write the failing test**

```ts
test("always offers All work first, then one room per source", () => {
  const cs = builtinContexts([{ id: "jira", label: "Tidewater Jira" }, { id: "gitea", label: "Gitea" }]);
  expect(cs.map((c) => c.id)).toEqual(["all", "src:jira", "src:gitea"]);
  expect(cs[0]?.filter.sources).toEqual([]);
  expect(cs[1]?.filter.sources).toEqual(["jira"]);
});

test("an unknown context id falls back to All work rather than blanking the room", () => {
  expect(contextById("src:gone", builtinContexts([])).id).toBe("all");
});
```

- [ ] **Step 2: Implement and wire into `TopStrip`**

`ContextTabs.svelte` renders the mockup's `.ctx-switch` + `.tabs` (`signal-miller.html:2283-2289`): the current context as `.ctx-name` (opening a `.pop` popover listing all of them), then one `.tab` each, `.tab.on` for the active one, and the disabled `.tab.new`. Navigation is `router.go(`#/ctx/${id}`)` — never a local `selected` variable, because the address *is* the state (spec §2).

`RoomBar.svelte` renders `.room-bar` with `h1` = the context label, the `.kind` chip, and an `.acts` area that in M1 holds nothing (the adaptive action bar of spec §2 needs *Start work* / *Trigger build*, which are M2 write-backs; an action bar of disabled buttons is worse than none). The slot stays in the markup with a comment naming M2.

- [ ] **Step 3: Test the component**

Mount `ContextTabs` with two contexts, assert one `.tab.on`, click the other, assert `location.hash`.

- [ ] **Step 4: `just check`, QA, commit**

```bash
git add app/src/lib/shell
git commit -m "shell: read-only context tabs and the room bar"
```

---

### Task 10: The room and its read-only tiles

**Files:**
- Create: `app/src/lib/shell/Room.svelte`, `app/src/lib/shell/Tile.svelte`, `app/src/lib/shell/EntityLine.svelte`, `app/src/lib/shell/kinds.ts`, `app/src/lib/shell/kinds.test.ts`, `app/src/lib/shell/Tile.test.ts`
- Modify: `app/src/App.svelte`, `app/src/lib/shell/dev/fake-tauri.ts`

**Interfaces:**
- Consumes: `listEntities` (Task 7), `RoomContext` (Task 9).
- Produces:

```ts
export interface TileSpec { id: string; label: string; kinds: string[] }
/** The mockup's five tiles, plus one tile per kind nothing claims (§3a open kinds). */
export function tilesFor(kinds: string[]): TileSpec[];
export function kindLabel(kind: string, info?: KindInfo | null): string;    // "Pull requests"
export function kindMonogram(kind: string, info?: KindInfo | null): string; // "PR"
```

- [ ] **Step 1: Write the failing kinds test**

```ts
test("known kinds land in the mockup's tiles", () => {
  const tiles = tilesFor(["ticket", "pr", "commit", "build", "page"]);
  expect(tiles.map((t) => t.id)).toEqual(["tickets", "code", "builds", "docs"]);
  expect(tiles.find((t) => t.id === "code")?.kinds).toEqual(["pr", "commit"]);
});

test("a kind no tile claims gets its own tile (open kinds, §3a)", () => {
  const tiles = tilesFor(["ticket", "incident"]);
  expect(tiles.map((t) => t.id)).toEqual(["tickets", "incident"]);
  expect(tiles[1]?.label).toBe("Incidents");
});

test("labels and monograms come from the adapter when it declares them", () => {
  const info = { id: "incident", label: "Incident", plural: "Incidents", monogram: "IN" };
  expect(kindMonogram("incident", info)).toBe("IN");
  expect(kindMonogram("incident", null)).toBe("IN");     // fallback: first two letters, upper
  expect(kindLabel("build_config", null)).toBe("Build configs");
});
```

`tilesFor` uses a **declared bucket map with a generic fallback** — the compromise §3a permits: known kinds keep the designed room (`ticket → Tickets`, `pr|commit|branch|repo → Code`, `build → Builds`, `page → Docs`, `note → Notes`), and anything undeclared gets its own tile rather than being dropped. A tile with no kinds present is not rendered.

- [ ] **Step 2: Implement `Tile.svelte`**

Each tile **fetches its own page** — that is what "components own their state" means here, and it is why the mockup's single `render()` sweep does not carry over:

```svelte
<script lang="ts">
  import { listEntities, type EntityPage } from "../ipc/entity";
  import { asIpcError } from "./errors";

  let { spec, sources }: { spec: TileSpec; sources: string[] } = $props();

  let page = $state<EntityPage | null>(null);
  let error = $state<string | null>(null);
  let token = 0;

  $effect(() => {
    const mine = ++token;
    const filter = { sources, kinds: spec.kinds, updated_within_days: null,
                     order: "updated_desc" as const, include_deleted: false };
    void listEntities(filter, 50, 0)
      .then((p) => { if (mine === token) { page = p; error = null; } })
      .catch((e) => { if (mine === token) { page = null; error = asIpcError(e).message; } });
  });
</script>
```

The `token` guard is the M0 `App.svelte` out-of-order-response defence, kept because it is still right: a slow tile must not overwrite a newer context's answer.

Markup: `.tile > .tile-h (.lab, .cnt) + .tile-b`, rows via `EntityLine.svelte` (`.row.g5w`: monogram, key, title, source, sync age). Empty state uses `.empty` with a sentence per tile, taken from the mockup's wording where it still applies and rewritten where it promised an action M1 does not have (`signal-miller.html:2380,2416,2428,2437,2446` promise *New ticket*, *Git…*, *Trigger build…* — all M2 write-backs; M1's empty tile says what is missing and nothing more).

Tickets get the mockup's four-column mini board (`.board > .col`) **only when the rows carry a status**, which in M1 they do not (the SPI has no normalized status field until M2). So M1's Tickets tile is a list like the others, and the board markup is not written — a board of four empty columns is a lie about the data.

- [ ] **Step 3: Implement `Room.svelte`**

Reads `router.route.ctx` → `contextById` → `tilesFor(kinds present)`. To know which kinds are present it makes one `listEntities({...ctx.filter, kinds: []}, 200, 0)` call and takes the distinct kinds — one round trip, and it doubles as the room's "N items" count. `.tiles` is the mockup's grid; with fewer than six tiles the CSS grid rows are adjusted by a modifier class `.tiles.rows-<n>` added to `app.css` (a class, not an inline `grid-template-rows`).

- [ ] **Step 4: Test the tile**

Mount `Tile` with `vi.mock("../ipc/entity")` returning a page of three rows; assert three `.row`s, the `.cnt` shows the total, an `IpcError` renders in `.empty` and not as a blank tile, and a second `$effect` run with a new `sources` prop does not leave the old rows on screen.

- [ ] **Step 5: `just check`, QA, commit**

QA screenshot: `?fake-ipc#/ctx/all` must now show five populated tiles from the demo fixture. This is the first screenshot that should look like the mockup — compare it side by side with `mockups/round-3/signal-miller.html` opened in the same browser and note any drift in the PR body.

```bash
git add app/src/lib/shell app/src/App.svelte
git commit -m "room: read-only tiles over list_entities"
```

---

### Task 11: The detail slide-over and the §3a generic payload view

**Files:**
- Create: `app/src/lib/detail/Detail.svelte`, `app/src/lib/detail/PayloadView.svelte`, `app/src/lib/detail/payload.ts`, `app/src/lib/detail/payload.test.ts`, `app/src/lib/detail/Detail.test.ts`
- Modify: `app/src/lib/shell/Room.svelte` (renders the slide-over over its right half), `app/src/lib/shell/dev/fake-tauri.ts`

**Interfaces:**
- Consumes: `getEntity` (Task 8).
- Produces:

```ts
export type Projected =
  | { key: string; label: string; kind: "scalar"; text: string }
  | { key: string; label: string; kind: "text"; text: string }         // long strings
  | { key: string; label: string; kind: "nested"; summary: string; json: string };
export function projectPayload(payload: unknown): Projected[];
```

**Spec §3a, verbatim:** *"an **unknown kind gets a generic detail view** — title, metadata fields projected from the raw payload, body text, the links panel, and actions derived from the adapter's declared capabilities. A new ticket system is browsable on day one."*

**Scope, stated plainly:** in M1 **every** kind uses this view. The tailored per-kind detail views of spec §5 (status dropdown, approve, re-run, section edit) are write-backs — M2's identity release — and the *read* halves of those views cannot be built either, because the fields they show (`status`, `approvals`, `checks`) exist only inside each adapter's own `payload` shape, which differs per adapter. What differs per kind in M1 is the **header**: `kind_info` supplies the label and monogram when the adapter declares one (Task 21), the title-cased kind otherwise. That is exactly the exit criterion's "renders a known kind and, for an undeclared kind, the §3a generic view".

- [ ] **Step 1: Write the failing projection test**

```ts
test("projects scalars in declaration order", () => {
  const out = projectPayload({ key: "PAY-231", status: "In Progress", story_points: 3, blocked: false });
  expect(out.map((f) => [f.key, f.text]))
    .toEqual([["key", "PAY-231"], ["status", "In Progress"], ["story_points", "3"], ["blocked", "false"]]);
  expect(out.every((f) => f.kind === "scalar")).toBe(true);
});

test("humanises keys without losing them", () => {
  expect(projectPayload({ story_points: 1 })[0]?.label).toBe("Story points");
  expect(projectPayload({ story_points: 1 })[0]?.key).toBe("story_points");
});

test("long strings become text blocks, nested values become summaries", () => {
  const long = "x".repeat(200);
  const out = projectPayload({ desc: long, comments: [{ by: "mara" }, { by: "jonas" }], fields: { a: 1 } });
  expect(out[0]?.kind).toBe("text");
  expect(out[1]).toMatchObject({ kind: "nested", summary: "2 items" });
  expect(out[2]).toMatchObject({ kind: "nested", summary: "1 field" });
});

test("null and undefined render as an em dash, not as the word null", () => {
  expect(projectPayload({ assignee: null })[0]?.text).toBe("—");
});

test("a payload that is not an object still projects", () => {
  expect(projectPayload("just a string")).toEqual([
    { key: "payload", label: "Payload", kind: "text", text: "just a string" }]);
  expect(projectPayload(null)).toEqual([]);
});

test("markup in a payload stays text", () => {
  // The defence is that PayloadView interpolates; this asserts the projector
  // does not "helpfully" pre-render anything.
  expect(projectPayload({ desc: "<script>alert(1)</script>" })[0]?.text)
    .toBe("<script>alert(1)</script>");
});
```

- [ ] **Step 2: Implement `payload.ts` and `PayloadView.svelte`**

`PayloadView` renders `.kv` rows for scalars, a `.sec p` block for text, and a `<details><summary>` for nested values whose body is `JSON.stringify(value, null, 2)` inside `.log` — **as text**, always. Nothing here uses `{@html}` and the house-rules test enforces it.

- [ ] **Step 3: Implement `Detail.svelte`**

The mockup's `dShell` (`signal-miller.html:2546`): `<aside class="detail">` with `.d-h` (crumb "‹context› › ‹kind›", actions, close `x`), `.d-b` containing `.d-title` (monogram + id + `h2` title), `.d-meta` (source, kind, author, updated, synced age), then `.sec` blocks — Description (`body_text`), Details (the projection), and the panels Task 12 adds.

Behaviour:
- Fetches `getEntity(route.detail.entityId)` in an `$effect` keyed on the id, with the same out-of-order token guard as `Tile`.
- `NotFound` → a `.d-b` panel saying the entity is not in the local index, with the id in mono and a *Close* button. A deep link into a corpus that has not synced yet is a normal event, not an error dialog.
- Close = `router.go('#/ctx/' + ctx)`; the `x` button, the ladder's rung 2, and clicking the room behind all route through that one call.
- `animation: fade .12s` comes from `app.css` and is already disabled under reduced motion.
- Focus moves to the panel heading on open and back to the row that opened it on close (`.d-h` gets `tabindex="-1"` and `aria-labelledby` pointing at the title).

- [ ] **Step 4: Test the slide-over**

Mount `Detail` with a mocked `getEntity`; assert the title, that `body_text` appears as text, that a `not_found` error renders the panel and not a blank aside, that `Escape` reaches the router, and that focus returns to the opener.

- [ ] **Step 5: `just check`, QA, commit**

QA: `?fake-ipc#/ticket/mock:PAY-231` and `?fake-ipc#/incident/mock:INC-1` (add an undeclared-kind row to the fixture) — two screenshots, the second proving §3a's promise.

```bash
git add app/src/lib/detail app/src/lib/shell
git commit -m "detail: read-only slide-over with the generic payload view"
```

---

### Task 12: Detail panels — deleted banner, links, history

**Files:**
- Create: `app/src/lib/detail/LinksPanel.svelte`, `app/src/lib/detail/HistoryPanel.svelte`, `app/src/lib/detail/HistoryPanel.test.ts`, `app/src/lib/shell/time.ts`, `app/src/lib/shell/time.test.ts`
- Modify: `app/src/lib/detail/Detail.svelte`

**Interfaces:**
- Produces: `ago(iso: string, now?: Date): string` — the mockup's `ago()` (`signal-miller.html:778`) as a tested function: `"just now" | "4 min ago" | "3 h ago" | "2026-08-19"`. Used by every sync-age reading in the app (spec §4 per-row provenance).

- [ ] **Step 1: Write the failing `ago` test**

```ts
const now = new Date("2026-08-22T14:32:00Z");
test.each([
  ["2026-08-22T14:31:40Z", "just now"],
  ["2026-08-22T14:28:00Z", "4 min ago"],
  ["2026-08-22T11:32:00Z", "3 h ago"],
  ["2026-08-19T09:00:00Z", "2026-08-19"],
  ["2026-08-22T14:33:00Z", "just now"],   // clock skew must not print "-1 min ago"
])("%s -> %s", (iso, want) => expect(ago(iso, now)).toBe(want));
```

- [ ] **Step 2: `LinksPanel.svelte`**

Renders `detail.links` grouped by `relation` with the mockup's `.row.g4` shape. **In M1 the array is always empty** (interfaces §2.5), so the panel renders the mockup's empty state — *"Nothing linked yet."* — with the M2 note in a `title`. It ships now because §5a calls links the core object and the panel is where every later milestone hangs: writing it against the real `LinkRow` shape now costs ten lines and saves a redesign.

- [ ] **Step 3: `HistoryPanel.svelte`**

Spec §12.1 *"Change history per asset: every mutation appends a line"*, §2a as its store. Renders `detail.activity` (already fetched by `get_entity`) as `.row.g4`: `ago(at)`, actor (`user` vs `sync:<source>` rendered as the source's monogram), verb, and `detail` projected through `projectPayload` in a `<details>`. Empty state: *"No recorded activity for this item yet."*

Test: two rows, one `user` and one `sync:mock`, assert the actor rendering differs and that the verb is text.

- [ ] **Step 4: The deleted banner**

When `detail.deleted_at` is set, `.d-b` opens with a `.prompt` strip: *"Withdrawn upstream — last seen ‹ago(deleted_at)›. Kept because links and notes may point at it."* (spec §5a link history / interfaces §1 sweep). Test with the mock's `PAY-198`.

- [ ] **Step 5: `just check`, QA, commit**

```bash
git add app/src/lib/detail app/src/lib/shell
git commit -m "detail: deleted banner, links and history panels"
```

---

### Task 13: *Open in browser* — `web_url` and the first Tauri plugin

**Files:**
- Create: `app/src/lib/shell/open-external.ts`, `app/src/lib/shell/open-external.test.ts`
- Modify: `app/src/lib/detail/Detail.svelte`
- Modify (orchestrator edits requested): `crates/knobas-app/Cargo.toml` (`tauri-plugin-opener`), `crates/knobas-app/src/lib.rs` (`.plugin(tauri_plugin_opener::init())`), `crates/knobas-app/capabilities/default.json`, `app/package.json` (`@tauri-apps/plugin-opener`)

**Carry-over discharged:** *"`capabilities/default.json` grants only `core:default` — the first plugin (notifications, shortcuts) must extend it. CSP note: desktop dev builds get NO CSP; verify CSP changes against production builds only."*

**Interfaces:**
- Produces: `export async function openExternal(url: string): Promise<void>` — refuses anything that is not `http:` or `https:`.

- [ ] **Step 1: Write the failing test**

```ts
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn(async () => undefined) }));
import { openUrl } from "@tauri-apps/plugin-opener";
import { openExternal } from "./open-external";

test("opens http and https", async () => {
  await openExternal("https://jira.tidewater.internal/browse/PAY-231");
  expect(openUrl).toHaveBeenCalledWith("https://jira.tidewater.internal/browse/PAY-231");
});

test.each(["javascript:alert(1)", "file:///etc/passwd", "data:text/html,<script>", "vscode://x", ""])(
  "refuses %s", async (url) => {
    await expect(openExternal(url)).rejects.toThrow(/refused/i);
    expect(openUrl).not.toHaveBeenCalled();
  });
```

**Why this test exists:** `web_url` arrives from an adapter, i.e. from a remote system's data. Handing an arbitrary scheme to the OS opener is a "click a ticket, run a program" bug. The check is in the frontend because that is the only consumer, and the plugin's own scope is configured as a second wall.

- [ ] **Step 2: Add the plugin**

Add the crate and the npm package (pin both to the same 2.x minor as `@tauri-apps/api` 2.11.1 — a plugin whose JS and Rust halves disagree fails at runtime with an unhelpful "command not found"). Register it in `lib.rs`.

`capabilities/default.json`: add the opener permission to `"permissions"`. **Read the regenerated `crates/knobas-app/gen/schemas/desktop-schema.json` to get the exact identifier** (`tauri-build` rewrites it on the next build) rather than guessing — `opener:default` is the expected name, and the schema lists the scoped alternatives. Restrict the scope to `https://*` and `http://*` if the installed version supports a scope block; record what it actually offers in the PR body.

- [ ] **Step 3: Implement and wire**

```ts
import { openUrl } from "@tauri-apps/plugin-opener";

const ALLOWED = new Set(["http:", "https:"]);

export async function openExternal(url: string): Promise<void> {
  let parsed: URL;
  try { parsed = new URL(url); } catch { throw new Error(`refused: ${url} is not a URL`); }
  if (!ALLOWED.has(parsed.protocol)) throw new Error(`refused: ${parsed.protocol} is not http(s)`);
  await openUrl(url);
}
```

In `Detail.svelte`'s `.d-h` actions: `{#if detail.web_url}<button class="btn sm" onclick={...}>Open in browser</button>{/if}`, failures going to a toast. **The button is absent when `web_url` is null** — which, until open question D5 is settled, is always (Task 8). Ship it anyway: the plumbing is what this task is for, and the day `web_url` starts arriving the button appears with no further work.

- [ ] **Step 4: Verify the CSP is still satisfied in a *bundle***

Run `cd crates/knobas-app && PATH="$PWD/../../app/node_modules/.bin:$PATH" tauri build --debug`, launch the produced app, open a detail, and check the webview console is free of CSP violations. **This is the only way to test the CSP** (carry-over: dev builds have none). Record the result; if `tauri build` is not viable on the agent's machine, say so in the PR body rather than claiming a pass.

- [ ] **Step 5: `just check` and commit**

```bash
git add app/src crates/knobas-app
git commit -m "detail: open in browser via the opener plugin"
```

---

### Task 14: The status bar

**Files:**
- Modify: `app/src/lib/shell/StatusBar.svelte`
- Create: `app/src/lib/shell/latest-change.svelte.ts`, `app/src/lib/shell/latest-change.test.ts`

**Interfaces:**
- Consumes: `EVENTS.ACTIVITY_NEW`, `recentActivity` (Task 8), `lifecycle.status`.
- Produces: `latestChange` — a rune holding the newest `ActivityRow`, seeded from `recentActivity(1)` and updated from the `activity:new` event, **coalesced to at most one visible update per second** (interfaces §2.3: the event is already coalesced ≥ 1 s by the emitter; the UI must not thrash if that changes).

**Spec §2 status bar:** *"DB size, FTS freshness, sync cadence, counts, pending writes, user, clock"* plus §2 *"status bar carries a 'latest change' one-liner (the ticker's quiet replacement)"*.

M1 members and where each comes from:

| Reading | Source | Task |
|---|---|---|
| `postgres knobas · 212 MB` | `dbStats().db_bytes` (F) | 19 |
| `N entities · N items` | `dbStats()` (F) | 19 |
| `sync every 5 min · next in 4:12` | `syncStatus().next_run_at` (F), counted down client-side in a `<Flap>` | 19 |
| latest change | `activity:new` + `recentActivity(1)` | **this task** |
| `pending writes 0` | constant 0 in M1 (the write queue is M2) — rendered so the slot exists, with a `title` naming M2 | **this task** |
| user | **omitted in M1** — there is no identity model until M2; an invented username is worse than an empty slot | — |
| clock | local, ticking | 6 |

- [ ] **Step 1: Write the failing coalescing test**

```ts
test("shows the newest line and coalesces a burst into one update", async () => {
  vi.useFakeTimers();
  const store = createLatestChange({ windowMs: 1000 });
  store.push(row(1, "synced"));
  store.push(row(2, "synced"));
  store.push(row(3, "linked"));
  expect(store.current?.id).toBe(1);        // the first shows immediately
  vi.advanceTimersByTime(1000);
  expect(store.current?.id).toBe(3);        // then the newest of the burst, once
});
```

- [ ] **Step 2: Implement, wire the listener with its teardown**

The `listen` subscription lives in `StatusBar.svelte`'s `$effect` and is unlistened in the teardown, including the unmounted-before-resolve race (`let dead = false; ... if (dead) off();`).

- [ ] **Step 3: Render**

`.statusbar` per the mockup (`signal-miller.html:2304-2310`): fixed readings in `.mono`, the latest-change line as `.latest` with the verb in `<b>` and the rest as text, `.spacer`, `pending writes 0`, the clock. Everything the backend cannot answer yet renders as `—`, never as a plausible-looking zero.

- [ ] **Step 4: `just check`, QA, commit**

```bash
git add app/src/lib/shell
git commit -m "shell: status bar with the latest-change line"
```

---

## Phase 2 — sources, health, first run

> **Gate: Checkpoint 1 (interfaces §6.2).** Every task below imports from `app/src/lib/ipc/sources.ts` and `crates/knobas-app/src/commands/sources.rs`, which stream **F** owns and ships. Do not start Phase 2 until `listSources`, `listAdapters`, `addSource`, `testSource`, `setSourceSecret`, `deleteSource`, `credentialHealth`, `syncNow`, `syncStatus`, `listSyncRuns` and `dbStats` are merged on `main`. **No D-owned file may declare any of those types.** Task 15 is the exception — it is pure TypeScript over JSON Schema and can be written any time.

---

### Task 15: `config_schema` → a form model

**Files:**
- Create: `app/src/lib/sources/schema-form.ts`, `app/src/lib/sources/schema-form.test.ts`, `app/src/lib/sources/fixtures.ts`

**Spec §3a, verbatim:** *"An adapter's descriptor declares everything the app needs to host it: its config schema (the *Add source* form is **generated** from it, not hand-built per adapter)…"*

**Interfaces:**
- Produces:

```ts
export type Control =
  | { kind: "text"; default: string }
  | { kind: "select"; options: string[]; default: string }
  | { kind: "number"; integer: boolean; min: number | null; max: number | null; default: number | null }
  | { kind: "toggle"; default: boolean }
  | { kind: "list"; default: string[] }        // array of string, one value per line
  | { kind: "json"; default: unknown };        // anything the subset does not cover

export interface SchemaField { key: string; label: string; help: string | null;
                               required: boolean; control: Control }

export function schemaFields(schema: unknown): SchemaField[];
export function defaultValues(fields: SchemaField[]): Record<string, unknown>;
export function validate(fields: SchemaField[], values: Record<string, unknown>):
  { errors: Record<string, string>; config: Record<string, unknown> };
```

- [ ] **Step 1: Write the fixtures — the three real adapter schemas**

`app/src/lib/sources/fixtures.ts` transcribes interfaces §4.2's config column as JSON Schema. These are the shapes the form must handle on the day A/B/C land, so they are the test corpus:

```ts
export const JIRA_SCHEMA = {
  type: "object",
  properties: {
    flavor: { type: "string", enum: ["datacenter", "cloud"], default: "datacenter",
              title: "Deployment flavor",
              description: "Data Center speaks REST v2; Cloud is not supported yet." },
    projects: { type: "array", items: { type: "string" }, title: "Projects" },
    jql_filter: { type: "string", title: "JQL filter" },
    username: { type: "string", title: "Username", description: "Basic auth only." },
  },
  required: ["flavor"],
} as const;

export const GITEA_SCHEMA = { type: "object", properties: {
  owners: { type: "array", items: { type: "string" }, title: "Owners" },
  repos:  { type: "array", items: { type: "string" }, title: "Repositories" },
  username: { type: "string", title: "Username" } } } as const;

export const TEAMCITY_SCHEMA = { type: "object", properties: {
  project_ids:    { type: "array", items: { type: "string" }, title: "Project ids" },
  build_type_ids: { type: "array", items: { type: "string" }, title: "Build config ids" },
  builds_per_config: { type: "integer", minimum: 1, maximum: 500, default: 100,
                       title: "Builds per config" } } } as const;

/** What `knobas-source-mock` declares today. */
export const EMPTY_SCHEMA = { type: "object", properties: {} } as const;
```

- [ ] **Step 2: Write the failing tests**

```ts
test("Jira: an enum becomes a select with its default preselected", () => {
  const [flavor] = schemaFields(JIRA_SCHEMA);
  expect(flavor).toMatchObject({ key: "flavor", label: "Deployment flavor", required: true });
  expect(flavor?.control).toEqual({ kind: "select", options: ["datacenter", "cloud"], default: "datacenter" });
});

test("an array of strings becomes a list control", () => {
  const projects = schemaFields(JIRA_SCHEMA).find((f) => f.key === "projects");
  expect(projects?.control).toEqual({ kind: "list", default: [] });
});

test("integer bounds survive", () => {
  const n = schemaFields(TEAMCITY_SCHEMA).find((f) => f.key === "builds_per_config");
  expect(n?.control).toEqual({ kind: "number", integer: true, min: 1, max: 500, default: 100 });
});

test("declaration order is preserved — it is the adapter author's intent", () => {
  expect(schemaFields(GITEA_SCHEMA).map((f) => f.key)).toEqual(["owners", "repos", "username"]);
});

test("an adapter with no configuration yields no fields", () => {
  expect(schemaFields(EMPTY_SCHEMA)).toEqual([]);
});

test("a shape the subset does not cover degrades to a JSON control, never to a crash", () => {
  const f = schemaFields({ type: "object", properties: {
    matrix: { type: "array", items: { type: "object" } } } })[0];
  expect(f?.control.kind).toBe("json");
});

test("garbage in, empty out", () => {
  expect(schemaFields(null)).toEqual([]);
  expect(schemaFields({ type: "string" })).toEqual([]);
  expect(schemaFields({ type: "object" })).toEqual([]);
});

test("validate reports missing required fields by key and builds the config object", () => {
  const fields = schemaFields(JIRA_SCHEMA);
  const bad = validate(fields, { flavor: "", projects: ["PAY"], jql_filter: "", username: "" });
  expect(bad.errors.flavor).toMatch(/required/i);

  const good = validate(fields, { flavor: "datacenter", projects: ["PAY", "OPS"],
                                  jql_filter: "", username: "mara" });
  expect(good.errors).toEqual({});
  // Empty optional values are omitted, not sent as "" -- the adapter's own
  // defaults must be able to apply.
  expect(good.config).toEqual({ flavor: "datacenter", projects: ["PAY", "OPS"], username: "mara" });
});

test("validate rejects a value outside an enum and a number outside its bounds", () => {
  expect(validate(schemaFields(JIRA_SCHEMA), { flavor: "onprem" }).errors.flavor).toMatch(/one of/i);
  expect(validate(schemaFields(TEAMCITY_SCHEMA), { builds_per_config: 5000 })
    .errors.builds_per_config).toMatch(/500/);
});
```

- [ ] **Step 3: Implement**

`schemaFields` reads `schema.properties` in insertion order (`JSON.parse` and object literals both preserve it for string keys), maps `type` + `enum` + `items.type` onto `Control`, takes `title` as the label and falls back to humanising the key (`jql_filter` → "Jql filter" — deliberately dumb; the fix is for the adapter to declare a `title`), `description` as help, and membership of `schema.required` as `required`.

**Secrets never appear here.** If a property is named `password`/`token`/`secret`, `schemaFields` throws — an adapter that puts a secret in its config schema is a bug worth failing loudly on (interfaces §3: "Nothing secret ever reaches Postgres"). Add that test.

- [ ] **Step 4: `just check` and commit**

```bash
git add app/src/lib/sources
git commit -m "sources: generate a form model from config_schema"
```

---

### Task 16: The sources view

**Files:**
- Create: `app/src/lib/sources/SourcesView.svelte`, `app/src/lib/sources/SourceRow.svelte`, `app/src/lib/sources/SourcesView.test.ts`
- Modify: `app/src/App.svelte` (route `#/sources`), `app/src/lib/shell/dev/fake-tauri.ts`

**Interfaces:**
- Consumes: `listSources`, `syncNow`, `syncStatus`, `deleteSource` (**stream F**), `EVENTS.SYNC_STATE`.

- [ ] **Step 1: Write the failing view test**

```ts
vi.mock("../ipc/sources", () => ({ listSources: vi.fn(), syncNow: vi.fn(), deleteSource: vi.fn() }));

test("renders one row per source with its health, item count and last run", async () => { … });
test("Sync now calls syncNow with that source's id and shows the run as started", async () => { … });
test("a source whose health is unauthorized offers Re-enter, not Sync now", async () => { … });
test("Delete asks first, and the confirm dialog names the source and the purge choice", async () => { … });
test("listSources failing renders an error panel, not an empty list", async () => { … });
```

- [ ] **Step 2: Implement**

The mockup's `renderSources` (`signal-miller.html:2800-2838`) is the layout reference: a `.room-bar` with *Sync now* / *Add source*, a `.dbbar` summary line, a `.src` header row, then one `.src` per source — monogram, name + `adapter_kind`, base URL + capability chips (from `SourceSummary.kinds`), auth kind, sync state, actions. `.src.err` shading for an unhealthy source.

Live updates: subscribe to `sync:state` and patch the matching row rather than re-listing (interfaces §2.3: the event carries `SourceSyncStatus`). Re-list only after a mutation (add/delete/secret), because those change the row set.

**Never render a secret, an auth header, or a keychain item.** The auth column shows the `AuthMethod` word and, when `secret_expires_at` is set, the countdown (Task 18).

- [ ] **Step 3: Test, `just check`, QA, commit**

QA: `?fake-ipc#/sources` with a fixture of three sources, one of them `unauthorized`.

```bash
git add app/src/lib/sources app/src/App.svelte
git commit -m "sources: the sources view over list_sources"
```

---

### Task 17: The Add-source dialog

**Files:**
- Create: `app/src/lib/sources/AddSource.svelte`, `app/src/lib/sources/SchemaForm.svelte`, `app/src/lib/sources/AddSource.test.ts`, `app/src/lib/sources/SchemaForm.test.ts`
- Modify: `app/src/lib/sources/SourcesView.svelte`

**Interfaces:**
- Consumes: `listAdapters`, `testSource`, `addSource` (**stream F**), `schemaFields`/`validate` (Task 15), `Modal` (Task 3).

**Spec §3 flow:** *"type → URL → auth → Test connection → sync schedule → save"* — the mockup's five `.steps` (`signal-miller.html:3595-3621`), now with a **generated** step 2 instead of the mockup's hand-written one.

- [ ] **Step 1: Write the failing SchemaForm test**

```ts
test("renders a select, a list and a number from the Jira schema", () => { … });
test("a list control splits on newlines and drops blank lines", () => { … });
test("an invalid value shows its message next to its field and blocks Next", () => { … });
test("an adapter with no configuration shows a sentence, not an empty box", () => {
  // "The mock adapter needs no configuration."
});
```

- [ ] **Step 2: Implement `SchemaForm.svelte`**

One `.form` grid (`140px 1fr`), one row per `SchemaField`, control chosen by `control.kind`, `aria-describedby` wiring help text and errors to the input, `required` reflected as an attribute so the browser and the assistive layer agree with the validator.

- [ ] **Step 3: Write the failing AddSource test**

```ts
test("step 1 lists adapters from listAdapters, not from a hardcoded table", async () => { … });
test("the instance id defaults to the adapter kind and is validated", async () => {
  // [a-z][a-z0-9-]{0,31} (interfaces §4.1), and the field is disabled for an
  // existing source because the id is immutable (P10).
});
test("auth options come from the descriptor's auth_methods", async () => { … });
test("Test connection shows account, server version and elapsed time on success", async () => { … });
test("a failed test shows the SourceError and leaves Next disabled", async () => { … });
test("Save sends config from the generated form and the secret exactly once", async () => {
  expect(addSource).toHaveBeenCalledWith(expect.objectContaining({
    id: "jira", adapter_kind: "jira",
    config: { flavor: "datacenter", projects: ["PAY"] },
    secret: { value: "s3cret" } }));
});
test("the secret is never written anywhere but the request", async () => {
  // after Save: no secret in localStorage, sessionStorage, or the DOM
});
```

- [ ] **Step 4: Implement**

Five steps in a `Modal`, `.steps` breadcrumb, state held in one `$state` object. Step 4 calls `testSource(draft)` and renders `ConnectionReport` in `.test-res` — *"Connected as ‹account› · ‹server_version› · ‹elapsed_ms› ms"*, with the fields it does not get simply absent (interfaces §2.2: those three depend on P4 and may be `None`). Step 5 posts `NewSource`.

**Order of operations is the backend's** (interfaces §3: secret first, then the config row, roll back on failure). The dialog sends one `addSource` call and does not attempt its own two-phase dance.

- [ ] **Step 5: `just check`, QA, commit**

QA: screenshot each of the five steps.

```bash
git add app/src/lib/sources
git commit -m "sources: add-source dialog with a generated config form"
```

---

### Task 18: Credential health — monograms, re-enter, expiry

**Files:**
- Create: `app/src/lib/sources/ReenterSecret.svelte`, `app/src/lib/shell/health.svelte.ts`, `app/src/lib/shell/health.test.ts`
- Modify: `app/src/lib/shell/TopStrip.svelte`, `app/src/lib/sources/SourceRow.svelte`, `app/src/lib/shell/contexts.ts` (per-source contexts become real)

**Interfaces:**
- Consumes: `credentialHealth`, `setSourceSecret` (**stream F**), `EVENTS.SOURCE_HEALTH`.
- Produces: `health` — a rune keyed by `source_id`, seeded from `credentialHealth()` and patched by `source:health`.

**Spec §3:** *"Credential health: PAT expiry countdown, 401 detection → Re-enter password, reminder in inbox (snoozable)"* — the inbox half is M2; the countdown and *Re-enter* are M1.

- [ ] **Step 1: Write the failing tests**

```ts
test("an unauthorized source paints its monogram as failed and shows the code", () => { … });
test("a PAT expiring in 12 days reads amber; in 40 days it reads plain", () => {
  expect(expiryNote("2026-09-03T00:00:00Z", now)).toEqual({ text: "PAT expires in 12 days", tone: "amber" });
  expect(expiryNote("2026-10-02T00:00:00Z", now)).toEqual({ text: "PAT expires in 41 days", tone: "plain" });
  expect(expiryNote(null, now)).toBeNull();
});
test("an already-expired secret reads failed, not '-3 days'", () => { … });
test("a source:health event patches one source and leaves the others alone", () => { … });
test("Re-enter posts the secret once and clears the field on success", async () => { … });
```

- [ ] **Step 2: Implement**

Top strip `.sync`: one `<Monogram>` per source with the `::after` dot painted by `auth_state`, the mockup's `.err-txt` reading `401` when any source is unauthorized, and a click that navigates to `#/sources`. The whole cluster's `title` lists each source and its state — the mockup's tooltip (`signal-miller.html:2299`), which is the only place the full list fits in 44 px.

`ReenterSecret.svelte` is the mockup's `.src-fix` inline strip: a `type="password"` input, *Save and retry sync*, *Cancel*, and the line **"Stored in the OS keychain, never in the database."** (spec §14, and it is true — interfaces §3). On success it calls `syncNow` for that source, because the user's intent in re-entering a password is to make the sync work again.

Then extend `builtinContexts` (Task 9) to take the real source list from `health`/`listSources`, so the tab strip finally shows per-source rooms.

- [ ] **Step 3: `just check`, QA, commit**

```bash
git add app/src/lib
git commit -m "sources: credential health in the top strip and the sources view"
```

---

### Task 19: Diagnostics and the live status bar

**Files:**
- Create: `app/src/lib/sources/Diagnostics.svelte`, `app/src/lib/sources/SyncRunList.svelte`, `app/src/lib/sources/Diagnostics.test.ts`
- Modify: `app/src/lib/shell/StatusBar.svelte`, `app/src/lib/sources/SourcesView.svelte`

**Interfaces:**
- Consumes: `listSyncRuns`, `dbStats`, `reindexFts`, `syncStatus` (**stream F**).

**Spec §3 (Diagnostics, Rec 08-24):** *"per-source sync log with errors, last-run durations, item counts, FTS index state, re-index button, DB size. The status bar shows the summary; this is where you look when a sync misbehaves."*

- [ ] **Step 1: Write the failing tests**

```ts
test("lists runs newest first with duration, counts and the error when there is one", () => { … });
test("a run still in flight shows as running, not as a 0 ms success", () => {
  // finished_at === null
});
test("formatBytes renders db_bytes the way the status bar reads it", () => {
  expect(formatBytes(222_298_112)).toBe("212 MB");
  expect(formatBytes(0)).toBe("0 B");
});
test("the countdown to the next sync ticks down and never goes negative", () => { … });
```

- [ ] **Step 2: Implement**

`SyncRunList` renders `SyncRunRow`s as `.row.g5w`: trigger, `ago(started_at)`, duration (`finished_at − started_at`, `—` while running), `upserted/deleted/swept`, outcome as a `<Flap>` (a value that changes while you watch), and the error as text in `.log` when present.

`Diagnostics` adds the `.dbbar` with `db_bytes`, entity/item counts, oldest/newest `synced_at`, and a *Re-index full text* button calling `reindexFts()` behind a confirm (it is M1-optional on the backend; if the command is absent, the button is not rendered — check the export, do not call blindly).

`StatusBar` picks up `dbStats` (refreshed on `sync:state` and every 60 s) and the `next_run_at` countdown in a `<Flap value={mm:ss}>`.

- [ ] **Step 3: `just check`, QA, commit**

```bash
git add app/src/lib
git commit -m "sources: diagnostics view and live status-bar readings"
```

---

### Task 20: The first-run wizard

**Files:**
- Create: `app/src/lib/sources/FirstRun.svelte`, `app/src/lib/sources/FirstRun.test.ts`
- Modify: `app/src/App.svelte`, `crates/knobas-app/src/commands/app.rs` (`complete_first_run`), `app/src/lib/ipc/app.ts`
- Modify (orchestrator edits requested): `lib.rs` handler list

**Spec §14a:** *"First-run wizard: initialize the database → add the first source (the §3 flow) → initial sync with progress → land in the launcher."*

**Interfaces:**
- Consumes: `AddSource` (Task 17), `syncNow` + `Channel<SyncProgress>` (**stream F**, P3), `demoLoad` (**stream F**), `appStatus.first_run`.
- Produces: `#[tauri::command] pub async fn complete_first_run(state: State<'_, AppState>) -> Result<(), IpcError>` — writes `knobas.setting['first_run.completed']` (see open question **D3**).

- [ ] **Step 1: Write the failing tests**

```ts
test("appears when app_status says first_run and never again after completion", async () => { … });

test("the demo profile offers Load the Tidewater dataset as the first option", async () => {
  // status.demo === true -> a "Load demo data" path calling demoLoad()
});

test("progress from the Channel is rendered per phase and item count", async () => {
  const channel = captureChannelPassedToSyncNow();
  channel.onmessage({ run_id: 1, source_id: "jira", phase: "fetching", items: 40,
                      elapsed_ms: 900, message: null });
  expect(screen()).toContain("40 items");
  channel.onmessage({ run_id: 1, source_id: "jira", phase: "finished", items: 213,
                      elapsed_ms: 4200, message: null });
  expect(finishEnabled()).toBe(true);
});

test("a failed phase offers Retry and Skip, and never silently lands in an empty room", async () => { … });

test("Finish calls complete_first_run and routes to the room", async () => { … });
```

- [ ] **Step 2: Implement the command**

```rust
#[tauri::command]
pub async fn complete_first_run(state: State<'_, AppState>) -> Result<(), IpcError> {
    sqlx::query("insert into knobas.setting (key, value) values ('first_run.completed', 'true'::jsonb)
                 on conflict (key) do update set value = excluded.value, updated_at = now()")
        .execute(&state.pool).await.map_err(IpcError::from_db)?;
    Ok(())
}
```

- [ ] **Step 3: Implement the wizard**

Four panels over the mockup's `.steps` breadcrumb: **Database** (already green — the shell only gets here once `lifecycle.ready`), **Source** (embeds `AddSource`'s steps inline rather than in a modal; a modal over an otherwise empty window is a dialog with nothing behind it), **First sync**, **Done**.

The sync panel attaches a channel:

```ts
import { Channel } from "@tauri-apps/api/core";
import { syncNow, type SyncProgress } from "../ipc/sources";

const channel = new Channel<SyncProgress>();
channel.onmessage = (p) => { progress = p; };
const runId = await syncNow(sourceId, channel);
```

`syncNow` returns the run id immediately (P3) — the panel therefore reports completion from the **channel's** `finished`/`failed` phase, not from the command resolving. Progress renders as a native `<progress>` when a total is knowable and as an item counter otherwise (`items` grows, there is no total until the run ends) — and never as an inline-styled bar (CSP).

> **Verify at implementation time (P3, explicitly demanded by the ruling):** that `Option<Channel<_>>` decodes when the argument is *omitted*. Call `syncNow("mock")` with no channel from a test or the dev console; if Tauri rejects it, report it to the orchestrator — the contract's fallback is splitting into `sync_now` / `sync_now_with_progress`, which is F's change, not D's.

- [ ] **Step 4: `just check`, QA, commit**

QA under `just dev` with a fresh profile: `just dev` after `mv "$HOME/Library/Application Support/dev.knobas.desktop.dev" /tmp/knobas-profile-bak` — the wizard must appear, take a source, sync it, and land in a room. Restore the profile afterwards. Record the observed sequence.

```bash
git add app/src crates/knobas-app
git commit -m "sources: first-run wizard with channel progress"
```

---

### Task 21: Kind metadata everywhere

**Files:**
- Modify: `crates/knobas-app/src/commands/entity.rs` (`kind_info`), `app/src/lib/shell/kinds.ts`, `app/src/lib/shell/Room.svelte`, `app/src/lib/detail/Detail.svelte`
- Create: `app/src/lib/shell/kind-registry.svelte.ts`

**Interfaces:**
- Consumes: `listAdapters()` → `SourceDescriptor[]` (**stream F**), each carrying `entity_kinds: KindInfo[]`.
- Produces: `kindRegistry.info(kind: string): KindInfo | null` — loaded once at shell start, since `list_adapters` is static per build (interfaces §1: "the descriptor is static per adapter kind").

- [ ] **Step 1: Write the failing tests**

```ts
test("a declared kind uses the adapter's label, plural and monogram", () => { … });
test("an undeclared kind still gets a usable label and monogram", () => { … });
test("two adapters declaring the same kind id do not fight (first wins, deterministically)", () => { … });
```

Rust side: `get_entity` fills `kind_info` from the registry for the row's `source_id`, and leaves it `None` for a source with no configured adapter.

- [ ] **Step 2: Implement and wire**

Tile labels, tile monograms, detail headers and the room's kind chips all read `kindRegistry`. This is the task that makes §3a's "a new source's items get grouped, chipped, and labeled in the launcher without touching core" literally true for the shell.

- [ ] **Step 3: `just check`, QA, commit**

```bash
git add app/src crates/knobas-app
git commit -m "shell: kind labels and monograms from the adapter descriptors"
```

---

## Phase 3 — closing the stream

---

### Task 22: The launcher seam and address completeness

**Files:**
- Modify: `app/src/App.svelte`, `app/src/lib/shell/keys.ts`, `app/src/lib/shell/TopStrip.svelte`, `app/src/lib/shell/router.svelte.ts`
- Create: `app/src/lib/shell/router-deeplink.test.ts`

**Gate:** stream E's `app/src/lib/launcher/**` merged.

- [ ] **Step 1: Swap the stub for the real launcher**

`App.svelte` imports `Launcher` from `$lib/launcher/…` and renders it when `launcherOpen`; `keys.ts`'s `⌘K` toast disappears; the `.searchfield` opens the same overlay. The **only** contract between D and E is: D owns opening/closing and the `Esc` rung ordering (the launcher is rung 1, above the detail); E owns everything inside the overlay. Record that in a comment in `keys.ts`.

- [ ] **Step 2: Deep-link tests**

```ts
test.each([
  "#/ctx/all", "#/ctx/src:jira", "#/sources", "#/first-run",
  "#/ticket/mock:PAY-231", "#/pr/mock:payout-service%23142", "#/entity/mock:c90d11",
  "#/inbox", "#/assets/board", "#/nonsense",
])("%s renders something and never throws", (hash) => { … });
```

Every M2–M4 address renders the "arrives in M‹n›" panel with a link back to the room. A deep link from a future notification must not produce a blank window.

- [ ] **Step 3: `just check`, QA, commit**

```bash
git add app/src
git commit -m "shell: wire the launcher and cover every address"
```

---

### Task 23: Stream-D exit sweep

**Files:**
- Modify: whatever the sweep finds; `README.md` gets the frontend section (orchestrator edit requested)

**Interfaces:** consumes everything; produces the stream-D exit record for interfaces §6.3.

- [ ] **Step 1: Keyboard and accessibility pass (spec §2, §14)**

Walk the whole shell with the mouse unplugged: Tab reaches every control in visual order, `:focus-visible` is visible on all of them (`app.css` sets `outline: 1px solid var(--link)`), the modal traps and restores focus, `Esc` unwinds one rung at a time, the detail is `role="complementary"` with an accessible name, tiles are `<section>`s with headings, and every icon-only button has an `aria-label`. Fix what fails; record the walk in the PR body.

- [ ] **Step 2: Reduced motion and contrast**

Run with `prefers-reduced-motion: reduce` forced (`--force-prefers-reduced-motion` in Chrome, or the macOS setting) — no flap animation, no slide-over fade. Check the readings against 4.5:1 (spec §14): `--muted` (#8B929A) on `--bg` (#15171A) passes; `--faint` (#5A6169) on `--bg` does **not** (≈3.2:1) and is used for secondary readings in the mockup. Either lift `--faint` to a passing value or restrict it to non-text decoration; whichever is chosen, write the decision into `app.css` as a comment. **Do not silently ship a token that fails the spec's own bar.**

- [ ] **Step 3: Production-bundle verification**

`tauri build --debug`, launch, and exercise: loading state, a room, a detail, the sources view. Confirm the webview console reports **zero** CSP violations and that no font or asset request leaves the machine. This is the only build where the CSP is live (carry-over).

- [ ] **Step 4: Run the stream-D exit criteria (interfaces §6.3) and record the output**

```
□ the window appears BEFORE the database is ready and shows a real loading state driven by db:state/app_status
□ top strip and status bar read live sync_status / db_stats / credential_health (401 highlighted)
□ a room renders its tiles from list_entities
□ the detail slide-over renders a known kind and, for an undeclared kind, the §3a generic view from payload
□ the sources view does add (form generated from config_schema) → test → save → re-enter → delete
□ the first-run wizard reaches a synced launcher
```

Each line gets a screenshot or a pasted command output. No claim without evidence.

- [ ] **Step 5: README and commit**

Add a "Frontend" section to `README.md`: the component map, the QA convention from Task 1, the house rules (no inline styles, no `{@html}`, no network), and the sentence that the mockup is a behaviour reference whose rendering strategy is not carried over.

```bash
git add README.md app/src
git commit -m "stream d: exit sweep, accessibility pass and readme"
```

---

## Open questions for the orchestrator

Each blocks nothing — every one has a fallback the plan already takes — but each is a decision that must not be made inside a stream.

**D1 — Who parses `--demo`, and who applies it?** P13 grants `--demo` = separate profile (own data dir, own embedded-server port, own keychain suffix). D needs only the boolean (`AppStatus.demo`) and provides `Profile::from_args` in `commands/app.rs` (Task 4). *Applying* it touches `crates/knobas-db/src/embedded.rs` (F, data dir + port) and `crates/knobas-secrets` (F, service suffix), wired in `lib.rs` (orchestrator). Confirm F owns the application, D owns the report. *Fallback taken:* D ships `Profile` and reports it; if nobody applies it, `demo` is merely honest about a flag that changes nothing yet.

**D2 — May `app_status` be `async fn`?** Interfaces §2.1 writes `pub fn`. The ruling's reason was "must not take `State<'_, AppState>`", which `async fn app_status(app: AppHandle)` honours via `try_state`. `async` is what makes `source_count`/`first_run` live rather than cached. The TS mirror is unchanged. *Fallback taken:* the cache in `Lifecycle`, refreshed from `frontend_ready`, documented as boot-time truth.

**D3 — `complete_first_run` as a new command in `commands/app.rs`.** §14a's wizard needs to record completion, `0002` provides `knobas.setting` explicitly for "first-run completion", and §2 declares no command that writes it. D proposes one command in a D-owned file plus one line in the orchestrator's handler list. *Fallback:* `first_run = source_count == 0` and a webview-local `localStorage` flag for "skip" — works, but the flag then lives outside the database that the export is supposed to carry (§14).

**D4 — `EntityDetail.web_url` has no storage.** P5 GRANTED adds `SyncItem.web_url: Option<String>`, but migration `0002` adds no `web_url` column to `sync.item` and `sync.live_item` selects none, so the sink has nowhere to put it and `get_entity` has nothing to read. Either `0002` gains `alter table sync.item add column web_url text` **and** the view gains the column (single-writer: orchestrator, before F's sink lands), or P5's storage is deferred and *Open in browser* stays absent for M1. *Fallback taken:* `web_url: None`, button absent, plumbing shipped (Task 13).

**D5 — May stream D extend `knobas-core::activity`?** §6.1 assigns `crates/knobas-core/**` to nobody. §2.5's `recent_activity(limit, entity_id)` needs `activity::recent` to take the filter. One function, one caller (D's own command). Confirm D may make that edit. *Fallback:* the filter is applied in `commands/entity.rs` with its own statement, duplicating four lines of SQL.

**D6 — Contexts in M1.** §7 contexts need links (M2). D therefore ships **derived, read-only** contexts: `all` plus one per source, with `+ new` disabled. Confirm that is the intended reading of "context tabs (read-only M1)", or say which built-ins you want instead.

**D7 — Bits UI.** Roadmap §4 pins "Bits UI for primitives". This plan hand-rolls `Modal` (Task 3) behind a one-file boundary instead, because M1 needs exactly one dialog and one popover and a new dependency's peer range against Svelte 5.56 cannot be verified from a plan document. Confirm, or say "adopt Bits UI now" and Task 3 swaps its internals with no change to its props. TanStack Virtual is deliberately unused in M1 — tiles are capped at 50 rows.

**D8 — The four M1-absent top-strip members.** Spec §2 lists timer, inbox count, Assets + alert count, and Today/Day buttons in the persistent top strip; all four are M2/M3/M4 features and this plan omits them rather than shipping disabled chrome. Confirm.

---

## Self-review

### Spec requirement → task

| Spec / contract requirement | Task |
|---|---|
| §2 Persistent top strip (search field `⌘K`, sync monograms with 401 highlighted, sources gear) | 6, 18, 22 |
| §2 Context switcher: tabs for contexts, dropdown with all, *+ new* | 9 (read-only), 18 (per-source rooms); *+ new* disabled → **D6** |
| §2 Room per context with tiles | 10 |
| §2 Status bar (DB size, FTS freshness, cadence, counts, pending writes, clock) | 6, 14, 19 |
| §2 Detail slide-over for any entity; `Esc` unwinds | 11, 12, 6 |
| §2 Entity addressing as the navigation contract | 6, 22 |
| §2 Toasts with optional action | 3 |
| §2 Split-flap cells only for values that change while you watch | 3, 14, 19 |
| §2 Keyboard: `⌘K`, `Esc` ladder, visible focus, reduced motion | 6, 22, 23 |
| §2 Window: 1440×900 target, usable ≥ 1100 wide | 2 (`.app { min-width: 1100px }`), 23 |
| §2a Activity stream surfaced (status-bar latest change, per-entity history) | 14, 12 |
| §3 Add source flow: type → URL → auth → Test connection → schedule → save | 17 |
| §3 Sync schedule per source; *Sync now* | 16 |
| §3 Credential health: PAT expiry countdown, 401 → *Re-enter password* | 18 |
| §3 Diagnostics: per-source sync log, durations, item counts, re-index, DB size | 19 |
| §3a Add-source form **generated** from `config_schema` | 15, 17 |
| §3a Open kinds; unknown kind gets the generic detail view from the raw payload | 10 (`tilesFor`), 11 (`projectPayload`), 21 |
| §3a Entity kinds with display metadata drive grouping/chips/labels | 21 |
| §5 Read halves of the work-item detail views | 11, 12 (write-backs are M2 — see gaps) |
| §5a Links panel on every item | 12 (empty in M1 by contract) |
| §12.1 Change history per entity | 12 |
| §14 Accessibility: keyboard-complete, visible focus, reduced motion, contrast ≥ 4.5:1 | 23 |
| §14 Secrets: OS keychain, never in the DB, masked in the UI | 17, 18 (and the "no secret in config_schema" guard in 15) |
| §14a First-run wizard → add source → initial sync with progress → land | 20 |
| §14a Demo seed mode as the UI stream's data | 1, 20 |
| Roadmap §4 "mockup CSS kept global" | 2 |
| Roadmap §4 "port region-by-region via components; rendering strategy not carried over" | every component task |
| Roadmap §3 headless-Chrome QA, per-agent port/user-data-dir | 1, and every task's QA step |
| Carry-over: async DB bring-up with a real loading state | 5 |
| Carry-over: `capabilities/default.json` must be extended by the first plugin | 13 |
| Carry-over: dev builds have no CSP — verify against production builds | 13, 23 |
| Interfaces §2.1 `app_status`, `frontend_ready`, `DbState` | 4 |
| Interfaces §2.5 `get_entity`, `list_entities`, `recent_activity` | 7, 8 |
| Interfaces §2.6 hand-written TS mirrors shipping with their command | 4, 7, 8, 20 |
| Interfaces §6.3 stream-D exit criteria | 23 |

### Gaps I could not close (flagged, not hidden)

1. **`web_url` has no column.** P5 grants the SPI field; `0002` gives it nowhere to live. *Open question D4.* Consequence: *Open in browser* is built and inert in M1.
2. **Structured snippet highlighting** (carry-over to stream D: "sentinel selectors → segments") is **stream E's** by ownership — `Segment[]` appears only in `SearchResponse` (interfaces §2.4) and the launcher is `app/src/lib/launcher/**`. Stream D renders no snippets. If the orchestrator meant D to own a shared `<Snippet>` component, say so and it lands in Task 3.
3. **Per-kind tailored detail views (§5)** are not built. Their read halves depend on normalized fields the SPI does not carry (each adapter's `payload` has its own shape) and their write halves are M2. M1 ships the §3a generic view for every kind, differing in header metadata. Recorded in Task 11 rather than pretended away.
4. **The room's tickets mini board** (four status columns) is not built — no normalized status field exists in M1. Recorded in Task 10.
5. **The adaptive room action bar and the suggestion tray** (§2) are not built: both are M2 (write-backs, §5a suggestions). The markup slots and their comments are in place.
6. **Contexts are derived, not real** (`all` + per-source). §7 membership needs links. *Open question D6.*
7. **"User" in the status bar** is omitted — no identity model before M2. Recorded in Task 14.
8. **`--demo` profile isolation** is only *reported* by stream D, not applied. *Open question D1.*
9. **Contrast:** `--faint` (#5A6169 on #15171A) is below the spec's own 4.5:1 bar and is used for secondary readings throughout the mockup. Task 23 forces the decision rather than letting it ship unnoticed; it may end as a token change that visibly differs from the mockup.

### Type-consistency check

`DbState` / `AppStatus` (Task 4) are the shapes Tasks 5, 6, 14, 20 consume. `EntityRow` / `EntityFilter` / `EntityPage` (Task 7) are what Tasks 1 (fixture), 10 and 22 use, with `EntityOrder` spelled `"updated_desc" | "title_asc"` in TS to match `#[serde(rename_all = "snake_case")]`. `EntityDetail` (Task 8) is what Tasks 11, 12, 13, 21 read, and `kind_info`/`web_url` are documented as `None` at introduction and filled in Tasks 21 and (pending D4) never. `SchemaField` / `Control` (Task 15) are what Task 17's `SchemaForm` renders. `RoomContext` (Task 9) is what Tasks 10 and 18 consume. `ago()` (Task 12) is used by Tasks 10, 16, 19. `openExternal` (Task 13) has exactly one caller. `toasts.push` (Task 3) is used by Tasks 6, 13, 16, 17, 18. No name appears with two spellings.
