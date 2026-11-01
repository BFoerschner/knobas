/**
 * D2 behind the renderer port (#2259, spec #2234 step 5): D2's official WASM
 * build (`@terrastruct/d2`), a chunk of its own that loads with the first D2
 * diagram.
 *
 * **Where it runs** (#2381). The package runs its WASM in a `blob:` worker of
 * its own, and in WebKit (the bundled app) D2's Go WASM overflows a worker
 * thread's stack while it starts. So the package's worker source runs on the
 * thread that starts D2 instead ({@link ThreadWorker}): in the app that is
 * D2's frame (`d2-frame.ts`), a sandboxed document of its own whose policy
 * allows WASM and `new Function`, which the app's document does not; in dev
 * and tests (no bridge, no CSP) it is the page itself (`port.ts`).
 *
 * - **Colours** are the knobas tokens of the theme drawn now, as D2 theme
 *   overrides on its default theme. A theme override the source sets itself
 *   (`vars.d2-config.theme-overrides`) wins, and a source that picks a
 *   theme of its own (`theme-id`) keeps it.
 * - **One at a time**: the package answers one request at a time, so the
 *   renders queue.
 * - **Errors** are D2's messages, one per line, as `line 3:5: …`.
 * - The SVG still goes through the sanitizer and the sandboxed frame
 *   (`frame.ts`) on the way into the page.
 */
import { D2 } from "@terrastruct/d2";

import type { DiagramAnswer, Palette } from "./port";

/** The part of the package's `D2` this module uses. */
export interface D2Engine {
  compile(input: string, options?: Record<string, unknown>): Promise<{ diagram: unknown; renderOptions: Record<string, unknown> }>;
  render(diagram: never, options?: Record<string, unknown>): Promise<string>;
}

/** D2's theme colour slots, filled from the palette (D2's default theme names: N text to background, B shapes, AA/AB accents). */
function overrides(p: Palette): Record<string, string> {
  return {
    n1: p.ink, n2: p.ink2, n3: p.ink2, n4: p.rule, n5: p.rule, n6: p.mutedBg, n7: p.sheet,
    b1: p.ink2, b2: p.ink2, b3: p.rule, b4: p.mutedBg, b5: p.sheet2, b6: p.sheet2,
    aa2: p.ink2, aa4: p.mutedBg, aa5: p.sheet2, ab4: p.mutedBg, ab5: p.sheet2,
  };
}

/**
 * A renderer over the engine `make` gives, made on the first render. An
 * engine that fails to start, or has not started within `startLimit` ms, is
 * an error, and is made again on the next render. The limit is there because
 * the package never says when its own worker could not load (refused by a
 * CSP, say): it would wait for ever, and the diagram would stay its source
 * with no error.
 */
export function d2Renderer(
  make: () => D2Engine | Promise<D2Engine>,
  startLimit = 30_000,
): (code: string, palette: Palette) => Promise<DiagramAnswer> {
  let engine: Promise<D2Engine> | null = null;
  let queue: Promise<unknown> = Promise.resolve();
  return (code, palette) => {
    const run = queue.then(() => {
      const started = (engine ??= withLimit(Promise.resolve().then(make), startLimit));
      started.catch(() => engine === started && (engine = null));
      return drawOnce(started, code, palette);
    });
    queue = run.catch(() => undefined);
    return run;
  };
}

function withLimit<T>(promise: Promise<T>, ms: number): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const late = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error(`D2 did not start within ${ms / 1000} s.`)), ms);
  });
  return Promise.race([promise, late]).finally(() => clearTimeout(timer));
}

async function drawOnce(started: Promise<D2Engine>, code: string, palette: Palette): Promise<DiagramAnswer> {
  try {
    const engine = await started;
    const { diagram, renderOptions } = await engine.compile(code, { pad: 16 });
    const own = (renderOptions.themeOverrides ?? {}) as Record<string, string | null>;
    const themed = renderOptions.themeID === 0 || renderOptions.themeID == null;
    const themeOverrides = themed ? { ...overrides(palette), ...Object.fromEntries(Object.entries(own).filter(([, v]) => v != null)) } : own;
    const svg = await engine.render(diagram as never, { ...renderOptions, themeOverrides, noXMLTag: true });
    return { svg };
  } catch (error) {
    return { error: messageOf(error) };
  }
}

/** D2's compile error is a JSON list of `{ errmsg: "index:1:6: …" }`; anything else is its own message. */
function messageOf(error: unknown): string {
  const text = error instanceof Error ? error.message : String(error);
  try {
    const list = JSON.parse(text) as { errmsg?: string }[];
    if (Array.isArray(list) && list.every((e) => typeof e.errmsg === "string")) {
      return list.map((e) => e.errmsg!.replace(/^index:(\d+:\d+): /, "line $1: ")).join("\n");
    }
  } catch {
    // Not JSON: D2's message as it is.
  }
  return text;
}

