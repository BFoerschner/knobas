/**
 * The room switcher's list, which in M1 is derived rather than stored.
 *
 * Spec §7's contexts (epic / ticket / ad-hoc, membership through links) need
 * `knobas.link`, which M1 never writes. What is here instead is one built-in
 * room plus one per configured source — the same shape, one prop away from
 * reading `knobas.context` when M2 lands.
 */
import { expect, test } from "vitest";

import { ALL_CONTEXT, builtinContexts, contextById } from "./contexts";

test("always offers All work first, then one room per source", () => {
  const cs = builtinContexts([
    { id: "jira", label: "Tidewater Jira" },
    { id: "gitea", label: "Gitea" },
  ]);

  expect(cs.map((c) => c.id)).toEqual(["all", "src:jira", "src:gitea"]);
  expect(cs[0]?.filter.sources).toEqual([]);
  expect(cs[1]?.filter.sources).toEqual(["jira"]);
  expect(cs[1]?.label).toBe("Tidewater Jira");
});

/**
 * The `all` room's filter is *empty*, not "every known source".
 *
 * `EntityFilter.sources` is unfiltered when empty, and listing the sources
 * knobas happens to know about would silently hide anything synced by a source
 * that has no configuration row (interfaces §1: `run_once` syncs those).
 */
test("All work filters by nothing at all, however many sources exist", () => {
  const cs = builtinContexts([{ id: "jira", label: "Jira" }]);
  expect(cs[0]?.filter.sources).toEqual([]);
});

test("an unknown context id falls back to All work rather than blanking the room", () => {
  expect(contextById("src:gone", builtinContexts([])).id).toBe("all");
});

/**
 * ...and to *All work*, not merely to the first entry.
 *
 * With a source room present, "the first one" and "the all room" happen to be
 * the same context, which is exactly how a fallback that returns `contexts[0]`
 * passes the test above while sending a reader into an arbitrary room the day
 * the order changes.
 */
test("the fallback is All work by identity, not by position", () => {
  const cs = [
    { id: "src:jira", label: "Jira", kindWord: "source", filter: { sources: ["jira"] } },
    ALL_CONTEXT,
  ];
  expect(contextById("src:gone", cs).id).toBe("all");
  // ...and a context that *is* there is still found wherever it sits.
  expect(contextById("src:jira", cs).id).toBe("src:jira");
});

/** A room with no contexts at all still resolves to something renderable. */
test("an empty list still yields All work", () => {
  expect(contextById("all", []).label).toBe("All work");
});
