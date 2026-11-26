// Drawing more as the view nears (`whileNear`): the scroller is found once the mark has been laid out, never as it is
// attached (the render-cost pass, 2026-11-14), and a mark of no size, which a resize observer never reports, is found
// the frame after.
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { whileNear } from "./near-view";

let resized: (() => void)[] = [];
let near: ((entries: { isIntersecting: boolean }[]) => void)[] = [];
let frames: (() => void)[] = [];
const made: { root: Element | null }[] = [];
const saved = { RO: globalThis.ResizeObserver, IO: globalThis.IntersectionObserver, raf: globalThis.requestAnimationFrame };

beforeEach(() => {
  vi.useFakeTimers();
  resized = [];
  near = [];
  frames = [];
  made.length = 0;
  globalThis.ResizeObserver = class {
    constructor(private readonly said: () => void) {}
    observe() {
      resized.push(this.said);
    }
    disconnect() {}
  } as unknown as typeof ResizeObserver;
  globalThis.IntersectionObserver = class {
    constructor(said: (entries: { isIntersecting: boolean }[]) => void, options: { root: Element | null }) {
      near.push(said);
      made.push({ root: options.root });
    }
    observe() {}
    disconnect() {}
  } as unknown as typeof IntersectionObserver;
  globalThis.requestAnimationFrame = ((go: () => void) => frames.push(go)) as typeof requestAnimationFrame;
});

afterEach(() => {
  vi.useRealTimers();
  globalThis.ResizeObserver = saved.RO;
  globalThis.IntersectionObserver = saved.IO;
  globalThis.requestAnimationFrame = saved.raf;
  document.body.replaceChildren();
});

function page(): HTMLElement {
  const box = document.createElement("div");
  box.style.overflowY = "auto";
  const mark = document.createElement("div");
  box.append(mark);
  document.body.append(box);
  return mark;
}

test("the scroller is read once the mark is laid out, and the view nearing draws a piece a task", () => {
  const mark = page();
  let pieces = 3;
  const stop = whileNear(() => --pieces > 0)(mark);
  expect(made).toHaveLength(0);
  resized[0]!();
  expect(made).toEqual([{ root: mark.parentElement }]);
  near[0]!([{ isIntersecting: true }]);
  vi.runAllTimers();
  expect(pieces).toBe(0);
  // The frame after finds the observer made already: one, not two.
  frames.forEach((go) => go());
  vi.runAllTimers();
  expect(made).toHaveLength(1);
  stop();
});

test("a mark no resize observer reports is found the frame after, and one taken away first is never watched", () => {
  const mark = page();
  const stop = whileNear(() => false)(mark);
  frames[0]!();
  expect(made).toHaveLength(0);
  vi.runAllTimers();
  expect(made).toHaveLength(1);
  stop();
  const gone = whileNear(() => false)(page());
  gone();
  frames.at(-1)!();
  vi.runAllTimers();
  resized.at(-1)!();
  expect(made).toHaveLength(1);
});
