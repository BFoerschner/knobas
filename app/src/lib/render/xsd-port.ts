/**
 * Where a schema set is read and kept (the XSD design, 2026-11-23, sections 6 and 30): the schema worker in the app
 * (`xsd.worker.ts`), this thread in tests and wherever a worker cannot start. The explorer holds an {@link XsdPort} and
 * asks it for what it draws.
 *
 * - **The shell draws first.** `open` answers from the opened file alone, its references *reading…*; the port then
 *   reads what the worker names (at most {@link FILE_CAP} a file, {@link AT_ONCE} at once) and hands each round's
 *   answer to `onindex`, so each file's declarations fill rows that already stand.
 * - **The repository is indexed only when asked** (`?index`, `xsd-index.ts`), and another repository only when a
 *   reference is not found in this one (`?elsewhere:<namespace>`).
 * - **A schema opened again** reads its files from the worker's kept trees (by text): only the compile runs again.
 * - **The check's set is kept whole** where the reads are pinned to a commit (`XsdReads.keep`, `xsd-kept.ts`): a
 *   document opened again, or another on the same schema, is answered whole by the open, with nothing read.
 */
import type { ChangesAnswer, Checked, Opened, PageAnswer, Row, SchemaIndexWire, SetIndex, TreeRow, UseRow, XsdCall } from "./xsd-answer";
import type { Elsewhere, Pointed } from "./xsd-locate";
import type { KeepAt } from "./xsd-kept";
import type { Example, ExampleOptions } from "./xsd-example";

/** Runs one call of one session where the set is kept. */
export type XsdEngine = (session: number, call: XsdCall) => Promise<unknown>;

/** The most of a file the set reads (ADR-0058). */
export const FILE_CAP = 5 * 1024 * 1024;
/** How many files are read at once. */
export const AT_ONCE = 6;

let worker: Worker | null = null;
let workerFailed = false;
let nextCall = 0;
const waiting = new Map<number, (result: unknown) => void>();

