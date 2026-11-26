import { expect, test } from "vitest";

import { placeAt, placeBeside, placeFloat } from "./floats";

const view = { width: 1440, height: 900 };
const opener = { top: 100, bottom: 126, left: 400, right: 500 };

test("opens under its opener, with the opener's edge as its own", () => {
  expect(placeFloat(opener, { width: 300, height: 200 }, view, "end")).toEqual({ left: 200, top: 130, maxHeight: 762 });
  expect(placeFloat(opener, { width: 300, height: 200 }, view, "start").left).toBe(400);
});

test("stays inside the window on both sides", () => {
  expect(placeFloat({ ...opener, left: 10, right: 60 }, { width: 300, height: 200 }, view, "end").left).toBe(8);
  expect(placeFloat({ ...opener, left: 1300, right: 1400 }, { width: 300, height: 200 }, view, "start").left).toBe(1132);
});

test("opens over its opener when it fits only there", () => {
  const low = { ...opener, top: 700, bottom: 726 };
  expect(placeFloat(low, { width: 300, height: 400 }, view, "end")).toEqual({ left: 200, top: 296, maxHeight: 688 });
});

test("taller than either side: takes the larger, and scrolls in it", () => {
  const short = { width: 1100, height: 400 };
  const at = placeFloat({ ...opener, top: 80, bottom: 106 }, { width: 300, height: 900 }, short, "end");
  expect(at.top).toBe(110);
  expect(at.maxHeight).toBe(282);
  const lower = placeFloat({ ...opener, top: 300, bottom: 326 }, { width: 300, height: 900 }, short, "end");
  expect(lower).toEqual({ left: 200, top: 8, maxHeight: 288 });
});

// The right-click menu (2026-11-16): at the pointer, kept inside the window at any size.
const window1440 = { width: 1440, height: 868 };

test("a menu at a point: its corner at the point where it fits", () => {
  expect(placeAt({ x: 300, y: 200 }, { width: 240, height: 300 }, window1440)).toEqual({ left: 300, top: 200, maxHeight: 660 });
});

test("at the right edge it opens to the left of the point, at the bottom over it", () => {
  expect(placeAt({ x: 1400, y: 200 }, { width: 240, height: 300 }, window1440).left).toBe(1160);
  expect(placeAt({ x: 300, y: 800 }, { width: 240, height: 300 }, window1440)).toEqual({ left: 300, top: 500, maxHeight: 792 });
});

test("never off the window, even on one smaller than the menu", () => {
  const tiny = { width: 200, height: 120 };
  const at = placeAt({ x: 190, y: 110 }, { width: 240, height: 300 }, tiny);
  expect(at.left).toBe(8);
  expect(at.top).toBe(8);
  expect(at.maxHeight).toBe(102);
  expect(placeAt({ x: 2, y: 2 }, { width: 100, height: 50 }, window1440)).toEqual({ left: 8, top: 8, maxHeight: 852 });
});

test("a submenu stands beside its item, on the left where the right has no room", () => {
  const item = { top: 300, bottom: 328, left: 400, right: 640 };
  expect(placeBeside(item, { width: 220, height: 160 }, window1440)).toEqual({ left: 636, top: 296, maxHeight: 852 });
  expect(placeBeside({ ...item, left: 1100, right: 1340 }, { width: 220, height: 160 }, window1440).left).toBe(884);
  expect(placeBeside({ ...item, top: 820, bottom: 848 }, { width: 220, height: 160 }, window1440).top).toBe(700);
});
