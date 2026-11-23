/**
 * XPath 1.0, read and answered by knobas over the outline (`xml-outline.ts`) the XML view already holds (its design,
 * 2026-11-23, sections 20 and 21). It runs in the document worker; libxml2 is the witness in tests
 * (`xml-xpath-witness.test.ts`).
 *
 * Why not libxml2's own XPath, which the schema's check loads: it needs the document a second time, as its own tree
 * (187 MB for 50 MB), it gives a node's line as 16 bits, it cannot be stopped once it runs, and its one error is
 * *Invalid expression*. This one walks the columns the view holds, counts its looks against a budget, and says where an
 * expression breaks, in words.
 *
 * - All of XPath 1.0 but two things nobody types: the namespace axis and `id()`, which needs a DTD. A variable has no
 *   value here, and `$x` says so.
 * - **A name matches as the document writes it** (Björn's answer 4): `ORDER_ITEM` finds `<ORDER_ITEM>` whatever default
 *   namespace it stands under, and `bmecat:PRICE` finds every element of the namespace the document binds `bmecat` to.
 *   `strict: true` is XPath's own rule, which the witness runs. The rule is {@link NAME_AS_WRITTEN}.
 * - Numbers are printed as xmllint prints them: fifteen digits, never an exponent.
 */
import { XML_NS } from "./xml-names";
import { CDATA, COMMENT, DOC, EL, PI, TEXT, nameOf, stringOf, type XmlOutline } from "./xml-outline";

/**
 * Björn's answer 4 (2026-11-23): a name in a query with no prefix matches the element as the document writes it,
 * not XPath 1.0's "in no namespace". The one place the rule is chosen: `false` here makes the field XPath to the letter.
 */
export const NAME_AS_WRITTEN = true;

/** Where an expression breaks, in words, and the character it breaks at (from 0). */
export class XPathError extends Error {
  readonly at: number;
  /** The query looked at more of the document than one query may. */
  readonly budget: boolean;
  constructor(message: string, at: number, budget = false) {
    super(message);
    this.at = at;
    this.budget = budget;
  }
}

type Tok =
  | { t: "lit"; v: string; at: number }
  | { t: "num"; v: number; at: number }
  | { t: "op"; v: string; at: number }
  | { t: "name"; prefix: string | null; local: string; at: number }
  | { t: "fn" | "type" | "axis"; v: string; at: number }
  | { t: "//" | "::" | ".." | "(" | ")" | "[" | "]" | "@" | "," | "." | "/"; at: number };

type Test =
  | { t: "node" | "text" | "comment" | "any" }
  | { t: "pi"; name: string | null }
  | { t: "nsany"; prefix: string; at: number }
  | { t: "name"; prefix: string | null; local: string; at: number };

interface Step {
  axis: string;
  test: Test;
  preds: Expr[];
}

type Expr =
  | { k: "num"; v: number }
  | { k: "lit"; v: string }
  | { k: "or" | "and"; a: Expr; b: Expr }
  | { k: "cmp" | "arith"; op: string; a: Expr; b: Expr }
  | { k: "neg"; a: Expr }
  | { k: "union"; a: Expr; b: Expr }
  | { k: "filter"; a: Expr; preds: Expr[] }
  | { k: "path"; abs: boolean; base: Expr | null; steps: Step[] }
  | { k: "fn"; name: string; args: Expr[]; at: number };

/** A read expression. */
export interface Compiled {
  readonly src: string;
  readonly ast: Expr;
}

const NAME = /[A-Za-z_À-￿][\w.\-·À-￿]*/y;
const NUM = /(?:\d+(?:\.\d*)?|\.\d+)/y;
const AXES = new Set(["ancestor", "ancestor-or-self", "attribute", "child", "descendant", "descendant-or-self", "following", "following-sibling", "namespace", "parent", "preceding", "preceding-sibling", "self"]);
const NODE_TYPES = new Set(["comment", "text", "processing-instruction", "node"]);

