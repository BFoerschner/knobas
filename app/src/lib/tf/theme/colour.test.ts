/** The colour maths themes are checked and fixed with (ADR-0055). */
import { expect, test } from "vitest";

import { apart, contrast, deltaE, fix, mix, over, parse, ratioText, simulate, toOklch, worst } from "./colour";

test("a colour is read with or without alpha, and a translucent one is laid over what it lies on", () => {
  expect(parse("#abc")).toEqual(["#AABBCC", 1]);
  expect(parse("#FFFFFF40")).toEqual(["#FFFFFF", 64 / 255]);
  expect(parse("blue")).toBeNull();
  // A highlight of white at 25 % on Dracula's panel: what VS Code draws, not white.
  expect(over("#FFFFFF40", "#282A36")).toBe("#5E5F68");
  expect(over("#123456", "#000000")).toBe("#123456");
});

test("a ratio is cut to one decimal and never rounded up, so 4.45 does not read as passing", () => {
  expect(ratioText(4.45)).toBe("4.4:1");
  expect(ratioText(4.5)).toBe("4.5:1");
  expect(ratioText(2.999)).toBe("2.9:1");
  expect(ratioText(12.04)).toBe("12.0:1");
});

test("the fix reaches the mark on every surface, keeps the hue, and leaves a passing colour as it is", () => {
  const surfaces = { page: "#E4E8EC", panel: "#F7F8F9", inset: "#EEF1F4" };
  const fixed = fix("#28A745", surfaces);
  expect(worst(fixed, surfaces).ratio).toBeGreaterThanOrEqual(4.5);
  const hue = (hex: string) => (toOklch(hex)[2] * 180) / Math.PI;
  expect(Math.abs(hue(fixed) - hue("#28A745"))).toBeLessThan(2);
  expect(fix("#17212B", surfaces)).toBe("#17212B");
  // Checked on the first surface only, a colour passing on the panel but not the page would come back short.
  expect(worst(fix("#5F6975", { panel: "#FFFFFF", page: "#C8CDD2" }), { panel: "#FFFFFF", page: "#C8CDD2" }).ratio).toBeGreaterThanOrEqual(4.5);
});

test("a colour that cannot reach the mark darker reaches it lighter, and one that can do neither is black or white", () => {
  // Mid-grey surfaces on the light side of the middle: darker cannot reach 4.5 from a dark blue, lighter can.
  const surfaces = { a: "#5A5A5A", b: "#606060" };
  const fixed = fix("#20307A", surfaces, 4.5, "darker");
  expect(worst(fixed, surfaces).ratio).toBeGreaterThanOrEqual(4.5);
  expect(contrast(fixed, "#000000")).toBeGreaterThan(contrast("#20307A", "#000000"));
  expect(["#000000", "#FFFFFF"]).toContain(fix("#777777", { a: "#000000", b: "#FFFFFF" }, 21));
});

test("mix is OKLab's, as color-mix(in oklab) draws it", () => {
  expect(mix("#000000", "#FFFFFF", 1)).toBe("#000000");
  expect(mix("#000000", "#FFFFFF", 0)).toBe("#FFFFFF");
  // Half of black in white is OKLab lightness 0.5: a quarter of the light, #636363.
  expect(mix("#000000", "#FFFFFF", 0.5)).toBe("#636363");
});

test("the colour-vision simulations hold Machado's matrices: what each kind of reader loses", () => {
  // Pure red and green look alike to protan and deutan readers, not to tritan ones.
  expect(deltaE(simulate("#FF0000", "deutan"), simulate("#00A000", "deutan"))).toBeLessThan(deltaE("#FF0000", "#00A000") / 2);
  expect(deltaE(simulate("#FF0000", "protan"), simulate("#00A000", "protan"))).toBeLessThan(deltaE("#FF0000", "#00A000") / 2);
  // Blue and yellow stay apart for red-green readers and come together for tritan ones.
  expect(deltaE(simulate("#0000FF", "deutan"), simulate("#FFFF00", "deutan"))).toBeGreaterThan(0.5);
  expect(deltaE(simulate("#00A0A0", "tritan"), simulate("#3080C0", "tritan"))).toBeLessThan(deltaE("#00A0A0", "#3080C0"));
  // Grey is grey to everyone.
  for (const kind of ["protan", "deutan", "tritan"] as const) expect(deltaE(simulate("#808080", kind), "#808080")).toBeLessThan(0.01);
  // White and black do not move.
  expect(simulate("#FFFFFF", "deutan")).toBe("#FFFFFF");
  expect(simulate("#000000", "tritan")).toBe("#000000");
});

test("two colours' distance is their closest under any vision: knobas Light's Attention and Failed are 0.018 apart for a deutan reader", () => {
  const a = apart("#8A5600", "#B0262A");
  expect(a.under).toBe("deutan");
  expect(a.delta).toBeCloseTo(0.018, 3);
});
