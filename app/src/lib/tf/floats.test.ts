import { expect, test } from "vitest";

import { placeFloat } from "./floats";

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
