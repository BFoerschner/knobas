/** XPath 1.0 over the outline (the XML view's design, 2026-11-23, sections 20 and 21). */
import { expect, test } from "vitest";

import { fixture } from "./doc-fixture";
import { readXml, nameOf, stringOf } from "./xml-outline";
import { XPathError, compile, numStr, run, type Answer } from "./xml-xpath";

const ORDER = readXml(fixture("order-4711.xml"));
const SMALL = readXml(
  [
    '<lib xmlns="urn:lib" xmlns:p="urn:p" xmlns:q="urn:p">',
    '  <book id="b1" xml:lang="en-GB"><title>One</title><price>10.5</price></book>',
    '  <book id="b2"><title>Two</title><price>20</price><!-- c --></book>',
    '  <p:note>n1</p:note>',
    '  <q:note>n2</q:note>',
    "  <?pi data?>",
    "</lib>",
  ].join("\n"),
);

const ask = (o: typeof SMALL, src: string, strict = false): Answer => run(o, compile(src), { strict });
const names = (o: typeof SMALL, src: string, strict = false) => {
  const a = ask(o, src, strict);
  if (a.type !== "nodes") throw new Error(`not nodes: ${a.type}`);
  return a.ids.map((id) => nameOf(o, id) || stringOf(o, id));
};
const value = (o: typeof SMALL, src: string) => {
  const a = ask(o, src);
  return a.type === "nodes" ? null : a.text;
};
const refused = (src: string): { message: string; at: number } => {
  try {
    run(SMALL, compile(src));
  } catch (cause) {
    if (cause instanceof XPathError) return { message: cause.message, at: cause.at };
    throw cause;
  }
  throw new Error(`${src} answered`);
};

test("a name matches as the document writes it; strictly, XPath's own rule finds nothing under a default namespace", () => {
  expect(names(ORDER, "//ORDER_ITEM")).toEqual(["ORDER_ITEM", "ORDER_ITEM"]);
  expect(names(ORDER, "//ORDER_ITEM", true)).toEqual([]);
  expect(value(ORDER, "count(//ORDER_ITEM[QUANTITY > 10]/LINE_ITEM_ID)")).toBe(String(names(ORDER, "//ORDER_ITEM[QUANTITY > 10]/LINE_ITEM_ID").length));
});

test("a prefix finds its namespace's elements whatever prefix the document wrote for it", () => {
  expect(names(SMALL, "//p:note")).toEqual(["p:note", "q:note"]);
  expect(names(SMALL, "//q:*")).toEqual(["p:note", "q:note"]);
  expect(refused("//z:note").message).toBe("The document binds no prefix z:. It binds xml:, p:, q:.");
});

test("each axis, in document order, a reverse axis counted nearest first", () => {
  expect(names(SMALL, "//book[2]/preceding-sibling::*[1]/@id")).toEqual(["id"]);
  expect(value(SMALL, "string(//book[2]/preceding-sibling::*[1]/@id)")).toBe("b1");
  expect(names(SMALL, "//title[.='Two']/ancestor::*")).toEqual(["lib", "book"]);
  expect(value(SMALL, "name(//title[.='Two']/ancestor::*[1])")).toBe("book");
  expect(names(SMALL, "//book[1]/following::title")).toEqual(["title"]);
  expect(names(SMALL, "//price[.='20']/preceding::title")).toEqual(["title", "title"]);
  expect(names(SMALL, "//book[1]/following-sibling::book/title")).toEqual(["title"]);
  expect(names(SMALL, "/lib/book/self::book")).toHaveLength(2);
  expect(names(SMALL, "//title/..")).toEqual(["book", "book"]);
  expect(names(SMALL, "//book[1]/descendant-or-self::*")).toEqual(["book", "title", "price"]);
  expect(names(SMALL, "//book/@*")).toEqual(["id", "xml:lang", "id"]);
  expect(names(SMALL, "/lib/@*")).toEqual([]);
  expect(names(SMALL, "//comment()")).toEqual([" c "]);
  expect(names(SMALL, "//processing-instruction('pi')")).toEqual(["pi"]);
  expect(names(SMALL, "(//title | //price)")).toEqual(["title", "price", "title", "price"]);
  expect(names(SMALL, "//book[last()]/title")).toEqual(["title"]);
  expect(value(SMALL, "string(//book[position() = 2]/title)")).toBe("Two");
});

