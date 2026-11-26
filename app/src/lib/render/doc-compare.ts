/**
 * XML and JSON compared by element (the XML view's design, 2026-11-23, section 28; step C1): one list of the elements
 * (or JSON values) that differ, by their path, on S7's list. Formatting is no change: indentation and white space
 * between elements, attribute order, quotes, a prefix spelled differently (a name is its namespace and local name),
 * `<a/>` against `<a></a>`, a JSON object's key order. *Changed* says both values; *Added* and *Removed* name the
 * topmost part that came or went; *Moved* is an element or array item that stands elsewhere among its siblings.
 *
 * - **Matching** goes level by level. Siblings are matched by name (a JSON member by its key), then by an attribute or
 *   field that names them (`id`, `key`, `name`, `code`), then whole: a subtree the same on both sides is the same one
 *   wherever it stands. What is left is matched in order.
 * - **Moved** is a matched sibling outside the longest run that kept its order (an object's members never move).
 *
 * Pure: it reads two outlines (`xml-outline.ts`, `json-outline.ts`), in the document worker.
 */
import { CDATA, EL, TEXT, decode, readXml, type XmlOutline } from "./xml-outline";
import { J_ARR, J_OBJ, readJson, unquote, type JsonOutline } from "./json-outline";

/** A part of a document, read for comparing. */
interface Part {
  /** What it is matched by among its siblings: an element's namespace and local name, a member's key, `[]` an item. */
  readonly kind: string;
  /** How it is written in a path: an element's name as written, a member's key. */
  readonly label: string;
  /** What names it among siblings of its kind (`@id` of an element, `id` of an object item), or `null`. */
  readonly ident: string | null;
  /** Attributes by namespace and local name: their name as written and value. */
  readonly attrs: Map<string, { name: string; value: string }>;
  /** Its own value: an element's text (trimmed), a JSON scalar as written; `null` where it holds parts. */
  readonly value: string | null;
  readonly kids: Part[];
  readonly line: number;
  /** A hash of the whole subtree, formatting left out. */
  hash: number;
  /** An array or object (JSON): an object's members never move. */
  readonly ordered: boolean;
}

/** One fact that differs on a changed part. */
export interface DocFact {
  readonly place: string;
  readonly before: string | null;
  readonly after: string | null;
}

export interface DocRow {
  readonly mark: "changed" | "added" | "removed" | "moved";
  /** Its path on its side: `/ORDER/ORDER_ITEM_LIST/ORDER_ITEM[2]/QUANTITY`, `.orders[3].total`. */
  readonly path: string;
  readonly line: number;
  /** The line on the other side, for a changed or moved part. */
  readonly otherLine: number | null;
  readonly facts: readonly DocFact[];
  /** What changed, in one line. */
  readonly say: string;
}

export interface DocChanges {
  readonly rows: readonly DocRow[];
  readonly changed: number;
  readonly added: number;
  readonly removed: number;
  readonly moved: number;
  /** Parts that read the same, however they are written. */
  readonly same: number;
  /** Each side's reading: `null` where it is the kind asked, else why not. */
  readonly bad: { readonly old: string | null; readonly new: string | null };
  readonly ms: number;
}

/** The most of a side compared by element: past it, Lines has the change. */
export const COMPARE_CAP = 8 * 1024 * 1024;
/** The rows answered: a document of thousands of changes says how many more. */
export const ROWS_CAP = 2000;
const IDENTS = ["id", "ID", "Id", "key", "name", "code"];

const hashStr = (s: string, h = 0x811c9dc5): number => {
  for (let i = 0; i < s.length; i += 1) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h;
};

