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
  const info = {
    id: "incident",
    label: "Incident",
    plural: "Incidents",
    monogram: "IN",
    full_sync_exhaustive: true,
  };
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
  const info = {
    id: "ticket",
    label: "Issue",
    plural: "Issues",
    monogram: "IS",
    full_sync_exhaustive: true,
  };
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

/**
 * §3a's claim at the tile level: an adapter's own plural reaches the room.
 *
 * The resolver is optional, so a caller with no registry falls through to
 * knobas' own vocabulary and then to the humaniser. What must not happen is
 * the resolver being *accepted and ignored*, which looks identical from the
 * outside until a source ships a kind whose humanised plural is wrong.
 */
test("an open kind's tile takes the adapter's declared plural", () => {
  const declared = (kind: string) =>
    kind === "incident"
      ? {
          id: "incident",
          label: "Outage",
          plural: "Outages",
          monogram: "OU",
          full_sync_exhaustive: true,
        }
      : null;

  const [tile] = tilesFor(["incident"], declared);
  expect(tile?.label).toBe("Outages");
  // Without the resolver: the humaniser, which is right about the grammar and
  // wrong about the word.
  expect(tilesFor(["incident"])[0]?.label).toBe("Incidents");
});

test("a declared kind that already has a bucket keeps the bucket's heading", () => {
  const declared = () => ({
    id: "ticket",
    label: "Issue",
    plural: "Issues",
    monogram: "IS",
    full_sync_exhaustive: false,
  });
  // `Tickets` is the *bucket* — a designed room region that can hold several
  // kinds — and it is not one adapter's to rename.
  expect(tilesFor(["ticket"], declared)[0]?.label).toBe("Tickets");
});


/**
 * The Kuma room, drawn from nothing but the descriptor (#442).
 *
 * `monitor` is deliberately absent from both the bucket map and knobas' own
 * vocabulary above: it is a kind one adapter emits, and §3a's promise is that
 * such a kind gets grouped, chipped and labelled with no UI work at all. So
 * this is the promise, spelled with the values `knobas-source-kuma`'s
 * descriptor actually declares — a tile of its own, the adapter's plural on it,
 * and the adapter's monogram on the chip.
 *
 * The failure it guards is the tempting one: adding `monitor` to `VOCABULARY`
 * above "so the Kuma room reads well". That is the per-adapter table §3a
 * forbids, and it would pass every test here while making the *next* source's
 * kind the thing nobody remembered to add.
 */
test("a monitor is tiled, labelled and chipped from the Kuma descriptor alone", () => {
  const declared = (kind: string) =>
    kind === "monitor"
      ? {
          id: "monitor",
          label: "Monitor",
          plural: "Monitors",
          monogram: "MO",
          full_sync_exhaustive: true,
        }
      : null;

  const [tile] = tilesFor(["monitor"], declared);
  expect(tile).toEqual({ id: "monitor", label: "Monitors", kinds: ["monitor"] });
  expect(kindSingular("monitor", declared("monitor"))).toBe("Monitor");
  expect(kindMonogram("monitor", declared("monitor"))).toBe("MO");

  // **These two prove nothing about the declaration, and are here so that
  // nobody mistakes them for proof.** With no resolver the generic layer
  // answers `MO` (the first two letters) and `Monitors` (the humaniser), which
  // is the same answer — so a `kindMonogram("monitor")` in some other test
  // would go on passing after the descriptor stopped declaring anything. The
  // assertions above, with `declared` handed in, are the ones that can fail.
  expect(kindMonogram("monitor")).toBe("MO");
  expect(kindLabel("monitor")).toBe("Monitors");
});
