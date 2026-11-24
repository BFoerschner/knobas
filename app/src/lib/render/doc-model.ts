/**
 * The document worker's half of the XML view (its design, 2026-11-23, sections 7 to 9 and 27): each open document's
 * outline, kept by its text, and the answers the view asks of it. The view holds a `DocSession` (`doc-port.ts`); this
 * runs in `doc.worker.ts`, and on the page's thread in tests and where a worker cannot start.
 *
 * What crosses to the page is small and asked for: once, the fold table (a few columns); on a pick, the way down and
 * what the picked element holds; on leaving a file, its folds' names. The outline itself never crosses.
 */
import { XSI_NS } from "./xml-names";
import { layOutXml } from "./doc-line";
import { CHARACTERS, ELEMENTS, LINES, type FoldTable } from "./doc-folds";
import { COMMENT, EL, TEXT, CDATA, PI, decode, lineOfNode, nameOf, namePathOf, nodeAtPath, pathOf, readXml, stepOf, stringOf, type XmlOutline } from "./xml-outline";
import { XPathError, compile, describe, prefixesOf, run, type Answer } from "./xml-xpath";

/** One crumb of the way down: a name, and the line it stands on. */
export interface Crumb {
  name: string;
  line: number;
}

/** What the picked line holds (section 9). */
export interface Picked {
  /** Its row. */
  node: number;
  name: string;
  /** *Element*, *Comment*, … */
  what: string;
  line: number;
  /** An XPath that finds it alone. */
  path: string;
  namespace: string | null;
  attributes: { name: string; value: string }[];
  /** What it holds, counted by name: the first fourteen names, and how many more. */
  holds: { name: string; count: number }[];
  holdsMore: number;
  /** Its value cut at 240 characters, and how long it is; `null` where it holds elements. */
  value: string | null;
  valueLength: number;
}

/** What `open` answers: the document's shape, the fold table, and what its root says. */
export interface DocOpened {
  lines: number;
  nodes: number;
  folds: FoldTable;
  error: { message: string; line: number } | null;
  /** The root's name, its namespace, and where it says its schema is. */
  root: { name: string; namespace: string; location: string | null } | null;
  /** Whether the root stands under a default namespace (the query's hint, section 21). */
  defaultNs: boolean;
}

/** One call of a session. */
export type DocCall =
  | { kind: "open"; text: string }
  | { kind: "at"; line: number }
  | { kind: "keys"; lines: number[] }
  | { kind: "find"; keys: string[] }
  | { kind: "query"; source: string }
  | { kind: "results"; from: number; count: number }
  | { kind: "copy"; what: CopyWhat }
  | { kind: "examples" }
  | { kind: "like"; line: number }
  | { kind: "values"; lines: number[]; all: boolean }
  | { kind: "layout" }
  | { kind: "close" };

/** One row of a query's results, as the rail draws it. */
export interface ResultRow {
  /** The node: its row, or the row count plus its attribute row. */
  node: number;
  line: number;
  name: string;
  /** Its value cut at {@link VALUE_CUT}, or `null` for an element that holds elements. */
  value: string | null;
  /** How many elements it holds, where it holds some. */
  kids: number;
}

/** What a query answered (section 22). */
export type QueryAnswer =
  | { state: "nodes"; count: number; rows: ResultRow[]; lines: Uint32Array; ms: number }
  | { state: "value"; type: "number" | "string" | "boolean"; text: string; ms: number }
  | { state: "error"; message: string; at: number }
  | { state: "stopped"; message: string; ms: number };

/** A value of the document on a line, where a value pipeline's `Take` finds it (section 16). */
export interface ValueAt {
  line: number;
  node: number;
  /** The element's or attribute's name, as written. */
  name: string;
  /** The names down to it, no positions: what a kept chain is found by. `MIME_EMBEDDED/MIME_DATA`. */
  field: string;
  /** Where its bytes are in the file, counted from after a byte-order mark. */
  from: number;
  to: number;
  read: "xml_text" | "xml_attribute";
  /** Its start, references read: 4 KB at most. */
  head: string;
  /** Its whole length in characters, and without its white space. */
  length: number;
  bare: number;
  /** What the file says of it. */
  said: { mime?: string; base64?: boolean };
  /** A name the document gives beside it: openTRANS's `FILE_NAME`, a `filename` attribute. */
  fileName: string | null;
}