/** An XML document's parts: elements, their attributes (namespace declarations left out) and their trimmed text. */
export function xmlParts(o: XmlOutline): Part | null {
  if (o.root < 0) return null;
  const build = (row: number): Part => {
    const attrs = new Map<string, { name: string; value: string }>();
    for (let a = o.a0[row]!; a < o.a0[row]! + o.an[row]!; a += 1) {
      const name = o.names[o.aName[a]!]!;
      if (name === "xmlns" || name.startsWith("xmlns:")) continue;
      attrs.set(`${o.uris[o.aNs[a]!] ?? ""}|${o.names[o.aLocal[a]!]}`, { name, value: decode(o.text.slice(o.aStart[a]!, o.aEnd[a]!)) });
    }
    const kids: Part[] = [];
    let text = "";
    for (let c = o.first[row]!; c !== -1; c = o.next[c]!) {
      const t = o.type[c]!;
      if (t === EL) kids.push(build(c));
      else if (t === TEXT) text += decode(o.text.slice(o.start[c]!, o.end[c]!));
      else if (t === CDATA) text += o.text.slice(o.open[c]!, o.close[c]!);
    }
    const value = text.trim();
    let ident: string | null = null;
    for (const k of IDENTS) {
      const hit = attrs.get(`|${k}`);
      if (hit !== undefined) {
        ident = `@${k}=${hit.value}`;
        break;
      }
    }
    return { kind: `${o.uris[o.ns[row]!] ?? ""}|${o.names[o.local[row]!]}`, label: o.names[o.name[row]!]!, ident, attrs, value: kids.length > 0 && value === "" ? null : value, kids, line: o.line[row]!, hash: 0, ordered: true };
  };
  return build(o.root);
}

const OBJ = J_OBJ;
const ARR = J_ARR;
/** A JSON document's parts: members by key, items by place, scalars as written. */
export function jsonParts(o: JsonOutline): Part | null {
  if (o.root < 0) return null;
  const build = (row: number, label: string, kind: string): Part => {
    const t = o.type[row]!;
    const kids: Part[] = [];
    let value: string | null = null;
    if (t === OBJ || t === ARR) {
      for (let c = o.first[row]!; c !== -1; c = o.next[c]!) {
        const key = o.key[c]!;
        kids.push(key >= 0 ? build(c, o.keys[key]!, `.${o.keys[key]}`) : build(c, "", "[]"));
      }
    } else {
      const raw = o.text.slice(o.start[row]!, o.end[row]!);
      // A number is its value (`1.0` and `1` are one), a string its text unescaped.
      value = raw.startsWith('"') ? JSON.stringify(unquote(raw)) : /^-?\d/.test(raw) ? String(Number(raw)) : raw;
    }
    let ident: string | null = null;
    if (t === OBJ)
      for (const k of IDENTS) {
        const hit = kids.find((p) => p.label === k && p.value !== null);
        if (hit !== undefined) {
          ident = `${k}=${hit.value}`;
          break;
        }
      }
    return { kind, label, ident, attrs: new Map(), value, kids, line: o.line[row]!, hash: 0, ordered: t === ARR };
  };
  return build(o.root, "", "");
}

/** Hashes each subtree bottom-up; attribute order and a member's place in its object are left out. */
function hashAll(p: Part): number {
  let h = hashStr(p.kind);
  h = hashStr(p.value ?? "\u0000", h);
  const attrs = [...p.attrs].map(([k, v]) => `${k}=${v.value}`).sort();
  for (const a of attrs) h = hashStr(a, h);
  const kidHashes = p.kids.map(hashAll);
  // An object's members are a set: their order is formatting.
  if (!p.ordered && p.kids.some((k) => k.kind.startsWith("."))) kidHashes.sort((x, y) => x - y);
  for (const k of kidHashes) h = Math.imul(h ^ k, 0x01000193) >>> 0;
  p.hash = h;
  return h;
}

/** The longest increasing subsequence's members, as a set of indices into `seq`. */
function keptOrder(seq: readonly number[]): Set<number> {
  const tails: number[] = [];
  const prev = new Array<number>(seq.length).fill(-1);
  for (let i = 0; i < seq.length; i += 1) {
    let lo = 0;
    let hi = tails.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (seq[tails[mid]!]! < seq[i]!) lo = mid + 1;
      else hi = mid;
    }
    if (lo > 0) prev[i] = tails[lo - 1]!;
    tails[lo] = i;
  }
  const out = new Set<number>();
  for (let i = tails.length > 0 ? tails[tails.length - 1]! : -1; i !== -1; i = prev[i]!) out.add(i);
  return out;
}

