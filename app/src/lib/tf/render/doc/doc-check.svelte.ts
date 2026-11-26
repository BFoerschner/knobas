/**
 * The schema check in the XML view (the XML view's design, 2026-11-23, sections 10 to 12; the XSD design's 24 to 26).
 * When the view opens a document that says what its schema is, knobas looks for it and checks the document by itself.
 *
 * - **What the document says** is its root's namespace, `xsi:schemaLocation` (the pair for the root's namespace) and
 *   `xsi:noNamespaceSchemaLocation`. One that says none of the three has no schema, and nothing is looked for.
 * - **Where it is looked for** is the schema explorer's own order: the document's schema is read as a set whose entry
 *   imports the root's namespace from the document's folder (or includes the no-namespace location), so every step,
 *   the person's pointing, the catalog, the index, another repository and the schemas that come with knobas, is the
 *   one the explorer takes.
 * - **The check runs once the set is whole**, by libxml2 in the schema worker, on the document's text.
 * - **It starts with the view**, the root read from the text's first tag, but reads no file before the outline has
 *   landed, and draws nothing before it: a set kept whole (`xsd-kept.ts`) is checked beside the outline, so the verdict
 *   lands with it.
 * - **A departure is put on its line by libxml2's path to the node** (the document worker's `places`), never by its
 *   line, which is 16 bits.
 * - **Stop checking this file** holds until knobas quits.
 */
import type { DocSession } from "../../../render/doc-port";
import type { Checked, Departure, PageAnswer, SetIndex } from "../../../render/xsd-answer";
import type { Pointed } from "../../../render/xsd-locate";
import type { KeepAt } from "../../../render/xsd-kept";
import type { Words } from "../../../render/xsd-findings";
import { isRemote } from "../../../render/xsd-locate";
import { XsdPort, xsdEngine, type XsdEngine, type XsdReads } from "../../../render/xsd-port";
import { hintKey, hintStore, hintedPaths, keepHint } from "../../../render/xsd-hint";
import type { KeptValue } from "../../kept-value";
import { firstTag } from "../../../render/xsd-sniff";
import { XS, XSI } from "../../../render/xsd-xml";

/** The files whose check was stopped, until knobas quits (`repository\npath`). */
const stopped = new Set<string>();
/** Forgets every stopped check (tests). */
export function forgetStopped(): void {
  stopped.clear();
}

export type Head =
  /** The outline has not landed. */
  | { readonly k: "idle" }
  | { readonly k: "looking"; readonly found: string | null; readonly how: string | null }
  | { readonly k: "checked"; readonly count: number; readonly total: number; readonly against: string; readonly how: string; readonly ms: number; readonly readMs: number; readonly files: number }
  | { readonly k: "not-checked"; readonly warn: boolean; readonly lines: readonly string[]; readonly against: string | null }
  | { readonly k: "not-found"; readonly location: string }
  | { readonly k: "address"; readonly address: string; readonly namespace: string | null }
  | { readonly k: "two"; readonly namespace: string }
  | { readonly k: "no-schema"; readonly namespace: string | null }
  | { readonly k: "off" };

/** The Settings a check reads: whether XML files are checked, and the files pointed to. */
export interface CheckSettings {
  readonly on: boolean;
  readonly pointed: readonly Pointed[];
}
/** The Settings the last check read: the next check starts on them while they are read again. */
let lastSettings: CheckSettings | null = null;
const sameSettings = (a: CheckSettings, b: CheckSettings) => a.on === b.on && JSON.stringify(a.pointed) === JSON.stringify(b.pointed);

/**
 * The Settings a check starts on, without waiting for their read: those the last check read, where they answer the
 * same once read again (`same`); else `null`, and the check waits for `fresh`. Two reads of the profile at 6x come to
 * some 50 ms, enough to put the check's first answer behind the outline's paint.
 */
export function settingsFor(read: () => Promise<CheckSettings>): { held: CheckSettings | null; same: Promise<boolean>; fresh: Promise<CheckSettings> } {
  const held = heldSettings();
  const fresh = read().then((s) => {
    lastSettings = s;
    return s;
  });
  return { held, same: held === null ? Promise.resolve(false) : fresh.then((s) => sameSettings(s, held)), fresh };
}
/** The Settings the last check read, `null` before the first. */
export function heldSettings(): CheckSettings | null {
  return lastSettings;
}
/** Forgets what checks hold from one document to the next: the Settings, the asks seen whole, the lines (tests). */
export function forgetHeld(): void {
  lastSettings = null;
  seenWhole.clear();
  linesKept.length = 0;
}

