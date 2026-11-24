/**
 * A schema set: one XML Schema file and every file it reaches, read as one model (the XSD design, 2026-11-23,
 * sections 6, 10, 11, 12 and 18). It runs in the schema worker (`xsd.worker.ts`) and, in tests, on the test's thread.
 *
 * - **Six kinds of declaration** under their namespace and local name: element, type (complex or simple, one symbol
 *   space), group, attribute group, attribute. Each knows its file and line.
 * - **A name inside a declaration stays a name** (`{ns}local`) until a page or an example needs what it stands for.
 * - **Read as far as the texts at hand go.** The set is loaded from the texts the worker holds; a file it reaches and
 *   does not hold yet is a `need`, read by the port, and the set is loaded again with it (each file's tree is kept by
 *   its text, so a second load costs the compile alone).
 * - **Bounds**: 5 MiB a file, 400 files and 64 MiB a set. A set that meets one says which and keeps what it has.
 */
import { locate, type Elsewhere, type How, type Located, type LocateIn, type Pointed, type RefKind, type SchemaIndex } from "./xsd-locate";
import { sniffNamespace } from "./xsd-sniff";
import { NotRead, XS, XML_NS, attr, kid, kids, localOf, nsOf, qkey, readSchemaXml, type SchemaTree, type XNode } from "./xsd-xml";

export const FILE_CAP = 5 * 1024 * 1024;
export const FILES_CAP = 400;
export const SET_CAP = 64 * 1024 * 1024;

// ---- The model's shapes ----

export interface Docu {
  readonly lang: string | null;
  readonly text: string;
  /** A `documentation` that holds elements (UBL's `ccts:`): its leaves as named facts. */
  readonly facts: readonly (readonly [string, string])[];
  readonly source: string | null;
}

export interface Facets {
  enumeration?: { v: string; docu: Docu[] }[];
  pattern?: string[];
  length?: string;
  minLength?: string;
  maxLength?: string;
  totalDigits?: string;
  fractionDigits?: string;
  minInclusive?: string;
  maxInclusive?: string;
  minExclusive?: string;
  maxExclusive?: string;
  whiteSpace?: string;
  explicitTimezone?: string;
}
const FACETS = ["length", "minLength", "maxLength", "totalDigits", "fractionDigits", "minInclusive", "maxInclusive", "minExclusive", "maxExclusive", "whiteSpace", "explicitTimezone"] as const;
type Scalar = (typeof FACETS)[number];

/** What every declaration carries once it is in the set. */
interface Placed {
  key: string;
  ns: string;
  file: string;
  /** A redefine or an override replaced it: `"redefine"`, `"override"`. */
  over?: string;
  original?: Decl;
  appinfo?: number;
}

export interface SimpleDecl extends Partial<Placed> {
  kind: "st";
  name: string | null;
  line: number;
  docu: Docu[];
  variety: "atomic" | "list" | "union";
  base: string | null;
  anonBase: SimpleDecl | null;
  facets: Facets;
  item: string | SimpleDecl | null;
  members: (string | SimpleDecl)[];
  asserts: string[];
  builtin?: boolean;
  prim?: string;
  lim?: { min?: number; max?: number } | null;
}

export type AttrUse =
  | { ag: string; line: number }
  | { any: string; pc: string; line: number }
  | AttrDecl;

export interface AttrDecl extends Partial<Placed> {
  kind: "at";
  name: string;
  /** The namespace it stands in, as an instance writes it: `""` for an unqualified local attribute. */
  attrNs: string;
  ref: string | null;
  type: string | null;
  anon: SimpleDecl | null;
  use: string;
  def: string | null;
  fixed: string | null;
  docu: Docu[];
  line: number;
}

export type Particle =
  | { k: "el"; el: ElementDecl; min: number; max: number }
  | { k: "any"; ns: string; pc: string; min: number; max: number; line: number }
  | { k: "grp"; ref: string; min: number; max: number; line: number }
  | ModelGroup
  | { k: "missing"; ref: string; min: number; max: number }
  | { k: "loop"; ref: string; min: number; max: number };

export interface ModelGroup {
  k: "seq" | "choice" | "all";
  min: number;
  max: number;
  items: Particle[];
  line: number;
  docu: Docu[];
  /** An extension's base and own content, joined: drawn as the rows of both. */
  joined?: boolean;
  /** A named group opened here. */
  from?: string;
  name?: string;
}

export interface ComplexDecl extends Partial<Placed> {
  kind: "ct";
  name: string | null;
  line: number;
  docu: Docu[];
  abstract: boolean;
  mixed: boolean;
  deriv: "ext" | "res" | null;
  base: string | null;
  simple: boolean;
  facets: Facets;
  content: Particle | null;
  attrs: AttrUse[];
  asserts: string[];
  open: string | null;
  anonBase: SimpleDecl | null;
}

export interface ElementDecl extends Partial<Placed> {
  kind: "el";
  name: string;
  /** The namespace it stands in, as an instance writes it: `""` for an unqualified local element. */
  elNs: string;
  ref: string | null;
  type: string | null;
  anon: ComplexDecl | SimpleDecl | null;
  sub: string[];
  abstract: boolean;
  nillable: boolean;
  def: string | null;
  fixed: string | null;
  docu: Docu[];
  line: number;
  idc: { kind: string; name: string | null; refer: string | null }[];
  alts: { test: string | null; type: string | null }[];
  /** The file it is declared in, for a local element (a global one has it from `Placed`). */
  at?: string;
}

export interface GroupDecl extends Partial<Placed> {
  kind: "gr";
  name: string;
  line: number;
  docu: Docu[];
  content: Particle | null;
}

export interface AttrGroupDecl extends Partial<Placed> {
  kind: "ag";
  name: string;
  line: number;
  docu: Docu[];
  attrs: AttrUse[];
}

export type Decl = ElementDecl | ComplexDecl | SimpleDecl | GroupDecl | AttrGroupDecl | AttrDecl;
export type Space = "el" | "ty" | "gr" | "ag" | "at";

/** A reference from one file to another, with where it was found. */
export interface FileRef {
  readonly kind: RefKind;
  readonly namespace: string | null;
  readonly location: string | null;
  readonly line: number;
  readonly to: string | null;
  readonly how: How | null;
  readonly why: string | null;
  readonly address: string | null;
  readonly others: readonly string[];
  readonly via: string | null;
  readonly from: Elsewhere | null;
  readonly two: readonly Elsewhere[] | null;
  /** Its file is being read. */
  readonly reading: boolean;
}

