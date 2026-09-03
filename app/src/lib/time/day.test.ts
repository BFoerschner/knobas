/**
 * The day review's arithmetic (issue #279).
 *
 * Everything here is pure, and it is a module of its own for the reason
 * `shell/timer.ts` is: the two rules with real content — **which instants a
 * day is** and **where the gaps are** — can then be driven without a mounted
 * component, a bridge or a clock.
 *
 * Every fixture builds its instants with the *local* `Date` constructor and
 * hands the helpers the ISO string that produces. That is not a convenience:
 * a day is a local thing, these tests run in whatever timezone the machine is
 * set to, and a fixture written as `"2026-09-03T09:00:00Z"` would assert
 * `"09:00"` only in UTC.
 */
import { expect, test } from "vitest";

import type { Block, DayBlock } from "../ipc/time";
import {
  clockReading,
  dayBounds,
  dayKey,
  dayLabel,
  durationReading,
  segmentsOf,
  shiftDay,
} from "./day";

/** A local instant, as the wire spells it. */
function at(hour: number, minute = 0, day = 3): string {
  return new Date(2026, 8, day, hour, minute, 0, 0).toISOString();
}

function block(id: number, from: string, to: string, over: Partial<Block> = {}): DayBlock {
  return {
    block: {
      id,
      started_at: from,
      ended_at: to,
      target: { kind: "label", label: "DB config for the migration" },
      kind: "manual",
      ended_by_relaunch: false,
      worklog_id: null,
      ...over,
    },
    title: null,
  };
}

test("a day is named by the reader's own calendar, not by UTC's", () => {
  expect(dayKey(new Date(2026, 8, 3, 23, 30))).toBe("2026-09-03");
  // Single digits are padded, or `#/time/2026-9-3` is an address the parser
  // rejects and the view then silently shows today for.
  expect(dayKey(new Date(2026, 0, 5, 0, 5))).toBe("2026-01-05");
});

test("a day runs from local midnight to the next local midnight", () => {
  const { from, to } = dayBounds("2026-09-03");
  expect(from).toEqual(new Date(2026, 8, 3, 0, 0, 0, 0));
  expect(to).toEqual(new Date(2026, 8, 4, 0, 0, 0, 0));
});

/**
 * The reason the boundaries are computed with `Date` rather than from a UTC
 * offset the webview could have sent the backend: a day containing a DST
 * change is 23 or 25 hours long, and an offset is a single number that is
 * right on one edge of it and wrong on the other.
 *
 * Asserted as *"the next midnight is the next midnight"* rather than as a
 * count of hours, because the machine running this may be in a zone with no
 * DST at all — in which case the day is 24 hours and the test still says
 * something true.
 */
test("a day's end is the next day's midnight however many hours that is", () => {
  for (const day of ["2026-03-29", "2026-10-25"]) {
    const { from, to } = dayBounds(day);
    expect(to.getHours()).toBe(0);
    expect(to.getDate()).toBe(from.getDate() + 1);
  }
});

test("the neighbouring days are a day either side, across a month end", () => {
  expect(shiftDay("2026-09-03", 1)).toBe("2026-09-04");
  expect(shiftDay("2026-09-01", -1)).toBe("2026-08-31");
  expect(shiftDay("2026-12-31", 1)).toBe("2027-01-01");
});

test("a day reads as a date a person can check against a calendar", () => {
  expect(dayLabel("2026-09-03")).toBe("Thursday 3 September 2026");
});

test("an instant reads as the clock the reader was looking at", () => {
  expect(clockReading(at(9, 5))).toBe("09:05");
  expect(clockReading(at(17, 45))).toBe("17:45");
});

test("a span reads in hours and minutes, and never in bare minutes past an hour", () => {
  expect(durationReading(0)).toBe("0 min");
  expect(durationReading(45)).toBe("45 min");
  expect(durationReading(60)).toBe("1 h");
  expect(durationReading(135)).toBe("2 h 15 min");
});

// -- the strip ---------------------------------------------------------------

test("blocks are laid out in time order whatever order they arrive in", () => {
  const segments = segmentsOf([block(2, at(13), at(14)), block(1, at(9), at(10))]);
  expect(segments.map((s) => (s.kind === "block" ? s.block.block.id : "gap"))).toEqual([
    1,
    "gap",
    2,
  ]);
});

/**
 * **The load-bearing distinction of this whole file.** A gap is time nobody
 * accounted for; a zero-width block is a block — a timer that was started and
 * stopped inside a second, or one the relaunch sweep closed before its first
 * heartbeat (migration `0013` allows both, and `time_ipc.rs` witnesses the
 * second). They are opposites, and a layout that emitted a gap of no length,
 * or dropped a block of no length, would draw one as the other.
 *
 * So both are asserted at once, on one strip: the zero-width block is present
 * as a block, and the only gap is the real one beside it.
 */
test("a zero-width block is a block, and a gap of no length is not a gap", () => {
  const segments = segmentsOf([
    block(1, at(9), at(10)),
    // Butts straight onto the first: no gap between them, however tempting a
    // `>=` would be.
    block(2, at(10), at(10)),
    block(3, at(11), at(12)),
  ]);

  expect(segments).toHaveLength(4);
  expect(segments.map((s) => s.kind)).toEqual(["block", "block", "gap", "block"]);

  const zero = segments[1]!;
  expect(zero.kind === "block" ? zero.block.block.id : null).toBe(2);
  const gap = segments[2]!;
  expect(gap.kind === "gap" ? gap.minutes : null).toBe(60);
});

test("a gap spans exactly the time between the blocks either side of it", () => {
  const segments = segmentsOf([block(1, at(9), at(10)), block(2, at(10, 25), at(11))]);
  const gap = segments[1]!;
  expect(gap.kind).toBe("gap");
  expect(gap.kind === "gap" ? gap.from : null).toBe(at(10));
  expect(gap.kind === "gap" ? gap.to : null).toBe(at(10, 25));
  expect(gap.kind === "gap" ? gap.minutes : null).toBe(25);
});

/**
 * The strip is what happened between the first block and the last, so it
 * opens and closes on a block. Time before the day's first block and after
 * its last is not an unaccounted gap — it is the rest of a person's life, and
 * drawing it would make every strip start with an eight-hour hole.
 */
test("the strip begins and ends on a block, never on a gap", () => {
  const segments = segmentsOf([block(1, at(9), at(10)), block(2, at(14), at(15))]);
  expect(segments[0]!.kind).toBe("block");
  expect(segments[segments.length - 1]!.kind).toBe("block");
});

/**
 * Overlapping blocks make no gap. Nothing writes one today — one timer, one
 * block at a time — but #279's own editor is the first thing that can: moving
 * a start back over the block before it is one drag. A layout that measured
 * the gap from the *previous* block rather than from the furthest end so far
 * would draw a negative gap between them.
 */
test("blocks that overlap leave no gap between them", () => {
  const segments = segmentsOf([
    block(1, at(9), at(12)),
    block(2, at(10), at(11)),
    block(3, at(12), at(13)),
  ]);
  expect(segments.map((s) => s.kind)).toEqual(["block", "block", "block"]);
});

test("an empty day is an empty strip, not a strip of one long gap", () => {
  expect(segmentsOf([])).toEqual([]);
});
