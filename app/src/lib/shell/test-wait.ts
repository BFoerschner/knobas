import { flushSync } from "svelte";
import { beforeEach, vi } from "vitest";

/**
 * The one wait for the frontend tests (#1799). Vitest's `waitFor` polls on an interval and
 * keeps a separate deadline timer whose handler only rejects. When the event loop
 * stalls past the deadline, every expired timer runs in one pass: the poll (still
 * false), the app's own timer that makes the condition true, then the deadline,
 * which rejects with the work already done. This `waitFor` is vitest's with one
 * more look when the deadline fires: it flushes effects, calls the callback again,
 * and fails only if that call fails too. `until` is the labelled form of it.
 */
export async function waitFor<T>(
  callback: () => T | Promise<T>,
  options?: { timeout?: number; interval?: number },
): Promise<T> {
  try {
    return await vi.waitFor(callback, options);
  } catch (deadline) {
    flushSync();
    try {
      return await callback();
    } catch {
      throw deadline;
    }
  }
}

/** Every `until` wait of the running test, in order: label, how long it took, how many polls. */
let waits: { label: string; ms: number; polls: number; timedOut: boolean }[] = [];
let testStart = 0;

beforeEach(() => {
  waits = [];
  testStart = performance.now();
});

function timeline(): string {
  const rows = waits.map((w, i) => `  ${i + 1}. ${w.label}: ${Math.round(w.ms)} ms, ${w.polls} polls${w.timedOut ? " (TIMED OUT)" : ""}`);
  return `waits so far (test started ${Math.round(performance.now() - testStart)} ms ago):\n${rows.join("\n")}`;
}

/**
 * Wait for `condition`, running `flushSync` before each look so an effect can run
 * between attempts. A timeout throws `whatWasWaitedFor` followed by the timeline of
 * the test's waits so far (#1814), the original error as `cause`.
 */
export async function until(
  condition: () => boolean,
  whatWasWaitedFor: string,
  { timeout = 3_000, interval = 5 }: { timeout?: number; interval?: number } = {},
): Promise<void> {
  const start = performance.now();
  let polls = 0;
  try {
    await waitFor(() => {
      polls += 1;
      flushSync();
      if (!condition()) throw new Error(whatWasWaitedFor);
    }, { timeout, interval });
    waits.push({ label: whatWasWaitedFor, ms: performance.now() - start, polls, timedOut: false });
  } catch (cause) {
    waits.push({ label: whatWasWaitedFor, ms: performance.now() - start, polls, timedOut: true });
    throw new Error(`${whatWasWaitedFor}\n${timeline()}`, { cause });
  }
}