export interface SetDoc {
  path: string;
  /** The namespace it declares, and the one it stands under (a chameleon include takes its includer's). */
  own: string;
  tns: string;
  adopted: string | null;
  prefixes: Record<string, string>;
  refs: FileRef[];
  head: string | null;
  counts: Partial<Record<Decl["kind"], number>>;
  lines: number;
  bytes: number;
  v11: boolean;
  version: string | null;
  efd: boolean;
  bad: string | null;
  badLine: number | null;
  by: string | null;
  /** Its text is being read. */
  reading: boolean;
  /** What the entities in it did not explain. */
  problems: { line: number; text: string }[];
}

export type Problem =
  | { kind: "twice"; key: string; first: string; second: string; line: number }
  | { kind: "cap"; what: "files" | "bytes" | "file"; path: string }
  | { kind: "loop"; key: string };

export interface Use {
  readonly by: string;
  readonly how: string;
}

/** What a set's load reads files through. */
export interface SetInput {
  readonly entry: string;
  /** The texts at hand, by path: `null` for a file that could not be read. */
  readonly texts: ReadonlyMap<string, string | null>;
  readonly paths: ReadonlySet<string> | null;
  readonly pointed: readonly Pointed[];
  readonly index: SchemaIndex | null;
  readonly elsewhere: ((namespace: string) => readonly Elsewhere[] | null) | null;
  readonly bundled: ReadonlyMap<string, string>;
}

export interface SchemaSet {
  readonly entry: string;
  readonly docs: SetDoc[];
  readonly comps: Map<string, Decl & Placed>;
  readonly problems: Problem[];
  uses: Map<string, Use[]>;
  members: Map<string, string[]>;
  derived: Map<string, string[]>;
  missing: Map<string, string[]>;
  roots: string[];
  prefixes: Map<string, string>;
  home: string;
  /** What the port must read before the set is whole: paths, `?index`, `?elsewhere:<ns>`. */
  need: string[];
  times: { read: number; compile: number };
}

// ---- Compiling one file's declarations ----

interface Ctx {
  tns: string;
  efd: boolean;
  afd: boolean;
  file: string;
  key: (node: XNode, qname: string) => string;
}

const occurs = (node: XNode): { min: number; max: number } => {
  const lo = attr(node, "minOccurs");
  const hi = attr(node, "maxOccurs");
  return { min: lo === null ? 1 : Number(lo), max: hi === null ? 1 : hi === "unbounded" ? Infinity : Number(hi) };
};

/** All the words under a node, in order. */
function wordsOf(n: XNode): string {
  return [n.t, ...n.c.map(wordsOf)].join(" ");
}

/** An annotation's words: each `documentation` its language and text; one that holds elements is read as named facts. */
export function docuOf(node: XNode): Docu[] {
  const out: Docu[] = [];
  for (const ann of kids(node, "annotation")) {
    for (const d of ann.c) {
      if (d.ns !== XS || d.l !== "documentation") continue;
      const lang = d.a.find((a) => a.l === "lang" && a.ns === XML_NS)?.v ?? null;
      if (d.c.length > 0 && !/\S/.test(d.t)) {
        const facts: [string, string][] = [];
        const walk = (list: readonly XNode[]) => {
          for (const c of list) {
            if (c.c.length > 0) walk(c.c);
            else if (/\S/.test(c.t)) facts.push([c.l, c.t.trim().replace(/\s+/g, " ")]);
          }
        };
        walk(d.c);
        const at = facts.findIndex((f) => /^(definition|description|documentation)$/i.test(f[0]));
        out.push({ lang, text: at === -1 ? "" : facts[at]![1], facts: facts.filter((_, i) => i !== at), source: attr(d, "source") });
      } else {
        const text = wordsOf(d)
          .trim()
          .replace(/[ \t]+\n/g, "\n")
          .replace(/\n{3,}/g, "\n\n")
          .replace(/[ \t]{2,}/g, " ");
        if (text !== "" || attr(d, "source") !== null) out.push({ lang, text, facts: [], source: attr(d, "source") });
      }
    }
  }
  return out;
}
const appinfoCount = (node: XNode) => kids(node, "annotation").reduce((s, a) => s + kids(a, "appinfo").length, 0);

function readRestriction(res: XNode, into: { base: string | null; anonBase: SimpleDecl | null; facets: Facets; asserts: string[] }, ctx: Ctx): void {
  const b = attr(res, "base");
  if (b !== null) into.base = ctx.key(res, b);
  else if (kid(res, "simpleType") !== null) into.anonBase = compileSimple(kid(res, "simpleType")!, ctx);
  for (const f of res.c) {
    if (f.ns !== XS) continue;
    if (f.l === "enumeration") (into.facets.enumeration ??= []).push({ v: attr(f, "value") ?? "", docu: docuOf(f) });
    else if (f.l === "pattern") (into.facets.pattern ??= []).push(attr(f, "value") ?? "");
    else if (f.l === "assertion") into.asserts.push(attr(f, "test") ?? "");
    else if ((FACETS as readonly string[]).includes(f.l)) into.facets[f.l as Scalar] = attr(f, "value") ?? "";
  }
}

export function compileSimple(node: XNode, ctx: Ctx): SimpleDecl {
  const st: SimpleDecl = { kind: "st", name: attr(node, "name"), line: node.line, docu: docuOf(node), variety: "atomic", base: null, anonBase: null, facets: {}, item: null, members: [], asserts: [] };
  const res = kid(node, "restriction");
  const list = kid(node, "list");
  const union = kid(node, "union");
  if (res !== null) readRestriction(res, st, ctx);
  else if (list !== null) {
    st.variety = "list";
    const it = attr(list, "itemType");
    st.item = it !== null ? ctx.key(list, it) : kid(list, "simpleType") !== null ? compileSimple(kid(list, "simpleType")!, ctx) : xsKey("anySimpleType");
  } else if (union !== null) {
    st.variety = "union";
    const names = (attr(union, "memberTypes") ?? "").trim();
    st.members = [...(names === "" ? [] : names.split(/\s+/).map((q) => ctx.key(union, q))), ...kids(union, "simpleType").map((s) => compileSimple(s, ctx))];
  }
  return st;
}

function compileAttribute(node: XNode, ctx: Ctx, global: boolean): AttrDecl {
  const ref = attr(node, "ref");
  const form = attr(node, "form");
  const qualified = global || (form !== null ? form === "qualified" : ctx.afd);
  const t = attr(node, "type");
  const rk = ref !== null ? ctx.key(node, ref) : null;
  return {
    kind: "at",
    name: rk !== null ? localOf(rk) : (attr(node, "name") ?? ""),
    attrNs: rk !== null ? nsOf(rk) : qualified ? ctx.tns : "",
    ref: rk,
    type: t !== null ? ctx.key(node, t) : null,
    anon: kid(node, "simpleType") !== null ? compileSimple(kid(node, "simpleType")!, ctx) : null,
    use: attr(node, "use") ?? "optional",
    def: attr(node, "default"),
    fixed: attr(node, "fixed"),
    docu: docuOf(node),
    line: node.line,
  };
}

