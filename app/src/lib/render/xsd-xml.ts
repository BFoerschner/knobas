/**
 * A schema file read as the schema's code reads it (the XSD design, 2026-11-23, section 6): a tree of plain nodes made
 * from the XML view's outline (`xml-outline.ts`), the one XML reader knobas has. The outline reads a document into
 * columns of numbers; a schema's compiler wants `node.c`, `attr(node, "name")` and the namespaces in scope, so this
 * walks the outline once and hands those.
 *
 * - **Only the internal subset's general entities are read**, under a budget of {@link ENTITY_BUDGET} of expanded text
 *   a file: xmldsig's schema starts with a DOCTYPE that declares `&dsig;`. The external subset and external entities
 *   are never read; an entity that names one is left as written and said in `problems`.
 * - **Blank text between elements is dropped**, as the outline drops it; the words an annotation holds are kept.
 * - **`head`** is the file's first comment before its root: for BMEcat and openTRANS the only prose they have.
 */
import { CDATA, COMMENT, EL, TEXT, decode, readXml, type XmlOutline } from "./xml-outline";

export const XS = "http://www.w3.org/2001/XMLSchema";
export const XSI = "http://www.w3.org/2001/XMLSchema-instance";
export const XML_NS = "http://www.w3.org/XML/1998/namespace";

/** How much entity text one file may expand to: a file that asks for more is refused (the billion laughs). */
export const ENTITY_BUDGET = 1 << 20;

export interface XAttr {
  /** As written: `xml:lang`. */
  readonly q: string;
  readonly l: string;
  readonly ns: string;
  readonly v: string;
}

export interface XNode {
  /** The name as written, its prefix, its local name and its namespace. */
  readonly q: string;
  readonly p: string;
  readonly l: string;
  readonly ns: string;
  readonly a: readonly XAttr[];
  readonly c: readonly XNode[];
  /** The text it holds directly, decoded; `""` for none. */
  readonly t: string;
  /** The line its start tag stands on (from 1). */
  readonly line: number;
  /** The prefixes in scope: `""` is the default namespace. */
  readonly nsmap: Readonly<Record<string, string>>;
}

export interface SchemaTree {
  readonly root: XNode;
  readonly head: string | null;
  readonly lines: number;
  readonly problems: readonly { line: number; text: string }[];
}

export class NotRead extends Error {
  readonly line: number | null;
  constructor(message: string, line: number | null) {
    super(message);
    this.line = line;
  }
}

/** The general entities the DOCTYPE's internal subset declares, by name. Parameter entities are not read. */
export function internalEntities(text: string): Map<string, string> {
  const out = new Map<string, string>();
  const at = text.indexOf("<!DOCTYPE");
  if (at === -1) return out;
  const open = text.indexOf("[", at);
  const gt = text.indexOf(">", at);
  if (open === -1 || (gt !== -1 && gt < open)) return out;
  const close = text.indexOf("]>", open);
  const body = text.slice(open + 1, close === -1 ? text.length : close);
  const ENT = /<!ENTITY\s+([^\s%]+)\s+(?:"([^"]*)"|'([^']*)')\s*>/g;
  for (let m = ENT.exec(body); m !== null; m = ENT.exec(body)) out.set(m[1]!, m[2] ?? m[3] ?? "");
  return out;
}

/** Reads a schema file's text into a tree. Throws {@link NotRead} where it is not XML. */
export function readSchemaXml(text: string): SchemaTree {
  const o = readXml(text);
  if (o.error !== null && o.root === -1) throw new NotRead(o.error.message, o.error.line);
  if (o.error !== null) throw new NotRead(o.error.message, o.error.line);
  if (o.root === -1) throw new NotRead("The file holds no element.", null);
  return treeOf(o, text);
}

