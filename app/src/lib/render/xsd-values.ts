/**
 * A value for an example made from a schema (the XSD design, 2026-11-23, section 20): chosen in a fixed order, and a
 * guess kept only if the type takes it. Valid first, realistic second, the same every time.
 *
 * 1. `fixed`, then `default` (the maker's, `xsd-example.ts`).
 * 2. An enumeration: the value a reader expects where it is allowed (`EUR`, `DE`, `C62`), else a seeded pick among the
 *    first 40.
 * 3. What the name asks for (`example-names.ts`' trade block), each candidate tested against every facet and pattern.
 * 4. A value the type alone suggests: `true`, a date, a time.
 * 5. A string made from the pattern (`xsd-pattern.ts`), a pattern's plain words preferred.
 * 6. The built-in type with its bounds; a string nobody's name explains is its own name in words.
 *
 * Dates are made against a fixed day, never today. Bytes are bytes whatever their name says. People, companies,
 * streets, postcodes, cities, phones and bank details are Faker's in the language picked (`example-fakes.ts`); product
 * names, texts and features are knobas's own, in German and English: Faker's German has none, its English ones are
 * jokes.
 */
import { fakeValue, seedOf } from "./example-fakes";
import { tradeKindOf, wordsOf, type Kind, type TradeKind } from "./example-names";
import { simpleShape, type ComplexDecl, type SchemaSet, type SimpleDecl, type SimpleShape } from "./xsd";
import { rx, rxHasWords, rxMake, rxTest } from "./xsd-pattern";

/** The day dates are made against, so an example never changes with the calendar. */
const REF_DAY = Date.UTC(2026, 10, 23);

