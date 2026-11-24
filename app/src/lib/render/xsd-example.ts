/**
 * An example made from an element of a schema set (the XSD design, 2026-11-23, sections 19 and 22): a whole document
 * with that element as its root, valid first, realistic second, the same every time.
 *
 * - **The content model decides what stands**: a sequence in its order, a choice by its pick (the first branch that
 *   ends unless one was picked), `all` in declared order, each particle as often as the options say, never under its
 *   `minOccurs` nor over its `maxOccurs`; `maxOccurs="0"` is nothing.
 * - **Attributes**: every required one, every optional one with *Everything*; `fixed` is its value, `default` where it
 *   is optional.
 * - **Namespaces**: the root's is the default one, every other gets the prefix the schema's own files give it, declared
 *   on the root; where an element in no namespace stands, no default namespace is declared.
 * - **An abstract element** is replaced by the first that may stand in for it; **an abstract type** by the first that
 *   derives from it, with `xsi:type`.
 * - **An element that holds itself** is left out the second time where it may be.
 * - **`xs:any`**: lax or skip, one made-up element; strict, nothing (and a comment where it must stand).
 * - **What cannot be made is left out, upward**; where every holder is required it stays with a comment, and the
 *   notes say so.
 * - **An element that would be empty takes its first optional part**, with required parts only.
 * - **Bounds**: with *Everything*, optional parts stop after 4,000 elements and 24 levels.
 *
 * Values are `xsd-values.ts`'. The spike's is `makeExample` in `xr-make.js`; its rules reached 5,618 of 5,618 examples
 * libxml2 judged valid.
 */
import { XML_NS } from "./xml-names";
import { baseOf } from "./xsd-locate";
import { XSI } from "./xsd-xml";
import { childElements, elDecl, elType, label, labelOfKey, openGroup, shape, typeDef, xsKey, type ComplexDecl, type ElementDecl, type ModelGroup, type Particle, type SchemaSet, type SimpleDecl } from "./xsd";
import { freshValueState, mulberry, simpleValue, valueSeed, type ValueCtx } from "./xsd-values";

/** Where *Everything* stops adding optional parts. */
export const MAX_NODES = 4000;
export const MAX_DEPTH = 24;
/** The default seed. */
export const SEED = 4711;

/** How an example is made (section 22): kept while the set is open, never saved. */
export interface ExampleOptions {
  readonly seed: number;
  readonly optional: "required" | "all";
  readonly repeats: 1 | 2 | 3;
  readonly lang: "de" | "en";
  /** A choice's id (its file and line) to the branch picked. */
  readonly picks: Readonly<Record<string, number>>;
  /** An optional row's id to how often it stands (S4b). */
  readonly ticks?: Readonly<Record<string, number>>;
}

/** A choice the example passed: its id, its branches' names and the one taken. */
export interface ExampleChoice {
  readonly id: string;
  readonly names: readonly string[];
  readonly picked: number;
}

export interface Example {
  readonly xml: string;
  /** Each line's element (a component key where it has one) and depth: a line pressed goes to its declaration. */
  readonly lines: readonly { readonly key: string | null; readonly depth: number }[];
  readonly label: string;
  readonly notes: readonly string[];
  readonly choices: readonly ExampleChoice[];
  /** How many elements it holds. */
  readonly count: number;
  readonly seed: number;
  /** The file saved: `ORDER-example-4711.xml`. */
  readonly fileName: string;
}

interface Node {
  name: string;
  ns: string;
  key: string | null;
  attrs: { name: string; ns: string; v: string; typeNs?: string }[];
  kids: Item[];
  text: string | null;
  comment?: string;
  broken?: boolean;
  any?: boolean;
}
type Item = Node | { comment: string; broken?: boolean; name?: undefined };

// ---- Choices keep an id: the file and line they are written at ----

