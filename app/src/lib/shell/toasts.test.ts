import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { amend, dismiss, heldWrite, hold, push, release, reset, toasts, undoable } from "./toasts.svelte";

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

test("the same words again are the one toast, on a new time, with the newest press's buttons", () => {
  const first = vi.fn();
  const newest = vi.fn();
  const id = push({ text: "Syncing 5 sources.", action: { label: "Show", run: first } });
  vi.advanceTimersByTime(6000);
  expect(push({ text: "Syncing 5 sources.", action: { label: "Show", run: newest } })).toBe(id);
  expect(toasts.items).toHaveLength(1);
  toasts.items[0]!.action!.run();
  expect(first).not.toHaveBeenCalled();
  expect(newest).toHaveBeenCalledOnce();
  vi.advanceTimersByTime(6000);
  expect(toasts.items).toHaveLength(1);
  vi.advanceTimersByTime(1000);
  expect(toasts.items).toHaveLength(0);
});

test("the same words in another tone are another toast", () => {
  push({ text: "Refused." });
  push({ text: "Refused.", tone: "err" });
  expect(toasts.items).toHaveLength(2);
});

test("a repeat of a toast held under the pointer stays held, with a full time for later", () => {
  const id = push({ text: "Sent." });
  hold(id, "hover");
  push({ text: "Sent." });
  vi.advanceTimersByTime(60_000);
  expect(toasts.items).toHaveLength(1);
  release(id, "hover");
  vi.advanceTimersByTime(6000);
  expect(toasts.items).toHaveLength(1);
  vi.advanceTimersByTime(600);
  expect(toasts.items).toHaveLength(0);
});

test("a second toast amended to the words of a first stands alone", () => {
  push({ text: "Sent: re-run." });
  const second = push({ text: "Queued: re-run." });
  amend(second, { text: "Sent: re-run." });
  expect(toasts.items.map((toast) => toast.id)).toEqual([second]);
});

test("undoable: Undo undoes, and a repeat's Undo takes back both", () => {
  const first = vi.fn();
  const second = vi.fn();
  undoable("Hidden.", first);
  undoable("Hidden.", second);
  expect(toasts.items).toHaveLength(1);
  expect(toasts.items[0]!.action!.label).toBe("Undo");
  toasts.items[0]!.action!.run();
  expect(first).toHaveBeenCalledOnce();
  expect(second).toHaveBeenCalledOnce();
});

test("heldWrite sends when the toast ends on its time, or by its \u00d7", () => {
  const send = vi.fn();
  heldWrite("Dismissed.", send, () => {});
  expect(send).not.toHaveBeenCalled();
  vi.advanceTimersByTime(6500);
  expect(send).toHaveBeenCalledOnce();

  const byHand = vi.fn();
  const id = heldWrite("Dismissed too.", byHand, () => {});
  dismiss(id);
  dismiss(id);
  expect(byHand).toHaveBeenCalledOnce();
});

test("heldWrite on Undo puts back and never sends", () => {
  const send = vi.fn();
  const undo = vi.fn();
  const id = heldWrite("Dismissed.", send, undo);
  toasts.items[0]!.action!.run();
  dismiss(id);
  vi.advanceTimersByTime(10_000);
  expect(undo).toHaveBeenCalledOnce();
  expect(send).not.toHaveBeenCalled();
});

test("heldWrite repeated: both are sent, or both put back", () => {
  const send = vi.fn();
  heldWrite("Dismissed.", () => send(1), () => {});
  heldWrite("Dismissed.", () => send(2), () => {});
  expect(toasts.items).toHaveLength(1);
  vi.advanceTimersByTime(6500);
  expect(send.mock.calls).toEqual([[1], [2]]);
});

test("a reset sends nothing that was held", () => {
  const send = vi.fn();
  heldWrite("Dismissed.", send, () => {});
  reset();
  vi.advanceTimersByTime(10_000);
  expect(send).not.toHaveBeenCalled();
});

test("a toast with a live toast's subject takes its place, with the first Undo (F-648)", () => {
  const undone: string[] = [];
  const first = undoable("Terminal is the default", () => undone.push("VS Code"), { subject: "default" });
  const second = undoable("Fork is the default", () => undone.push("Terminal"), { subject: "default" });
  expect(second).toBe(first);
  expect(toasts.items.map((toast) => toast.text)).toEqual(["Fork is the default"]);
  toasts.items[0]!.action!.run();
  expect(undone).toEqual(["VS Code"]);
});

test("a toast with no Undo keeps the Undo it replaced, and takes the new words, tone and buttons otherwise", () => {
  const undo = vi.fn();
  undoable("Paused", undo, { subject: "syncing" });
  push({ text: "Syncs again", subject: "syncing" });
  expect(toasts.items[0]!.text).toBe("Syncs again");
  toasts.items[0]!.action!.run();
  expect(undo).toHaveBeenCalledOnce();
  reset();
  push({ text: "Added", subject: "review", action: { label: "Open", run: () => {} } });
  push({ text: "Failed", tone: "err", subject: "review", action: { label: "Retry", run: () => {} } });
  expect(toasts.items.map((toast) => [toast.text, toast.tone, toast.action?.label])).toEqual([["Failed", "err", "Retry"]]);
});

test("a toast about something else, or about nothing, stands beside it", () => {
  push({ text: "One", subject: "a" });
  push({ text: "Two", subject: "b" });
  push({ text: "Three" });
  expect(toasts.items.map((toast) => toast.text)).toEqual(["One", "Two", "Three"]);
});

test("a replaced toast starts its time over", () => {
  push({ text: "One", subject: "a", ms: 1000 });
  vi.advanceTimersByTime(900);
  push({ text: "Two", subject: "a", ms: 1000 });
  vi.advanceTimersByTime(900);
  expect(toasts.items.map((toast) => toast.text)).toEqual(["Two"]);
  vi.advanceTimersByTime(100);
  expect(toasts.items).toEqual([]);
});
