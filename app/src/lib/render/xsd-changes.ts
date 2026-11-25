/**
 * Two versions of a schema compared by what they declare, not by their lines (the XSD design, 2026-11-23, section 27;
 * step S7). A declaration is read into facts, each at a place a person can read (`ORDER_ITEM › PRICE_LINE_AMOUNT`), and
 * the two sides' facts are compared. Formatting, attribute order, a prefix spelled differently and an enumeration's
 * order are no change. Where XML Schema itself says which way a change goes, the fact says it: *stricter* (a document
 * that passed may fail now), *looser* (a document that passes now may have failed before) or *different*. Nothing here
 * says whether a change is acceptable.
 *
 * Pure: it reads two loaded sets (`xsd.ts`), in the schema worker.
 */
import { label, labelOfKey, type AttrUse, type ComplexDecl, type Decl, type Docu, type Facets, type Particle, type SchemaSet, type SimpleDecl } from "./xsd";

/** Which way a change goes. */
export type Dir = "stricter" | "looser" | "different";

/** How a fact is compared. */
type FactKind = "type" | "set" | "pattern" | "fixed" | "default" | "use" | "occurs" | "choice" | "order" | "words" | "has" | "abstract" | "mixed" | "nillable" | keyof Facets;

/** A fact of a declaration: its value, and how it is compared. */
interface Fact {
  readonly kind: FactKind;
  readonly v: unknown;
}

/** One fact that differs, in words: `\u0001…\u0002` marks what is drawn in the code face. */
export interface FactChange {
  readonly place: string;
  readonly text: string;
  readonly dir: Dir | null;
  /** The part came or went whole (an element, an attribute): what stood under it goes with it. */
  readonly how?: "added" | "removed";
  /** Only its documentation's words changed. */
  readonly words?: true;
}

/** A declaration that changed itself: its facts in the declaration's order. */
export interface Changed {
  readonly key: string;
  readonly facts: readonly FactChange[];
  /** Only words changed: it marks nothing *changed through* it. */
  readonly wordsOnly: boolean;
}

export interface Changes {
  readonly added: string[];
  readonly removed: string[];
  readonly changed: Changed[];
  /** A declaration whose own facts are the same but which names one that changed: by its key, the one it names. */
  readonly through: Map<string, string>;
  /** Declarations that read the same, however they are written. */
  same: number;
  readonly counts: Record<Dir | "words", number>;
}

/** How often, as the explorer writes it: `1`, `0..1`, `1..∞`. */
export const occ = (min: number, max: number): string => (min === max ? String(min) : `${min}..${max === Infinity ? "∞" : max}`);
const code = (s: string): string => `\u0001${s}\u0002`;

const FACET_WORDS: Partial<Record<keyof Facets, string>> = {
  length: "length",
  minLength: "shortest",
  maxLength: "longest",
  totalDigits: "digits",
  fractionDigits: "decimal places",
  minInclusive: "at least",
  maxInclusive: "at most",
  minExclusive: "more than",
  maxExclusive: "less than",
  whiteSpace: "white space",
  explicitTimezone: "time zone",
};
/** A facet a larger value of which takes more documents in. */
const UP_LOOSER = new Set<string>(["maxLength", "maxInclusive", "maxExclusive", "totalDigits", "fractionDigits"]);
/** A facet a larger value of which takes fewer. */
const UP_STRICTER = new Set<string>(["minLength", "minInclusive", "minExclusive"]);

