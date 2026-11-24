/**
 * What the schema worker answers the page (the XSD design, 2026-11-23, sections 6 to 13 and 30): the set's index once
 * per load, a declaration's page whole in one answer, the rows a row opened in place holds, a fold of the tree, and
 * who uses a declaration. The page never holds the model: every answer is plain data, names already in words, each
 * name that goes somewhere carrying the key it goes to.
 */
import { loadFakes } from "./example-fakes";
import { choiceIdOf, makeExample, tickIdOf, type ExampleOptions } from "./xsd-example";
import {
  childElements,
  elDecl,
  elType,
  keyOfPlace,
  label,
  labelOfKey,
  leftoversOf,
  loadSet,
  missingAttrs,
  openGroup,
  placeOf,
  shape,
  simpleShape,
  typeDef,
  xsKey,
  isBuiltin,
  type AttrDecl,
  type AttrRowOf,
  type ComplexDecl,
  type Decl,
  type Docu,
  type ElementDecl,
  type Particle,
  type SchemaSet,
  type SetInput,
  type SimpleDecl,
  type SimpleShape,
} from "./xsd";
import { baseOf, isRemote, type Elsewhere, type How, type Pointed, type SchemaIndex } from "./xsd-locate";
import { attach, detach, dropMoved, forget, keepIfWhole, keptOf, keyOf, release, sameAnswers, type Kept, type KeepAt } from "./xsd-kept";
import { findingWords, type Finding, type Words } from "./xsd-findings";
import { rx } from "./xsd-pattern";
import { XS, localOf, nsOf } from "./xsd-xml";

// ---- What crosses to the page ----

/** A run of words; `code` in the code face. */
export interface Run {
  readonly text: string;
  readonly code?: true;
}
/** A name as a page writes it: its prefix dim, and the component it goes to where it is one. */
export interface Name {
  readonly prefix: string;
  readonly local: string;
  readonly key: string | null;
}
export interface Occurs {
  readonly min: number;
  /** `-1` for unbounded (it crosses as JSON). */
  readonly max: number;
}
export interface TypeCell {
  readonly name: Name | null;
  /** *its own*, *narrowed*, *with attributes*. */
  readonly note: string | null;
  /** The name of a type not found here. */
  readonly missing: string | null;
}

export type Row =
  | { readonly k: "attr"; readonly id: string; readonly name: Name; readonly type: TypeCell; readonly notes: readonly Run[]; readonly occ: Occurs; readonly facts: readonly (readonly Run[])[]; readonly docu: DocuLine | null }
  | {
      readonly k: "el";
      readonly id: string;
      readonly name: Name;
      readonly type: TypeCell;
      readonly notes: readonly Run[];
      readonly occ: Occurs;
      readonly facts: readonly (readonly Run[])[];
      readonly docu: DocuLine | null;
      /** It holds rows of its own that open in place. */
      readonly can: boolean;
      /** It is already on the way down: it does not open again. */
      readonly again: boolean;
      /** Its declaration is not found here. */
      readonly lost: boolean;
      /** An optional row's tick (S4b): the particle's file and line, as the example names it; `null` where it must stand. */
      readonly tick: string | null;
      /** It may stand more than once: its tick carries a count. */
      readonly many: boolean;
    }
  | { readonly k: "group"; readonly id: string; readonly words: string; readonly from: Name | null; readonly occ: Occurs; readonly items: readonly Row[]; readonly choice: Choice | null }
  | { readonly k: "any"; readonly id: string; readonly words: string; readonly occ: Occurs }
  | { readonly k: "note"; readonly id: string; readonly runs: readonly Run[]; readonly lost: Name | null };

/** A choice's branches, named, for the example's pick on its line (section 22). */
export interface Choice {
  readonly id: string;
  readonly branches: readonly string[];
}

export interface DocuLine {
  readonly lang: string | null;
  readonly text: string;
}

export interface ValueFacts {
  readonly held: readonly (readonly Run[])[];
  readonly always: string | null;
  readonly fallback: string | null;
  readonly values: readonly { v: string; title: string }[];
  readonly valueCount: number;
  readonly valuesFrom: Name | null;
  readonly patterns: readonly { text: string; unread: string | null }[];
  readonly union: readonly Name[];
  readonly list: Name | null;
  readonly builtin: string | null;
}

export interface UseRow {
  readonly kind: string;
  readonly name: Name;
  readonly how: string;
  readonly file: string | null;
}

export interface PageAnswer {
  readonly kind: "page";
  readonly key: string;
  readonly place: string;
  readonly name: Name;
  readonly what: string;
  readonly file: string;
  /** *which this file reaches*, where it is another file. */
  readonly reached: boolean;
  readonly line: number;
  readonly over: string | null;
  /** The line it derives along: steps of a word and a name. */
  readonly chain: readonly { readonly word: string; readonly name: Name | null; readonly text: string }[];
  readonly type: TypeCell | null;
  readonly docu: readonly Docu[];
  readonly value: ValueFacts | null;
  /** *Holds* or *Attributes*; its rows. */
  readonly holdsLabel: string;
  readonly rows: readonly Row[];
  readonly empty: boolean;
  readonly expandable: boolean;
  readonly standIns: readonly Name[];
  readonly derived: readonly Name[];
  readonly derivedCount: number;
  readonly rules: readonly string[];
  readonly uses: readonly UseRow[];
  readonly useCount: number;
  /** Elements only: an example can be made from it. */
  readonly global: boolean;
}

export interface FileRow {
  readonly path: string;
  readonly depth: number;
  readonly ns: string;
  readonly prefix: string | null;
  /** *imported, beside it*, or *12 declarations* for the opened file. */
  readonly how: string;
  readonly count: number;
  /** How it was found, as a key (`beside`, `name`, …); `null` for the opened file and one still being read. */
  readonly found: How | null;
  readonly adopted: boolean;
  readonly reading: boolean;
  readonly bad: string | null;
  /** Another repository's, and where it was read. */
  readonly from: Elsewhere | null;
  /** The address it was downloaded from; `null` for a file not downloaded. */
  readonly url: string | null;
}
export interface LostRow {
  readonly depth: number;
  readonly kind: string;
  readonly location: string | null;
  readonly namespace: string | null;
  readonly why: string;
  readonly address: string | null;
  readonly two: readonly Elsewhere[] | null;
  readonly by: string;
}

