/**
 * A hidden page's pause (#2249) and its kept scroll (#2248): the work runs
 * only while shown and once on the return, and the scroll noted at the hide
 * is set back on the show.
 */
import { flushSync } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { everyWhileShown, keepScroll, onFocusWhileShown } from "./while-shown.svelte";

const page = $state({ shown: true });
let stop: (() => void) | null = null;

beforeEach(() => {
  page.shown = true;
});

afterEach(() => {
  stop?.();
  stop = null;
  vi.useRealTimers();
});

function run(setup: () => void) {
  stop = $effect.root(setup);
  flushSync();
}

function show(on: boolean) {
  page.shown = on;
  flushSync();
}

test("a focus re-read runs only while shown, and once when shown again", () => {
  let reads = 0;
  run(() => onFocusWhileShown(() => page.shown, () => (reads += 1)));
  expect(reads).toBe(0);
  window.dispatchEvent(new Event("focus"));
  expect(reads).toBe(1);

  show(false);
  window.dispatchEvent(new Event("focus"));
  window.dispatchEvent(new Event("focus"));
  expect(reads).toBe(1);

  show(true);
  expect(reads).toBe(2);
  window.dispatchEvent(new Event("focus"));
  expect(reads).toBe(3);
});

test("a timer stops while hidden and runs once when shown again", () => {
  vi.useFakeTimers();
  let ticks = 0;
  run(() => everyWhileShown(() => page.shown, 30_000, () => (ticks += 1)));
  vi.advanceTimersByTime(60_000);
  expect(ticks).toBe(2);

  show(false);
  vi.advanceTimersByTime(120_000);
  expect(ticks).toBe(2);

  show(true);
  expect(ticks).toBe(3);
  vi.advanceTimersByTime(30_000);
  expect(ticks).toBe(4);
});

/** An element whose scroll a test can set and read: jsdom keeps none. */
function scroller(el: HTMLElement): HTMLElement {
  let top = 0;
  let left = 0;
  Object.defineProperty(el, "scrollTop", { get: () => top, set: (n: number) => (top = n), configurable: true });
  Object.defineProperty(el, "scrollLeft", { get: () => left, set: (n: number) => (left = n), configurable: true });
  return el;
}

test("the shell's scroller and a scroller inside go back to where they were when shown again", () => {
  const main = scroller(document.createElement("div"));
  main.className = "tf-main";
  const anchor = document.createElement("div");
  const tree = scroller(document.createElement("nav"));
  anchor.append(tree);
  main.append(anchor);
  document.body.append(main);
  run(() => keepScroll(() => anchor, () => page.shown));

  main.scrollTop = 840;
  tree.scrollTop = 120;
  show(false);
  // Another page scrolls the shared scroller; the hidden subtree lost its own.
  main.scrollTop = 30;
  tree.scrollTop = 0;

  show(true);
  expect(main.scrollTop).toBe(840);
  expect(tree.scrollTop).toBe(120);
  main.remove();
});