/** A declaration as facts: place to fact, in the declaration's own order. */
export function factsOf(set: SchemaSet, comp: Decl): Map<string, Fact> {
  const out = new Map<string, Fact>();
  const put = (place: string, kind: FactKind, v: unknown) => out.set(place, { kind, v });
  const name = (key: string | null | undefined) => (key === null || key === undefined ? "" : labelOfKey(set, key));
  const words = (docu: readonly Docu[]) =>
    docu
      .map((d) => d.text + d.facts.map((f) => f.join(": ")).join("; "))
      .join("\n")
      .replace(/\s+/g, " ")
      .trim();
  /** A place not taken yet: the same name twice in one model is told apart by its turn. */
  const free = (base: string) => {
    let place = base;
    for (let n = 2; out.has(place); n += 1) place = `${base} (${n})`;
    return place;
  };

  function facets(f: Facets, at: string) {
    for (const k of Object.keys(f) as (keyof Facets)[]) {
      if (k === "enumeration") put(`${at}allowed values`, "set", (f.enumeration ?? []).map((e) => e.v));
      else if (k === "pattern") put(`${at}pattern`, "pattern", (f.pattern ?? []).join("  or  "));
      else put(`${at}${FACET_WORDS[k] ?? k}`, k, f[k]);
    }
  }
  function simple(st: SimpleDecl, at: string) {
    if (st.variety === "list") put(`${at}list of`, "type", typeof st.item === "string" ? name(st.item) : "(its own)");
    if (st.variety === "union") put(`${at}one of the types`, "set", st.members.map((m) => (typeof m === "string" ? name(m) : "(its own)")));
    if (st.base !== null) put(`${at}base`, "type", name(st.base));
    if (st.anonBase !== null) simple(st.anonBase, at);
    facets(st.facets, at);
  }
  function attrs(list: readonly AttrUse[], at: string) {
    for (const a of list) {
      if ("ag" in a) put(`${at}attributes of ${name(a.ag)}`, "has", true);
      else if ("any" in a) put(`${at}any attribute`, "has", true);
      else {
        const p = `${at}@${a.ref !== null ? name(a.ref) : a.name}`;
        put(p, "use", a.use);
        if (a.type !== null) put(`${p} type`, "type", name(a.type));
        if (a.anon !== null) simple(a.anon, `${p} `);
        if (a.fixed !== null) put(`${p} fixed`, "fixed", a.fixed);
        if (a.def !== null) put(`${p} default`, "default", a.def);
      }
    }
  }
  function complex(ct: ComplexDecl, at: string, depth: number) {
    if (ct.base !== null) put(`${at}${ct.deriv === "ext" ? "extends" : "restricts"}`, "type", name(ct.base));
    if (ct.abstract) put(`${at}abstract`, "abstract", true);
    if (ct.mixed) put(`${at}text between elements`, "mixed", true);
    if (ct.anonBase !== null) simple(ct.anonBase, at);
    facets(ct.facets, at);
    attrs(ct.attrs, at);
    if (ct.content !== null) particle(ct.content, at, depth, 1, 1, false);
  }
  /** A particle's facts; answers the name it goes by in its model's order. */
  function particle(p: Particle, at: string, depth: number, lo: number, hi: number, nested: boolean): string {
    if (p.k === "el") {
      const el = p.el;
      const said = el.ref !== null ? name(el.ref) : label(set, el.elNs, el.name);
      const place = free(`${at}${said}`);
      put(place, "occurs", [p.min * lo, p.max * hi]);
      if (el.ref === null) {
        if (el.type !== null) put(`${place} type`, "type", name(el.type));
        if (el.fixed !== null) put(`${place} fixed`, "fixed", el.fixed);
        if (el.def !== null) put(`${place} default`, "default", el.def);
        if (el.nillable) put(`${place} may be nil`, "nillable", true);
        if (el.anon !== null && depth < 6) {
          if (el.anon.kind === "ct") complex(el.anon, `${place} › `, depth + 1);
          else simple(el.anon, `${place} `);
        }
      }
      return said;
    }
    if (p.k === "any") {
      put(free(`${at}any element`), "occurs", [p.min * lo, p.max * hi]);
      return "any element";
    }
    if (p.k === "grp" || p.k === "missing" || p.k === "loop") {
      const said = `group ${name(p.ref)}`;
      put(free(`${at}${said}`), "occurs", [p.min * lo, p.max * hi]);
      return said;
    }
    // A choice makes each branch optional; a sequence that repeats multiplies what it holds.
    const names: string[] = [];
    const choice = p.k === "choice";
    for (const it of p.items) names.push(particle(it, at, depth, choice ? 0 : lo * p.min, hi * p.max, nested || choice));
    if (choice) put(free(`${at}one of`), "choice", { names, occurs: [p.min * lo, p.max * hi] });
    else if (p.k === "seq" && names.length > 1) put(free(`${at}${nested ? "order inside a choice" : "order"}`), "order", names);
    return choice ? `one of ${names.length}` : names.join(", ");
  }

  if (comp.docu.length > 0) put("words", "words", words(comp.docu));
  if (comp.kind === "el") {
    if (comp.type !== null) put("type", "type", name(comp.type));
    if (comp.abstract) put("abstract", "abstract", true);
    if (comp.nillable) put("may be nil", "nillable", true);
    if (comp.fixed !== null) put("fixed", "fixed", comp.fixed);
    if (comp.def !== null) put("default", "default", comp.def);
    if (comp.sub.length > 0) put("stands in for", "set", comp.sub.map((s) => name(`el ${s}`)));
    if (comp.anon !== null) {
      if (comp.anon.kind === "ct") complex(comp.anon, "", 0);
      else simple(comp.anon, "");
    }
  } else if (comp.kind === "ct") complex(comp, "", 0);
  else if (comp.kind === "st") simple(comp, "");
  else if (comp.kind === "gr") {
    if (comp.content !== null) particle(comp.content, "", 0, 1, 1, false);
  } else if (comp.kind === "ag") attrs(comp.attrs, "");
  else if (comp.kind === "at") {
    if (comp.type !== null) put("type", "type", name(comp.type));
    if (comp.anon !== null) simple(comp.anon, "");
    if (comp.fixed !== null) put("fixed", "fixed", comp.fixed);
    if (comp.def !== null) put("default", "default", comp.def);
  }
  return out;
}

