/**
 * An example made from a schema, where the spec gives none (ux31; Björn, 2026-11-22: "if they can also be generated if
 * there's no example data that'd be insanely awesome"). It runs in the spec worker, over the shapes `openapi.ts`
 * merges, and is **deterministic**: the same schema always gives the same example, so a page drawn twice reads the
 * same and a test can hold it.
 *
 * - **The order a value is chosen in** (Björn, 2026-11-22): the spec's `example` or `default`; then `enum` or `const`;
 *   then the `format` (date-time, date, uuid, email, uri, ipv4, …); then the `pattern` (randexp, seeded, where it is
 *   simple and bounded); then the property's name (`example-names.ts`: `firstName`, `phone_number`, `plz`,
 *   `versicherungsnummer`, made by Faker, `example-fakes.ts`); then the type with its limits. A format or a pattern
 *   beats the name. A password is always a placeholder. `example` is the last resort, for a string whose name the
 *   table does not know.
 * - **A name is read with the thing that holds it** (2026-11-22 polish): a parameter's own name (`org`, `repo`); a
 *   property's object, named by its schema or the property that holds it. A free-text field (a description, a
 *   summary, a title, a message, a comment, notes) is a plain English phrase from both, `A short description of
 *   the repository`, never Faker's Latin; a bare `name` is a person's under a user or a contact, a repository's under
 *   a repository, and an identifier elsewhere.
 * - A string is held to `minLength` and `maxLength`; a number to `minimum`, `maximum`, their exclusive forms and
 *   `multipleOf`.
 * - An object holds every required property and the first few others; `allOf` is merged; `oneOf` and `anyOf` take
 *   the choice asked for at the top (the first by default) and the first below it.
 * - **Direction**: a request leaves out `readOnly` properties, an answer `writeOnly` ones.
 * - **Recursion is capped**: a ref met again on the way down is `null`, and nothing goes more than {@link DEPTH} deep.
 */
import type { Shape } from "./openapi";
import { fakeInt, fakePattern, fakeValue } from "./example-fakes";
import { kindOf, nounOf, phraseOf, wordsOf, type Kind } from "./example-names";

/** Which way the example goes: into a request, or out of an answer. */
export type Direction = "request" | "response" | "any";

/** What the generator reads of the spec: a ref resolved, and a schema's merged shape. */
export interface Reader {
  /** The node `ref` names from file `base`, and its file; `null` where it cannot be followed here. */
  resolve(base: string, ref: string): { node: unknown; base: string } | null;
  shape(base: string, node: unknown): Shape;
}

export interface ExampleOptions {
  direction: Direction;
  /** Which choice of the top schema's `oneOf` or `anyOf` to take; the first where left out. */
  choice?: number;
  /**
   * Whether the spec's own values (`example`, `examples`, `default`) are taken where a schema gives them; `true` where
   * left out. `false` makes every value, as the tests that hold knobas's own values to their schemas need: a spec's
   * example may not fit its schema (GitHub's `display_name`, typed a string, has the example `20.04`).
   */
  given?: boolean;
  /** The name the value stands under where the schema is not a property's: a parameter's (`org`, `repo`). */
  name?: string | undefined;
}

/** How deep an example goes before it stops. */
export const DEPTH = 6;
/** How many properties an object's example holds at its top, and below it. */
export const TOP_PROPS = 14;
export const DEEP_PROPS = 6;

type Obj = Record<string, unknown>;
const isObj = (v: unknown): v is Obj => typeof v === "object" && v !== null && !Array.isArray(v);
const str = (v: unknown): string => (typeof v === "string" ? v : "");

/** The usual shape of a string of each format. */
export const FORMATS: Readonly<Record<string, string>> = {
  "date-time": "2026-11-22T14:05:00Z",
  date: "2026-11-22",
  time: "14:05:00",
  duration: "PT1H30M",
  uuid: "3fa85f64-5717-4562-b3fc-2c963f66afa6",
  email: "mara@example.com",
  "idn-email": "mara@example.com",
  uri: "https://example.com/",
  url: "https://example.com/",
  "uri-reference": "/example",
  iri: "https://example.com/",
  hostname: "api.example.com",
  "idn-hostname": "api.example.com",
  ipv4: "192.0.2.10",
  ipv6: "2001:db8::10",
  byte: "ZXhhbXBsZQ==",
  binary: "",
  password: "example-password",
  regex: "^[a-z]+$",
  "json-pointer": "/example",
};

