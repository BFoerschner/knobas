/**
 * An XML document read into an **outline** (the XML view's design, 2026-11-23, sections 7 and 27): every element, text,
 * comment and instruction a row of numbers in typed arrays, in document order, with its line. No object stands for a
 * node: a 50 MB file is about 2.5 million rows in 100 MB of columns. It runs in the document worker (`doc.worker.ts`).
 *
 * - **A node is a number**: its row. An attribute is `outline.n` plus its row in the attribute columns, so one number
 *   names anything a query can answer.
 * - **A subtree is the rows right after its root**, up to `sub[root]`: `//name` under a node is one pass over a range.
 * - **Blank text between elements is no row**, as libxml2 reads with `XML_PARSE_NOBLANKS`: it is formatting, and it
 *   would double the rows.
 * - **It counts first.** One pass counts `<`, `=` and newlines, the columns are allocated once at that size, and the
 *   second pass fills them. (The spike grew by doubling and held 259 MB while reading for columns of 97.)
 * - **A file that is not well formed** is read as far as it holds: what stands open is closed where the file stops, so
 *   the lines above still fold, and `error` says where and why in a sentence.
 * - Only the five predefined entities and character references are read. Nothing outside the file is ever read.
 */
import { XML_NS } from "./xml-names";

/** Row types: XML's node type numbers, so a row reads as the DOM names it. */
export const DOC = 9;
export const EL = 1;
export const TEXT = 3;
export const CDATA = 4;
export const PI = 7;
export const COMMENT = 8;


/** Prefixes in scope at a row: `""` is the default namespace. Each element that declares one has a scope of its own. */
export type Scope = Readonly<Record<string, string>>;

export interface XmlOutline {
  readonly kind: "xml";
  readonly text: string;
  /** Rows: the document (row 0) and every node. */
  readonly n: number;
  readonly type: Uint8Array;
  /** Where the node's markup starts, its content opens and closes, and its markup ends, as offsets into `text`. */
  readonly start: Uint32Array;
  readonly open: Uint32Array;
  readonly close: Uint32Array;
  readonly end: Uint32Array;
  readonly parent: Int32Array;
  readonly next: Int32Array;
  readonly first: Int32Array;
  readonly last: Int32Array;
  /** The last row of the node's subtree (itself where it holds nothing). */
  readonly sub: Uint32Array;
  /** How many element children it has. */
  readonly kids: Uint32Array;
  /** The line (from 1) it starts and ends on. */
  readonly line: Uint32Array;
  readonly endLine: Uint32Array;
  /** Its name as written, its local name, its namespace: ids into `names` and `uris`. */
  readonly name: Uint32Array;
  readonly local: Uint32Array;
  readonly ns: Uint32Array;
  /** Its scope: an index into `scopes`. */
  readonly scope: Int32Array;
  /** Its attributes: rows `a0` to `a0 + an` of the attribute columns. */
  readonly a0: Uint32Array;
  readonly an: Uint32Array;
  /** How deep it stands: the document 0, the root 1. */
  readonly depth: Uint16Array;
  /** Attribute rows: how many, and their columns. */
  readonly attrs: number;
  readonly aName: Uint32Array;
  readonly aLocal: Uint32Array;
  readonly aNs: Uint32Array;
  /** Where the value starts and ends (inside its quotes). */
  readonly aStart: Uint32Array;
  readonly aEnd: Uint32Array;
  readonly aOwner: Uint32Array;
  readonly names: readonly string[];
  readonly uris: readonly string[];
  readonly scopes: readonly Scope[];
  /** Where each line starts: line `k` (from 1) starts at `starts[k - 1]`. */
  readonly starts: Uint32Array;
  /** How many lines the text holds, as the file view counts them (a last newline ends the last line). */
  readonly lines: number;
  /** The root element's row, or -1. */
  readonly root: number;
  /** Where the file stops being XML, or `null`. */
  readonly error: { message: string; line: number } | null;
}