/** A declaration in the rail's lists and the filter. */
export interface DeclRow {
  readonly key: string;
  readonly kind: "el" | "ct" | "st" | "gr" | "ag" | "at";
  readonly prefix: string;
  readonly local: string;
  readonly file: string;
  readonly line: number;
  /** An element's type, quiet at the row's end. */
  readonly type: string;
  /** Its documentation's first sentence: the filter reads it. */
  readonly words: string;
}

export interface SetIndex {
  /**
   * The overview's *Files* in the order they were found, breadth first: a round of reads only ever appends, so a row
   * drawn never moves when the next files land (section 30). Each names a row of `files` or of `lost`.
   */
  readonly listing: readonly { readonly kind: "file" | "lost"; readonly at: number }[];
  readonly entry: string;
  readonly home: string;
  readonly version: string | null;
  readonly v11: boolean;
  readonly head: string | null;
  readonly files: readonly FileRow[];
  readonly lost: readonly LostRow[];
  readonly decls: readonly DeclRow[];
  readonly roots: readonly { key: string; holds: number }[];
  /** One root that only wraps the documents: those it holds, with how often. */
  readonly under: readonly { name: Name; occ: Occurs }[];
  readonly underOneOf: boolean;
  readonly seen: readonly (readonly Run[])[];
  readonly ownElements: number;
  readonly ownTypes: number;
  readonly total: number;
  readonly reading: number;
  readonly ms: { read: number; compile: number };
  /** The rows' own counts of each file, for the rail's folds. */
  readonly perFile: readonly { path: string; elements: number; types: number }[];
}

export interface TreeRow {
  readonly id: string;
  readonly name: Name;
  readonly occ: Occurs | null;
  readonly kids: boolean;
  readonly again: boolean;
  readonly lost: boolean;
  readonly root: boolean;
}

export type XsdCall =
  | { kind: "open"; entry: string; text: string; paths: string[] | null; pointed: Pointed[]; home?: string; lean?: boolean; keep?: KeepAt }
  | { kind: "addFiles"; files: { path: string; text: string | null }[]; paths?: string[]; index?: SchemaIndexWire; elsewhere?: { namespace: string; list: Elsewhere[] } }
  | { kind: "pointed"; pointed: Pointed[] }
  | { kind: "page"; place: string }
  | { kind: "rows"; id: string }
  | { kind: "tree"; under: string | null }
  | { kind: "uses"; key: string }
  | { kind: "values"; key: string; from: number }
  | { kind: "way"; key: string }
  | { kind: "check"; xml: string }
  | { kind: "example"; key: string; options: ExampleOptions }
  | { kind: "declOf"; names: { ns: string; local: string }[] }
  | { kind: "close" }
  /** Every kept set dropped (`xsd-kept.ts`): a pick, a download, a Settings change. Asks no session. */
  | { kind: "forget" };

/** The repository's index as it crosses: a map does not survive JSON in every engine. */
export interface SchemaIndexWire {
  readonly declares: readonly [string, string | null][];
  readonly catalogs: SchemaIndex["catalogs"];
  readonly total: number;
  readonly read: number;
}

export type Opened = { ok: true; index: SetIndex; need: string[] } | { ok: false; error: string; line: number | null };

// ---- A session ----

export interface XsdSession {
  entry: string;
  home?: string;
  /** A check's session: its answers carry no declarations, which only the explorer draws. */
  lean?: boolean;
  texts: Map<string, string | null>;
  paths: Set<string> | null;
  pointed: Pointed[];
  index: SchemaIndex | null;
  elsewhere: Map<string, Elsewhere[]>;
  set: SchemaSet | null;
  /** What each row id stands for, for the set's life: an element and the declarations on its way down. */
  rows: Map<string, { el: ElementDecl; trail: readonly object[] }>;
  tree: Map<string, { el: ElementDecl; trail: readonly object[] }>;
  /** What the set is kept by once whole, where the page's reads are pinned (`xsd-kept.ts`). */
  keep?: KeepAt & { readonly key: string };
  /** The kept set this session reads, shared until it changes it. */
  kept?: Kept;
  /** A kept set served once what moves under its pin has answered as it did. */
  verifying?: boolean;
}
export type XsdSessions = Map<number, XsdSession>;

/** The schemas that come with knobas, by namespace and path, handed in by the worker (they load with its chunk). */
export interface Bundled {
  readonly byNs: ReadonlyMap<string, string>;
  readonly texts: ReadonlyMap<string, string>;
}

function load(s: XsdSession, bundled: Bundled): SchemaSet {
  const texts = new Map(s.texts);
  for (const [p, t] of bundled.texts) if (!texts.has(p)) texts.set(p, t);
  const inn: SetInput = {
    entry: s.entry,
    texts,
    paths: s.paths,
    pointed: s.pointed,
    index: s.index,
    elsewhere: (ns) => s.elsewhere.get(ns) ?? null,
    bundled: bundled.byNs,
    ...(s.home !== undefined ? { home: s.home } : {}),
  };
  s.set = loadSet(inn);
  // Row and fold ids stay good across rounds: what they name is compiled data the new set reads by its names.
  return s.set;
}