function compileAttrs(node: XNode, ctx: Ctx): AttrUse[] {
  const out: AttrUse[] = [];
  for (const c of node.c) {
    if (c.ns !== XS) continue;
    if (c.l === "attribute") out.push(compileAttribute(c, ctx, false));
    else if (c.l === "attributeGroup" && attr(c, "ref") !== null) out.push({ ag: ctx.key(c, attr(c, "ref")!), line: c.line });
    else if (c.l === "anyAttribute") out.push({ any: attr(c, "namespace") ?? "##any", pc: attr(c, "processContents") ?? "strict", line: c.line });
  }
  return out;
}

const PARTICLES = ["element", "any", "group", "sequence", "choice", "all"];

function compileParticle(node: XNode, ctx: Ctx): Particle {
  const o = occurs(node);
  if (node.l === "element") return { k: "el", el: compileElement(node, ctx, false), min: o.min, max: o.max };
  if (node.l === "any") return { k: "any", ns: attr(node, "namespace") ?? "##any", pc: attr(node, "processContents") ?? "strict", min: o.min, max: o.max, line: node.line };
  if (node.l === "group") return { k: "grp", ref: ctx.key(node, attr(node, "ref") ?? ""), min: o.min, max: o.max, line: node.line };
  const k = node.l === "sequence" ? "seq" : node.l === "choice" ? "choice" : "all";
  const items: Particle[] = [];
  for (const c of node.c) if (c.ns === XS && PARTICLES.includes(c.l)) items.push(compileParticle(c, ctx));
  return { k, min: o.min, max: o.max, items, line: node.line, docu: docuOf(node) };
}
function groupOf(node: XNode, ctx: Ctx): Particle | null {
  for (const c of node.c) if (c.ns === XS && ["group", "sequence", "choice", "all"].includes(c.l)) return compileParticle(c, ctx);
  return null;
}

function compileComplex(node: XNode, ctx: Ctx): ComplexDecl {
  const ct: ComplexDecl = {
    kind: "ct",
    name: attr(node, "name"),
    line: node.line,
    docu: docuOf(node),
    abstract: attr(node, "abstract") === "true",
    mixed: attr(node, "mixed") === "true",
    deriv: null,
    base: null,
    simple: false,
    facets: {},
    content: null,
    attrs: [],
    asserts: [],
    open: null,
    anonBase: null,
  };
  const sc = kid(node, "simpleContent");
  const cc = kid(node, "complexContent");
  let holder = node;
  if (sc !== null || cc !== null) {
    const inner = (sc ?? cc)!;
    if (cc !== null && attr(cc, "mixed") !== null) ct.mixed = attr(cc, "mixed") === "true";
    const ext = kid(inner, "extension");
    const res = kid(inner, "restriction");
    holder = ext ?? res ?? inner;
    ct.simple = sc !== null;
    ct.deriv = ext !== null ? "ext" : "res";
    if (attr(holder, "base") !== null) ct.base = ctx.key(holder, attr(holder, "base")!);
    if (sc !== null && res !== null) readRestriction(res, ct, ctx);
  }
  if (!ct.simple) ct.content = groupOf(holder, ctx);
  ct.attrs = compileAttrs(holder, ctx);
  for (const a of kids(holder, "assert")) ct.asserts.push(attr(a, "test") ?? "");
  if (kid(holder, "openContent") !== null) ct.open = attr(kid(holder, "openContent")!, "mode") ?? "interleave";
  return ct;
}

function compileElement(node: XNode, ctx: Ctx, global: boolean): ElementDecl {
  const ref = attr(node, "ref");
  const form = attr(node, "form");
  const qualified = global || (form !== null ? form === "qualified" : ctx.efd);
  const t = attr(node, "type");
  const ct = kid(node, "complexType");
  const st = kid(node, "simpleType");
  const sub = attr(node, "substitutionGroup");
  const rk = ref !== null ? ctx.key(node, ref) : null;
  return {
    kind: "el",
    name: rk !== null ? localOf(rk) : (attr(node, "name") ?? ""),
    elNs: rk !== null ? nsOf(rk) : qualified ? ctx.tns : "",
    ref: rk,
    type: t !== null ? ctx.key(node, t) : null,
    anon: ct !== null ? compileComplex(ct, ctx) : st !== null ? compileSimple(st, ctx) : null,
    sub: sub !== null ? sub.trim().split(/\s+/).map((q) => ctx.key(node, q)) : [],
    abstract: attr(node, "abstract") === "true",
    nillable: attr(node, "nillable") === "true",
    def: attr(node, "default"),
    fixed: attr(node, "fixed"),
    docu: docuOf(node),
    line: node.line,
    idc: node.c.filter((c) => c.ns === XS && ["key", "keyref", "unique"].includes(c.l)).map((c) => ({ kind: c.l, name: attr(c, "name"), refer: attr(c, "refer") })),
    alts: kids(node, "alternative").map((a) => ({ test: attr(a, "test"), type: attr(a, "type") !== null ? ctx.key(a, attr(a, "type")!) : null })),
    at: ctx.file,
  };
}

const SYMBOL: Record<string, Space> = { element: "el", complexType: "ty", simpleType: "ty", group: "gr", attributeGroup: "ag", attribute: "at" };
export const compKey = (space: Space, ns: string, name: string): string => `${space} {${ns}}${name}`;

// ---- Loading a set ----

/** Each file's tree, kept by its text, so a set loaded again with one more file read parses that file alone. */
const trees = new Map<string, { text: string; tree: SchemaTree | NotRead }>();
const TREES_KEPT = 600;
function treeFor(path: string, text: string): SchemaTree | NotRead {
  const had = trees.get(path);
  if (had !== undefined && had.text === text) return had.tree;
  let tree: SchemaTree | NotRead;
  try {
    tree = readSchemaXml(text);
  } catch (e) {
    tree = e instanceof NotRead ? e : new NotRead(e instanceof Error ? e.message : String(e), null);
  }
  if (trees.size >= TREES_KEPT) trees.delete(trees.keys().next().value!);
  trees.set(path, { text, tree });
  return tree;
}

const now = () => (typeof performance !== "undefined" ? performance.now() : Date.now());