const choiceIds = new WeakMap<SchemaSet, Map<object, string>>();
function choiceIdsOf(set: SchemaSet): Map<object, string> {
  let ids = choiceIds.get(set);
  if (ids !== undefined) return ids;
  ids = new Map();
  const visit = (p: Particle | null, file: string) => {
    if (p === null) return;
    if (p.k === "choice") ids!.set(p, `${file}:${p.line}`);
    if (p.k === "seq" || p.k === "choice" || p.k === "all") for (const it of p.items) visit(it, file);
    if (p.k === "el" && p.el.anon !== null && p.el.anon.kind === "ct") visit(p.el.anon.content, p.el.at ?? file);
  };
  for (const c of set.comps.values()) {
    if (c.kind === "ct" || c.kind === "gr") visit(c.content, c.file);
    else if (c.kind === "el" && c.anon !== null && c.anon.kind === "ct") visit(c.anon.content, c.file);
  }
  choiceIds.set(set, ids);
  return ids;
}
/** A choice's id. A group's choice opened by `openGroup` is a copy: it is known by the group's own content. */
function choiceIdOf(set: SchemaSet, q: ModelGroup): string | null {
  const ids = choiceIdsOf(set);
  const direct = ids.get(q);
  if (direct !== undefined) return direct;
  if (q.from !== undefined) {
    const g = set.comps.get(q.from);
    if (g !== undefined && g.kind === "gr" && g.content !== null) return ids.get(g.content) ?? null;
  }
  return null;
}

// ---- The fewest levels an element needs to end: a choice's default branch is the one that ends soonest ----

function depthOf(set: SchemaSet, el: ElementDecl, memo: Map<object, number>, trail: Set<object>): number {
  const decl = elDecl(set, el);
  if (decl === null) return 1;
  const known = memo.get(decl);
  if (known !== undefined) return known;
  if (trail.has(decl)) return Infinity;
  trail.add(decl);
  const sh = shape(set, elType(set, decl).def);
  let d = 1;
  if (sh !== null && sh.content !== null) d = 1 + particleDepth(set, sh.content, memo, trail);
  trail.delete(decl);
  if (d !== Infinity) memo.set(decl, d);
  return d;
}
function particleDepth(set: SchemaSet, p: Particle, memo: Map<object, number>, trail: Set<object>): number {
  const q = openGroup(set, p);
  if (q.min === 0) return 0;
  if (q.k === "el") return depthOf(set, q.el, memo, trail);
  if (q.k === "choice") return q.items.length === 0 ? 0 : Math.min(...q.items.map((it) => particleDepth(set, it, memo, trail)));
  if (q.k === "seq" || q.k === "all") return q.items.reduce((m, it) => Math.max(m, particleDepth(set, it, memo, trail)), 0);
  return 0;
}