/** Answers one call of one session. */
export function answerXsd(sessions: XsdSessions, session: number, call: XsdCall, bundled: Bundled): unknown {
  const store = keptOf(sessions);
  if (call.kind === "forget") {
    forget(store);
    return null;
  }
  if (call.kind === "open") {
    const was = sessions.get(session);
    if (was !== undefined) release(store, was);
    const keep = call.keep === undefined ? undefined : { ...call.keep, key: keyOf(call.keep, call, bundled) };
    if (keep !== undefined) {
      dropMoved(store, keep);
      const kept = store.get(keep.key);
      if (kept !== undefined && kept.base.set !== null) {
        // The set kept whole: served at once, or once what moves under the pin has answered as it did.
        const s = attach(store, kept);
        sessions.set(session, s);
        const answer = opened(kept.base.set, s.lean === true, s.index);
        if (kept.verify.length === 0 || !answer.ok) return answer;
        s.verifying = true;
        return { ...answer, need: [...kept.verify] };
      }
    }
    const s: XsdSession = { entry: call.entry, ...(call.home !== undefined ? { home: call.home } : {}), ...(call.lean === true ? { lean: true } : {}), texts: new Map([[call.entry, call.text]]), paths: call.paths === null ? null : new Set(call.paths), pointed: call.pointed, index: null, elsewhere: new Map(), set: null, rows: new Map(), tree: new Map(), ...(keep !== undefined ? { keep } : {}) };
    sessions.set(session, s);
    const answer = opened(load(s, bundled), s.lean === true, s.index);
    keepIfWhole(store, s);
    return answer;
  }
  const s = sessions.get(session);
  if (s === undefined) return { ok: false, error: "The schema is no longer open.", line: null };
  switch (call.kind) {
    case "addFiles": {
      if (s.verifying === true) {
        if (s.kept !== undefined && sameAnswers(s, s.kept.verify, call.files, call.elsewhere)) {
          delete s.verifying;
          return opened(s.set!, s.lean === true, s.index);
        }
        // Something under the pin moved: the kept set is stale, and this session reads the set again.
        if (s.kept !== undefined) store.delete(s.kept.key);
      }
      detach(store, s);
      for (const f of call.files) s.texts.set(f.path, f.text);
      if (call.paths !== undefined) s.paths = new Set(call.paths);
      if (call.index !== undefined) s.index = { declares: new Map(call.index.declares.map(([p, n]) => [p, n ?? undefined])), catalogs: call.index.catalogs, total: call.index.total, read: call.index.read };
      if (call.elsewhere !== undefined) s.elsewhere.set(call.elsewhere.namespace, call.elsewhere.list);
      const answer = opened(load(s, bundled), s.lean === true, s.index);
      keepIfWhole(store, s);
      return answer;
    }
    case "pointed":
      // Pointed elsewhere, the set is no longer the one its key names: it is read again and not kept.
      detach(store, s);
      delete s.keep;
      s.pointed = call.pointed;
      return opened(load(s, bundled), s.lean === true, s.index);
    case "page":
      return pageOf(s, call.place);
    case "rows": {
      const at = s.rows.get(call.id);
      if (at === undefined || s.set === null) return [];
      const t = elType(s.set, at.el);
      return rowsOf(s, t.def, call.id, [...at.trail, elDecl(s.set, at.el) ?? at.el]);
    }
    case "tree":
      return treeOf(s, call.under);
    case "uses":
      return s.set === null ? [] : usesOf(s.set, call.key, Infinity);
    case "values":
      return s.set === null ? [] : allValues(s.set, call.key).slice(call.from);
    case "way":
      return wayDown(s, call.key);
    case "check":
      return checkWith(s, call.xml, bundled);
    case "example": {
      // Faker in the language asked for loads with the first example in it, here in the worker; until then, and where
      // it cannot load, the fixed values stand (`example-fakes.ts`).
      const set = s.set;
      if (set === null) return null;
      const options = call.options;
      return loadFakes(options.lang).then(
        () => makeExample(set, call.key, options),
        () => makeExample(set, call.key, options),
      );
    }
    case "declOf":
      return declarationOf(s, call.names);
    case "close":
      release(store, s);
      sessions.delete(session);
      return null;
  }
}

function opened(set: SchemaSet, lean = false, repo: { total: number; read: number } | null = null): Opened {
  const entry = set.docs[0];
  if (entry !== undefined && entry.bad !== null) return { ok: false, error: entry.bad, line: entry.badLine };
  return { ok: true, index: indexOf(set, lean, repo), need: set.need };
}

// ---- The check (section 24) ----

/** A finding as it crosses: libxml2's, and its words. */
export interface Departure extends Finding {
  readonly words: Words;
}
export type Checked =
  | { readonly state: "checked"; readonly findings: readonly Departure[]; readonly total: number; readonly ms: number; readonly compileMs: number; readonly asked: readonly string[] }
  | { readonly state: "not-checked"; readonly why: string; readonly detail: readonly string[] };

async function checkWith(s: XsdSession, xml: string, bundled: Bundled): Promise<Checked> {
  const set = s.set;
  if (set === null) return { state: "not-checked", why: "reading", detail: [] };
  const { check } = await import("./xsd-check");
  const answer = await check(set, (p) => s.texts.get(p) ?? bundled.texts.get(p) ?? null, xml);
  if (answer.state !== "checked") return answer;
  return { ...answer, findings: answer.findings.map((f) => ({ ...f, words: findingWords(f, (ns, local) => label(set, ns, local)) })) };
}

// ---- Words ----

const nameOf = (set: SchemaSet, ns: string, local: string, key: string | null): Name => {
  const l = label(set, ns, local);
  const c = l.indexOf(":");
  if (ns === XS) return { prefix: "", local, key: null };
  return c === -1 ? { prefix: "", local: l, key } : { prefix: l.slice(0, c + 1), local: l.slice(c + 1), key };
};
const nameOfKey = (set: SchemaSet, key: string, go = true): Name => {
  const k = key.includes(" ") ? key.slice(key.indexOf(" ") + 1) : key;
  const comp = key.includes(" ") ? key : null;
  return nameOf(set, nsOf(k), localOf(k), go && comp !== null && set.comps.has(comp) ? comp : null);
};
const occ = (min: number, max: number): Occurs => ({ min, max: max === Infinity ? -1 : max });
const count = (n: number) => n.toLocaleString("en");
const plural = (n: number, one: string, many: string) => `${count(n)} ${n === 1 ? one : many}`;

/** A type as a row's type cell says it. */
function typeCell(set: SchemaSet, t: { def: (ComplexDecl | SimpleDecl) | null; key: string | null; missing?: string | null }): TypeCell {
  if (t.def === null) return { name: null, note: null, missing: t.missing ? labelOfKey(set, t.missing) : "?" };
  if (t.key !== null) return { name: nameOfKey(set, `ty ${t.key}`), note: null, missing: null };
  if (t.def.kind === "ct") {
    if (t.def.simple && t.def.base !== null) return { name: nameOfKey(set, `ty ${t.def.base}`), note: "with attributes", missing: null };
    return { name: null, note: "its own", missing: null };
  }
  const base = t.def.base;
  if (base !== null) return { name: nameOfKey(set, `ty ${base}`), note: "narrowed", missing: null };
  return { name: null, note: t.def.variety === "list" ? "a list of its own" : t.def.variety === "union" ? "a union of its own" : "its own", missing: null };
}

