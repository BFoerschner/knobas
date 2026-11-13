/** The fuzzy finder's rules: which options typed letters keep, in what order, and what they mark. */
import { expect, test } from "vitest";

import { fuzzyRank } from "./fuzzy";

const PROJECTS = [
  "Platform Analytics Yearly (PLY)",
  "Laptops (LAP)",
  "Team Pay (TPY)",
  "Payments (PAY)",
  "Operations (OPS)",
  "Customs (CUS)",
];
const names = (typed: string, options: readonly string[] = PROJECTS) =>
  fuzzyRank(options, typed, (option) => option).map((hit) => hit.option);
const marked = (text: string, typed: string) =>
  fuzzyRank([text], typed, (option) => option)[0]!
    .runs.filter((run) => run.hit)
    .map((run) => run.text);

test("nothing typed keeps every option in the order given, with nothing marked", () => {
  expect(names("")).toEqual(PROJECTS);
  expect(names("   ")).toEqual(PROJECTS);
  expect(fuzzyRank(["Payments"], "", (option) => option)[0]!.runs).toEqual([{ text: "Payments", hit: false }]);
});

test("an option stays when the typed letters appear in it in order, whatever their case", () => {
  expect(names("pymt")).toEqual(["Payments (PAY)"]);
  expect(names("CUSTOMS")).toEqual(["Customs (CUS)"]);
  expect(names("zq")).toEqual([]);
});

test("the closest match comes first: a start, then a word's start, then letters side by side, then scattered ones", () => {
  // Three hold p, a, y in order.
  expect(names("pay")).toEqual([
    "Payments (PAY)",
    "Team Pay (TPY)",
    "Platform Analytics Yearly (PLY)",
  ]);
  // "Laptops" holds `ops` whole but inside a word; "Operations (OPS)" starts with it.
  expect(names("ops")).toEqual(["Operations (OPS)", "Laptops (LAP)"]);
  // Inside a word, letters side by side beat ones with others between them, and fewer between beat more.
  expect(names("tra", ["Catering area", "Contract"])).toEqual(["Contract", "Catering area"]);
  expect(names("cs", ["Calculations", "Cars"])).toEqual(["Cars", "Calculations"]);
  // Equally close options keep the order given.
  expect(names("s", ["Customs", "Sales", "Support"])).toEqual(["Sales", "Support", "Customs"]);
  // A word's start is as close in a long first word as after a short one.
  expect(names("plat", ["Billing Platform", "Fleet Platform", "Nameplate Works"])).toEqual(["Billing Platform", "Fleet Platform", "Nameplate Works"]);
});

test("every word typed has to match, in any order", () => {
  expect(names("yearly plat")).toEqual(["Platform Analytics Yearly (PLY)"]);
  expect(names("pay ops")).toEqual([]);
});

test("the letters marked are the ones that made the match, side by side where they can be", () => {
  expect(marked("Payments (PAY)", "pay")).toEqual(["Pay"]);
  expect(marked("Team Pay (TPY)", "pay")).toEqual(["Pay"]);
  expect(marked("Payments (PAY)", "pymt")).toEqual(["P", "ym", "t"]);
  expect(marked("Platform Analytics Yearly (PLY)", "yearly plat")).toEqual(["Plat", "Yearly"]);
});