/** Loads the set that starts at `inn.entry` from the texts at hand. What it still needs is `set.need`. */
export function loadSet(inn: SetInput): SchemaSet {
  const set: SchemaSet = {
    entry: inn.entry,
    docs: [],
    comps: new Map(),
    problems: [],
    uses: new Map(),
    members: new Map(),
    derived: new Map(),
    missing: new Map(),
    roots: [],
    prefixes: new Map(),
    home: "",
    need: [],
    times: { read: 0, compile: 0 },
  };
  const need = new Set<string>();
  const loaded = new Map<string, SetDoc>();
  const queue: { path: string; adopt: string | null; by: string | null }[] = [{ path: inn.entry, adopt: null, by: null }];
  let bytes = 0;
  const locIn: LocateIn = {
    paths: inn.paths,
    declares: (path) => {
      // A file the index sniffed answers from it; one read for the set, from its text.
      if (!inn.texts.has(path)) return inn.index !== null && inn.index.declares.has(path) ? inn.index.declares.get(path) : null;
      const t = inn.texts.get(path);
      return t === null || t === undefined ? undefined : sniffNamespace(t);
    },
    gone: (path) => inn.texts.has(path) && inn.texts.get(path) === null,
    pointed: inn.pointed,
    index: inn.index,
    elsewhere: inn.elsewhere,
    bundled: inn.bundled,
  };

  const register = (space: Space, ns: string, name: string, comp: Decl, doc: SetDoc, over: string | null) => {
    const key = compKey(space, ns, name);
    const c = comp as Decl & Placed;
    c.key = key;
    c.ns = ns;
    c.file = doc.path;
    const had = set.comps.get(key);
    if (had === undefined) {
      set.comps.set(key, c);
      if (over !== null) c.over = over;
      return;
    }
    if (had.over !== undefined && had.original === undefined && over === null) {
      had.original = c;
      return;
    }
    if (over !== null) {
      c.over = over;
      c.original = had;
      set.comps.set(key, c);
      return;
    }
    if (had.file !== doc.path || had.line !== c.line) set.problems.push({ kind: "twice", key, first: had.file, second: doc.path, line: c.line });
  };

  const declare = (node: XNode, ctx: Ctx, doc: SetDoc, over: string | null) => {
    const space = SYMBOL[node.l];
    if (space === undefined) return;
    const name = attr(node, "name");
    if (name === null) return;
    let comp: Decl;
    if (node.l === "element") comp = compileElement(node, ctx, true);
    else if (node.l === "complexType") comp = compileComplex(node, ctx);
    else if (node.l === "simpleType") comp = compileSimple(node, ctx);
    else if (node.l === "group") comp = { kind: "gr", name, line: node.line, docu: docuOf(node), content: groupOf(node, ctx) };
    else if (node.l === "attributeGroup") comp = { kind: "ag", name, line: node.line, docu: docuOf(node), attrs: compileAttrs(node, ctx) };
    else comp = compileAttribute(node, ctx, true);
    comp.name = name;
    comp.appinfo = appinfoCount(node);
    doc.counts[comp.kind] = (doc.counts[comp.kind] ?? 0) + 1;
    register(space, ctx.tns, name, comp, doc, over);
  };

  while (queue.length > 0) {
    const job = queue.shift()!;
    const id = `${job.path}\n${job.adopt ?? ""}`;
    if (loaded.has(id)) continue;
    if (loaded.size >= FILES_CAP) {
      set.problems.push({ kind: "cap", what: "files", path: job.path });
      continue;
    }
    const doc: SetDoc = {
      path: job.path,
      own: "",
      tns: "",
      adopted: null,
      prefixes: {},
      refs: [],
      head: null,
      counts: {},
      lines: 0,
      bytes: 0,
      v11: false,
      version: null,
      efd: false,
      bad: null,
      badLine: null,
      by: job.by,
      reading: false,
      problems: [],
    };
    loaded.set(id, doc);
    set.docs.push(doc);
    if (!inn.texts.has(job.path)) {
      doc.reading = true;
      need.add(job.path);
      continue;
    }
    const text = inn.texts.get(job.path)!;
    if (text === null) {
      doc.bad = "The file could not be read.";
      continue;
    }
    if (text.length > FILE_CAP) {
      doc.bad = "It is larger than knobas reads (5 MiB).";
      set.problems.push({ kind: "cap", what: "file", path: job.path });
      continue;
    }
    if (bytes + text.length > SET_CAP) {
      doc.bad = "The set is larger than knobas reads (64 MiB).";
      set.problems.push({ kind: "cap", what: "bytes", path: job.path });
      continue;
    }
    bytes += text.length;
    doc.bytes = text.length;
    const t0 = now();
    const tree = treeFor(job.path, text);
    set.times.read += now() - t0;
    if (tree instanceof NotRead) {
      doc.bad = tree.message;
      doc.badLine = tree.line;
      continue;
    }
    const root = tree.root;
    if (root.ns !== XS || root.l !== "schema") {
      doc.bad = "Its first element is not an XML Schema's.";
      continue;
    }
    const t1 = now();
    doc.lines = tree.lines;
    doc.head = tree.head;
    doc.problems = [...tree.problems];
    doc.own = attr(root, "targetNamespace") ?? "";
    doc.adopted = doc.own === "" && job.adopt ? job.adopt : null;
    doc.tns = doc.adopted ?? doc.own;
    doc.version = attr(root, "version");
    for (const p in root.nsmap) if (p !== "xml") doc.prefixes[p] = root.nsmap[p]!;
    doc.v11 = root.a.some((a) => a.l === "minVersion" && a.v === "1.1") || attr(root, "defaultAttributes") !== null || attr(root, "xpathDefaultNamespace") !== null;
    const ctx: Ctx = {
      tns: doc.tns,
      efd: attr(root, "elementFormDefault") === "qualified",
      afd: attr(root, "attributeFormDefault") === "qualified",
      file: doc.path,
      key: (node, qname) => qkey(node, qname, doc.adopted),
    };
    doc.efd = ctx.efd;
    for (const c of root.c) {
      if (c.ns !== XS) continue;
      if (c.l === "import" || c.l === "include" || c.l === "redefine" || c.l === "override") {
        const kind = c.l as RefKind;
        const namespace = kind === "import" ? attr(c, "namespace") : null;
        const location = attr(c, "schemaLocation");
        if (kind === "override") doc.v11 = true;
        const found: Located = locate(locIn, doc.path, doc.tns, kind, namespace, location);
        if ("need" in found) for (const n of found.need) need.add(n);
        const ref: FileRef =
          "need" in found
            ? { kind, namespace, location, line: c.line, to: null, how: null, why: null, address: null, others: [], via: null, from: null, two: null, reading: true }
            : found.to === null
              ? { kind, namespace, location, line: c.line, to: null, how: null, why: found.why, address: found.address, others: [], via: null, from: null, two: found.two ?? null, reading: false }
              : { kind, namespace, location, line: c.line, to: found.to, how: found.how, why: null, address: null, others: found.others, via: found.via ?? null, from: found.from ?? null, two: null, reading: false };
        doc.refs.push(ref);
        if (ref.to !== null && ref.how !== "builtin") queue.push({ path: ref.to, adopt: kind === "import" ? null : doc.tns, by: doc.path });
        if (kind === "redefine" || kind === "override") for (const r of c.c) if (r.ns === XS) declare(r, ctx, doc, kind);
      } else declare(c, ctx, doc, null);
      if (c.l === "defaultOpenContent") doc.v11 = true;
    }
    set.times.compile += now() - t1;
  }
  set.need = [...need];
  finish(set);
  return set;
}

