/**
 * An XML media type's example as XML (#2525): where a body or an answer is `application/xml`, `text/xml`, a `+xml`
 * type (`application/atom+xml`) or `application/xml+…`, the explorer shows and sends XML, not JSON. The value is the
 * one `openapi-example.ts` makes (or the spec's own, where it gives one that is not already text); this writes it out
 * along its schema, reading the schema's `xml` object as OpenAPI says:
 *
 * - **`name`** renames the element (or the attribute). Where there is none, a property is named for its property, a
 *   named schema at the top for its schema (`#/components/schemas/Pet` is `<Pet>`), and anything else at the top is
 *   `<root>`; an item of a list at the top is `<item>`.
 * - **`attribute: true`** makes a property with a plain value (text, a number, true or false) an attribute of its
 *   object's element.
 * - **`wrapped: true`** puts a list's items in one element named for the list (its `xml.name`, else its property);
 *   unwrapped, the items stand in the object one after another. An item is named by its items schema's `xml.name`,
 *   else (wrapped) the list's `xml.name`, else the property's name.
 * - **`prefix`** writes the name as `prefix:name`; **`namespace`** declares it on that element (`xmlns:prefix="…"`,
 *   or `xmlns="…"` without a prefix), once: an element below one that declared it does not again.
 *
 * Names are kept to what XML allows (anything else an underscore), text and attribute values escaped. Deterministic:
 * the same schema and value always give the same text.
 */
import type { Reader } from "./openapi-example";
import { xmlAttr, xmlText as escText } from "./xml-escape";

type Obj = Record<string, unknown>;
const isObj = (v: unknown): v is Obj => typeof v === "object" && v !== null && !Array.isArray(v);

/** Whether media type `type` is XML: `application/xml`, `text/xml`, `…/…+xml` or `application/xml+…`. */
export function isXmlMedia(type: string): boolean {
  const t = type.split(";")[0]!.trim().toLowerCase();
  return /^(application|text)\/xml(\+[\w.-]+)?$/.test(t) || /^[\w.-]+\/[\w.-]+\+xml$/.test(t);
}

/** What a schema's `xml` object says. */
interface XmlMeta {
  name: string;
  attribute: boolean;
  wrapped: boolean;
  namespace: string;
  prefix: string;
}

/** A schema followed through its refs (and the parts of an `allOf`): what it is, where, and the xml it says. */
interface Seen {
  node: unknown;
  base: string;
  /** The last ref's schema name, where the schema came by a ref: what names it at the top. */
  named: string;
  xml: XmlMeta;
}

const NONE: XmlMeta = { name: "", attribute: false, wrapped: false, namespace: "", prefix: "" };

/** The first `xml` object along `node`: its own, then its ref's, then its `allOf` parts'; each field the first given. */
function follow(read: Reader, base: string, node: unknown): Seen {
  const xml: XmlMeta = { ...NONE };
  const got = new Set<keyof XmlMeta>();
  let named = "";
  let at = base;
  let n = node;
  let end: unknown = node;
  let endBase = base;
  const seen = new Set<string>();
  const take = (x: unknown) => {
    if (!isObj(x)) return;
    for (const k of ["name", "namespace", "prefix"] as const) if (!got.has(k) && typeof x[k] === "string") (xml[k] = x[k] as string), got.add(k);
    for (const k of ["attribute", "wrapped"] as const) if (!got.has(k) && typeof x[k] === "boolean") (xml[k] = x[k] as boolean), got.add(k);
  };
  for (let depth = 0; depth < 12 && isObj(n); depth++) {
    take(n.xml);
    // An allOf's parts say the xml of what they make: a part by ref, its schema's.
    if (Array.isArray(n.allOf)) {
      for (const part of n.allOf) {
        const p = isObj(part) && typeof part.$ref === "string" ? read.resolve(at, part.$ref)?.node : part;
        if (isObj(p)) take(p.xml);
      }
    }
    if (typeof n.$ref !== "string" || seen.has(`${at}#${n.$ref}`)) break;
    seen.add(`${at}#${n.$ref}`);
    if (named === "") named = refName(n.$ref);
    const r = read.resolve(at, n.$ref);
    if (r === null) break;
    n = r.node;
    at = r.base;
    end = n;
    endBase = at;
  }
  return { node: end, base: endBase, named, xml };
}

