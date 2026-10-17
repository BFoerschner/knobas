import { expect, test } from "vitest";
import { parseTfHash } from "../tf/route";
import { navigationActions } from "./actions";

/**
 * #1085: *All work* went to `#/ctx/all`, a room the ticket-first shell does
 * not have, and *Sources* and *First-run setup* to views it never built.
 */
test("every > action goes to a page the ticket-first shell draws", () => {
  const actions = navigationActions();
  expect(actions.map((action) => [action.label, action.hash])).toEqual([
    ["Tickets", "#/tickets"],
    ["Sources", "#/settings/sources"],
  ]);
  for (const action of actions) expect(parseTfHash(action.hash).view).not.toBe("unknown");
});