function workerEngine(): XsdEngine | null {
  if (workerFailed || typeof Worker === "undefined") return null;
  if (worker === null) {
    try {
      worker = new Worker(new URL("./xsd.worker.ts", import.meta.url), { type: "module", name: "xsd" });
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

/**
 * Every set the worker keeps whole is dropped (`xsd-kept.ts`): a pick, a download, *Download again*, a file pointed to
 * forgotten. A worker not started keeps nothing.
 */
export function forgetKeptSets(): void {
  worker?.postMessage({ id: ++nextCall, session: 0, call: { kind: "forget" } satisfies XsdCall });
}

/** The set kept on this thread: tests, and the fallback. */
export function localXsdEngine(): XsdEngine {
  const sessions = new Map();
  return async (session, call) => {
    const [{ answerXsd }, { BUNDLED }] = await Promise.all([import("./xsd-answer"), import("./xsd-known")]);
    return answerXsd(sessions, session, call, BUNDLED);
  };
}

/** The app's engine: the worker, else this thread. */
export function xsdEngine(): XsdEngine {
  const viaWorker = workerEngine();
  if (viaWorker === null) return localXsdEngine();
  const local = localXsdEngine();
  return async (session, call) => {
    const result = await viaWorker(session, call);
    if (result !== null && typeof result === "object" && (result as { stopped?: boolean }).stopped === true) return local(session, call);
    return result;
  };
}

/** What the explorer hands the port: the repository at the commit shown, and the reads beyond it. */
export interface XsdReads {
  /** Every path of the repository at the commit shown: read only when the index is (steps 3 to 5). */
  readonly paths: () => Promise<readonly string[]>;
  /** A file's text at the commit shown; `null` where it cannot be read. */
  readonly read: (path: string) => Promise<string | null>;
  /** A file of another kind (`@repo/…`, `@download/…`): read where it lives. */
  readonly readAway?: (path: string) => Promise<string | null>;
  /** The repository's schemas indexed (`xsd-index.ts`). */
  readonly index: (from: string, paths: readonly string[]) => Promise<SchemaIndexWire>;
  /** Other synced repositories' `.xsd` that declare a namespace (`files_declaring`). */
  readonly elsewhere?: (namespace: string) => Promise<Elsewhere[]>;
  readonly pointed: readonly Pointed[];
  /** What the reads stand on, where it does not move (a commit, or the working tree's stamp): the set is kept whole by it. */
  readonly keep?: KeepAt | null;
}

let nextSession = 0;

/** Reads `paths` through the port's reads, {@link AT_ONCE} at a time, each at most {@link FILE_CAP}; in their order. */
async function readAll(reads: XsdReads, paths: readonly string[]): Promise<{ path: string; text: string | null }[]> {
  const out = new Map<string, string | null>();
  const queue = [...paths];
  const work = async () => {
    for (let p = queue.shift(); p !== undefined; p = queue.shift()) {
      const text = await (p.startsWith("@") ? (reads.readAway?.(p) ?? Promise.resolve(null)) : reads.read(p)).catch(() => null);
      out.set(p, text !== null && text.length > FILE_CAP ? null : text);
    }
  };
  await Promise.all(Array.from({ length: AT_ONCE }, work));
  return paths.map((path) => ({ path, text: out.get(path) ?? null }));
}

/** An answer to an open, or what a worker that answered nothing usable means. */
function opened(result: unknown): Opened {
  if (result !== null && typeof result === "object" && "ok" in result) return result as Opened;
  return { ok: false, error: "The schema could not be read.", line: null };
}

/** One schema set opened in one view. */
export class XsdPort {
  readonly #engine: XsdEngine;
  readonly #id = ++nextSession;
  #closed = false;
  #reads: XsdReads | null = null;
  #entry = "";
  /** Each round's index, as the files land. */
  onindex: ((index: SetIndex) => void) | null = null;
  /** The last index answered. */
  index: SetIndex | null = null;
  /** Settles once every file the set needs has been read (or found not to be). */
  whole: Promise<void> = Promise.resolve();
  /** The verdict on the document that crossed with the open, from the round that left the set whole. */
  checked: Checked | null = null;

  constructor(engine: XsdEngine) {
    this.#engine = engine;
  }

  /** The session's id where its engine keeps it: a comparison names both sides' (S7). */
  get id(): number {
    return this.#id;
  }

  /** The engine the set is kept by: two sides compared must be kept by one. */
  get engine(): XsdEngine {
    return this.#engine;
  }

  /**
   * Reads the opened file alone and answers at once; the files it names are read after (`onindex`), once `after`
   * settles true (the XML view's outline: its reads never slow the outline's paint). A set kept whole needs none.
   *
   * - `xml`: the document a check reads, crossed once: the round that leaves the set whole answers its verdict too
   *   ({@link checked}), with no trip of its own.
   * - `hint`: the paths the set read last time (`xsd-hint.ts`), read once `after` settles, six at a time, beside the
   *   first round; the worker takes each only when its own walk asks for it.
   */
  async open(entry: string, text: string, reads: XsdReads, home?: string, lean = false, after: Promise<boolean> | null = null, more: { xml?: string; hint?: readonly string[] } = {}): Promise<Opened> {
    this.#reads = reads;
    this.#entry = entry;
    const keep = reads.keep ?? null;
    const hint = more.hint ?? [];
    const go = after ?? Promise.resolve(true);
    // Read from the hint as soon as reads may start, beside the open's answer.
    const hinted = hint.length === 0 ? null : go.then((yes) => (yes && !this.#closed ? readAll(reads, hint) : []));
    const answer = opened(await this.#engine(this.#id, { kind: "open", entry, text, paths: null, pointed: [...reads.pointed], ...(home !== undefined ? { home } : {}), ...(lean ? { lean } : {}), ...(keep !== null ? { keep } : {}), ...(more.xml !== undefined ? { xml: more.xml } : {}) }));
    if (answer.ok) {
      this.index = answer.index;
      if (answer.checked !== undefined) this.checked = answer.checked;
      const need = answer.need;
      if (need.length > 0) this.whole = after === null ? this.#follow(need, hinted) : after.then((yes) => (yes ? this.#follow(need, hinted) : undefined));
    }
    return answer;
  }

  /** Reads what the set needs, round after round, until it needs nothing; the first round brings what the hint read. */
  async #follow(need: readonly string[], hinted: Promise<{ path: string; text: string | null }[]> | null = null): Promise<void> {
    let ask = need;
    const asked = new Set<string>();
    for (let round = 0; round < 64 && ask.length > 0 && !this.#closed; round += 1) {
      const reads = this.#reads!;
      let index: SchemaIndexWire | undefined;
      let elsewhere: { namespace: string; list: Elsewhere[] } | undefined;
      const wanted = ask.filter((p) => !p.startsWith("?") && !asked.has(p));
      for (const p of ask) asked.add(p);
      const byHint = round === 0 && hinted !== null ? await hinted : [];
      const had = new Map(byHint.map((f) => [f.path, f]));
      const files = [...wanted.flatMap((p) => had.get(p) ?? []), ...(await readAll(reads, wanted.filter((p) => !had.has(p))))];
      const aside = byHint.filter((f) => !wanted.includes(f.path));
      let paths: string[] | undefined;
      if (ask.includes("?index")) {
        paths = [...(await reads.paths().catch(() => []))];
        index = await reads.index(this.#entry, paths).catch(() => ({ declares: [], catalogs: [], total: 0, read: 0 }));
      }
      const away = ask.find((p) => p.startsWith("?elsewhere:"));
      if (away !== undefined) {
        const namespace = away.slice("?elsewhere:".length);
        elsewhere = { namespace, list: (await reads.elsewhere?.(namespace).catch(() => [])) ?? [] };
      }
      if (this.#closed) return;
      const call: XsdCall = { kind: "addFiles", files };
      if (aside.length > 0) call.hinted = aside;
      if (index !== undefined) call.index = index;
      if (paths !== undefined) call.paths = paths;
      if (elsewhere !== undefined) call.elsewhere = elsewhere;
      const answer = opened(await this.#engine(this.#id, call));
      if (!answer.ok || this.#closed) return;
      if (answer.checked !== undefined) this.checked = answer.checked;
      this.index = answer.index;
      this.onindex?.(answer.index);
      // Ask each path once: a file that answered nothing is not asked again.
      ask = answer.need.filter((p) => !asked.has(p));
    }
  }

  /** The person pointed a reference at a file: the set is read again. */
  async repoint(pointed: readonly Pointed[]): Promise<void> {
    if (this.#reads === null) return;
    this.#reads = { ...this.#reads, pointed };
    const answer = opened(await this.#engine(this.#id, { kind: "pointed", pointed: [...pointed] }));
    if (!answer.ok) return;
    this.index = answer.index;
    this.onindex?.(answer.index);
    if (answer.need.length > 0) await this.#follow(answer.need);
  }

  /** A declaration's page, whole. */
  page(place: string): Promise<PageAnswer | { missing: string; reading: number }> {
    return this.#engine(this.#id, { kind: "page", place }) as Promise<PageAnswer | { missing: string; reading: number }>;
  }

  /** What a row opened in place holds. */
  rows(id: string): Promise<Row[]> {
    return this.#engine(this.#id, { kind: "rows", id }).then((r) => (Array.isArray(r) ? (r as Row[]) : []));
  }

  /** A fold of the tree: the roots for `null`. */
  tree(under: string | null): Promise<TreeRow[]> {
    return this.#engine(this.#id, { kind: "tree", under }).then((r) => (Array.isArray(r) ? (r as TreeRow[]) : []));
  }

  /** Everyone who uses a declaration: *Show the other*. */
  uses(key: string): Promise<UseRow[]> {
    return this.#engine(this.#id, { kind: "uses", key }).then((r) => (Array.isArray(r) ? (r as UseRow[]) : []));
  }

  /** The shortest way down the tree to an element: the rows to open, its own last. */
  way(key: string): Promise<string[]> {
    return this.#engine(this.#id, { kind: "way", key }).then((r) => (Array.isArray(r) ? (r as string[]) : []));
  }

  /** The declaration of a document's element, by its names from the root down (section 26). */
  declOf(names: { ns: string; local: string }[]): Promise<(PageAnswer & { found: number }) | null> {
    return this.#engine(this.#id, { kind: "declOf", names }).then((r) => (r !== null && typeof r === "object" && "kind" in r ? (r as PageAnswer & { found: number }) : null));
  }

  /** An example of the element `key` names (section 19); `null` where it is no element of the set. */
  example(key: string, options: ExampleOptions): Promise<Example | null> {
    return this.#engine(this.#id, { kind: "example", key, options }).then((r) => (r !== null && typeof r === "object" && "xml" in r ? (r as Example) : null));
  }

  /** libxml2's verdict on a document against the set (section 24). */
  check(xml: string): Promise<Checked> {
    return this.#engine(this.#id, { kind: "check", xml }).then((r) =>
      r !== null && typeof r === "object" && "state" in r ? (r as Checked) : { state: "not-checked", why: "not compiled", detail: ["The checker stopped."] },
    );
  }

  /** Every value a type's enumeration allows: *Show the other 1,047*. */
  values(key: string, from = 0): Promise<{ v: string; title: string }[]> {
    return this.#engine(this.#id, { kind: "values", key, from }).then((r) => (Array.isArray(r) ? (r as { v: string; title: string }[]) : []));
  }

  close(): void {
    if (this.#closed) return;
    this.#closed = true;
    void this.#engine(this.#id, { kind: "close" });
  }
}

/**
 * Two versions of a schema compared by what they declare (S7): each side a port opened on the same engine, `null` for a
 * side that holds no schema; `before` and `after` the compared file's path on each side.
 */
export async function compareSets(old: XsdPort | null, now: XsdPort | null, before: string, after: string): Promise<ChangesAnswer | null> {
  const by = now ?? old;
  if (by === null) return null;
  const answer = await by.engine(by.id, { kind: "changes", old: old?.id ?? null, new: now?.id ?? null, before, after });
  return answer !== null && typeof answer === "object" && "rows" in answer ? (answer as ChangesAnswer) : null;
}
