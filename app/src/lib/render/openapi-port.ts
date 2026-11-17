/**
 * Where an OpenAPI spec is parsed and kept (ux18): the spec worker in the app (`openapi.worker.ts`), this thread in
 * tests and wherever a worker cannot start. The view holds a {@link SpecSession} and asks it for what it draws.
 *
 * A ref into another file of the repository is read here, through the file read the view hands over (the source at
 * the spec's own commit, Fable's ruling in `docs/decisions/2026-11-17-an-api-spec-is-drawn-as-its-endpoints.md`):
 * only when a person opens it, one file at a time per ref, at most {@link FILE_CAP} bytes, at most
 * {@link IN_FLIGHT} reads at once, each file once per session. A ref to a URL is never read.
 */
import type { Opened, Operation, PathRef, Resolved, SpecCall, SpecFiles } from "./openapi";

export type { SpecCall };

/** Runs one call of one session where the spec is kept. */
export type SpecEngine = (session: number, call: SpecCall) => Promise<unknown>;

/** The most of another file a ref reads (Fable's bound). */
export const FILE_CAP = 5 * 1024 * 1024;
/** How many other files are read at once. */
export const IN_FLIGHT = 4;

let worker: Worker | null = null;
let workerFailed = false;
let nextCall = 0;
const waiting = new Map<number, (result: unknown) => void>();

/** The spec worker, started with the first spec, shared by every spec after it. */
function workerEngine(): SpecEngine | null {
  if (workerFailed || typeof Worker === "undefined") return null;
  if (worker === null) {
    try {
      worker = new Worker(new URL("./openapi.worker.ts", import.meta.url), { type: "module", name: "openapi" });
    } catch {
      workerFailed = true;
      return null;
    }
    worker.onmessage = (event: MessageEvent<{ id: number; result: unknown }>) => {
      const done = waiting.get(event.data.id);
      waiting.delete(event.data.id);
      done?.(event.data.result);
    };
    worker.onerror = () => {
      // A worker that cannot load answers nothing: every call waiting, and every call after, runs on this thread.
      workerFailed = true;
      worker?.terminate();
      worker = null;
      const pending = [...waiting.values()];
      waiting.clear();
      for (const done of pending) done({ ok: false, error: "The spec worker stopped.", line: null });
    };
  }
  const w = worker;
  return (session, call) =>
    new Promise((resolve) => {
      const id = ++nextCall;
      waiting.set(id, resolve);
      w.postMessage({ id, session, call });
    });
}

/** The spec kept on this thread: tests, and the fallback. js-yaml loads with the first call. */
export function localEngine(): SpecEngine {
  const sessions = new Map<number, SpecFiles>();
  return async (session, call) => (await import("./openapi")).answerCall(sessions, session, call);
}

/** The app's engine: the worker, else this thread. */
export function specEngine(): SpecEngine {
  const viaWorker = workerEngine();
  if (viaWorker === null) return localEngine();
  const local = localEngine();
  /** Each session's open, and the files a ref read into it, so this thread can take the session over. */
  const kept = new Map<number, { open: SpecCall; added: SpecCall[]; local: boolean }>();
  const onThread = async (session: number, call: SpecCall) => {
    const held = kept.get(session);
    if (held !== undefined && !held.local && call.kind !== "open") {
      held.local = true;
      await local(session, held.open);
      for (const add of held.added) await local(session, add);
    }
    return local(session, call);
  };
  // A worker that failed to load (a CSP, a missing chunk) answers nothing: its calls, and every call after, run here.
  return async (session, call) => {
    if (call.kind === "open") kept.set(session, { open: call, added: [], local: false });
    else if (call.kind === "addFile") kept.get(session)?.added.push(call);
    else if (call.kind === "close") kept.delete(session);
    if (workerFailed) return onThread(session, call);
    const result = await viaWorker(session, call);
    if (workerFailed && result !== null && typeof result === "object" && (result as { error?: string }).error === "The spec worker stopped.") {
      return onThread(session, call);
    }
    return result;
  };
}

let nextSession = 0;