function lex(src: string): Tok[] {
  const out: Tok[] = [];
  let i = 0;
  const n = src.length;
  // XPath 1.0, 3.7: after anything but `@`, `::`, `(`, `[`, `,`, an operator or a `/`, `*` multiplies and a name is an operator.
  const prevIsOperand = () => {
    const p = out[out.length - 1];
    return p !== undefined && !(p.t === "@" || p.t === "::" || p.t === "(" || p.t === "[" || p.t === "," || p.t === "op" || p.t === "/" || p.t === "//");
  };
  while (i < n) {
    const ch = src[i]!;
    if (ch === " " || ch === "\n" || ch === "\t" || ch === "\r") {
      i += 1;
      continue;
    }
    const at = i;
    if (ch === '"' || ch === "'") {
      const e = src.indexOf(ch, i + 1);
      if (e === -1) throw new XPathError(`The text that starts at character ${i + 1} is not closed: a ${ch} is missing.`, i);
      out.push({ t: "lit", v: src.slice(i + 1, e), at });
      i = e + 1;
      continue;
    }
    NUM.lastIndex = i;
    const num = NUM.exec(src);
    if (num !== null && !(ch === "." && (src[i + 1] === "." || !/\d/.test(src[i + 1] ?? "")))) {
      out.push({ t: "num", v: Number(num[0]), at });
      i += num[0].length;
      continue;
    }
    const two = src.slice(i, i + 2);
    if (two === "//" || two === "::" || two === ".." || two === "!=" || two === "<=" || two === ">=") {
      out.push(two === "::" || two === ".." || two === "//" ? { t: two, at } : { t: "op", v: two, at });
      i += 2;
      continue;
    }
    if (ch === "(" || ch === ")" || ch === "[" || ch === "]" || ch === "@" || ch === "," || ch === "." || ch === "/") {
      out.push({ t: ch, at });
      i += 1;
      continue;
    }
    if (ch === "|" || ch === "+" || ch === "-" || ch === "=" || ch === "<" || ch === ">") {
      out.push({ t: "op", v: ch, at });
      i += 1;
      continue;
    }
    if (ch === "*") {
      out.push(prevIsOperand() ? { t: "op", v: "*", at } : { t: "name", prefix: null, local: "*", at });
      i += 1;
      continue;
    }
    if (ch === "$") {
      NAME.lastIndex = i + 1;
      const m = NAME.exec(src);
      throw new XPathError(`knobas has no value for ${m !== null ? "$" + m[0] : "a variable"}: a query stands by itself.`, i);
    }
    NAME.lastIndex = i;
    const m = NAME.exec(src);
    if (m === null) throw new XPathError(`${JSON.stringify(ch)} at character ${i + 1} is not part of XPath.`, i);
    let name = m[0];
    i += name.length;
    if (prevIsOperand() && (name === "and" || name === "or" || name === "mod" || name === "div")) {
      out.push({ t: "op", v: name, at });
      continue;
    }
    let prefix: string | null = null;
    if (src[i] === ":" && src[i + 1] !== ":") {
      if (src[i + 1] === "*") {
        out.push({ t: "name", prefix: name, local: "*", at });
        i += 2;
        continue;
      }
      NAME.lastIndex = i + 1;
      const l = NAME.exec(src);
      if (l === null) throw new XPathError(`A name is missing after ${name}: at character ${i + 2}.`, i);
      prefix = name;
      name = l[0];
      i += 1 + name.length;
    }
    let j = i;
    while (src[j] === " " || src[j] === "\n" || src[j] === "\t") j += 1;
    if (src[j] === "(" && prefix === null) out.push({ t: NODE_TYPES.has(name) ? "type" : "fn", v: name, at });
    else if (src[j] === ":" && src[j + 1] === ":" && prefix === null) {
      if (!AXES.has(name)) throw new XPathError(`${name} is no axis. The axes are child, descendant, parent, ancestor, following-sibling, preceding-sibling, following, preceding, attribute and self.`, at);
      out.push({ t: "axis", v: name, at });
    } else out.push({ t: "name", prefix, local: name, at });
  }
  return out;
}

const ARITY: Record<string, [number, number]> = {
  last: [0, 0],
  position: [0, 0],
  count: [1, 1],
  "local-name": [0, 1],
  "namespace-uri": [0, 1],
  name: [0, 1],
  string: [0, 1],
  concat: [2, 99],
  "starts-with": [2, 2],
  contains: [2, 2],
  "substring-before": [2, 2],
  "substring-after": [2, 2],
  substring: [2, 3],
  "string-length": [0, 1],
  "normalize-space": [0, 1],
  translate: [3, 3],
  boolean: [1, 1],
  not: [1, 1],
  true: [0, 0],
  false: [0, 0],
  lang: [1, 1],
  number: [0, 1],
  sum: [1, 1],
  floor: [1, 1],
  ceiling: [1, 1],
  round: [1, 1],
};
/** What does it in XPath 1.0, for a function that came later. */
const LATER: Record<string, string> = {
  "lower-case": "translate(., 'ABC…', 'abc…')",
  "upper-case": "translate(., 'abc…', 'ABC…')",
  "ends-with": "substring(., string-length(.) - n)",
  matches: "contains() or starts-with()",
  "string-join": "concat()",
  replace: "translate() or substring-before() and substring-after()",
  "format-number": "round()",
  exists: "boolean()",
  empty: "not()",
};