/** A departure placed on its line. */
export interface Placed {
  readonly line: number;
  readonly words: Words;
  readonly raw: string;
}

/** What the document's root says of its schema (`DocOpened.root`). */
export interface RootSays {
  readonly name: string;
  readonly namespace: string;
  readonly location: string | null;
  /** The prefixes the document binds at its root: a finding writes names as the document does. */
  readonly prefixes?: Readonly<Record<string, string>>;
}

/** How a document's schema was found, in the rail's words after *found* (the XML view's section 10). */
const FOUND: Record<string, string> = {
  pointed: "where you pointed",
  downloaded: "at its address, downloaded",
  beside: "beside the file its xsi:schemaLocation names",
  catalog: "by the catalog",
  name: "by its name",
  namespace: "by its namespace, in this repository",
  bundled: "among the schemas that come with knobas",
};

/** How a set open elsewhere was found, in the rail's words after *found*: an example's own (E1). */
const MADE_FROM = "where the example was made";

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");
const baseOf = (p: string) => p.slice(p.lastIndexOf("/") + 1).replace(/^@(repo\/[^/]+|download|knobas)\//, "");
const dirOf = (p: string) => (p.lastIndexOf("/") === -1 ? "" : p.slice(0, p.lastIndexOf("/") + 1));

/**
 * What a document's root says of its schema, read from its first tag in the text at hand: the check starts beside the
 * outline instead of after it. The same answer the outline gives (`DocOpened.root`), from the tag alone.
 */
export function rootSays(text: string): RootSays | null {
  const tag = firstTag(text);
  if (tag === null) return null;
  const name = /^<([^\s/>]+)/.exec(tag)?.[1];
  if (name === undefined) return null;
  const attrs = new Map<string, string>();
  const ATTR = /([^\s=<>/]+)\s*=\s*(?:"([^"]*)"|'([^']*)')/g;
  for (let m = ATTR.exec(tag); m !== null; m = ATTR.exec(tag)) attrs.set(m[1]!, (m[2] ?? m[3] ?? "").replace(/&amp;/g, "&").replace(/&quot;/g, '"').replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&apos;/g, "'"));
  const prefixes: Record<string, string> = {};
  for (const [k, v] of attrs) if (k.startsWith("xmlns:")) prefixes[k.slice(6)] = v;
  const colon = name.indexOf(":");
  const namespace = colon === -1 ? (attrs.get("xmlns") ?? "") : (prefixes[name.slice(0, colon)] ?? "");
  const xsi = Object.entries(prefixes).find(([, uri]) => uri === XSI)?.[0];
  let location: string | null = null;
  if (xsi !== undefined) {
    const none = attrs.get(`${xsi}:noNamespaceSchemaLocation`)?.trim();
    if (none !== undefined && none !== "") location = none;
    const pairs = (attrs.get(`${xsi}:schemaLocation`) ?? "").trim().split(/\s+/).filter(Boolean);
    for (let k = 0; location === null && k + 1 < pairs.length; k += 2) if (pairs[k] === namespace) location = pairs[k + 1]!;
    if (location === null && pairs.length >= 2) location = pairs[1]!;
  }
  return { name: colon === -1 ? name : name.slice(colon + 1), namespace, location, prefixes };
}

/**
 * The schema a document asks for, written as an entry the explorer's set reads: an import of its namespace from the
 * document's own folder, or an include of its no-namespace location. `null` where it says nothing of a schema. Named
 * by its folder alone, so two documents of one folder that say the same of their schema ask for one set, and the
 * second takes the set the first kept (`xsd-kept.ts`).
 */
export function entryFor(path: string, root: RootSays): { path: string; text: string } | null {
  if (root.namespace === "" && root.location === null) return null;
  const at = `${dirOf(path)}.knobas-document-schema.xsd`;
  const loc = root.location === null ? "" : ` schemaLocation="${esc(root.location)}"`;
  const body = root.namespace === "" ? `<xs:include${loc}/>` : `<xs:import namespace="${esc(root.namespace)}"${loc}/>`;
  // The document's own prefixes stand first, so a name in a finding is written as the document writes it; in one order,
  // so the root read from the first tag and from the outline ask for one set.
  const binds = Object.entries(root.prefixes ?? {})
    .filter(([p]) => p !== "xs" && /^[A-Za-z_][\w.-]*$/.test(p))
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .map(([p, uri]) => ` xmlns:${p}="${esc(uri)}"`)
    .join("");
  return { path: at, text: `<xs:schema xmlns:xs="${XS}"${binds}>${body}</xs:schema>` };
}