/** A generator seeded by `seed`: mulberry32, the same numbers on every machine. */
export function mulberry(seed: number): () => number {
  let s = seed >>> 0 || 1;
  return () => {
    s = (s + 0x6d2b79f5) | 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** What one example's values share: the ids made, the parties to refer to, what could not be made. */
export interface ValueState {
  pool: string[];
  taken: number;
  ids: number;
  dangling: boolean;
  unmade: { name: string; pattern: string }[];
}
export const freshValueState = (): ValueState => ({ pool: [], taken: 0, ids: 0, dangling: false, unmade: [] });

/** Where a value stands: its name, what holds it, its turn among its siblings, the language, the example's seed and path. */
export interface ValueCtx {
  name: string;
  holder?: string;
  lang: "de" | "en";
  /** The turn of the nearest repeated element above: a line's number counts up with its item. */
  index: number;
  siblings: number;
  /** The example's seed and the path the value stands at: Faker's seed. */
  at: string;
  state: ValueState;
  /** A kind given by the caller (a mixed element's words are text). */
  kind?: TradeKind | null;
}

/** What an enumeration's reader expects, where the schema allows it. */
const EXPECTED: Partial<Record<TradeKind, readonly string[]>> = {
  currency: ["EUR", "USD", "CHF", "GBP"],
  countryCode: ["DE", "AT", "CH", "NL", "FR", "US"],
  country: ["DE", "Germany", "Deutschland"],
  unit: ["C62", "PCE", "H87", "PK", "KGM", "MTR", "LTR", "EA"],
  language: ["deu", "de", "ger", "eng", "en"],
};

/** The kinds Faker makes, as the API explorer's table names them. */
const FAKED: Partial<Record<TradeKind, Kind>> = {
  firstName: "firstName",
  lastName: "lastName",
  fullName: "fullName",
  email: "email",
  phone: "phone",
  fax: "phone",
  street: "street",
  zip: "zip",
  city: "city",
  state: "state",
  company: "company",
  iban: "iban",
  bic: "bic",
};

/** knobas's own products, fourteen a language: what a catalogue or an order names. */
const PRODUCTS: Record<"de" | "en", readonly string[]> = {
  de: [
    "Akku-Bohrschrauber 18 V",
    "Kugellager 6204-2RS",
    "Wellendichtring 20x35x7",
    "Sechskantschraube M8x40",
    "Schutzhandschuhe Größe 9",
    "Kabelbinder 200 mm",
    "Werkzeugkoffer leer",
    "LED-Arbeitsleuchte 20 W",
    "Druckluftschlauch 10 m",
    "Schleifpapier K120",
    "Drehmomentschlüssel 40–200 Nm",
    "Gliedermaßstab 2 m",
    "Spiralbohrer HSS 6 mm",
    "Kreuzschlitz-Schraubendreher PH2",
  ],
  en: [
    "Cordless drill 18 V",
    "Ball bearing 6204-2RS",
    "Shaft seal 20x35x7",
    "Hex bolt M8x40",
    "Safety gloves size 9",
    "Cable ties 200 mm",
    "Tool case, empty",
    "LED work light 20 W",
    "Air hose 10 m",
    "Sandpaper P120",
    "Torque wrench 40–200 Nm",
    "Folding rule 2 m",
    "HSS twist drill 6 mm",
    "Phillips screwdriver PH2",
  ],
};
const PRODUCT_TEXT: Record<"de" | "en", (name: string) => string> = {
  de: (name) => `${name}, in der Originalverpackung, mit Prüfprotokoll.`,
  en: (name) => `${name}, in its original packaging, with an inspection report.`,
};
const PHRASES: Record<"de" | "en", readonly string[]> = {
  de: ["Bitte an Tor 3 liefern.", "Lieferung nur werktags von 7 bis 15 Uhr.", "Teillieferungen sind erlaubt.", "Rechnung bitte als PDF."],
  en: ["Please deliver to gate 3.", "Deliveries on weekdays from 7 to 3 only.", "Part deliveries are allowed.", "Please send the invoice as a PDF."],
};
const FEATURES: Record<"de" | "en", readonly (readonly [string, string])[]> = {
  de: [
    ["Spannung", "18 V"],
    ["Gewicht", "1,4 kg"],
    ["Farbe", "Blau"],
    ["Werkstoff", "Edelstahl"],
  ],
  en: [
    ["Voltage", "18 V"],
    ["Weight", "1.4 kg"],
    ["Colour", "Blue"],
    ["Material", "Stainless steel"],
  ],
};
const KEYWORDS: Record<"de" | "en", readonly string[]> = { de: ["Werkzeug", "Lager", "Schrauben", "Arbeitsschutz"], en: ["tools", "bearings", "fasteners", "safety"] };

const pad = (n: number, w: number) => String(n).padStart(w, "0");
/** Which way a date leans by its name: a start before the day, an end, a due date or a delivery after it. */
const lean = (name: string) => (/start|begin|from|order|generation|issue|von/i.test(name) ? -25 : /end|until|due|deliver|valid|expir|bis/i.test(name) ? 25 : 0);
function dayOf(rnd: () => number, spread: number, shift = 0): string {
  const d = new Date(REF_DAY + (Math.floor((rnd() - 0.5) * spread) + shift) * 86400000);
  return `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1, 2)}-${pad(d.getUTCDate(), 2)}`;
}
const clock = (rnd: () => number) => `${pad(7 + Math.floor(rnd() * 11), 2)}:${pad(Math.floor(rnd() * 12) * 5, 2)}:00`;
const pickOf = <T>(list: readonly T[], rnd: () => number): T => list[Math.floor(rnd() * list.length)]!;

/** A GTIN-13 with its check digit. */
export function gtin(rnd: () => number): string {
  let digits = "40";
  for (let i = 0; i < 10; i += 1) digits += Math.floor(rnd() * 10);
  let sum = 0;
  for (let i = 0; i < 12; i += 1) sum += Number(digits[i]) * (i % 2 === 0 ? 1 : 3);
  return digits + ((10 - (sum % 10)) % 10);
}

/** The candidates a kind offers, the likeliest first. Each is tried against the type's facets and patterns. */
function candidates(kind: TradeKind, ctx: ValueCtx, rnd: () => number): string[] {
  const lang = ctx.lang;
  const faked = FAKED[kind];
  if (faked !== undefined) return [String(fakeValue(faked, `${ctx.at}\n${kind}`, lang))];
  switch (kind) {
    case "gtin":
      return [gtin(rnd)];
    case "currency":
    case "unit":
      return [...(EXPECTED[kind] ?? [])];
    case "countryCode":
      return lang === "de" ? ["DE", "AT", "CH"] : ["GB", "US", "DE"];
    case "country":
      return [lang === "de" ? "Deutschland" : "Germany", "DE"];
    case "language":
      return lang === "de" ? ["deu", "de", "ger", "de-DE"] : ["eng", "en", "en-GB"];
    case "taxRate":
      return ["0.19", "19", "19.00", "0.07"];
    case "price": {
      const v = (5 + rnd() * 495).toFixed(2);
      return [v, String(Math.round(Number(v)))];
    }
    case "quantity":
      return [String(1 + Math.floor(rnd() * 24)), "1"];
    case "weight":
      return [(0.2 + rnd() * 20).toFixed(1), "2"];
    case "count":
      return [String(ctx.siblings > 1 ? ctx.siblings : 1 + Math.floor(rnd() * 5))];
    case "lineId":
      return [String(ctx.index + 1)];
    case "serial":
      return [`${/lot|batch|charge/i.test(ctx.name) ? "LOT" : "SN"}-2026-${pad(Math.floor(rnd() * 100000), 5)}`, String(10000000 + Math.floor(rnd() * 89999999))];
    case "articleNo":
      return [`${String.fromCharCode(65 + Math.floor(rnd() * 26))}${String.fromCharCode(65 + Math.floor(rnd() * 26))}-${1000 + Math.floor(rnd() * 9000)}-${pad(Math.floor(rnd() * 1000), 3)}`, String(100000 + Math.floor(rnd() * 900000))];
    case "orderId":
      return [`2026-${pad(1 + Math.floor(rnd() * 99999), 5)}`, String(4500000000 + Math.floor(rnd() * 99999999))];
    case "partyId": {
      // A reference (`BUYER_IDREF`) names a party the example already holds; an id is new, and remembered.
      const pool = ctx.state.pool;
      if (/idref$/i.test(ctx.name.replace(/[^A-Za-z]/g, "")) && pool.length > 0) return [pool[ctx.state.taken++ % pool.length]!];
      return [String(1000000 + Math.floor(rnd() * 8999999)), `P-${1000 + Math.floor(rnd() * 9000)}`];
    }
    case "version":
      return ["2.1", "1.0", "2005", "1.0.0"];
    case "date": {
      const d = dayOf(rnd, 20, lean(ctx.name));
      return [d, `${d}T00:00:00`];
    }
    case "dateTime": {
      const d = dayOf(rnd, 20, lean(ctx.name));
      return [`${d}T${clock(rnd)}+01:00`, `${d}T${clock(rnd)}`, d];
    }
    case "time":
      return [clock(rnd)];
    case "vatId":
      return [`DE${100000000 + Math.floor(rnd() * 899999999)}`];
    case "mime":
      return ["application/pdf", "image/jpeg", "url"];
    case "url":
      return [`https:/${"/"}example.com/catalog/${pad(Math.floor(rnd() * 10000), 4)}`];
    case "productName":
      return [pickOf(PRODUCTS[lang], rnd)];
    case "productText":
      return [PRODUCT_TEXT[lang](pickOf(PRODUCTS[lang], rnd))];
    case "featureName":
      return [pickOf(FEATURES[lang], rnd)[0]];
    case "featureValue":
      return [pickOf(FEATURES[lang], rnd)[1]];
    case "keyword":
      return [pickOf(KEYWORDS[lang], rnd)];
    case "text":
      return [pickOf(PHRASES[lang], rnd)];
    default:
      return [];
  }
}

const num = (v: string | undefined): number | undefined => (v === undefined ? undefined : Number(v));
const LEX: Record<string, RegExp> = {
  boolean: /^(true|false|1|0)$/,
  date: /^-?\d{4,}-\d\d-\d\d(Z|[+-]\d\d:\d\d)?$/,
  time: /^\d\d:\d\d:\d\d(\.\d+)?(Z|[+-]\d\d:\d\d)?$/,
  dateTime: /^-?\d{4,}-\d\d-\d\dT\d\d:\d\d:\d\d(\.\d+)?(Z|[+-]\d\d:\d\d)?$/,
  gYear: /^-?\d{4,}(Z|[+-]\d\d:\d\d)?$/,
  gYearMonth: /^-?\d{4,}-\d\d(Z|[+-]\d\d:\d\d)?$/,
  duration: /^-?P(?=.)(\d+Y)?(\d+M)?(\d+D)?(T(?=.)(\d+H)?(\d+M)?(\d+(\.\d+)?S)?)?$/,
  hexBinary: /^([0-9a-fA-F]{2})*$/,
  base64Binary: /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/,
  language: /^[a-zA-Z]{1,8}(-[a-zA-Z0-9]{1,8})*$/,
  ncname: /^[A-Za-z_][\w.-]*$/,
  id: /^[A-Za-z_][\w.-]*$/,
  idref: /^[A-Za-z_][\w.-]*$/,
  nmtoken: /^[\w.:-]+$/,
  name: /^[A-Za-z_:][\w.:-]*$/,
};

/**
 * Whether `value` is one the simple type takes: its enumeration, patterns, lengths, bounds, digits and its built-in's
 * written form. `true`; `null` where a pattern knobas does not read decides it (not judged); else what failed, in words.
 */
export function fits(sh: SimpleShape, value: string): true | null | string {
  const f = sh.facets;
  if (f.enumeration !== undefined && !f.enumeration.some((e) => e.v === value)) return "it is not one of the allowed values";
  for (const step of sh.patterns) {
    let any = false;
    for (const src of step) {
      const r = rx(src);
      if (!r.ok) return null;
      if (rxTest(r.re, value) !== false) any = true;
    }
    if (!any) return "it does not match the pattern";
  }
  const len = sh.prim === "hexBinary" ? value.length / 2 : Array.from(value).length;
  if (f.length !== undefined && len !== Number(f.length)) return `it is not ${f.length} long`;
  if (f.minLength !== undefined && len < Number(f.minLength)) return `it is shorter than ${f.minLength}`;
  if (f.maxLength !== undefined && len > Number(f.maxLength)) return `it is longer than ${f.maxLength}`;
  if (sh.prim === "decimal" || sh.prim === "integer" || sh.prim === "float") {
    if (!/^[+-]?(\d+(\.\d*)?|\.\d+)([eE][+-]?\d+)?$/.test(value) || (sh.prim !== "float" && /[eE]/.test(value))) return "it is not a number";
    if (sh.prim === "integer" && !/^[+-]?\d+$/.test(value)) return "it is not a whole number";
    const v = Number(value);
    const lo = Math.max(num(f.minInclusive) ?? -Infinity, sh.lim.min ?? -Infinity);
    const hi = Math.min(num(f.maxInclusive) ?? Infinity, sh.lim.max ?? Infinity);
    if (v < lo) return `it is under ${lo}`;
    if (v > hi) return `it is over ${hi}`;
    if (f.minExclusive !== undefined && v <= Number(f.minExclusive)) return `it is not over ${f.minExclusive}`;
    if (f.maxExclusive !== undefined && v >= Number(f.maxExclusive)) return `it is not under ${f.maxExclusive}`;
    const digits = value.replace(/^[+-]/, "").replace(/^0+(?=\d)/, "");
    const frac = digits.includes(".") ? digits.split(".")[1]!.replace(/0+$/, "").length : 0;
    if (f.fractionDigits !== undefined && frac > Number(f.fractionDigits)) return `it has more than ${f.fractionDigits} decimal places`;
    if (f.totalDigits !== undefined && digits.replace(".", "").replace(/0+$/, frac > 0 ? "" : "$&").length > Number(f.totalDigits)) return `it has more than ${f.totalDigits} digits`;
  }
  const lex = LEX[sh.prim];
  if (lex !== undefined && !lex.test(value)) return `it is not written as ${sh.builtin !== undefined ? `xs:${sh.builtin}` : "its type"} is`;
  return true;
}

/** The built-in type with its bounds: lengths, ranges, digits. */
function fromPrim(sh: SimpleShape, kind: TradeKind | null, ctx: ValueCtx, rnd: () => number): string {
  const f = sh.facets;
  const minLen = Number(f.length ?? f.minLength ?? 0);
  const maxLen = Number(f.length ?? f.maxLength ?? Infinity);
  const clip = (s: string) => {
    let out = s;
    if (Array.from(out).length > maxLen) {
      out = Array.from(out).slice(0, maxLen).join("");
      // Cut at a word's end where that leaves enough.
      const sp = out.lastIndexOf(" ");
      if (sp >= Math.max(minLen, Math.floor(maxLen * 0.6))) out = out.slice(0, sp);
    }
    while (Array.from(out).length < minLen) out += "x";
    return out;
  };
  switch (sh.prim) {
    case "boolean":
      return "true";
    case "integer":
    case "decimal":
    case "float": {
      let lo = Math.max(num(f.minInclusive) ?? -Infinity, sh.lim.min ?? -Infinity, f.minExclusive !== undefined ? Number(f.minExclusive) + (sh.prim === "integer" ? 1 : 0.01) : -Infinity);
      let hi = Math.min(num(f.maxInclusive) ?? Infinity, sh.lim.max ?? Infinity, f.maxExclusive !== undefined ? Number(f.maxExclusive) - (sh.prim === "integer" ? 1 : 0.01) : Infinity);
      if (f.totalDigits !== undefined) hi = Math.min(hi, 10 ** Math.max(1, Number(f.totalDigits) - Number(f.fractionDigits ?? (sh.prim === "integer" ? 0 : 2))) - 1);
      if (lo === -Infinity) lo = hi === Infinity ? 1 : Math.min(1, hi);
      if (hi === Infinity || hi > lo + 500) hi = lo + (kind === "quantity" || kind === "count" ? 24 : 500);
      const v = lo + rnd() * (hi - lo);
      if (sh.prim === "integer") return String(Math.round(Math.min(hi, Math.max(lo, Math.round(v)))));
      const places = Math.min(Number(f.fractionDigits ?? 2), 2);
      return Math.min(hi, Math.max(lo, v)).toFixed(places);
    }
    case "date":
      return dayOf(rnd, 60);
    case "dateTime":
      return `${dayOf(rnd, 60)}T${clock(rnd)}Z`;
    case "time":
      return clock(rnd);
    case "gYear":
      return "2026";
    case "gYearMonth":
      return "2026-11";
    case "gMonthDay":
      return "--11-23";
    case "gDay":
      return "---23";
    case "gMonth":
      return "--11";
    case "duration":
      return "P30D";
    case "hexBinary":
      return "0FB7".repeat(Math.max(1, Math.ceil(minLen / 2))).slice(0, Math.max(2, Math.min(maxLen === Infinity ? 4 : maxLen * 2, 64)));
    case "base64Binary":
      return "ZXhhbXBsZQ==";
    case "anyURI":
      return clip(`https:/${"/"}example.com/${ctx.name.toLowerCase().replace(/[^a-z0-9]+/g, "-") || "resource"}`);
    case "language":
      return ctx.lang;
    case "id":
      return clip(`id-${++ctx.state.ids}`);
    case "idref":
    case "idrefs":
      if (ctx.state.ids === 0) ctx.state.dangling = true;
      return ctx.state.ids > 0 && sh.prim === "idref" ? `id-${1 + Math.floor(rnd() * ctx.state.ids)}` : "id-1";
    case "qname":
      return "xs:string";
    case "ncname":
    case "name":
    case "nmtoken":
    case "nmtokens":
      return clip(ctx.name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^[^a-z]+/, "") || "value");
    default: {
      // A string nobody's name explains: its own name in words, so the example still reads.
      const said = wordsOf(ctx.name).join(" ");
      return clip(said === "" ? "value" : said.charAt(0).toUpperCase() + said.slice(1));
    }
  }
}

/** What made a value: the rule of the order it came from. */
export type ValueFrom = "enumeration" | "name" | "type" | "pattern" | "list" | "unknown";

/** A value for simple type `def` standing where `ctx` says. `bad` says why a value made from the type alone may not fit. */
export function simpleValue(set: SchemaSet, def: ComplexDecl | SimpleDecl | null, ctx: ValueCtx, rnd: () => number, depth = 0): { v: string; from: ValueFrom; bad?: string } {
  if (def === null || depth > 8) return { v: "", from: "unknown" };
  const sh = simpleShape(set, def);
  if (sh.variety === "list") {
    const item = sh.item === null ? null : simpleValue(set, sh.item, ctx, rnd, depth + 1);
    const n = Math.max(1, Number(sh.facets.length ?? sh.facets.minLength ?? 1));
    return { v: Array.from({ length: Math.min(n, 4) }, () => (item !== null ? item.v : "value")).join(" "), from: "list" };
  }
  if (sh.variety === "union") {
    for (const m of sh.members) {
      const got = simpleValue(set, m, ctx, rnd, depth + 1);
      if (got.from !== "unknown") return got;
    }
    return { v: "", from: "unknown" };
  }
  const kind = ctx.kind !== undefined ? ctx.kind : tradeKindOf(ctx.name);
  const en = sh.facets.enumeration;
  if (en !== undefined && en.length > 0) {
    const wanted = (kind !== null ? EXPECTED[kind] : undefined) ?? (sh.prim === "boolean" ? ["true"] : null);
    const order = wanted === null ? null : kind === "language" && ctx.lang !== "de" ? ["eng", "en", ...wanted] : wanted;
    const liked = order === null ? undefined : order.find((w) => en.some((e) => e.v === w));
    if (liked !== undefined) return { v: liked, from: "enumeration" };
    return { v: en[Math.floor(rnd() * Math.min(en.length, 40))]!.v, from: "enumeration" };
  }
  // What the name asks for, each kept only if the type takes it. Bytes are bytes whatever their name says.
  const tries = kind === null || sh.prim === "base64Binary" || sh.prim === "hexBinary" ? [] : candidates(kind, ctx, rnd);
  for (const c of tries) {
    if (fits(sh, c) !== true) continue;
    // A party's own id is what a reference (`BUYER_IDREF`) names; a supplier's or a buyer's number is not one.
    if (kind === "partyId" && /partyid$/i.test(ctx.name.replace(/[^A-Za-z]/g, "")) && !ctx.state.pool.includes(c)) ctx.state.pool.push(c);
    return { v: c, from: "name" };
  }
  const typed: Record<string, string[]> = { boolean: ["true", "false"], date: [dayOf(rnd, 20, lean(ctx.name))], dateTime: [`${dayOf(rnd, 20, lean(ctx.name))}T${clock(rnd)}+01:00`], time: [clock(rnd)] };
  for (const c of typed[sh.prim] ?? []) if (fits(sh, c) === true) return { v: c, from: "type" };
  const own = sh.patterns[0];
  if (own !== undefined) {
    const parsed = own.map((src) => rx(src)).flatMap((r) => (r.ok ? [r.re] : []));
    // A pattern that offers plain words is answered with one of them; one that offers none is first asked whether a
    // value a person would write fits it (a pattern that spells out a date takes a date, one for booleans `true`).
    if (!parsed.some((re) => rxHasWords(re))) {
      for (const c of ["true", dayOf(rnd, 20, lean(ctx.name)), ctx.lang === "de" ? "DE" : "GB", clock(rnd), "1"]) if (fits(sh, c) === true) return { v: c, from: "pattern" };
    }
    const holder = ctx.holder !== undefined ? wordsOf(ctx.holder) : [];
    const holderKind = ctx.holder !== undefined ? tradeKindOf(ctx.holder) : null;
    const prefer = holder.length === 0 ? undefined : (word: string) => word.toLowerCase() === String(holderKind).toLowerCase() || holder.some((h) => h.length > 2 && word.toLowerCase().split(/[^a-z0-9]+/).includes(h));
    for (const re of parsed) {
      for (let t = 0; t < 6; t += 1) {
        const made = rxMake(re, rnd, prefer);
        if (made !== null && fits(sh, made) === true) return { v: made, from: "pattern" };
      }
    }
    ctx.state.unmade.push({ name: ctx.name, pattern: own[0] ?? "" });
  }
  const v = fromPrim(sh, kind, ctx, rnd);
  const said = fits(sh, v);
  return said === true || said === null ? { v, from: "type" } : { v, from: "type", bad: said };
}

/** The seed of a value: the example's seed and the path it stands at (`seedOf`). */
export const valueSeed = (seed: number, path: string): number => seedOf(`${seed}\n${path}`);