// ---- Reading the model ----

/** The built-in simple types (Part 2, section 3): each its base, its primitive and its bounds. */
export const BUILTIN: Record<string, readonly [string | null, string, { min?: number; max?: number }?]> = {
  anyType: [null, "any"],
  anySimpleType: ["anyType", "string"],
  anyAtomicType: ["anySimpleType", "string"],
  string: ["anySimpleType", "string"],
  normalizedString: ["string", "string"],
  token: ["normalizedString", "string"],
  language: ["token", "language"],
  NMTOKEN: ["token", "nmtoken"],
  NMTOKENS: ["anySimpleType", "nmtokens"],
  Name: ["token", "name"],
  NCName: ["Name", "ncname"],
  ID: ["NCName", "id"],
  IDREF: ["NCName", "idref"],
  IDREFS: ["anySimpleType", "idrefs"],
  ENTITY: ["NCName", "ncname"],
  ENTITIES: ["anySimpleType", "nmtokens"],
  boolean: ["anySimpleType", "boolean"],
  decimal: ["anySimpleType", "decimal"],
  integer: ["decimal", "integer"],
  nonPositiveInteger: ["integer", "integer", { max: 0 }],
  negativeInteger: ["nonPositiveInteger", "integer", { max: -1 }],
  long: ["integer", "integer", { min: -9223372036854775808, max: 9223372036854775807 }],
  int: ["long", "integer", { min: -2147483648, max: 2147483647 }],
  short: ["int", "integer", { min: -32768, max: 32767 }],
  byte: ["short", "integer", { min: -128, max: 127 }],
  nonNegativeInteger: ["integer", "integer", { min: 0 }],
  unsignedLong: ["nonNegativeInteger", "integer", { min: 0, max: 18446744073709551615 }],
  unsignedInt: ["unsignedLong", "integer", { min: 0, max: 4294967295 }],
  unsignedShort: ["unsignedInt", "integer", { min: 0, max: 65535 }],
  unsignedByte: ["unsignedShort", "integer", { min: 0, max: 255 }],
  positiveInteger: ["nonNegativeInteger", "integer", { min: 1 }],
  float: ["anySimpleType", "float"],
  double: ["anySimpleType", "float"],
  duration: ["anySimpleType", "duration"],
  yearMonthDuration: ["duration", "duration"],
  dayTimeDuration: ["duration", "duration"],
  dateTime: ["anySimpleType", "dateTime"],
  dateTimeStamp: ["dateTime", "dateTime"],
  time: ["anySimpleType", "time"],
  date: ["anySimpleType", "date"],
  gYearMonth: ["anySimpleType", "gYearMonth"],
  gYear: ["anySimpleType", "gYear"],
  gMonthDay: ["anySimpleType", "gMonthDay"],
  gDay: ["anySimpleType", "gDay"],
  gMonth: ["anySimpleType", "gMonth"],
  hexBinary: ["anySimpleType", "hexBinary"],
  base64Binary: ["anySimpleType", "base64Binary"],
  anyURI: ["anySimpleType", "anyURI"],
  QName: ["anySimpleType", "qname"],
  NOTATION: ["anySimpleType", "qname"],
};
export const xsKey = (name: string): string => `{${XS}}${name}`;
export const isBuiltin = (key: string): boolean => key.startsWith(`{${XS}}`) && BUILTIN[localOf(key)] !== undefined;

const BUILT = new Map<string, SimpleDecl & Placed>();
/** The definition a type key names: a declared type, a built-in, or `null` (not found here). */
export function typeDef(set: SchemaSet, key: string | null | undefined): ((ComplexDecl | SimpleDecl) & Partial<Placed>) | null {
  if (key === null || key === undefined) return null;
  const c = set.comps.get(`ty ${key}`);
  if (c !== undefined) return c as ComplexDecl | SimpleDecl;
  if (isBuiltin(key)) {
    let b = BUILT.get(key);
    if (b === undefined) {
      const [base, prim, lim] = BUILTIN[localOf(key)]!;
      b = {
        kind: "st",
        builtin: true,
        name: localOf(key),
        key: `ty ${key}`,
        ns: XS,
        file: "",
        line: 0,
        variety: prim === "nmtokens" || prim === "idrefs" ? "list" : "atomic",
        base: base === null ? null : xsKey(base),
        anonBase: null,
        prim,
        lim: lim ?? null,
        facets: {},
        docu: [],
        members: [],
        item: null,
        asserts: [],
      };
      BUILT.set(key, b);
    }
    return b;
  }
  return null;
}

/** The declaration an element particle stands for: a ref's global element, else itself. */
export const elDecl = (set: SchemaSet, el: ElementDecl): ElementDecl | null => (el.ref !== null ? ((set.comps.get(`el ${el.ref}`) as ElementDecl | undefined) ?? null) : el);

export interface ElType {
  def: ((ComplexDecl | SimpleDecl) & Partial<Placed>) | null;
  key: string | null;
  missing?: string | null;
}
/** An element's type: `def` null where its type was not found here. An element with none is `anyType`. */
export function elType(set: SchemaSet, el: ElementDecl, seen: Set<ElementDecl> = new Set()): ElType {
  const d = elDecl(set, el);
  if (d === null) return { def: null, key: null, missing: el.ref };
  if (d.anon !== null) return { def: d.anon, key: null };
  if (d.type !== null) {
    const def = typeDef(set, d.type);
    return { def, key: d.type, missing: def === null ? d.type : null };
  }
  seen.add(d);
  for (const head of d.sub) {
    const h = set.comps.get(`el ${head}`) as ElementDecl | undefined;
    if (h !== undefined && !seen.has(h)) return elType(set, h, seen);
  }
  return { def: typeDef(set, xsKey("anyType")), key: xsKey("anyType") };
}

