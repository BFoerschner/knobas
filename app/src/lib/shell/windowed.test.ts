import { describe, expect, it } from "vitest";

import { Heights } from "./windowed";

/** Ten rows keyed `r0` … `r9`, 50 px estimated, no gap. */
const ten = () => new Heights(50, 0);
const keys = Array.from({ length: 10 }, (_, i) => `r${i}`);

describe("Heights", () => {
  it("draws the rows a viewport overlaps, with no overscan", () => {
    const h = ten();
    h.keys(keys);
    // 120 to 220 px overlaps rows 2 (100–150), 3 and 4 (200–250).
    expect(h.span(120, 100, 0)).toEqual({ start: 2, end: 5, before: 100, after: 250 });
  });

  it("widens the drawn rows by the overscan on both sides, within the list", () => {
    const h = ten();
    h.keys(keys);
    expect(h.span(120, 100, 60)).toEqual({ start: 1, end: 6, before: 50, after: 200 });
    expect(h.span(0, 100, 1000)).toEqual({ start: 0, end: 10, before: 0, after: 0 });
  });

  it("puts a measured row's height in place of the estimate", () => {
    const h = ten();
    h.keys(keys);
    h.set("r0", 130);
    h.set("r1", 20);
    expect(h.total()).toBe(130 + 20 + 8 * 50);
    // 140 to 160: row 1 is 130–150, row 2 is 150–200.
    expect(h.span(140, 20, 0)).toEqual({ start: 1, end: 3, before: 130, after: 350 });
    expect(h.offsetOf(3)).toBe(200);
  });

  it("keeps a measurement with its key when the rows change order or narrow", () => {
    const h = ten();
    h.keys(keys);
    h.set("r9", 90);
    h.keys(["r9", "r3"]);
    expect(h.total()).toBe(140);
    expect(h.span(0, 10, 0)).toEqual({ start: 0, end: 1, before: 0, after: 50 });
  });

  it("counts the gap between rows, not after the last", () => {
    const h = new Heights(50, 6);
    h.keys(keys);
    expect(h.total()).toBe(10 * 50 + 9 * 6);
    // Row 2 starts at 112; the gap before row 3 (162–168) draws nothing.
    expect(h.span(163, 2, 0)).toEqual({ start: 2, end: 3, before: 112, after: 10 * 56 - 3 * 56 });
  });

  it("says whether a measurement changed anything", () => {
    const h = ten();
    h.keys(keys);
    expect(h.set("r1", 50)).toBe(false);
    expect(h.set("r1", 51)).toBe(true);
    expect(h.set("r1", 51)).toBe(false);
  });

  it("draws nothing and needs no room for an empty list", () => {
    const h = ten();
    h.keys([]);
    expect(h.span(0, 500, 100)).toEqual({ start: 0, end: 0, before: 0, after: 0 });
    expect(h.total()).toBe(0);
  });

  it("draws the last rows when the viewport is past the end", () => {
    const h = ten();
    h.keys(keys);
    expect(h.span(900, 100, 0)).toEqual({ start: 9, end: 10, before: 450, after: 0 });
  });

  it("finds the scroll that brings a row into view, moving as little as it can", () => {
    const h = ten();
    h.keys(keys);
    // Viewport 100–200: row 2 (100–150) is in view already.
    expect(h.reveal(2, 100, 100)).toBe(100);
    // Row 6 (300–350) is below: scroll until its bottom meets the viewport's.
    expect(h.reveal(6, 100, 100)).toBe(250);
    // Row 0 is above: scroll to its top.
    expect(h.reveal(0, 100, 100)).toBe(0);
  });
});
