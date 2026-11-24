// The filter bar's height on the page's scroller (`fbar-height.ts`, the ux30 end gate, Windows): written only when it
// changes from what is in force, so a bar on one line restyles none of the rows the scroller holds.
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { afterEach, expect, test, vi } from "vitest";

import { FBAR_FALLBACK_PX, fbarHeight } from "./fbar-height";

let told: (() => void) | null = null;
class Seen {
  constructor(callback: () => void) {
    told = callback;
  }
  observe() {}
  disconnect() {}
}

afterEach(() => {
  told = null;
  vi.unstubAllGlobals();
});

/** A scroller holding a filter bar `px` high, and the observer's word that it was laid out. */
function page(px: number) {
  vi.stubGlobal("ResizeObserver", Seen);
  const wrap = document.createElement("div");
  wrap.innerHTML = `<div class="tf-sbody"><div class="tf-fbar"></div></div>`;
  const bar = wrap.querySelector<HTMLElement>(".tf-fbar")!;
  const height = { px };
  Object.defineProperty(bar, "offsetHeight", { get: () => height.px });
  const stop = fbarHeight(wrap);
  return { wrap, height, stop, laidOut: () => told!() };
}

test("a bar on one line writes nothing: the stylesheet's fallback is its height", () => {
  const { wrap, laidOut } = page(FBAR_FALLBACK_PX);
  laidOut();
  laidOut();
  expect(wrap.style.getPropertyValue("--tf-fbar-h")).toBe("");
});

test("a bar that wraps writes its height once, and again only when it changes", () => {
  const { wrap, height, laidOut } = page(79);
  const writes = vi.spyOn(wrap.style, "setProperty");
  laidOut();
  expect(wrap.style.getPropertyValue("--tf-fbar-h")).toBe("79px");
  laidOut();
  expect(writes).toHaveBeenCalledTimes(1);
  height.px = FBAR_FALLBACK_PX;
  laidOut();
  expect(wrap.style.getPropertyValue("--tf-fbar-h")).toBe(`${FBAR_FALLBACK_PX}px`);
  expect(writes).toHaveBeenCalledTimes(2);
});

test("a page held hidden measures nothing and writes nothing", () => {
  const { wrap, laidOut } = page(0);
  laidOut();
  expect(wrap.style.getPropertyValue("--tf-fbar-h")).toBe("");
});

test("the fallback is the one tickets.css falls back to", () => {
  const css = readFileSync(join(process.cwd(), "src/lib/tf/css/tickets.css"), "utf8");
  const uses = [...css.matchAll(/var\(--tf-fbar-h,\s*(\d+)px\)/g)].map((m) => Number(m[1]));
  expect(uses.length).toBeGreaterThan(0);
  for (const px of uses) expect(px).toBe(FBAR_FALLBACK_PX);
});

test("each layout tells where the board's cards start: the scroller's top and its bar, and its left, read before any write", () => {
  vi.stubGlobal("ResizeObserver", Seen);
  const wrap = document.createElement("div");
  wrap.innerHTML = `<div class="tf-sbody"><div class="tf-fbar"></div></div>`;
  const bar = wrap.querySelector<HTMLElement>(".tf-fbar")!;
  const height = { px: 79 };
  Object.defineProperty(bar, "offsetHeight", { get: () => height.px });
  vi.spyOn(wrap, "getBoundingClientRect").mockReturnValue({ top: 120, left: 260 } as DOMRect);
  const heard: number[][] = [];
  const writes: string[] = [];
  vi.spyOn(wrap.style, "setProperty").mockImplementation((name) => void writes.push(`${heard.length}:${name}`));
  fbarHeight(wrap, (top, left) => heard.push([top, left]));
  told!();
  expect(heard).toEqual([[199, 260]]);
  // The top was read before the bar's height was written.
  expect(writes).toEqual(["1:--tf-fbar-h"]);
  // A page held hidden tells nothing.
  height.px = 0;
  told!();
  expect(heard).toEqual([[199, 260]]);
});