/** Reads another file of the repository: its bytes at the spec's commit. */
export type ReadFile = (path: string) => Promise<Uint8Array>;

/** One spec opened in one view. */
export class SpecSession {
  readonly #engine: SpecEngine;
  readonly #id = ++nextSession;
  readonly #read: ReadFile | null;
  /** Files read for refs, by path: done, or why not. */
  readonly #files = new Map<string, Promise<{ ok: true } | { ok: false; error: string }>>();
  readonly #refs = new Map<string, Promise<Resolved>>();
  #active = 0;
  readonly #queue: (() => void)[] = [];
  #closed = false;

  constructor(engine: SpecEngine, read: ReadFile | null) {
    this.#engine = engine;
    this.#read = read;
  }

  open(path: string, text: string): Promise<Opened> {
    return this.#engine(this.#id, { kind: "open", path, text }) as Promise<Opened>;
  }

  operation(i: number): Promise<Operation | null> {
    return this.#engine(this.#id, { kind: "operation", i }) as Promise<Operation | null>;
  }

  /** A path item kept in another file: its operations, once that file is read. */
  async pathItem(ref: PathRef): Promise<{ ok: true; ops: Operation[] } | { ok: false; error: string }> {
    for (let tries = 0; tries < 2; tries++) {
      const r = (await this.#engine(this.#id, { kind: "pathItem", ref })) as { ok: true; ops: Operation[] } | { need: string } | { ok: false; error: string };
      if (!("need" in r)) return r;
      const loaded = await this.#load(r.need);
      if (!loaded.ok) return loaded;
    }
    return { ok: false, error: `${ref.ref} could not be read.` };
  }

  /** Follows `ref` from the file `base`, reading the file it names first where it is another; once per ref. */
  resolve(base: string, ref: string): Promise<Resolved> {
    const key = `${base}\n${ref}`;
    let held = this.#refs.get(key);
    if (held === undefined) {
      held = this.#follow(base, ref);
      this.#refs.set(key, held);
    }
    return held;
  }

  async #follow(base: string, ref: string): Promise<Resolved> {
    for (let tries = 0; tries < 2; tries++) {
      const r = (await this.#engine(this.#id, { kind: "resolve", base, ref })) as Resolved;
      if (!("need" in r)) return r;
      const loaded = await this.#load(r.need);
      if (!loaded.ok) return loaded;
    }
    return { ok: false, error: `${ref} could not be read.` };
  }

  #load(path: string): Promise<{ ok: true } | { ok: false; error: string }> {
    let held = this.#files.get(path);
    if (held === undefined) {
      held = this.#slot(() => this.#readFile(path));
      this.#files.set(path, held);
    }
    return held;
  }

  async #readFile(path: string): Promise<{ ok: true } | { ok: false; error: string }> {
    if (this.#read === null) return { ok: false, error: `${path} is another file, and knobas cannot read it here.` };
    if (this.#closed) return { ok: false, error: "The spec was closed." };
    let bytes: Uint8Array;
    try {
      bytes = await this.#read(path);
    } catch (cause) {
      return { ok: false, error: `${path} could not be read: ${cause instanceof Error ? cause.message : String(cause)}` };
    }
    if (bytes.length > FILE_CAP) return { ok: false, error: `${path} is over 5 MB, so knobas does not read it for a ref.` };
    const text = new TextDecoder().decode(bytes);
    return (await this.#engine(this.#id, { kind: "addFile", path, text })) as { ok: true } | { ok: false; error: string };
  }

  /** Runs `work` once fewer than {@link IN_FLIGHT} reads are running. */
  async #slot<T>(work: () => Promise<T>): Promise<T> {
    if (this.#active >= IN_FLIGHT) await new Promise<void>((go) => this.#queue.push(go));
    this.#active++;
    try {
      return await work();
    } finally {
      this.#active--;
      this.#queue.shift()?.();
    }
  }

  close(): void {
    if (this.#closed) return;
    this.#closed = true;
    void this.#engine(this.#id, { kind: "close" });
  }
}