function checkFn(f: { name: string; args: Expr[]; at: number }): void {
  const a = ARITY[f.name];
  if (a === undefined) {
    if (f.name === "id") throw new XPathError("id() needs a DTD, which knobas does not read.", f.at);
    const later = LATER[f.name];
    throw new XPathError(`XPath 1.0 has no ${f.name}().${later !== undefined ? ` It came with XPath 2.0; here, ${later} does it.` : ""}`, f.at);
  }
  if (f.args.length < a[0] || f.args.length > a[1]) {
    const takes = a[0] === a[1] ? `${a[0]}` : `${a[0]} to ${a[1] === 99 ? "any number of" : a[1]}`;
    throw new XPathError(`${f.name}() takes ${takes} ${a[0] === 1 && a[1] === 1 ? "value" : "values"}; it was given ${f.args.length}.`, f.at);
  }
}

/** Reads `src`. Throws an {@link XPathError}: what is wrong, in words, and where. */
export function compile(src: string): Compiled {
  const toks = lex(src);
  let p = 0;
  const peek = (): Tok | undefined => toks[p];
  const is = (t: Tok["t"], v?: string): boolean => p < toks.length && toks[p]!.t === t && (v === undefined || (toks[p] as { v?: unknown }).v === v);
  const take = (t: Tok["t"], v?: string): Tok | null => (is(t, v) ? toks[p++]! : null);
  const endAt = src.length;
  const need = (t: Tok["t"], what: string, openedAt?: number): Tok => {
    const k = take(t);
    if (k !== null) return k;
    const here = peek();
    throw new XPathError(
      openedAt !== undefined ? `${what} is missing: what opens at character ${openedAt + 1} is not closed.` : `${what} is missing${here !== undefined ? ` before character ${here.at + 1}` : " at the end"}.`,
      here !== undefined ? here.at : endAt,
    );
  };
  const binary =
    (next: () => Expr, ops: string[], make: (op: string, a: Expr, b: Expr) => Expr) =>
    (): Expr => {
      let a = next();
      for (;;) {
        const o = peek();
        if (o === undefined || o.t !== "op" || !ops.includes(o.v)) return a;
        p += 1;
        if (p >= toks.length) throw new XPathError(`Something is missing after ${o.v}.`, endAt);
        a = make(o.v, a, next());
      }
    };
  function union(): Expr {
    let a = path();
    while (take("op", "|") !== null) a = { k: "union", a, b: path() };
    return a;
  }
  function unary(): Expr {
    let neg = false;
    while (take("op", "-") !== null) neg = !neg;
    const a = union();
    return neg ? { k: "neg", a } : a;
  }
  const mul = binary(unary, ["*", "div", "mod"], (op, a, b) => ({ k: "arith", op, a, b }));
  const addx = binary(mul, ["+", "-"], (op, a, b) => ({ k: "arith", op, a, b }));
  const rel = binary(addx, ["<", "<=", ">", ">="], (op, a, b) => ({ k: "cmp", op, a, b }));
  const eq = binary(rel, ["=", "!="], (op, a, b) => ({ k: "cmp", op, a, b }));
  const and = binary(eq, ["and"], (_, a, b) => ({ k: "and", a, b }));
  const or = binary(and, ["or"], (_, a, b) => ({ k: "or", a, b }));
  function preds(): Expr[] {
    const out: Expr[] = [];
    for (let open = take("["); open !== null; open = take("[")) {
      if (is("]")) throw new XPathError(`The test that opens at character ${open.at + 1} is empty.`, open.at);
      out.push(or());
      need("]", "A ]", open.at);
    }
    return out;
  }
  const startsStep = () => is("name") || is("axis") || is("type") || is("@") || is(".") || is("..");
  function step(): Step {
    if (take(".") !== null) return { axis: "self", test: { t: "node" }, preds: [] };
    if (take("..") !== null) return { axis: "parent", test: { t: "node" }, preds: [] };
    let axis = "child";
    const ax = take("axis") as { v: string; at: number } | null;
    if (ax !== null) {
      axis = ax.v;
      need("::", "::");
    } else if (take("@") !== null) axis = "attribute";
    if (axis === "namespace") throw new XPathError("knobas does not read the namespace axis.", ax?.at ?? 0);
    let test: Test;
    const ty = take("type") as { v: string; at: number } | null;
    if (ty !== null) {
      need("(", "A (");
      let name: string | null = null;
      if (ty.v === "processing-instruction" && is("lit")) name = (take("lit") as { v: string }).v;
      need(")", "A )", ty.at);
      test = ty.v === "node" ? { t: "node" } : ty.v === "text" ? { t: "text" } : ty.v === "comment" ? { t: "comment" } : { t: "pi", name };
    } else {
      const nm = take("name") as { prefix: string | null; local: string; at: number } | null;
      if (nm === null) {
        const here = peek();
        throw new XPathError(`A name is missing${here !== undefined ? ` at character ${here.at + 1}` : " at the end"}.`, here !== undefined ? here.at : endAt);
      }
      test = nm.local === "*" ? (nm.prefix === null ? { t: "any" } : { t: "nsany", prefix: nm.prefix, at: nm.at }) : { t: "name", prefix: nm.prefix, local: nm.local, at: nm.at };
    }
    return { axis, test, preds: preds() };
  }
  const DOS = (): Step => ({ axis: "descendant-or-self", test: { t: "node" }, preds: [] });
  function steps(out: Step[]): Step[] {
    out.push(step());
    for (;;) {
      if (take("/") !== null) out.push(step());
      else if (take("//") !== null) {
        out.push(DOS());
        out.push(step());
      } else return out;
    }
  }
  function path(): Expr {
    if (is("/")) {
      p += 1;
      return { k: "path", abs: true, base: null, steps: startsStep() ? steps([]) : [] };
    }
    if (is("//")) {
      p += 1;
      return { k: "path", abs: true, base: null, steps: steps([DOS()]) };
    }
    if (startsStep()) return { k: "path", abs: false, base: null, steps: steps([]) };
    // A filter expression, perhaps with a path after it.
    let prim: Expr;
    const open = take("(");
    if (open !== null) {
      prim = or();
      need(")", "A )", open.at);
    } else if (is("lit")) prim = { k: "lit", v: (take("lit") as { v: string }).v };
    else if (is("num")) prim = { k: "num", v: (take("num") as { v: number }).v };
    else if (is("fn")) {
      const f = take("fn") as { v: string; at: number };
      need("(", "A (");
      const args: Expr[] = [];
      if (!is(")"))
        do args.push(or());
        while (take(",") !== null);
      need(")", "A )", f.at);
      const call = { k: "fn" as const, name: f.v, args, at: f.at };
      checkFn(call);
      prim = call;
    } else {
      const here = peek();
      throw new XPathError(
        here !== undefined ? `${JSON.stringify(src.slice(here.at, here.at + 12))} cannot stand at character ${here.at + 1}.` : "The expression stops where something should follow.",
        here !== undefined ? here.at : endAt,
      );
    }
    const ps = preds();
    if (ps.length > 0) prim = { k: "filter", a: prim, preds: ps };
    if (is("/") || is("//")) {
      const out: Step[] = [];
      if (take("//") !== null) out.push(DOS());
      else p += 1;
      return { k: "path", abs: false, base: prim, steps: steps(out) };
    }
    return prim;
  }
  if (toks.length === 0) throw new XPathError("Nothing to ask yet.", 0);
  const ast = or();
  if (p < toks.length) throw new XPathError(`${JSON.stringify(src.slice(toks[p]!.at, toks[p]!.at + 12))} cannot stand at character ${toks[p]!.at + 1}.`, toks[p]!.at);
  return { src, ast };
}

