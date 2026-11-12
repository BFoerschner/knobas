/**
 * Dev-only: answers `invoke` out of a fixture so the whole frontend runs in a
 * plain browser.
 *
 * It is **never reached in a bundle**. The only import site is guarded by
 * `import.meta.env.DEV`, which Rollup constant-folds to `false` in a
 * production build and drops together with the branch and this module;
 * `house-rules.test.ts` fails if a second, unguarded importer appears.
 *
 * ## The QA convention (roadmap §3)
 *
 * Every task's QA step drives this. `PORT = 5300 + <your agent number>` and a
 * per-agent `--user-data-dir`, because parallel agents sharing either have
 * collided before:
 *
 * ```sh
 * cd app && npx vite --port $PORT --strictPort &
 * "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
 *   --headless --disable-gpu --window-size=1440,900 \
 *   --user-data-dir=/tmp/knobas-qa-$USER-$PORT \
 *   --screenshot=/tmp/knobas-qa-$PORT.png \
 *   "http://localhost:$PORT/?fake-ipc#/ctx/all"
 * # attach the PNG to the PR; kill the dev server afterwards.
 * ```
 *
 * **The dev server, not `vite preview`** — and this is a correction, because
 * the convention as originally written could not work. `vite preview` serves
 * the *production* build, where `import.meta.env.DEV` is `false`, this module
 * is dropped from the bundle and `?fake-ipc` therefore does nothing at all.
 * The symptom is a window stuck on "Starting the local database" for ever,
 * which reads like a bug in the boot screen rather than a QA command pointed
 * at the wrong server. (`grep -c fake-ipc app/dist/assets/*.js` is `0`, which
 * is exactly the property the guard above promises.)
 *
 * A `--screenshot` is taken at the load event, and the shell is not finished
 * booting then: it awaits a dynamic import and then polls `app_status`. For a
 * screen that has to be *settled* — anything reading `db_stats` or
 * `list_sync_runs` — drive Chrome over the DevTools Protocol and wait for the
 * `.boot` element to go, rather than trusting `--virtual-time-budget`.
 *
 * `?fake-db=starting|migrating|failed` holds the boot screen on one state so
 * it can be photographed; without it the fake answers `ready` immediately.
 *
 * **This checks layout and interaction, not the bridge.** The real end-to-end
 * check is `just dev` (Tauri + embedded PostgreSQL) or `just demo`.
 *
 * ## One module per feature (#908)
 *
 * The answers live in `fake/<feature>.ts`, one module per feature, and this
 * file only puts them together. Each module exports `handlers(params)`, the
 * commands it answers, and may export `seed(params)`, which reads the query
 * before the bridge is installed. The modules are found by a glob, so a new
 * feature adds a file and a new command edits only its feature's file; two
 * modules answering the same command is an error, not a silent override.
 */
import type { Answers } from "./fake-answers";
import type { EntityDetail, EntityFilter, EntityPage, Reached } from "../../ipc/entity";
import { expandScale, seedScale } from "./fake-scale";
import { CORPUS } from "./fake/corpus";
import { addFakeRuns } from "./fake/builds";
import { addInboxItem } from "./fake/inbox";
import { downFakeMonitor } from "./fake/monitors";
import { endRunsChanged } from "./fake/sources";

/** One fake command. Arguments arrive camelCased, exactly as Tauri sends them. */
export type Handler = (args: Record<string, unknown>) => unknown;

/**
 * A feature's handler map. Any command the barrels wrap (`fake-answers.ts`) must answer with the
 * shape its barrel function resolves to; a command only the fake knows stays free-form (#1670).
 */
export type Handlers = { [K in keyof Answers]?: (args: Record<string, unknown>) => Answers[K] } & Record<string, Handler>;

/**
 * A registered event listener, as `@tauri-apps/api/event` sets one up.
 *
 * `listen()` is not a special entry point on the internals object: it is an
 * ordinary `invoke("plugin:event|listen", ..)` whose handler has been put
 * through `transformCallback`. So a fake bridge that only implements `invoke`
 * makes every `listen()` in the app hang for ever.
 */
interface Listener {
  event: string;
  callback: (payload: unknown) => void;
}

/** What a fake bridge hands back to whoever installed it. */
export interface FakeBridge {
  /** Deliver `payload` to everything listening to `event`. */
  emit(event: string, payload: unknown): void;
}

let nextCallbackId = 0;
let nextEventId = 0;

/**
 * Define `window.__TAURI_INTERNALS__` so `invoke` and `listen` resolve against
 * `handlers` instead of against a backend that is not there.
 */
