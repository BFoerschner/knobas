/**
 * The Monitors tab's two pieces of arithmetic (#448): the chips' counts and
 * the 24-hour bar's bucketing.
 *
 * Asserted here rather than through the rendered tab because both are drawable
 * when they are wrong — a bar of forty-eight segments looks like a bar of
 * forty-eight segments whatever hour each one is standing for, and a chip
 * reading `up 6` looks like a chip either way. What a component test can see
 * is that the numbers reach the screen; what only this can see is that they
 * are the right numbers.
 */
import { expect, test } from "vitest";

import type { MonitorRow, MonitorSample, UnmonitoredAsset } from "../ipc/assets";
import {
  BAR_BUCKETS,
  BAR_WINDOW_MS,
  bar,
  byType,
  chipCounts,
  chipOf,
  filtered,
  typeCounts,
} from "./monitors";

/** Half an hour: the width of one bucket, and the unit these fixtures place. */
const BUCKET_MS = BAR_WINDOW_MS / BAR_BUCKETS;

/** The instant every fixture below is placed relative to. */
const NOW = Date.parse("2026-09-07T12:00:00Z");

/** A sample `minutes` before {@link NOW}. */
function at(minutes: number, state: string | null): MonitorSample {
  return { taken_at: new Date(NOW - minutes * 60_000).toISOString(), state };
}

/** A roster row carrying only what the chips read. */
function monitor(name: string, state: string | null, tombstoned = false): MonitorRow {
  return {
    entity_id: `kuma:${name}`,
    source_id: "kuma",
    name,
    state,
    monitor_type: "http",
    target: null,
    response_time_ms: null,
    checked_at: new Date(NOW).toISOString(),
    uptime: [],
    cert_days_remaining: null,
    web_url: null,
    tombstoned,
    assets: [],
    samples: [],
  };
}

// ---------------------------------------------------------------------------
// The chips
// ---------------------------------------------------------------------------

/**
 * **Paused beats the state word**, because a monitor Kuma no longer publishes
 * keeps whatever state it was last sampled at: a tombstone still reading `up`
 * is a check nobody is running, and a chip that counted it among the healthy
 * would be the one wrong answer this tab must not give.
 */
test("a tombstoned monitor is paused whatever its last state said", () => {
  expect(chipOf(monitor("gitea", "up", true))).toBe("paused");
  expect(chipOf(monitor("jira", "down", true))).toBe("paused");
  expect(chipOf(monitor("live", "up"))).toBe("up");
});

/**
 * The five states the issue names get their own chip; `maintenance` and a
 * monitor with no reading at all get the sixth.
 *
 * Not dropped, and not folded into one of the five. Kuma's `maintenance` is a
 * real state and a null one is a genuine miss — calling either of them *up*
 * would make the counts a fiction, and leaving them out of every count would
 * make the chips add up to fewer monitors than the list below them holds.
 */
test("every state reaches a chip, and the ones with no chip of their own reach the last", () => {
  expect(chipOf(monitor("a", "up"))).toBe("up");
  expect(chipOf(monitor("b", "warn"))).toBe("warn");
  expect(chipOf(monitor("c", "down"))).toBe("down");
  expect(chipOf(monitor("d", "pending"))).toBe("pending");
  expect(chipOf(monitor("e", "maintenance"))).toBe("other");
  expect(chipOf(monitor("f", null))).toBe("other");
  expect(chipOf(monitor("g", "asleep"))).toBe("other");
});

/**
 * The counts are in the chips' own order and every chip is present, zero
 * included: a chip that vanished when its count reached zero would move the
 * chip beside it under the reader's pointer exactly when something recovered.
 */
test("the counts are one per chip, in order, and they add up to the roster", () => {
  const roster = [
    monitor("a", "up"),
    monitor("b", "up"),
    monitor("c", "warn"),
    monitor("d", "down"),
    monitor("e", "pending"),
    monitor("f", "up", true),
    monitor("g", "maintenance"),
  ];
  expect(chipCounts(roster)).toEqual([
    { state: "up", count: 2 },
    { state: "warn", count: 1 },
    { state: "down", count: 1 },
    { state: "pending", count: 1 },
    { state: "paused", count: 1 },
    { state: "other", count: 1 },
  ]);
  expect(chipCounts(roster).reduce((sum, chip) => sum + chip.count, 0)).toBe(roster.length);
});

/** No filter is the whole roster; a filter is the monitors that chip counts. */
test("a chip filters the list to exactly what it counted", () => {
  const roster = [monitor("a", "up"), monitor("b", "down"), monitor("c", "up", true)];
  expect(filtered(roster, null).map((row) => row.name)).toEqual(["a", "b", "c"]);
  expect(filtered(roster, "up").map((row) => row.name)).toEqual(["a"]);
  expect(filtered(roster, "paused").map((row) => row.name)).toEqual(["c"]);
  expect(filtered(roster, "warn")).toEqual([]);
});

// ---------------------------------------------------------------------------
// The bar
// ---------------------------------------------------------------------------

/**
 * The bar is forty-eight half-hour buckets, oldest on the left, and the last
 * one ends now.
 *
 * Forty-eight is the mockup's own number (`mockups/shared/assets.md`: *"Check
 * interval 60 s; 24 h bar = 1 440 checks (render as a 48-segment bar)"*), and
 * it is why this arithmetic exists at all: at Kuma's own one-minute cadence a
 * segment per sample would be 1 440 rectangles per row.
 */
test("the bar is forty-eight buckets covering the last day, oldest first", () => {
  const drawn = bar([], NOW);
  expect(drawn).toHaveLength(BAR_BUCKETS);
  expect(drawn.at(0)?.from).toBe(NOW - BAR_WINDOW_MS);
  expect(drawn.at(0)?.to).toBe(NOW - BAR_WINDOW_MS + BUCKET_MS);
  expect(drawn.at(-1)?.to).toBe(NOW);
});

