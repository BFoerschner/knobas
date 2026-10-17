/**
 * The filter bar's rules (#810, R-2.1 to R-2.7): which rows a filter keeps,
 * what it marks, what it says, and the query it sends a server-side view.
 */
import { expect, test } from "vitest";

import { countLine, emptyFilter, isFiltering, marks, matchesAll, toQuery, type FilterConfig } from "./filter";

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
