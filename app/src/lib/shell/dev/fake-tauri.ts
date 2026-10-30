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
import { installPipelines } from "../../tf/diff/piped";
import type { Answers } from "./fake-answers";

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
    invoke,
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
  const params = new URLSearchParams(location.search);
  if (!params.has("fake-ipc")) return;
  for (const [, feature] of FEATURES) feature.seed?.(params);
  // `?fake-diff-pipeline` (#2270): a stand-in default pipeline for `*.json.b64` until #2265's store installs the real ones.
  if (params.has("fake-diff-pipeline"))
    installPipelines({
      of: (path) => (path.endsWith(".json.b64") ? { name: null, pattern: "*.json.b64", steps: [{ step: "base64_decode" }, { step: "pretty", format: "json" }] } : null),
    });
  const bridge = installFakeTauri(holdBack(demoHandlers(params), params));
  for (const [, feature] of FEATURES) feature.attach?.(bridge);
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
  return out;
}

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
  return all;
}