export interface SimpleShape {
  prim: string;
  variety: "atomic" | "list" | "union";
  facets: Facets;
  patterns: string[][];
  chain: string[];
  item: ((ComplexDecl | SimpleDecl) & Partial<Placed>) | SimpleDecl | null;
  members: (SimpleDecl | ComplexDecl)[];
  lim: { min?: number; max?: number };
  missing: string | null;
  enumFrom: string | null;
  builtin?: string;
  loop?: boolean;
}

/** A simple type read down to its built-in: the nearest facet wins, every step's patterns all hold. */
export function simpleShape(set: SchemaSet, def: (ComplexDecl | SimpleDecl) | null): SimpleShape {
  const out: SimpleShape = { prim: "string", variety: "atomic", facets: {}, patterns: [], chain: [], item: null, members: [], lim: {}, missing: null, enumFrom: null };
  let d: (ComplexDecl | SimpleDecl) | null = def;
  const guard = new Set<object>();
  while (d !== null) {
    if (guard.has(d)) {
      out.loop = true;
      break;
    }
    guard.add(d);
    if (d.kind === "st" && d.builtin) {
      out.prim = d.prim ?? "string";
      if (d.variety === "list") {
        out.variety = "list";
        out.item = typeDef(set, xsKey(d.prim === "idrefs" ? "IDREF" : "NMTOKEN"));
      }
      for (let b: SimpleDecl | null = d; b !== null; b = b.base === null ? null : (typeDef(set, b.base) as SimpleDecl | null)) {
        if (!b.lim) continue;
        if (b.lim.min !== undefined && (out.lim.min === undefined || b.lim.min > out.lim.min)) out.lim.min = b.lim.min;
        if (b.lim.max !== undefined && (out.lim.max === undefined || b.lim.max < out.lim.max)) out.lim.max = b.lim.max;
      }
      out.builtin = d.name ?? "string";
      break;
    }
    const placed = d as Partial<Placed>;
    if (d.name) out.chain.push(placed.key ?? d.name);
    for (const f of Object.keys(d.facets) as (keyof Facets)[]) {
      if (f === "pattern") out.patterns.push(d.facets.pattern!);
      else if (out.facets[f] === undefined) {
        (out.facets as Record<string, unknown>)[f] = d.facets[f];
        if (f === "enumeration") out.enumFrom = placed.key ?? null;
      }
    }
    if (d.kind === "ct") {
      const b = d.base === null ? null : typeDef(set, d.base);
      if (b === null && d.base !== null) out.missing = d.base;
      d = b;
      continue;
    }
    if (d.variety === "list") {
      out.variety = "list";
      out.item = typeof d.item === "string" ? typeDef(set, d.item) : d.item;
      break;
    }
    if (d.variety === "union") {
      out.variety = "union";
      out.members = d.members.map((m) => (typeof m === "string" ? typeDef(set, m) : m)).filter((m): m is SimpleDecl | ComplexDecl => m !== null);
      break;
    }
    if (d.anonBase !== null) d = d.anonBase;
    else {
      const b = d.base === null ? null : typeDef(set, d.base);
      if (b === null && d.base !== null) out.missing = d.base;
      d = b;
    }
  }
  return out;
}

export type AttrRowOf = { at: AttrDecl; from: string | null } | { missing: string; kind: "ag" } | { any: string; pc: string };

/** An attribute list with its groups opened and its base's attributes first. */
function attrsOf(set: SchemaSet, list: readonly AttrUse[], from: string | null, out: AttrRowOf[], seen: Set<object>): AttrRowOf[] {
  for (const a of list) {
    if ("ag" in a) {
      const g = set.comps.get(`ag ${a.ag}`) as (AttrGroupDecl & Placed) | undefined;
      if (g === undefined) out.push({ missing: a.ag, kind: "ag" });
      else if (!seen.has(g)) {
        seen.add(g);
        attrsOf(set, g.attrs, g.key, out, seen);
      }
    } else if ("any" in a) out.push({ any: a.any, pc: a.pc });
    else {
      const decl = a.ref !== null ? (set.comps.get(`at ${a.ref}`) as AttrDecl | undefined) : a;
      const at: AttrDecl = {
        ...(decl ?? a),
        use: a.use,
        def: a.def !== null ? a.def : (decl?.def ?? null),
        fixed: a.fixed !== null ? a.fixed : (decl?.fixed ?? null),
        name: a.name,
        attrNs: a.attrNs,
        docu: a.docu.length > 0 ? a.docu : (decl?.docu ?? []),
      };
      const missing = a.ref !== null && decl === undefined ? a.ref : null;
      const at0 = out.findIndex((o) => "at" in o && o.at.name === at.name && o.at.attrNs === at.attrNs);
      if (missing !== null) {
        out.push({ at: { ...at, ref: a.ref, type: null, anon: null }, from });
        missingAttrs.set(at, missing);
      } else if (at.use === "prohibited") {
        if (at0 !== -1) out.splice(at0, 1);
      } else if (at0 !== -1) out[at0] = { at, from };
      else out.push({ at, from });
    }
  }
  return out;
}
/** Attribute rows whose `ref` named an attribute not found here. */
export const missingAttrs = new WeakMap<AttrDecl, string>();

export interface Shape {
  chain: { key: string | null; name: string | null; how: "ext" | "res" | null; builtin: boolean }[];
  content: Particle | null;
  attrs: AttrRowOf[];
  simple: SimpleShape | null;
  mixed: boolean;
  abstract: boolean;
  missing: string | null;
  loop: boolean;
  asserts: string[];
}

const shapes = new WeakMap<object, Shape>();
/**
 * A type read merged (section 10): the chain it derives along, its base's content before its own for an extension,
 * its own alone for a restriction, the attributes of all of them, and what a text value is held to.
 */