test("//x[1] is the first x of each parent, not the first of the document", () => {
  expect(names(SMALL, "//title[1]")).toHaveLength(2);
  expect(names(SMALL, "(//title)[1]")).toHaveLength(1);
});

test("functions", () => {
  expect(value(SMALL, "concat('a', 'b', 'c')")).toBe("abc");
  expect(value(SMALL, "substring('12345', 1.5, 2.6)")).toBe("234");
  expect(value(SMALL, "substring-before('a/b', '/')")).toBe("a");
  expect(value(SMALL, "substring-after('a/b', '/')")).toBe("b");
  expect(value(SMALL, "translate('bar', 'abc', 'ABC')")).toBe("BAr");
  expect(value(SMALL, "normalize-space('  a   b ')")).toBe("a b");
  expect(value(SMALL, "string-length('äb')")).toBe("2");
  expect(value(SMALL, "starts-with('abc', 'ab') and contains('abc', 'bc')")).toBe("true");
  expect(value(SMALL, "sum(//price)")).toBe("30.5");
  expect(value(SMALL, "round(2.5) + floor(-1.5) + ceiling(1.2)")).toBe("3");
  expect(value(SMALL, "count(//book[lang('en')])")).toBe("1");
  expect(value(SMALL, "local-name(//p:note)")).toBe("note");
  expect(value(SMALL, "namespace-uri(//q:note)")).toBe("urn:p");
  expect(value(SMALL, "boolean(//nothing) or not(true())")).toBe("false");
  expect(value(SMALL, "7 mod 3 * 2 div 4")).toBe("0.5");
  expect(value(SMALL, "-(-3)")).toBe("3");
});

test("comparisons of sets: = with any, != with any other, relational by number", () => {
  expect(value(SMALL, "//price = 20")).toBe("true");
  expect(value(SMALL, "//price != 20")).toBe("true");
  expect(value(SMALL, "//price > 15")).toBe("true");
  expect(value(SMALL, "//price > 25")).toBe("false");
  expect(value(SMALL, "//title = //title")).toBe("true");
  // Any node of each set, not the first alone: the match is the second title.
  expect(value(SMALL, "//title = //title[. = 'Two']")).toBe("true");
  expect(value(SMALL, "//nothing = //title")).toBe("false");
});

test("the three places libxml2 leaves XPath 1.0 are pinned to the specification's answer", () => {
  expect(value(SMALL, "number('1e3')")).toBe("NaN");
  expect(value(SMALL, "100000000000000000000")).toBe("100000000000000000000");
  expect(value(SMALL, "0.000001")).toBe("0.000001");
  expect(numStr(487382.139999991)).toBe("487382.139999991");
  expect(numStr(0.1 + 0.2)).toBe("0.3");
});

test("an expression that breaks says what and where, in words", () => {
  expect(refused("//ORDER_ITEM[QUANTITY > 10")).toEqual({ message: "A ] is missing: what opens at character 13 is not closed.", at: 26 });
  expect(refused("lower-case(.)")).toEqual({ message: "XPath 1.0 has no lower-case(). It came with XPath 2.0; here, translate(., 'ABC…', 'abc…') does it.", at: 0 });
  expect(refused("id('b1')").message).toBe("id() needs a DTD, which knobas does not read.");
  expect(refused("//a/namespace::*").message).toBe("knobas does not read the namespace axis.");
  expect(refused("$x").message).toBe("knobas has no value for $x: a query stands by itself.");
  expect(refused("'open").message).toBe("The text that starts at character 1 is not closed: a ' is missing.");
  expect(refused("count(1, 2)").message).toBe("count() takes 1 value; it was given 2.");
  expect(refused("//a[]").message).toBe("The test that opens at character 4 is empty.");
  expect(refused("sideways::a").message).toMatch(/^sideways is no axis\./);
  expect(refused("").message).toBe("Nothing to ask yet.");
  expect(refused("//a +").message).toBe("Something is missing after +.");
  expect(refused("count('a')").message).toBe("count() counts nodes and was given a value.");
});

test("a query stops at its budget and says so", () => {
  let stopped: XPathError | null = null;
  try {
    run(ORDER, compile("//*[. = //bmecat:CITY]"), { budget: 100_000 });
  } catch (cause) {
    stopped = cause as XPathError;
  }
  expect(stopped?.budget).toBe(true);
  expect(stopped?.message).toBe("Stopped: the query looked at more of the document than knobas lets one query look at.");
});
