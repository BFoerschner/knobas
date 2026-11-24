import { describe, expect, test } from "vitest";

import { rx, rxMake, rxParse, rxTest } from "./xsd-pattern";
import { familyFiles } from "./xsd-fixture";
import { readSchemaXml, type XNode } from "./xsd-xml";

const ok = (src: string) => {
  const r = rxParse(src);
  if (!r.ok) throw new Error(r.why);
  return r.re;
};
const seeded = (seed: number) => () => {
  seed = (seed * 1103515245 + 12345) % 2147483648;
  return seed / 2147483648;
};

describe("XML Schema's pattern language is not JavaScript's (section 21)", () => {
  test("the whole value matches; ^ and $ are characters", () => {
    expect(rxTest(ok("ab"), "xab")).toBe(false);
    expect(rxTest(ok("^a$"), "^a$")).toBe(true);
    expect(rxTest(ok("^a$"), "a")).toBe(false);
  });
  test("\\w leaves out the underscore; \\i and \\c are XML's name characters", () => {
    expect(rxTest(ok("\\w+"), "abc1")).toBe(true);
    expect(rxTest(ok("\\w+"), "a_b")).toBe(false);
    expect(rxTest(ok("\\i\\c*"), "_x-1")).toBe(true);
    expect(rxTest(ok("\\i\\c*"), "1x")).toBe(false);
  });
  test("a category, a block, and a class taken from a class", () => {
    expect(rxTest(ok("\\p{L}+"), "Ärger")).toBe(true);
    expect(rxTest(ok("\\p{IsGreek}+"), "αβ")).toBe(true);
    expect(rxTest(ok("[a-z-[aeiou]]+"), "bcd")).toBe(true);
    expect(rxTest(ok("[a-z-[aeiou]]+"), "bad")).toBe(false);
  });
  test("a range that runs backwards and an unknown block are refused by name", () => {
    expect(rxParse("[z-a]")).toEqual({ ok: false, why: "the range z-a runs backwards" });
    expect(rxParse("\\p{IsKlingon}")).toEqual({ ok: false, why: "knobas does not know the block Klingon" });
  });
  test("a pattern that backtracks without end answers 'not known' inside the budget", () => {
    expect(rxTest(ok("(a*)*b"), "a".repeat(40), 50_000)).toBeNull();
  });
});

describe("a string made from a pattern", () => {
  test("plain words are preferred, and the holder's liked word wins", () => {
    const re = ok("gros_list|net_customer|net_list|udp_\\w{1,16}");
    expect(["gros_list", "net_customer", "net_list"]).toContain(rxMake(re, seeded(1)));
    expect(rxMake(re, seeded(2), (w) => w === "net_list")).toBe("net_list");
  });
  test("every pattern of openTRANS and BMEcat is read, and what is made from it matches", () => {
    const patterns = new Set<string>();
    const walk = (n: XNode) => {
      if (n.l === "pattern") patterns.add(n.a.find((a) => a.q === "value")!.v);
      n.c.forEach(walk);
    };
    for (const text of familyFiles("opentrans-2.1").values()) walk(readSchemaXml(text).root);
    expect(patterns.size).toBeGreaterThanOrEqual(20);
    for (const p of patterns) {
      const r = rx(p);
      expect(r.ok, p).toBe(true);
      if (!r.ok) continue;
      const made = rxMake(r.re, seeded(7));
      expect(made, p).not.toBeNull();
      expect(rxTest(r.re, made!), `${p} → ${made}`).toBe(true);
    }
  });
});