export function shape(set: SchemaSet, def: (ComplexDecl | SimpleDecl) | null): Shape | null {
  if (def === null) return null;
  const memo = shapes.get(def);
  if (memo !== undefined) return memo;
  const out: Shape = { chain: [], content: null, attrs: [], simple: null, mixed: false, abstract: def.kind === "ct" && def.abstract, missing: null, loop: false, asserts: [] };
  if (def.kind === "st") {
    out.simple = simpleShape(set, def);
    shapes.set(def, out);
    return out;
  }
  const line: (ComplexDecl | SimpleDecl)[] = [];
  const seen = new Set<object>();
  for (let d: (ComplexDecl | SimpleDecl) | null = def; d !== null; ) {
    if (seen.has(d)) {
      out.loop = true;
      break;
    }
    seen.add(d);
    line.push(d);
    if (d.kind !== "ct" || d.base === null) break;
    const b = typeDef(set, d.base);
    if (b === null) out.missing = d.base;
    d = b;
  }
  const attrs: AttrRowOf[] = [];
  const agSeen = new Set<object>();
  for (let k = line.length - 1; k >= 0; k -= 1) {
    const d = line[k]!;
    if (k < line.length - 1) {
      const up = line[k + 1]! as (ComplexDecl | SimpleDecl) & Partial<Placed>;
      out.chain.unshift({ key: up.key ?? null, name: up.name, how: d.kind === "ct" ? d.deriv : null, builtin: up.kind === "st" && up.builtin === true });
    }
    if (d.kind === "st") continue;
    if (d.mixed) out.mixed = true;
    if (d.simple) continue;
    if (d.deriv === "ext" && out.content !== null && d.content !== null) out.content = { k: "seq", min: 1, max: 1, items: [out.content, d.content], joined: true, line: d.line, docu: [] };
    else if (!(d.deriv === "ext" && out.content !== null)) out.content = d.content;
    out.asserts.push(...d.asserts);
  }
  for (let k = line.length - 1; k >= 0; k -= 1) {
    const d = line[k]!;
    if (d.kind === "ct") attrsOf(set, d.attrs, (d as Partial<Placed>).key ?? null, attrs, agSeen);
  }
  out.attrs = attrs;
  const leaf = line[line.length - 1];
  if (def.simple || (leaf !== undefined && leaf.kind === "st" && line.length > 1)) out.simple = simpleShape(set, def);
  shapes.set(def, out);
  return out;
}

/** A model group's particle with a group reference opened. */
export function openGroup(set: SchemaSet, p: Particle, seen?: Set<object>): Particle {
  if (p.k !== "grp") return p;
  const g = set.comps.get(`gr ${p.ref}`) as (GroupDecl & Placed) | undefined;
  if (g === undefined || g.content === null) return { k: "missing", ref: p.ref, min: p.min, max: p.max };
  if (seen !== undefined && seen.has(g)) return { k: "loop", ref: p.ref, min: p.min, max: p.max };
  if (g.content.k !== "seq" && g.content.k !== "choice" && g.content.k !== "all") return g.content;
  return { ...g.content, min: p.min, max: p.max, from: g.key, name: g.name };
}

export interface ChildEl {
  el: ElementDecl;
  min: number;
  max: number;
}
/** The elements under a particle, groups opened, each with the occurrence it ends up with: an outline's children. */
export function childElements(set: SchemaSet, p: Particle | null, out: ChildEl[] = [], lo = 1, hi = 1, seen: Set<object> = new Set()): ChildEl[] {
  if (p === null) return out;
  const q = openGroup(set, p, seen);
  if (q.k === "el") out.push({ el: q.el, min: lo * q.min, max: hi * q.max });
  else if (q.k === "seq" || q.k === "choice" || q.k === "all") {
    const within = q.from !== undefined ? new Set(seen).add(set.comps.get(q.from)!) : seen;
    for (const it of q.items) childElements(set, it, out, q.k === "choice" ? 0 : lo * q.min, hi * q.max, within);
  }
  return out;
}

/** After loading: who names what (section 11), substitution groups, derived types, roots, prefixes. */
function finish(set: SchemaSet): void {
  const uses = new Map<string, Use[]>();
  const members = new Map<string, string[]>();
  const derived = new Map<string, string[]>();
  const missing = new Map<string, string[]>();
  const use = (target: string, by: string, how: string) => {
    let list = uses.get(target);
    if (list === undefined) uses.set(target, (list = []));
    if (!list.some((u) => u.by === by && u.how === how)) list.push({ by, how });
  };
  const want = (space: Space, key: string, by: string) => {
    if (space === "ty" && isBuiltin(key)) return;
    if (!set.comps.has(`${space} ${key}`)) {
      const k = `${space} ${key}`;
      if (!missing.has(k)) missing.set(k, []);
      missing.get(k)!.push(by);
    }
  };
  const walkSimple = (st: SimpleDecl, by: string) => {
    if (st.base !== null) {
      use(`ty ${st.base}`, by, "base");
      want("ty", st.base, by);
    }
    if (st.anonBase !== null) walkSimple(st.anonBase, by);
    if (typeof st.item === "string") {
      use(`ty ${st.item}`, by, "item");
      want("ty", st.item, by);
    } else if (st.item !== null) walkSimple(st.item, by);
    for (const m of st.members) {
      if (typeof m === "string") {
        use(`ty ${m}`, by, "member");
        want("ty", m, by);
      } else walkSimple(m, by);
    }
  };
  const walkAttrs = (list: readonly AttrUse[], by: string) => {
    for (const a of list) {
      if ("ag" in a) {
        use(`ag ${a.ag}`, by, "group");
        want("ag", a.ag, by);
      } else if (!("any" in a)) {
        if (a.ref !== null) {
          use(`at ${a.ref}`, by, "ref");
          want("at", a.ref, by);
        }
        if (a.type !== null) {
          use(`ty ${a.type}`, by, "type");
          want("ty", a.type, by);
        }
        if (a.anon !== null) walkSimple(a.anon, by);
      }
    }
  };
  const walkParticle = (p: Particle | null, by: string) => {
    if (p === null) return;
    if (p.k === "el") walkEl(p.el, by, false);
    else if (p.k === "grp") {
      use(`gr ${p.ref}`, by, "group");
      want("gr", p.ref, by);
    } else if (p.k === "seq" || p.k === "choice" || p.k === "all") for (const it of p.items) walkParticle(it, by);
  };
  const walkComplex = (ct: ComplexDecl, by: string) => {
    if (ct.base !== null) {
      use(`ty ${ct.base}`, by, ct.deriv === "ext" ? "extends" : "narrows");
      want("ty", ct.base, by);
    }
    if (ct.anonBase !== null) walkSimple(ct.anonBase, by);
    walkParticle(ct.content, by);
    walkAttrs(ct.attrs, by);
  };
  const walkEl = (el: ElementDecl, by: string, global: boolean) => {
    if (el.ref !== null) {
      use(`el ${el.ref}`, by, "ref");
      want("el", el.ref, by);
    }
    if (el.type !== null) {
      use(`ty ${el.type}`, by, "type");
      want("ty", el.type, by);
    }
    if (el.anon !== null) {
      if (el.anon.kind === "ct") walkComplex(el.anon, by);
      else walkSimple(el.anon, by);
    }
    for (const h of el.sub) {
      use(`el ${h}`, by, "substitutes");
      want("el", h, by);
      if (global) {
        if (!members.has(`el ${h}`)) members.set(`el ${h}`, []);
        members.get(`el ${h}`)!.push(by);
      }
    }
  };
  const derive = (base: string, key: string) => {
    if (!derived.has(`ty ${base}`)) derived.set(`ty ${base}`, []);
    derived.get(`ty ${base}`)!.push(key);
  };
  for (const [key, c] of set.comps) {
    if (c.kind === "el") walkEl(c, key, true);
    else if (c.kind === "ct") {
      walkComplex(c, key);
      if (c.base !== null) derive(c.base, key);
    } else if (c.kind === "st") {
      walkSimple(c, key);
      if (c.base !== null) derive(c.base, key);
    } else if (c.kind === "gr") walkParticle(c.content, key);
    else if (c.kind === "ag") walkAttrs(c.attrs, key);
    else if (c.kind === "at") {
      if (c.type !== null) {
        use(`ty ${c.type}`, key, "type");
        want("ty", c.type, key);
      }
      if (c.anon !== null) walkSimple(c.anon, key);
    }
  }
  set.uses = uses;
  set.members = members;
  set.derived = derived;
  set.missing = missing;
  set.roots = rootsOf(set);
  const prefixes = new Map<string, string>();
  const entryDoc = set.docs[0];
  // The opened file's own prefixes first, then each other file's, then a made one.
  if (entryDoc !== undefined) for (const p in entryDoc.prefixes) if (p !== "" && !prefixes.has(entryDoc.prefixes[p]!)) prefixes.set(entryDoc.prefixes[p]!, p);
  for (const doc of set.docs) for (const p in doc.prefixes) if (p !== "" && !prefixes.has(doc.prefixes[p]!)) prefixes.set(doc.prefixes[p]!, p);
  let made = 0;
  for (const doc of set.docs) if (doc.tns !== "" && !prefixes.has(doc.tns)) prefixes.set(doc.tns, `ns${++made}`);
  set.prefixes = prefixes;
  set.home = entryDoc?.tns ?? "";
}

