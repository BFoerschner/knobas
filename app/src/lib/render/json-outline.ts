/**
 * A JSON file read as columns (the XML view's design, 2026-11-23, section 25): every value a row, a member's key beside
 * it, in the XML outline's columns where they mean the same (`start`, `end`, `parent`, `next`, `first`, `sub`, `line`,
 * `endLine`, `depth`, `kids`), so the document view reads either. Strict JSON, as jq reads it. The spike's is `scanJson`
 * in `xr-doc.js`.
 *
 * - **Row 0 is the document**, the value under it its root: depth counts from the root as the XML outline's does.
 * - **A line-delimited file** (`.jsonl`, `.ndjson`) holds a value a line, each a child of the document.
 * - **A file that is not JSON** is read as far as it goes; `error` says where it stops, and every value still open ends
 *   at the file's end.
 */
import { lineStarts, linesOf } from "./xml-outline";

export const J_DOC = 0;
export const J_OBJ = 1;
export const J_ARR = 2;
export const J_STR = 3;
export const J_NUM = 4;
export const J_LIT = 5;

export interface JsonOutline {
  readonly kind: "json";
  readonly text: string;
  /** Rows: the document (row 0) and every value. */
  readonly n: number;
  readonly type: Uint8Array;
  /** Where the value's text starts and ends (a string's quotes inside), as offsets into `text`. */
  readonly start: Uint32Array;
  readonly end: Uint32Array;
  readonly parent: Int32Array;
  readonly next: Int32Array;
  readonly first: Int32Array;
  readonly last: Int32Array;
  /** The last row of the value's subtree (itself where it holds nothing). */
  readonly sub: Uint32Array;
  /** How many members or items it holds. */
  readonly kids: Uint32Array;
  /** The line (from 1) it starts and ends on. */
  readonly line: Uint32Array;
  readonly endLine: Uint32Array;
  /** A member's name: an id into `keys`; -1 for an item of an array, and for the root. */
  readonly key: Int32Array;
  /** Its place among its holder's members or items. */
  readonly index: Uint32Array;
  /** How deep it stands: the document 0, the root 1. */
  readonly depth: Uint16Array;
  readonly keys: readonly string[];
  readonly starts: Uint32Array;
  readonly lines: number;
  /** The root value's row (the first, in a line-delimited file), or -1. */
  readonly root: number;
  /** Whether the file holds a value a line. */
  readonly delimited: boolean;
  readonly error: { message: string; line: number } | null;
}

class JsonError extends Error {
  constructor(
    message: string,
    readonly line: number,
  ) {
    super(message);
  }
}

/** A string's value from its text with its quotes: escapes read. */
export function unquote(s: string): string {
  if (s.indexOf("\\") === -1) return s.slice(1, -1);
  try {
    return JSON.parse(s) as string;
  } catch {
    return s.slice(1, -1);
  }
}

