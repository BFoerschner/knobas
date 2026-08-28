/**
 * The shared readings. Every one of these is a number two surfaces render, so
 * the value of testing them is that the status bar and the `.dbbar` cannot
 * disagree about the same database.
 */
import { expect, test } from "vitest";

import { countdown, formatBytes, formatDuration, formatInterval, runDuration } from "./diagnostics";

test("formatBytes renders db_bytes the way the status bar reads it", () => {
  expect(formatBytes(222_298_112)).toBe("212 MB");
  expect(formatBytes(0)).toBe("0 B");
  expect(formatBytes(512)).toBe("512 B");
  expect(formatBytes(1024)).toBe("1 KB");
  expect(formatBytes(1536)).toBe("1.5 KB");
  expect(formatBytes(1_073_741_824)).toBe("1 GB");
  expect(formatBytes(1_610_612_736)).toBe("1.5 GB");
});

test("formatBytes says nothing rather than NaN for a reading it did not get", () => {
  expect(formatBytes(Number.NaN)).toBe("—");
  expect(formatBytes(-1)).toBe("—");
});

const NOW = new Date("2026-08-25T12:00:00Z");

test("the countdown to the next sync ticks down and never goes negative", () => {
  expect(countdown("2026-08-25T12:04:20Z", NOW)).toBe("4:20");
  expect(countdown("2026-08-25T12:00:05Z", NOW)).toBe("0:05");
  // A scheduler a few seconds behind its own tick is ordinary; "-0:04" is a
  // fact about two clocks rather than about the sync.
  expect(countdown("2026-08-25T11:59:56Z", NOW)).toBe("due");
  expect(countdown("2026-08-25T12:00:00Z", NOW)).toBe("due");
});

test("an unscheduled source has no countdown at all", () => {
  // Null while a run is in flight, and for a source that is disabled or needs
  // a human (contract §2.3).
  expect(countdown(null, NOW)).toBe("—");
  expect(countdown("not a date", NOW)).toBe("—");
});

test("a countdown over an hour keeps its hours rather than reading 75:00", () => {
  expect(countdown("2026-08-25T13:30:00Z", NOW)).toBe("1:30:00");
});

test("a run still in flight has no duration, rather than a duration of zero", () => {
  // `finished_at === null`. A `0` here makes a running sync indistinguishable
  // from an instant success, which is the exact row a reader is staring at
  // when they wonder whether it is stuck.
  expect(runDuration("2026-08-25T11:59:00Z", null)).toBeNull();
  expect(formatDuration(null)).toBe("—");
});

test("a finished run reports the wall time between its two stamps", () => {
  expect(runDuration("2026-08-25T11:59:00Z", "2026-08-25T11:59:12Z")).toBe(12_000);
  expect(formatDuration(12_000)).toBe("12.0 s");
  expect(formatDuration(840)).toBe("840 ms");
  expect(formatDuration(72_000)).toBe("1:12");
});

test("a run whose stamps are out of order reads as instant, not as negative", () => {
  expect(runDuration("2026-08-25T11:59:12Z", "2026-08-25T11:59:00Z")).toBe(0);
});

test("formatInterval says the cadence the way the status bar does", () => {
  expect(formatInterval(300)).toBe("every 5 min");
  expect(formatInterval(900)).toBe("every 15 min");
  expect(formatInterval(3600)).toBe("hourly");
  expect(formatInterval(21_600)).toBe("every 6 h");
  expect(formatInterval(86_400)).toBe("daily");
  expect(formatInterval(0)).toBe("—");
});
