/**
 * `KNOBAS_TEST_CLOCK` is a fact, not an intention: `just test-midnight` passes
 * `00:10`, and this is the one test that fails if `test-setup.ts` stopped
 * pinning `Date` to it. Without the variable the pin is off and `Date` is the
 * machine's, which the second test states so that a run can tell the two
 * apart in its own output.
 */
import { expect, test } from "vitest";

const CLOCK = process.env.KNOBAS_TEST_CLOCK;

test.runIf(CLOCK)(`the clock is pinned to ${CLOCK} today, and keeps running`, async () => {
  const [hours, minutes] = CLOCK!.split(":").map(Number);
  const before = new Date();
  expect([before.getHours(), before.getMinutes()]).toEqual([hours, minutes]);
  expect(before.toDateString()).toBe(new Date(Date.now()).toDateString());
  // Only `Date` is faked, and it advances with real time: a timer still
  // fires, and the clock has moved when it does (the fake advances every
  // 20 ms, so the wait is longer than one tick).
  await new Promise((resolve) => setTimeout(resolve, 60));
  expect(Date.now()).toBeGreaterThan(before.getTime());
});

test.skipIf(CLOCK)("the clock is the machine's (KNOBAS_TEST_CLOCK is unset)", () => {
  expect(Math.abs(Date.now() - new Date().getTime())).toBeLessThan(1000);
});
