import { expect, test } from "vitest";

import { parseTfHash } from "../tf/route";
import { seedKinds, seedName, sectionFilter } from "./section";
import { SYNTAX } from "./syntax";

/** What `/`, ⌘/ and the top bar's button open the launcher with, per address (#2425). */
test.each([
  ["#/tickets", "#t "],
  ["#/ticket/jira%3APAY-231", "#t "],
  ["#/ticket/jira%3APAY-231/conv", "#t "],
  ["#/job/PAY-231", "#t "],
  ["#/inbox", "#i "],
  ["#/code", "#rp "],
  ["#/repo/gitea%3Aacme%2Fsvc", "#rp "],
  ["#/builds", "#b "],
  ["#/build/pipe-1/run-4812", "#b "],
  ["#/docs", "#d "],
  ["#/page/confluence%3A1", "#d "],
  ["#/monitors", "#m "],
  ["#/time", ""],
  ["#/settings", ""],
  ["#/asset/asset%3Avm-pay-01", ""],
  ["#/sets", ""],
  ["#/note/note-1", ""],
  ["#/nowhere", ""],
])("%s opens the launcher with %j", (hash, filter) => {
  expect(sectionFilter(parseTfHash(hash))).toBe(filter);
});

test.each([
  ["#t ", "Tickets"],
  ["#rp ", "Code"],
  ["#b ", "Builds"],
  ["#i ", "Inbox"],
  ["#x ", null],
  ["", null],
])("the chip for %j says %j (F-008)", (filter, name) => {
  expect(seedName(filter)).toBe(name);
});

test("a section's filter names its kinds from the grammar's table, not a copy of it", () => {
  expect(seedKinds("#rp ", SYNTAX)).toEqual(["pr", "repo"]);
  expect(seedKinds("#b ", SYNTAX)).toEqual(["pipeline"]);
});