/** What a simple type holds a value to, as the facts line under a row (section 10). */
export function factsOf(sh: SimpleShape, wide: boolean): Run[][] {
  const f = sh.facets;
  const out: Run[][] = [];
  const words = (t: string) => out.push([{ text: t }]);
  const lo = f.length ?? f.minLength;
  const hi = f.length ?? f.maxLength;
  if (f.length !== undefined) words(`exactly ${f.length} characters`);
  else if (lo !== undefined && hi !== undefined) words(`${lo} to ${Number(hi).toLocaleString("en")} characters`);
  else if (hi !== undefined) words(`up to ${Number(hi).toLocaleString("en")} characters`);
  else if (lo !== undefined) words(`at least ${lo} characters`);
  const min = f.minInclusive !== undefined ? `at least ${f.minInclusive}` : f.minExclusive !== undefined ? `more than ${f.minExclusive}` : sh.lim.min !== undefined && sh.builtin !== "integer" ? `at least ${sh.lim.min}` : null;
  const max = f.maxInclusive !== undefined ? `at most ${f.maxInclusive}` : f.maxExclusive !== undefined ? `less than ${f.maxExclusive}` : sh.lim.max !== undefined && sh.lim.max < 1e12 ? `at most ${sh.lim.max}` : null;
  if (min !== null) words(min);
  if (max !== null) words(max);
  if (f.totalDigits !== undefined) words(`up to ${f.totalDigits} digits`);
  if (f.fractionDigits !== undefined) words(`${f.fractionDigits} decimal places`);
  if (f.enumeration !== undefined) {
    const show = wide ? 60 : 8;
    const runs: Run[] = [{ text: "One of " }];
    f.enumeration.slice(0, show).forEach((e, i) => {
      if (i > 0) runs.push({ text: " " });
      runs.push({ text: e.v, code: true });
    });
    if (f.enumeration.length > show) runs.push({ text: ` and ${count(f.enumeration.length - show)} more` });
    out.push(runs);
  }
  for (const step of sh.patterns) for (const p of step) out.push([{ text: "Pattern " }, { text: !wide && p.length > 64 ? p.slice(0, 61) + "…" : p, code: true }]);
  if (sh.variety === "list") words("a list, space between its values");
  return out;
}

const docuLine = (docu: readonly Docu[]): DocuLine | null => {
  const d = docu.find((x) => x.text !== "");
  if (d === undefined) return null;
  const first = d.text.split(/\n\n/)[0]!;
  return { lang: d.lang, text: first.length > 280 ? first.slice(0, 277) + "…" : first };
};

const notesOf = (fixed: string | null, def: string | null, nil = false, abstract = false): Run[] => {
  const out: Run[] = [];
  if (fixed !== null) out.push({ text: "always " }, { text: fixed, code: true });
  if (def !== null) out.push({ text: (out.length > 0 ? " " : "") + "default " }, { text: def, code: true });
  if (nil) out.push({ text: (out.length > 0 ? " " : "") + "may be nil" });
  if (abstract) out.push({ text: (out.length > 0 ? " " : "") + "abstract" });
  return out;
};

// ---- Rows ----

function attrRow(set: SchemaSet, at: AttrDecl, id: string): Row {
  const lost = missingAttrs.get(at);
  const def = at.anon ?? (at.type !== null ? typeDef(set, at.type) : typeDef(set, xsKey("string")));
  const t = at.anon !== null ? { def: at.anon, key: null } : { def, key: at.type ?? xsKey("string"), missing: def === null ? at.type : null };
  const sh = def === null ? null : simpleShape(set, def);
  const name = { ...nameOf(set, at.attrNs, at.name, null), prefix: "@" + (at.attrNs !== "" && at.attrNs !== set.home ? (set.prefixes.get(at.attrNs) ?? "") + ":" : "") };
  return {
    k: "attr",
    id: `${id}/@${at.name}`,
    name: lost !== undefined ? { ...name, key: null } : name,
    type: lost !== undefined ? { name: null, note: null, missing: labelOfKey(set, lost) } : typeCell(set, t),
    notes: notesOf(at.fixed, at.def),
    occ: occ(at.use === "required" ? 1 : 0, 1),
    facts: sh === null ? [] : factsOf(sh, false),
    docu: docuLine(at.docu),
  };
}

function rowsOf(s: XsdSession, def: (ComplexDecl | SimpleDecl) | null, id: string, trail: readonly object[]): Row[] {
  const set = s.set!;
  const sh = shape(set, def);
  if (sh === null) return [];
  const rows: Row[] = [];
  sh.attrs.forEach((a: AttrRowOf, i) => {
    if ("any" in a) rows.push({ k: "note", id: `${id}/@*${i}`, runs: [{ text: "Any other attribute may stand here." }], lost: null });
    else if ("missing" in a) rows.push({ k: "note", id: `${id}/@!${i}`, runs: [{ text: "The group of attributes " }], lost: nameOfKey(set, `ag ${a.missing}`, false) });
    else rows.push(attrRow(set, a.at, id));
  });
  if (sh.content !== null) rows.push(...particleRows(s, sh.content, id, trail, 1, 1));
  return rows;
}