/** A ref's last part, decoded: `#/components/schemas/Pet` is `Pet`. */
function refName(ref: string): string {
  const last = ref.slice(ref.lastIndexOf("/") + 1);
  try {
    return decodeURIComponent(last).replace(/~1/g, "/").replace(/~0/g, "~");
  } catch {
    return last;
  }
}

/** `name` as XML allows it: what a name may not hold an underscore, and an underscore before what may not start one. */
export function xmlName(name: string): string {
  const safe = name.replace(/[^\p{L}\p{N}_.-]/gu, "_");
  if (safe === "") return "_";
  return /^[\p{L}_]/u.test(safe) ? safe : `_${safe}`;
}

/** A plain value's text: a string as it is, a number or a boolean as written. */
const plain = (v: unknown): v is string | number | boolean => typeof v === "string" || typeof v === "number" || typeof v === "boolean";

/** The XML declaration every example starts with. */
const PROLOG = '<?xml version="1.0" encoding="UTF-8"?>';
/** How deep the writer goes before it stops: a value deeper than this is left out. */
const DEPTH = 24;

/**
 * Value `value` written as XML along schema `node` of file `base` (see the module's words); `choice` is the top
 * `oneOf`'s choice it was made from. A string value that starts as XML is the spec's own text and is kept as it is.
 */
export function xmlText(read: Reader, base: string, node: unknown, value: unknown, choice = 0): string {
  if (typeof value === "string" && value.trimStart().startsWith("<")) return value;
  let top = follow(read, base, node);
  // A choice at the top with no name of its own is the choice the value was made from, named as it is.
  if (top.xml.name === "" && top.named === "" && isObj(top.node)) {
    const sh = read.shape(top.base, top.node);
    if (sh.variants !== null && sh.props.length === 0) top = follow(read, sh.base, sh.variants.list[Math.min(choice, sh.variants.list.length - 1)]);
  }
  const lines: string[] = [PROLOG];
  const w = new Writer(read, lines);
  const name = top.xml.name || top.named || "root";
  if (Array.isArray(value)) {
    // A list at the top is one element holding its items, whether or not the schema says wrapped.
    const sh = isObj(top.node) ? read.shape(top.base, top.node) : null;
    const item = follow(read, sh?.base ?? top.base, sh?.items);
    const itemName = item.xml.name || item.named || "item";
    const ns = w.open(top.xml, name, [], 0, new Map(), value.length === 0);
    for (const v of value) w.element(item, itemName, v, 1, ns);
    if (value.length > 0) w.close(top.xml, name, 0);
  } else w.element(top, name, value, 0, new Map());
  return lines.join("\n");
}

class Writer {
  constructor(
    private readonly read: Reader,
    private readonly lines: string[],
  ) {}

