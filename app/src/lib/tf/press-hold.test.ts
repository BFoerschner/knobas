/** `press-hold`: a second press inside about two seconds lands on the hold, and the hold lets go by itself. */
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { PRESS_HOLD_MS, PressHold } from "./press-hold.svelte";

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
});

test("it holds for about two seconds, and then lets go", () => {
  const hold = new PressHold();
  expect(hold.held()).toBe(false);
  hold.hold();
  expect(hold.held()).toBe(true);
  vi.advanceTimersByTime(PRESS_HOLD_MS - 1);
  expect(hold.held()).toBe(true);
  vi.advanceTimersByTime(1);
  expect(hold.held()).toBe(false);
});

test("each row's press holds by itself", () => {
  const hold = new PressHold();
  hold.hold("a");
  expect(hold.held("a")).toBe(true);
  expect(hold.held("b")).toBe(false);
});

test("a second hold runs from itself, not from the first", () => {
  const hold = new PressHold();
  hold.hold("a");
  vi.advanceTimersByTime(PRESS_HOLD_MS - 1);
  hold.hold("a");
  vi.advanceTimersByTime(PRESS_HOLD_MS - 1);
  expect(hold.held("a")).toBe(true);
});

test("release lets go early, and its timer can't let go of a later hold", () => {
  const hold = new PressHold();
  hold.hold("a");
  vi.advanceTimersByTime(PRESS_HOLD_MS - 1);
  hold.release("a");
  expect(hold.held("a")).toBe(false);
  hold.hold("a");
  vi.advanceTimersByTime(1);
  expect(hold.held("a")).toBe(true);
});