/**
 * A string `pattern` matches, for the simple patterns specs write: anchors, literals, `\d` `\w`, character classes
 * of ranges and characters, and the counts `{n}`, `{n,m}`, `+`, `*` and `?`, the fewest each allows (one for `+`).
 * `null` for anything else: groups, alternation, lookarounds.
 */
export function fromPattern(pattern: string): string | null {
  let src = pattern;
  if (src.startsWith("^")) src = src.slice(1);
  if (src.endsWith("$") && !src.endsWith("\\$")) src = src.slice(0, -1);
  let out = "";
  let i = 0;
  while (i < src.length) {
    let unit: string;
    const c = src[i]!;
    if (c === "(" || c === ")" || c === "|" || c === ".") return null;
    if (c === "\\") {
      const e = src[i + 1];
      if (e === undefined) return null;
      unit = e === "d" ? "1" : e === "w" ? "a" : e === "s" ? " " : /[A-Za-z]/.test(e) ? "" : e;
      if (unit === "") return null;
      i += 2;
    } else if (c === "[") {
      const close = src.indexOf("]", i + 1);
      if (close < 0) return null;
      const body = src.slice(i + 1, close);
      if (body.startsWith("^") || body === "") return null;
      // The class's first member: a range's start, an escape's character, else the character.
      unit = body[0] === "\\" ? (body[1] === "d" ? "1" : body[1] === "w" ? "a" : (body[1] ?? "")) : body[0]!;
      if (unit === "") return null;
      i = close + 1;
    } else if ("{}+*?".includes(c)) return null;
    else {
      unit = c;
      i += 1;
    }
    let times = 1;
    const q = src[i];
    if (q === "{") {
      const close = src.indexOf("}", i);
      const m = close < 0 ? null : /^(\d+)(?:,(\d*))?$/.exec(src.slice(i + 1, close));
      if (m === null) return null;
      times = Number(m[1]);
      i = close + 1;
    } else if (q === "+") {
      i += 1;
    } else if (q === "*" || q === "?") {
      times = 0;
      i += 1;
    }
    if (times > 64) return null;
    out += unit.repeat(times);
  }
  return new RegExp(pattern).test(out) ? out : null;
}

/** Who a bare `name` belongs to where its holder is one of these: a person's name. */
const PERSON = new Set(["user", "person", "people", "contact", "customer", "member", "employee", "account", "profile", "author", "patient", "student", "guest", "attendee", "recipient", "applicant", "kunde", "mitarbeiter", "benutzer", "teilnehmer", "owner", "assignee", "reviewer", "committer", "actor", "sender"]);

/** What `name` asks for under `parent`: a bare `name` reads its holder, the rest the table (`example-names.ts`). */
function kindFor(name: string, parent: string): Kind | null {
  const kind = kindOf(name);
  if (kind !== "fullName" || wordsOf(name).join("") !== "name") return kind;
  const holder = nounOf(parent).split(" ").at(-1) ?? "";
  if (PERSON.has(holder)) return "fullName";
  const of = kindOf(holder);
  if (of === "fullName" || of === "username") return "fullName";
  if (of === "org" || of === "repo" || of === "branch" || of === "tag" || of === "environment" || of === "company") return of;
  return "slug";
}

/** The value kind `kind` gives for `name` under `parent` at `path`: a free-text field a phrase, the rest made. */
function valueFor(kind: Kind, name: string, parent: string, path: string): string | number {
  if (kind === "title" || kind === "text") return phraseOf(name, parent);
  return fakeValue(kind, path);
}

/** A string example for schema `n` of property `name` at `path`: format, then pattern, then name, then type. */
function stringFor(n: Obj, name: string, path: string, parent: string): string {
  const format = FORMATS[str(n.format)];
  const pattern = format === undefined && typeof n.pattern === "string" ? (fakePattern(n.pattern, path) ?? safePattern(n.pattern)) : null;
  const kind = kindFor(name, parent);
  let s = format ?? pattern ?? (kind === null ? null : String(valueFor(kind, name, parent, path))) ?? "example";
  const min = typeof n.minLength === "number" ? n.minLength : 0;
  const max = typeof n.maxLength === "number" ? n.maxLength : Infinity;
  if (pattern === null) {
    if (s.length < min) s = s.padEnd(min, "x");
    if (s.length > max) s = s.slice(0, Math.max(0, max));
  }
  return s;
}

function safePattern(pattern: string): string | null {
  try {
    return fromPattern(pattern);
  } catch {
    return null;
  }
}

/** The number a name asks for, where its kind is a number. */
function numberFor(kind: Kind | null, path: string, integer: boolean): number | null {
  if (kind === null) return null;
  // A numeric id: a number of its own at its path.
  if (kind === "id") return fakeInt(path, 1, 9999);
  const v = fakeValue(kind, path);
  if (typeof v !== "number") return null;
  return integer ? Math.round(v) : v;
}