/** Character codes the reader looks for. */
const C = { LT: 60, GT: 62, SLASH: 47, BANG: 33, QUESTION: 63, EQ: 61, DQ: 34, SQ: 39, LBRACKET: 91, RBRACKET: 93 } as const;

function interner(): { list: string[]; id: (s: string) => number } {
  const ids = new Map<string, number>();
  const list: string[] = [];
  return {
    list,
    id(s) {
      let i = ids.get(s);
      if (i === undefined) {
        i = list.length;
        list.push(s);
        ids.set(s, i);
      }
      return i;
    },
  };
}

/** Where each line of `text` starts, counted once and allocated once. */
export function lineStarts(text: string): Uint32Array {
  let n = 1;
  for (let at = text.indexOf("\n"); at !== -1; at = text.indexOf("\n", at + 1)) n += 1;
  const starts = new Uint32Array(n);
  let k = 1;
  for (let at = text.indexOf("\n"); at !== -1; at = text.indexOf("\n", at + 1)) starts[k++] = at + 1;
  return starts;
}

/** The lines the file view draws of a text with these starts: a last newline ends the last line, it opens none. */
export function linesOf(text: string, starts: Uint32Array): number {
  return starts.length > 1 && text.endsWith("\n") ? starts.length - 1 : starts.length;
}

/** Counts `<` and `=` once: the rows and attribute rows a text can hold at most. */
function capacity(text: string): { rows: number; attrs: number } {
  let lt = 0;
  let eq = 0;
  for (let i = text.indexOf("<"); i !== -1; i = text.indexOf("<", i + 1)) lt += 1;
  for (let i = text.indexOf("="); i !== -1; i = text.indexOf("=", i + 1)) eq += 1;
  // Every row but a text starts at a `<`, and a text stands before one: at most twice the `<`, plus the document. A
  // closing tag starts at one too and makes no row, so a document is about one row a `<`; the second half is held
  // back and taken only if the file needs it (the columns grow once then).
  return { rows: lt + 2, attrs: eq + 1 };
}

class Rows {
  cap: number;
  type: Uint8Array;
  start: Uint32Array;
  open: Uint32Array;
  close: Uint32Array;
  end: Uint32Array;
  parent: Int32Array;
  next: Int32Array;
  first: Int32Array;
  last: Int32Array;
  sub: Uint32Array;
  kids: Uint32Array;
  line: Uint32Array;
  endLine: Uint32Array;
  name: Uint32Array;
  local: Uint32Array;
  ns: Uint32Array;
  scope: Int32Array;
  a0: Uint32Array;
  an: Uint32Array;
  depth: Uint16Array;
  constructor(cap: number) {
    this.cap = cap;
    this.type = new Uint8Array(cap);
    this.start = new Uint32Array(cap);
    this.open = new Uint32Array(cap);
    this.close = new Uint32Array(cap);
    this.end = new Uint32Array(cap);
    this.parent = new Int32Array(cap);
    this.next = new Int32Array(cap);
    this.first = new Int32Array(cap);
    this.last = new Int32Array(cap);
    this.sub = new Uint32Array(cap);
    this.kids = new Uint32Array(cap);
    this.line = new Uint32Array(cap);
    this.endLine = new Uint32Array(cap);
    this.name = new Uint32Array(cap);
    this.local = new Uint32Array(cap);
    this.ns = new Uint32Array(cap);
    this.scope = new Int32Array(cap);
    this.a0 = new Uint32Array(cap);
    this.an = new Uint32Array(cap);
    this.depth = new Uint16Array(cap);
  }
  /** Twice the room: only a document of many short texts between self-closed tags gets here. */
  grow(): void {
    const next = new Rows(this.cap * 2);
    for (const key of COLUMNS) (next[key] as { set(a: ArrayLike<number>): void }).set(this[key]);
    Object.assign(this, next);
  }
}
const COLUMNS = ["type", "start", "open", "close", "end", "parent", "next", "first", "last", "sub", "kids", "line", "endLine", "name", "local", "ns", "scope", "a0", "an", "depth"] as const;

