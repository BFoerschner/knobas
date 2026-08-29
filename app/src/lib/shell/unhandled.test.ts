/**
 * The repo-wide unhandled-rejection guard, tested against itself (issue #103).
 *
 * The guard's whole claim is that a rejection nobody caught stops being an
 * unattributed line at the end of the run and becomes a failure of the test
 * that caused it. That claim rests on one fact — the recorder actually sees a
 * rejection — and this file is where that fact is checked, because every other
 * test in the repo can only ever observe the recorder staying *empty*.
 *
 * A test that leaks on purpose is the only test allowed to clear the record:
 * `takeUnhandled()` hands the leak over and empties the list, so the shared
 * `afterEach` in `test-setup.ts` sees nothing left and this file does not fail
 * on the rejection it exists to provoke.
 */
import { expect, test } from "vitest";

import { settleRejections, takeUnhandled } from "./unhandled";

test("a rejection nobody caught is recorded, and naming it clears the record", async () => {
  const boom = new Error("nobody is holding this");
  void Promise.reject(boom);

  await settleRejections();

  expect(takeUnhandled(), "the shared recorder did not see an unhandled rejection").toEqual([boom]);
  // …and the take emptied it, which is the half that keeps this file green.
  expect(takeUnhandled()).toEqual([]);
});

/**
 * A promise whose handler is attached a tick late is not a leak.
 *
 * Node emits `unhandledRejection` at the end of the turn in which the promise
 * rejected, and `rejectionHandled` later if somebody does take it. Recording
 * only the first would make the guard fire on ordinary code — an `await` that
 * follows a `void`-ed call, a retry that attaches its `catch` after an
 * intervening microtask — and a guard that cries wolf on working code is
 * removed rather than obeyed.
 */
test("a rejection handled a tick later is not counted as a leak", async () => {
  const late = Promise.reject(new Error("handled, just not immediately"));

  await settleRejections();
  late.catch(() => {});
  await settleRejections();

  expect(takeUnhandled(), "a late-handled rejection was reported as unhandled").toEqual([]);
});