/** What *Copy ▾* copies of a query's results. */
export type CopyWhat = "written" | "values" | "lines";

/** Results handed to the page at a time (section 27). */
export const RESULTS_AT_ONCE = 2000;
/** The most of a value a result's row says. */
export const VALUE_CUT = 120;
/** A query may look at 50 rows for every row of the document, at least 4 million, and at most 1.5 s (section 22). */
export const LOOKS_A_ROW = 50;
export const LOOKS_AT_LEAST = 4_000_000;
export const QUERY_MS = 1500;

/** A document kept: its outline and, by line, the first element on each line. */
export interface Kept {
  /** The file as written, where this is its layout (section 6): a value's place is found there, by its row. */
  origin?: Kept;
  /** UTF-8 bytes before every 4,096th character: a character's place made a byte's quickly. */
  bytesAt?: Uint32Array;
  outline: XmlOutline;
  byLine: Int32Array;
  folds: FoldTable;
  /** Each fold's row. */
  foldRows: Int32Array;
}

/** The outlines read lately, by text, the newest last: a file opened again asks nothing of the reader. */
const recent: { text: string; kept: Kept }[] = [];
/** Section 32, risk 12: four documents or 256 MB of text, whichever is less. */
const RECENT = 4;
const RECENT_TEXT = 256 * 1024 * 1024;

function keep(text: string): Kept {
  const at = recent.findIndex((r) => r.text.length === text.length && r.text === text);
  if (at !== -1) {
    const [hit] = recent.splice(at, 1);
    recent.push(hit!);
    return hit!.kept;
  }
  const outline = readXml(text);
  const foldRows = foldingRows(outline);
  const kept: Kept = { outline, byLine: elementsByLine(outline), folds: foldTable(outline, foldRows), foldRows };
  recent.push({ text, kept });
  let held = recent.reduce((sum, r) => sum + r.text.length, 0);
  while (recent.length > RECENT || (held > RECENT_TEXT && recent.length > 1)) held -= recent.shift()!.text.length;
  return kept;
}

/** The first element that starts on each line (from 1), or -1. */
function elementsByLine(o: XmlOutline): Int32Array {
  const by = new Int32Array(o.lines + 2).fill(-1);
  for (let i = 1; i < o.n; i += 1) {
    if (o.type[i] !== EL) continue;
    const l = o.line[i]!;
    if (by[l] === -1) by[l] = i;
  }
  return by;
}

/** The rows that fold, in order of their line: the first element or comment that starts on a line and ends on a later one. */
export function foldingRows(o: XmlOutline): Int32Array {
  const taken = new Uint8Array(o.lines + 2);
  let n = 0;
  const rows = new Int32Array(Math.max(0, o.lines));
  for (let i = 1; i < o.n; i += 1) {
    const t = o.type[i];
    if (t !== EL && t !== COMMENT) continue;
    const l = o.line[i]!;
    if (Math.min(o.endLine[i]!, o.lines) > l && taken[l] === 0) {
      taken[l] = 1;
      rows[n++] = i;
    }
  }
  return rows.subarray(0, n);
}

/** What folds, as the page reads it (`doc-folds.ts`). */
export function foldTable(o: XmlOutline, rows: Int32Array = foldingRows(o)): FoldTable {
  const byLine = new Int32Array(o.lines + 2).fill(-1);
  const n = rows.length;
  for (let f = 0; f < n; f += 1) byLine[o.line[rows[f]!]!] = f;
  const line = new Uint32Array(n);
  const end = new Uint32Array(n);
  const depth = new Uint16Array(n);
  const count = new Uint32Array(n);
  const unit = new Uint8Array(n);
  const closer = new Uint32Array(n);
  const closers: string[] = [];
  const closerId = new Map<string, number>();
  const idOf = (s: string) => {
    let id = closerId.get(s);
    if (id === undefined) {
      id = closers.length;
      closers.push(s);
      closerId.set(s, id);
    }
    return id;
  };
  for (let f = 0; f < n; f += 1) {
    const i = rows[f]!;
    line[f] = o.line[i]!;
    end[f] = Math.min(o.endLine[i]!, o.lines);
    depth[f] = o.depth[i]!;
    if (o.type[i] === COMMENT) {
      count[f] = end[f]! - line[f]! + 1;
      unit[f] = LINES;
      closer[f] = idOf("-->");
    } else if (o.close[i] === o.end[i] && o.close[i] === o.open[i]) {
      // A self-closed tag written over several lines: it hides the rest of its attributes.
      count[f] = end[f]! - line[f]!;
      unit[f] = LINES;
      closer[f] = idOf("/>");
    } else if (o.kids[i]! > 0) {
      count[f] = o.kids[i]!;
      unit[f] = ELEMENTS;
      closer[f] = idOf(`</${nameOf(o, i)}>`);
    } else {
      count[f] = o.close[i]! - o.open[i]!;
      unit[f] = CHARACTERS;
      closer[f] = idOf(`</${nameOf(o, i)}>`);
    }
  }
  return { lines: o.lines, byLine, line, end, depth, count, unit, closer, closers };
}