/** A number example for schema `n` of property `name`: kept to its bounds and its `multipleOf`. */
function numberOf(n: Obj, name: string, integer: boolean, path: string): number {
  const lo = typeof n.minimum === "number" ? n.minimum : typeof n.exclusiveMinimum === "number" ? n.exclusiveMinimum + (integer ? 1 : 0.5) : null;
  const hi = typeof n.maximum === "number" ? n.maximum : typeof n.exclusiveMaximum === "number" ? n.exclusiveMaximum - (integer ? 1 : 0.5) : null;
  let v = numberFor(kindFor(name, ""), path, integer) ?? (integer ? 1 : 1.5);
  if (lo !== null && v < lo) v = lo;
  if (hi !== null && v > hi) v = hi;
  const step = typeof n.multipleOf === "number" && n.multipleOf > 0 ? n.multipleOf : null;
  if (step !== null) {
    v = Math.ceil(v / step) * step;
    if (hi !== null && v > hi) v = Math.floor(hi / step) * step;
  }
  return integer ? Math.round(v) : v;
}

/** The type a choice names, its ref followed. */
function choiceType(read: Reader, base: string, m: unknown): string | undefined {
  const node = isObj(m) && typeof m.$ref === "string" ? read.resolve(base, m.$ref)?.node : m;
  return isObj(node) ? typeOf(node) : undefined;
}

/** The first type a schema names other than `null`. */
function typeOf(n: Obj): string | undefined {
  if (Array.isArray(n.type)) return n.type.find((t): t is string => typeof t === "string" && t !== "null");
  return typeof n.type === "string" ? n.type : undefined;
}

/** Whether a schema says only null (or only the empty string): what `oneOf: [X, null]` adds to X. */
function nullish(n: unknown): boolean {
  return isObj(n) && (n.type === "null" || (Array.isArray(n.enum) && n.enum.length === 1 && (n.enum[0] === null || n.enum[0] === "")));
}

/** An example of schema `node` of file `base` (see the module's words). */
export function makeExample(read: Reader, base: string, node: unknown, options: ExampleOptions): unknown {
  const ref = isObj(node) && typeof node.$ref === "string" ? node.$ref : "";
  // A parameter's name seeds where it stands too, so `org` and `repo` beside each other differ.
  const path = options.name === undefined || options.name === "" ? ref : `${ref}/${options.name}`;
  return walk(read, base, node, options.name ?? "", path, 0, new Set(), options, true, "");
}

/** A ref's last part, decoded: `#/components/schemas/full-repository` is `full-repository`. */
function refName(ref: string): string {
  const last = ref.slice(ref.lastIndexOf("/") + 1);
  try {
    return decodeURIComponent(last).replace(/~1/g, "/").replace(/~0/g, "~");
  } catch {
    return last;
  }
}

/**
 * The example of `node`, named `name`, at schema path `path` (what seeds its values). `parent` names the thing that
 * holds it: its object's schema, or the property that holds the object.
 */
