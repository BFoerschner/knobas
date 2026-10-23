import { describe, expect, it } from "vitest";
import { until, waitFor } from "./test-wait";

/**
 * The stall, ordered rather than timed (#1898). The flip and the deadline share one duration,
 * and timers of one duration fire in the order they were set, so the flip always runs before
 * the deadline handler, with no poll in between (the interval is far past the deadline): the
 * order a long stall produces, whatever the machine is doing.
 */
const DEADLINE = 20;
const stalled = { timeout: DEADLINE, interval: 60_000 };

function flipBeforeDeadline(): { flipped: () => boolean } {
  let done = false;
  setTimeout(() => {
    done = true;
  }, DEADLINE);
  return { flipped: () => done };
}

describe("test-wait (#1799)", () => {
  it("waitFor returns the callback's value", async () => {
    expect(await waitFor(() => 7, { timeout: 100, interval: 5 })).toBe(7);
  });

  it("waitFor still fails at the deadline with the callback's error", async () => {
    await expect(waitFor(() => expect(1).toBe(2), { timeout: 50, interval: 5 })).rejects.toThrow(/expected 1 to be 2/);
  });

  it("until names the label and the timeline when a wait times out", async () => {
    await until(() => true, "first wait", { timeout: 50, interval: 5 });
    const failure = await until(() => false, "never true", { timeout: 50, interval: 5 }).catch((e: Error) => e);
    expect(failure).toBeInstanceOf(Error);
    expect((failure as Error).message).toMatch(/^never true\nwaits so far \(test started \d+ ms ago\):\n {2}1\. first wait: \d+ ms, 1 polls\n {2}2\. never true: \d+ ms, \d+ polls \(TIMED OUT\)$/);
    expect((failure as Error).cause).toBeInstanceOf(Error);
  });

  describe("a stall past the deadline", () => {
    it("waitFor passes when the condition is true by the time the deadline fires", async () => {
      const { flipped } = flipBeforeDeadline();
      await waitFor(() => expect(flipped()).toBe(true), stalled);
    });

    it("until passes a stalled wait whose condition came true", async () => {
      const { flipped } = flipBeforeDeadline();
      await until(flipped, "flip", stalled);
    });
  });
});