const same = (a: unknown, b: unknown): boolean => JSON.stringify(a) === JSON.stringify(b);
const LIMITS = new Set<string>(["maxLength", "maxInclusive", "maxExclusive", "minLength", "minInclusive", "minExclusive", "length", "totalDigits", "fractionDigits"]);

/** One fact that differs, in words, with the way it goes; `null` where it does not differ or another fact says it. */
export function factChange(place: string, a: Fact | undefined, b: Fact | undefined): FactChange | null {
  const kind = (a ?? b)!.kind;
  if (a === undefined) {
    const v = b!.v;
    switch (kind) {
      case "occurs": {
        const [lo, hi] = v as [number, number];
        return { place, text: lo > 0 ? `added, and it must stand (${occ(lo, hi)})` : `added (${occ(lo, hi)})`, dir: lo > 0 ? "stricter" : "looser", how: "added" };
      }
      case "use":
        return { place, text: v === "required" ? "added, and it is required" : `added (${String(v)})`, dir: v === "required" || v === "prohibited" ? "stricter" : "looser", how: "added" };
      case "words":
        return { place, text: "words added", dir: null, words: true };
      case "order":
      case "choice":
        return null; // follows from the parts that were added
      case "set":
        return { place, text: `now held to ${(v as unknown[]).length} values`, dir: "stricter", how: "added" };
      case "pattern":
        return { place, text: `now held to ${code(String(v))}`, dir: "stricter", how: "added" };
      case "fixed":
        return { place, text: `now fixed at ${code(String(v))}`, dir: "stricter", how: "added" };
      case "default":
        return { place, text: `a default, ${code(String(v))}`, dir: null, how: "added" };
      case "mixed":
      case "nillable":
        return { place, text: "now allowed", dir: "looser", how: "added" };
      case "abstract":
        return { place, text: "now abstract", dir: "stricter", how: "added" };
      case "type":
        return { place, text: `now ${code(String(v))}`, dir: null, how: "added" };
      default:
        return { place, text: `now ${code(String(v))}`, dir: LIMITS.has(kind) ? "stricter" : null, how: "added" };
    }
  }
  if (b === undefined) {
    switch (kind) {
      case "occurs":
      case "use":
        return { place, text: "removed", dir: "stricter", how: "removed" };
      case "words":
        return { place, text: "words removed", dir: null, words: true };
      case "order":
      case "choice":
        return null;
      case "set":
      case "pattern":
      case "fixed":
        return { place, text: "no longer held to it", dir: "looser", how: "removed" };
      case "mixed":
      case "nillable":
        return { place, text: "no longer allowed", dir: "stricter", how: "removed" };
      case "abstract":
        return { place, text: "no longer abstract", dir: "looser", how: "removed" };
      case "default":
        return { place, text: "no default any more", dir: null, how: "removed" };
      default:
        return { place, text: "no longer held to it", dir: kind === "type" ? null : "looser", how: "removed" };
    }
  }
  if (same(a.v, b.v)) return null;
  if (kind === "occurs") {
    const [a0, a1] = a.v as [number, number];
    const [b0, b1] = b.v as [number, number];
    const tighter = b0 > a0 || b1 < a1;
    const wider = b0 < a0 || b1 > a1;
    const said = a0 === 0 && b0 > 0 && a1 === b1 ? "optional, now it must stand" : a0 > 0 && b0 === 0 && a1 === b1 ? "it had to stand, now optional" : `${occ(a0, a1)} → ${occ(b0, b1)}`;
    return { place, text: said, dir: tighter && wider ? "different" : tighter ? "stricter" : "looser" };
  }
  if (kind === "use") return { place, text: `${String(a.v)} → ${String(b.v)}`, dir: b.v === "required" || b.v === "prohibited" ? "stricter" : "looser" };
  if (kind === "set") {
    const av = a.v as string[];
    const bv = b.v as string[];
    const gone = av.filter((v) => !bv.includes(v));
    const come = bv.filter((v) => !av.includes(v));
    // The order of an enumeration is no change.
    if (gone.length === 0 && come.length === 0) return null;
    const list = (vs: string[]) => vs.slice(0, 6).map(code).join(" ") + (vs.length > 6 ? ` and ${vs.length - 6} more` : "");
    const text = [come.length > 0 ? `added ${list(come)}` : "", gone.length > 0 ? `removed ${list(gone)}` : ""].filter(Boolean).join("; ");
    return { place, text, dir: gone.length > 0 && come.length > 0 ? "different" : gone.length > 0 ? "stricter" : "looser" };
  }
  if (kind === "choice") {
    const av = a.v as { names: string[]; occurs: [number, number] };
    const bv = b.v as { names: string[]; occurs: [number, number] };
    const gone = av.names.filter((v) => !bv.names.includes(v));
    const come = bv.names.filter((v) => !av.names.includes(v));
    if (gone.length === 0 && come.length === 0) return same(av.occurs, bv.occurs) ? null : factChange(place, { kind: "occurs", v: av.occurs }, { kind: "occurs", v: bv.occurs });
    return null; // each branch added or removed is its own fact
  }
  if (kind === "order") {
    const av = a.v as string[];
    const bv = b.v as string[];
    const keptA = av.filter((v) => bv.includes(v));
    const keptB = bv.filter((v) => av.includes(v));
    if (same(keptA, keptB)) return null;
    const moved = keptB.find((v, i) => keptA[i] !== v) ?? "";
    return { place: place.replace(/order( inside a choice)?( \(\d+\))?$/, "").replace(/ › $/, ""), text: `the order changed: ${code(moved)} moved`, dir: "different" };
  }
  if (kind === "words") return { place, text: "words changed", dir: null, words: true };
  let dir: Dir | null = null;
  if (UP_LOOSER.has(kind)) dir = Number(b.v) > Number(a.v) ? "looser" : "stricter";
  else if (UP_STRICTER.has(kind)) dir = Number(b.v) > Number(a.v) ? "stricter" : "looser";
  else if (kind === "type" || kind === "pattern" || kind === "fixed" || kind === "length") dir = "different";
  return { place, text: `${code(String(a.v))} → ${code(String(b.v))}`, dir };
}