const PREDEF: Record<string, string> = { lt: "<", gt: ">", amp: "&", quot: '"', apos: "'" };

/** Character references and the five predefined entities, read. Anything else stays as written. */
export function decode(s: string): string {
  if (s.indexOf("&") === -1) return s;
  return s.replace(/&(#x[0-9a-fA-F]+|#[0-9]+|[a-zA-Z]+);/g, (whole, name: string) => {
    if (name[0] === "#") {
      const code = name[1] === "x" ? parseInt(name.slice(2), 16) : parseInt(name.slice(1), 10);
      return code > 0 && code <= 0x10ffff ? String.fromCodePoint(code) : whole;
    }
    return PREDEF[name] ?? whole;
  });
}

class NotXml extends Error {
  readonly at: number;
  constructor(message: string, at: number) {
    super(message);
    this.at = at;
  }
}

/** Reads `text` into an outline. Never throws on what the text holds: a file that is not XML says so in `error`. */
export function readXml(text: string): XmlOutline {
  const cap = capacity(text);
  const N = new Rows(cap.rows);
  let aCap = cap.attrs;
  let aName = new Uint32Array(aCap);
  let aStart = new Uint32Array(aCap);
  let aEnd = new Uint32Array(aCap);
  let aOwner = new Uint32Array(aCap);
  let attrs = 0;
  const names = interner();
  const uris = interner();
  uris.id(""); // 0: no namespace
  const scopes: Scope[] = [{ xml: XML_NS }];
  const starts = lineStarts(text);
  // Places asked for only grow, so the line is found by walking forward from the last.
  let lineAt = 0;
  const lineOf = (pos: number) => {
    while (lineAt + 1 < starts.length && starts[lineAt + 1]! <= pos) lineAt += 1;
    if (lineAt > 0 && starts[lineAt]! > pos) {
      // A closing place asked after a later opening one (an error's place): search from the top.
      let lo = 0;
      let hi = starts.length - 1;
      while (lo < hi) {
        const mid = (lo + hi + 1) >> 1;
        if (starts[mid]! <= pos) lo = mid;
        else hi = mid - 1;
      }
      return lo + 1;
    }
    return lineAt + 1;
  };
  let n = 0;
  const add = (type: number, start: number, parent: number): number => {
    if (n === N.cap) N.grow();
    const i = n++;
    N.type[i] = type;
    N.start[i] = start;
    N.parent[i] = parent;
    N.next[i] = -1;
    N.first[i] = -1;
    N.last[i] = -1;
    N.scope[i] = parent < 0 ? 0 : N.scope[parent]!;
    N.depth[i] = parent < 0 ? 0 : N.depth[parent]! + 1;
    N.line[i] = lineOf(start);
    if (parent >= 0) {
      if (N.last[parent] === -1) N.first[parent] = i;
      else N.next[N.last[parent]!] = i;
      N.last[parent] = i;
    }
    return i;
  };
  const len = text.length;
  const doc = add(DOC, 0, -1);
  let cur = doc;
  let i = 0;
  let error: { message: string; line: number } | null = null;
  const firstThing = text.search(/\S/);
  try {
    while (i < len) {
      const lt = text.indexOf("<", i);
      const stop = lt === -1 ? len : lt;
      if (stop > i && cur !== doc) {
        // Text. Blank text is a row only where libxml2 keeps it: alone in a leaf, or beside other text.
        let blank = true;
        for (let k = i; k < stop; k += 1) {
          const ch = text.charCodeAt(k);
          if (ch !== 32 && ch !== 10 && ch !== 9 && ch !== 13) {
            blank = false;
            break;
          }
        }
        const lastKid = N.last[cur]!;
        const keep = !blank || (lastKid === -1 ? text.charCodeAt(stop + 1) === C.SLASH : N.type[lastKid] === TEXT || N.type[N.first[cur]!] === TEXT);
        if (keep) {
          const t = add(TEXT, i, cur);
          N.open[t] = i;
          N.close[t] = stop;
          N.end[t] = stop;
          N.endLine[t] = lineOf(stop === 0 ? 0 : stop - 1);
          N.sub[t] = t;
        }
      } else if (stop > i && cur === doc && text.slice(i, stop).trim() !== "") {
        throw new NotXml(N.first[doc] === -1 ? "Text stands before the first element." : "Text stands after the root element.", i + text.slice(i, stop).search(/\S/));
      }
      if (lt === -1) break;
      const c1 = text.charCodeAt(lt + 1);
      if (c1 === C.BANG) {
        if (text.startsWith("<!--", lt)) {
          const e = text.indexOf("-->", lt + 4);
          if (e === -1) throw new NotXml("A comment is not closed.", lt);
          const t = add(COMMENT, lt, cur);
          N.open[t] = lt + 4;
          N.close[t] = e;
          N.end[t] = e + 3;
          N.endLine[t] = lineOf(e + 2);
          N.sub[t] = t;
          i = e + 3;
          continue;
        }
        if (text.startsWith("<![CDATA[", lt)) {
          if (cur === doc) throw new NotXml("A CDATA section stands outside the root element.", lt);
          const e = text.indexOf("]]>", lt + 9);
          if (e === -1) throw new NotXml("A CDATA section is not closed.", lt);
          const t = add(CDATA, lt, cur);
          N.open[t] = lt + 9;
          N.close[t] = e;
          N.end[t] = e + 3;
          N.endLine[t] = lineOf(e + 2);
          N.sub[t] = t;
          i = e + 3;
          continue;
        }
        if (text.startsWith("<!DOCTYPE", lt)) {
          let j = lt + 9;
          let depth = 0;
          let quote = 0;
          for (; j < len; j += 1) {
            const ch = text.charCodeAt(j);
            if (quote !== 0) {
              if (ch === quote) quote = 0;
            } else if (ch === C.DQ || ch === C.SQ) quote = ch;
            else if (ch === C.LBRACKET) depth += 1;
            else if (ch === C.RBRACKET) depth -= 1;
            else if (ch === C.GT && depth === 0) break;
          }
          if (j >= len) throw new NotXml("The DOCTYPE is not closed.", lt);
          i = j + 1;
          continue;
        }
        throw new NotXml("This is not XML knobas reads.", lt);
      }
      if (c1 === C.QUESTION) {
        const e = text.indexOf("?>", lt + 2);
        if (e === -1) throw new NotXml("A processing instruction is not closed.", lt);
        // The declaration is no node; an instruction is.
        if (!(text.startsWith("<?xml", lt) && lt === firstThing)) {
          const t = add(PI, lt, cur);
          let k = lt + 2;
          while (k < e && text.charCodeAt(k) > 32) k += 1;
          const id = names.id(text.slice(lt + 2, k));
          N.name[t] = id;
          N.local[t] = id;
          N.open[t] = Math.min(e, k + 1);
          N.close[t] = e;
          N.end[t] = e + 2;
          N.endLine[t] = lineOf(e + 1);
          N.sub[t] = t;
        }
        i = e + 2;
        continue;
      }
      if (c1 === C.SLASH) {
        const gt = text.indexOf(">", lt + 2);
        if (gt === -1) throw new NotXml("A closing tag is not closed.", lt);
        let e = gt;
        while (e > lt + 2 && text.charCodeAt(e - 1) <= 32) e -= 1;
        const q = text.slice(lt + 2, e);
        if (cur === doc) throw new NotXml(`\`</${q}>\` closes nothing open here.`, lt);
        const open = names.list[N.name[cur]!]!;
        if (open !== q) throw new NotXml(`\`</${q}>\` closes nothing open here: \`<${open}>\` is open.`, lt);
        N.close[cur] = lt;
        N.end[cur] = gt + 1;
        N.endLine[cur] = lineOf(gt);
        N.sub[cur] = n - 1;
        cur = N.parent[cur]!;
        i = gt + 1;
        continue;
      }
      // A start tag: it ends at the first `>` outside a quoted value.
      let j = lt + 1;
      let quote = 0;
      for (; j < len; j += 1) {
        const ch = text.charCodeAt(j);
        if (quote !== 0) {
          if (ch === quote) quote = 0;
        } else if (ch === C.DQ || ch === C.SQ) quote = ch;
        else if (ch === C.GT) break;
        else if (ch === C.LT) throw new NotXml("A tag is not closed.", lt);
      }
      if (j >= len) throw new NotXml("A tag is not closed.", lt);
      const selfClose = text.charCodeAt(j - 1) === C.SLASH;
      let k = lt + 1;
      while (k < j) {
        const ch = text.charCodeAt(k);
        if (ch <= 32 || ch === C.SLASH) break;
        k += 1;
      }
      if (k === lt + 1) throw new NotXml("A tag has no name.", lt);
      if (cur === doc && N.first[doc] !== -1) {
        for (let s = N.first[doc]!; s !== -1; s = N.next[s]!) if (N.type[s] === EL) throw new NotXml("A second root element: a document has one.", lt);
      }
      const q = text.slice(lt + 1, k);
      const el = add(EL, lt, cur);
      if (cur !== doc) N.kids[cur] = N.kids[cur]! + 1;
      N.name[el] = names.id(q);
      N.a0[el] = attrs;
      // Attributes: name="value" or name='value'. Namespace declarations make a scope of their own.
      let own: Record<string, string> | null = null;
      const tagEnd = selfClose ? j - 1 : j;
      while (k < tagEnd) {
        while (k < tagEnd && text.charCodeAt(k) <= 32) k += 1;
        if (k >= tagEnd) break;
        const n0 = k;
        while (k < tagEnd) {
          const ch = text.charCodeAt(k);
          if (ch === C.EQ || ch <= 32) break;
          k += 1;
        }
        const aq = text.slice(n0, k);
        while (k < tagEnd && text.charCodeAt(k) <= 32) k += 1;
        if (text.charCodeAt(k) !== C.EQ) throw new NotXml(`The attribute \`${aq}\` has no value.`, n0);
        k += 1;
        while (k < tagEnd && text.charCodeAt(k) <= 32) k += 1;
        const qc = text.charCodeAt(k);
        if (qc !== C.DQ && qc !== C.SQ) throw new NotXml(`The value of \`${aq}\` is not quoted.`, k);
        const v0 = k + 1;
        const v1 = text.indexOf(qc === C.DQ ? '"' : "'", v0);
        if (v1 === -1 || v1 > tagEnd) throw new NotXml(`The value of \`${aq}\` is not closed.`, k);
        k = v1 + 1;
        if (aq === "xmlns" || aq.startsWith("xmlns:")) {
          own ??= Object.create(scopes[N.scope[el]!]!) as Record<string, string>;
          own[aq === "xmlns" ? "" : aq.slice(6)] = decode(text.slice(v0, v1));
        }
        if (attrs === aCap) {
          aCap *= 2;
          const grow = (a: Uint32Array) => {
            const b = new Uint32Array(aCap);
            b.set(a);
            return b;
          };
          aName = grow(aName);
          aStart = grow(aStart);
          aEnd = grow(aEnd);
          aOwner = grow(aOwner);
        }
        const a = attrs++;
        aName[a] = names.id(aq);
        aStart[a] = v0;
        aEnd[a] = v1;
        aOwner[a] = el;
      }
      N.an[el] = attrs - N.a0[el]!;
      if (own !== null) {
        N.scope[el] = scopes.length;
        scopes.push(own);
      }
      const scope = scopes[N.scope[el]!]!;
      const colon = q.indexOf(":");
      N.local[el] = colon === -1 ? N.name[el]! : names.id(q.slice(colon + 1));
      N.ns[el] = uris.id(scope[colon === -1 ? "" : q.slice(0, colon)] ?? "");
      N.open[el] = j + 1;
      if (selfClose) {
        N.close[el] = j + 1;
        N.end[el] = j + 1;
        N.endLine[el] = lineOf(j);
        N.sub[el] = el;
      } else cur = el;
      i = j + 1;
    }
    if (cur !== doc) throw new NotXml(`\`<${names.list[N.name[cur]!]!}>\` is never closed.`, len === 0 ? 0 : len - 1);
  } catch (cause) {
    if (!(cause instanceof NotXml)) throw cause;
    error = { message: cause.message, line: lineOf(cause.at) };
    // What stands open is closed where the file stops, so the lines above still fold.
    for (let c = cur; c !== -1 && c !== doc; c = N.parent[c]!) {
      N.close[c] = len;
      N.end[c] = len;
      N.endLine[c] = starts.length;
      N.sub[c] = n - 1;
    }
  }
  N.end[doc] = len;
  N.close[doc] = len;
  N.endLine[doc] = starts.length;
  N.sub[doc] = n - 1;
  // Attribute local names and namespaces, now every scope is known.
  const aLocal = new Uint32Array(attrs);
  const aNs = new Uint32Array(attrs);
  for (let a = 0; a < attrs; a += 1) {
    const aq = names.list[aName[a]!]!;
    const ac = aq.indexOf(":");
    aLocal[a] = ac === -1 ? aName[a]! : names.id(aq.slice(ac + 1));
    aNs[a] = ac === -1 || aq.startsWith("xmlns") ? 0 : uris.id(scopes[N.scope[aOwner[a]!]!]![aq.slice(0, ac)] ?? "");
  }
  let root = -1;
  for (let c = N.first[doc]!; c !== -1; c = N.next[c]!)
    if (N.type[c] === EL) {
      root = c;
      break;
    }
  if (root === -1 && error === null) error = { message: "The file holds no element.", line: 1 };
  const cut = <T extends Uint8Array | Uint16Array | Uint32Array | Int32Array>(a: T): T => a.subarray(0, n) as T;
  return {
    kind: "xml",
    text,
    n,
    type: cut(N.type),
    start: cut(N.start),
    open: cut(N.open),
    close: cut(N.close),
    end: cut(N.end),
    parent: cut(N.parent),
    next: cut(N.next),
    first: cut(N.first),
    last: cut(N.last),
    sub: cut(N.sub),
    kids: cut(N.kids),
    line: cut(N.line),
    endLine: cut(N.endLine),
    name: cut(N.name),
    local: cut(N.local),
    ns: cut(N.ns),
    scope: cut(N.scope),
    a0: cut(N.a0),
    an: cut(N.an),
    depth: cut(N.depth),
    attrs,
    aName: aName.subarray(0, attrs),
    aLocal,
    aNs,
    aStart: aStart.subarray(0, attrs),
    aEnd: aEnd.subarray(0, attrs),
    aOwner: aOwner.subarray(0, attrs),
    names: names.list,
    uris: uris.list,
    scopes,
    starts,
    lines: linesOf(text, starts),
    root,
    error,
  };
}

