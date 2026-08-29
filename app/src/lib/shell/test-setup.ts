/**
 * What every suite gets whether it asks or not: the jsdom the shell expects,
 * and the guard against a green file that has leaked a rejection.
 *
 * Loaded by `vite.config.ts`'s `test.setupFiles`, so every suite gets it
 * without importing anything. That "without opting in" is the point of the
 * second half: a file-local guard protects the one file that remembered it,
 * and the file that leaks is by definition the one that did not (#103).
 */
import { afterAll, afterEach, expect } from "vitest";

import { settleRejections, takeUnhandled, watchRejections } from "./unhandled";

// jsdom has no `matchMedia`, and the shell asks it about reduced motion the
// moment a `Flap` mounts. Without this a component test fails inside Svelte's
// own effect runner, which reports it as an unrelated mount error.
if (!window.matchMedia) {
  Object.defineProperty(window, "matchMedia", {
    writable: true,
    configurable: true,
    value: (query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    }),
  });
}

/**
 * Recording starts at import time, not in a `beforeAll`.
 *
 * Setup files run before the test file's own module graph is imported, so a
 * top-level `await import()` that rejects — which several suites here use to
 * pull a component in after `vi.mock` — is inside the window this covers.
 */
const stopWatching = watchRejections();

/**
 * Every test in every file: a rejection nobody caught fails the run without
 * failing an assertion, so the run reports every test passing and exits
 * non-zero anyway. Asserting it here attributes it to the test that caused it.
 *
 * Registered by the setup file, so it is the *first* `afterEach` registered
 * and therefore — Vitest unwinds `afterEach` hooks in reverse — the last one
 * to run. That order is deliberate: a suite's own teardown is where an unmount
 * happens, and a teardown-triggered rejection is one of the shapes this exists
 * to catch.
 */
afterEach(async () => {
  await settleRejections();
  expect(takeUnhandled(), "something rejected and nobody was holding it").toEqual([]);
});

/**
 * …and once more for the tail, after which the listener comes off.
 *
 * A rejection surfacing between the last `afterEach` and the end of the file
 * has no test left to be attributed to, but it is still a failure of this
 * file, and `afterAll` is the last place that can say so. Removing the
 * listener afterwards is not tidiness: while it is installed Vitest's own
 * unhandled-error reporting stays silent, so anything later in the run has to
 * be handed back to it.
 */
afterAll(async () => {
  await settleRejections();
  try {
    expect(takeUnhandled(), "something rejected after the last test in this file").toEqual([]);
  } finally {
    stopWatching();
  }
});
