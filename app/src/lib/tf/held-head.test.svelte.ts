/** A header drawn once, whole (`held-head.svelte.ts`, the nothing-moves pass, 2026-11-17). */
import { flushSync } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { HEAD_HOLD_MS, heldHead } from "./held-head.svelte";

beforeEach(() => vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] }));
afterEach(() => vi.useRealTimers());

function held() {
  const state = $state({ key: "a", answered: false });
  let head!: ReturnType<typeof heldHead>;
  const stop = $effect.root(() => {
    head = heldHead(
      () => state.key,
      () => state.answered,
    );
  });
  flushSync();
  return { state, head, stop };
}

test("unseen until its reads answer, then shown at once", () => {
  const { state, head, stop } = held();
  expect(head.shown).toBe(false);
  state.answered = true;
  flushSync();
  expect(head.shown).toBe(true);
  stop();
});

test("a read that never answers holds it no longer than the hold", () => {
  const { head, stop } = held();
  vi.advanceTimersByTime(HEAD_HOLD_MS - 1);
  flushSync();
  expect(head.shown).toBe(false);
  vi.advanceTimersByTime(1);
  flushSync();
  expect(head.shown).toBe(true);
  stop();
});

test("once shown it stays shown while a refresh reads again, and a new key holds it again", () => {
  const { state, head, stop } = held();
  state.answered = true;
  flushSync();
  state.answered = false;
  flushSync();
  expect(head.shown, "a refresh lands over what it drew").toBe(true);
  state.key = "b";
  flushSync();
  expect(head.shown, "another item's header is held").toBe(false);
  stop();
});