function particleRows(s: XsdSession, p: Particle, id: string, trail: readonly object[], lo: number, hi: number): Row[] {
  const set = s.set!;
  const q = openGroup(set, p);
  if (q.k === "missing") return [{ k: "note", id: `${id}/!${q.ref}`, runs: [{ text: "The group " }], lost: nameOfKey(set, `gr ${q.ref}`, false) }];
  if (q.k === "loop") return [{ k: "note", id: `${id}/~${q.ref}`, runs: [{ text: "It holds itself." }], lost: null }];
  if (q.k === "any") {
    const from = q.ns === "##other" ? "from another namespace" : q.ns === "##any" ? "from any namespace" : q.ns === "##local" ? "in no namespace" : q.ns === "##targetNamespace" ? "from this namespace" : `from ${q.ns}`;
    return [{ k: "any", id: `${id}/*${q.line}`, words: `${from}${q.pc === "strict" ? ", declared" : ""}`, occ: occ(q.min * lo, q.max * hi) }];
  }
  if (q.k === "el") return [elementRow(s, q.el, `${id}/${q.el.name}`, trail, q.min * lo, q.max * hi, q.min === 0 && q.max > 0 ? tickIdOf(q.el) : null, q.max > 1)];
  if (q.k === "grp") return [];
  const plain = q.k === "seq" && q.min === 1 && q.max === 1 && q.from === undefined;
  const inner = q.items.flatMap((it) => particleRows(s, it, plain ? id : `${id}/${q.k}${q.line}`, trail, plain ? lo : 1, plain ? hi : 1));
  if (plain) return inner;
  const words = q.k === "choice" ? "One of these" : q.k === "all" ? "These, in any order" : "These together";
  let choice: Choice | null = null;
  if (q.k === "choice") {
    const branches = q.items.map((it) => {
      const b = openGroup(set, it);
      if (b.k === "el") return label(set, b.el.elNs, b.el.name);
      const kids = childElements(set, b).map((c) => label(set, c.el.elNs, c.el.name));
      return kids.length === 0 ? "nothing" : kids.length <= 2 ? kids.join(" + ") : `${kids[0]} and ${kids.length - 1} more`;
    });
    // A choice is named by the file and line it is written at, as the example names it: one pick holds wherever it
    // stands (section 22).
    choice = { id: choiceIdOf(set, q) ?? `${q.line}:${branches.join("|")}`, branches };
  }
  return [{ k: "group", id: `${id}/${q.k}${q.line}`, words, from: q.from !== undefined ? nameOfKey(set, q.from) : null, occ: occ(q.min * lo, q.max * hi), items: inner, choice }];
}

function elementRow(s: XsdSession, el: ElementDecl, id: string, trail: readonly object[], min: number, max: number, tick: string | null = null, many = false): Row {
  const set = s.set!;
  const decl = elDecl(set, el);
  const name = nameOf(set, el.elNs, el.name, decl !== null && el.ref !== null ? `el ${el.ref}` : null);
  if (decl === null) return { k: "el", id, name: { ...name, key: null }, type: { name: null, note: null, missing: null }, notes: [], occ: occ(min, max), facts: [], docu: null, can: false, again: false, lost: true, tick: null, many: false };
  const t = elType(set, decl);
  const sh = t.def === null ? null : shape(set, t.def);
  const again = trail.includes(decl);
  const can = sh !== null && !again && (sh.content !== null || sh.attrs.length > 0);
  if (can) s.rows.set(id, { el: decl, trail });
  return {
    k: "el",
    id,
    name,
    type: typeCell(set, t),
    notes: notesOf(decl.fixed, decl.def, decl.nillable, decl.abstract),
    occ: occ(min, max),
    facts: sh !== null && sh.simple !== null ? factsOf(sh.simple, false) : [],
    docu: docuLine(decl.docu.length > 0 ? decl.docu : el.docu),
    can,
    again,
    lost: false,
    tick: again ? null : tick,
    many,
  };
}

// ---- A page ----

const KIND: Record<Decl["kind"], string> = { el: "element", ct: "type", st: "type", gr: "group", ag: "group of attributes", at: "attribute" };
const HOW_USED: Record<string, string> = {
  ref: "holds it",
  type: "is of this type",
  base: "narrows it",
  extends: "extends it",
  narrows: "narrows it",
  item: "is a list of it",
  member: "may be one",
  group: "takes it in",
  substitutes: "may stand in for it",
};

function usesOf(set: SchemaSet, key: string, max: number): UseRow[] {
  const list = (set.uses.get(key) ?? []).filter((u) => u.by !== key);
  return list.slice(0, max).map((u) => {
    const c = set.comps.get(u.by);
    return { kind: c === undefined ? "" : KIND[c.kind], name: nameOfKey(set, u.by), how: HOW_USED[u.how] ?? u.how, file: c !== undefined && c.file !== set.entry ? baseOf(c.file) : null };
  });
}

function chainOf(set: SchemaSet, keys: readonly string[], builtin: string | undefined, after: boolean): PageAnswer["chain"] {
  const names: { name: Name | null; text: string }[] = [...keys.map((k) => ({ name: nameOfKey(set, k), text: "" })), ...(builtin !== undefined ? [{ name: null, text: builtin }] : [])];
  return names.map((n, i) => ({ word: i === 0 && !after ? "Narrows" : "which narrows", ...n }));
}

function valueOf(set: SchemaSet, sh: SimpleShape, decl: { fixed?: string | null; def?: string | null; key?: string; type?: string | null }): ValueFacts {
  const f = sh.facets;
  const values = f.enumeration ?? [];
  const ownEnum = sh.enumFrom === null || sh.enumFrom === decl.key || `ty ${decl.type ?? ""}` === sh.enumFrom;
  return {
    held: factsOf({ ...sh, facets: { ...f, enumeration: undefined as never }, patterns: [] }, true),
    always: decl.fixed ?? null,
    fallback: decl.def ?? null,
    values: values.slice(0, 48).map((e) => ({ v: e.v, title: e.docu[0]?.text ?? "" })),
    valueCount: values.length,
    valuesFrom: ownEnum || sh.enumFrom === null ? null : nameOfKey(set, sh.enumFrom),
    patterns: sh.patterns.flat().map((p) => {
      const r = rx(p);
      return { text: p, unread: r.ok ? null : r.why };
    }),
    union: sh.variety === "union" ? sh.members.map((m) => ((m as { key?: string }).key !== undefined ? nameOfKey(set, (m as { key: string }).key) : { prefix: "", local: m.name ?? "its own", key: null })) : [],
    list: sh.variety === "list" ? ((sh.item as { key?: string } | null)?.key !== undefined ? nameOfKey(set, (sh.item as { key: string }).key) : { prefix: "", local: "values", key: null }) : null,
    builtin: sh.builtin ?? null,
  };
}