export function installFakeTauri(handlers: Record<string, Handler>): FakeBridge {
  const callbacks = new Map<number, (payload: unknown) => void>();
  const listeners = new Map<number, Listener>();

  const invoke = async (cmd: string, args: Record<string, unknown> = {}) => {
    if (cmd === "plugin:event|listen") {
      const id = ++nextEventId;
      const callback = callbacks.get(args["handler"] as number);
      if (callback) {
        listeners.set(id, { event: String(args["event"]), callback });
      }
      return id;
    }
    if (cmd === "plugin:event|unlisten") {
      listeners.delete(args["eventId"] as number);
      return null;
    }
    const handler = handlers[cmd];
    if (!handler) {
      // The same shape a real rejection has, so error handling under the fake
      // exercises the code that runs against the real bridge.
      throw { code: "internal", message: `fake-tauri: no handler for ${cmd}`, source_id: null };
    }
    return handler(args);
  };

  const internals = {
    transformCallback(callback: (payload: unknown) => void) {
      const id = ++nextCallbackId;
      callbacks.set(id, callback);
      return id;
    },
    unregisterCallback(id: number) {
      callbacks.delete(id);
    },
    convertFileSrc: (path: string) => path,
  };

  // `invoke` read-only, as Tauri's own `scripts/core.js` defines it (`Object.defineProperty` with `value` only): a
  // module that assigns to it throws here as it does in the packaged app, which is how v3.2.9-ux7 hung at start
  // while every probe and fixture run passed. A probe taps commands through `window.__knobasIpcTap` instead.
  Object.defineProperty(internals, "invoke", { value: invoke });

  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: internals,
    configurable: true,
    writable: true,
  });

  // `unlisten()` is not only the `plugin:event|unlisten` command: since
  // `@tauri-apps/api` 2.x it first calls this second internals object to drop
  // the listener on the JS side, and reaches the command only afterwards. A
  // fake that defined `invoke` alone made every listener teardown -- every
  // room switch, once the tray held a subscription -- reject with a
  // `TypeError` nothing awaited. Found by mounting the real tray over this
  // bridge under the rejection guard every suite runs (#237).
  Object.defineProperty(window, "__TAURI_EVENT_PLUGIN_INTERNALS__", {
    value: {
      unregisterListener(_event: string, eventId: number) {
        listeners.delete(eventId);
      },
    },
    configurable: true,
    writable: true,
  });

  return {
    emit(event, payload) {
      for (const [id, listener] of listeners) {
        if (listener.event === event) {
          listener.callback({ event, id, payload });
        }
      }
    },
  };
}

/** What one `fake/<feature>.ts` module exports. */
interface FakeFeature {
  /** The commands this feature answers. */
  handlers: (params: URLSearchParams) => Record<string, Handler>;
  /** Read the page's query before the bridge is installed, where it needs to. */
  seed?: (params: URLSearchParams) => void;
  /** Hold the installed bridge, for a feature that emits events (a clone's progress, #846). */
  attach?: (bridge: FakeBridge) => void;
}

/** Every feature module, in path order. */
const FEATURES = Object.entries(import.meta.glob<FakeFeature>("./fake/*.ts", { eager: true })).sort(
  ([left], [right]) => left.localeCompare(right),
);

/**
 * Install the demo bridge when the page was opened with `?fake-ipc`.
 *
 * A query flag rather than "install whenever there is no Tauri", because
 * `just dev` serves the app from the same Vite server the browser QA uses: an
 * automatic fallback would silently answer from the fixture the first time the
 * real backend was slow to attach.
 */
export function installIfRequested(): void {
  // `fake-scale` turns on the per-area switches it stands for (`fake-scale.ts`).
  const params = expandScale(new URLSearchParams(location.search));
  if (!params.has("fake-ipc")) return;
  seedScale(params);
  for (const [, feature] of FEATURES) feature.seed?.(params);
  const real = params.get("fake-real");
  const handlers = real ? realBackend(Number(real), demoHandlers(params)) : demoHandlers(params);
  // The real backend's latency is its own: nothing is held back over it.
  const bridge = installFakeTauri(real ? handlers : holdBack(handlers, params));
  for (const [, feature] of FEATURES) feature.attach?.(bridge);
  // `?fake-hooks` (the still-rows trips, 2026-11-11): a trip adds rows to the corpus and ends a run that changed
  // something, so a page reads again with rows it did not draw.
  // `emit` (the monitors-builds pass): any event, so a probe on a real backend, which emits none, can end a run.
  if (params.has("fake-hooks")) Object.assign(window, { __fake: { corpus: CORPUS, endRuns: endRunsChanged, inboxNew: () => addInboxItem(), runNew: () => addFakeRuns(), monitorDown: () => downFakeMonitor(), emit: (event: string, payload: unknown) => bridge.emit(event, payload) } });
}

/**
 * `?fake-ipc&fake-real=<port>` (the still-rows pass, 2026-11-11): every command goes to the app's own backend running
 * with no window on that port (`crates/knobas-app/examples/headless_ipc.rs`), over a real database, so headless Chrome
 * draws a built front end on real data. A plugin's command stays the fixture's; nothing emits events.
 */
