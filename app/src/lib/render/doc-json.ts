/**
 * The JSON view's half of the document model (the XML view's design, 2026-11-23, section 25): what `doc-model.ts`
 * answers for an XML outline, for a JSON one. Objects and arrays fold and say ***48** items* or ***9** members*; the
 * way down is a jq path; the values looked at are strings, with their sibling members saying what they are.
 */
import { ITEMS, MEMBERS, type FoldTable } from "./doc-folds";
import { J_ARR, J_LIT, J_NUM, J_OBJ, J_STR, jsonPathOf, jsonStepOf, jsonValue, type JsonOutline } from "./json-outline";
import type { Crumb, Picked, ResultRow, ValueAt } from "./doc-model";

/** The first value that starts on each line (from 1), or -1. */
export function jsonByLine(o: JsonOutline): Int32Array {
  const by = new Int32Array(o.lines + 2).fill(-1);
  for (let i = 1; i < o.n; i += 1) {
    const l = o.line[i]!;
    if (by[l] === -1) by[l] = i;
  }
  return by;
}

const holds = (o: JsonOutline, i: number) => o.type[i] === J_OBJ || o.type[i] === J_ARR;

/** The rows that fold, in order of their line: the first object or array that starts on a line and ends on a later one. */
export function jsonFoldRows(o: JsonOutline): Int32Array {
  const taken = new Uint8Array(o.lines + 2);
  const rows = new Int32Array(Math.max(0, o.lines));
  let n = 0;
  for (let i = 1; i < o.n; i += 1) {
    if (!holds(o, i)) continue;
    const l = o.line[i]!;
    if (Math.min(o.endLine[i]!, o.lines) > l && taken[l] === 0) {
      taken[l] = 1;
      rows[n++] = i;
    }
  }
  return rows.subarray(0, n);
}

/** What folds, as the page reads it: an array counts its items, an object its members. */
export function jsonFoldTable(o: JsonOutline, rows: Int32Array): FoldTable {
  const byLine = new Int32Array(o.lines + 2).fill(-1);
  const n = rows.length;
  const line = new Uint32Array(n);
  const end = new Uint32Array(n);
  const depth = new Uint16Array(n);
  const count = new Uint32Array(n);
  const unit = new Uint8Array(n);
  const closer = new Uint32Array(n);
  for (let f = 0; f < n; f += 1) {
    const i = rows[f]!;
    line[f] = o.line[i]!;
    byLine[line[f]!] = f;
    end[f] = Math.min(o.endLine[i]!, o.lines);
    depth[f] = o.depth[i]!;
    count[f] = o.kids[i]!;
    unit[f] = o.type[i] === J_ARR ? ITEMS : MEMBERS;
    closer[f] = o.type[i] === J_ARR ? 0 : 1;
  }
  return { lines: o.lines, byLine, line, end, depth, count, unit, closer, closers: ["]", "}"] };
}

/** The value a line stands for: the one that starts on it, else the innermost object or array it lies inside; -1 for none. */
export function jsonOnLine(o: JsonOutline, byLine: Int32Array, line: number): number {
  const at = byLine[line] ?? -1;
  if (at !== -1) return at;
  for (let l = line - 1; l >= 1; l -= 1) {
    const e = byLine[l]!;
    if (e === -1) continue;
    for (let up = e; up > 0; up = o.parent[up]!) if (holds(o, up) && o.endLine[up]! >= line) return up;
    return -1;
  }
  return -1;
}

/** The way down to a value: each value above it and itself, by its member's name or its item's place. */
export function jsonCrumbs(o: JsonOutline, id: number): Crumb[] {
  const out: Crumb[] = [];
  for (let at = id; at > 0; at = o.parent[at]!) out.push({ name: jsonStepOf(o, at), line: o.line[at]! });
  return out.reverse();
}

const WHAT: Record<number, string> = { [J_OBJ]: "Object", [J_ARR]: "Array", [J_STR]: "String", [J_NUM]: "Number", [J_LIT]: "Literal" };
/** The most of a value the picked one shows, and the most names it counts (as an XML element's). */
const VALUE_SHOWN = 240;
const HOLDS_SHOWN = 14;

