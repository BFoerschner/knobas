/** Whether a column takes a held card (ticket-edit decisions, section 6): one case a rule, and their order. */
import { describe, expect, test } from "vitest";

import { dropOn, either, sameStatus, type DropColumn, type DropFacts } from "./drop";
import type { Reach } from "./reach.svelte";

const ticket = { status: "In Progress" };
const facts: DropFacts = { source: "Tidewater Jira", moves: true, refused: null };
const column = (statuses: string[], over: Partial<DropColumn> = {}): DropColumn => ({ name: statuses[0] ?? "No status", statuses, own: false, sameLane: true, ...over });
const exact = (statuses: string[]): Reach => ({ statuses, standing: "exact" });

describe("dropOn", () => {
  test("1: the card's own column puts it back, whatever else is true", () => {
    expect(dropOn(ticket, column(["In Progress"], { own: true, sameLane: false }), null, { ...facts, refused: "no" })).toEqual({ kind: "own" });
  });

  test("2: another lane is refused before anything about the status", () => {
    expect(dropOn(ticket, column(["Done"], { sameLane: false }), exact(["Done"]), facts)).toEqual({ kind: "no", why: "A move changes the status. Drop it in its own lane." });
  });

  test("3: a source whose adapter offers no moves", () => {
    expect(dropOn(ticket, column(["Done"]), exact(["Done"]), { ...facts, moves: false })).toEqual({
      kind: "no",
      why: "Tidewater Jira tickets cannot be moved from knobas: its adapter offers no moves.",
    });
  });

  test("4: a refused credential, in the source's words", () => {
    const why = "This Jira user cannot move tickets in PAY: it lacks the Transition Issues permission";
    expect(dropOn(ticket, column(["Done"]), exact(["Done"]), { ...facts, refused: why })).toEqual({ kind: "no", why });
  });

  test("5: the No status column", () => {
    expect(dropOn(ticket, column([]), exact(["Done"]), facts)).toEqual({ kind: "no", why: "A ticket cannot be moved to no status." });
  });

  test("6: nothing known asks, with the column's statuses", () => {
    expect(dropOn(ticket, column(["In Review", "Testing"]), null, facts)).toEqual({ kind: "ask", statuses: ["In Review", "Testing"] });
  });

  test("7: only what the source was seen to use asks with those, never offers", () => {
    const seen: Reach = { statuses: ["To Do", "In Progress", "Done"], standing: "observed" };
    expect(dropOn(ticket, column(["Done", "Archived"]), seen, facts)).toEqual({ kind: "ask", statuses: ["Done"] });
    expect(dropOn(ticket, column(["Archived", "Gone"]), seen, facts)).toEqual({ kind: "no", why: "Tidewater Jira has no status Archived or Gone." });
  });

  test("8: the column's statuses the ticket can reach, in the column's order, compared as the adapters compare", () => {
    expect(dropOn(ticket, column(["Testing", "In review"]), exact(["In Review", "Testing"]), facts)).toEqual({ kind: "yes", statuses: ["Testing", "In review"] });
    expect(dropOn(ticket, column(["In progress"]), exact(["In Progress "]), { ...facts })).toEqual({ kind: "yes", statuses: ["In progress"] });
  });

  test("9: none reachable says from where to where", () => {
    expect(dropOn(ticket, column(["Done", "Closed"]), exact(["In Review"]), facts)).toEqual({
      kind: "no",
      why: "Tidewater Jira offers no move from In Progress to Done or Closed.",
    });
  });

  test("a like answer counts as an answer", () => {
    expect(dropOn(ticket, column(["Done"]), { statuses: ["Done"], standing: "like" }, facts)).toEqual({ kind: "yes", statuses: ["Done"] });
  });
});

test("statuses compare trimmed and ignoring case; either joins names", () => {
  expect(sameStatus(" In progress", "in Progress ")).toBe(true);
  expect(sameStatus("Done", null)).toBe(false);
  expect(either(["A"])).toBe("A");
  expect(either(["A", "B", "C"])).toBe("A, B or C");
});