/** Every prefix the document binds, the first binding of each, and those bound to two namespaces. */
export function prefixesOf(o: XmlOutline): { map: Map<string, string>; twice: Set<string> } {
  const map = new Map<string, string>();
  const twice = new Set<string>();
  for (const scope of o.scopes) {
    // A scope inherits its parent's bindings: only its own keys and the root scope's are read.
    for (const p of Object.keys(scope)) {
      if (p === "") continue;
      const uri = scope[p]!;
      if (!map.has(p)) map.set(p, uri);
      else if (map.get(p) !== uri) twice.add(p);
    }
  }
  return { map, twice };
}

/** A number as XPath prints it, with fifteen digits as xmllint does: `487382.14`, never an exponent. */
export function numStr(x: number): string {
  if (Number.isNaN(x)) return "NaN";
  if (x === Infinity) return "Infinity";
  if (x === -Infinity) return "-Infinity";
  if (x === 0) return "0";
  const r = Number(x.toPrecision(15));
  let s = String(r);
  if (s.includes("e")) s = r.toLocaleString("en-US", { useGrouping: false, maximumFractionDigits: 20 });
  return s;
}

function toNum(s: string): number {
  const t = s.trim();
  return /^-?(?:\d+(?:\.\d*)?|\.\d+)$/.test(t) ? Number(t) : NaN;
}

/** What a query answers: nodes (rows, in document order), or a value. */
export type Answer =
  | { type: "nodes"; ids: number[]; looked: number }
  | { type: "number"; value: number; text: string; looked: number }
  | { type: "string"; value: string; text: string; looked: number }
  | { type: "boolean"; value: boolean; text: string; looked: number };

export interface RunOptions {
  /** How many nodes it may look at before it stops (2,000,000 by default). */
  budget?: number;
  /** XPath 1.0's own rule for a name with no prefix: in no namespace. */
  strict?: boolean;
  /** The prefixes a query reads with: the document's own where not given. */
  prefixes?: Map<string, string>;
  /** The context node: the document where not given. */
  context?: number;
  /** When it must stop, in `performance.now()`'s milliseconds. */
  deadline?: number;
}

type Value = number[] | string | number | boolean;