/** What a value holds, for the column beside the document: an object its members, an array its items by kind. */
export function jsonPicked(o: JsonOutline, id: number): Picked {
  const by = new Map<string, number>();
  for (let c = o.first[id]!; c !== -1; c = o.next[c]!) {
    const name = o.type[id] === J_OBJ ? o.keys[o.key[c]!]! : (WHAT[o.type[c]!] ?? "Value").toLowerCase();
    by.set(name, (by.get(name) ?? 0) + 1);
  }
  let value: string | null = null;
  let valueLength = 0;
  if (!holds(o, id)) {
    const whole = jsonValue(o, id);
    valueLength = whole.length;
    value = whole.length > VALUE_SHOWN ? `${whole.slice(0, VALUE_SHOWN - 3)}…` : whole;
  }
  return {
    node: id,
    name: jsonStepOf(o, id),
    what: WHAT[o.type[id]!] ?? "Value",
    line: o.line[id]!,
    path: jsonPathOf(o, id),
    namespace: null,
    attributes: [],
    holds: [...by].slice(0, HOLDS_SHOWN).map(([name, count]) => ({ name, count })),
    holdsMore: Math.max(0, by.size - HOLDS_SHOWN),
    value,
    valueLength,
  };
}

/** A string at least this long gets a chip (a JSON string's counted case, section 17). */
const STRING_AT_LEAST = 8;
const HEAD = 4096;
const SAYS_MIME = /^(?:type|contentType|content-type|content_type|mimeType|mime_type|mime|mediaType|media_type)$/i;
const SAYS_NAME = /^(?:fileName|filename|file_name|file-name|FILE_NAME)$/;

/**
 * The strings on a line, each a value a pipeline can take: the file's bytes `from` to `to` inside its quotes, read as
 * a JSON string. Its sibling members say what it is (`type`, `contentType`, `mime`) and what it is called. With `all`,
 * any string the line stands for, however short.
 */
export function jsonValuesOn(o: JsonOutline, byLine: Int32Array, line: number, all: boolean, byteAt: (at: number) => number, src: JsonOutline = o): ValueAt[] {
  const out: ValueAt[] = [];
  const first = all ? jsonOnLine(o, byLine, line) : (byLine[line] ?? -1);
  if (first <= 0) return out;
  // The values that start on this line: the first and the ones after it on the same line (a compact array's).
  const on: number[] = [];
  if (o.line[first] === line) for (let i = first; i < o.n && o.line[i] === line; i += 1) on.push(i);
  else on.push(first);
  for (const id of on) {
    if (o.type[id] !== J_STR) continue;
    // In a layout, a value's place is the file's: the same row of the file as written.
    const from = src.start[id]! + 1;
    const to = src.end[id]! - 1;
    if (!all && to - from < STRING_AT_LEAST) continue;
    const said: { mime?: string } = {};
    let fileName: string | null = null;
    const up = o.parent[id]!;
    if (up > 0 && o.type[up] === J_OBJ)
      for (let c = o.first[up]!; c !== -1; c = o.next[c]!) {
        if (c === id || o.type[c] !== J_STR) continue;
        const k = o.keys[o.key[c]!]!;
        const v = jsonValue(o, c).trim();
        if (SAYS_MIME.test(k) && /^[\w.+-]+\/[\w.+-]+/.test(v)) said.mime = v;
        if (SAYS_NAME.test(k) && v !== "") fileName = v;
      }
    const raw = src.text.slice(from, to);
    const head = jsonValue(o, id).slice(0, HEAD);
    let bare = 0;
    for (let i = 0; i < raw.length; i += 1) {
      const c = raw.charCodeAt(i);
      if (c !== 32 && c !== 10 && c !== 13 && c !== 9) bare += 1;
    }
    out.push({ line: o.line[id]!, node: id, name: jsonStepOf(o, id), field: fieldOf(o, id), from: byteAt(from), to: byteAt(to), read: "json_string", head, length: raw.length, bare, said, fileName });
  }
  return out;
}