function walk(read: Reader, base: string, node: unknown, name: string, path: string, depth: number, seen: ReadonlySet<string>, options: ExampleOptions, top: boolean, parent: string): unknown {
  let n = node;
  let at = base;
  let trail = seen;
  // What this value is, for what it holds: its schema's name where it is a ref, else its own name, else its holder's.
  let noun = name !== "" ? name : parent;
  if (isObj(n) && typeof n.$ref === "string") {
    const ref = n.$ref;
    noun = refName(ref) || noun;
    if (seen.has(`${at}#${ref}`) || depth > DEPTH) return null;
    trail = new Set(seen).add(`${at}#${ref}`);
    const r = read.resolve(at, ref);
    if (r === null) return null;
    n = r.node;
    at = r.base;
  }
  if (n === true) return "example";
  // A password, whatever else its schema says, is a placeholder: never a real-looking secret.
  if (name !== "" && kindOf(name) === "password") return fakeValue("password", path);
  if (!isObj(n)) return null;
  const given = options.given !== false;
  if (given && n.example !== undefined) return n.example;
  if (given && Array.isArray(n.examples) && n.examples.length > 0) return n.examples[0];
  if (given && n.default !== undefined) return n.default;
  if (n.const !== undefined) return n.const;
  if (Array.isArray(n.enum) && n.enum.length > 0) return n.enum.find((v) => v !== null) ?? n.enum[0];
  if (depth > DEPTH) return null;
  const sh = read.shape(at, n);
  // What the merged parts name (an allOf's own enum or const) counts as the node's own.
  const f = sh.facts;
  if (given && f.example !== undefined) return f.example;
  if (given && f.default !== undefined) return f.default;
  if (f.const !== undefined) return f.const;
  if (Array.isArray(f.enum) && f.enum.length > 0) return f.enum.find((v) => v !== null) ?? f.enum[0];
  // A choice: the one asked for at the top, the first below it.
  if (sh.variants !== null) {
    const asked = top && options.choice !== undefined && options.choice > 0;
    // Unasked, the first choice whose type the rest of the schema allows (an allOf part may narrow it).
    const fitting = typeof sh.facts.type === "string" ? sh.variants.list.findIndex((m) => choiceType(read, sh.base, m) === sh.facts.type) : -1;
    const pick = asked ? Math.min(options.choice!, sh.variants.list.length - 1) : Math.max(0, fitting);
    const chosen = walk(read, sh.base, sh.variants.list[pick], name, `${path}|${pick}`, depth + 1, trail, options, false, noun);
    // A choice beside properties of its own (a base with a oneOf): both, merged.
    if (sh.props.length === 0 || !isObj(chosen)) return chosen;
    return { ...objectOf(read, sh, path, depth, trail, options, noun), ...chosen };
  }
  for (const key of ["oneOf", "anyOf"] as const) {
    const list = n[key];
    if (Array.isArray(list) && sh.props.length === 0) {
      const real = list.find((m) => !nullish(isObj(m) && typeof m.$ref === "string" ? read.resolve(at, m.$ref)?.node : m));
      if (real !== undefined) return walk(read, at, real, name, path, depth + 1, trail, options, false, parent);
    }
  }
  const type = typeOf(n) ?? (typeof sh.facts.type === "string" ? sh.facts.type : undefined);
  if (sh.props.length > 0 || type === "object" || (type === undefined && Array.isArray(n.allOf))) {
    const out = objectOf(read, sh, path, depth, trail, options, noun);
    if (Object.keys(out).length === 0 && sh.extra !== undefined && sh.extra !== false) {
      return { key: walk(read, sh.base, sh.extra, "key", `${path}/{}`, depth + 1, trail, options, false, noun) };
    }
    return out;
  }
  if (type === "array" || sh.items !== undefined) {
    if (depth >= DEPTH) return [];
    // An item is named for its list (`labels`, a `label`); a list of `tags` holds labels, not a repository's tags.
    const single = name.replace(/s$/, "");
    const item = walk(read, sh.base, sh.items, kindOf(single) === "tag" && single === "tag" ? "keyword" : single, `${path}/[]`, depth + 1, trail, options, false, parent);
    const count = Math.min(3, Math.max(1, typeof n.minItems === "number" ? n.minItems : 1));
    return Array.from({ length: count }, () => item);
  }
  // The merged facts stand for a ref's or an allOf's own type and bounds.
  const facts = { ...sh.facts, ...n };
  if (type === "integer") return numberOf(facts, name, true, path);
  if (type === "number") return numberOf(facts, name, false, path);
  if (type === "boolean") return true;
  if (type === "string" || typeof facts.format === "string" || typeof facts.pattern === "string") return stringFor(facts, name, path, parent);
  if (type === "null") return null;
  const kind = kindFor(name, parent);
  return kind === null ? null : valueFor(kind, name, parent, path);
}

/** An object's example: its required properties and the first few others, as its direction allows. */
function objectOf(read: Reader, sh: Shape, path: string, depth: number, seen: ReadonlySet<string>, options: ExampleOptions, noun: string): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  // Every required property, so the example is valid; then the first few others, fewer deeper down.
  const cap = depth === 0 ? TOP_PROPS : DEEP_PROPS;
  let taken = 0;
  for (const p of sh.props) {
    if (!p.required && taken >= cap) continue;
    const own = isObj(p.node) ? p.node : {};
    const resolved = isObj(p.node) && typeof p.node.$ref === "string" ? read.resolve(sh.base, p.node.$ref)?.node : p.node;
    const flags = { ...(isObj(resolved) ? resolved : {}), ...own };
    if (options.direction === "request" && flags.readOnly === true) continue;
    if (options.direction === "response" && flags.writeOnly === true) continue;
    if (flags.deprecated === true && !p.required) continue;
    if (!p.required && depth >= 2) continue;
    out[p.name] = walk(read, sh.base, p.node, p.name, `${path}/${p.name}`, depth + 1, seen, options, false, noun);
    if (!p.required) taken += 1;
  }
  return out;
}
