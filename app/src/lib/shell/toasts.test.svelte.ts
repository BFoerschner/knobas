import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { dismiss, push, toasts } from "./toasts.svelte";

beforeEach(() => {
  vi.useFakeTimers();
  toasts.items = [];
});

afterEach(() => {
  vi.useRealTimers();
});

test("a pushed toast is on the stack and leaves on its own", () => {
  const id = push({ text: "Synced 21 items from mock" });
  expect(toasts.items.map((t) => t.text)).toEqual(["Synced 21 items from mock"]);
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
