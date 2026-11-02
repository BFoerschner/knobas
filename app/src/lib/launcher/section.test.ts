import { expect, test } from "vitest";

import { parseTfHash } from "../tf/route";
import { sectionFilter } from "./section";

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
  ["#/deps", ""],
  ["#/sets", ""],
  ["#/note/note-1", ""],
  ["#/nowhere", ""],
])("%s opens the launcher with %j", (hash, filter) => {
  expect(sectionFilter(parseTfHash(hash))).toBe(filter);
});