/** The node a line stands for: the element that starts on it, else the innermost one it lies inside; -1 for none. */
export function nodeOnLine(o: XmlOutline, byLine: Int32Array, line: number): number {
  const el = byLine[line] ?? -1;
  if (el !== -1) return el;
  for (let l = line - 1; l >= 1; l -= 1) {
    const e = byLine[l]!;
    if (e === -1) continue;
    for (let at = e; at > 0; at = o.parent[at]!) if (o.type[at] === EL && o.endLine[at]! >= line) return at;
    return -1;
  }
  return -1;
}

/** The way down to a node: each element above it and itself, its position written where it has siblings of its name. */
export function crumbsOf(o: XmlOutline, node: number): Crumb[] {
  const out: Crumb[] = [];
  for (let at = node; at > 0; at = o.parent[at]!) if (o.type[at] === EL) out.push({ name: stepOf(o, at), line: o.line[at]! });
  return out.reverse();
}

const WHAT: Record<number, string> = { [EL]: "Element", [TEXT]: "Text", [CDATA]: "CDATA", [COMMENT]: "Comment", [PI]: "Instruction" };
/** The most of a value the picked element shows. */
export const VALUE_SHOWN = 240;
/** The most attributes the picked element lists. */
export const ATTRIBUTES_SHOWN = 40;

/** What node `node` holds, for the column beside the document. */
export function pickedOf(o: XmlOutline, node: number): Picked {
  const attributes: { name: string; value: string }[] = [];
  for (let a = o.a0[node]!, e = a + o.an[node]!; a < e; a += 1) {
    const name = o.names[o.aName[a]!]!;
    if (name === "xmlns" || name.startsWith("xmlns:")) continue;
    if (attributes.length === ATTRIBUTES_SHOWN) break;
    attributes.push({ name, value: stringOf(o, o.n + a).slice(0, 120) });
  }
  const by = new Map<number, number>();
  for (let c = o.first[node]!; c !== -1; c = o.next[c]!) if (o.type[c] === EL) by.set(o.name[c]!, (by.get(o.name[c]!) ?? 0) + 1);
  const holds = [...by].slice(0, 14).map(([id, count]) => ({ name: o.names[id]!, count }));
  let value: string | null = null;
  let valueLength = 0;
  if (o.kids[node] === 0) {
    const whole = stringOf(o, node).trim();
    valueLength = whole.length;
    value = whole.length > VALUE_SHOWN ? `${whole.slice(0, VALUE_SHOWN - 3)}…` : whole;
  }
  const ns = o.type[node] === EL ? o.uris[o.ns[node]!]! : "";
  return {
    node,
    name: nameOf(o, node),
    what: WHAT[o.type[node]!] ?? "Node",
    line: o.line[node]!,
    path: pathOf(o, node),
    namespace: ns === "" ? null : ns,
    attributes,
    holds,
    holdsMore: Math.max(0, by.size - 14),
    value,
    valueLength,
  };
}

/** Where the root says its schema is: the location paired with its namespace, or the no-namespace one. */
function locationOf(o: XmlOutline, root: number, namespace: string): string | null {
  for (let a = o.a0[root]!, e = a + o.an[root]!; a < e; a += 1) {
    const local = o.names[o.aLocal[a]!];
    if (o.uris[o.aNs[a]!] !== XSI_NS) continue;
    const value = stringOf(o, o.n + a).trim();
    if (local === "noNamespaceSchemaLocation" && value !== "") return value;
    if (local === "schemaLocation") {
      const parts = value.split(/\s+/);
      for (let k = 0; k + 1 < parts.length; k += 2) if (parts[k] === namespace) return parts[k + 1]!;
      if (parts.length >= 2) return parts[1]!;
    }
  }
  return null;
}