/** Makes an example of the element `key` names (a global element's component key). `null` where it is no element. */
export function makeExample(set: SchemaSet, key: string, options: ExampleOptions): Example | null {
  const root = set.comps.get(key);
  if (root === undefined || root.kind !== "el") return null;
  const o = options;
  const every = o.optional === "all";
  const values = freshValueState();
  const state = { row: 0, nodes: 0, capped: false, deep: false, missing: new Set<string>(), any: 0, anyMade: 0, abstract: 0, bad: [] as string[], choices: new Map<string, ExampleChoice>() };
  const memo = new Map<object, number>();

  const count = (q: { min: number; max: number }, optionalHere: boolean): number => {
    if (q.max === 0) return 0; // maxOccurs="0": the schema forbids it here
    if (!every || !optionalHere) return q.min;
    if (state.nodes > MAX_NODES) {
      state.capped ||= q.min === 0;
      return q.min;
    }
    return q.max > 1 ? Math.max(q.min, Math.min(q.max, o.repeats)) : Math.max(q.min, 1);
  };

  function element(el: ElementDecl, path: string, depth: number, trail: Set<object>, index: number, siblings: number): Item {
    let decl = elDecl(set, el);
    if (decl === null) {
      const named = labelOfKey(set, el.ref ?? el.name);
      state.missing.add(named);
      return { comment: `${named}: not found here`, broken: true };
    }
    if (decl.abstract) {
      let member: ElementDecl | null = null;
      for (const k of set.members.get(decl.key ?? "") ?? []) {
        const m = set.comps.get(k);
        if (m !== undefined && m.kind === "el" && !m.abstract) {
          member = m;
          break;
        }
      }
      if (member === null) return { comment: `${decl.name} is abstract and nothing here stands in for it`, broken: true };
      state.abstract += 1;
      decl = member;
    }
    state.nodes += 1;
    const node: Node = { name: decl.name, ns: decl.elNs, key: decl.key ?? null, attrs: [], kids: [], text: null };
    let t = elType(set, decl);
    if (t.def !== null && t.def.kind === "ct" && t.def.abstract && t.key !== null) {
      const sub = (set.derived.get(`ty ${t.key}`) ?? []).map((k) => set.comps.get(k)).find((d) => d !== undefined && (d.kind === "ct" || d.kind === "st") && !(d.kind === "ct" && d.abstract));
      if (sub !== undefined && (sub.kind === "ct" || sub.kind === "st")) {
        node.attrs.push({ name: "type", ns: XSI, v: labelOfKey(set, sub.key), typeNs: sub.ns });
        t = { def: sub, key: sub.key.slice(3) };
      }
    }
    if (t.def === null) {
      if (t.missing !== undefined && t.missing !== null) state.missing.add(labelOfKey(set, t.missing));
      node.comment = "its type was not found here";
      node.broken = true;
      return node;
    }
    const sh = shape(set, t.def);
    if (sh === null) return node;
    const rnd = mulberry(valueSeed(o.seed, path));
    // The turn of the nearest repeated element above: a line's number counts up with its item.
    const row = siblings > 1 ? index : state.row;
    const ctx: ValueCtx = { name: decl.name, lang: o.lang, index: row, siblings, at: `${o.seed}\n${path}`, state: values };
    for (const a of sh.attrs) {
      if (!("at" in a)) continue;
      const at = a.at;
      if (at.use !== "required" && !every) continue;
      if (at.use === "prohibited") continue;
      let v: string;
      if (at.fixed !== null) v = at.fixed;
      else if (at.def !== null && at.use !== "required") v = at.def;
      else {
        const def: ComplexDecl | SimpleDecl | null = at.anon !== null ? at.anon : at.type !== null ? typeDef(set, at.type) : typeDef(set, xsKey("string"));
        const got = simpleValue(set, def, { ...ctx, name: at.name, holder: decl.name, at: `${o.seed}\n${path}/@${at.name}` }, mulberry(valueSeed(o.seed, `${path}/@${at.name}`)));
        v = got.v;
        if (got.bad !== undefined) state.bad.push(`@${at.name} of ${decl.name}: ${got.bad}`);
      }
      node.attrs.push({ name: at.name, ns: at.attrNs, v });
    }
    if (sh.simple !== null && sh.content === null) {
      if (decl.fixed !== null) node.text = decl.fixed;
      else if (decl.def !== null && !every) node.text = decl.def;
      else {
        const got = simpleValue(set, t.def, ctx, rnd);
        node.text = got.v;
        if (got.bad !== undefined) state.bad.push(`${decl.name}: ${got.bad}`);
      }
      return node;
    }
    if (sh.content !== null) {
      if (depth >= MAX_DEPTH) state.deep = true;
      else {
        const within = new Set(trail).add(decl);
        const above = state.row;
        state.row = row;
        if (!walk(sh.content, node.kids, path, depth + 1, within)) node.broken = true;
        state.row = above;
        // Required parts only would leave it empty (`<PRODUCT_ID/>`): it takes its first optional part, so it says something.
        if (!every && node.kids.length === 0 && node.attrs.length === 0 && node.broken !== true) {
          for (const c of childElements(set, sh.content, [], 1, 1)) {
            if (c.max === 0) continue;
            const d = elDecl(set, c.el);
            if (d === null || within.has(d)) continue;
            const first = element(c.el, `${path}/${c.el.name}[0]`, depth + 1, within, 0, 1);
            if (first.broken !== true) node.kids.push(first);
            break;
          }
        }
      }
    }
    if (node.kids.length === 0 && sh.mixed) node.text = simpleValue(set, typeDef(set, xsKey("string")), { ...ctx, kind: "text" }, rnd).v;
    if (t.key === xsKey("anyType")) node.text = "";
    return node;
  }

  /**
   * Writes what particle `p` asks for into `out`. Answers `false` where a required part could not be made: the element
   * that holds it is then left out where it is optional, and the example says so where it is not.
   */
  function walk(p: Particle, out: Item[], path: string, depth: number, trail: Set<object>): boolean {
    const q = openGroup(set, p);
    if (q.k === "missing") {
      state.missing.add(labelOfKey(set, q.ref));
      return q.min === 0;
    }
    if (q.k === "loop" || q.k === "grp") return true;
    if (q.k === "el") {
      const decl = elDecl(set, q.el);
      // An element that holds itself is left out the second time where it may be.
      const again = decl !== null && trail.has(decl);
      let n = again ? q.min : count(q, true);
      if (again && q.min > 0 && depth >= MAX_DEPTH) {
        state.deep = true;
        n = 0;
      }
      let whole = true;
      for (let i = 0; i < n; i += 1) {
        const node = element(q.el, `${path}/${q.el.name}[${i}]`, depth, trail, i, n);
        if (node.broken === true && i >= q.min) continue; // optional: left out
        if (node.broken === true) whole = false;
        out.push(node);
      }
      return whole;
    }
    if (q.k === "any") {
      const n = count(q, true);
      let whole = true;
      for (let i = 0; i < n; i += 1) {
        const made = anyElement(q);
        if (made === null) {
          if (i >= q.min) continue;
          state.any += 1;
          out.push({ comment: "any declared element may stand here; nothing in this schema set fits" });
          whole = false;
        } else out.push(made);
      }
      return whole;
    }
    const n = count(q, true);
    let whole = true;
    for (let i = 0; i < n; i += 1) {
      const mark = out.length;
      let ok = true;
      if (q.k === "choice") {
        if (q.items.length === 0) continue;
        const id = choiceIdOf(set, q);
        let pick = id === null ? undefined : o.picks[id];
        if (pick === undefined || pick >= q.items.length) {
          // The first branch that ends; where several do, the first.
          pick = q.items.findIndex((it) => particleDepth(set, it, memo, new Set(trail)) !== Infinity);
          if (pick === -1) pick = 0;
        }
        if (id !== null && !state.choices.has(id)) state.choices.set(id, { id, names: q.items.map(branchName), picked: pick });
        ok = walk(q.items[pick]!, out, `${path}|${pick}`, depth, trail);
      } else for (const it of q.items) if (!walk(it, out, n > 1 ? `${path}#${i}` : path, depth, trail)) ok = false;
      if (!ok && i >= q.min) out.length = mark; // an optional group that could not be made whole is left out
      else if (!ok) whole = false;
    }
    return whole;
  }

  /** What stands at an `xs:any`: lax or skip, one made-up element in a namespace it allows; strict, nothing. */
  function anyElement(q: { ns: string; pc: string }): Node | null {
    if (q.pc === "strict") return null;
    const home = root!.ns;
    let ns: string;
    if (q.ns === "##any" || q.ns === "##other") ns = "urn:example:anything";
    else if (q.ns === "##local") ns = "";
    else if (q.ns === "##targetNamespace") ns = home;
    else ns = q.ns.split(/\s+/).map((u) => (u === "##targetNamespace" ? home : u === "##local" ? "" : u))[0] ?? "";
    state.anyMade += 1;
    return { name: "Anything", ns, key: null, attrs: [], kids: [], text: "any element may stand here", any: true };
  }
  function branchName(it: Particle): string {
    const q = openGroup(set, it);
    if (q.k === "el") return label(set, q.el.elNs, q.el.name);
    if (q.k === "any") return "any element";
    const inner = childElements(set, q, [], 1, 1).map((c) => label(set, c.el.elNs, c.el.name));
    return inner.length === 0 ? "nothing" : inner.length <= 2 ? inner.join(" + ") : `${inner[0]} and ${inner.length - 1} more`;
  }

  const tree = element(root, `/${root.name}`, 0, new Set(), 0, 1);
  if (tree.name === undefined) return null;

  // ---- Writing it: the root's namespace is the default one unless an element stands in none ----
  const used = new Map<string, string | null>();
  let bare = false;
  const scan = (n: Item) => {
    if (n.name === undefined) return;
    if (n.ns === "") bare = true;
    else if (!used.has(n.ns)) used.set(n.ns, null);
    for (const a of n.attrs) {
      if (a.ns !== "" && !used.has(a.ns)) used.set(a.ns, null);
      if (a.typeNs !== undefined && !used.has(a.typeNs)) used.set(a.typeNs, null);
    }
    for (const k of n.kids) scan(k);
  };
  scan(tree);
  const defaultNs = !bare && tree.ns !== "" ? tree.ns : null;
  const taken = new Set<string>();
  const prefixFor = (ns: string) => {
    let p = ns === XSI ? "xsi" : ns === XML_NS ? "xml" : (set.prefixes.get(ns) ?? "ns");
    while (taken.has(p)) p += "1";
    taken.add(p);
    return p;
  };
  for (const ns of used.keys()) if (ns !== defaultNs) used.set(ns, prefixFor(ns));
  // An attribute in a namespace needs a prefix even where its namespace is the default one.
  const attrPrefix = (ns: string) => {
    const had = used.get(ns);
    if (had !== undefined && had !== null) return had;
    const p = prefixFor(ns);
    used.set(ns, p);
    return p;
  };
  const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  const escA = (s: string) => esc(s).replace(/"/g, "&quot;");
  const qn = (n: Node) => (n.ns === "" || n.ns === defaultNs ? n.name : `${used.get(n.ns)}:${n.name}`);
  const lines: { key: string | null; depth: number }[] = [];
  const text: string[] = [];
  const push = (s: string, key: string | null, depth: number) => {
    text.push(s);
    lines.push({ key, depth });
  };
  const ROOT_MARK = "\u0000";
  const write = (n: Item, depth: number, top: boolean): void => {
    const ind = "  ".repeat(depth);
    if (n.name === undefined) return push(`${ind}<!-- ${n.comment} -->`, null, depth);
    let attrs = "";
    for (const a of n.attrs) attrs += ` ${a.ns === "" ? a.name : `${a.ns === XML_NS ? "xml" : attrPrefix(a.ns)}:${a.name}`}="${escA(a.v)}"`;
    const name = qn(n);
    const open = `${ind}<${name}${top ? ROOT_MARK : ""}${attrs}`;
    if (n.comment !== undefined) return push(`${open}/> <!-- ${n.comment} -->`, n.key, depth);
    if (n.any === true) return push(`${open}${n.ns === "" ? "" : ` xmlns:${used.get(n.ns)}="${escA(n.ns)}"`}>${esc(n.text ?? "")}</${name}>`, null, depth);
    if (n.kids.length === 0) return push(n.text === null || n.text === "" ? `${open}/>` : `${open}>${esc(n.text)}</${name}>`, n.key, depth);
    push(`${open}>`, n.key, depth);
    for (const k of n.kids) write(k, depth + 1, false);
    push(`${ind}</${name}>`, n.key, depth);
  };
  write(tree, 0, true);
  let decls = defaultNs !== null ? ` xmlns="${escA(defaultNs)}"` : "";
  for (const [ns, p] of used) if (p !== null && ns !== XML_NS && ns !== "urn:example:anything") decls += ` xmlns:${p}="${escA(ns)}"`;
  const rootAt = text.findIndex((l) => l.includes(ROOT_MARK));
  text[rootAt] = text[rootAt]!.replace(ROOT_MARK, decls);

  const plural = (n: number, one: string, many: string) => (n === 1 ? one : many.replace("#", n.toLocaleString("en")));
  const notes: string[] = [];
  if (state.missing.size > 0) notes.push(`${plural(state.missing.size, "One name was", "# names were")} not found here, so the example leaves ${state.missing.size === 1 ? "it" : "them"} out: ${[...state.missing].slice(0, 4).join(", ")}${state.missing.size > 4 ? "…" : ""}.`);
  if (state.capped) notes.push(`Optional parts stop after ${MAX_NODES.toLocaleString("en")} elements.`);
  if (state.deep) notes.push(`The schema nests deeper than ${MAX_DEPTH} levels here; the example stops there.`);
  if (state.any > 0) notes.push(`${plural(state.any, "One place needs", "# places need")} a declared element of another schema (xs:any, strict), which this set does not hold.`);
  if (state.anyMade > 0) notes.push(`${plural(state.anyMade, "One place takes", "# places take")} any element (xs:any): the example puts a made-up one there.`);
  if (values.unmade.length > 0) notes.push(`knobas could not make a value for ${plural(values.unmade.length, "one pattern", "# patterns")}: ${values.unmade[0]!.pattern.slice(0, 40)}.`);
  if (values.dangling) notes.push("An IDREF names an ID the example does not hold.");
  for (const b of state.bad.slice(0, 3)) notes.push(`A made value may not fit: ${b}.`);
  const said = `Made by knobas from ${baseOf(root.file)} (${root.name}), seed ${o.seed}, ${every ? "every optional part" : "required parts only"}. The values are invented.`;
  return {
    xml: `<?xml version="1.0" encoding="UTF-8"?>\n<!-- ${said} -->\n${text.join("\n")}\n`,
    lines: [{ key: null, depth: 0 }, { key: null, depth: 0 }, ...lines],
    label: said,
    notes,
    count: state.nodes,
    seed: o.seed,
    choices: [...state.choices.values()],
    fileName: `${root.name}-example-${o.seed}.xml`,
  };
}
