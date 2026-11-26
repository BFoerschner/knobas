/**
 * The witness (the XML view's design, 2026-11-23, section 20): knobas's XPath reader and libxml2 asked the same
 * questions of the fixture order, in Node. knobas runs `strict`, XPath's own rule for a name with no prefix; libxml2
 * reads with `XML_PARSE_NOBLANKS`, as the outline drops blank text between elements. Of the questions, exactly three
 * answer differently, and each is libxml2 leaving XPath 1.0 (`xml-xpath.test.ts` pins the specification's answer).
 */
import { XSI_NS, ns } from "./xml-names";
import { XmlDocument, XmlElement } from "libxml2-wasm";
import { expect, test } from "vitest";

import { fixture } from "./doc-fixture";
import { CDATA, COMMENT, DOC, EL, PI, TEXT, lineOfNode, readXml, stringOf } from "./xml-outline";
import { XPathError, compile, run } from "./xml-xpath";

const TEXT_ = fixture("order-4711.xml");
const doc = readXml(TEXT_);
const NS = { ot: ns("www.opentrans.org/XMLSchema/2.1"), bmecat: ns("www.bmecat.org/bmecat/2005"), xsi: XSI_NS };
const prefixes = new Map(Object.entries(NS));

type Said = { err: true } | { v: unknown } | { n: string[] };

/** knobas's answer, as a list of `name|value|line` or a value. */
function mine(x: string): Said {
  try {
    const r = run(doc, compile(x), { strict: true, prefixes, budget: 5e7, context: doc.root });
    if (r.type !== "nodes") return { v: r.type === "number" ? (Number.isNaN(r.value) ? "NaN" : r.value) : r.value };
    return {
      n: r.ids.map((id) => {
        const t = id >= doc.n ? -1 : doc.type[id];
        const name = id >= doc.n ? "@" + doc.names[doc.aLocal[id - doc.n]!] : t === EL ? doc.names[doc.local[id]!] : t === DOC ? "#doc" : t === CDATA ? "#3" : "#" + t;
        const value = t === DOC ? "" : id >= doc.n || t !== EL || doc.kids[id] === 0 ? stringOf(doc, id).slice(0, 40) : "";
        return `${name}|${value}|${t === DOC ? 1 : Math.min(65535, lineOfNode(doc, id))}`;
      }),
    };
  } catch (cause) {
    if (cause instanceof XPathError) return { err: true };
    throw cause;
  }
}

/** libxml2's answer, in the same form. */
function theirs(ldoc: XmlDocument, x: string): Said {
  let r: unknown;
  const said = console.error;
  console.error = () => {};
  try {
    r = (ldoc as unknown as { eval(x: string, ns: Record<string, string>): unknown }).eval(x, NS);
  } catch {
    return { err: true };
  } finally {
    console.error = said;
  }
  if (typeof r === "number") return { v: Number.isNaN(r) ? "NaN" : r };
  if (typeof r !== "object" || r === null) return { v: r };
  const list = (Array.isArray(r) ? r : [r]) as Record<string, unknown>[];
  return {
    n: list.map((node) => {
      const get = <T,>(k: string, or: T): T => {
        try {
          return (node[k] as T) ?? or;
        } catch {
          return or;
        }
      };
      if (node instanceof XmlElement) {
        let leaf = true;
        for (let c = (node as unknown as { firstChild: unknown }).firstChild as Record<string, unknown> | null; c; c = c["next"] as Record<string, unknown> | null)
          if (c instanceof XmlElement) {
            leaf = false;
            break;
          }
        const name = String(get("name", "")).replace(/^.*:/, "");
        return `${name}|${leaf ? String(get("content", "")).slice(0, 40) : ""}|${get("line", 0)}`;
      }
      const name = String(get("name", ""));
      const content = String(get("content", "")).slice(0, 40);
      const parent = get<unknown>("parent", null);
      if (parent === null && name === "") return "#doc||1";
      // An attribute has a name; text, a comment and an instruction are told by what they hold.
      return `?${name}|${content}`;
    }),
  };
}

