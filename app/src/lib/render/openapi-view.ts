/**
 * What the OpenAPI view draws from a spec's index (ux18): its endpoints grouped, the filter over them, the rows a
 * windowed list draws, and a schema's one-line type. Pure, so the view's tests and the 5,000-operation timing read
 * the same code the view runs.
 */
import type { OpRow, PathRef, SpecIndex } from "./openapi";

export interface Group {
  /** The tag's name, or the path's first segment (`/pets`) for operations no tag holds. */
  name: string;
  /** A path's segment, drawn in the code face; a tag is a word. */
  code: boolean;
  description: string;
  ops: OpRow[];
  /** Paths whose items live in another file, listed under their first segment. */
  refs: PathRef[];
}

/** The path's first segment, `/` for the root: how operations no tag holds are grouped. */
export function firstSegment(path: string): string {
  const seg = path.split("/").find((s) => s !== "");
  return seg === undefined ? "/" : `/${seg}`;
}

/**
 * The spec's groups: one per tag in the spec's order, an operation under each tag it names (as the spec puts it in
 * each), then the operations no tag holds by their path's first segment, in the order the paths come.
 */
export function groupsOf(index: SpecIndex): Group[] {
  const byTag = new Map<string, Group>();
  for (const tag of index.tags) byTag.set(tag.name, { name: tag.name, code: false, description: tag.description, ops: [], refs: [] });
  const byPath = new Map<string, Group>();
  const pathGroup = (path: string) => {
    const name = firstSegment(path);
    let g = byPath.get(name);
    if (g === undefined) {
      g = { name, code: true, description: "", ops: [], refs: [] };
      byPath.set(name, g);
    }
    return g;
  };
  for (const op of index.ops) {
    if (op.tags.length === 0) pathGroup(op.path).ops.push(op);
    else for (const tag of op.tags) byTag.get(tag)?.ops.push(op);
  }
  for (const ref of index.pathRefs) pathGroup(ref.path).refs.push(ref);
  return [...[...byTag.values()].filter((g) => g.ops.length > 0), ...byPath.values()];
}

/** What the filter reads of an operation: its method, path, summary, tags and operationId, lowercase. */
export function haystack(op: OpRow): string {
  return `${op.method} ${op.path} ${op.summary} ${op.tags.join(" ")} ${op.operationId ?? ""}`.toLowerCase();
}

/** The filter's words: each must be in an operation's haystack. */
export function terms(typed: string): string[] {
  return typed.toLowerCase().split(/\s+/).filter((t) => t !== "");
}

export function matches(hay: string, words: readonly string[]): boolean {
  for (const w of words) if (!hay.includes(w)) return false;
  return true;
}

export type Row =
  | { k: string; kind: "group"; group: Group; shown: number }
  | { k: string; kind: "op"; op: OpRow }
  | { k: string; kind: "detail"; op: OpRow }
  | { k: string; kind: "pref"; ref: PathRef }
  | { k: string; kind: "prefbody"; ref: PathRef }
  | { k: string; kind: "shead"; shown: number }
  | { k: string; kind: "schema"; name: string }
  | { k: string; kind: "sbody"; name: string };

/** The key of operation `op` under group `g`, for the windowed list and for an anchor. */
export const opKey = (g: number, op: OpRow) => `o${g}:${op.i}`;

/**
 * The rows the list draws: each group's head and its operations that the filter keeps, an open operation's detail
 * under it, then the schemas' head and the schemas whose names it keeps. A group the filter empties is left out;
 * the schemas' head stays while any schema is kept.
 */
export function rowsOf(
  groups: readonly Group[],
  hays: ReadonlyMap<OpRow, string>,
  schemas: readonly string[],
  words: readonly string[],
  open: { ops: ReadonlySet<number>; refs: ReadonlySet<string>; schemas: ReadonlySet<string> },
): Row[] {
  const rows: Row[] = [];
  groups.forEach((group, g) => {
    const head = rows.length;
    let shown = 0;
    rows.push({ k: `g${g}`, kind: "group", group, shown: 0 });
    for (const op of group.ops) {
      if (words.length > 0 && !matches(hays.get(op) ?? haystack(op), words)) continue;
      shown++;
      const k = opKey(g, op);
      rows.push({ k, kind: "op", op });
      if (open.ops.has(op.i)) rows.push({ k: `d${k}`, kind: "detail", op });
    }
    for (const ref of group.refs) {
      if (words.length > 0 && !matches(ref.path.toLowerCase(), words)) continue;
      shown++;
      rows.push({ k: `r${g}:${ref.path}`, kind: "pref", ref });
      if (open.refs.has(ref.path)) rows.push({ k: `dr${g}:${ref.path}`, kind: "prefbody", ref });
    }
    if (shown === 0) rows.length = head;
    else rows[head] = { k: `g${g}`, kind: "group", group, shown };
  });
  const kept = words.length === 0 ? schemas : schemas.filter((name) => matches(name.toLowerCase(), words));
  if (kept.length > 0) {
    rows.push({ k: "s", kind: "shead", shown: kept.length });
    for (const name of kept) {
      rows.push({ k: `s:${name}`, kind: "schema", name });
      if (open.schemas.has(name)) rows.push({ k: `ds:${name}`, kind: "sbody", name });
    }
  }
  return rows;
}

