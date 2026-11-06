import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { dismiss, hold, push, release, reset, toasts } from "./toasts.svelte";

beforeEach(() => {
  vi.useFakeTimers();
  toasts.items = [];
});

afterEach(() => {
  vi.useRealTimers();
});

test("a pushed toast is on the stack and leaves on its own", () => {
  const id = push({ text: "Synced 21 items from mock" });
  expect(toasts.items.map((t) => t.text)).toEqual([
    "Synced 21 items from mock",
  ]);
  expect(toasts.items[0]?.id).toBe(id);

  vi.advanceTimersByTime(6499);
  expect(toasts.items).toHaveLength(1);
  vi.advanceTimersByTime(1);
  expect(toasts.items).toEqual([]);
});

test("a caller can shorten the life of one", () => {
  push({ text: "quick", ms: 100 });
  vi.advanceTimersByTime(100);
  expect(toasts.items).toEqual([]);
});

/**
 * The auto-dismiss timer fires whether or not the reader closed the toast
 * first, and it fires against an id that may since have been reused by
 * nothing — a second removal must not take the wrong toast with it.
 */
test("dismiss is idempotent and removes only its own", () => {
  const first = push({ text: "first" });
  const second = push({ text: "second" });

  dismiss(first);
  expect(toasts.items.map((t) => t.id)).toEqual([second]);

  dismiss(first);
  expect(toasts.items.map((t) => t.id)).toEqual([second]);

  // ...and the timer for the toast already gone changes nothing.
  vi.advanceTimersByTime(6500);
  expect(toasts.items).toEqual([]);
});

test("ids are not reused across pushes", () => {
  const ids = [push({ text: "a" }), push({ text: "b" }), push({ text: "c" })];
  expect(new Set(ids).size).toBe(3);
});

/**
 * #1762: `reset()` runs after every test and leaves the timers of the toasts it
 * dropped armed. A restarted counter handed the next test's first toast the
 * same id, so a timer from a test seconds earlier removed it -- under load,
 * between the toast being raised and the assertion on it.
 */
test("a reset does not let a dropped toast's timer take a later one", () => {
  push({ text: "from an earlier test" });
  vi.advanceTimersByTime(3000);
  reset();

  push({ text: "from this test" });
  vi.advanceTimersByTime(3500);

  expect(toasts.items.map((t) => t.text)).toEqual(["from this test"]);
});

test("a reset leaves no dismiss timer armed", () => {
  push({ text: "a" });
  push({ text: "b" });
  expect(vi.getTimerCount()).toBe(2);
  reset();
  expect(vi.getTimerCount()).toBe(0);
});

/** F-621: an *Undo* being reached for stays. */
test("a held toast stays past its time, and its time runs on once nothing holds it", () => {
  const id = push({ text: "Removed the row", ms: 6500 });
  vi.advanceTimersByTime(4000);

  hold(id, "hover");
  vi.advanceTimersByTime(60_000);
  expect(toasts.items).toHaveLength(1);

  hold(id, "focus");
  release(id, "hover");
  vi.advanceTimersByTime(60_000);
  expect(toasts.items, "focus still holds it").toHaveLength(1);

  release(id, "focus");
  vi.advanceTimersByTime(2499);
  expect(toasts.items, "what it had left").toHaveLength(1);
  vi.advanceTimersByTime(1);
  expect(toasts.items).toEqual([]);
});

test("a toast held at the end of its time keeps enough to reach back for", () => {
  const id = push({ text: "Removed the row", ms: 1000 });
  vi.advanceTimersByTime(999);
  hold(id, "hover");
  release(id, "hover");
  vi.advanceTimersByTime(1999);
  expect(toasts.items).toHaveLength(1);
  vi.advanceTimersByTime(1);
  expect(toasts.items).toEqual([]);
});

test("a hold on a toast that has gone does nothing", () => {
  const id = push({ text: "gone" });
  dismiss(id);
  expect(() => {
    hold(id, "hover");
    release(id, "hover");
  }).not.toThrow();
});