function opened(k: Kept): DocOpened {
  const o = k.outline;
  const root = o.root;
  const namespace = root === -1 ? "" : o.uris[o.ns[root]!]!;
  return {
    lines: o.lines,
    nodes: o.n - 1 + o.attrs,
    folds: k.folds,
    error: o.error,
    root: root === -1 ? null : { name: nameOf(o, root), namespace, location: locationOf(o, root, namespace) },
    defaultNs: root !== -1 && (o.scopes[o.scope[root]!]![""] ?? "") !== "",
  };
}

/** A session: its document, and its last query's nodes, for the rows asked for as the list scrolls and for a copy. */
export interface DocSessionState {
  k: Kept;
  last: number[];
}

/** Each session's document. */
export type DocSessions = Map<number, DocSessionState>;

/** Answers one call of session `session`: in the worker, or on this thread. */
export function answerDoc(sessions: DocSessions, session: number, call: DocCall): unknown {
  if (call.kind === "open") {
    const k = keep(call.text);
    sessions.set(session, { k, last: [] });
    return opened(k);
  }
  if (call.kind === "close") {
    sessions.delete(session);
    return null;
  }
  const held = sessions.get(session);
  if (held === undefined) return null;
  const k = held.k;
  const o = k.outline;
  if (call.kind === "at") {
    const node = nodeOnLine(o, k.byLine, call.line);
    if (node <= 0) return { crumbs: [], picked: null };
    return { crumbs: crumbsOf(o, node), picked: pickedOf(o, node) };
  }
  if (call.kind === "keys") {
    // A fold's name across versions of the file: the names down to its element, with positions.
    return call.lines.map((line) => {
      const f = k.folds.byLine[line] ?? -1;
      return f === -1 ? null : pathOf(o, k.foldRows[f]!);
    });
  }
  if (call.kind === "query") {
    const answer = query(o, call.source);
    held.last = answer.ids;
    return answer.answer;
  }
  if (call.kind === "results") return held.last.slice(call.from, call.from + call.count).map((id) => resultRow(o, id));
  if (call.kind === "copy") return copyOf(o, held.last, call.what);
  if (call.kind === "examples") return examplesOf(o);
  if (call.kind === "like") {
    const node = nodeOnLine(o, k.byLine, call.line);
    if (node <= 0) return null;
    const up = o.parent[node]!;
    return up > 0 && o.type[up] === EL ? `//${nameOf(o, up)}/${nameOf(o, node)}` : `//${nameOf(o, node)}`;
  }
  if (call.kind === "layout") {
    // The session goes on over the document laid out: its rows are the file's, in the same order.
    const laid = layOutXml(o);
    const next = keep(laid);
    if (next !== k) next.origin = k.origin ?? k;
    held.k = next;
    held.last = [];
    return { text: laid, opened: opened(next) };
  }
  if (call.kind === "values") {
    const out: Record<number, ValueAt[]> = {};
    for (const line of call.lines) out[line] = valuesOn(k, line, call.all);
    return out;
  }
  if (call.kind === "find") {
    // The folds found again by their elements, as the lines they start on now.
    const out: number[] = [];
    for (const key of call.keys) {
      const node = nodeAtPath(o, key);
      if (node <= 0 || node >= o.n) continue;
      const l = o.line[node]!;
      const f = k.folds.byLine[l] ?? -1;
      if (f !== -1 && k.foldRows[f] === node) out.push(l);
    }
    return out;
  }
  return null;
}

/** One result as the rail draws it. */
export function resultRow(o: XmlOutline, id: number): ResultRow {
  const d = describe(o, id);
  const value = d.value === null ? null : d.value.replace(/\s+/g, " ").trim();
  return { node: id, line: lineOfNode(o, id), name: d.name, value: value === null ? null : value.length > VALUE_CUT ? `${value.slice(0, VALUE_CUT - 1)}…` : value, kids: d.kids };
}