/** Answers `compiled` on outline `o`. Throws an {@link XPathError} where it breaks or stops. */
export function run(o: XmlOutline, compiled: Compiled, options: RunOptions = {}): Answer {
  const strict = options.strict ?? !NAME_AS_WRITTEN;
  const prefixes = options.prefixes ?? prefixesOf(o).map;
  const budget = options.budget ?? 2e6;
  let left = budget;
  const deadline = options.deadline ?? Infinity;
  const N = o.n;
  let ticks = 0;
  const spend = (k: number) => {
    left -= k;
    if (left < 0) throw new XPathError("Stopped: the query looked at more of the document than knobas lets one query look at.", 0, true);
    if (deadline !== Infinity && (ticks += 1) % 4096 === 0 && performance.now() > deadline) throw new XPathError("Stopped: the query took longer than knobas lets one query take.", 0, true);
  };
  const nameIds = new Map<string, number>();
  const idOf = (s: string) => {
    let i = nameIds.get(s);
    if (i === undefined) {
      i = o.names.indexOf(s);
      nameIds.set(s, i);
    }
    return i;
  };
  const uriId = (prefix: string, at: number) => {
    if (prefix === "xml") return o.uris.indexOf(XML_NS);
    const uri = prefixes.get(prefix);
    if (uri === undefined) throw new XPathError(`The document binds no prefix ${prefix}:. It binds ${[...prefixes.keys()].map((x) => x + ":").join(", ") || "none"}.`, at);
    return o.uris.indexOf(uri);
  };
  const nsDecl = new Map<number, boolean>();
  const isNsDecl = (a: number) => {
    const id = o.aName[a]!;
    let v = nsDecl.get(id);
    if (v === undefined) {
      const s = o.names[id]!;
      v = s === "xmlns" || s.startsWith("xmlns:");
      nsDecl.set(id, v);
    }
    return v;
  };
  const key = (id: number) => {
    if (id < N) return id;
    const owner = o.aOwner[id - N]!;
    return owner + (id - N - o.a0[owner]! + 1) / (o.an[owner]! + 1);
  };
  const str = (id: number) => stringOf(o, id);

  /** A test as a function of a row, for one axis. */
  function tester(test: Test, axis: string): (id: number) => boolean {
    const onAttr = axis === "attribute";
    if (test.t === "node") return () => true;
    if (test.t === "text") return (id) => id < N && (o.type[id] === TEXT || o.type[id] === CDATA);
    if (test.t === "comment") return (id) => id < N && o.type[id] === COMMENT;
    if (test.t === "pi") {
      const want = test.name === null ? -2 : idOf(test.name);
      return (id) => id < N && o.type[id] === PI && (want === -2 || o.name[id] === want);
    }
    if (test.t === "any") return onAttr ? (id) => id >= N : (id) => id < N && o.type[id] === EL;
    if (test.t === "nsany") {
      const u = uriId(test.prefix, test.at);
      return onAttr ? (id) => id >= N && o.aNs[id - N] === u : (id) => id < N && o.type[id] === EL && o.ns[id] === u;
    }
    const named = test as Extract<Test, { t: "name" }>;
    const local = idOf(named.local);
    if (named.prefix !== null) {
      const u = uriId(named.prefix, named.at);
      return onAttr ? (id) => id >= N && o.aLocal[id - N] === local && o.aNs[id - N] === u : (id) => id < N && o.type[id] === EL && o.local[id] === local && o.ns[id] === u;
    }
    if (onAttr) return (id) => id >= N && o.aName[id - N] === local;
    // No prefix: as the document writes it, or, strictly, in no namespace.
    return strict ? (id) => id < N && o.type[id] === EL && o.local[id] === local && o.ns[id] === 0 && o.name[id] === local : (id) => id < N && o.type[id] === EL && o.name[id] === local;
  }

  /** The rows on `axis` from `id`, in the axis's own order (nearest first on a reverse axis), that pass `test`. */
  function along(axis: string, id: number, test: (id: number) => boolean, out: number[]): number[] {
    const attr = id >= N;
    const el = attr ? o.aOwner[id - N]! : id;
    switch (axis) {
      case "child":
        if (!attr)
          for (let c = o.first[id]!; c !== -1; c = o.next[c]!) {
            spend(1);
            if (test(c)) out.push(c);
          }
        break;
      case "attribute":
        if (!attr && o.type[id] === EL)
          for (let a = o.a0[id]!, e = a + o.an[id]!; a < e; a += 1) {
            spend(1);
            if (!isNsDecl(a) && test(N + a)) out.push(N + a);
          }
        break;
      case "self":
        if (test(id)) out.push(id);
        break;
      case "parent":
        if (attr) {
          if (test(el)) out.push(el);
        } else if (o.parent[id]! >= 0 && test(o.parent[id]!)) out.push(o.parent[id]!);
        break;
      case "descendant-or-self":
      case "descendant":
        if (axis === "descendant-or-self" && test(id)) out.push(id);
        if (!attr) {
          const last = o.sub[id]!;
          spend(last - id);
          for (let k = id + 1; k <= last; k += 1) if (test(k)) out.push(k);
        }
        break;
      case "ancestor-or-self":
      case "ancestor":
        if (axis === "ancestor-or-self" && test(id)) out.push(id);
        for (let a = attr ? el : o.parent[id]!; a >= 0; a = o.parent[a]!) {
          spend(1);
          if (test(a)) out.push(a);
        }
        break;
      case "following-sibling":
        if (!attr)
          for (let c = o.next[id]!; c !== -1; c = o.next[c]!) {
            spend(1);
            if (test(c)) out.push(c);
          }
        break;
      case "preceding-sibling":
        if (!attr && o.parent[id]! >= 0) {
          const sibs: number[] = [];
          for (let c = o.first[o.parent[id]!]!; c !== -1 && c !== id; c = o.next[c]!) {
            spend(1);
            if (test(c)) sibs.push(c);
          }
          for (let k = sibs.length - 1; k >= 0; k -= 1) out.push(sibs[k]!);
        }
        break;
      case "following": {
        const from = attr ? el + 1 : o.sub[id]! + 1;
        spend(N - from);
        for (let k = from; k < N; k += 1) if (test(k)) out.push(k);
        break;
      }
      case "preceding": {
        spend(el);
        // Everything before it that does not hold it: an ancestor's last row reaches past it.
        for (let k = el - 1; k >= 1; k -= 1) if (o.sub[k]! < el && test(k)) out.push(k);
        break;
      }
      default:
        throw new XPathError(`knobas does not read the ${axis} axis.`, 0);
    }
    return out;
  }
  const REVERSE = new Set(["ancestor", "ancestor-or-self", "preceding", "preceding-sibling"]);
  function order(ids: number[]): number[] {
    let sorted = true;
    let last = -1;
    for (let i = 0; i < ids.length; i += 1) {
      const k = key(ids[i]!);
      if (k <= last) {
        sorted = false;
        break;
      }
      last = k;
    }
    if (sorted) return ids;
    const keyed = ids.map((id) => [key(id), id] as const).sort((a, b) => a[0] - b[0]);
    const out: number[] = [];
    let prev = -1;
    for (const [k, id] of keyed) {
      if (k !== prev) out.push(id);
      prev = k;
    }
    return out;
  }
  /** Whether a predicate could ask where its node stands: a number, `position()` or `last()` outside a nested path. */
  function positional(e: Expr): boolean {
    if (e.k === "num") return true;
    if (e.k === "fn") return e.name === "position" || e.name === "last" || (["count", "string-length", "number", "sum", "floor", "ceiling", "round"].includes(e.name) ? true : e.args.some(positional));
    if (e.k === "arith" || e.k === "neg") return true;
    if (e.k === "and" || e.k === "or" || e.k === "cmp") return positional(e.a) || positional(e.b);
    if (e.k === "path") return e.base !== null && positional(e.base);
    if (e.k === "filter") return positional(e.a);
    return e.k !== "lit" && e.k !== "union";
  }
  function applyPreds(list: number[], preds: Expr[]): number[] {
    for (const pr of preds) {
      const kept: number[] = [];
      const size = list.length;
      for (let i = 0; i < size; i += 1) {
        const v = ev(pr, { node: list[i]!, pos: i + 1, size });
        if (typeof v === "number" ? v === i + 1 : bool(v)) kept.push(list[i]!);
      }
      list = kept;
    }
    return list;
  }
  function walk(start: number[], steps: Step[]): number[] {
    let cur = start;
    for (let s = 0; s < steps.length; s += 1) {
      let st = steps[s]!;
      // `//x` is every x under here: one pass over the rows, where no test asks for a position among siblings.
      const next = steps[s + 1];
      if (st.axis === "descendant-or-self" && st.test.t === "node" && st.preds.length === 0 && next !== undefined && next.axis === "child" && !next.preds.some(positional)) {
        st = { axis: "descendant", test: next.test, preds: next.preds };
        s += 1;
        // Nested starting points would give a row twice: keep the outermost.
        const tops: number[] = [];
        let reach = -1;
        for (const id of cur) {
          if (id < N && id > reach) {
            tops.push(id);
            reach = o.sub[id]!;
          }
        }
        cur = tops;
      }
      const test = tester(st.test, st.axis);
      const out: number[] = [];
      const simple = st.preds.length === 0;
      for (const id of cur) {
        if (simple) along(st.axis, id, test, out);
        else for (const x of applyPreds(along(st.axis, id, test, []), st.preds)) out.push(x);
      }
      cur = REVERSE.has(st.axis) || cur.length > 1 || st.axis === "following" ? order(out) : out;
      if (cur.length === 0) return cur;
    }
    return cur;
  }
  const isSet = (v: Value): v is number[] => Array.isArray(v);
  const bool = (v: Value): boolean => (isSet(v) ? v.length > 0 : typeof v === "string" ? v.length > 0 : typeof v === "number" ? v !== 0 && !Number.isNaN(v) : v);
  const string = (v: Value): string => (isSet(v) ? (v.length === 0 ? "" : str(v[0]!)) : typeof v === "number" ? numStr(v) : typeof v === "boolean" ? String(v) : v);
  const number = (v: Value): number => (typeof v === "number" ? v : typeof v === "boolean" ? (v ? 1 : 0) : toNum(string(v)));
  function compare(op: string, a: Value, b: Value): boolean {
    const cmp = <T>(x: T, y: T) => (op === "=" ? x === y : op === "!=" ? x !== y : op === "<" ? x < y : op === "<=" ? x <= y : op === ">" ? x > y : x >= y);
    const relational = op !== "=" && op !== "!=";
    if (isSet(a) && isSet(b)) {
      if (a.length === 0 || b.length === 0) return false;
      spend(a.length + b.length);
      if (!relational) {
        const seen = new Set(b.map(str));
        if (op === "=") return a.some((x) => seen.has(str(x)));
        return a.some((x) => seen.size > 1 || !seen.has(str(x)));
      }
      const bs = b.map((x) => toNum(str(x)));
      return a.some((x) => {
        const n = toNum(str(x));
        return bs.some((y) => cmp(n, y));
      });
    }
    if (isSet(a) || isSet(b)) {
      const set = (isSet(a) ? a : b) as number[];
      const other = isSet(a) ? b : a;
      const flip = !isSet(a);
      spend(set.length);
      if (typeof other === "boolean") return flip ? cmp(other, bool(set)) : cmp(bool(set), other);
      if (typeof other === "number" || relational) {
        const n = number(other);
        return set.some((x) => (flip ? cmp(n, toNum(str(x))) : cmp(toNum(str(x)), n)));
      }
      return set.some((x) => (flip ? cmp(other, str(x)) : cmp(str(x), other)));
    }
    if (relational) return cmp(number(a), number(b));
    if (typeof a === "boolean" || typeof b === "boolean") return cmp(bool(a), bool(b));
    if (typeof a === "number" || typeof b === "number") return cmp(number(a), number(b));
    return cmp(string(a), string(b));
  }
  interface Ctx {
    node: number;
    pos: number;
    size: number;
  }
  function ev(e: Expr, ctx: Ctx): Value {
    switch (e.k) {
      case "num":
      case "lit":
        return e.v;
      case "or":
        return bool(ev(e.a, ctx)) || bool(ev(e.b, ctx));
      case "and":
        return bool(ev(e.a, ctx)) && bool(ev(e.b, ctx));
      case "cmp":
        return compare(e.op, ev(e.a, ctx), ev(e.b, ctx));
      case "neg":
        return -number(ev(e.a, ctx));
      case "arith": {
        const a = number(ev(e.a, ctx));
        const b = number(ev(e.b, ctx));
        return e.op === "+" ? a + b : e.op === "-" ? a - b : e.op === "*" ? a * b : e.op === "div" ? a / b : a % b;
      }
      case "union": {
        const a = ev(e.a, ctx);
        const b = ev(e.b, ctx);
        if (!isSet(a) || !isSet(b)) throw new XPathError("| joins two sets of nodes; one side here is a value.", 0);
        return order(a.concat(b));
      }
      case "filter": {
        const a = ev(e.a, ctx);
        if (!isSet(a)) throw new XPathError("A test in [ ] stands after a value, and can only stand after nodes.", 0);
        return applyPreds(a, e.preds);
      }
      case "path": {
        let start: number[];
        if (e.base !== null) {
          const b = ev(e.base, ctx);
          if (!isSet(b)) throw new XPathError("A / stands after a value, and can only stand after nodes.", 0);
          start = b;
        } else start = e.abs ? [0] : [ctx.node];
        return walk(start, e.steps);
      }
      case "fn":
        return call(e, ctx);
    }
  }
  function call(e: { name: string; args: Expr[]; at: number }, ctx: Ctx): Value {
    const arg = (i: number) => ev(e.args[i]!, ctx);
    const s = (i: number) => (e.args.length > i ? string(arg(i)) : str(ctx.node));
    const first = (): number => {
      if (e.args.length === 0) return ctx.node;
      const v = arg(0);
      if (!isSet(v)) throw new XPathError(`${e.name}() asks for nodes and was given a value.`, e.at);
      return v.length === 0 ? -1 : v[0]!;
    };
    switch (e.name) {
      case "last":
        return ctx.size;
      case "position":
        return ctx.pos;
      case "count": {
        const v = arg(0);
        if (!isSet(v)) throw new XPathError("count() counts nodes and was given a value.", e.at);
        return v.length;
      }
      case "local-name": {
        const n = first();
        if (n < 0) return "";
        return n >= N ? o.names[o.aLocal[n - N]!]! : o.type[n] === EL ? o.names[o.local[n]!]! : o.type[n] === PI ? o.names[o.name[n]!]! : "";
      }
      case "name": {
        const n = first();
        return n < 0 ? "" : nameOf(o, n);
      }
      case "namespace-uri": {
        const n = first();
        if (n < 0) return "";
        return n >= N ? o.uris[o.aNs[n - N]!]! : o.type[n] === EL ? o.uris[o.ns[n]!]! : "";
      }
      case "string":
        return s(0);
      case "concat":
        return e.args.map((_, i) => string(arg(i))).join("");
      case "starts-with":
        return s(0).startsWith(s(1));
      case "contains":
        return s(0).includes(s(1));
      case "substring-before": {
        const a = s(0);
        const b = s(1);
        const i = a.indexOf(b);
        return i === -1 ? "" : a.slice(0, i);
      }
      case "substring-after": {
        const a = s(0);
        const b = s(1);
        const i = a.indexOf(b);
        return i === -1 ? "" : a.slice(i + b.length);
      }
      case "substring": {
        const a = [...s(0)];
        const from = Math.round(number(arg(1)));
        const len = e.args.length > 2 ? Math.round(number(arg(2))) : Infinity;
        if (Number.isNaN(from) || Number.isNaN(len)) return "";
        return a.filter((_, i) => i + 1 >= from && i + 1 < from + len).join("");
      }
      case "string-length":
        return [...s(0)].length;
      case "normalize-space":
        return s(0).replace(/[ \t\r\n]+/g, " ").replace(/^ | $/g, "");
      case "translate": {
        const a = s(0);
        const from = [...s(1)];
        const to = [...s(2)];
        let out = "";
        for (const ch of a) {
          const i = from.indexOf(ch);
          out += i === -1 ? ch : i < to.length ? to[i] : "";
        }
        return out;
      }
      case "boolean":
        return bool(arg(0));
      case "not":
        return !bool(arg(0));
      case "true":
        return true;
      case "false":
        return false;
      case "lang": {
        const want = string(arg(0)).toLowerCase();
        for (let a = ctx.node >= N ? o.aOwner[ctx.node - N]! : ctx.node; a >= 0; a = o.parent[a]!) {
          if (o.type[a] !== EL) continue;
          for (let k = o.a0[a]!, end = k + o.an[a]!; k < end; k += 1)
            if (o.names[o.aName[k]!] === "xml:lang") {
              const v = str(N + k).toLowerCase();
              return v === want || v.startsWith(want + "-");
            }
        }
        return false;
      }
      case "number":
        return e.args.length === 0 ? toNum(str(ctx.node)) : number(arg(0));
      case "sum": {
        const v = arg(0);
        if (!isSet(v)) throw new XPathError("sum() adds up nodes and was given a value.", e.at);
        spend(v.length);
        let t = 0;
        for (const x of v) t += toNum(str(x));
        return t;
      }
      case "floor":
        return Math.floor(number(arg(0)));
      case "ceiling":
        return Math.ceil(number(arg(0)));
      case "round": {
        const x = number(arg(0));
        return Number.isNaN(x) || !Number.isFinite(x) ? x : Math.floor(x + 0.5);
      }
    }
    throw new XPathError(`XPath 1.0 has no ${e.name}().`, e.at);
  }
  const v = ev(compiled.ast, { node: options.context ?? 0, pos: 1, size: 1 });
  const looked = budget - left;
  if (isSet(v)) return { type: "nodes", ids: v, looked };
  if (typeof v === "number") return { type: "number", value: v, text: numStr(v), looked };
  if (typeof v === "boolean") return { type: "boolean", value: v, text: String(v), looked };
  return { type: "string", value: v, text: v, looked };
}

/** What a row is, in one line of a list of results: its kind, its name and its value where it has one. */
export function describe(o: XmlOutline, id: number): { kind: string; name: string; value: string | null; kids: number } {
  if (id >= o.n) return { kind: "attribute", name: `@${nameOf(o, id)}`, value: stringOf(o, id), kids: 0 };
  const t = o.type[id];
  if (t === EL) {
    const leaf = o.kids[id] === 0;
    return { kind: "element", name: nameOf(o, id), value: leaf ? stringOf(o, id) : null, kids: o.kids[id]! };
  }
  if (t === DOC) return { kind: "document", name: "/", value: null, kids: o.root >= 0 ? 1 : 0 };
  return { kind: t === COMMENT ? "comment" : t === PI ? "instruction" : "text", name: t === COMMENT ? "comment" : t === PI ? nameOf(o, id) : "text", value: stringOf(o, id), kids: 0 };
}
