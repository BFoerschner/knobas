/**
 * A reader's layout overrides, per room, for the session (#245).
 *
 * The store holds departures from a room's default and nothing else: what a
 * room draws is `effectiveMiniBoardLayout`'s answer over this, and the tests
 * for that live in `contexts.test.ts`. What is pinned here is the keying and
 * the lifetime — one override per room id, cleared by choosing the default,
 * and none at all in a store that was just made.
 */
import { expect, test } from "vitest";

import { ALL_CONTEXT, storedContext } from "./contexts";
import { createMiniBoardOverrides } from "./mini-board-overrides.svelte";

const EPIC = storedContext({
  id: "ctx:a",
  kind: "epic",
  title: "SEPA payout retries",
  anchor_id: "jira:EPIC-1",
  created_at: "2026-08-28T12:00:00Z",
  archived_at: null,
});

/** A restart is a fresh store, and a fresh store remembers nothing. */
test("a fresh store has no overrides", () => {
  const overrides = createMiniBoardOverrides();
  expect(overrides.overrideFor(EPIC.id)).toBeUndefined();
  expect(overrides.overrideFor(ALL_CONTEXT.id)).toBeUndefined();
});

test("choosing the other layout records an override for that room only", () => {
  const overrides = createMiniBoardOverrides();
  overrides.choose(EPIC, "stacked");

  expect(overrides.overrideFor(EPIC.id)).toBe("stacked");
  // Keyed by the room, so another room's default is untouched.
  expect(overrides.overrideFor(ALL_CONTEXT.id)).toBeUndefined();
});

/**
 * Choosing the room's own default again clears the override rather than
 * recording it: there is no third "auto" state, and a store of departures
 * must not hold an entry that says "the default".
 */
test("choosing the room's default clears its override", () => {
  const overrides = createMiniBoardOverrides();
  overrides.choose(EPIC, "stacked");
  overrides.choose(EPIC, EPIC.miniBoardLayout);

  expect(overrides.overrideFor(EPIC.id)).toBeUndefined();
});

test("choosing the default where nothing was overridden records nothing", () => {
  const overrides = createMiniBoardOverrides();
  overrides.choose(ALL_CONTEXT, ALL_CONTEXT.miniBoardLayout);

  expect(overrides.overrideFor(ALL_CONTEXT.id)).toBeUndefined();
});
