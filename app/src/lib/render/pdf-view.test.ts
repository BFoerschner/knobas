import { expect, test } from "vitest";

import { fitWidth, zoomStep, ZOOMS } from "./pdf-view";

test("zooming in and out walks the steps, and stops at the ends", () => {
  expect(zoomStep(1, 1)).toBe(1.25);
  expect(zoomStep(1, -1)).toBe(0.8);
  expect(zoomStep(ZOOMS[ZOOMS.length - 1]!, 1)).toBe(4);
  expect(zoomStep(ZOOMS[0]!, -1)).toBe(0.25);
});

test("from a zoom between steps (fit to width), a step goes to the next step that way", () => {
  expect(zoomStep(1.1, 1)).toBe(1.25);
  expect(zoomStep(1.1, -1)).toBe(1);
  expect(zoomStep(0.9, -1)).toBe(0.8);
});

test("fit to width takes the pane's width less its gutters, within the zoom's ends", () => {
  // An 816 px page (US Letter) in a 900 px pane with 18 px each side.
  expect(fitWidth(816, 900)).toBeCloseTo(864 / 816);
  expect(fitWidth(816, 100)).toBe(0.25);
  expect(fitWidth(100, 9000)).toBe(4);
  expect(fitWidth(816, 0)).toBe(1);
});