function same(a: Said, b: Said): boolean {
  if ("err" in a || "err" in b) return "err" in a && "err" in b;
  if ("v" in a || "v" in b) {
    if (!("v" in a) || !("v" in b)) return false;
    if (a.v === b.v) return true;
    return typeof a.v === "number" && typeof b.v === "number" && Math.abs(a.v - b.v) < 1e-9 * Math.max(1, Math.abs(a.v));
  }
  if (a.n.length !== b.n.length) return false;
  for (let i = 0; i < a.n.length; i += 1) {
    const x = a.n[i]!;
    const y = b.n[i]!;
    if (!y.startsWith("?")) {
      if (x !== y) return false;
      continue;
    }
    const [name, value] = x.split("|");
    const [lname, lvalue] = y.slice(1).split("|");
    if (name!.startsWith("@")) {
      if (lname!.replace(/^.*:/, "") !== name!.slice(1) || lvalue !== value) return false;
    } else if (name === `#${TEXT}` || name === `#${COMMENT}` || name === `#${PI}` || name === "#3") {
      if (lvalue !== value) return false;
    } else return false;
  }
  return true;
}

const els = [...new Set(Array.from({ length: doc.n }, (_, i) => (doc.type[i] === EL ? (doc.ns[i] === doc.uris.indexOf(NS.bmecat) ? "bmecat:" : "ot:") + doc.names[doc.local[i]!] : null)).filter((x): x is string => x !== null))];
const attrs = [...new Set(Array.from({ length: doc.attrs }, (_, a) => doc.names[doc.aName[a]!]!).filter((n) => !n.startsWith("xmlns") && !n.includes(":")))];
const hand = [
  "/", "/*", "/ot:ORDER", "//ot:ORDER_ITEM", "count(//ot:ORDER_ITEM)", "//ot:ORDER_ITEM[1]", "//ot:ORDER_ITEM[last()]", "//ot:ORDER_ITEM[position() = 2]", "(//ot:ORDER_ITEM)[1]", "(//ot:ORDER_ITEM)[last()]",
  "//ot:ORDER_ITEM/ot:LINE_ITEM_ID", "//ot:ORDER_ITEM[ot:QUANTITY > 10]", "//ot:ORDER_ITEM[ot:QUANTITY > 10]/ot:LINE_ITEM_ID", "//ot:QUANTITY", "sum(//ot:QUANTITY)", "//bmecat:SUPPLIER_PID", "//bmecat:SUPPLIER_PID[@type]", "//bmecat:SUPPLIER_PID/@type",
  "//@type", "//@*", "count(//@*)", "//*[@lang='deu']", "//*[@lang][1]", "//@lang[. = 'deu']", "/ot:ORDER/@version", "/ot:ORDER/@*", "string(/ot:ORDER/@version)", "//ot:ORDER_ID", "string(//ot:ORDER_ID)", "//ot:ORDER_INFO/*[1]", "//ot:ORDER_INFO/*[2]",
  "//ot:PARTY[ot:PARTY_ROLE = 'buyer']", "//ot:PARTY[ot:PARTY_ROLE = 'buyer']//bmecat:NAME", "//ot:PARTY/ot:PARTY_ROLE", "//bmecat:CITY | //bmecat:ZIP", "//bmecat:ZIP/preceding-sibling::*", "//bmecat:ZIP/following-sibling::*[1]", "//bmecat:ZIP/preceding-sibling::*[1]", "//bmecat:ZIP/..",
  "//bmecat:ZIP/ancestor::*", "//bmecat:ZIP/ancestor::*[1]", "//bmecat:ZIP/ancestor::*[last()]", "//bmecat:ZIP/ancestor-or-self::*[2]", "//ot:ORDER_ITEM[2]/following::ot:LINE_ITEM_ID", "//ot:ORDER_ITEM[2]/preceding::ot:LINE_ITEM_ID", "//ot:ORDER_ITEM[2]/preceding::*[1]", "//ot:ORDER_ITEM[1]/following::*[1]",
  "//ot:ORDER_ITEM[2]/descendant::*[3]", "//ot:ORDER_ITEM/descendant-or-self::*[1]", "//text()[contains(., 'https://')]", "count(//text())", "count(//node())", "count(//*)", "count(/descendant::node())", "//ot:REMARKS/text()", "//ot:ORDER_ITEM[ot:LINE_ITEM_ID = 2]/ot:QUANTITY * 2",
  "1 + 2 * 3", "10 div 4", "10 mod 3", "-5 mod 3", "5 mod -3", "1 div 0", "-1 div 0", "0 div 0", "1 = 1", "'a' = 'a'", "'a' != 'b'", "1 < 2", "2 <= 2", "'10' > '9'", "true() = 1", "false() != 'x'", "not(1)", "boolean('')", "boolean('false')", "number('  12.5 ')", "number('1e3')", "number('abc')", "string(12)", "string(1.50)", "string(0.1 + 0.2)", "string(1 div 3)", "string(100000000000000000000)", "string(0.000001)", "string(-0)", "string(true())",
  "concat('a', 'b', 1)", "starts-with('abc', 'ab')", "contains('abc', 'd')", "substring('12345', 2, 3)", "substring('12345', 2)", "substring('12345', 1.5, 2.6)", "substring('12345', 0, 3)", "substring('12345', 0 div 0, 3)", "substring('12345', -42, 1 div 0)", "substring-before('1999/04/01', '/')", "substring-after('1999/04/01', '/')", "string-length('abc')", "normalize-space('  a   b  ')", "translate('bar', 'abc', 'ABC')", "translate('--aaa--', 'abc-', 'ABC')",
  "floor(1.5)", "ceiling(1.2)", "round(2.5)", "round(-2.5)", "round(-0.4)", "sum(//ot:LINE_ITEM_ID)", "count(//ot:ORDER_ITEM) * 2 - 1", "//ot:ORDER_ITEM[position() mod 2 = 1]", "//ot:ORDER_ITEM[position() > 1 and ot:QUANTITY]", "//ot:ORDER_ITEM[not(ot:REMARKS)]",
  "//*[local-name() = 'CITY']", "//*[name() = 'bmecat:CITY']", `//*[namespace-uri() = '${ns("www.bmecat.org/bmecat/2005")}'][1]`, "local-name(/*)", "name(/*)", "namespace-uri(/*)", "name(//@type)", "local-name(//bmecat:CITY)", "//bmecat:*[1]", "//ot:*[last()]", "count(//bmecat:*)",
  "//ot:ORDER_ITEM[ot:LINE_ITEM_ID][2]", "//ot:ORDER_ITEM[2][ot:LINE_ITEM_ID]", "//*[self::bmecat:CITY or self::bmecat:ZIP]", "//bmecat:CITY[. = //bmecat:CITY]", "//bmecat:CITY[. != //bmecat:CITY]", "//ot:QUANTITY[. > //ot:LINE_ITEM_ID]", "//ot:QUANTITY = 12", "//ot:QUANTITY != 12", "//ot:QUANTITY > 1", "//ot:QUANTITY < //ot:LINE_ITEM_ID",
  "//ot:NOPE", "count(//ot:NOPE)", "string(//ot:NOPE)", "//ot:NOPE = ''", "//ot:NOPE != ''", "sum(//ot:NOPE)", "//ot:ORDER_ITEM[", "//ot:ORDER_ITEM]", "//", "foo(", "//ot:ORDER_ITEM[1", "1 +", "count()", "//unbound:X", "lang('de')", "//*[lang('de')][1]", "//comment()", "//processing-instruction()", "/ot:ORDER/node()[1]", "/ot:ORDER/node()[last()]", "//ot:ORDER_HEADER/..", "//ot:ORDER_HEADER/../*", ".", "..", "./*", "self::node()", "/self::node()", "//*[.//bmecat:CITY][last()]", "//ot:PARTY[.//bmecat:CITY = //bmecat:CITY[1]]", "(//bmecat:CITY)[2]/text()", "//bmecat:CITY/text()[1]", "string-length(//bmecat:CITY)", "normalize-space(//ot:REMARKS)", "//*[starts-with(., '2026')][1]", "//*[contains(name(), 'DATE')]", "//ot:ORDER_ITEM/@*", "//*[@type and @default]", "//*[count(*) > 20]", "//*[count(*) = 0][5]", "//*[not(*)][last()]", "(//*)[100]", "(//*)[position() < 4]", "(//@*)[3]/..", "//@type/..", "//@type/parent::*/ot:nope", "//bmecat:ZIP/following::*[2]", "//bmecat:ZIP[1]/preceding::bmecat:*[1]", "//ot:ORDER_SUMMARY/preceding-sibling::*[1]/*[last()]", "//ot:ORDER_SUMMARY/ot:TOTAL_ITEM_NUM + 1",
];
let seed = 4711; const rnd = (n: number) => { seed = (seed * 1103515245 + 12345) & 0x7fffffff; return seed % n; }; const pick = <T,>(a: readonly T[]): T => a[rnd(a.length)]!;
const AX = ["child", "descendant", "parent", "ancestor", "following-sibling", "preceding-sibling", "following", "preceding", "self", "descendant-or-self", "ancestor-or-self"];
const mk = (): string => {
  let x = pick(["//", "/ot:ORDER//", "//", "//"]) + pick(els);
  for (let s = rnd(3); s > 0; s--) {
    const pred = pick(["", "", "", "[1]", "[2]", "[last()]", `[${pick(els)}]`, `[@${pick(attrs)}]`, "[position() < 3]", `[not(${pick(els)})]`, `[count(*) > ${rnd(4)}]`, "[. != '']", `[contains(., '${pick(["a", "1", "e", "-"])}')]`]);
    x += pred;
    const how = rnd(10);
    if (how < 4) x += "/" + pick(["*", pick(els), "node()", "text()", `@${pick(attrs)}`, "@*", ".."]);
    else if (how < 6) x += "//" + pick(["*", pick(els)]);
    else x += `/${pick(AX)}::${pick(["*", "*", pick(els), "node()"])}` + pick(["", "", "[1]", "[2]", "[last()]"]);
  }
  const wrap = rnd(12);
  if (wrap === 0) return `count(${x})`;
  if (wrap === 1) return `string(${x})`;
  if (wrap === 2) return `(${x})[${1 + rnd(3)}]`;
  if (wrap === 3) return `${x} | //${pick(els)}`;
  if (wrap === 4) return `sum(${x})`;
  if (wrap === 5) return `${x} = ${pick(["1", "'deu'", "//" + pick(els)])}`;
  if (wrap === 6) return `boolean(${x}) and ${x} > 2`;
  if (wrap === 7) return `normalize-space(${x})`;
  return x;
};

/** What libxml2 answers outside XPath 1.0: an exponent the specification has no form for, and `number('1e3')`. */
const LIBXML2_LEAVES = ["number('1e3')", "string(100000000000000000000)", "string(0.000001)"];

test("knobas's reader answers as libxml2 does, but where libxml2 leaves XPath 1.0", () => {
  const ldoc = XmlDocument.fromString(TEXT_, { option: 256 });
  try {
    const all = [...hand, ...Array.from({ length: 6000 }, mk)];
    const off: string[] = [];
    for (const x of all) if (!same(mine(x), theirs(ldoc, x))) off.push(x);
    expect(all.length).toBeGreaterThan(6100);
    expect(off).toEqual(LIBXML2_LEAVES);
  } finally {
    ldoc.dispose();
  }
});
