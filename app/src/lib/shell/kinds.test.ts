/**
 * Which tiles a room draws, and what a kind is called.
 *
 * §3a is the constraint: knobas may not carry a table of every kind every
 * adapter will ever emit. So the mockup's designed rooms are kept as a
 * *bucket map* and anything undeclared gets its own tile — a new ticket system
 * is browsable on day one, in a room that still looks like the mockup for the
 * kinds it was drawn for.
 */
import { expect, test } from "vitest";

import { kindLabel, kindMonogram, kindSingular, tilesFor } from "./kinds";

test("known kinds land in the mockup's tiles", () => {
  const tiles = tilesFor(["ticket", "pr", "commit", "build", "page"]);
  expect(tiles.map((t) => t.id)).toEqual(["tickets", "code", "builds", "docs"]);
  expect(tiles.find((t) => t.id === "code")?.kinds).toEqual(["pr", "commit"]);
});

/**
 * A bucket carries only the kinds that are actually present.
 *
 * The tile fetches `spec.kinds`, so a bucket that always listed all four code
 * kinds would send `branch` and `repo` to a corpus that has none — harmless
 * here, and exactly the habit that turns into a room querying every kind a
 * future adapter might emit.
 */
test("a tile lists the kinds present, not the kinds its bucket could hold", () => {
  expect(tilesFor(["commit"]).find((t) => t.id === "code")?.kinds).toEqual(["commit"]);
});

/** A tile with nothing in it is not drawn at all. */
test("a bucket no kind fills produces no tile", () => {
  expect(tilesFor(["ticket"]).map((t) => t.id)).toEqual(["tickets"]);
  expect(tilesFor([])).toEqual([]);
});

test("a kind no tile claims gets its own tile (open kinds, §3a)", () => {
  const tiles = tilesFor(["ticket", "incident"]);
  expect(tiles.map((t) => t.id)).toEqual(["tickets", "incident"]);
  expect(tiles[1]?.label).toBe("Incidents");
  expect(tiles[1]?.kinds).toEqual(["incident"]);
});

/** Declared buckets first, whatever order the corpus reported its kinds in. */
test("the designed rooms keep their order and the open kinds follow", () => {
  const tiles = tilesFor(["incident", "page", "ticket", "outage"]);
  expect(tiles.map((t) => t.id)).toEqual(["tickets", "docs", "incident", "outage"]);
});

/** A kind reported twice is one tile, not two. */
test("duplicate kinds collapse", () => {
  expect(tilesFor(["incident", "incident"]).map((t) => t.id)).toEqual(["incident"]);
  expect(tilesFor(["pr", "pr"]).find((t) => t.id === "code")?.kinds).toEqual(["pr"]);
});

test("labels and monograms come from the adapter when it declares them", () => {
  const info = { id: "incident", label: "Incident", plural: "Incidents", monogram: "IN" };
  expect(kindMonogram("incident", info)).toBe("IN");
  expect(kindMonogram("incident", null)).toBe("IN"); // fallback: first two letters, upper
  expect(kindLabel("build_config", null)).toBe("Build configs");
  expect(kindSingular("build_config", null)).toBe("Build config");
});

/**
 * The adapter wins over knobas' own vocabulary, not the other way round.
 *
 * Two adapters may both emit `ticket` and disagree about what to call it;
 * whoever emitted the row is the one who gets to name it (§3a).
 */
test("a declared kind_info overrides the built-in vocabulary", () => {
  const info = { id: "ticket", label: "Issue", plural: "Issues", monogram: "IS" };
  expect(kindLabel("ticket", info)).toBe("Issues");
  expect(kindSingular("ticket", info)).toBe("Issue");
  expect(kindMonogram("ticket", info)).toBe("IS");
  // ...and without one, knobas' own vocabulary rather than a naive plural.
  expect(kindLabel("pr", null)).toBe("Pull requests");
  expect(kindLabel("pr", null)).not.toBe("Prs");
  expect(kindMonogram("pr", null)).toBe("PR");
});

/** Pluralising has to survive the words a source system actually uses. */
test("the generic plural handles the endings that are not just +s", () => {
  expect(kindLabel("story", null)).toBe("Stories");
  expect(kindLabel("patch", null)).toBe("Patches");
  expect(kindLabel("release", null)).toBe("Releases");
  expect(kindLabel("bus", null)).toBe("Buses");
});

/** A kind of one character still yields something renderable. */
test("a one-character kind still has a monogram", () => {
  expect(kindMonogram("x", null)).toBe("X");
  expect(kindMonogram("", null)).toBe("?");
});
