/**
 * The filter bar's rules (#810, R-2.1 to R-2.7): which rows a filter keeps,
 * what it marks, what it says, and the query it sends a server-side view.
 */
import { expect, test } from "vitest";

import { countLine, emptyFilter, isFiltering, joined, markPairs, marks, matchesAll, runPairs, sliceMarks, toQuery, wordPairs, type FilterConfig } from "./filter";

const config: FilterConfig = {
  id: "tickets-all",
  placeholder: "Filter by key, summary, epic or person",
  noun: "tickets",
  base: "",
  chips: [
    { key: "issuetype", label: "Type", options: ["Story", "Bug"].map((value) => ({ value, label: value })) },
    { key: "priority", label: "Priority", options: [{ value: "High", label: "High" }] },
  ],
  dropdowns: [
    { key: "assignee", label: "Assignee", options: [{ value: "@me", label: "Me" }] },
    { key: "under", label: "Epic", options: [{ value: "PAY-200", label: "PAY-200" }] },
  ],
};

test("every word typed has to appear, in any order, ignoring case", () => {
  expect(matchesAll("PAY-231 Retry failed SEPA payouts", "sepa retry")).toBe(true);
  expect(matchesAll("PAY-231 Retry failed SEPA payouts", "  SEPA   pay-231 ")).toBe(true);
  expect(matchesAll("PAY-231 Retry failed SEPA payouts", "sepa refund")).toBe(false);
  expect(matchesAll("anything", "")).toBe(true);
});

test("each word is marked wherever it occurs, and a one-letter word is not", () => {
  expect(marks("Retry failed SEPA payouts", "sepa pay a")).toEqual([
    { text: "Retry failed ", hit: false },
    { text: "SEPA", hit: true },
    { text: " ", hit: false },
    { text: "pay", hit: true },
    { text: "outs", hit: false },
  ]);
  expect(marks("Retry", "")).toEqual([{ text: "Retry", hit: false }]);
  // Overlapping words mark the union, never a mark inside a mark.
  expect(marks("payouts", "pay payout")).toEqual([
    { text: "payout", hit: true },
    { text: "s", hit: false },
  ]);
});

test("the count says how many of how many, or only how many when nothing filters", () => {
  expect(countLine(8, 75, "tickets")).toBe("8 of 75 tickets");
  expect(countLine(75, 75, "tickets")).toBe("75 tickets");
});

test("a total of one takes the singular noun when the config has one", () => {
  expect(countLine(1, 1, "tickets", "ticket")).toBe("1 ticket");
  expect(countLine(0, 1, "tickets", "ticket")).toBe("0 of 1 ticket");
  expect(countLine(2, 2, "tickets", "ticket")).toBe("2 tickets");
  expect(countLine(1, 1, "tickets")).toBe("1 tickets");
});

test("a server-side view sends its base query, the chips, the dropdowns and the words", () => {
  const state = emptyFilter();
  expect(toQuery(config, state)).toBe("");
  expect(isFiltering(state)).toBe(false);

  state.chips["issuetype"] = ["Story", "Bug"];
  state.dropdowns["assignee"] = "@me";
  state.dropdowns["under"] = "PAY-200";
  state.text = "sepa retry";
  expect(toQuery({ ...config, base: "project:PAY" }, state)).toBe(
    "project:PAY issuetype:Story issuetype:Bug assignee:@me under:PAY-200 sepa retry",
  );
  expect(isFiltering(state)).toBe(true);
});

test("a value with a space or a quote is quoted, so the grammar reads it as one", () => {
  const state = emptyFilter();
  state.chips["issuetype"] = ["Sub-task", "User Story"];
  expect(toQuery(config, state)).toBe('issuetype:Sub-task issuetype:"User Story"');
});

test("sliceMarks clips and rebases UTF-16 mark pairs onto a field slice", () => {
  // Haystack: "get /repos/adproxy summary"
  // Pairs:    "get" [0, 3], "ad" [11, 13], "proxy" [13, 18], "sum" [19, 22]
  const pairs = [0, 3, 11, 13, 13, 18, 19, 22];
  // Slice "/repos/adproxy" starts at 4, length 14 -> [4, 18)
  expect(sliceMarks(pairs, 4, 14)).toEqual([7, 9, 9, 14]);
  // Pair straddling both boundaries gets clipped cleanly
  expect(sliceMarks([2, 20], 4, 10)).toEqual([0, 10]);
  // Empty, out-of-range, or zero-length slices yield no marks
  expect(sliceMarks([], 0, 10)).toEqual([]);
  expect(sliceMarks([10, 12], 0, 5)).toEqual([]);
  expect(sliceMarks([0, 5], 0, 0)).toEqual([]);
  // Surrogate pair (emoji = 2 UTF-16 code units): "🚀 ADProxy" -> "Proxy" is [5, 10]
  const emoji = "🚀 ADProxy";
  expect(sliceMarks([5, 10], 3, emoji.length - 3)).toEqual([2, 7]);
});

test("markPairs splits text into plain and hit runs from UTF-16 pairs and an optional offset", () => {
  expect(markPairs("ADProxy", [0, 2, 3, 4])).toEqual([
    { text: "AD", hit: true },
    { text: "P", hit: false },
    { text: "r", hit: true },
    { text: "oxy", hit: false },
  ]);
  // With `from` offset into a longer haystack `"acme/ADProxy"` (`"ADProxy"` starts at 5)
  expect(markPairs("ADProxy", [0, 2, 7, 12], 5)).toEqual([
    { text: "AD", hit: false },
    { text: "Proxy", hit: true },
  ]);
  // Surrogate pair: `"🚀 ADProxy"` -> `"🚀 "` is 3 UTF-16 units, `"Proxy"` is [5, 10]
  expect(markPairs("🚀 ADProxy", [5, 10])).toEqual([
    { text: "🚀 AD", hit: false },
    { text: "Proxy", hit: true },
  ]);
  // No overlapping marks returns one plain run
  expect(markPairs("ADProxy", [20, 25])).toEqual([{ text: "ADProxy", hit: false }]);
});

test("runs become the UTF-16 pairs of their marked ones, as the ranking answers (#2532)", () => {
  expect(runPairs([{ text: "pay", hit: true }, { text: "out-", hit: false }, { text: "s", hit: true }, { text: "ervice", hit: false }])).toEqual([0, 3, 7, 8]);
  expect(runPairs([{ text: "nothing", hit: false }])).toEqual([]);
  expect(wordPairs("Payout payout", "pay")).toEqual([0, 3, 7, 10]);
});

test("joined fields cut a ranking's marks back onto each field, whatever the separator (#2532, #2535)", () => {
  const f = joined(["Payout", "", "ops, payments", "Rust"], ", ");
  expect(f.text).toBe("Payout, , ops, payments, Rust");
  // "pay" in the name and in the teams, "ust" in the language.
  const pairs = [0, 3, 15, 18, 26, 29];
  expect(f.cut(pairs, 0)).toEqual([0, 3]);
  expect(f.cut(pairs, 1)).toEqual([]);
  expect(f.cut(pairs, 2)).toEqual([5, 8]);
  expect(f.cut(pairs, 3)).toEqual([1, 4]);
  expect(f.cut(pairs, 2, 5, 3)).toEqual([0, 3]);
  // Marks past the joined text (a longer text it starts) reach the last field when cut to the end.
  expect(joined(["a", "b"]).cut([2, 5], 1, 0, Infinity)).toEqual([0, 3]);
});
