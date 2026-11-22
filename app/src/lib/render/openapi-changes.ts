/**
 * What changed in an API between two versions of its spec (ux31, section 13), for a comparison's *Rendered* stage:
 * operations by `method path` (and a 3.1 webhook by `method name`), schemas by name, each compared by its JSON with
 * refs left as refs and keys in any order, so a reordered document is no change and a reworded description is.
 *
 * A schema that changed marks the operations that reach it through their refs, directly or through other schemas, as
 * *Changed through* the first changed schema they reach, unless the operation changed itself. Pure: the spec worker
 * runs it (`SpecFiles.changes`), the tests read it.
 */
import { METHODS } from "./openapi";

/** One side's marks: added and removed by key, changed by key with the schema it changed through, where it did. */
export interface SpecChanges {
  ops: { added: string[]; removed: string[]; changed: { key: string; via?: string }[] };
  schemas: { added: string[]; removed: string[]; changed: string[] };
}

type Obj = Record<string, unknown>;
const isObj = (v: unknown): v is Obj => typeof v === "object" && v !== null && !Array.isArray(v);

/** `value` as JSON with every object's keys sorted: the same API written in another order reads the same. */
export function stableJson(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(stableJson).join(",")}]`;
  if (isObj(value)) {
    return `{${Object.keys(value)
      .sort()
      .filter((k) => value[k] !== undefined)
      .map((k) => `${JSON.stringify(k)}:${stableJson(value[k])}`)
      .join(",")}}`;
  }
  return JSON.stringify(value) ?? "null";
}

/** A document's schemas, by name: 3.x's `components.schemas`, 2.0's `definitions`. */
function schemasOf(doc: Obj): Obj {
  const components = doc.components;
  if (isObj(components) && isObj(components.schemas)) return components.schemas;
  return isObj(doc.definitions) ? doc.definitions : {};
}

/** A document's operations by key, each with the path item's own parameters and servers, which it inherits. */
function opsOf(doc: Obj): Map<string, unknown> {
  const out = new Map<string, unknown>();
  const take = (items: unknown) => {
    if (!isObj(items)) return;
    for (const [path, item] of Object.entries(items)) {
      if (!isObj(item)) continue;
      for (const method of METHODS) {
        const op = item[method];
        if (op === undefined) continue;
        out.set(`${method} ${path}`, { op, parameters: item.parameters, servers: item.servers });
      }
    }
  };
  take(doc.paths);
  take(doc.webhooks);
  return out;
}

/** The schema names a node's local refs name (`#/components/schemas/Pet`, `#/definitions/Pet`). */
function schemaRefs(node: unknown, into: Set<string> = new Set()): Set<string> {
  if (Array.isArray(node)) for (const v of node) schemaRefs(v, into);
  else if (isObj(node)) {
    for (const [k, v] of Object.entries(node)) {
      if (k === "$ref" && typeof v === "string") {
        const m = /^#\/(?:components\/schemas|definitions)\/(.+)$/.exec(v);
        if (m !== null) into.add(m[1]!.replace(/~1/g, "/").replace(/~0/g, "~"));
      } else schemaRefs(v, into);
    }
  }
  return into;
}

/** Added, removed and changed keys between two maps of JSON texts. */
function compare(before: Map<string, string>, after: Map<string, string>) {
  const added = [...after.keys()].filter((k) => !before.has(k));
  const removed = [...before.keys()].filter((k) => !after.has(k));
  const changed = [...after.keys()].filter((k) => before.has(k) && before.get(k) !== after.get(k));
  return { added, removed, changed };
}

/** What changed from `before` to `after`, two parsed spec documents. */
export function specChanges(before: unknown, after: unknown): SpecChanges {
  const a = isObj(before) ? before : {};
  const b = isObj(after) ? after : {};
  const schemasA = schemasOf(a);
  const schemasB = schemasOf(b);
  const schemaText = (s: Obj) => new Map(Object.entries(s).map(([k, v]) => [k, stableJson(v)]));
  const schemas = compare(schemaText(schemasA), schemaText(schemasB));

  const opsA = opsOf(a);
  const opsB = opsOf(b);
  const opText = (m: Map<string, unknown>) => new Map([...m].map(([k, v]) => [k, stableJson(v)]));
  const ops = compare(opText(opsA), opText(opsB));

  // A changed schema reaches the operations whose refs lead to it, on the side after.
  const changedSchemas = new Set(schemas.changed);
  const own = new Set(ops.changed);
  const through: { key: string; via: string }[] = [];
  if (changedSchemas.size > 0) {
    const refsOf = new Map<string, Set<string>>();
    const refs = (name: string) => {
      let r = refsOf.get(name);
      if (r === undefined) refsOf.set(name, (r = schemaRefs(schemasB[name])));
      return r;
    };
    for (const [key, op] of opsB) {
      if (own.has(key) || !opsA.has(key)) continue;
      // Breadth first, so the schema named is the nearest changed one; each name once, so a cycle ends.
      const seen = new Set<string>();
      let level = [...schemaRefs(op)];
      let via: string | undefined;
      while (level.length > 0 && via === undefined) {
        const next: string[] = [];
        for (const name of level) {
          if (seen.has(name)) continue;
          seen.add(name);
          if (changedSchemas.has(name)) {
            via = name;
            break;
          }
          next.push(...refs(name));
        }
        level = next;
      }
      if (via !== undefined) through.push({ key, via });
    }
  }
  const order = new Map([...opsB.keys()].map((k, n) => [k, n]));
  const changed = [...ops.changed.map((key) => ({ key })), ...through].sort((x, y) => order.get(x.key)! - order.get(y.key)!);
  return { ops: { added: ops.added, removed: ops.removed, changed }, schemas };
}

/** Whether nothing an API says differs. */
export function sameApi(c: SpecChanges): boolean {
  return c.ops.added.length + c.ops.removed.length + c.ops.changed.length + c.schemas.added.length + c.schemas.removed.length + c.schemas.changed.length === 0;
}
