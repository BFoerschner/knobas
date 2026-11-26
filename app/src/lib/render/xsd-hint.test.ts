import { expect, test } from "vitest";

import { HINT_DAYS, HINT_ENTRIES, HINT_KEY, HINT_PATHS, checkedLately, hintKey, hintStore, hintable, hintedPaths, keepHint } from "./xsd-hint";
import type { KeptStorage } from "../tf/kept-value";

function memory(): KeptStorage & { held: Map<string, string> } {
  const held = new Map<string, string>();
  return { held, getItem: (k) => held.get(k) ?? null, setItem: (k, v) => void held.set(k, v), removeItem: (k) => void held.delete(k) };
}
const DAY = 24 * 60 * 60 * 1000;
const NOW = Date.UTC(2026, 10, 25);
const SET = ["schemas/openTRANS_ORDER_1_0.xsd", "schemas/bmecat_2005.xsd", "@repo/gitea%3At%2Fc/x/y.xsd", "@download/ab12", "@knobas/xml.xsd"];

test("the paths a set read come back for its key, and for no other", () => {
  const s = memory();
  const store = hintStore(() => s);
  const key = hintKey("gitea:t/p", "orders/.knobas-document-schema.xsd", "<xs:schema/>");
  keepHint(store, key, SET, NOW);
  expect(hintedPaths(store, key, NOW)).toEqual(SET);
  // Another repository, another entry path, another entry text: another key.
  expect(hintedPaths(store, hintKey("gitea:t/q", "orders/.knobas-document-schema.xsd", "<xs:schema/>"), NOW)).toEqual([]);
  expect(hintedPaths(store, hintKey("gitea:t/p", "other/.knobas-document-schema.xsd", "<xs:schema/>"), NOW)).toEqual([]);
  expect(hintedPaths(store, hintKey("gitea:t/p", "orders/.knobas-document-schema.xsd", "<xs:schema xmlns:b='x'/>"), NOW)).toEqual([]);
  expect(s.held.has(HINT_KEY)).toBe(true);
});

test("only paths the walk makes are taken; an address, a question, a climb or a control character is not", () => {
  for (const ok of SET) expect(hintable(ok)).toBe(true);
  for (const no of ["", "/etc/passwd", "../x.xsd", "a/../../x.xsd", "a//b.xsd", "./a.xsd", "https://example.com/a.xsd", "file:///a.xsd", "c:/a.xsd", "?index", "?elsewhere:urn:a", "@repo/", "@repo/x", "@download/", "@other/a.xsd", "a\\b.xsd", "a\u0000b", "a\nb", "x".repeat(1025), 5, null]) expect(hintable(no)).toBe(false);
  // A set that read one such path keeps no hint.
  const held = memory();
  const store = hintStore(() => held);
  keepHint(store, "k", ["a.xsd", "https://example.com/b.xsd"], NOW);
  expect(hintedPaths(store, "k", NOW)).toEqual([]);
});

test("what the store holds is read back with care: garbage, a bad path or a bad entry is dropped, never thrown", () => {
  const s = memory();
  const store = hintStore(() => s);
  for (const raw of ["not json", "{}", "null", "[1,2]", JSON.stringify([{ k: 1, p: [], t: NOW }])]) {
    s.held.set(HINT_KEY, raw);
    expect(hintedPaths(store, "k", NOW)).toEqual([]);
    expect(checkedLately(store, NOW)).toBe(false);
  }
  s.held.set(HINT_KEY, JSON.stringify([{ k: "bad", p: ["a.xsd", "https://example.invalid/y.xsd"], t: NOW }, { k: "good", p: ["a.xsd"], t: NOW }]));
  expect(hintedPaths(store, "bad", NOW)).toEqual([]);
  expect(hintedPaths(store, "good", NOW)).toEqual(["a.xsd"]);
  // A store that throws reads as nothing kept.
  const blocked = hintStore(() => ({ getItem: () => { throw new Error("x"); }, setItem: () => { throw new Error("x"); }, removeItem: () => {} }));
  expect(() => keepHint(blocked, "k", ["a.xsd"], NOW)).not.toThrow();
  expect(hintedPaths(blocked, "k", NOW)).toEqual([]);
});

test("bounded: the least recently written entry goes past the bound, a set of too many files is not hinted", () => {
  const held = memory();
  const store = hintStore(() => held);
  for (let i = 0; i <= HINT_ENTRIES; i += 1) keepHint(store, `k${i}`, [`s${i}.xsd`], NOW + i);
  expect(JSON.parse(held.held.get(HINT_KEY)!), "what is written holds the bound too").toHaveLength(HINT_ENTRIES);
  expect(hintedPaths(store, "k0", NOW + 100)).toEqual([]);
  expect(hintedPaths(store, "k1", NOW + 100)).toEqual(["s1.xsd"]);
  expect(hintedPaths(store, `k${HINT_ENTRIES}`, NOW + 100)).toEqual([`s${HINT_ENTRIES}.xsd`]);
  // Written again, k1 is the newest: the next one in drops k2.
  keepHint(store, "k1", ["s1.xsd", "t1.xsd"], NOW + 200);
  keepHint(store, "new", ["n.xsd"], NOW + 201);
  expect(hintedPaths(store, "k2", NOW + 300)).toEqual([]);
  expect(hintedPaths(store, "k1", NOW + 300)).toEqual(["s1.xsd", "t1.xsd"]);
  // Exactly the bound of paths is kept; one more is not.
  const many = Array.from({ length: HINT_PATHS + 1 }, (_, i) => `m${i}.xsd`);
  keepHint(store, "many", many.slice(0, HINT_PATHS), NOW);
  expect(hintedPaths(store, "many", NOW)).toHaveLength(HINT_PATHS);
  keepHint(store, "more", many, NOW);
  expect(hintedPaths(store, "more", NOW)).toEqual([]);
});

test("an entry not written for 30 days goes, and with the last one this profile no longer checks lately", () => {
  const held = memory();
  const store = hintStore(() => held);
  expect(checkedLately(store, NOW)).toBe(false);
  keepHint(store, "k", ["a.xsd"], NOW);
  expect(checkedLately(store, NOW + HINT_DAYS * DAY)).toBe(true);
  expect(hintedPaths(store, "k", NOW + HINT_DAYS * DAY)).toEqual(["a.xsd"]);
  expect(checkedLately(store, NOW + HINT_DAYS * DAY + 1)).toBe(false);
  expect(hintedPaths(store, "k", NOW + HINT_DAYS * DAY + 1)).toEqual([]);
  // A clock set far ahead when it was written does not keep it forever.
  keepHint(store, "ahead", ["b.xsd"], NOW + 400 * DAY);
  expect(hintedPaths(store, "ahead", NOW)).toEqual([]);
});