/** The tree of an outline already read. */
export function treeOf(o: XmlOutline, text: string = o.text): SchemaTree {
  const entities = internalEntities(text);
  const problems: { line: number; text: string }[] = [];
  let spent = 0;
  const read = (raw: string, line: number): string => {
    if (raw.indexOf("&") === -1) return raw;
    if (entities.size === 0) return decode(raw);
    const expand = (s: string, depth: number): string =>
      s.replace(/&(#x[0-9a-fA-F]+|#[0-9]+|[^;&\s]+);/g, (whole, name: string) => {
        if (name[0] === "#") return decode(whole);
        const got = entities.get(name);
        if (got === undefined) return decode(whole);
        spent += got.length;
        if (spent > ENTITY_BUDGET || depth > 16) throw new NotRead("The file's entities expand past 1 MiB.", line);
        return expand(got, depth + 1);
      });
    const out = expand(raw, 0);
    for (const m of out.matchAll(/&([A-Za-z_][\w.-]*);/g)) {
      if (!["lt", "gt", "amp", "quot", "apos"].includes(m[1]!)) problems.push({ line, text: `The entity &${m[1]}; is not declared in this file.` });
    }
    return out;
  };

  const maps = new Map<number, Readonly<Record<string, string>>>();
  /** A scope's prefixes as one plain object, made once a scope. */
  const mapOf = (k: number): Readonly<Record<string, string>> => {
    let got = maps.get(k);
    if (got === undefined) {
      const scope = k >= 0 ? o.scopes[k]! : {};
      // A scope inherits its parent's prefixes through its prototype: `for … in` reads those too, a spread would not.
      const made: Record<string, string> = { xml: XML_NS };
      for (const p in scope) made[p] = scope[p]!;
      maps.set(k, (got = made));
    }
    return got;
  };
  const node = (id: number): XNode => {
    const nsmap = mapOf(o.scope[id]!);
    const attrs: XAttr[] = [];
    for (let a = o.a0[id]!, e = a + o.an[id]!; a < e; a += 1) {
      const q = o.names[o.aName[a]!]!;
      const colon = q.indexOf(":");
      const l = colon === -1 || q.startsWith("xmlns") ? q : q.slice(colon + 1);
      attrs.push({ q, l, ns: o.uris[o.aNs[a]!] ?? "", v: read(text.slice(o.aStart[a], o.aEnd[a]), o.line[id]!) });
    }
    const kids: XNode[] = [];
    let words = "";
    for (let c = o.first[id]!; c !== -1; c = o.next[c]!) {
      const t = o.type[c];
      if (t === EL) kids.push(node(c));
      else if (t === TEXT) words += read(text.slice(o.open[c], o.close[c]), o.line[c]!);
      else if (t === CDATA) words += text.slice(o.open[c], o.close[c]);
    }
    const q = o.names[o.name[id]!]!;
    const colon = q.indexOf(":");
    return {
      q,
      p: colon === -1 ? "" : q.slice(0, colon),
      l: o.names[o.local[id]!]!,
      ns: o.uris[o.ns[id]!] ?? "",
      a: attrs,
      c: kids,
      t: words,
      line: o.line[id]!,
      nsmap,
    };
  };

  let head: string | null = null;
  for (let c = o.first[0]!; c !== -1 && c !== o.root; c = o.next[c]!) {
    if (o.type[c] === COMMENT) {
      head = text.slice(o.open[c], o.close[c]);
      break;
    }
  }
  return { root: node(o.root), head, lines: o.lines, problems };
}

/** An attribute's value as written on `node`, or `null`. */
export function attr(node: XNode, name: string): string | null {
  for (const a of node.a) if (a.q === name) return a.v;
  return null;
}
/** `node`'s XML Schema children of local name `local` (or all). */
export const kids = (node: XNode, local?: string): XNode[] => node.c.filter((c) => c.ns === XS && (local === undefined || c.l === local));
/** `node`'s first XML Schema child of local name `local`. */
export const kid = (node: XNode, local: string): XNode | null => node.c.find((c) => c.ns === XS && c.l === local) ?? null;

/** `{ns}local` for a QName as written in an attribute's value, read with the node's namespaces. */
export function qkey(node: XNode, qname: string, chameleon: string | null = null): string {
  const c = qname.indexOf(":");
  if (c === -1) return `{${node.nsmap[""] || chameleon || ""}}${qname}`;
  const ns = node.nsmap[qname.slice(0, c)];
  return `{${ns === undefined ? "?" + qname.slice(0, c) : ns}}${qname.slice(c + 1)}`;
}
export const localOf = (key: string): string => key.slice(key.indexOf("}") + 1);
export const nsOf = (key: string): string => key.slice(1, key.indexOf("}"));
