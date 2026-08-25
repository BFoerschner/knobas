/**
 * The mockup's `ago()` (`signal-miller.html:778`) as a tested function.
 *
 * Every row in the app carries one of these — spec §4's per-row provenance —
 * so its edges are worth pinning: a clock a few seconds ahead must not print
 * "-1 min ago", and a stamp old enough to be a date must stop pretending to be
 * a duration.
 */
import { expect, test } from "vitest";

import { ago } from "./time";

const now = new Date("2026-08-22T14:32:00Z");

test.each([
  ["2026-08-22T14:31:40Z", "just now"],
  ["2026-08-22T14:28:00Z", "4 min ago"],
  ["2026-08-22T13:33:00Z", "59 min ago"],
  ["2026-08-22T13:32:00Z", "1 h ago"],
  ["2026-08-22T11:32:00Z", "3 h ago"],
  ["2026-08-19T09:00:00Z", "2026-08-19"],
  // Clock skew: knobas' `synced_at` is the local clock and a source's
  // `updated_at` is a remote one, so a stamp in the future is ordinary.
  ["2026-08-22T14:33:00Z", "just now"],
  ["2026-09-01T00:00:00Z", "just now"],
])("%s -> %s", (iso, want) => expect(ago(iso, now)).toBe(want));

/**
 * The boundary is a day, and it is crossed in one direction only.
 *
 * 23 h is still a duration; 25 h is a date. Both, because a cutoff that had
 * been dropped would keep the first assertion green for ever.
 */
test("a day is where a duration becomes a date", () => {
  expect(ago("2026-08-21T15:32:00Z", now)).toBe("23 h ago");
  expect(ago("2026-08-21T13:32:00Z", now)).toBe("2026-08-21");
});

/**
 * `updated_at` is nullable and `payload` values are whatever a source sent.
 * A timestamp the app cannot read is an em dash, not `Invalid Date`.
 */
test("what cannot be read renders as an em dash", () => {
  expect(ago("not a date", now)).toBe("—");
  expect(ago("", now)).toBe("—");
  expect(ago(null, now)).toBe("—");
});