/** The call that ends the package's worker source: it answers on the worker's own `self`. */
const WORKER_CALL = "setupMessageHandler(false, self, initWasmBrowser);";
/** Where the rewritten source finds the port it answers on while it is evaluated. */
const PORT = "__knobasD2Port";

interface Port {
  onmessage: ((event: { data: unknown }) => void) | null;
  postMessage(message: unknown): void;
}

/**
 * The package's worker (`new Worker(blob:…)`), run on this thread (#2381).
 * It reads the worker's source (`read`), has it answer a port of this
 * object's in place of its `self`, and evaluates it as a module (`run`);
 * messages go between the package and that port as they would between it
 * and its worker.
 *
 * The package says `ready` even when its Go runtime threw while it started
 * (it does not await `go.run`), and its first `compile` then fails on an
 * engine that is not there. So a `ready` with no engine on `globalThis.d2`
 * is answered as the error it is, with the reason the start was rejected
 * with when there was one.
 */
export class ThreadWorker {
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onerror: ((event: unknown) => void) | null = null;
  readonly #port: Port;
  readonly #started: Promise<void>;
  #rejected: string | null = null;

  constructor(url: string | URL, read: (url: string) => Promise<string>, run: (module: string) => Promise<unknown>) {
    this.#port = { onmessage: null, postMessage: (message) => queueMicrotask(() => this.#answer(message)) };
    this.#started = read(String(url)).then(async (source) => {
      const scope = globalThis as Record<string, unknown>;
      scope[PORT] = this.#port;
      try {
        await run(onThisThread(source));
      } finally {
        delete scope[PORT];
      }
    });
  }

  postMessage(message: unknown): void {
    const init = (message as { type?: unknown } | null)?.type === "init";
    if (init) globalThis.addEventListener?.("unhandledrejection", this.#heard);
    this.#started.then(
      () => this.#port.onmessage?.({ data: message }),
      (error: unknown) => this.#deliver({ type: "error", error: `D2 could not start: ${messageOf(error)}` }),
    );
  }

  terminate(): void {
    globalThis.removeEventListener?.("unhandledrejection", this.#heard);
  }

  readonly #heard = (event: Event) => {
    const reason = (event as PromiseRejectionEvent).reason as unknown;
    this.#rejected ??= reason instanceof Error ? reason.message : String(reason);
  };

  #answer(message: unknown): void {
    const type = (message as { type?: unknown } | null)?.type;
    if (type !== "ready" && type !== "error") return this.#deliver(message);
    // An unhandled rejection is reported after the task that made it: wait one, so its reason is heard.
    setTimeout(() => {
      this.terminate();
      const engine = (globalThis as { d2?: { compile?: unknown } }).d2;
      if (type === "ready" && typeof engine?.compile !== "function") {
        this.#deliver({ type: "error", error: `D2 could not start: ${this.#rejected ?? "its engine did not come up."}` });
      } else this.#deliver(message);
    });
  }

  #deliver(message: unknown): void {
    this.onmessage?.({ data: message });
  }
}

/** The package's worker source, answering `globalThis[PORT]` in place of its `self`. */
function onThisThread(source: string): string {
  const at = source.indexOf(WORKER_CALL);
  if (at < 0 || source.indexOf(WORKER_CALL, at + 1) >= 0) {
    throw new Error("its worker source has changed (a new @terrastruct/d2?), and it cannot run on this thread.");
  }
  return `${source.slice(0, at)}setupMessageHandler(false, globalThis.${PORT}, initWasmBrowser);${source.slice(at + WORKER_CALL.length)}`;
}

/** A `blob:` URL's text. */
async function readBlob(url: string): Promise<string> {
  return (await fetch(url)).text();
}

/** `source` evaluated as a module on this thread. */
async function runModule(source: string): Promise<unknown> {
  const url = URL.createObjectURL(new Blob([source], { type: "text/javascript" }));
  try {
    return await import(/* @vite-ignore */ url);
  } finally {
    URL.revokeObjectURL(url);
  }
}

/**
 * The package's browser build, its worker on this thread. The package makes
 * its worker inside `new D2()`, before its first `await`, so `Worker` is
 * {@link ThreadWorker} for that call only, and every other worker of the
 * page is a real one.
 */
async function startOnThisThread(): Promise<D2Engine> {
  const scope = globalThis as { Worker: unknown };
  const real = scope.Worker;
  scope.Worker = class extends ThreadWorker {
    constructor(url: string | URL) {
      super(url, readBlob, runModule);
    }
  };
  let d2: D2;
  try {
    d2 = new D2();
  } finally {
    scope.Worker = real;
  }
  await (d2 as unknown as { ready: Promise<void> }).ready;
  return d2 as unknown as D2Engine;
}

/** The app's D2. */
export const renderD2 = d2Renderer(startOnThisThread);
