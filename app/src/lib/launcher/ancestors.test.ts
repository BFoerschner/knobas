/**
 * The ancestor path (#284), and the direction it fails in.
 *
 * ADR-0007 requirement 3 makes "which way does this read fail" an obligation
 * rather than an author's habit, and the answer here is **absence**: a record
 * with no recognizable ancestors shows no path, and never a wrong one. That is
 * what lets one row component draw a Confluence page beside a Jira ticket
 * without either of them being asked what kind it is.
 */
import { expect, test } from "vitest";

import { ancestorPath } from "./ancestors";

test("a page's ancestors read as one path, outermost first", () => {
  expect(
    ancestorPath({
      ancestors: [
        { id: "65537", title: "Engineering" },
        { id: "65540", title: "Payments" },
      ],
    }),
  ).toBe("Engineering › Payments");
  // A page directly under the space home is one segment, and the space's own
  // name is kept: it is what tells two identically-titled runbooks apart.
  expect(
    ancestorPath({ ancestors: [{ id: "65537", title: "Engineering" }] }),
  ).toBe("Engineering");
});

test("a record with no readable ancestors has no path", () => {
  // Every shape a payload can be that this read must miss on, rather than
  // guess at. The first three are what a Jira ticket, a build and a page at
  // the top of its space actually look like.
  for (const payload of [
    { fields: { summary: "Retry failed SEPA payouts" } },
    { buildType: { projectId: "Payout" } },
    { ancestors: [] },
    { ancestors: "Engineering" },
    {
      ancestors: [{ id: "1" }, { id: "2", title: "  " }, { id: "3", title: 7 }],
    },
    {},
    null,
    undefined,
    "a payload that is not an object",
  ]) {
    expect(
      ancestorPath(payload),
      JSON.stringify(payload) ?? "undefined",
    ).toBeNull();
  }
});

test("an unreadable segment is dropped and the rest still form a path", () => {
  // Half a path is still where the page lives; discarding the whole thing
  // because one ancestor lost its title would hide the space as well.
  expect(
    ancestorPath({
      ancestors: [
        { id: "1", title: "Engineering" },
        { id: "2" },
        { id: "3", title: "Payments" },
      ],
    }),
  ).toBe("Engineering › Payments");
});