  /** Element `name` of schema `s` holding `value`, at `depth`, below the namespaces `ns` already declares. */
  element(s: Seen, name: string, value: unknown, depth: number, ns: ReadonlyMap<string, string>): void {
    if (depth > DEPTH) return;
    const pad = "  ".repeat(depth);
    if (value === null || value === undefined) {
      this.open(s.xml, name, [], depth, ns, true);
      return;
    }
    if (plain(value)) {
      const tag = qname(s.xml, name);
      this.lines.push(`${pad}<${tag}${declare(s.xml, ns, new Map(ns))}>${escText(String(value))}</${tag}>`);
      return;
    }
    if (Array.isArray(value)) {
      // A list with no property to stand in (a list of lists): its items named as it is.
      const sh = isObj(s.node) ? this.read.shape(s.base, s.node) : null;
      const item = follow(this.read, sh?.base ?? s.base, sh?.items);
      for (const v of value) this.element(item, item.xml.name || name, v, depth, ns);
      return;
    }
    if (!isObj(value)) return;
    const sh = isObj(s.node) ? this.read.shape(s.base, s.node) : null;
    const props = sh === null ? [] : sh.props.concat(this.choiceProps(sh));
    const attrs: [string, string, XmlMeta][] = [];
    const kids: [string, unknown][] = [];
    for (const [key, v] of Object.entries(value)) {
      const p = props.find((x) => x.name === key);
      const meta = p === undefined ? NONE : follow(this.read, sh!.base, p.node).xml;
      if (meta.attribute && plain(v)) attrs.push([qname(meta, key), String(v), meta]);
      else kids.push([key, v]);
    }
    const inner = this.open(s.xml, name, attrs, depth, ns, kids.length === 0);
    if (kids.length === 0) return;
    for (const [key, v] of kids) {
      const p = props.find((x) => x.name === key);
      const ps = p === undefined ? { node: undefined, base: s.base, named: "", xml: NONE } : follow(this.read, sh!.base, p.node);
      const own = ps.xml.name || key;
      if (Array.isArray(v)) this.list(ps, key, v, depth + 1, inner);
      else this.element(ps, own, v, depth + 1, inner);
    }
    this.close(s.xml, name, depth);
  }

  /** A property's list `value`: its items one after another, or (wrapped) in one element named for the list. */
  private list(s: Seen, key: string, value: readonly unknown[], depth: number, ns: ReadonlyMap<string, string>): void {
    const sh = isObj(s.node) ? this.read.shape(s.base, s.node) : null;
    const item = follow(this.read, sh?.base ?? s.base, sh?.items);
    if (s.xml.wrapped) {
      const wrap = s.xml.name || key;
      const inner = this.open(s.xml, wrap, [], depth, ns, value.length === 0);
      for (const v of value) this.element(item, item.xml.name || wrap, v, depth + 1, inner);
      if (value.length > 0) this.close(s.xml, wrap, depth);
      return;
    }
    for (const v of value) this.element(item, item.xml.name || key, v, depth, ns);
  }

  /** The properties a choice's schemas add, for a value made from one of them. */
  private choiceProps(sh: { variants: { list: unknown[] } | null; base: string }) {
    if (sh.variants === null) return [];
    return sh.variants.list.flatMap((m) => this.read.shape(sh.base, m).props);
  }

  /** Writes element `name`'s start (or the whole element where it is empty); the namespaces declared inside it. */
  open(xml: XmlMeta, name: string, attrs: readonly [string, string, XmlMeta][], depth: number, ns: ReadonlyMap<string, string>, empty: boolean): Map<string, string> {
    const inner = new Map(ns);
    let decl = declare(xml, ns, inner);
    for (const [, , meta] of attrs) if (meta.prefix !== "" && meta.namespace !== "") decl += declare(meta, inner, inner);
    const at = attrs.map(([n, v]) => ` ${n}="${xmlAttr(v)}"`).join("");
    this.lines.push(`${"  ".repeat(depth)}<${qname(xml, name)}${decl}${at}${empty ? "/>" : ">"}`);
    return inner;
  }

  close(xml: XmlMeta, name: string, depth: number): void {
    this.lines.push(`${"  ".repeat(depth)}</${qname(xml, name)}>`);
  }
}

/** The element's or attribute's name: its `xml.prefix` before it where given. */
function qname(xml: XmlMeta, name: string): string {
  const local = xmlName(name);
  return xml.prefix === "" ? local : `${xmlName(xml.prefix)}:${local}`;
}

/** The `xmlns` declaration `xml` asks for, where `ns` does not already declare it; noted in `into`. */
function declare(xml: XmlMeta, ns: ReadonlyMap<string, string>, into: Map<string, string>): string {
  if (xml.namespace === "") return "";
  const key = xml.prefix === "" ? "" : xmlName(xml.prefix);
  if (ns.get(key) === xml.namespace) return "";
  into.set(key, xml.namespace);
  return key === "" ? ` xmlns="${xmlAttr(xml.namespace)}"` : ` xmlns:${key}="${xmlAttr(xml.namespace)}"`;
}
