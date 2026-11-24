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
 * - **A departure is put on its line by libxml2's path to the node** (the document worker's `places`), never by its
 *   line, which is 16 bits.
 * - **Stop checking this file** holds until knobas quits.
 */
import type { DocSession } from "../../../render/doc-port";
import type { Checked, Departure, PageAnswer, SetIndex } from "../../../render/xsd-answer";
import type { Words } from "../../../render/xsd-findings";
import { isRemote } from "../../../render/xsd-locate";
import { XsdPort, xsdEngine, type XsdEngine, type XsdReads } from "../../../render/xsd-port";
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
 * document's own folder, or an include of its no-namespace location. `null` where it says nothing of a schema.
 */
export function entryFor(path: string, root: RootSays): { path: string; text: string } | null {
  if (root.namespace === "" && root.location === null) return null;
  const at = `${dirOf(path)}.knobas-schema-of-${baseOf(path)}.xsd`;
  const loc = root.location === null ? "" : ` schemaLocation="${esc(root.location)}"`;
  const body = root.namespace === "" ? `<xs:include${loc}/>` : `<xs:import namespace="${esc(root.namespace)}"${loc}/>`;
  // The document's own prefixes stand first, so a name in a finding is written as the document writes it.
  const binds = Object.entries(root.prefixes ?? {})
    .filter(([p]) => p !== "xs" && /^[A-Za-z_][\w.-]*$/.test(p))
    .map(([p, uri]) => ` xmlns:${p}="${esc(uri)}"`)
    .join("");
  return { path: at, text: `<xs:schema xmlns:xs="${XS}"${binds}>${body}</xs:schema>` };
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
  #port: XsdPort | null = null;
  #run = 0;

  constructor(key: string, engine: XsdEngine | null = null) {
    this.key = key;
    this.#engine = engine ?? xsdEngine();
  }

  /** Whether this file's check was stopped. */
  get stopped(): boolean {
    return stopped.has(this.key);
  }

  /** Looks for the document's schema and checks the document against it. */
  async run(path: string, root: RootSays | null, text: string, doc: DocSession, reads: XsdReads, on: boolean): Promise<void> {
    const run = ++this.#run;
    this.#port?.close();
    this.#port = null;
    this.found = [];
    this.schema = null;
    const entry = root === null ? null : entryFor(path, root);
    if (entry === null) {
      this.head = { k: "no-schema", namespace: null };
      return;
    }
    if (!on || this.stopped) {
      this.head = { k: "off" };
      return;
    }
    this.head = { k: "looking", found: null, how: null };
    const port = new XsdPort(this.#engine);
    this.#port = port;
    // Lean: the check draws no declaration, so none crosses back to the page.
    const answer = await port.open(entry.path, entry.text, reads, root!.namespace, true);
    if (run !== this.#run) return;
    if (!answer.ok) {
      this.head = { k: "not-checked", warn: true, lines: [answer.error], against: null };
      return;
    }
    const t0 = performance.now();
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
    const checked: Checked = await port.check(text);
    if (run !== this.#run) return;
    if (checked.state !== "checked") {
      const quiet = checked.why === "larger than knobas checks";
      this.head = { k: "not-checked", warn: !quiet, lines: checked.detail.length > 0 ? [...checked.detail] : [checked.why], against };
      return;
    }
    const lines = await doc.places(checked.findings.map((f) => f.xpath));
    if (run !== this.#run) return;
    this.found = checked.findings
      .map((f: Departure, i) => ({ line: lines[i]! > 0 ? lines[i]! : f.line, words: f.words, raw: f.message.trim() }))
      .sort((a, b) => a.line - b.line);
    this.head = { k: "checked", count: checked.findings.length, total: checked.total, against, how, ms: checked.ms + checked.compileMs, readMs, files: index.files.length - 1 };
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
    const port = this.#port;
    if (port === null || this.head.k !== "checked") return null;
    const names = await doc.names(line);
    if (names.length === 0) return null;
    return port.declOf(names.map(({ ns, local }) => ({ ns, local })));
  }

  /** The session the schema's set is read in: the declaration beside the document opens its rows through it. */
  get port(): XsdPort | null {
    return this.#port;
  }

  /** The set's index, for *Open the schema*. */
  get index(): SetIndex | null {
    return this.#port?.index ?? null;
  }

  close(): void {
    this.#run += 1;
    this.#port?.close();
    this.#port = null;
  }
}