/** Whether two readings of a root ask for the same check: the same location and the same entry. */
export function sameAsk(path: string, a: RootSays | null, b: RootSays | null): boolean {
  if (a === null || b === null) return a === b;
  return a.namespace === b.namespace && a.location === b.location && entryFor(path, a)?.text === entryFor(path, b)?.text;
}

/** The largest document checked right behind the open (it crosses to the worker twice where the set is not kept). */
export const ASK_BEHIND_CAP = 4 * 1024 * 1024;
/** How many asks seen whole are remembered: a hint, the worker's kept sets decide. */
const SEEN_WHOLE = 32;
/** The asks (pin, entry, files pointed to) checked whole before, the least recent first. */
const seenWhole = new Set<string>();
const hintOf = (keep: KeepAt, entry: { path: string; text: string }, pointed: readonly Pointed[]) => JSON.stringify([keep, entry.path, entry.text, pointed]);
/**
 * Whether a check is likely answered by a set the worker kept whole: the same ask (pin, entry, files pointed to) was
 * checked whole before. A hint, never a promise; only such a check starts beside the outline.
 */
export function likelyKept(path: string, root: RootSays | null, keep: KeepAt | null | undefined, pointed: readonly Pointed[]): boolean {
  const entry = root === null ? null : entryFor(path, root);
  return keep != null && entry !== null && seenWhole.has(hintOf(keep, entry, pointed));
}
/** How many documents' departure lines are kept. */
const LINES_KEPT = 8;
/**
 * The lines the document worker put the last departures on, by the document's text and their paths: the same text
 * reads to the same outline, so a document checked again places its departures without asking (its verdict is drawn
 * in the outline's frame). The least recent first.
 */
const linesKept: { text: string; paths: string; lines: readonly number[] }[] = [];
function keptLines(text: string, paths: string): readonly number[] | null {
  const at = linesKept.findIndex((k) => k.paths === paths && k.text === text);
  if (at === -1) return null;
  const [k] = linesKept.splice(at, 1);
  linesKept.push(k!);
  return k!.lines;
}
function keepLines(text: string, paths: string, lines: readonly number[]): void {
  if (keptLines(text, paths) !== null) return;
  linesKept.push({ text, paths, lines });
  if (linesKept.length > LINES_KEPT) linesKept.shift();
}
/** The departures' lines, from the document's worker or as it answered them for the same text before. */
async function placesOf(doc: DocSession, text: string, findings: readonly Departure[]): Promise<readonly number[]> {
  if (findings.length === 0) return [];
  const xpaths = findings.map((f) => f.xpath);
  const paths = JSON.stringify(xpaths);
  const kept = keptLines(text, paths);
  if (kept !== null) return kept;
  const lines = await doc.places(xpaths);
  keepLines(text, paths, lines);
  return lines;
}

function sawWhole(hint: string): void {
  seenWhole.delete(hint);
  seenWhole.add(hint);
  if (seenWhole.size > SEEN_WHOLE) seenWhole.delete(seenWhole.values().next().value!);
}

export class DocCheck {
  readonly key: string;
  head = $state.raw<Head>({ k: "idle" });
  /** The departures, by line, in the document's order. */
  found = $state.raw<readonly Placed[]>([]);
  /** Each line's departures. */
  readonly byLine = $derived.by(() => {
    const out = new Map<number, Placed[]>();
    for (const p of this.found) {
      const list = out.get(p.line);
      if (list === undefined) out.set(p.line, [p]);
      else list.push(p);
    }
    return out;
  });
  /** The schema's file, where one was found: *Open the schema*. */
  schema = $state<string | null>(null);
  readonly #engine: XsdEngine;
  /** Where the paths a set read are kept across restarts (`xsd-hint.ts`); `null` keeps none. */
  readonly #hints: KeptValue | null;
  #port: XsdPort | null = null;
  /** A set open elsewhere the document was checked against ({@link runOn}): read, never closed, here. */
  #borrowed: XsdPort | null = null;
  #run = 0;

  constructor(key: string, engine: XsdEngine | null = null, more: { hints?: KeptValue | null } = {}) {
    this.key = key;
    this.#engine = engine ?? xsdEngine();
    this.#hints = more.hints === undefined ? hintStore() : more.hints;
  }

  /** Whether this file's check was stopped. */
  get stopped(): boolean {
    return stopped.has(this.key);
  }

