import { afterEach, expect, test, vi } from "vitest";

import { count, counting, seconds, ticks } from "./comb";

afterEach(() => {
  vi.useRealTimers();
});

test("thirty ticks for a hold; a 60-second code keeps thirty, two seconds each", () => {
  expect(ticks(30)).toBe(30);
  expect(ticks(29)).toBe(29);
  expect(ticks(1)).toBe(1);
  expect(ticks(0)).toBe(0);
  expect(ticks(60, 60)).toBe(30);
  expect(ticks(59, 60)).toBe(30);
  expect(ticks(58, 60)).toBe(29);
  expect(ticks(2, 60)).toBe(1);
  expect(seconds(24)).toBe("24s");
});

test("one ticker sets --n and the seconds on the nodes, takes the warn tone under six, and stops when nothing counts", () => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2026-11-22T10:00:00Z"));
  const comb = document.createElement("span");
  const left = document.createElement("span");
  let ended = 0;
  count({ comb, left, until: Date.now() + 30_000, span: 30, words: seconds, ended: () => ended++ });
  expect(comb.style.getPropertyValue("--n")).toBe("30");
  expect(left.textContent).toBe("30s");
  vi.advanceTimersByTime(1_000);
  expect(comb.style.getPropertyValue("--n")).toBe("29");
  expect(comb.classList.contains("tf-pw-low")).toBe(false);
  vi.advanceTimersByTime(24_000);
  expect(comb.style.getPropertyValue("--n")).toBe("5");
  expect(comb.classList.contains("tf-pw-low")).toBe(true);
  expect(counting()).toBe(1);
  vi.advanceTimersByTime(5_250);
  expect(comb.style.getPropertyValue("--n")).toBe("0");
  expect(ended).toBe(1);
  expect(counting()).toBe(0);
});

test("a node let go stops counting", () => {
  vi.useFakeTimers();
  const comb = document.createElement("span");
  const stop = count({ comb, left: null, until: Date.now() + 10_000, span: 30, words: seconds });
  expect(counting()).toBe(1);
  stop();
  expect(counting()).toBe(0);
});
