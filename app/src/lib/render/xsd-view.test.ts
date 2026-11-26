import { expect, test } from "vitest";

import { answerXsd, type SetIndex, type XsdSessions } from "./xsd-answer";
import { bundledFiles, familyFiles } from "./xsd-fixture";
import { occText, occWords, railRows } from "./xsd-view";

test("how often, as a token and in words", () => {
  expect(occText({ min: 1, max: 1 })).toBe("1");
  expect(occText({ min: 0, max: 1 })).toBe("0..1");
  expect(occText({ min: 1, max: -1 })).toBe("1..∞");
  expect(occText({ min: 2, max: 4 })).toBe("2..4");
  expect(occText({ min: 0, max: 0 })).toBe("0");
  expect(occWords({ min: 1, max: -1 })).toBe("Once or more");
  expect(occWords({ min: 0, max: 1 })).toBe("Once, or not at all");
  expect(occWords({ min: 0, max: 0 })).toBe("Never: the schema forbids it here");
});

function opened(): { index: SetIndex; sessions: XsdSessions } {
  const files = familyFiles("opentrans-2.1");
  const b = bundledFiles();
  const sessions: XsdSessions = new Map();
  const bundled = { byNs: b.byNs, texts: b.texts };
  let a = answerXsd(sessions, 1, { kind: "open", entry: "opentrans_2_1.xsd", text: files.get("opentrans_2_1.xsd")!, paths: null, pointed: [] }, bundled) as { index: SetIndex; need: string[] };
  a = answerXsd(sessions, 1, { kind: "addFiles", files: a.need.map((p) => ({ path: p, text: files.get(p) ?? null })) }, bundled) as typeof a;
  return { index: a.index, sessions };
}

test("Elements: the opened file's fold open, the others shut until opened; Types lists the rest", () => {
  const { index } = opened();
  const rows = railRows(index, { mode: "elements", open: new Set(), tree: new Map(), ranked: null });
  const files = rows.filter((r) => r.kind === "file");
  expect(files.map((f) => (f.kind === "file" ? [f.path, f.open, f.count] : null))).toEqual([
    ["opentrans_2_1.xsd", true, 283],
    ["bmecat_2005.xsd", false, 440],
    ["xmlmime.xsd", false, 0],
    ["xmldsig-core-schema.xsd", false, 24],
  ]);
  expect(rows.length).toBe(1 + 4 + 283);
  const shut = railRows(index, { mode: "elements", open: new Set(["F:opentrans_2_1.xsd:shut", "F:bmecat_2005.xsd"]), tree: new Map(), ranked: null });
  expect(shut.length).toBe(1 + 4 + 440);
  const types = railRows(index, { mode: "types", open: new Set(), tree: new Map(), ranked: null });
  expect(types.filter((r) => r.kind === "decl").every((r) => r.kind === "decl" && r.decl.kind !== "el")).toBe(true);
});

test("the files are listed in the order they were found, breadth first", () => {
  const { index } = opened();
  expect(index.listing.map((l) => (l.kind === "file" ? index.files[l.at]!.path : "lost"))).toEqual(["opentrans_2_1.xsd", "bmecat_2005.xsd", "xmlmime.xsd", "xmldsig-core-schema.xsd"]);
});

test("the way down to an element is the shortest from a root", () => {
  const { sessions } = opened();
  const b = bundledFiles();
  const way = answerXsd(sessions, 1, { kind: "way", key: `el {${"http:/" + "/"}www.opentrans.org/XMLSchema/2.1}ORDER_HEADER` }, { byNs: b.byNs, texts: b.texts }) as string[];
  expect(way).toHaveLength(3);
});