type Obj = Record<string, unknown>;
const isObj = (v: unknown): v is Obj => typeof v === "object" && v !== null && !Array.isArray(v);

/** A schema's one line: its type in words, and the ref it names where it is one (or an array of one). */
export interface Summary {
  /** `string`, `integer, int64`, `array of`, `object`, `one of 3`, `any`. */
  type: string;
  /** The ref drawn after `type` as a link (`Pet` in `array of Pet`). */
  ref: string | null;
  nullable: boolean;
  enumValues: string[] | null;
}

/** A schema's one line. */
export function summary(node: unknown): Summary {
  if (!isObj(node)) return { type: node === true ? "any" : node === false ? "nothing" : "any", ref: null, nullable: false, enumValues: null };
  if (typeof node.$ref === "string") return { type: "", ref: node.$ref, nullable: false, enumValues: null };
  let types = Array.isArray(node.type) ? node.type.filter((t): t is string => typeof t === "string") : typeof node.type === "string" ? [node.type] : [];
  // 3.1 writes `type: [string, "null"]`, 3.0 `nullable: true`.
  const nullable = node.nullable === true || types.includes("null");
  types = types.filter((t) => t !== "null");
  const enumValues = Array.isArray(node.enum) ? node.enum.map((v) => (typeof v === "string" ? v : JSON.stringify(v))) : null;
  for (const key of ["oneOf", "anyOf", "allOf"] as const) {
    const list = node[key];
    if (Array.isArray(list) && types.length === 0) {
      const words = key === "oneOf" ? "one of" : key === "anyOf" ? "any of" : "all of";
      return { type: `${words} ${list.length}`, ref: null, nullable, enumValues };
    }
  }
  if (types.length === 0) {
    if (isObj(node.properties) || isObj(node.additionalProperties)) types = ["object"];
    else if (node.items !== undefined) types = ["array"];
  }
  if (types.length === 1 && types[0] === "array") {
    const items = summary(node.items);
    if (items.ref !== null) return { type: "array of", ref: items.ref, nullable, enumValues };
    return { type: `array of ${items.type || "any"}`, ref: null, nullable, enumValues };
  }
  const format = typeof node.format === "string" ? `, ${node.format}` : "";
  const type = types.length === 0 ? (enumValues !== null ? "enum" : "any") : types.join(" or ");
  return { type: `${type}${format}`, ref: null, nullable, enumValues };
}

/** The ref's name as a reader knows it: a named schema's name, else the pointer's last part, else the file's name. */
export function refName(ref: string): string {
  const hash = ref.indexOf("#");
  const ptr = hash < 0 ? "" : ref.slice(hash + 1);
  const last = ptr.split("/").filter((s) => s !== "").pop();
  if (last !== undefined) return decodeURIComponent(last).replace(/~1/g, "/").replace(/~0/g, "~");
  const file = hash < 0 ? ref : ref.slice(0, hash);
  return file.slice(file.lastIndexOf("/") + 1) || ref;
}

/** Whether a schema has more than its one line: properties, items with their own, a composition, a ref to open. */
export function hasInside(node: unknown): boolean {
  if (!isObj(node)) return false;
  if (typeof node.$ref === "string") return true;
  if (isObj(node.properties) && Object.keys(node.properties).length > 0) return true;
  if (Array.isArray(node.oneOf) || Array.isArray(node.anyOf) || Array.isArray(node.allOf)) return true;
  if (isObj(node.additionalProperties)) return true;
  if (node.items !== undefined) return hasInside(node.items);
  return false;
}

/** An example's text: a string as it is, anything else as indented JSON, cut at {@link EXAMPLE_CAP} characters. */
export const EXAMPLE_CAP = 20_000;
export function exampleText(value: unknown): string {
  let text: string;
  try {
    // `JSON.stringify(undefined)` is `undefined` whatever its type says; an example with no value draws empty.
    text = typeof value === "string" ? value : value === undefined ? "" : JSON.stringify(value, null, 2);
  } catch {
    text = String(value);
  }
  return text.length > EXAMPLE_CAP ? `${text.slice(0, EXAMPLE_CAP)}\n…` : text;
}

/** Where a ref to a named schema of the spec itself lands: its name, else `null`. */
export function namedSchema(ref: string, schemaBase: string, schemas: ReadonlySet<string>): string | null {
  if (!ref.startsWith(schemaBase)) return null;
  const name = decodeURIComponent(ref.slice(schemaBase.length)).replace(/~1/g, "/").replace(/~0/g, "~");
  return schemas.has(name) ? name : null;
}
