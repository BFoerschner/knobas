/** The outline (the XML view's design, 2026-11-23, sections 7 and 27). */
import { ns } from "./xml-names";
import { expect, test } from "vitest";

import { BROKEN, CORNERS, fixture, ONE_LINE } from "./doc-fixture";
import { CDATA, COMMENT, EL, TEXT, decode, nameOf, namePathOf, nodeAtPath, pathOf, readXml, stringOf } from "./xml-outline";

test("the order reads into 2,961 rows and 1,009 attributes on 1,814 lines", () => {
  const o = readXml(fixture("order-4711.xml"));
  expect(o.error).toBeNull();
  expect(o.lines).toBe(1814);
  expect(o.n).toBe(2961);
  expect(o.attrs).toBe(1009);
  expect(nameOf(o, o.root)).toBe("ORDER");
  expect(o.uris[o.ns[o.root]!]).toBe(ns("www.opentrans.org/XMLSchema/2.1"));
});

test("a subtree is the rows after its root, and a path finds the node it was written from", () => {
  const o = readXml(fixture("order-4710.xml"));
  for (let i = 1; i < o.n; i += 97) {
    if (o.type[i] !== EL) continue;
    for (let k = i + 1; k <= o.sub[i]!; k += 1) expect(o.depth[k]!).toBeGreaterThan(o.depth[i]!);
    expect(nodeAtPath(o, pathOf(o, i))).toBe(i);
  }
});

test("positions are written where a name has siblings, and the name path has none", () => {
  const o = readXml(fixture("order-4710.xml"));
  let second = -1;
  for (let i = 1; i < o.n && second === -1; i += 1) if (nameOf(o, i) === "ORDER_ITEM" && pathOf(o, i).endsWith("ORDER_ITEM[2]")) second = i;
  expect(second).toBeGreaterThan(0);
  expect(pathOf(o, second)).toBe("/ORDER/ORDER_ITEM_LIST/ORDER_ITEM[2]");
  expect(namePathOf(o, second)).toBe("ORDER/ORDER_ITEM_LIST/ORDER_ITEM");
});

test("blank text between elements is no row; text, comments and CDATA are", () => {
  const o = readXml(CORNERS);
  const types = [...o.type.subarray(1)];
  expect(types.filter((t) => t === COMMENT)).toHaveLength(1);
  expect(types.filter((t) => t === CDATA)).toHaveLength(1);
  expect(types.filter((t) => t === TEXT)).toHaveLength(2);
  const cdata = types.indexOf(CDATA) + 1;
  expect(stringOf(o, o.parent[cdata]!)).toBe("a < b && c");
});

test("namespaces: two prefixes for one, and a default reset to none", () => {
  const o = readXml(CORNERS);
  const byName = (n: string) => [...Array(o.n).keys()].find((i) => o.type[i] === EL && nameOf(o, i) === n)!;
  expect(o.uris[o.ns[byName("a:x")]!]).toBe("urn:two");
  expect(o.uris[o.ns[byName("b:x")]!]).toBe("urn:two");
  expect(o.uris[o.ns[byName("inner")]!]).toBe("");
  expect(o.uris[o.ns[o.root]!]).toBe("urn:one");
});

test("a file that is not well formed reads as far as it holds and says where it breaks", () => {
  const o = readXml(BROKEN);
  expect(o.error).toEqual({ message: "`</PRICES>` closes nothing open here: `<ITEM>` is open.", line: 4 });
  // What stands open is closed where the file stops, so the lines above still fold.
  expect(o.endLine[o.root]).toBe(5);
  expect(readXml("<a>\n<b>").error?.message).toBe("`<b>` is never closed.");
  expect(readXml("").error).toEqual({ message: "The file holds no element.", line: 1 });
  expect(readXml("<a/><b/>").error?.message).toBe("A second root element: a document has one.");
  expect(readXml("<a x=1/>").error?.message).toBe("The value of `x` is not quoted.");
});

test("a document on one line reads", () => {
  const o = readXml(ONE_LINE);
  expect(o.error).toBeNull();
  expect(o.lines).toBe(1);
  expect(stringOf(o, o.root)).toBe("onetwo  spaces");
});

test("references are read; anything else stays as written", () => {
  expect(decode("a &lt; b &amp;&#65;&#x42; &nbsp;")).toBe("a < b &AB &nbsp;");
});

test("many short texts between self-closed tags grow the columns once and keep every row", () => {
  const text = `<r>${"x<b/>".repeat(5000)}</r>`;
  const o = readXml(text);
  expect(o.error).toBeNull();
  expect(o.n).toBe(1 + 1 + 10000);
  expect(nameOf(o, o.n - 1)).toBe("b");
});

test("a line ending CR LF counts its lines as the file view does", () => {
  const o = readXml("<a>\r\n<b/>\r\n</a>\r\n");
  expect(o.lines).toBe(3);
  expect(o.line[o.root + 1]).toBe(2);
});
