/**
 * `Enter` before the ranking answered (#2535), the one helper the pickers and Secrets share: the press waits for the
 * answer to the words it was pressed on, then acts once; typing on drops it; `null` acts at once and drops a wait.
 */
import { flushSync } from "svelte";
import { expect, test } from "vitest";

import { enterWhenRanked } from "./ranked.svelte";

function picker() {
  const s = $state({ typed: "", settled: true, open: true });
  const acts: string[] = [];
  let enter!: { press(): void };
  const stop = $effect.root(() => {
    enter = enterWhenRanked(
      () => (s.open ? { typed: s.typed, settled: s.settled } : null),
      () => acts.push(`now ${s.typed}`),
      () => acts.push(`late ${s.typed}`),
    );
  });
  return { s, acts, enter, stop };
}

test("a press over a landed answer acts at once", () => {
  const { s, acts, enter, stop } = picker();
  s.typed = "pay";
  enter.press();
  expect(acts).toEqual(["now pay"]);
  stop();
});

test("a press before the answer waits for it, then acts once", () => {
  const { s, acts, enter, stop } = picker();
  s.typed = "pay";
  s.settled = false;
  enter.press();
  flushSync();
  expect(acts).toEqual([]);
  s.settled = true;
  flushSync();
  expect(acts).toEqual(["late pay"]);
  s.settled = false;
  flushSync();
  s.settled = true;
  flushSync();
  expect(acts).toEqual(["late pay"]);
  stop();
});

test("typing on drops a waiting press", () => {
  const { s, acts, enter, stop } = picker();
  s.typed = "pay";
  s.settled = false;
  enter.press();
  flushSync();
  s.typed = "payo";
  flushSync();
  s.settled = true;
  flushSync();
  expect(acts).toEqual([]);
  stop();
});

test("with nothing to wait on the press acts at once, and a waiting one is dropped", () => {
  const { s, acts, enter, stop } = picker();
  s.typed = "pay";
  s.settled = false;
  enter.press();
  flushSync();
  s.open = false;
  flushSync();
  s.open = true;
  s.settled = true;
  flushSync();
  expect(acts).toEqual([]);
  s.open = false;
  enter.press();
  expect(acts).toEqual(["now pay"]);
  stop();
});