/** A field kept for a chain is named by how its names end (section 18): the last two members, an array's items `[]`. */
export function fieldOf(o: JsonOutline, id: number): string {
  const parts: string[] = [];
  for (let at = id; at > 0 && o.parent[at]! > 0; at = o.parent[at]!) parts.push(o.type[o.parent[at]!] === J_ARR ? "[]" : `.${o.keys[o.key[at]!]}`);
  // `.orders[].attachments[].content` is `attachments[].content`: each name with the `[]` after it, the last two.
  return parts.reverse().join("").replace(/^\./, "").split(".").slice(-2).join(".");
}

/** The file laid out (section 6, a file written on one line): two spaces a level, every value written as it stands. */
export function layOutJson(o: JsonOutline): string {
  const out: string[] = [];
  const write = (id: number, ind: string) => {
    const t = o.type[id];
    const up = o.parent[id]!;
    const lead = up > 0 && o.type[up] === J_OBJ ? `${JSON.stringify(o.keys[o.key[id]!])}: ` : "";
    const comma = o.next[id] !== -1 && up > 0 ? "," : "";
    if ((t === J_OBJ || t === J_ARR) && o.kids[id]! > 0) {
      out.push(`${ind}${lead}${t === J_OBJ ? "{" : "["}`);
      for (let c = o.first[id]!; c !== -1; c = o.next[c]!) write(c, `${ind}  `);
      out.push(`${ind}${t === J_OBJ ? "}" : "]"}${comma}`);
    } else out.push(`${ind}${lead}${o.text.slice(o.start[id], o.end[id])}${comma}`);
  };
  for (let c = o.first[0]!; c !== -1; c = o.next[c]!) write(c, "");
  return `${out.join("\n")}\n`;
}

/** The most of a value a result's row shows. */
const ROW_CUT = 120;

/** One result as the rail draws it: its line, its member's name or place, its value cut, or what it holds. */
export function jsonRow(o: JsonOutline, id: number): ResultRow {
  const kids = holds(o, id) ? o.kids[id]! : 0;
  let value: string | null = null;
  if (!holds(o, id)) {
    const v = o.text.slice(o.start[id], o.end[id]);
    value = v.length > ROW_CUT ? `${v.slice(0, ROW_CUT - 1)}…` : v;
  } else if (kids === 0) value = o.type[id] === J_ARR ? "[]" : "{}";
  const row: ResultRow = { node: id, line: o.line[id]!, name: jsonStepOf(o, id), value, kids };
  if (kids > 0) row.holds = o.type[id] === J_ARR ? "items" : "members";
  return row;
}

/**
 * Filters to press, made from the document's own members (the field's offers, as the XML view's are): its first array of
 * objects asked for its first item, its length, and its items with a number over a value it holds.
 */
export function jsonExamples(o: JsonOutline): string[] {
  const out: string[] = ["."];
  if (o.root <= 0) return out;
  let list = -1;
  for (let c = o.first[o.root]!; c !== -1 && list === -1; c = o.next[c]!) if (o.type[c] === J_ARR && o.kids[c]! > 0 && o.type[o.first[c]!] === J_OBJ) list = c;
  if (list === -1 && o.type[o.root] === J_ARR && o.kids[o.root]! > 0) list = o.root;
  if (list === -1) return [".", "keys", "paths(type == \"string\") | map(tostring) | join(\".\")"];
  // The root list is `.`: its items `.[0]`, its length `length`; a member's `.orders[0]`, `.orders | length`.
  const path = jsonPathOf(o, list);
  const root = path === ".";
  const piped = (f: string) => (root ? f : `${path} | ${f}`);
  out.push(`${root ? "." : path}[0]`, piped("length"));
  const item = o.first[list]!;
  for (let c = o.first[item]!; c !== -1; c = o.next[c]!) {
    if (o.type[c] !== J_NUM) continue;
    const name = o.keys[o.key[c]!]!;
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name)) continue;
    out.push(`${root ? "." : path}[] | select(.${name} > ${o.text.slice(o.start[c], o.end[c])})`, piped(`map(.${name}) | add`));
    break;
  }
  return out;
}

