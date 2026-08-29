/**
 * The room switcher's list: the derived rooms (one built-in *All work*, one
 * per configured source) and, since #47, the stored ones — spec §7's contexts
 * (epic / ticket / ad-hoc), whose membership is the fixed one-hop rule over
 * `knobas.confirmed_link` (never the base table — ADR-0008), resolved
 * server-side. What is tested here is the list's shape and order, not the
 * membership rule: that lives in `crates/knobas-core/tests/contexts.rs`.
 */
import { expect, test } from "vitest";

import { ALL_CONTEXT, builtinContexts, contextById, storedContext, switcherContexts } from "./contexts";

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
    { id: "src:jira", label: "Jira", kindWord: "source", filter: { sources: ["jira"], context: null } },
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

/**
 * The switcher's assembly since #47: *All work* first, then the stored
 * contexts in the order `list_contexts` answered, then the source rooms —
 * the rooms a person made on purpose come before the raw feeds.
 */
test("stored contexts sit between All work and the source rooms", () => {
  const cs = switcherContexts(
    [
      {
        id: "ctx:b",
        kind: "adhoc",
        title: "Staging DB configuration",
        anchor_id: null,
        created_at: "2026-08-29T12:00:00Z",
        archived_at: null,
      },
      {
        id: "ctx:a",
        kind: "epic",
        title: "SEPA payout retries",
        anchor_id: "jira:EPIC-1",
        created_at: "2026-08-28T12:00:00Z",
        archived_at: null,
      },
    ],
    [{ id: "jira", label: "Tidewater Jira" }],
  );

  expect(cs.map((c) => c.id)).toEqual(["all", "ctx:b", "ctx:a", "src:jira"]);
  // A stored room scopes by membership, never by source.
  expect(cs[1]?.filter).toEqual({ sources: [], context: "ctx:b" });
  expect(cs[2]?.kindWord).toBe("epic");
});

/** `adhoc` is the column's spelling; the chip reads as a word. */
test("an ad-hoc context's chip says ad-hoc", () => {
  const room = storedContext({
    id: "ctx:x",
    kind: "adhoc",
    title: "x",
    anchor_id: null,
    created_at: "2026-08-29T12:00:00Z",
    archived_at: null,
  });
  expect(room.kindWord).toBe("ad-hoc");
  expect(room.label).toBe("x");
});
