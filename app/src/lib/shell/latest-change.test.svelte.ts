/**
 * The status bar's one-liner — spec §2's *"latest change"*, the ticker's quiet
 * replacement.
 *
 * The behaviour worth pinning is the **coalescing**. `activity:new` is already
 * throttled to at most one per second by the emitter (interfaces §2.3), and
 * this is the second wall: a sync that lands two hundred items must not make
 * the bottom of the window flicker two hundred times, whatever the backend
 * does tomorrow.
 */
import { afterEach, expect, test, vi } from "vitest";

import type { ActivityRow } from "../ipc/entity";
import { createLatestChange } from "./latest-change.svelte";

function row(id: number, verb: string): ActivityRow {
  return {
    id,
    at: "2026-08-22T14:30:00Z",
    actor: "sync:mock",
    verb,
    entity_id: `mock:PAY-${id}`,
    detail: {},
  };
}

afterEach(() => vi.useRealTimers());

test("shows the newest line and coalesces a burst into one update", () => {
  vi.useFakeTimers();
  const store = createLatestChange({ windowMs: 1000 });

  store.push(row(1, "synced"));
  store.push(row(2, "synced"));
  store.push(row(3, "linked"));

  expect(store.current?.id, "the first shows immediately").toBe(1);
  vi.advanceTimersByTime(999);
  expect(store.current?.id, "still inside the window").toBe(1);
  vi.advanceTimersByTime(1);
  expect(store.current?.id, "then the newest of the burst, once").toBe(3);

  store.stop();
});

/**
 * The window is *trailing*, so a quiet period does not delay the next line.
 *
 * A store that armed the timer on every push regardless would show the first
 * line, then hold every later one for a full second even when they arrive
 * minutes apart — which is a status bar that is always a second stale.
 */
test("a line arriving after the window shows immediately", () => {
  vi.useFakeTimers();
  const store = createLatestChange({ windowMs: 1000 });

  store.push(row(1, "synced"));
  vi.advanceTimersByTime(5000);
  store.push(row(2, "linked"));

  expect(store.current?.id).toBe(2);

  store.stop();
});

/** Only the newest of a burst is ever shown; the ones in between are dropped. */
test("the middle of a burst never reaches the screen", () => {
  vi.useFakeTimers();
  const store = createLatestChange({ windowMs: 1000 });
  const seen: number[] = [];

  store.push(row(1, "a"));
  seen.push(store.current!.id);
  store.push(row(2, "b"));
  store.push(row(3, "c"));
  store.push(row(4, "d"));
  vi.advanceTimersByTime(1000);
  seen.push(store.current!.id);
  vi.advanceTimersByTime(5000);
  seen.push(store.current!.id);

  expect(seen).toEqual([1, 4, 4]);

  store.stop();
});

/** Nothing has happened yet is a state, and it is `null`. */
test("an untouched store has no line", () => {
  const store = createLatestChange();
  expect(store.current).toBeNull();
  store.stop();
});

/**
 * `stop` cancels a pending release.
 *
 * The store outlives no component — `StatusBar` creates one and drops it — but
 * a timer still holding a reference after unmount is a leak in a dev session
 * that remounts a hundred times, and worse, it writes a rune nothing is
 * watching.
 */
test("stop cancels the pending release", () => {
  vi.useFakeTimers();
  const store = createLatestChange({ windowMs: 1000 });

  store.push(row(1, "a"));
  store.push(row(2, "b"));
  store.stop();
  vi.advanceTimersByTime(5000);

  expect(store.current?.id, "the deferred line landed after stop").toBe(1);
  expect(vi.getTimerCount()).toBe(0);
});