/** What a declaration is, in words. */
export const KIND: Record<Decl["kind"], string> = { el: "element", ct: "complex type", st: "simple type", gr: "group", ag: "attribute group", at: "attribute" };

/**
 * What differs between the declarations of one file in two sets: `before` and `after` name it on each side (a rename
 * compares the old path's with the new). A side that is `null` has no schema: everything on the other is added or removed.
 */
export function changes(a: SchemaSet | null, b: SchemaSet | null, before: string, after: string): Changes {
  const mine = (set: SchemaSet | null, file: string) => new Map(set === null ? [] : [...set.comps].filter(([, c]) => c.file === file));
  const A = mine(a, before);
  const B = mine(b, after);
  const out: Changes = { added: [], removed: [], changed: [], through: new Map(), same: 0, counts: { stricter: 0, looser: 0, different: 0, words: 0 } };
  for (const [key, cb] of B) {
    const ca = A.get(key);
    if (ca === undefined) {
      out.added.push(key);
      continue;
    }
    const fa = factsOf(a!, ca);
    const fb = factsOf(b!, cb);
    const facts: FactChange[] = [];
    for (const place of new Set([...fa.keys(), ...fb.keys()])) {
      const f = factChange(place, fa.get(place), fb.get(place));
      if (f !== null) facts.push(f);
    }
    // A part that came or went takes what stood under it with it: one fact, not ten.
    const whole = facts.filter((f) => f.how !== undefined && !f.place.includes(" ")).map((f) => f.place);
    const kept = facts.filter((f) => !whole.some((w) => f.place !== w && (f.place.startsWith(`${w} `) || f.place.startsWith(`${w} › `))));
    if (kept.length === 0) {
      out.same += 1;
      continue;
    }
    if (ca.kind !== cb.kind) kept.unshift({ place: "", text: `was a ${KIND[ca.kind]}, now a ${KIND[cb.kind]}`, dir: "different" });
    out.changed.push({ key, facts: kept, wordsOnly: kept.every((f) => f.words === true) });
    for (const f of kept) {
      if (f.dir !== null) out.counts[f.dir] += 1;
      if (f.words === true) out.counts.words += 1;
    }
  }
  for (const key of A.keys()) if (!B.has(key)) out.removed.push(key);
  // Who names a changed declaration: marked as changed through it, where it did not change itself. One step.
  const touched = new Set([...out.changed.filter((c) => !c.wordsOnly).map((c) => c.key), ...out.removed]);
  // A declaration of another file of the set that this file's names, changed between the sides (its file changed too).
  if (a !== null && b !== null) {
    for (const [target, users] of b.uses) {
      if (B.has(target) || touched.has(target) || !users.some((u) => B.has(u.by))) continue;
      const ca = a.comps.get(target);
      const cb = b.comps.get(target);
      if (ca === undefined || cb === undefined) {
        if (ca !== undefined || cb !== undefined) touched.add(target);
        continue;
      }
      const fa = factsOf(a, ca);
      const fb = factsOf(b, cb);
      for (const place of new Set([...fa.keys(), ...fb.keys()])) {
        const f = factChange(place, fa.get(place), fb.get(place));
        if (f !== null && f.words !== true) {
          touched.add(target);
          break;
        }
      }
    }
  }
  if (b !== null) {
    const added = new Set(out.added);
    for (const t of touched) {
      for (const u of b.uses.get(t) ?? []) if (B.has(u.by) && !touched.has(u.by) && !added.has(u.by) && !out.through.has(u.by)) out.through.set(u.by, t);
    }
  }
  return out;
}

/** The fact's text cut into runs, the code face's marked. */
export function runsOf(text: string): { text: string; code?: true }[] {
  const out: { text: string; code?: true }[] = [];
  for (const part of text.split(/(\u0001[^\u0002]*\u0002)/)) {
    if (part === "") continue;
    if (part.startsWith("\u0001")) out.push({ text: part.slice(1, -1), code: true });
    else out.push({ text: part });
  }
  return out;
}
