import { describe, expect, it } from "vitest";
import { until, waitFor } from "./test-wait";

/** A stall longer than the deadline, then the timer that makes the condition true. */
function stallThenFlip(): { flipped: () => boolean } {
  let done = false;
  setTimeout(() => {
    const end = performance.now() + 200;
    while (performance.now() < end);
  }, 0);
  setTimeout(() => {
    done = true;
  }, 10);
  return { flipped: () => done };
}

describe("test-wait (#1799)", () => {
  it("waitFor passes when the condition is true by the time the deadline fires", async () => {
    const { flipped } = stallThenFlip();
    await waitFor(() => expect(flipped()).toBe(true), { timeout: 100, interval: 5 });
  });

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

  it("until passes a stalled wait whose condition came true", async () => {
    const { flipped } = stallThenFlip();
    await until(flipped, "flip", { timeout: 100, interval: 5 });
  });
});
