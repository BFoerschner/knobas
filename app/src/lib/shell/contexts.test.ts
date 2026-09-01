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
    {
      id: "src:jira",
      label: "Jira",
      kindWord: "source",
      filter: { sources: ["jira"], context: null, project: null },
      miniBoardLayout: "stacked" as const,
    },
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
  expect(cs[1]?.filter).toEqual({ sources: [], context: "ctx:b", project: null });
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

/**
 * A project room sits **immediately after its own source's room** (#209), so
 * the switcher reads as *All work*, the stored contexts, then each source
 * followed by the projects inside it.
 */
test("each project gets a room directly under its own source's", () => {
  const cs = builtinContexts(
    [
      { id: "jira", label: "Tidewater Jira" },
      { id: "gitea", label: "Gitea" },
    ],
    [
      { source_id: "jira", key: "OPS", name: "Operations" },
      { source_id: "jira", key: "PAY", name: "Payments Platform" },
    ],
  );

  expect(cs.map((c) => c.id)).toEqual([
    "all",
    "src:jira",
    "proj:jira:OPS",
    "proj:jira:PAY",
    "src:gitea",
  ]);
});

/**
 * A project room's filter names **both** halves.
 *
 * The key alone is not an identity: it is unique only inside its own source,
 * so a room that narrowed by `project` and left `sources` empty would draw
 * another source's `PAY` work as well as its own.
 */
test("a project room narrows by its project within its own source", () => {
  const cs = builtinContexts(
    [{ id: "jira", label: "Tidewater Jira" }],
    [{ source_id: "jira", key: "PAY", name: "Payments Platform" }],
  );

  expect(cs[2]?.filter).toEqual({ sources: ["jira"], context: null, project: "PAY" });
  expect(cs[2]?.label).toBe("Payments Platform");
  expect(cs[2]?.kindWord).toBe("project");
});

/**
 * A key with no readable name is still reachable, labelled by its key.
 *
 * The fallback is the shell's on purpose: `Project.name` is `null` because the
 * backend refuses to invent one (#208), and a room the switcher could not name
 * would be a project nobody could stand in.
 */
test("a project with no readable name is labelled by its key", () => {
  const cs = builtinContexts(
    [{ id: "jira", label: "Tidewater Jira" }],
    [{ source_id: "jira", key: "PAY", name: null }],
  );

  expect(cs[2]?.label).toBe("PAY");
  expect(cs[2]?.id).toBe("proj:jira:PAY");
});

/**
 * Two sources that happen to use one key are two projects, two rooms and two
 * addresses — one source's work never leaks into the other's room.
 */
test("one key in two sources is two rooms with two addresses", () => {
  const cs = builtinContexts(
    [
      { id: "jira", label: "Tidewater Jira" },
      { id: "teamcity", label: "TeamCity" },
    ],
    [
      { source_id: "jira", key: "PAY", name: "Payments Platform" },
      { source_id: "teamcity", key: "PAY", name: "Payments pipelines" },
    ],
  );

  expect(cs.map((c) => c.id)).toEqual([
    "all",
    "src:jira",
    "proj:jira:PAY",
    "src:teamcity",
    "proj:teamcity:PAY",
  ]);
  expect(cs[2]?.filter.sources).toEqual(["jira"]);
  expect(cs[4]?.filter.sources).toEqual(["teamcity"]);
});

/**
 * A source whose corpus reports no projects — which is what a **disabled**
 * source is, since migration `0012` took its items out of `sync.live_item`
 * (#202, #203; pinned backend-side by `a_disabled_source_shows_no_projects`)
 * — offers no project rooms, while its own room behaves as it always has.
 *
 * The switcher does not check for it: the census is the only thing that says
 * which projects exist, so "no rows for that source" is the whole mechanism.
 */
test("a source the census reports no projects for offers no project rooms", () => {
  const cs = builtinContexts(
    [
      { id: "jira", label: "Tidewater Jira" },
      { id: "gitea", label: "Gitea" },
    ],
    [{ source_id: "jira", key: "PAY", name: "Payments Platform" }],
  );

  expect(cs.map((c) => c.id)).toEqual(["all", "src:jira", "proj:jira:PAY", "src:gitea"]);
});

/**
 * ...and a project reported for a source that has no room here gets none
 * either, rather than being appended somewhere arbitrary.
 *
 * "Immediately after its own source's room" has no answer when there is no
 * such room, and a room under nothing is a room a reader cannot place.
 */
test("a project whose source has no room is not offered one", () => {
  const cs = builtinContexts(
    [{ id: "jira", label: "Tidewater Jira" }],
    [
      { source_id: "jira", key: "PAY", name: "Payments Platform" },
      { source_id: "gone", key: "OLD", name: "Retired" },
    ],
  );

  expect(cs.map((c) => c.id)).toEqual(["all", "src:jira", "proj:jira:PAY"]);
});

/**
 * A bookmarked project address outlives the project: a room exists exactly as
 * long as the corpus shows it, so the address for one it no longer shows lands
 * in *All work* — **by identity**, the existing rule for the existing reason.
 */
test("an address for a project the corpus no longer shows lands in All work", () => {
  const cs = builtinContexts(
    [{ id: "jira", label: "Tidewater Jira" }],
    [{ source_id: "jira", key: "PAY", name: "Payments Platform" }],
  );

  expect(contextById("proj:jira:PAY", cs).label).toBe("Payments Platform");
  expect(contextById("proj:jira:OPS", cs).id).toBe("all");
  // ...and not merely `cs[0]`: with *All work* moved off the front, a fallback
  // by position would answer the first project room it happened to find.
  expect(contextById("proj:jira:OPS", [...cs].reverse()).id).toBe("all");
});

/** The whole switcher: *All work*, the stored rooms, then sources with their projects. */
test("stored contexts still sit between All work and the source rooms with projects", () => {
  const cs = switcherContexts(
    [
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
    [{ source_id: "jira", key: "PAY", name: "Payments Platform" }],
  );

  expect(cs.map((c) => c.id)).toEqual(["all", "ctx:a", "src:jira", "proj:jira:PAY"]);
});

/**
 * Which layout a room's mini board draws, per room kind (#210).
 *
 * The distinction is boundedness, not size: *All work* and a source room hold
 * whatever synced, so they hold whatever workflows synced, and a horizontal
 * strip of columns is the wrong shape for them however few statuses they show
 * today. A project room and a stored context are bounded, so they keep the
 * column layout the tile was designed for.
 */
test("the unbounded rooms draw stacked and the bounded ones draw columns", () => {
  const cs = switcherContexts(
    [
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
    [{ source_id: "jira", key: "PAY", name: "Payments Platform" }],
  );

  expect(cs.map((c) => [c.id, c.miniBoardLayout])).toEqual([
    ["all", "stacked"],
    ["ctx:a", "columns"],
    ["src:jira", "stacked"],
    ["proj:jira:PAY", "columns"],
  ]);
});