/** All values of a type's enumeration: *Show the other 1,047*. */
export function allValues(set: SchemaSet, key: string): { v: string; title: string }[] {
  const c = set.comps.get(key);
  if (c === undefined) return [];
  const def = c.kind === "el" ? elType(set, c).def : c.kind === "ct" || c.kind === "st" ? c : c.kind === "at" ? (c.anon ?? typeDef(set, c.type ?? xsKey("string"))) : null;
  if (def === null) return [];
  return (simpleShape(set, def).facets.enumeration ?? []).map((e) => ({ v: e.v, title: e.docu[0]?.text ?? "" }));
}

function pageOf(s: XsdSession, place: string): PageAnswer | { missing: string; reading: number } {
  const set = s.set;
  if (set === null) return { missing: place, reading: 1 };
  const reading = set.docs.filter((d) => d.reading).length;
  const key = place.startsWith("el ") || place.startsWith("ty ") || place.startsWith("gr ") || place.startsWith("ag ") || place.startsWith("at ") ? place : keyOfPlace(set, place);
  const decl = key === null ? undefined : set.comps.get(key);
  if (key === null || decl === undefined) return { missing: place, reading };
  return pageOfDecl(s, key, decl);
}

/**
 * The declaration of an element of a document (section 26): found by walking down from the root by name, with no
 * validation in it, so it is there for a valid document and for a wrong one. The page of the deepest element found.
 */
export function declarationOf(s: XsdSession, names: readonly { ns: string; local: string }[]): (PageAnswer & { found: number }) | null {
  const set = s.set;
  const first = names[0];
  if (set === null || first === undefined) return null;
  const rootKey = `el {${first.ns}}${first.local}`;
  let decl = set.comps.get(rootKey) as ElementDecl | undefined;
  if (decl === undefined) return null;
  let found = 1;
  for (const n of names.slice(1)) {
    const d = elDecl(set, decl);
    if (d === null) break;
    const t = elType(set, d);
    const sh = t.def === null ? null : shape(set, t.def);
    const next = childElements(set, sh?.content ?? null).find((c) => c.el.name === n.local && c.el.elNs === n.ns);
    if (next === undefined) break;
    decl = (elDecl(set, next.el) ?? next.el) as ElementDecl;
    found += 1;
  }
  const d = elDecl(set, decl) ?? decl;
  const key = (d as Partial<{ key: string }>).key;
  const page = pageOfDecl(s, key ?? `local ${d.at ?? ""}:${d.line}:${d.name}`, d as Decl & { file?: string });
  return { ...page, found };
}

function pageOfDecl(s: XsdSession, key: string, decl: Decl & { file?: string; over?: string }): PageAnswer {
  const set = s.set!;
  const file = decl.file ?? (decl.kind === "el" ? (decl.at ?? set.entry) : set.entry);
  const doc = set.docs.find((d) => d.path === file);
  const name = set.comps.has(key) ? nameOfKey(set, key) : decl.kind === "el" ? nameOf(set, decl.elNs, decl.name, null) : { prefix: "", local: decl.name ?? "", key: null };
  let what = "";
  let chain: PageAnswer["chain"] = [];
  let type: TypeCell | null = null;
  let value: ValueFacts | null = null;
  let rows: Row[] = [];
  let holdsLabel = "Holds";
  let empty = false;
  let expandable = false;
  const rules: string[] = [];
  const id = key;
  if (decl.kind === "el") {
    const t = elType(set, decl);
    const sh = t.def === null ? null : shape(set, t.def);
    what = `Element${decl.abstract ? ", abstract" : ""}`;
    type = typeCell(set, t);
    if (sh !== null && sh.simple !== null) {
      chain = chainOf(set, sh.simple.chain.slice(t.key !== null ? 1 : 0), sh.simple.builtin, true);
      value = valueOf(set, sh.simple, decl);
    }
    rows = t.def === null ? [] : rowsOf(s, t.def, id, [decl]);
    holdsLabel = sh !== null && sh.simple !== null ? "Attributes" : "Holds";
    empty = rows.length === 0 && sh !== null && sh.simple === null;
    expandable = sh !== null && sh.content !== null;
    for (const c of decl.idc) rules.push(`${c.kind} ${c.name ?? ""}`.trim());
  } else if (decl.kind === "ct" || decl.kind === "st") {
    const sh = shape(set, decl)!;
    what = decl.kind === "ct" ? `Complex type${decl.abstract ? ", abstract" : ""}` : "Simple type";
    if (decl.kind === "ct" && sh.chain.length > 0)
      chain = sh.chain.map((c) => ({ word: c.how === "ext" ? "extends" : "narrows", name: c.key !== null && !c.builtin ? nameOfKey(set, c.key) : null, text: c.key !== null && !c.builtin ? "" : (c.name ?? "").replace(/^xs:/, "") }));
    else if (sh.simple !== null) chain = chainOf(set, sh.simple.chain.slice(1), sh.simple.builtin, false);
    if (sh.simple !== null) value = valueOf(set, sh.simple, decl);
    if (decl.kind === "ct") rows = rowsOf(s, decl, id, []);
    holdsLabel = sh.simple !== null ? "Attributes" : "Holds";
    expandable = sh.content !== null;
  } else if (decl.kind === "gr") {
    what = "Group of elements";
    rows = decl.content === null ? [] : particleRows(s, decl.content, id, [], 1, 1);
    expandable = true;
  } else if (decl.kind === "ag") {
    what = "Group of attributes";
    holdsLabel = "Attributes";
    const fake: ComplexDecl = { kind: "ct", name: null, line: decl.line, docu: [], abstract: false, mixed: false, deriv: null, base: null, simple: false, facets: {}, content: null, attrs: decl.attrs, asserts: [], open: null, anonBase: null };
    rows = rowsOf(s, fake, id, []);
  } else {
    what = "Attribute";
    const def = decl.anon ?? typeDef(set, decl.type ?? xsKey("string"));
    type = typeCell(set, decl.anon !== null ? { def: decl.anon, key: null } : { def, key: decl.type ?? xsKey("string"), missing: def === null ? decl.type : null });
    if (def !== null) value = valueOf(set, simpleShape(set, def), decl);
  }
  const allUses = (set.uses.get(key) ?? []).filter((u) => u.by !== key);
  const derived = set.derived.get(key) ?? [];
  return {
    kind: "page",
    key,
    place: set.comps.has(key) ? placeOf(set, key) : "",
    name,
    what,
    file,
    reached: file !== set.entry && doc !== undefined,
    line: decl.line,
    over: decl.over === undefined ? null : decl.over === "redefine" ? "Redefined here" : "Overridden here",
    chain,
    type,
    docu: decl.docu,
    value,
    holdsLabel,
    rows,
    empty,
    expandable,
    standIns: (set.members.get(key) ?? []).map((m) => nameOfKey(set, m)),
    derived: derived.slice(0, 24).map((m) => nameOfKey(set, m)),
    derivedCount: derived.length,
    rules,
    uses: usesOf(set, key, 24),
    useCount: allUses.length,
    global: decl.kind === "el" && !decl.abstract,
  };
}