/** Matches two lists of siblings: pairs of indices, by kind, then name, then whole, then in order. */
export function matchKids(a: readonly Part[], b: readonly Part[]): [number, number][] {
  const pairs: [number, number][] = [];
  const takenA = new Set<number>();
  const takenB = new Set<number>();
  const byKind = (list: readonly Part[], taken: Set<number>) => {
    const m = new Map<string, number[]>();
    list.forEach((p, i) => {
      if (taken.has(i)) return;
      const k = p.kind;
      const l = m.get(k);
      if (l === undefined) m.set(k, [i]);
      else l.push(i);
    });
    return m;
  };
  const pass = (keyOf: (p: Part) => string | null) => {
    const left = new Map<string, number[]>();
    a.forEach((p, i) => {
      if (takenA.has(i)) return;
      const k = keyOf(p);
      if (k === null) return;
      const l = left.get(k);
      if (l === undefined) left.set(k, [i]);
      else l.push(i);
    });
    b.forEach((p, j) => {
      if (takenB.has(j)) return;
      const k = keyOf(p);
      if (k === null) return;
      const l = left.get(k);
      const i = l?.shift();
      if (i === undefined) return;
      takenA.add(i);
      takenB.add(j);
      pairs.push([i, j]);
    });
  };
  // A JSON member is its key, one of a kind: matched by it alone.
  pass((p) => (p.kind.startsWith(".") ? p.kind : null));
  pass((p) => (p.ident === null ? null : `${p.kind}\u0000${p.ident}`));
  pass((p) => `${p.kind}\u0000#${p.hash}`);
  // What is left, of each kind, in order.
  const leftA = byKind(a, takenA);
  const leftB = byKind(b, takenB);
  for (const [kind, js] of leftB) {
    const is = leftA.get(kind) ?? [];
    for (let n = 0; n < Math.min(is.length, js.length); n += 1) {
      takenA.add(is[n]!);
      takenB.add(js[n]!);
      pairs.push([is[n]!, js[n]!]);
    }
  }
  return pairs.sort((x, y) => x[1] - y[1]);
}

/** Compares two documents' parts; `xml` writes paths as XPath steps, else as jq paths. */
export function compareParts(a: Part, b: Part, xml: boolean): Omit<DocChanges, "bad" | "ms"> {
  hashAll(a);
  hashAll(b);
  const rows: DocRow[] = [];
  let same = 0;
  let changed = 0;
  let added = 0;
  let removed = 0;
  let moved = 0;
  const say = (facts: readonly DocFact[]) =>
    facts
      .slice(0, 3)
      .map((f) => `${f.place}: ${f.before === null ? "added" : f.after === null ? "removed" : `${clip(f.before)} → ${clip(f.after)}`}`)
      .join(" · ");
  const stepOf = (p: Part, siblings: readonly Part[], at: string): string => {
    if (!xml) return p.kind === "[]" ? `${at}[${siblings.indexOf(p)}]` : `${at}${/^[A-Za-z_][\w]*$/.test(p.label) ? `.${p.label}` : `[${JSON.stringify(p.label)}]`}`;
    const same = siblings.filter((s) => s.kind === p.kind);
    return `${at}/${p.label}${same.length > 1 ? `[${same.indexOf(p) + 1}]` : ""}`;
  };
  const walk = (pa: Part, pb: Part, pathA: string, pathB: string) => {
    if (pa.hash === pb.hash) {
      same += 1 + countAll(pb);
      return;
    }
    const facts: DocFact[] = [];
    if (pa.value !== pb.value && (pa.value !== null || pb.value !== null)) facts.push({ place: xml ? "text" : "value", before: pa.value, after: pb.value });
    for (const [k, v] of pb.attrs) {
      const was = pa.attrs.get(k);
      if (was === undefined) facts.push({ place: `@${v.name}`, before: null, after: v.value });
      else if (was.value !== v.value) facts.push({ place: `@${v.name}`, before: was.value, after: v.value });
    }
    for (const [k, v] of pa.attrs) if (!pb.attrs.has(k)) facts.push({ place: `@${v.name}`, before: v.value, after: null });
    if (facts.length > 0) {
      changed += 1;
      rows.push({ mark: "changed", path: pathB === "" ? (xml ? "/" : ".") : pathB, line: pb.line, otherLine: pa.line, facts, say: say(facts) });
    } else same += 1;
    const pairs = matchKids(pa.kids, pb.kids);
    const matchedA = new Set(pairs.map(([i]) => i));
    // Moved: a matched part outside the longest run that kept its order; an object's members keep none.
    const ordered = pb.ordered && pa.ordered;
    const kept = ordered ? keptOrder(pairs.map(([i]) => i)) : null;
    // In the side after's order: each part matched, moved or added where it now stands; what went, after.
    const pairOf = new Map(pairs.map(([i, j], n) => [j, { i, n }]));
    pb.kids.forEach((kb, j) => {
      const sb = stepOf(kb, pb.kids, pathB);
      const pair = pairOf.get(j);
      if (pair === undefined) {
        added += 1;
        rows.push({ mark: "added", path: sb, line: kb.line, otherLine: null, facts: [], say: describe(kb, xml) });
        return;
      }
      const ka = pa.kids[pair.i]!;
      if (kept !== null && !kept.has(pair.n)) {
        moved += 1;
        rows.push({ mark: "moved", path: sb, line: kb.line, otherLine: ka.line, facts: [], say: `${nth(pair.i + 1)} → ${nth(j + 1)}` });
      }
      walk(ka, kb, stepOf(ka, pa.kids, pathA), sb);
    });
    pa.kids.forEach((ka, i) => {
      if (matchedA.has(i)) return;
      removed += 1;
      rows.push({ mark: "removed", path: stepOf(ka, pa.kids, pathA), line: ka.line, otherLine: null, facts: [], say: describe(ka, xml) });
    });
  };
  if (a.kind !== b.kind) {
    rows.push({ mark: "removed", path: xml ? `/${a.label}` : ".", line: a.line, otherLine: null, facts: [], say: describe(a, xml) });
    rows.push({ mark: "added", path: xml ? `/${b.label}` : ".", line: b.line, otherLine: null, facts: [], say: describe(b, xml) });
    return { rows, changed: 0, added: 1, removed: 1, moved: 0, same: 0 };
  }
  // The rows stand in the walk's order: the document's, a part's own change before what it holds.
  walk(a, b, xml ? `/${a.label}` : "", xml ? `/${b.label}` : "");
  return { rows, changed, added, removed, moved, same };
}

