/**
 * What the OpenAPI view draws from a spec's index (ux18): its endpoints grouped, the filter over them, the rows a
 * windowed list draws, and a schema's one-line type. Pure, so the view's tests and the 5,000-operation timing read
 * the same code the view runs.
 */
import type { OpRow, PathRef, SpecIndex } from "./openapi";

export interface Group {
  /** The group's key in the rail's fold set: `g:<name>` for a tag, `g:/<path>` for a path group. */
  key: string;
  /** The tag's name, or the first part of the path that tells operations no tag holds apart (`/pets`, `/v1/charges`). */
  name: string;
  /** The start every path of the group shares (`/app/rest`), said once on its head; `""` for none. */
  shared: string;
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

const partsOf = (path: string) => path.split("/").filter((s) => s !== "");

/**
 * How many leading parts every path shares (ux31): never the whole of the shortest path, never a `{parameter}` or
 * past one. `/v1/charges` and `/v1/customers` share 1; `/a/{id}` and `/a/{id}/b` share 1.
 */
export function commonSegments(paths: readonly string[]): number {
  if (paths.length === 0) return 0;
  const split = paths.map(partsOf);
  let n = 0;
  for (;;) {
    const seg = split[0]![n];
    if (seg === undefined || seg.startsWith("{")) return n;
    if (!split.every((s) => s.length > n + 1 && s[n] === seg)) return n;
    n++;
  }
}

/** The start `n` parts long that `path` has: `/app/rest` for 2 of `/app/rest/builds`; `""` for 0. */
function startOf(path: string, n: number): string {
  return n === 0 ? "" : `/${partsOf(path).slice(0, n).join("/")}`;
}

/**
 * The spec's groups: one per tag in the spec's order, an operation under each tag it names (as the spec puts it in
 * each), then the operations no tag holds by the first part of their path that tells them apart, in the order the
 * paths come: Stripe's 612 operations all start `/v1`, so they group by the part after it. Each group says the
 * start its paths share.
 */
export function groupsOf(index: SpecIndex): Group[] {
  const byTag = new Map<string, Group>();
  for (const tag of index.tags) byTag.set(tag.name, { key: `g:${tag.name}`, name: tag.name, shared: "", code: false, description: tag.description, ops: [], refs: [] });
  const untagged = [...index.ops.filter((op) => op.tags.length === 0).map((op) => op.path), ...index.pathRefs.map((r) => r.path)];
  const skip = untagged.length < 2 ? 0 : commonSegments(untagged);
  const byPath = new Map<string, Group>();
  const pathGroup = (path: string) => {
    const parts = partsOf(path);
    const name = parts.length === 0 ? "/" : `/${parts.slice(0, skip + 1).join("/")}`;
    let g = byPath.get(name);
    if (g === undefined) {
      g = { key: `g:${name}`, name, shared: "", code: true, description: "", ops: [], refs: [] };
      byPath.set(name, g);
    }
    return g;
  };
  for (const op of index.ops) {
    if (op.tags.length === 0) pathGroup(op.path).ops.push(op);
    else for (const tag of op.tags) byTag.get(tag)?.ops.push(op);
  }
  for (const ref of index.pathRefs) pathGroup(ref.path).refs.push(ref);
  const groups = [...[...byTag.values()].filter((g) => g.ops.length > 0), ...byPath.values()];
  for (const g of groups) {
    // A tag's group says its paths' shared start once; a path group's name is its start already.
    if (g.code) continue;
    const paths = g.ops.map((o) => o.path);
    g.shared = paths.length < 2 ? "" : startOf(paths[0]!, commonSegments(paths));
  }
  return groups;
}

/** One folder of the path tree: a path part, or a chain of single folders joined (`app/rest`). */
export interface Dir {
  /** Its parts from its parent, joined (`app/rest`). */
  name: string;
  /** Its whole path from the top (`/app/rest`): its key in the fold set is `p:` and this. */
  path: string;
  dirs: Dir[];
  ops: OpRow[];
  /** Every operation under it. */
  count: number;
}

/**
 * The path tree (ux31): a folder a path part, an operation under the folder of its last part. A folder that holds
 * one folder and nothing else is one row with it (`/app/rest`), as the JAR view's are.
 */
export function pathTree(ops: readonly OpRow[]): Dir {
  type Node = { name: string; path: string; kids: Map<string, Node>; ops: OpRow[] };
  const top: Node = { name: "", path: "", kids: new Map(), ops: [] };
  for (const op of ops) {
    let at = top;
    for (const seg of partsOf(op.path)) {
      let kid = at.kids.get(seg);
      if (kid === undefined) {
        kid = { name: seg, path: `${at.path}/${seg}`, kids: new Map(), ops: [] };
        at.kids.set(seg, kid);
      }
      at = kid;
    }
    at.ops.push(op);
  }
  const settle = (node: Node, name: string): Dir => {
    let at = node;
    let joined = name;
    // Only a folder with no operation of its own and one folder in it joins that folder.
    while (at.ops.length === 0 && at.kids.size === 1 && at !== top) {
      const only = at.kids.values().next().value!;
      joined = `${joined}/${only.name}`;
      at = only;
    }
    const dirs = [...at.kids.values()].map((k) => settle(k, k.name));
    return { name: joined, path: at.path, dirs, ops: at.ops, count: at.ops.length + dirs.reduce((n, d) => n + d.count, 0) };
  };
  return settle(top, "");
}

/**
 * A path cut to `chars` characters of the code face (ux31): its first part and its last whole parts around an
 * ellipsis (`/organizations/…/retention-limit`), since the two ends tell endpoints apart. A path that fits, or of
 * two parts or fewer, is whole. A part is never cut inside.
 */
export function fitPath(path: string, chars: number): string {
  if (path.length <= chars) return path;
  const segs = partsOf(path);
  if (segs.length < 3) return path;
  const head = `/${segs[0]}/…`;
  let tail = "";
  for (let i = segs.length - 1; i >= 1; i--) {
    const next = `/${segs[i]}${tail}`;
    if (head.length + next.length > chars && tail !== "") break;
    tail = next;
  }
  return `${head}${tail}`;
}

/** The rail's three ways to list a spec. */
export type RailMode = "tags" | "paths" | "schemas";

/** One row of the explorer's rail; every row is 28 px. `d` is its depth. */
export type RailRow =
  | { k: string; kind: "overview"; d: 0 }
  | { k: string; kind: "group"; d: 0; group: Group; open: boolean }
  | { k: string; kind: "dir"; d: number; dir: Dir; open: boolean }
  | { k: string; kind: "webhooks"; d: 0; n: number; open: boolean }
  | { k: string; kind: "op"; d: number; op: OpRow; shown: string; marks: readonly number[] | null }
  | { k: string; kind: "pref"; d: number; ref: PathRef }
  | { k: string; kind: "schema"; d: 0; name: string; marks: readonly number[] | null };

/** What `railRows` reads besides the index. */
export interface RailInput {
  mode: RailMode;
  groups: readonly Group[];
  tree: Dir | null;
  /** The folds open, by key. */
  open: ReadonlySet<string>;
  /** With words typed: the operations kept, best first, and the schemas kept, each with its marked spans. */
  ranked: { ops: readonly { op: OpRow; marks: readonly number[] | null }[]; schemas: readonly { name: string; marks: readonly number[] | null }[] } | null;
}

/** A path drawn after the start its group shares: `/builds` under *Build* `/app/rest`; the whole where none is left. */
function afterShared(path: string, shared: string): string {
  if (shared === "" || !path.startsWith(shared)) return path;
  const rest = path.slice(shared.length);
  return rest === "" ? path : rest;
}

/**
 * The rows the rail draws (ux31, section 5). With words typed the list is flat, best first: operations with their
 * whole path (not in *Schemas*), then the schemas kept. Without, *Overview* first, then the mode's own order:
 * *Tags* one fold a group, *Paths* the tree, *Schemas* the names; 3.1's webhooks are a fold after the groups.
 */
export function railRows(index: SpecIndex, input: RailInput): RailRow[] {
  const out: RailRow[] = [];
  const { mode, open, ranked } = input;
  if (ranked !== null) {
    if (mode !== "schemas") for (const r of ranked.ops) out.push({ k: `o:${r.op.i}`, kind: "op", d: 0, op: r.op, shown: r.op.path, marks: r.marks });
    for (const s of ranked.schemas) out.push({ k: `s:${s.name}`, kind: "schema", d: 0, name: s.name, marks: s.marks });
    return out;
  }
  if (mode === "schemas") {
    for (const name of index.schemas) out.push({ k: `s:${name}`, kind: "schema", d: 0, name, marks: null });
    return out;
  }
  out.push({ k: "v", kind: "overview", d: 0 });
  if (mode === "tags") {
    for (const group of input.groups) {
      const isOpen = open.has(group.key);
      out.push({ k: group.key, kind: "group", d: 0, group, open: isOpen });
      if (!isOpen) continue;
      for (const op of group.ops) out.push({ k: `${group.key}\n${op.i}`, kind: "op", d: 1, op, shown: afterShared(op.path, group.code ? "" : group.shared), marks: null });
      for (const ref of group.refs) out.push({ k: `${group.key}\nr${ref.path}`, kind: "pref", d: 1, ref });
    }
  } else {
    const walk = (dir: Dir, d: number) => {
      for (const kid of dir.dirs) {
        const key = `p:${kid.path}`;
        const isOpen = open.has(key);
        out.push({ k: key, kind: "dir", d, dir: kid, open: isOpen });
        if (!isOpen) continue;
        for (const op of kid.ops) out.push({ k: `${key}\n${op.i}`, kind: "op", d: d + 1, op, shown: `/${partsOf(op.path).at(-1) ?? ""}`, marks: null });
        walk(kid, d + 1);
      }
    };
    const tree = input.tree;
    if (tree !== null) {
      for (const op of tree.ops) out.push({ k: `p:\n${op.i}`, kind: "op", d: 0, op, shown: op.path, marks: null });
      walk(tree, 0);
    }
  }
  if (index.webhooks.length > 0) {
    const isOpen = open.has("w");
    out.push({ k: "w", kind: "webhooks", d: 0, n: index.webhooks.length, open: isOpen });
    if (isOpen) for (const op of index.webhooks) out.push({ k: `w\n${op.i}`, kind: "op", d: 1, op, shown: op.path, marks: null });
  }
  return out;
}

/** How many operations a spec may have and still open with every fold open (section 5). */
export const OPEN_ALL_UP_TO = 60;

/**
 * The folds open when a spec opens: every group of a spec of 60 operations or fewer, none of a larger one, so its
 * first screen is the table of contents. In *Paths* a chain of single folders at the top is open, and so is the top
 * level where it has three folders or fewer.
 */
export function firstOpen(index: SpecIndex, groups: readonly Group[], tree: Dir | null): Set<string> {
  const open = new Set<string>();
  if (index.ops.length <= OPEN_ALL_UP_TO) {
    for (const g of groups) open.add(g.key);
    open.add("w");
  }
  if (tree !== null && tree.dirs.length <= 3) {
    let level = tree.dirs;
    while (level.length > 0 && level.length <= 3) {
      for (const d of level) open.add(`p:${d.path}`);
      if (level.length !== 1) break;
      level = level[0]!.dirs;
    }
  }
  return open;
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
  /** A choice of the schema and the empty string (Stripe's `anyOf [X, enum [""]]`). */
  empty?: boolean;
}

/** A schema that says only null or the empty string: what `oneOf: [X, null]` adds to X. */
function nullish(m: unknown): boolean {
  return isObj(m) && (m.type === "null" || (Array.isArray(m.enum) && m.enum.length === 1 && (m.enum[0] === null || m.enum[0] === "")));
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
  for (const key of ["oneOf", "anyOf"] as const) {
    const list = node[key];
    if (Array.isArray(list) && types.length === 0) {
      // A choice of one schema and null (or the empty string) is that schema, or null (or empty).
      const real = list.filter((m) => !nullish(m));
      if (real.length === 1) {
        const one = summary(real[0]);
        const empty = list.some((m) => isObj(m) && Array.isArray(m.enum) && m.enum.length === 1 && m.enum[0] === "");
        return { ...one, type: empty && one.ref === null ? `${one.type}, or empty` : one.type, nullable: one.nullable || nullable || list.some((m) => isObj(m) && m.type === "null"), empty };
      }
      return { type: `${key === "oneOf" ? "one of" : "any of"} ${real.length}`, ref: null, nullable, enumValues };
    }
  }
  if (Array.isArray(node.allOf) && types.length === 0) {
    // `allOf` of one ref is that ref (GitHub's `nullable-simple-user`); of more, an object merged.
    const parts = node.allOf;
    if (parts.length === 1) return { ...summary(parts[0]), nullable: nullable || summary(parts[0]).nullable };
    return { type: "object", ref: null, nullable, enumValues };
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

/**
 * Whether a schema has more than its one line: properties, items with their own, a composition, a ref to open. A
 * caret stands only where the opened block would draw a row (Fable, 2026-11-22): `anyOf [string, ""]` draws none.
 */
export function hasInside(node: unknown): boolean {
  if (!isObj(node)) return false;
  if (typeof node.$ref === "string") return true;
  if (isObj(node.properties) && Object.keys(node.properties).length > 0) return true;
  if (Array.isArray(node.allOf)) return true;
  for (const key of ["oneOf", "anyOf"] as const) {
    const list = node[key];
    if (!Array.isArray(list)) continue;
    const real = list.filter((m) => !nullish(m));
    if (real.length > 1 || real.some(hasInside)) return true;
  }
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

// ---- What a page says in words (ux31) ----

/**
 * A description held to about `max` characters (section 7: an operation's 700, the overview's 900): cut at a
 * paragraph's end where one falls between 60 % and 160 % of `max`, else at `max`. `cut` says *Show all* is owed.
 */
export function clampWords(text: string, max: number): { text: string; cut: boolean } {
  const t = text.replace(/\r/g, "").trim();
  if (max <= 0 || t.length <= max) return { text: t, cut: false };
  const stop = t.indexOf("\n\n", Math.floor(max * 0.6));
  return { text: t.slice(0, stop > 0 && stop < max * 1.6 ? stop : max).trimEnd(), cut: true };
}

/** Whether `text` holds anything Markdown would draw: plain text is drawn as text, which is cheaper by far. */
export function needsMarkdown(text: string): boolean {
  return /[`*_[\]#<>|!\\]|\n\s*\n|^\s*[-+]\s|^\s*\d+\.\s/m.test(text);
}

/** One run of a fact's line: words, or a value in the code face. */
export interface Run {
  text: string;
  code: boolean;
}

const runOf = (v: unknown): Run => ({ text: typeof v === "string" ? v : (v === undefined ? String(v) : JSON.stringify(v)), code: true });
const words = (text: string): Run => ({ text, code: false });

/** How many values of an `enum` a field's line draws before it counts the rest. */
export const ENUM_SHOWN = 14;

/**
 * A field's line of facts (section 8): *One of* `a` `b`, *Default*, *Example*, its bounds in words, *matches* its
 * pattern. Each fact is a list of runs; the view joins them with full stops.
 */
export function factsOf(node: unknown, example?: unknown): Run[][] {
  if (!isObj(node)) return example === undefined ? [] : [[words("Example "), runOf(example)]];
  const out: Run[][] = [];
  if (Array.isArray(node.enum)) {
    const shown = node.enum.slice(0, ENUM_SHOWN);
    const fact: Run[] = [words("One of ")];
    shown.forEach((v, i) => {
      if (i > 0) fact.push(words(" "));
      fact.push(runOf(v));
    });
    if (node.enum.length > ENUM_SHOWN) fact.push(words(` and ${node.enum.length - ENUM_SHOWN} more`));
    out.push(fact);
  } else if (node.const !== undefined) out.push([words("Always "), runOf(node.const)]);
  if (node.default !== undefined) out.push([words("Default "), runOf(node.default)]);
  const ex = example !== undefined ? example : node.example;
  if (ex !== undefined && (example !== undefined || (!isObj(ex) && !Array.isArray(ex)))) out.push([words("Example "), runOf(ex)]);
  const range: string[] = [];
  const num = (v: unknown) => (typeof v === "number" ? v.toLocaleString("en") : String(v));
  if (node.minimum !== undefined) range.push(`at least ${num(node.minimum)}`);
  if (typeof node.exclusiveMinimum === "number") range.push(`more than ${num(node.exclusiveMinimum)}`);
  if (node.maximum !== undefined) range.push(`at most ${num(node.maximum)}`);
  if (typeof node.exclusiveMaximum === "number") range.push(`less than ${num(node.exclusiveMaximum)}`);
  if (node.minLength !== undefined) range.push(`${num(node.minLength)} ${node.minLength === 1 ? "character" : "characters"} or more`);
  if (node.maxLength !== undefined) range.push(`${num(node.maxLength)} characters at most`);
  if (node.minItems !== undefined) range.push(`${num(node.minItems)} ${node.minItems === 1 ? "item" : "items"} or more`);
  if (node.maxItems !== undefined) range.push(`${num(node.maxItems)} items at most`);
  if (node.uniqueItems === true) range.push("no item twice");
  if (range.length > 0) {
    const line = range.join(", ");
    out.push([words(line.charAt(0).toUpperCase() + line.slice(1))]);
  }
  if (typeof node.pattern === "string") out.push([words(range.length > 0 ? "Matches " : "Matches "), runOf(node.pattern)]);
  return out;
}

/** What a security scheme asks for, in words (section 9). */
export function schemeWords(s: { type: string; scheme: string; in: string; keyName: string; bearerFormat: string }): string {
  if (s.type === "http") {
    if (s.scheme === "basic") return "A user name and a password (Basic)";
    if (s.scheme === "bearer") return `A bearer token${s.bearerFormat !== "" ? ` (${s.bearerFormat})` : ""}`;
    return `HTTP ${s.scheme}`;
  }
  if (s.type === "apiKey") return `A key in the ${s.in === "query" ? "query" : s.in === "cookie" ? "cookie" : "header"} ${s.keyName}`;
  if (s.type === "oauth2") return "OAuth 2.0: an access token, which you paste";
  if (s.type === "openIdConnect") return "OpenID Connect: an access token, which you paste";
  if (s.type === "mutualTLS") return "A client certificate (mutual TLS)";
  return s.type;
}

/** The method as a row draws it: `DELETE` and `OPTIONS` cut to `DEL` and `OPT` for the rail's 34 px column. */
export function shortMethod(method: string): string {
  return method === "delete" ? "DEL" : method === "options" ? "OPT" : method.toUpperCase();
}

/**
 * A name's parts, cut where it may wrap (Fable, 2026-11-22): after `_ - . / :` and before a capital that follows a
 * small letter or a digit, so `payment_method_configuration_details` wraps at an underscore. No lookbehind: WebKit
 * before 16.4 refuses to parse one.
 */
export const nameParts = (name: string): string[] => name.replace(/([_\-./:])(?=.)|([a-z0-9])(?=[A-Z])/g, "$1$2\u0000").split("\u0000");

/** A path's parts, its `{parameters}` marked to draw dim. */
export function pathParts(p: string): { text: string; param: boolean }[] {
  return p
    .split(/(\{[^}]*\})/)
    .filter((s) => s !== "")
    .map((s) => ({ text: s, param: s.startsWith("{") }));
}

/** A count in words: `1 endpoint`, `1,281 endpoints`. */
export function counted(n: number, one: string, many: string): string {
  return `${n.toLocaleString("en")} ${n === 1 ? one : many}`;
}

/** Whether `text` holds Markdown that is more than a line's: a list, a heading, a fence, a table, a quote. */
export function needsBlocks(text: string): boolean {
  return /^\s*([-*+]|\d+\.)\s|^\s*#{1,6}\s|```|^\s*\||^\s*>/m.test(text);
}

/** One run of inline Markdown: text, `code`, *em*, **strong**, or a link's words with its address. */
export interface InlineRun {
  t: string;
  k: "" | "code" | "em" | "strong" | "link";
  href?: string;
}

const INLINE = /`([^`]+)`|\*\*([^*]+)\*\*|__([^_]+)__|\[([^\]]+)\]\(([^)\s]+)[^)]*\)|(?<![\w*])\*([^*\s][^*]*?)\*(?![\w*])|(?<![\w_])_([^_\s][^_]*?)_(?![\w_])|<br\s*\/?>/g;

/**
 * A description's paragraphs as runs of inline Markdown, read as text, never markup (ux31): a field's words are
 * mostly a line or two with `code`, *emphasis* and [links](…), and drawing each through the page's whole Markdown
 * renderer made a schema of 86 fields cost 150 ms at 4x. Anything more than inline goes to the renderer.
 */
export function inlineMarkdown(text: string): InlineRun[][] {
  return text
    .replace(/\r/g, "")
    .split(/\n{2,}/)
    .map((para) => {
      const runs: InlineRun[] = [];
      const flat = para.replace(/\n/g, " ");
      let at = 0;
      for (const m of flat.matchAll(INLINE)) {
        if (m.index > at) runs.push({ t: flat.slice(at, m.index), k: "" });
        if (m[1] !== undefined) runs.push({ t: m[1], k: "code" });
        else if (m[2] !== undefined || m[3] !== undefined) runs.push({ t: (m[2] ?? m[3])!, k: "strong" });
        else if (m[4] !== undefined) runs.push({ t: m[4], k: "link", href: m[5]! });
        else if (m[6] !== undefined || m[7] !== undefined) runs.push({ t: (m[6] ?? m[7])!, k: "em" });
        else runs.push({ t: " ", k: "" });
        at = m.index + m[0].length;
      }
      if (at < flat.length) runs.push({ t: flat.slice(at), k: "" });
      return runs;
    })
    .filter((runs) => runs.length > 0);
}