/** How many distinct global elements an element reaches, itself included (bounded: it is a ranking, not a count). */
export function reachOf(set: SchemaSet, key: string, cap = 5000): number {
  const seen = new Set<object>();
  const stack: ElementDecl[] = [];
  const start = set.comps.get(key) as ElementDecl | undefined;
  if (start === undefined) return 0;
  stack.push(start);
  let n = 0;
  while (stack.length > 0 && n < cap) {
    const el = stack.pop()!;
    const d = elDecl(set, el);
    if (d === null || seen.has(d)) continue;
    seen.add(d);
    n += 1;
    const t = elType(set, d);
    if (t.def === null || t.def.kind !== "ct") continue;
    const s = shape(set, t.def);
    for (const c of childElements(set, s?.content ?? null)) stack.push(c.el);
  }
  return n;
}

/**
 * The roots (section 7): global elements of the opened file that no other element holds, the one that reaches the
 * most first; one that reaches under a fifth of what the first reaches is a leftover, not a root.
 */
function rootsOf(set: SchemaSet): string[] {
  const mine = [...set.comps.values()].filter((c) => c.kind === "el" && c.file === set.entry);
  const held = (c: Placed) => (set.uses.get(c.key) ?? []).some((u) => u.how === "ref" && u.by !== c.key);
  const free = mine.filter((c) => !held(c));
  const pool = free.length > 0 ? free : mine;
  const reach = new Map(pool.map((c) => [c.key, reachOf(set, c.key)]));
  const order = [...pool].sort((a, b) => reach.get(b.key)! - reach.get(a.key)! || a.line - b.line);
  const top = reach.get(order[0]?.key ?? "") ?? 0;
  return order.filter((c) => reach.get(c.key)! * 5 >= top).map((c) => c.key);
}

/** The global elements of the opened file nothing holds that are no root (section 7's leftovers). */
export function leftoversOf(set: SchemaSet): string[] {
  const roots = new Set(set.roots);
  const held = (c: Placed) => (set.uses.get(c.key) ?? []).some((u) => u.how === "ref" && u.by !== c.key);
  return [...set.comps.values()].filter((c) => c.kind === "el" && c.file === set.entry && !held(c) && !roots.has(c.key)).map((c) => c.key);
}

/** A name as the set's pages write it: bare in the opened file's namespace, with its prefix elsewhere. */
export function label(set: SchemaSet, ns: string, name: string): string {
  if (ns === XS) return `xs:${name}`;
  if (ns === set.home || ns === "") return name;
  const p = set.prefixes.get(ns);
  return p === undefined ? name : `${p}:${name}`;
}
/** `label` for a component key (`el {ns}name`) or a name key (`{ns}name`). */
export function labelOfKey(set: SchemaSet, key: string): string {
  const k = key.includes(" ") ? key.slice(key.indexOf(" ") + 1) : key;
  return label(set, nsOf(k), localOf(k));
}

/** The place an address names (section 12): `el/ORDER`, `type/bmecat:dtSTRING`, `group/…`, `attrs/…`, `attr/…`. */
const PLACE: Record<Space, string> = { el: "el", ty: "type", gr: "group", ag: "attrs", at: "attr" };
export function placeOf(set: SchemaSet, key: string): string {
  const space = key.slice(0, 2) as Space;
  return `${PLACE[space]}/${labelOfKey(set, key)}`;
}
/** The component a place names, or `null` where the set has none. */
export function keyOfPlace(set: SchemaSet, place: string): string | null {
  const slash = place.indexOf("/");
  if (slash === -1) return null;
  const space = (Object.keys(PLACE) as Space[]).find((s) => PLACE[s] === place.slice(0, slash));
  if (space === undefined) return null;
  const name = place.slice(slash + 1);
  const c = name.indexOf(":");
  let ns: string | undefined = set.home;
  let local = name;
  if (c !== -1) {
    local = name.slice(c + 1);
    const p = name.slice(0, c);
    ns = p === "xs" ? XS : [...set.prefixes].find(([, q]) => q === p)?.[0];
    if (ns === undefined) return null;
  }
  const key = `${space} {${ns}}${local}`;
  return set.comps.has(key) ? key : null;
}

/** The declaration whose lines hold `line` of `file`: the last declared at or before it (an Everything hit). */
export function placeAtLine(set: SchemaSet, file: string, line: number): string | null {
  let best: (Decl & Placed) | null = null;
  for (const c of set.comps.values()) if (c.file === file && c.line <= line && (best === null || c.line > best.line)) best = c;
  return best === null ? null : placeOf(set, best.key);
}
