/**
 * The days a digest is asked for (issue #288).
 *
 * The one rule with real content on this side, and it is arithmetic that is
 * wrong twice a year in a way no screenshot shows: eight consecutive **local**
 * midnights, not the first one plus 24n hours. `lib/time/week.test.ts` makes
 * the same assertion about its own seven, and for the same reason.
 */
import { expect, test } from "vitest";

import { digestWindows, LOOKBACK_DAYS } from "./standup";

/**
 * Eight windows: the day being asked about, and the seven before it.
 *
 * Seven is what the backend consults, so sending fewer would narrow the digest
 * and sending more would change nothing — which is why the count is asserted
 * against `LOOKBACK_DAYS` rather than against the number 7 written twice.
 */
test("a digest asks for its own day and the seven before it, oldest first", () => {
  const { today, earlier } = digestWindows("2026-08-31");

  expect(today.day).toBe("2026-08-31");
  expect(earlier).toHaveLength(LOOKBACK_DAYS);
  expect(earlier.map((window) => window.day)).toEqual([
    "2026-08-24",
    "2026-08-25",
    "2026-08-26",
    "2026-08-27",
    "2026-08-28",
    "2026-08-29",
    "2026-08-30",
  ]);
});

/**
 * Contiguous, and each edge that day's own midnight.
 *
 * The contiguity is what stops an hour belonging to no day or to two; the
 * midnights are what make the week containing a daylight-saving change right,
 * because one of its days is 23 or 25 hours long and eight equal spans would
 * put that day's evening on the next day's list.
 */
test("the windows are consecutive local midnights, not eight equal spans", () => {
  const { today, earlier } = digestWindows("2026-08-31");
  const all = [...earlier, today];

  for (let index = 0; index < all.length - 1; index += 1) {
    expect(all[index]!.to).toBe(all[index + 1]!.from);
  }
  expect(new Date(all[0]!.from)).toEqual(new Date(2026, 7, 24, 0, 0, 0, 0));
  expect(new Date(all[all.length - 1]!.to)).toEqual(new Date(2026, 8, 1, 0, 0, 0, 0));
});

/** A month end is `Date`'s to roll, not this module's to compute. */
test("the lookback crosses a month boundary", () => {
  const { earlier } = digestWindows("2026-09-02");
  expect(earlier.map((window) => window.day)).toEqual([
    "2026-08-26",
    "2026-08-27",
    "2026-08-28",
    "2026-08-29",
    "2026-08-30",
    "2026-08-31",
    "2026-09-01",
  ]);
});
