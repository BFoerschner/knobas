/** The document worker's answers (the XML view's design, 2026-11-23, sections 7 to 9). */
import { ns } from "./xml-names";
import { expect, test } from "vitest";

import { answerDoc, type DocOpened } from "./doc-model";
import type { AtAnswer } from "./doc-port";
import { BROKEN, fixture } from "./doc-fixture";

const ORDER = fixture("order-4711.xml");

test("open answers the fold table, what the root says and where a broken file stops", () => {
  const sessions = new Map();
  const o = answerDoc(sessions, 1, { kind: "open", text: ORDER }) as DocOpened;
  expect(o.lines).toBe(1814);
  expect(o.folds.line.length).toBeGreaterThan(100);
  expect(o.root).toEqual({
    name: "ORDER",
    namespace: ns("www.opentrans.org/XMLSchema/2.1"),
    location: "../schemas/opentrans/opentrans_2_1.xsd",
    // The prefixes it binds, which a check's findings write names with.
    prefixes: { xsi: ns("www.w3.org/2001/XMLSchema-instance"), bmecat: ns("www.bmecat.org/bmecat/2005"), xmime: ns("www.w3.org/2005/05/xmlmime") },
  });
  expect(o.defaultNs).toBe(true);
  const b = answerDoc(sessions, 2, { kind: "open", text: BROKEN }) as DocOpened;
  expect(b.error?.line).toBe(4);
  const pom = answerDoc(sessions, 3, { kind: "open", text: fixture("pom.xml") }) as DocOpened;
  expect(pom.root?.location).toBe(`https:/${"/"}maven.apache.org/xsd/maven-4.0.0.xsd`);
});

test("a picked line answers the way down and what it holds; a line inside a text picks the element that holds it", () => {
  const sessions = new Map();
  answerDoc(sessions, 1, { kind: "open", text: ORDER });
  const lines = ORDER.split("\n");
  const at = lines.findIndex((l) => l.includes("<bmecat:SUPPLIER_PID")) + 1;
  const a = answerDoc(sessions, 1, { kind: "at", line: at }) as AtAnswer;
  expect(a.crumbs.map((c) => c.name)).toEqual(["ORDER", "ORDER_ITEM_LIST", "ORDER_ITEM[1]", "PRODUCT_ID", "bmecat:SUPPLIER_PID"]);
  expect(a.picked?.path).toBe("/ORDER/ORDER_ITEM_LIST/ORDER_ITEM[1]/PRODUCT_ID/bmecat:SUPPLIER_PID");
  expect(a.picked?.namespace).toBe(ns("www.bmecat.org/bmecat/2005"));
  const root = answerDoc(sessions, 1, { kind: "at", line: 2 }) as AtAnswer;
  expect(root.picked?.holds.map((h) => h.name)).toEqual(["ORDER_HEADER", "ORDER_ITEM_LIST", "ORDER_SUMMARY"]);
  // Inside a long base64 text: the element that holds it.
  const inside = lines.findIndex((l) => l.includes("<MIME_DATA")) + 3;
  const held = answerDoc(sessions, 1, { kind: "at", line: inside }) as AtAnswer;
  expect(held.picked?.name).toBe("MIME_DATA");
  expect(held.picked?.valueLength).toBeGreaterThan(240);
  expect(held.picked?.value?.length).toBe(238);
});

test("folds are named by their elements and found again when a line is added above", () => {
  const sessions = new Map();
  answerDoc(sessions, 1, { kind: "open", text: ORDER });
  const lines = ORDER.split("\n");
  const second = lines.findIndex((l, i) => l.includes("<ORDER_ITEM>") && i > lines.findIndex((m) => m.includes("<ORDER_ITEM>"))) + 1;
  const [key] = answerDoc(sessions, 1, { kind: "keys", lines: [second] }) as string[];
  expect(key).toBe("/ORDER/ORDER_ITEM_LIST/ORDER_ITEM[2]");
  const moved = ORDER.replace("<ORDER_HEADER>", "<ORDER_HEADER>\n    <!-- added -->");
  answerDoc(sessions, 2, { kind: "open", text: moved });
  expect(answerDoc(sessions, 2, { kind: "find", keys: [key!, "/ORDER/GONE[3]"] })).toEqual([second + 1]);
});

test("a fold whose element no longer folds is not found again, though another fold starts on its line", () => {
  const sessions = new Map();
  answerDoc(sessions, 1, { kind: "open", text: "<r>\n<x>\n<y/>\n</x>\n</r>" });
  const [key] = answerDoc(sessions, 1, { kind: "keys", lines: [2] }) as string[];
  expect(key).toBe("/r/x");
  answerDoc(sessions, 2, { kind: "open", text: "<r>\n<x/><w>\n</w>\n</r>" });
  expect(answerDoc(sessions, 2, { kind: "find", keys: [key!] })).toEqual([]);
});