/** Reads `text` as JSON, or as a value a line where `delimited`. */
export function readJson(text: string, delimited = false): JsonOutline {
  let cap = Math.max(256, text.length >> 4);
  let n = 0;
  let type = new Uint8Array(cap);
  let start = new Uint32Array(cap);
  let end = new Uint32Array(cap);
  let parent = new Int32Array(cap);
  let next = new Int32Array(cap);
  let first = new Int32Array(cap);
  let last = new Int32Array(cap);
  let sub = new Uint32Array(cap);
  let kids = new Uint32Array(cap);
  let line = new Uint32Array(cap);
  let endLine = new Uint32Array(cap);
  let key = new Int32Array(cap);
  let index = new Uint32Array(cap);
  let depth = new Uint16Array(cap);
  const grow = () => {
    cap *= 2;
    const g = <T extends Uint8Array | Uint16Array | Uint32Array | Int32Array>(a: T): T => {
      const b = new (a.constructor as new (n: number) => T)(cap);
      b.set(a);
      return b;
    };
    type = g(type);
    start = g(start);
    end = g(end);
    parent = g(parent);
    next = g(next);
    first = g(first);
    last = g(last);
    sub = g(sub);
    kids = g(kids);
    line = g(line);
    endLine = g(endLine);
    key = g(key);
    index = g(index);
    depth = g(depth);
  };
  const keyIds = new Map<string, number>();
  const keys: string[] = [];
  const keyOf = (k: string) => {
    let id = keyIds.get(k);
    if (id === undefined) {
      id = keys.length;
      keys.push(k);
      keyIds.set(k, id);
    }
    return id;
  };
  const starts = lineStarts(text);
  const lines = linesOf(text, starts);
  let lineAt = 0;
  const lineOf = (pos: number) => {
    if (pos < (starts[lineAt] ?? 0)) lineAt = 0;
    while (lineAt + 1 < starts.length && starts[lineAt + 1]! <= pos) lineAt += 1;
    return lineAt + 1;
  };
  const len = text.length;
  let i = text.charCodeAt(0) === 0xfeff ? 1 : 0;
  const fail = (message: string) => new JsonError(message, lineOf(Math.min(i, Math.max(0, len - 1))));
  const ws = (lineBreaks = true) => {
    while (i < len) {
      const ch = text.charCodeAt(i);
      if (ch === 32 || ch === 9 || ch === 13 || (lineBreaks && ch === 10)) i += 1;
      else break;
    }
  };
  const add = (t: number, at: number, up: number, k: number) => {
    if (n === cap) grow();
    const id = n++;
    type[id] = t;
    start[id] = at;
    parent[id] = up;
    next[id] = -1;
    first[id] = -1;
    last[id] = -1;
    key[id] = k;
    line[id] = lineOf(at);
    depth[id] = up < 0 ? 0 : depth[up]! + 1;
    if (up >= 0) {
      index[id] = kids[up]!;
      kids[up] = kids[up]! + 1;
      if (last[up] === -1) first[up] = id;
      else next[last[up]!] = id;
      last[up] = id;
    }
    return id;
  };
  /** Steps over a string whose quote is at `i`; answers where it started. */
  const string = () => {
    let j = i + 1;
    for (;;) {
      j = text.indexOf('"', j);
      if (j === -1) throw fail("A string is not closed.");
      let b = j - 1;
      while (text.charCodeAt(b) === 92) b -= 1;
      if ((j - b) % 2 === 1) break;
      j += 1;
    }
    const s = i;
    i = j + 1;
    return s;
  };
  // The nesting is walked with a stack of its own: a deep file must not overflow the reader's.
  const value = (up: number, k: number): void => {
    ws();
    if (i >= len) throw fail("The file ends where a value should stand.");
    const open: number[] = [];
    let holder = up;
    let member = k;
    for (;;) {
      const ch = text.charCodeAt(i);
      let made: number;
      if (ch === 123 || ch === 91) {
        const id = add(ch === 123 ? J_OBJ : J_ARR, i, holder, member);
        i += 1;
        ws();
        if (text.charCodeAt(i) === (ch === 123 ? 125 : 93)) {
          i += 1;
          end[id] = i;
          endLine[id] = lineOf(i - 1);
          sub[id] = id;
          made = id;
        } else {
          open.push(id);
          holder = id;
          member = -1;
          if (ch === 123) {
            if (text.charCodeAt(i) !== 34) throw fail("A member needs a name in quotes.");
            const s = string();
            member = keyOf(unquote(text.slice(s, i)));
            ws();
            if (text.charCodeAt(i) !== 58) throw fail("A colon is missing after a member's name.");
            i += 1;
          }
          ws();
          if (i >= len) throw fail("The file ends where a value should stand.");
          continue;
        }
      } else if (ch === 34) {
        made = add(J_STR, i, holder, member);
        string();
        end[made] = i;
        endLine[made] = lineOf(i - 1);
        sub[made] = made;
      } else {
        const s = i;
        while (i < len) {
          const c = text.charCodeAt(i);
          if (c === 44 || c === 125 || c === 93 || c <= 32) break;
          i += 1;
        }
        const word = text.slice(s, i);
        let t: number;
        if (word === "true" || word === "false" || word === "null") t = J_LIT;
        else if (/^-?(0|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?$/.test(word)) t = J_NUM;
        else {
          i = s;
          throw fail(`${word.slice(0, 24) || "This"} is no JSON value.`);
        }
        made = add(t, s, holder, member);
        end[made] = i;
        endLine[made] = lineOf(i - 1);
        sub[made] = made;
      }
      // After a value: the holder's comma, or its end, closing as many holders as end here.
      for (;;) {
        if (open.length === 0) return;
        const at = open[open.length - 1]!;
        const isObj = type[at] === J_OBJ;
        ws();
        const sep = text.charCodeAt(i);
        if (sep === 44) {
          i += 1;
          ws();
          member = -1;
          if (isObj) {
            if (text.charCodeAt(i) !== 34) throw fail("A member needs a name in quotes.");
            const s = string();
            member = keyOf(unquote(text.slice(s, i)));
            ws();
            if (text.charCodeAt(i) !== 58) throw fail("A colon is missing after a member's name.");
            i += 1;
            ws();
          }
          holder = at;
          if (i >= len) throw fail("The file ends where a value should stand.");
          break;
        }
        if (sep === (isObj ? 125 : 93)) {
          i += 1;
          open.pop();
          end[at] = i;
          endLine[at] = lineOf(i - 1);
          sub[at] = n - 1;
          continue;
        }
        throw fail(isObj ? "A comma or } is missing." : "A comma or ] is missing.");
      }
    }
  };
  add(J_DOC, 0, -1, -1);
  let error: { message: string; line: number } | null = null;
  try {
    if (delimited) {
      for (;;) {
        ws();
        if (i >= len) break;
        value(0, -1);
        ws(false);
        if (i < len && text.charCodeAt(i) !== 10) throw fail("Something stands after the line's value.");
      }
    } else {
      value(0, -1);
      ws();
      if (i < len) throw fail("Something stands after the value.");
    }
  } catch (e) {
    if (!(e instanceof JsonError)) throw e;
    error = { message: e.message, line: e.line };
    // What is still open ends at the file's end.
    for (let c = 1; c < n; c += 1)
      if (end[c] === 0) {
        end[c] = len;
        endLine[c] = lines;
        sub[c] = n - 1;
      }
  }
  end[0] = len;
  endLine[0] = lines;
  sub[0] = n - 1;
  const trim = <T extends Uint8Array | Uint16Array | Uint32Array | Int32Array>(a: T): T => a.slice(0, n) as T;
  return {
    kind: "json",
    text,
    n,
    type: trim(type),
    start: trim(start),
    end: trim(end),
    parent: trim(parent),
    next: trim(next),
    first: trim(first),
    last: trim(last),
    sub: trim(sub),
    kids: trim(kids),
    line: trim(line),
    endLine: trim(endLine),
    key: trim(key),
    index: trim(index),
    depth: trim(depth),
    keys,
    starts,
    lines,
    root: n > 1 ? 1 : -1,
    delimited,
    error,
  };
}

/** A member's name as a jq path writes it: `.name`, else `["with space"]`. */
const step = (k: string) => (/^[A-Za-z_][A-Za-z0-9_]*$/.test(k) ? `.${k}` : `[${JSON.stringify(k)}]`);

/** The jq path to a value: `.orders[2].attachments[0].content`; `.` for the root. A line's value of a delimited file is its own input. */
export function jsonPathOf(o: JsonOutline, id: number): string {
  const parts: string[] = [];
  for (let at = id; at > 0 && o.parent[at]! > 0; at = o.parent[at]!) {
    const up = o.parent[at]!;
    parts.push(o.type[up] === J_ARR ? `[${o.index[at]}]` : step(o.keys[o.key[at]!]!));
  }
  return parts.length === 0 ? "." : parts.reverse().join("");
}

/** A value's step in the way down: its member's name or its item's place; the root `.`. */
export function jsonStepOf(o: JsonOutline, id: number): string {
  const up = o.parent[id]!;
  if (up <= 0) return o.delimited ? `line ${o.line[id]}` : ".";
  return o.type[up] === J_ARR ? `[${o.index[id]}]` : o.keys[o.key[id]!]!;
}

/** The row at a jq path's steps (`["orders", 3, "id"]`) under the root (or the `input`-th value of a delimited file), or -1. */
export function jsonAt(o: JsonOutline, path: readonly (string | number)[], input = 0): number {
  let at = o.first[0]!;
  for (let k = 0; k < input && at !== -1; k += 1) at = o.next[at]!;
  for (const s of path) {
    if (at < 0) return -1;
    let hit = -1;
    if (typeof s === "number") {
      if (o.type[at] !== J_ARR) return -1;
      let k = s < 0 ? o.kids[at]! + s : s;
      for (let c = o.first[at]!; c !== -1; c = o.next[c]!)
        if (k-- === 0) {
          hit = c;
          break;
        }
    } else {
      if (o.type[at] !== J_OBJ) return -1;
      for (let c = o.first[at]!; c !== -1; c = o.next[c]!)
        if (o.keys[o.key[c]!] === s) {
          hit = c;
          break;
        }
    }
    at = hit;
  }
  return at;
}

/** A jq path as written by `jsonPathOf`, back to its steps; `null` for one it does not write. */
export function stepsOf(path: string): (string | number)[] | null {
  if (path === ".") return [];
  const out: (string | number)[] = [];
  const RE = /\.([A-Za-z_][A-Za-z0-9_]*)|\[(\d+)\]|\[("(?:[^"\\]|\\.)*")\]/y;
  let at = 0;
  while (at < path.length) {
    RE.lastIndex = at;
    const m = RE.exec(path);
    if (m === null) return null;
    out.push(m[1] ?? (m[2] !== undefined ? Number(m[2]) : (JSON.parse(m[3]!) as string)));
    at = RE.lastIndex;
  }
  return out;
}

/** A value's text: a string's read, any other as written. */
export const jsonValue = (o: JsonOutline, id: number): string => (o.type[id] === J_STR ? unquote(o.text.slice(o.start[id], o.end[id])) : o.text.slice(o.start[id], o.end[id]));