/** Whether node `id` is an attribute. */
export const isAttr = (o: XmlOutline, id: number) => id >= o.n;

/** A node's string-value, as XPath defines it: an element's is every text under it, in order. */
export function stringOf(o: XmlOutline, id: number): string {
  if (id >= o.n) return decode(o.text.slice(o.aStart[id - o.n], o.aEnd[id - o.n]));
  const t = o.type[id];
  if (t === TEXT) return decode(o.text.slice(o.open[id], o.close[id]));
  if (t === CDATA || t === COMMENT || t === PI) return o.text.slice(o.open[id], o.close[id]);
  const last = o.sub[id]!;
  let out = "";
  for (let k = id + 1; k <= last; k += 1) {
    const kt = o.type[k];
    if (kt === TEXT) out += decode(o.text.slice(o.open[k], o.close[k]));
    else if (kt === CDATA) out += o.text.slice(o.open[k], o.close[k]);
  }
  return out;
}

/** The name as the file writes it: `bmecat:PRICE`; an attribute's without its `@`. */
export function nameOf(o: XmlOutline, id: number): string {
  if (id >= o.n) return o.names[o.aName[id - o.n]!]!;
  const t = o.type[id];
  return t === EL || t === PI ? o.names[o.name[id]!]! : "";
}

/** The line a node stands on: an attribute's is its element's. */
export const lineOfNode = (o: XmlOutline, id: number): number => (id >= o.n ? o.line[o.aOwner[id - o.n]!]! : o.line[id]!);

