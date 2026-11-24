/**
 * Where a document's outline is read and kept (the XML view's design, 2026-11-23, section 27): the document worker in
 * the app (`doc.worker.ts`), this thread in tests and wherever a worker cannot start. The view holds a
 * {@link DocSession} and asks it for what it draws; the first frame never waits for it.
 */
import type { CopyWhat, Crumb, DocCall, DocOpened, Picked, QueryAnswer, ResultRow, ValueAt } from "./doc-model";

/** Runs one call of one session where the outline is kept. */
export type DocEngine = (session: number, call: DocCall) => Promise<unknown>;

let worker: Worker | null = null;
let workerFailed = false;
let nextCall = 0;
const waiting = new Map<number, (result: unknown) => void>();

/** The document worker, started with the first document, shared by every document after it. */
function workerEngine(): DocEngine | null {
  if (workerFailed || typeof Worker === "undefined") return null;
  if (worker === null) {
    try {
      worker = new Worker(new URL("./doc.worker.ts", import.meta.url), { type: "module", name: "doc" });
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
      for (const done of pending) done({ stopped: true });
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

/** The outline kept on this thread: tests, and the fallback. */
export function localDocEngine(): DocEngine {
  const sessions = new Map();
  return async (session, call) => {
    const model = await import("./doc-model");
    return model.answerDoc(sessions, session, call);
  };
}

/** The app's engine: the worker, else this thread. */
export function docEngine(): DocEngine {
  const viaWorker = workerEngine();
  if (viaWorker === null) return localDocEngine();
  const local = localDocEngine();
  const opens = new Map<number, DocCall>();
  let moved = new Set<number>();
  return async (session, call) => {
    if (call.kind === "open") opens.set(session, call);
    else if (call.kind === "close") opens.delete(session);
    const onThread = async () => {
      // The worker stopped: this thread takes the session over, reading the document again.
      if (call.kind !== "open" && !moved.has(session)) {
        const open = opens.get(session);
        if (open !== undefined) await local(session, open);
        moved = new Set(moved).add(session);
      }
      return local(session, call);
    };
    if (workerFailed) return onThread();
    const result = await viaWorker(session, call);
    if (result !== null && typeof result === "object" && (result as { stopped?: boolean }).stopped === true) return onThread();
    return result;
  };
}

let nextSession = 0;

/** What a pick answers: the way down, and what the line holds. */
export interface AtAnswer {
  crumbs: Crumb[];
  picked: Picked | null;
}

/** One document opened in one view. */
export class DocSession {
  readonly #engine: DocEngine;
  readonly #id = ++nextSession;
  #closed = false;

  constructor(engine: DocEngine) {
    this.#engine = engine;
  }

  /** Reads the document: its fold table and what its root says. */
  open(text: string): Promise<DocOpened> {
    return this.#engine(this.#id, { kind: "open", text }) as Promise<DocOpened>;
  }

  /** The way down to a line, and what it holds. */
  at(line: number): Promise<AtAnswer> {
    return this.#engine(this.#id, { kind: "at", line }).then((r) => (r as AtAnswer | null) ?? { crumbs: [], picked: null });
  }

  /** The folds that start on these lines, named by their elements (`null` where none folds). */
  keys(lines: number[]): Promise<(string | null)[]> {
    return this.#engine(this.#id, { kind: "keys", lines }) as Promise<(string | null)[]>;
  }

  /** The folds named by these keys, found again in this document: the lines they start on. */
  find(keys: string[]): Promise<number[]> {
    return this.#engine(this.#id, { kind: "find", keys }) as Promise<number[]>;
  }

  /** Answers an XPath expression (section 22): the first rows of its nodes and every line they stand on, or a value. */
  query(source: string): Promise<QueryAnswer> {
    return this.#engine(this.#id, { kind: "query", source }) as Promise<QueryAnswer>;
  }

  /** More rows of the last query's nodes, as the list scrolls. */
  results(from: number, count: number): Promise<ResultRow[]> {
    return this.#engine(this.#id, { kind: "results", from, count }).then((r) => (r as ResultRow[] | null) ?? []);
  }

  /** The last query's nodes as *Copy ▾* copies them. */
  copy(what: CopyWhat): Promise<string> {
    return this.#engine(this.#id, { kind: "copy", what }).then((r) => (r as string | null) ?? "");
  }

  /** Queries to press, made from the document's own names. */
  examples(): Promise<string[]> {
    return this.#engine(this.#id, { kind: "examples" }).then((r) => (r as string[] | null) ?? []);
  }

  /** *Query for elements like this*: the line's element and the one that holds it, `//PRODUCT_ID/bmecat:SUPPLIER_PID`. */
  like(line: number): Promise<string | null> {
    return this.#engine(this.#id, { kind: "like", line }) as Promise<string | null>;
  }

  /** The document laid out by knobas (section 6): the session goes on over it, a value still found in the file. */
  layout(): Promise<{ text: string; opened: DocOpened } | null> {
    return this.#engine(this.#id, { kind: "layout" }) as Promise<{ text: string; opened: DocOpened } | null>;
  }

  /** The values on these lines (section 17); with `all`, any value a line holds, short ones too. */
  values(lines: number[], all = false): Promise<Record<number, ValueAt[]>> {
    return this.#engine(this.#id, { kind: "values", lines, all }).then((r) => (r as Record<number, ValueAt[]> | null) ?? {});
  }

  /** The lines of the nodes these paths name (libxml2's findings): -1 where none. */
  places(paths: (string | null)[]): Promise<number[]> {
    return this.#engine(this.#id, { kind: "places", paths }).then((r) => (Array.isArray(r) ? (r as number[]) : paths.map(() => -1)));
  }

  /** The element on a line, as names with their namespaces from the root down. */
  names(line: number): Promise<{ ns: string; local: string; line: number }[]> {
    return this.#engine(this.#id, { kind: "names", line }).then((r) => (Array.isArray(r) ? (r as { ns: string; local: string; line: number }[]) : []));
  }

  close(): void {
    if (this.#closed) return;
    this.#closed = true;
    void this.#engine(this.#id, { kind: "close" });
  }
}
