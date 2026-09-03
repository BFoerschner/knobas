/**
 * The reader's own reckoning of time — the two values `worklog_draft` cannot
 * work out for itself (#280).
 *
 * Both are one line long and both are wrong in a way nothing on screen would
 * explain: a day that is UTC's rather than the reader's silently drops the
 * block they just closed, and an offset with the wrong sign moves the day
 * boundary hours in the wrong direction.
 */
import { afterEach, expect, test, vi } from "vitest";

import { localDay, offsetMinutes } from "./draft";

afterEach(() => {
  vi.useRealTimers();
});

/**
 * The day is the **local** one, which is the whole point.
 *
 * `2026-09-03T23:10:00Z` is still the 3rd in UTC and already the 4th for a
 * reader east of it; `toISOString().slice(0, 10)` would answer the 3rd for
 * both. The assertion is written against the environment's own reading of the
 * date rather than a hardcoded string, so it says "these agree" in whatever
 * zone the test runs in -- which is the property, and the alternative is a
 * test that only passes in one timezone.
 */
test("a day is the reader's own, not UTC's", () => {
  const late = new Date("2026-09-03T23:10:00Z");
  const parts = [late.getFullYear(), late.getMonth() + 1, late.getDate()];
  expect(localDay(late)).toBe(
    `${parts[0]}-${String(parts[1]).padStart(2, "0")}-${String(parts[2]).padStart(2, "0")}`,
  );
});

/** Zero-padded, and never locale-shaped: `NaiveDate` parses one of those. */
test("a day is YYYY-MM-DD, padded", () => {
  expect(localDay(new Date(2026, 0, 5, 12))).toBe("2026-01-05");
  expect(localDay(new Date(2026, 10, 30, 12))).toBe("2026-11-30");
  expect(localDay(new Date(2026, 0, 5, 12))).toMatch(/^\d{4}-\d{2}-\d{2}$/);
});

/**
 * East of UTC is **positive**, which is the opposite of what the platform
 * answers.
 *
 * `getTimezoneOffset` is defined as the minutes to add to local time to reach
 * UTC, so Berlin in summer is `-120` there and `+120` here. A sign error is a
 * day boundary two hours the wrong way, every day, silently.
 */
test("the offset is positive east of UTC", () => {
  const berlin = { getTimezoneOffset: () => -120 } as Date;
  expect(offsetMinutes(berlin)).toBe(120);

  const chicago = { getTimezoneOffset: () => 300 } as Date;
  expect(offsetMinutes(chicago)).toBe(-300);

  const utc = { getTimezoneOffset: () => 0 } as Date;
  expect(offsetMinutes(utc)).toBe(0);
});