// ---- The tree (section 7) ----

function treeOf(s: XsdSession, under: string | null): TreeRow[] {
  const set = s.set;
  if (set === null) return [];
  if (under === null) {
    return set.roots.map((key) => {
      const el = set.comps.get(key) as ElementDecl;
      const id = `R${key}`;
      s.tree.set(id, { el, trail: [] });
      return { id, name: nameOfKey(set, key), occ: null, kids: kidsOf(set, el).length > 0, again: false, lost: false, root: true };
    });
  }
  const at = s.tree.get(under);
  if (at === undefined) return [];
  const self = elDecl(set, at.el);
  if (self === null) return [];
  const trail = [...at.trail, self];
  return kidsOf(set, self).map((c, i) => {
    const d = elDecl(set, c.el);
    const id = `${under}/${i}`;
    const again = d !== null && trail.includes(d);
    if (d !== null && !again) s.tree.set(id, { el: d, trail });
    return {
      id,
      name: nameOf(set, c.el.elNs, c.el.name, d !== null && c.el.ref !== null ? `el ${c.el.ref}` : null),
      occ: occ(c.min, c.max),
      kids: d !== null && !again && kidsOf(set, d).length > 0,
      again,
      lost: d === null,
      root: false,
    };
  });
}

/**
 * The shortest way down the tree from a root to an element (section 7): the ids of the rows to open, the last the
 * element's own; `[]` where no root reaches it. Breadth first, each element once, at most 20,000 steps.
 */
export function wayDown(s: XsdSession, key: string): string[] {
  const set = s.set;
  if (set === null || !key.startsWith("el ")) return [];
  const target = set.comps.get(key);
  if (target === undefined) return [];
  const queue: { el: ElementDecl; id: string; trail: readonly object[]; way: string[] }[] = set.roots.map((r) => ({ el: set.comps.get(r) as ElementDecl, id: `R${r}`, trail: [], way: [`R${r}`] }));
  const seen = new Set<object>();
  for (let steps = 0; queue.length > 0 && steps < 20_000; steps += 1) {
    const at = queue.shift()!;
    const d = elDecl(set, at.el);
    if (d === null || seen.has(d)) continue;
    seen.add(d);
    s.tree.set(at.id, { el: d, trail: at.trail });
    if (d === target) return at.way;
    kidsOf(set, d).forEach((c, i) => queue.push({ el: c.el, id: `${at.id}/${i}`, trail: [...at.trail, d], way: [...at.way, `${at.id}/${i}`] }));
  }
  return [];
}

const kidsMemo = new WeakMap<object, ReturnType<typeof childElements>>();
function kidsOf(set: SchemaSet, el: ElementDecl): ReturnType<typeof childElements> {
  let got = kidsMemo.get(el);
  if (got === undefined) {
    const t = elType(set, el);
    const sh = t.def === null ? null : shape(set, t.def);
    got = sh === null ? [] : childElements(set, sh.content);
    kidsMemo.set(el, got);
  }
  return got;
}

// ---- The set's index (sections 7, 8 and 13) ----

/** The guessed namespaces *Seen while reading* names: one line each, a handful at most. */
const GUESSES_SHOWN = 3;

const HOW: Record<How, string> = {
  pointed: "you pointed to it",
  downloaded: "downloaded",
  beside: "beside it",
  catalog: "by the catalog",
  name: "by its name",
  namespace: "by its namespace",
  repo: "in another repository",
  bundled: "comes with knobas",
  builtin: "built in",
};
const VERB: Record<string, string> = { import: "imported", include: "included", redefine: "redefined", override: "overridden" };