/** Answers a query on a document, under its budget: what the page is handed, and every node it found. */
export function query(o: XmlOutline, source: string): { answer: QueryAnswer; ids: number[] } {
  const t0 = performance.now();
  let answer: Answer;
  try {
    answer = run(o, compile(source), { budget: Math.max(LOOKS_AT_LEAST, LOOKS_A_ROW * (o.n + o.attrs)), deadline: t0 + QUERY_MS, prefixes: prefixesOf(o).map });
  } catch (cause) {
    if (!(cause instanceof XPathError)) throw cause;
    if (cause.budget) return { answer: { state: "stopped", message: cause.message, ms: performance.now() - t0 }, ids: [] };
    return { answer: { state: "error", message: cause.message, at: cause.at }, ids: [] };
  }
  const ms = performance.now() - t0;
  if (answer.type !== "nodes") return { answer: { state: "value", type: answer.type, text: answer.text, ms }, ids: [] };
  const seen = new Set<number>();
  for (const id of answer.ids) if (id !== 0) seen.add(lineOfNode(o, id));
  const lines = Uint32Array.from(seen).sort();
  return { answer: { state: "nodes", count: answer.ids.length, rows: answer.ids.slice(0, RESULTS_AT_ONCE).map((id) => resultRow(o, id)), lines, ms }, ids: answer.ids };
}

/** What *Copy ▾* copies: each node as the file writes it, the values one a line, or the lines' numbers. */
export function copyOf(o: XmlOutline, ids: number[], what: CopyWhat): string {
  if (what === "lines") return [...new Set(ids.map((id) => lineOfNode(o, id)))].join("\n");
  if (what === "values") return ids.map((id) => stringOf(o, id).replace(/\s+/g, " ").trim()).join("\n");
  return ids
    .map((id) => {
      if (id >= o.n) return `${nameOf(o, id)}="${o.text.slice(o.aStart[id - o.n], o.aEnd[id - o.n])}"`;
      return o.text.slice(o.start[id], o.end[id]);
    })
    .join("\n");
}

/** Queries to press, made from the document's own names: its most repeated element, an attribute it uses. */
export function examplesOf(o: XmlOutline): string[] {
  const count = new Map<number, number>();
  const holds = new Map<number, number>();
  for (let i = 1; i < o.n; i += 1) {
    if (o.type[i] !== EL || i === o.root) continue;
    count.set(o.name[i]!, (count.get(o.name[i]!) ?? 0) + 1);
    if (o.kids[i]! > 2) holds.set(o.name[i]!, 1);
  }
  const top = [...count].sort((a, b) => b[1] - a[1]).map(([id]) => id);
  const rep = top.find((id) => holds.has(id) && count.get(id)! > 1) ?? top[0];
  const out: string[] = [];
  if (rep === undefined) return out;
  const name = o.names[rep]!;
  out.push(`//${name}`, `count(//${name})`);
  let attr: string | null = null;
  for (let a = 0; a < o.attrs && attr === null; a += 1) {
    const n = o.names[o.aName[a]!]!;
    if (!n.startsWith("xmlns") && !n.includes(":")) attr = n;
  }
  if (attr !== null) out.push(`//*[@${attr}]`);
  // A leaf under the repeated element, asked of each.
  for (let i = 1; i < o.n; i += 1) {
    if (o.type[i] === EL && o.name[i] === rep) {
      for (let c = o.first[i]!; c !== -1; c = o.next[c]!)
        if (o.type[c] === EL && o.kids[c] === 0) {
          out.push(`//${name}/${nameOf(o, c)}`);
          break;
        }
      break;
    }
  }
  out.push("//*[not(*)][contains(., 'https://')]");
  return out;
}

const CHUNK = 4096;

/** The UTF-8 length of `s` from `from` to `to`. */
function utf8(s: string, from: number, to: number): number {
  let n = 0;
  for (let i = from; i < to; i += 1) {
    const c = s.charCodeAt(i);
    if (c < 0x80) n += 1;
    else if (c < 0x800) n += 2;
    else if (c >= 0xd800 && c <= 0xdbff) {
      n += 4;
      i += 1;
    } else n += 3;
  }
  return n;
}

/** Character `at`'s place in the file's UTF-8 bytes, counted from after a byte-order mark. */
export function byteAt(k: Kept, at: number): number {
  const text = k.outline.text;
  if (k.bytesAt === undefined) {
    const marks = new Uint32Array(Math.floor(text.length / CHUNK) + 1);
    for (let c = 1; c < marks.length; c += 1) marks[c] = marks[c - 1]! + utf8(text, (c - 1) * CHUNK, c * CHUNK);
    k.bytesAt = marks;
  }
  const chunk = Math.floor(at / CHUNK);
  const bom = text.charCodeAt(0) === 0xfeff ? 3 : 0;
  return k.bytesAt[chunk]! + utf8(text, chunk * CHUNK, at) - bom;
}

