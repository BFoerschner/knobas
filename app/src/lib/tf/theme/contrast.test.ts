/** A theme's colours on every surface code is drawn on, and the lightness fix (ADR-0051). */
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "vitest";

import { ROLES } from "../../render/roles";
import { BUILTINS } from "./builtins";
import { contrast, fix, INK, INK_2, lowSentence, mixOklab, roleContrast, SURFACES, surfacesFor, toOklch, worst } from "./contrast";

const tokens = readFileSync(join(process.cwd(), "src/lib/tf/css/tokens.css"), "utf8");
const valueIn = (block: string, name: string) => new RegExp(`${name}:\\s*(#[0-9A-F]{6})`, "i").exec(block)?.[1]?.toUpperCase();

test("the surfaces and inks are the stylesheet's, light and dark", () => {
  const light = tokens.slice(tokens.indexOf(":root {"), tokens.indexOf("@media"));
  const dark = tokens.slice(tokens.indexOf(':root[data-theme="dark"]'));
  for (const [kind, block] of [["light", light], ["dark", dark]] as const) {
    expect(SURFACES[kind]).toEqual({
      sheet: valueIn(block, "--sheet"),
      "sheet-2": valueIn(block, "--sheet-2"),
      paper: valueIn(block, "--paper"),
      "added line": valueIn(block, "--ok-bg"),
      "removed line": valueIn(block, "--err-bg"),
    });
    expect(INK[kind]).toBe(valueIn(block, "--ink"));
    expect(INK_2[kind]).toBe(valueIn(block, "--ink-2"));
  }
});

test("knobas Light's roles have the design system's figures", () => {
  const knobas = BUILTINS[0]!;
  // The figures the design system's Colour section lists for the role tokens on `--sheet`.
  expect(contrast("#7A3EB1", "#F7F8F9")).toBeCloseTo(6.27, 2);
  expect(contrast("#A2461B", "#F7F8F9")).toBeCloseTo(5.76, 2);
  // A role's figure is its lowest over the five surfaces; a role in ink is checked in ink.
  expect(roleContrast("light", "kw", knobas.syntax.kw).ratio).toBeCloseTo(Math.min(...Object.values(SURFACES.light).map((bg) => contrast("#7A3EB1", bg))), 5);
  expect(roleContrast("light", "prop", undefined).ratio).toBeCloseTo(Math.min(...Object.values(SURFACES.light).map((bg) => contrast(INK.light, bg))), 5);
});

test("every built-in role passes 4.5:1 on every surface of its side", () => {
  const failing: string[] = [];
  for (const theme of BUILTINS)
    for (const role of ROLES) {
      const { ratio, on } = roleContrast(theme.kind, role, theme.syntax[role]);
      if (ratio < 4.5) failing.push(`${theme.name} ${role} ${ratio.toFixed(2)} on ${on}`);
    }
  expect(failing).toEqual([]);
});

test("the fix reaches 4.5 on every surface, keeps the hue, and moves a passing colour not at all", () => {
  // GitHub Light's orange as published: 2.7:1 on a removed line.
  for (const [kind, colour] of [["light", "#E36209"], ["light", "#6A737D"], ["dark", "#5C6370"], ["dark", "#6272A4"]] as const) {
    const surfaces = surfacesFor(kind, "ty");
    expect(worst(colour, surfaces).ratio).toBeLessThan(4.5);
    const fixed = fix(colour, surfaces, kind);
    expect(worst(fixed, surfaces).ratio).toBeGreaterThanOrEqual(4.5);
    const [, c0, h0] = toOklch(colour);
    const [, , h1] = toOklch(fixed);
    if (c0 > 0.02) expect(Math.abs(((h1 - h0 + Math.PI * 3) % (Math.PI * 2)) - Math.PI) * (180 / Math.PI)).toBeLessThan(2);
  }
  expect(fix("#7A3EB1", surfacesFor("light", "kw"), "light")).toBe("#7A3EB1");
});

test("a colour that passes on the sheet and fails on a removed line is fixed for the removed line", () => {
  const surfaces = surfacesFor("light", "kw");
  const colour = "#C2410C";
  expect(contrast(colour, SURFACES.light.sheet)).toBeGreaterThan(4.5 - 0.6);
  const fixed = fix(colour, surfaces, "light");
  expect(contrast(fixed, SURFACES.light["removed line"])).toBeGreaterThanOrEqual(4.5);
});

test("a comment is not checked on the diff's lines, where it draws in the secondary ink", () => {
  expect(Object.keys(surfacesFor("light", "com"))).toEqual(["sheet", "sheet-2", "paper"]);
  expect(Object.keys(surfacesFor("light", "kw"))).toContain("removed line");
});

test("the sentence names the surface", () => {
  expect(lowSentence(2.54, "removed line")).toBe("2.5:1 on a removed line. Text needs 4.5:1.");
  expect(lowSentence(3, "added line")).toBe("3.0:1 on an added line. Text needs 4.5:1.");
});

test("a quiet line's colour, 40 % of a built-in role mixed into the secondary ink, passes 4.5:1 on the sheet", () => {
  const failing: string[] = [];
  for (const theme of BUILTINS)
    for (const role of ROLES) {
      const mixed = mixOklab(theme.syntax[role] ?? INK[theme.kind], INK_2[theme.kind], 0.4);
      const ratio = contrast(mixed, SURFACES[theme.kind].sheet);
      if (ratio < 4.5) failing.push(`${theme.name} ${role} ${ratio.toFixed(2)}`);
    }
  expect(failing).toEqual([]);
});