export function indexOf(set: SchemaSet, lean = false, repo: { total: number; read: number } | null = null): SetIndex {
  const files: FileRow[] = [];
  const lost: LostRow[] = [];
  const counts = new Map<string, number>();
  for (const c of set.comps.values()) counts.set(c.file, (counts.get(c.file) ?? 0) + 1);
  const listing: { kind: "file" | "lost"; at: number }[] = [];
  const listed = new Set<string>();
  type Job = { kind: "file"; path: string; depth: number; ref: { kind: string; how: How | null; from: Elsewhere | null; location: string | null } | null } | { kind: "lost"; row: LostRow };
  const queue: Job[] = [];
  const visit = (path: string, depth: number, ref: { kind: string; how: How | null; from: Elsewhere | null; location: string | null } | null) => {
    const doc = set.docs.find((d) => d.path === path);
    if (doc === undefined || listed.has(path)) return;
    listed.add(path);
    listing.push({ kind: "file", at: files.length });
    const n = counts.get(path) ?? 0;
    files.push({
      path,
      depth,
      ns: doc.tns,
      prefix: doc.tns !== set.home ? (set.prefixes.get(doc.tns) ?? null) : null,
      how: ref === null ? `${count(n)} declarations` : ref.how === null ? (VERB[ref.kind] ?? ref.kind) : `${VERB[ref.kind] ?? ref.kind}, ${ref.how === "repo" && ref.from !== null ? `in ${ref.from.name}, at ${ref.from.branch}` : HOW[ref.how]}`,
      count: n,
      found: ref?.how ?? null,
      adopted: doc.adopted !== null,
      reading: doc.reading,
      bad: doc.bad,
      from: ref?.from ?? null,
      url: ref?.how === "downloaded" ? ref.location : null,
    });
    for (const r of doc.refs) {
      if (r.to !== null && r.how !== "builtin") queue.push({ kind: "file", path: r.to, depth: depth + 1, ref: { kind: r.kind, how: r.how, from: r.from, location: r.location } });
      else if (r.to === null && !r.reading)
        queue.push({ kind: "lost", row: { depth: depth + 1, kind: r.kind, location: r.location, namespace: r.namespace, why: r.why ?? "", address: r.address ?? (r.location !== null && isRemote(r.location) ? r.location : null), two: r.two, by: path } });
    }
  };
  if (set.docs[0] !== undefined) visit(set.docs[0].path, 0, null);
  for (let job = queue.shift(); job !== undefined; job = queue.shift()) {
    if (job.kind === "file") visit(job.path, job.depth, job.ref);
    else {
      listing.push({ kind: "lost", at: lost.length });
      lost.push(job.row);
    }
  }

  const decls: DeclRow[] = [];
  for (const c of lean ? [] : set.comps.values()) {
    const n = nameOfKey(set, c.key);
    let type = "";
    if (c.kind === "el") {
      const t = elType(set, c);
      type = t.key !== null ? labelOfKey(set, t.key).replace(/^xs:/, "") : t.def === null ? "" : "its own";
    } else if (c.kind === "ct") type = "complex";
    else if (c.kind === "st") type = "simple";
    else if (c.kind === "gr") type = "group";
    else if (c.kind === "ag") type = "attributes";
    else type = "attribute";
    const words = c.docu.find((d) => d.text !== "")?.text.split(/(?<=[.!?])\s/)[0] ?? "";
    decls.push({ key: c.key, kind: c.kind, prefix: n.prefix, local: n.local, file: c.file, line: c.line, type, words: words.slice(0, 200) });
  }

  const own = [...set.comps.values()].filter((c) => c.file === set.entry);
  const els = own.filter((c) => c.kind === "el").length;
  const types = own.filter((c) => c.kind === "ct" || c.kind === "st").length;
  const roots = set.roots.map((key) => ({ key, holds: kidsOf(set, set.comps.get(key) as ElementDecl).length }));
  let under: { name: Name; occ: Occurs }[] = [];
  let underOneOf = false;
  if (roots.length === 1) {
    const kids = kidsOf(set, set.comps.get(roots[0]!.key) as ElementDecl).filter((k) => k.el.ref !== null && set.comps.has(`el ${k.el.ref}`));
    if (kids.length > 1 && kids.length <= 16) {
      under = kids.map((k) => ({ name: nameOfKey(set, `el ${k.el.ref}`), occ: occ(k.min, k.max) }));
      underOneOf = kids.every((k) => k.min === 0);
    }
  }

  const seen: Run[][] = [];
  const lostFiles = lost.length;
  if (lostFiles > 0) seen.push([{ text: `${plural(set.missing.size, "name", "names")} from ${plural(lostFiles, "file", "files")} not found here ${set.missing.size === 1 ? "is" : "are"} drawn as written and marked. An example leaves them out.` }]);
  const left = leftoversOf(set);
  if (left.length > 0) {
    const runs: Run[] = [{ text: `${plural(left.length, "element is", "elements are")} declared in this file and held by nothing: ` }];
    left.slice(0, 5).forEach((k, i) => {
      if (i > 0) runs.push({ text: " " });
      runs.push({ text: labelOfKey(set, k), code: true });
    });
    if (left.length > 5) runs.push({ text: " …" });
    seen.push(runs);
  }
  const twice = set.problems.filter((p) => p.kind === "twice").length;
  if (twice > 0) seen.push([{ text: `${plural(twice, "name is", "names are")} declared twice in one namespace; knobas shows the first it reached.` }]);
  const cap = set.problems.find((p) => p.kind === "cap");
  if (cap !== undefined && cap.kind === "cap")
    seen.push([{ text: cap.what === "files" ? "The set reaches more than 400 files; knobas read the first 400." : cap.what === "bytes" ? "The set is larger than 64 MiB; knobas read what fits." : `${baseOf(cap.path)} is larger than 5 MiB, which knobas reads at most.` }]);
  // Where a namespace was guessed: more than one file of the repository declares it, and the nearest was taken.
  const guessed = new Map<string, { to: string; others: number }>();
  for (const d of set.docs) for (const r of d.refs) if (r.to !== null && r.others.length > 0 && r.namespace !== null && !guessed.has(r.namespace)) guessed.set(r.namespace, { to: r.to, others: r.others.length });
  for (const [namespace, g] of [...guessed].slice(0, GUESSES_SHOWN))
    seen.push([{ text: `${plural(g.others + 1, "file", "files")} here declare ` }, { text: namespace, code: true }, { text: `; knobas took the nearest, ${baseOf(g.to)}.` }]);
  if (repo !== null && repo.read < repo.total) seen.push([{ text: `knobas looked at ${count(repo.read)} of ${count(repo.total)} .xsd files here.` }]);
  if (set.docs.some((d) => d.v11)) seen.push([{ text: "A file uses XML Schema 1.1. knobas draws it; the checker (libxml2) reads 1.0 and will say so." }]);
  if (set.need.length === 0 && ![...set.comps.values()].some((c) => c.docu.length > 0)) seen.push([{ text: "These files hold no documentation. Names, types and the file's own first comment are what there is." }]);
  for (const d of set.docs) for (const p of d.problems.slice(0, 3)) seen.push([{ text: `${baseOf(d.path)}, line ${count(p.line)}: ${p.text}` }]);

  const first = set.docs[0];
  return {
    listing,
    entry: set.entry,
    home: set.home,
    version: first?.version ?? null,
    v11: set.docs.some((d) => d.v11),
    head: first?.head ?? null,
    files,
    lost,
    decls,
    roots,
    under,
    underOneOf,
    seen,
    ownElements: els,
    ownTypes: types,
    total: set.comps.size,
    reading: set.docs.filter((d) => d.reading).length,
    ms: { read: Math.round(set.times.read), compile: Math.round(set.times.compile) },
    perFile: (lean ? [] : files).map((f) => ({
      path: f.path,
      elements: [...set.comps.values()].filter((c) => c.file === f.path && c.kind === "el").length,
      types: [...set.comps.values()].filter((c) => c.file === f.path && c.kind !== "el").length,
    })),
  };
}

export { isBuiltin };