/** The element that holds an attribute, or the node itself. */
export const ownerOf = (o: XmlOutline, id: number): number => (id >= o.n ? o.aOwner[id - o.n]! : id);

/** Element `at`'s position among its parent's elements of its name, and whether it has such siblings. */
function positionOf(o: XmlOutline, at: number): { pos: number; more: boolean } {
  let pos = 1;
  let more = false;
  for (let s = o.first[o.parent[at]!]!; s !== -1; s = o.next[s]!) {
    if (s === at) continue;
    if (o.type[s] === EL && o.name[s] === o.name[at]) {
      if (s < at) pos += 1;
      more = true;
    }
  }
  return { pos, more };
}

/** An element's name with its position where it has siblings of its name: `ORDER_ITEM[2]`. */
export function stepOf(o: XmlOutline, at: number): string {
  const { pos, more } = positionOf(o, at);
  return more ? `${nameOf(o, at)}[${pos}]` : nameOf(o, at);
}

/** The way down to a node, as an XPath a person can paste: `/ORDER/ORDER_ITEM_LIST/ORDER_ITEM[2]/@type`. */
export function pathOf(o: XmlOutline, id: number): string {
  const parts: string[] = [];
  let at = id;
  if (id >= o.n) {
    parts.push(`@${nameOf(o, id)}`);
    at = o.aOwner[id - o.n]!;
  }
  for (; at > 0; at = o.parent[at]!) {
    const t = o.type[at];
    if (t !== EL) {
      parts.push(t === COMMENT ? "comment()" : t === PI ? "processing-instruction()" : "text()");
      continue;
    }
    parts.push(stepOf(o, at));
  }
  return "/" + parts.reverse().join("/");
}

