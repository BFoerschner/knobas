/**
 * Whether a value fits a schema, as far as an example made from it must (ux31): types, `enum` and `const`, bounds,
 * lengths, a `pattern`, `required`, `allOf` (every part), `oneOf` and `anyOf` (at least one), arrays' items. Refs
 * are followed through the spec's files. `null` where the example's generator cut a cycle or its depth is taken as
 * cut, not as wrong. Used by the tests that hold made examples to their schemas.
 */
import type { SpecFiles } from "./openapi";

type Obj = Record<string, unknown>;
const isObj = (v: unknown): v is Obj => typeof v === "object" && v !== null && !Array.isArray(v);

/** Why `value` does not fit schema `node` of file `base`, or `null` where it does. */
export function validates(files: SpecFiles, base: string, node: unknown, value: unknown, direction: "request" | "response" | "any", path = "$", depth = 0): string | null {
  if (depth > 40) return null;
  let n = node;
  let at = base;
  if (isObj(n) && typeof n.$ref === "string") {
    const r = files.resolve(at, n.$ref);
    if (!("ok" in r) || !r.ok) return null;
    n = r.node;
    at = r.base;
  }
  if (!isObj(n)) return null;
  if (value === null) return null;
  if (n.const !== undefined && JSON.stringify(n.const) !== JSON.stringify(value)) return `${path}: not the const`;
  if (Array.isArray(n.enum) && !n.enum.some((e) => JSON.stringify(e) === JSON.stringify(value))) return `${path}: ${JSON.stringify(value)} not one of the enum`;
  if (Array.isArray(n.allOf)) {
    for (const part of n.allOf) {
      const why = validates(files, at, part, value, direction, path, depth + 1);
      if (why !== null) return why;
    }
  }
  for (const key of ["oneOf", "anyOf"] as const) {
    const list = n[key];
    if (Array.isArray(list) && list.length > 0) {
      const whys = list.map((m) => validates(files, at, m, value, direction, path, depth + 1));
      if (!whys.includes(null)) return whys[0]!;
    }
  }
  const types = Array.isArray(n.type) ? (n.type as string[]) : typeof n.type === "string" ? [n.type] : [];
  if (types.length > 0) {
    const kind = Array.isArray(value) ? "array" : typeof value === "number" ? (Number.isInteger(value) ? "integer" : "number") : typeof value === "object" ? "object" : typeof value;
    const fits = types.some((t) => t === kind || (t === "number" && kind === "integer"));
    if (!fits) return `${path}: a ${kind}, not ${types.join(" or ")}`;
  }
  if (typeof value === "number") {
    if (typeof n.minimum === "number" && value < n.minimum) return `${path}: under its minimum`;
    if (typeof n.maximum === "number" && value > n.maximum) return `${path}: over its maximum`;
    if (typeof n.exclusiveMinimum === "number" && value <= n.exclusiveMinimum) return `${path}: not over its exclusive minimum`;
    if (typeof n.exclusiveMaximum === "number" && value >= n.exclusiveMaximum) return `${path}: not under its exclusive maximum`;
  }
  if (typeof value === "string") {
    if (typeof n.minLength === "number" && value.length < n.minLength) return `${path}: shorter than its minLength`;
    if (typeof n.maxLength === "number" && value.length > n.maxLength) return `${path}: longer than its maxLength`;
    if (typeof n.pattern === "string") {
      let re: RegExp | null = null;
      try {
        re = new RegExp(n.pattern, "u");
      } catch {
        re = null;
      }
      if (re !== null && !re.test(value)) return `${path}: ${JSON.stringify(value)} does not match ${n.pattern}`;
    }
  }
  if (Array.isArray(value)) {
    if (typeof n.minItems === "number" && value.length < n.minItems) return `${path}: fewer items than its minItems`;
    for (let i = 0; i < value.length; i++) {
      const why = validates(files, at, n.items, value[i], direction, `${path}[${i}]`, depth + 1);
      if (why !== null) return why;
    }
  }
  if (isObj(value) && isObj(n.properties)) {
    for (const [name, v] of Object.entries(value)) {
      const why = validates(files, at, n.properties[name], v, direction, `${path}.${name}`, depth + 1);
      if (why !== null) return why;
    }
    if (Array.isArray(n.required)) {
      for (const name of n.required) {
        if (typeof name !== "string" || name in value) continue;
        const prop = n.properties[name];
        // A required name the schema never defines is the spec's own contradiction (GitHub's labels' `archived_at`).
        if (prop === undefined) continue;
        const own = isObj(prop) && typeof prop.$ref === "string" ? (() => { const r = files.resolve(at, prop.$ref as string); return "ok" in r && r.ok ? r.node : prop; })() : prop;
        const flags = isObj(own) ? own : {};
        // A field left out for its direction is not missing.
        if ((direction === "request" && flags.readOnly === true) || (direction === "response" && flags.writeOnly === true)) continue;
        return `${path}: lacks its required ${name}`;
      }
    }
  }
  return null;
}