  /**
   * Looks for the document's schema and checks the document against it. `landed` settles when the outline has landed,
   * true where it is well formed: no file is read and nothing is drawn before it.
   */
  async run(path: string, root: RootSays | null, text: string, doc: DocSession, reads: XsdReads, on: boolean, landed: Promise<boolean> = Promise.resolve(true)): Promise<void> {
    const run = ++this.#run;
    this.#port?.close();
    this.#port = null;
    this.#borrowed = null;
    this.found = [];
    this.schema = null;
    /** The outline stands well formed, and this run is still the one: what it found may be drawn. */
    const drawn = async () => (await landed) && run === this.#run;
    const entry = root === null ? null : entryFor(path, root);
    if (entry === null) {
      if (await drawn()) this.head = { k: "no-schema", namespace: null };
      return;
    }
    if (!on || this.stopped) {
      if (await drawn()) this.head = { k: "off" };
      return;
    }
    const port = new XsdPort(this.#engine);
    this.#port = port;
    // An ask seen whole before is likely kept whole: its check is asked right behind the open, so the worker runs it
    // while the page draws. It counts only where the open answers the set whole, with nothing to ask again.
    const hint = reads.keep != null ? hintOf(reads.keep, entry, reads.pointed) : null;
    const fits = text.length <= ASK_BEHIND_CAP;
    const likely = hint !== null && seenWhole.has(hint) && fits;
    // Any other crosses the document with its open, once: the round that leaves the set whole answers the verdict too.
    // Its reads wait for the outline, as they did, and take the paths the set read last time in their first round
    // (`xsd-hint.ts`). Started beside the outline, they were no sooner once the view was warmed (2026-11-25).
    const hintAt = this.#hints === null ? null : hintKey(this.key.slice(0, this.key.indexOf("\n")), entry.path, entry.text);
    const hinted = likely || hintAt === null ? [] : hintedPaths(this.#hints!, hintAt);
    let t0 = performance.now();
    void landed.then(() => (t0 = performance.now()));
    // Lean: the check draws no declaration, so none crosses back to the page.
    const opening = port.open(entry.path, entry.text, reads, root!.namespace, true, landed, { ...(!likely && fits ? { xml: text } : {}), hint: hinted });
    const asked = likely ? port.check(text) : null;
    const answer = await opening;
    if (run !== this.#run) return;
    // A set kept whole is checked beside the outline, and its departures' lines asked for: the document worker answers
    // them right behind the outline, which it reads first. A document with none needs no line: its verdict is drawn in
    // the outline's frame.
    const early: Promise<Checked> | null = !answer.ok || answer.need.length > 0 ? null : answer.checked !== undefined ? Promise.resolve(answer.checked) : (asked ?? port.check(text));
    const earlyLines =
      early === null
        ? null
        : early.then((c) => (c.state !== "checked" || run !== this.#run ? null : placesOf(doc, text, c.findings))).catch(() => null);
    if (!(await drawn())) return;
    this.head = { k: "looking", found: null, how: null };
    if (!answer.ok) {
      this.head = { k: "not-checked", warn: true, lines: [answer.error], against: null };
      return;
    }
    await port.whole;
    if (run !== this.#run) return;
    const index = port.index!;
    const readMs = performance.now() - t0;
    const lost = index.lost[0];
    const file = index.files.find((f) => f.depth === 1) ?? null;
    if (file === null || file.bad !== null) {
      if (lost !== undefined && lost.two !== null) this.head = { k: "two", namespace: lost.namespace ?? "" };
      else if (lost !== undefined && lost.address !== null) this.head = { k: "address", address: lost.address, namespace: lost.namespace };
      else if (root!.location !== null && !isRemote(root!.location)) this.head = { k: "not-found", location: root!.location };
      else this.head = { k: "no-schema", namespace: root!.namespace === "" ? null : root!.namespace };
      return;
    }
    this.schema = file.path;
    const against = file.url !== null ? baseOf(file.url) : baseOf(file.path);
    const how = file.from !== null ? `in ${file.from.name}, at ${file.from.branch}` : (FOUND[file.found ?? ""] ?? "");
    this.head = { k: "looking", found: against, how };
    // The round that left the set whole answered the verdict where the document crossed with the open.
    const checked: Checked = early !== null ? await early : (port.checked ?? (await port.check(text)));
    if (run !== this.#run) return;
    const placed = await this.#place(run, checked, doc, { against, how, readMs, files: index.files.length - 1 }, text, earlyLines);
    if (hint !== null && placed) {
      sawWhole(hint);
      // Read at a pin and checked whole: the files it read are the next first open's hint.
      if (hintAt !== null) keepHint(this.#hints!, hintAt, index.files.filter((f) => f.depth > 0 && f.found !== "bundled").map((f) => f.path));
    }
  }

  /**
   * A verdict made elsewhere (the example column's *Check*, step E1): its departures placed on their lines by their
   * paths, as {@link run} places its own. `"checking"` and `null` hold no marks. With `set`, the set it was made with:
   * the head names it, and the declaration beside a line is read from it.
   */
  async take(said: Checked | "checking" | null, doc: DocSession, set: { port: XsdPort; name: string; path: string } | null = null): Promise<void> {
    const run = ++this.#run;
    this.#borrow(set);
    const against = set?.name ?? "";
    if (said === null || said === "checking") {
      this.found = [];
      this.head = said === null ? { k: "idle" } : { k: "looking", found: against === "" ? null : against, how: MADE_FROM };
      return;
    }
    await this.#place(run, said, doc, { against, how: MADE_FROM, readMs: 0, files: (set?.port.index?.files.length ?? 1) - 1 });
  }

  /**
   * The document checked against a set open elsewhere (an example opened from its schema): no schema is looked for,
   * and the port stays its owner's.
   */
  async runOn(set: { port: XsdPort; name: string; path: string }, text: string, doc: DocSession): Promise<void> {
    const run = ++this.#run;
    this.#borrow(set);
    this.found = [];
    this.head = { k: "looking", found: set.name, how: MADE_FROM };
    const checked = await set.port.check(text);
    if (run !== this.#run) return;
    await this.#place(run, checked, doc, { against: set.name, how: MADE_FROM, readMs: 0, files: (set.port.index?.files.length ?? 1) - 1 });
  }

  #borrow(set: { port: XsdPort; path: string } | null): void {
    if (set === null) return;
    this.#port?.close();
    this.#port = null;
    this.#borrowed = set.port;
    this.schema = set.path;
  }

  /**
   * A verdict's departures put on their lines, and the head said. With `text`, the lines a document of the same text was
   * given are taken without asking (`placesOf`); `early`, lines asked beside the outline. Whether it was checked and
   * placed.
   */
  async #place(run: number, checked: Checked, doc: DocSession, head: { against: string; how: string; readMs: number; files: number }, text: string | null = null, early: Promise<readonly number[] | null> | null = null): Promise<boolean> {
    if (checked.state !== "checked") {
      const quiet = checked.why === "larger than knobas checks";
      this.found = [];
      this.head = { k: "not-checked", warn: !quiet, lines: checked.detail.length > 0 ? [...checked.detail] : [checked.why], against: head.against === "" ? null : head.against };
      return false;
    }
    const lines = (await early) ?? (text !== null ? await placesOf(doc, text, checked.findings) : await doc.places(checked.findings.map((f) => f.xpath)));
    if (run !== this.#run) return false;
    this.found = checked.findings
      .map((f: Departure, i) => ({ line: lines[i]! > 0 ? lines[i]! : f.line, words: f.words, raw: f.message.trim() }))
      .sort((a, b) => a.line - b.line);
    this.head = { k: "checked", count: checked.findings.length, total: checked.total, against: head.against, how: head.how, ms: checked.ms + checked.compileMs, readMs: head.readMs, files: head.files };
    return true;
  }

  /** The document is not well formed: nothing of a check stands, and the head is the outline's. */
  clear(): void {
    this.#run += 1;
    this.#port?.close();
    this.#port = null;
    this.found = [];
    this.head = { k: "idle" };
  }

  /** *Stop checking this file*: its marks and list go in one frame, until knobas quits. */
  stop(): void {
    stopped.add(this.key);
    this.#run += 1;
    this.found = [];
    this.head = { k: "off" };
  }

  /** *Check it*: the file is checked again from now on. */
  resume(): void {
    stopped.delete(this.key);
  }

  /** The declaration of the element on a line, from the schema's set (section 26). */
  async declarationOf(line: number, doc: DocSession): Promise<(PageAnswer & { found: number }) | null> {
    const port = this.port;
    if (port === null || this.head.k !== "checked") return null;
    const names = await doc.names(line);
    if (names.length === 0) return null;
    return port.declOf(names.map(({ ns, local }) => ({ ns, local })));
  }

  /** The session the schema's set is read in: the declaration beside the document opens its rows through it. */
  get port(): XsdPort | null {
    return this.#port ?? this.#borrowed;
  }

  /** The set's index, for *Open the schema*. */
  get index(): SetIndex | null {
    return this.port?.index ?? null;
  }

  close(): void {
    this.#run += 1;
    this.#port?.close();
    this.#port = null;
    this.#borrowed = null;
  }
}