/** The names down to a node, with no positions: what a chain kept for a field is found by. `MIME_EMBEDDED/MIME_DATA`. */
export function namePathOf(o: XmlOutline, id: number): string {
  const parts: string[] = [];
  let at = id;
  if (id >= o.n) {
    parts.push(`@${nameOf(o, id)}`);
    at = o.aOwner[id - o.n]!;
  }
  for (; at > 0; at = o.parent[at]!) if (o.type[at] === EL) parts.push(nameOf(o, at));
  return parts.reverse().join("/");
}

/** The node at a path {@link pathOf} wrote (positions where there were siblings), or -1: how a fold is found again. */
export function nodeAtPath(o: XmlOutline, path: string): number {
  let at = 0;
  for (const step of path.split("/").filter(Boolean)) {
    const m = /^(@)?([^[]+)(?:\[(\d+)\])?$/.exec(step);
    if (m === null) return -1;
    if (m[1] !== undefined) {
      for (let a = o.a0[at]!, e = a + o.an[at]!; a < e; a += 1) if (o.names[o.aName[a]!] === m[2]) return o.n + a;
      return -1;
    }
    if (m[2] === "comment()" || m[2] === "text()" || m[2] === "processing-instruction()") {
      const want = m[2] === "comment()" ? COMMENT : m[2] === "text()" ? TEXT : PI;
      let hit = -1;
      for (let c = o.first[at]!; c !== -1; c = o.next[c]!)
        if (o.type[c] === want) {
          hit = c;
          break;
        }
      if (hit === -1) return -1;
      at = hit;
      continue;
    }
    let want = m[3] !== undefined ? Number(m[3]) : 1;
    let hit = -1;
    for (let c = o.first[at]!; c !== -1; c = o.next[c]!) {
      if (o.type[c] !== EL) continue;
      if (m[2] !== "*" && o.names[o.name[c]!] !== m[2]) continue;
      want -= 1;
      if (want === 0) {
        hit = c;
        break;
      }
    }
    if (hit === -1) return -1;
    at = hit;
  }
  return at;
}