/** A place among siblings in words: `1st`, `2nd`, `11th`. */
export const nth = (n: number): string => `${n}${n % 100 >= 11 && n % 100 <= 13 ? "th" : (["th", "st", "nd", "rd"][n % 10] ?? "th")}`;
const clip = (s: string) => (s.length > 60 ? `${s.slice(0, 57)}…` : s);
function countAll(p: Part): number {
  let n = 0;
  for (const k of p.kids) n += 1 + countAll(k);
  return n;
}
/** A part that came or went, in a few words: what it holds. */
function describe(p: Part, xml: boolean): string {
  const n = countAll(p);
  if (p.value !== null && p.kids.length === 0) return clip(p.value === "" ? (xml ? "empty" : '""') : p.value);
  return n === 0 ? "empty" : `${n.toLocaleString("en")} ${n === 1 ? "part" : "parts"} inside`;
}

/** Two texts compared by element: XML or JSON, each side read on its own; a side that is not read says why. */
export function compareDocs(before: string | null, after: string | null, kind: "xml" | "json"): DocChanges {
  const t0 = performance.now();
  const read = (text: string | null): { part: Part | null; bad: string | null } => {
    if (text === null) return { part: null, bad: null };
    if (text.length > COMPARE_CAP) return { part: null, bad: "too large to compare by element" };
    if (kind === "xml") {
      const o = readXml(text);
      if (o.error !== null) return { part: null, bad: `not well formed: ${o.error.message} (line ${o.error.line})` };
      return { part: xmlParts(o), bad: null };
    }
    const o = readJson(text);
    if (o.error !== null) return { part: null, bad: `not JSON: ${o.error.message} (line ${o.error.line})` };
    return { part: jsonParts(o), bad: null };
  };
  const a = read(before);
  const b = read(after);
  const bad = { old: a.bad, new: b.bad };
  const xml = kind === "xml";
  if (a.bad !== null || b.bad !== null) return { rows: [], changed: 0, added: 0, removed: 0, moved: 0, same: 0, bad, ms: performance.now() - t0 };
  if (a.part === null || b.part === null) {
    const one = (a.part ?? b.part)!;
    hashAll(one);
    const mark = a.part === null ? "added" : "removed";
    const rows: DocRow[] = one === null ? [] : [{ mark, path: xml ? `/${one.label}` : ".", line: one.line, otherLine: null, facts: [], say: describe(one, xml) }];
    return { rows, changed: 0, added: mark === "added" ? 1 : 0, removed: mark === "removed" ? 1 : 0, moved: 0, same: 0, bad, ms: performance.now() - t0 };
  }
  const got = compareParts(a.part, b.part, xml);
  return { ...got, rows: got.rows.slice(0, ROWS_CAP), bad, ms: performance.now() - t0 };
}