/** A sample lands in the bucket holding the half-hour it was taken in. */
test("each sample colours the bucket its instant falls in", () => {
  const drawn = bar([at(10, "up"), at(23 * 60 + 50, "down")], NOW);
  expect(drawn.at(-1)?.state).toBe("up");
  expect(drawn.at(0)?.state).toBe("down");
  expect(drawn.at(-2)?.state).toBeNull();
});

/**
 * **The worst state in a bucket is the bucket's**, because a half hour holding
 * one `down` among twenty-nine `up`s is a half hour something was down — and a
 * bar that averaged it away would be a bar that never shows an outage shorter
 * than thirty minutes, which is most of them.
 */
test("a bucket reads as the worst thing that happened in it", () => {
  const half = BUCKET_MS / 60_000;
  const inLast = (offset: number, state: string | null) => at(half - offset, state);
  expect(bar([inLast(1, "up"), inLast(2, "down"), inLast(3, "up")], NOW).at(-1)?.state).toBe(
    "down",
  );
  expect(bar([inLast(1, "up"), inLast(2, "warn")], NOW).at(-1)?.state).toBe("warn");
  expect(bar([inLast(1, "up"), inLast(2, "pending")], NOW).at(-1)?.state).toBe("pending");
  expect(bar([inLast(1, "up"), inLast(2, "maintenance")], NOW).at(-1)?.state).toBe("maintenance");
  expect(bar([inLast(1, "warn"), inLast(2, "down")], NOW).at(-1)?.state).toBe("down");
});

/**
 * A poll whose state did not resolve is a gap and never a reading — the
 * failure direction `knobas_sync::samples` writes null for. Beside a real
 * reading in the same half hour, the reading wins: something was read.
 */
test("a sample with no state is a gap, and does not erase a reading beside it", () => {
  const half = BUCKET_MS / 60_000;
  expect(bar([at(half - 1, null)], NOW).at(-1)?.state).toBeNull();
  expect(bar([at(half - 1, null), at(half - 2, "up")], NOW).at(-1)?.state).toBe("up");
});

/**
 * **A monitor younger than a day draws what exists.** Two hours of samples
 * fill the four newest buckets and leave the other forty-four empty, which is
 * the honest picture: knobas was not watching, rather than everything being
 * fine.
 */
test("a monitor younger than a day fills only the buckets it has samples for", () => {
  const young = [at(115, "up"), at(85, "up"), at(55, "warn"), at(25, "up")];
  const drawn = bar(young, NOW);
  expect(drawn.filter((bucket) => bucket.state !== null)).toHaveLength(4);
  expect(drawn.slice(0, BAR_BUCKETS - 4).every((bucket) => bucket.state === null)).toBe(true);
  expect(drawn.slice(-4).map((bucket) => bucket.state)).toEqual(["up", "up", "warn", "up"]);
});

/**
 * A sample older than the window draws nothing. The backend already bounds the
 * list to a day on its own clock, so this is the reader's clock disagreeing by
 * however long the tab has been open — and an off-by-one that wrapped it to
 * the newest bucket would put yesterday's outage at the right-hand end.
 */
test("a sample older than the window is drawn nowhere at all", () => {
  const drawn = bar([at(25 * 60, "down"), at(10, "up")], NOW);
  expect(drawn.filter((bucket) => bucket.state !== null)).toHaveLength(1);
  expect(drawn.at(-1)?.state).toBe("up");
});

// -- the "Not monitored" roster's type filter (#449) -------------------------

/** One row of the roster, carrying only what the filter reads. */
function unwatched(name: string, typeId: string, label: string): UnmonitoredAsset {
  return {
    id: `asset:${name}`,
    type_id: typeId,
    type_label: label,
    monogram: label.slice(0, 2).toUpperCase(),
    name,
    path: null,
  };
}

const UNWATCHED = [
  unwatched("hel1", "site", "Site"),
  unwatched("vm-db-01", "vm", "VM"),
  unwatched("vm-web-01", "vm", "VM"),
  unwatched("postgres", "container", "Container"),
];

/**
 * **The filter's options are the types the answer holds**, each with how many
 * of them there are — never the whole nineteen-row type table. An option
 * behind which there is nothing can only empty the list, and a roster of gaps
 * that offered sixteen dead filters would hide the four live ones.
 *
 * Alphabetical by label, and not by count: the chips are found by reading
 * their names, and count order would rearrange them under the reader's pointer
 * every time somebody attached a monitor.
 */
test("the type filter offers the types the roster holds, counted, by label", () => {
  expect(typeCounts(UNWATCHED)).toEqual([
    { type_id: "container", label: "Container", monogram: "CO", count: 1 },
    { type_id: "site", label: "Site", monogram: "SI", count: 1 },
    { type_id: "vm", label: "VM", monogram: "VM", count: 2 },
  ]);
});

/** An empty roster offers no filter at all, rather than a row of zeroes. */
test("a roster with nothing in it offers no type filter", () => {
  expect(typeCounts([])).toEqual([]);
});

/**
 * The filter narrows to exactly what its option counted, and no filter is the
 * whole roster — the same `type_id` on both sides, so a chip reading `VM 2`
 * over a list of one is not a state this can reach.
 */
test("a type narrows the roster to what its option counted", () => {
  expect(byType(UNWATCHED, "vm").map((row) => row.name)).toEqual(["vm-db-01", "vm-web-01"]);
  expect(byType(UNWATCHED, null)).toHaveLength(UNWATCHED.length);
  expect(byType(UNWATCHED, "network")).toEqual([]);
});