function realBackend(port: number, fixture: Record<string, Handler>): Record<string, Handler> {
  const forward = (cmd: string) => async (args: Record<string, unknown>) => {
    const response = await fetch(`http://127.0.0.1:${port}/invoke`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ cmd, args }),
    });
    const answer = (await response.json()) as { ok?: unknown; err?: unknown };
    if ("err" in answer) throw answer.err;
    return answer.ok;
  };
  return new Proxy(fixture, {
    get: (target, cmd) => (typeof cmd === "string" && !cmd.startsWith("plugin:") ? forward(cmd) : target[cmd as string]),
    ownKeys: (target) => Reflect.ownKeys(target),
  });
}

/** A read that never answers. */
const never = () => new Promise<never>(() => {});

/**
 * Reads held back for a screenshot of a page waiting on them:
 * `?fake-hold-pages` (#2245) answers `list_entities`' first page and never a
 * later one, so a list shows what it draws from its first page;
 * `?fake-hold-view=<prefix>` (#2246) never answers an `entity_view` whose
 * section starts with `<prefix>` (`commits`, `file:src/`).
 */
export function holdBack(handlers: Record<string, Handler>, params: URLSearchParams): Record<string, Handler> {
  const out = { ...handlers };
  const list = handlers["list_entities"];
  if (params.has("fake-hold-pages") && list) out["list_entities"] = (args) => (Number(args["offset"] ?? 0) > 0 ? never() : list(args));
  const view = handlers["entity_view"];
  const prefix = params.get("fake-hold-view");
  if (prefix && view) out["entity_view"] = (args) => (String(args["section"] ?? "").startsWith(prefix) ? never() : view(args));
  const late = lateReads(params.get("fake-late"));
  for (const name of Object.keys(out)) {
    const ms = late.get(name) ?? late.get("*");
    const inner = out[name]!;
    if (ms !== undefined) out[name] = (args) => new Promise((resolve) => setTimeout(resolve, ms)).then(() => inner(args));
  }
  return out;
}

/**
 * `?fake-late=code_facts:600,entity_checkouts:900` (the still-rows pass, 2026-11-11): each named command answers that
 * many ms late, as the real backend's reads land one by one after a page first draws; `*:<ms>` is every other command.
 * The rows-still trips use it to land each late read after the first draw.
 */
export function lateReads(value: string | null): Map<string, number> {
  const out = new Map<string, number>();
  for (const part of (value ?? "").split(",")) {
    const [command, ms] = part.split(":");
    const n = Number(ms);
    if (command && ms !== undefined && Number.isFinite(n) && n >= 0) out.set(command, n);
  }
  return out;
}

/**
 * The commands the fixture answers from its other reads rather than from a feature module (the query audit,
 * 2026-11-12): what many ids are, and a card's sections over one reach.
 */
export const COMPOSED = ["entity_briefs", "card_reads"] as const;

/**
 * The fixture's answers: every feature module's, put together.
 *
 * Grows one handler per task, as each screen learns to call something new, in
 * that screen's module under `fake/`.
 */
export function demoHandlers(params = new URLSearchParams()): Record<string, Handler> {
  const all: Record<string, Handler> = {};
  for (const [path, feature] of FEATURES) {
    for (const [command, handler] of Object.entries(feature.handlers(params))) {
      if (command in all) throw new Error(`fake-tauri: ${command} is answered twice, again by ${path}`);
      all[command] = handler;
    }
  }
  // The backend's one-call reads ({@link COMPOSED}), answered from the fixture's own reads they stand for,
  // so the two can never disagree: what each id is, by `get_entity`; a card's sections, by `job_reach` and
  // `list_entities`.
  all["entity_briefs"] ??= async (args) => {
    const ids = [...new Set((args["entityIds"] as string[] | undefined) ?? [])];
    const read = await Promise.allSettled(ids.map(async (entityId) => (await all["get_entity"]!({ entityId })) as EntityDetail));
    return read.flatMap((answer) => {
      if (answer.status !== "fulfilled") return [];
      const { entity_id, kind, source_id, title, state } = answer.value.row;
      return [{ entity_id, kind, source_id, title, state }];
    });
  };
  all["card_reads"] ??= async (args) => {
    const entityId = String(args["entityId"] ?? "");
    const limit = Number(args["limit"] ?? 200);
    const reached = (await all["job_reach"]!({ entityId })) as Reached[];
    const sections = await Promise.all(
      ((args["sections"] as EntityFilter[] | undefined) ?? []).map((filter) =>
        Promise.resolve(all["list_entities"]!({ filter: { ...filter, reach: entityId }, limit, offset: 0 })).then(
          (page) => (page as EntityPage).rows,
          () => [],
        ),
      ),
    );
    return { reached, sections };
  };
  return all;
}
