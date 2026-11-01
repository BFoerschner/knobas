/**
 * What every suite gets whether it asks or not: the jsdom the shell expects,
 * and the guard against a green file that has leaked a rejection.
 *
 * Loaded by `vite.config.ts`'s `test.setupFiles`, so every suite gets it
 * without importing anything. That "without opting in" is the point of the
 * second half: a file-local guard protects the one file that remembered it,
 * and the file that leaks is by definition the one that did not (#103).
 */
import { afterAll, afterEach, beforeEach, expect, vi } from "vitest";

// First: it must run before any module that looks for a storage is evaluated.
import "./test-no-storage";

import { wholeTreeBudget } from "./test-budget";
import { resetStores } from "./test-resets";
import { settleRejections, takeUnhandled, watchRejections } from "./unhandled";

/**
 * Every module-level store goes back to a fresh window after each test
 * (`test-resets.ts` lists them; `test-resets.test.ts` fails on one missing).
 * Each is a singleton like the session it remembers, so without this a test
 * would inherit what the previous one left (FE-14, #1674).
 *
 * The whole tree's budget, not the hook's 10 s (#2359). Each reset imports its
 * store, so the first one in a file loads every store and, through the page
 * components among them, most of the app: 1.7 s for `a11y.test.ts` run alone
 * on an idle Mac, nearly all of it transform, then 0 ms for every later test.
 * Under load that one import measured the neighbours: `a11y.test.ts` alone
 * beside load 70-130 spent 12.6-13.8 s in it and failed a test whose own scan
 * takes milliseconds. A cold import costs in proportion to the tree it pulls,
 * as a whole-tree scan does, so it takes that budget.
 */
afterEach(resetStores, wholeTreeBudget());

/**
 * The zone is pinned in `vite.config.ts`; this is what makes the pin a fact
 * rather than an intention.
 *
 * `test.env` is Vitest setting `process.env.TZ` in each worker and Node
 * re-reading it — two mechanisms, either of which can change under an upgrade
 * without anybody noticing. The failure mode if it does is not a red suite: it
 * is a suite that quietly goes back to testing whichever zone the machine is
 * in, which is the condition #406 lived in for as long as no runner ran.
 * Refusing to start says so in one line instead.
 *
 * Here rather than in a test file because a test can be filtered out of a run
 * and a setup file cannot: this is checked once per file, in every worker,
 * whatever `-t` was passed.
 */
const PINNED_ZONE = "Europe/Berlin";
const zone = Intl.DateTimeFormat().resolvedOptions().timeZone;
if (zone !== PINNED_ZONE) {
  throw new Error(
    `the suite is pinned to ${PINNED_ZONE} but is running in ${zone}: ` +
      "`test.env.TZ` in vite.config.ts has stopped taking effect",
  );
}

/**
 * `KNOBAS_TEST_CLOCK=HH:MM` runs every test with the wall clock at that time
 * of today, in the pinned zone (`just test-midnight` passes `00:10`).
 *
 * The suite is pinned to Berlin, UTC+1 or +2, so a test that builds a local
 * date from "now" and compares it with a UTC one (`toISOString().slice(0, 10)`)
 * agrees with itself for 22 hours a day and fails in the first hours after
 * midnight, which is when nobody runs it: #1031, #1150 and #1155 were one
 * bug filed three times. Only `Date` is faked, and it keeps advancing with
 * real time, so timers, the wait helper and the rejection guard below are
 * untouched. A file that installs its own fake timers overrides this for the
 * tests that do, and gets it back from the next `beforeEach`.
 */
const CLOCK = process.env.KNOBAS_TEST_CLOCK;
if (CLOCK) {
  const [hours, minutes] = CLOCK.split(":").map(Number);
  if (!Number.isInteger(hours) || !Number.isInteger(minutes)) throw new Error(`KNOBAS_TEST_CLOCK=${CLOCK}: expected HH:MM`);
  const pinned = new Date();
  pinned.setHours(hours!, minutes!, 0, 0);
  beforeEach(() => vi.useFakeTimers({ toFake: ["Date"], now: pinned, shouldAdvanceTime: true }));
  afterEach(() => vi.useRealTimers());
}

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

// jsdom has no `IntersectionObserver` either, and an effect that observes an
// element returns early without one, so a component test never ran the code a
// browser runs. That hid the pipeline freeze (#1446): Svelte 5 sets a
// `bind:this` to `null` on unmount, the guard checked `undefined`, and
// `observe(null)` throws in WebKit and Chrome. This one refuses a non-Element
// as they do, and never reports an intersection.
if (!("IntersectionObserver" in globalThis)) {
  class StrictIntersectionObserver {
    readonly root = null;
    readonly rootMargin = "0px";
    readonly thresholds = [0];
    observe(target: unknown): void {
      if (!(target instanceof Element)) {
        throw new TypeError("Failed to execute 'observe' on 'IntersectionObserver': parameter 1 is not of type 'Element'.");
      }
    }
    unobserve(): void {}
    disconnect(): void {}
    takeRecords(): [] {
      return [];
    }
  }
  Object.defineProperty(globalThis, "IntersectionObserver", { writable: true, configurable: true, value: StrictIntersectionObserver });
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
