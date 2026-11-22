/** A theme's code colours on every surface code is drawn on, and the lightness fix (ADR-0051, ADR-0055). */
import { expect, test } from "vitest";

import { ROLES } from "../../render/roles";
import { derive, KNOBAS } from "./app-derive";
import { BUILTINS } from "./builtins";
import { contrast, diffDefault, fixedRole, lowSentence, mixOklab, roleContrast, surfacesFor, surfacesOf, toOklch, worst } from "./contrast";

const ownWindow = (id: string) => {
  const t = BUILTINS.find((b) => b.id === id)!;
  return derive(t.kind, t.app ?? {}).colours;
};

test("the surfaces are the window theme's: knobas's own are tokens.css's, and Dracula's are Dracula's", () => {
  expect(surfacesOf(KNOBAS.light)).toEqual({ sheet: "#F7F8F9", "sheet-2": "#EEF1F4", paper: "#E4E8EC", "added line": "#DDEFE4", "removed line": "#F7DEDD" });
  expect(surfacesOf(KNOBAS.dark).sheet).toBe("#1A2027");
  expect(surfacesOf(ownWindow("dracula")).sheet).toBe("#282A36");
  expect(diffDefault(KNOBAS.dark)).toEqual({ added: "#1B3427", removed: "#3F1F1F", addedWord: "#2C6243", removedWord: "#6E3333" });
});

test("knobas Light's roles have the design system's figures", () => {
  const knobas = BUILTINS[0]!;
  expect(contrast("#7A3EB1", "#F7F8F9")).toBeCloseTo(6.27, 2);
  expect(contrast("#A2461B", "#F7F8F9")).toBeCloseTo(5.76, 2);
  expect(roleContrast(KNOBAS.light, "kw", knobas.syntax.kw).ratio).toBeCloseTo(Math.min(...Object.values(surfacesOf(KNOBAS.light)).map((bg) => contrast("#7A3EB1", bg))), 5);
  expect(roleContrast(KNOBAS.light, "prop", undefined).ratio).toBeCloseTo(Math.min(...Object.values(surfacesOf(KNOBAS.light)).map((bg) => contrast(KNOBAS.light.ink, bg))), 5);
});

test("every built-in role passes 4.5:1 on knobas's surfaces of its side (on its own window a few fall under, and its editor offers the fix)", () => {
  const failing: string[] = [];
  for (const theme of BUILTINS)
    for (const window of [KNOBAS[theme.kind]])
      for (const role of ROLES) {
        const { ratio, on } = roleContrast(window, role, theme.syntax[role]);
        if (ratio < 4.5) failing.push(`${theme.name} ${role} ${ratio.toFixed(2)} on ${on} of ${window.sheet}`);
      }
  expect(failing).toEqual([]);
});

test("One Dark keeps its own keyword (Björn, 2026-11-22), which is 3.8:1 on its own added line, so its row is marked and fixable", () => {
  const oneDark = ownWindow("one-dark");
  expect(BUILTINS.find((t) => t.id === "one-dark")!.syntax.kw).toBe("#C678DD");
  const kw = roleContrast(oneDark, "kw", "#C678DD");
  expect([kw.on, kw.low]).toEqual(["added line", true]);
  expect(kw.ratio).toBeCloseTo(3.8, 1);
  expect(roleContrast(KNOBAS.dark, "kw", "#C678DD").low).toBe(false);
});

test("the fix reaches 4.5 on every surface, keeps the hue, and moves a passing colour not at all", () => {
  for (const [kind, colour] of [["light", "#E36209"], ["light", "#6A737D"], ["dark", "#5C6370"], ["dark", "#6272A4"]] as const) {
    const surfaces = surfacesFor(KNOBAS[kind], "ty");
    expect(worst(colour, surfaces).ratio).toBeLessThan(4.5);
    const fixed = fixedRole(KNOBAS[kind], kind, "ty", colour)!;
    expect(worst(fixed, surfaces).ratio).toBeGreaterThanOrEqual(4.5);
    const [, c0, h0] = toOklch(colour);
    const [, , h1] = toOklch(fixed);
    if (c0 > 0.02) expect(Math.abs(((h1 - h0 + Math.PI * 3) % (Math.PI * 2)) - Math.PI) * (180 / Math.PI)).toBeLessThan(2);
  }
  expect(fixedRole(KNOBAS.light, "light", "kw", "#7A3EB1")).toBeNull();
});

test("a colour that passes on the sheet and fails on a removed line is fixed for the removed line", () => {
  const colour = "#C2410C";
  expect(contrast(colour, KNOBAS.light.sheet)).toBeGreaterThan(4.5 - 0.6);
  const fixed = fixedRole(KNOBAS.light, "light", "kw", colour)!;
  expect(contrast(fixed, KNOBAS.light.errBg)).toBeGreaterThanOrEqual(4.5);
});

test("a comment is not checked on the diff's lines, where it draws in the secondary ink", () => {
  expect(Object.keys(surfacesFor(KNOBAS.light, "com"))).toEqual(["sheet", "sheet-2", "paper"]);
  expect(Object.keys(surfacesFor(KNOBAS.light, "kw"))).toContain("removed line");
});

test("the sentence names the surface, its figure cut and never rounded up", () => {
  expect(lowSentence(2.54, "removed line")).toBe("2.5:1 on a removed line. Text needs 4.5:1.");
  expect(lowSentence(3, "added line")).toBe("3.0:1 on an added line. Text needs 4.5:1.");
  expect(lowSentence(4.45, "sheet")).toBe("4.4:1 on the sheet. Text needs 4.5:1.");
});

test("a quiet line's colour, 40 % of a built-in role mixed into the secondary ink, passes 4.5:1 on the sheet", () => {
  const failing: string[] = [];
  for (const theme of BUILTINS)
    for (const c of [KNOBAS[theme.kind]])
      for (const role of ROLES) {
        const mixed = mixOklab(theme.syntax[role] ?? c.ink, c.ink2, 0.4);
        const ratio = contrast(mixed, c.sheet);
        if (ratio < 4.5) failing.push(`${theme.name} ${role} ${ratio.toFixed(2)} on ${c.sheet}`);
      }
  expect(failing).toEqual([]);
});