const MIME_ATTR = /^(?:[\w.-]+:)?(?:contentType|content-type|mimeType|mime-type|mediaType|mime)$/i;
const FILE_NAME = /^(?:[\w.-]+:)?(?:FILE_NAME|fileName|filename|file-name)$/;
/** A value at least this long gets a chip: an attribute's, and a leaf's text (section 17's counted cases). */
const ATTR_AT_LEAST = 20;
const TEXT_AT_LEAST = 8;
const HEAD = 4096;

/** The values on a line: its element's long attributes and its text where it holds no element. With `all`, any. */
export function valuesOn(k: Kept, line: number, all: boolean): ValueAt[] {
  const o = k.outline;
  const el = nodeOnLine(o, k.byLine, line);
  if (el <= 0 || (!all && o.line[el] !== line)) return [];
  const out: ValueAt[] = [];
  const said: { mime?: string; base64?: boolean } = {};
  let fileName: string | null = null;
  for (let a = o.a0[el]!, e = a + o.an[el]!; a < e; a += 1) {
    const n = o.names[o.aName[a]!]!;
    const v = stringOf(o, o.n + a).trim();
    if (MIME_ATTR.test(n) && /^[\w.+-]+\/[\w.+-]+/.test(v)) said.mime = v;
    if (/^(?:[\w.-]+:)?encoding$/i.test(n) && /^base64$/i.test(v)) said.base64 = true;
    if (FILE_NAME.test(n)) fileName = v;
  }
  // A sibling that says what the value is, or what it is called.
  const up = o.parent[el]!;
  for (let c = up > 0 ? o.first[up]! : -1; c !== -1; c = o.next[c]!) {
    if (c === el || o.type[c] !== EL || o.kids[c]! > 0) continue;
    const local = o.names[o.local[c]!]!;
    const v = stringOf(o, c).trim();
    if (/^(?:MIME_TYPE|mimeType|contentType|mediaType)$/.test(local) && said.mime === undefined && /^[\w.+-]+\/[\w.+-]+$/.test(v)) said.mime = v;
    if (FILE_NAME.test(local) && v !== "") fileName = v;
  }
  const field = namePathOf(o, el);
  // In a layout, a value's place is the file's: the same row of the file as written.
  const src = k.origin ?? k;
  const so = src.outline;
  const place = (from: number, to: number, read: ValueAt["read"], node: number, name: string, f: string, raw: string, what: typeof said) => {
    const head = decode(raw.length > HEAD ? raw.slice(0, HEAD) : raw);
    let bare = 0;
    for (let i = 0; i < raw.length; i += 1) {
      const c = raw.charCodeAt(i);
      if (c !== 32 && c !== 10 && c !== 13 && c !== 9) bare += 1;
    }
    out.push({ line: o.line[el]!, node, name, field: f, from: byteAt(src, from), to: byteAt(src, to), read, head, length: raw.length, bare, said: what, fileName });
  };
  for (let a = o.a0[el]!, e = a + o.an[el]!; a < e; a += 1) {
    const n = o.names[o.aName[a]!]!;
    if (n === "xmlns" || n.startsWith("xmlns:")) continue;
    const from = so.aStart[a]!;
    const to = so.aEnd[a]!;
    if (!all && to - from < ATTR_AT_LEAST) continue;
    place(from, to, "xml_attribute", o.n + a, `@${n}`, `${field}/@${n}`, so.text.slice(from, to), {});
  }
  if (o.kids[el] === 0 && so.close[el]! > so.open[el]!) {
    const from = so.open[el]!;
    const to = so.close[el]!;
    if (all || to - from >= TEXT_AT_LEAST) place(from, to, "xml_text", el, nameOf(o, el), field, so.text.slice(from, to).replace(/<!\[CDATA\[|\]\]>/g, ""), said);
  }
  // With `all`, the text first: Space looks at what the line holds.
  if (all) out.sort((a, b) => (a.read === b.read ? 0 : a.read === "xml_text" ? -1 : 1));
  return out;
}
