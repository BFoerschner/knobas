/** What a theme's window colours are checked against, and the fixes (ADR-0055, the decisions' 3.8 and 3.9). */
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "vitest";

import { apartLine, closestStates, failures, fixAll, fixRow, fixWords, lowSentence, pairs, rowFigure, tellApart, unreadable } from "./app-check";
import { derive, KNOBAS } from "./app-derive";
import { appFromVsCode } from "./app-import";
import { contrast, toOklch, worst } from "./colour";

const FIXTURES = join(process.cwd(), "src/lib/tf/theme/fixtures");
const fixture = (id: string) => JSON.parse(readFileSync(join(FIXTURES, `${id}.colors.json`), "utf8")) as Record<string, unknown>;

test("knobas Light and knobas Dark pass every check and are readable", () => {
  for (const kind of ["light", "dark"] as const) {
    expect(failures(KNOBAS[kind]), kind).toEqual([]);
    expect(unreadable(KNOBAS[kind]), kind).toEqual([]);
  }
});

test("a state is checked on the inset too: GitHub Light's own green is 2.7:1 there", () => {
  const { kind, set } = appFromVsCode(fixture("github-light"));
  const bad = failures(derive(kind, { ...set, ok: "#28A745" }).colours).filter((f) => f.key === "ok");
  expect(bad).toHaveLength(1);
  expect(bad[0]!.on).toBe("the inset");
  expect(bad[0]!.ratio).toBeCloseTo(2.73, 1);
});

test("a primary button's words stay white and the button darkens under them", () => {
  const set = { sheet: "#FFFFFF", ink: "#24292E", primary: "#159739", onPrimary: "#FFFFFF" };
  const fixed = fixAll("light", set);
  expect(fixed.onPrimary).toBe("#FFFFFF");
  expect(fixed.primary).toBe("#006423");
  const c = derive("light", set).colours;
  expect(fixWords("light", c, failures(c).find((f) => f.key === "onPrimary")!)).toBe("Darken the button to 4.5:1");
});

test("Fix all leaves no failure on five real themes, cut down to their colours", () => {
  for (const id of ["dracula", "tokyo-night", "solarized-light", "github-light", "material-theme-lighter"]) {
    const { kind, set } = appFromVsCode(fixture(id));
    expect(failures(derive(kind, fixAll(kind, set)).colours), id).toEqual([]);
  }
});

/** Every VS Code theme Shiki ships with workbench colours, as the decisions' figures were taken. */
const SHIKI = join(process.cwd(), "node_modules/@shikijs/themes/dist");
test("of Shiki's VS Code themes, most arrive with a colour under its mark and none is left after Fix all", async () => {
  const ids = readdirSync(SHIKI).filter((f) => f.endsWith(".mjs") && f !== "index.mjs");
  let themes = 0;
  let arriving = 0;
  for (const file of ids) {
    const theme = (await import(/* @vite-ignore */ join(SHIKI, file))).default as Record<string, unknown>;
    if (!theme.colors) continue;
    themes += 1;
    const { kind, set } = appFromVsCode(theme);
    if (failures(derive(kind, set).colours).length > 0) arriving += 1;
    expect(failures(derive(kind, fixAll(kind, set)).colours), file).toEqual([]);
  }
  expect(themes).toBe(65);
  expect(arriving).toBe(50);
});

test("a row's figure is its lowest ratio, and its sentence says the pair in the low row's words", () => {
  const c = derive("light", { sheet: "#FDF6E3", ink: "#586E75", ok: "#859900" }).colours;
  const f = rowFigure(c, ["ok", "okBg"])!;
  expect(f.ratio).toBeLessThan(4.5);
  expect(lowSentence(f)).toMatch(/^\d\.\d:1 on (the page|the panel|the inset|its tint)\. Text needs 4\.5:1\.$/);
  expect(lowSentence({ key: "focus", ratio: 2.29, on: "the panel", want: 3 })).toBe("2.2:1 on the panel. A part needs 3:1.");
  // One row's fix leaves that row passing.
  const fixed = derive("light", fixRow("light", { sheet: "#FDF6E3", ink: "#586E75", ok: "#859900" }, ["ok", "okBg"])).colours;
  expect(rowFigure(fixed, ["ok", "okBg"])!.ratio).toBeGreaterThanOrEqual(4.5);
});

test("every check reads the colour's own surfaces: the fix holds on all of them", () => {
  const c = derive("dark", { sheet: "#282A36", ink: "#F8F8F2", err: "#FF5555" }).colours;
  const fixed = derive("dark", fixAll("dark", { sheet: "#282A36", ink: "#F8F8F2", err: "#FF5555" })).colours;
  for (const [key, surfaces, want] of pairs(fixed)) expect(worst(fixed[key]!, surfaces).ratio, key).toBeGreaterThanOrEqual(want);
  expect(failures(c).map((f) => f.key)).toContain("err");
});

test("unreadable names the pairs under 3:1, and Material Theme Lighter's text on the field is one", () => {
  const { kind, set } = appFromVsCode(fixture("material-theme-lighter"));
  const u = unreadable(derive(kind, set).colours);
  expect(u.map((f) => [f.key, f.on])).toContainEqual(["ink", "the field"]);
  expect(u[0]!.ratio).toBeLessThan(3);
  expect(u[0]!.ratio).toBeGreaterThan(2);
  // At a floor of 2, it would pass.
  expect(u.every((f) => f.ratio >= 2)).toBe(true);
});

test("the closest states: knobas Light's Attention and Failed for a red-green colour-blind reader", () => {
  const cs = closestStates(KNOBAS.light);
  expect([cs.a, cs.b, cs.under]).toEqual(["warn", "err", "deutan"]);
  expect(cs.delta).toBeCloseTo(0.018, 3);
  expect(apartLine(KNOBAS.light)).toBe("Attention and Failed are 0.02 apart for red-green colour blindness. 0.08 reads as two colours.");
});

test("Tell them apart moves knobas Light's states to 0.08 apart, each still 4.5:1, and leaves a theme already apart alone", () => {
  const got = tellApart(KNOBAS.light);
  // Running moves in lightness only, and Unknown's grey stays: all five states apart (the final suite, 2026-11-22).
  expect([got.ok, got.warn, got.err, got.info]).toEqual(["#004D3F", "#905C0C", "#890012", "#1C3BB9"]);
  expect(got.reached).toBeGreaterThanOrEqual(0.08);
  const three = { paper: KNOBAS.light.paper, sheet: KNOBAS.light.sheet, sheet2: KNOBAS.light.sheet2 };
  for (const hex of [got.ok, got.warn, got.err, got.info]) expect(worst(hex, three).ratio).toBeGreaterThanOrEqual(4.5);
  const [, c0, h0] = toOklch(KNOBAS.light.info);
  const [, c1, h1] = toOklch(got.info);
  expect(Math.abs(c1 - c0), "its chroma").toBeLessThan(0.01);
  expect(Math.abs(h1 - h0), "its hue").toBeLessThan(0.03);
  const apart = { ...KNOBAS.light, ok: got.ok, warn: got.warn, err: got.err, info: got.info };
  expect(closestStates(apart).delta).toBeGreaterThanOrEqual(0.08);
  expect(tellApart(apart)).toMatchObject({ ok: got.ok, warn: got.warn, err: got.err, info: got.info });
  expect(apartLine(apart)).toMatch(/^Told apart: the closest two, \w+ and \w+, are 0\.\d\d apart\.$/);
});

test("a selected row is its own pair (Björn, 2026-11-22): the ink plate until set, then checked at 4.5:1 and floored at 3:1", () => {
  // Unset, it is today's look: the pressed plate and its words.
  expect([KNOBAS.light.selRow, KNOBAS.light.onSelRow]).toEqual([KNOBAS.light.fill, KNOBAS.light.onFill]);
  expect(derive("dark", { sheet: "#282A36", ink: "#F8F8F2" }).colours.selRow).toBe(derive("dark", { sheet: "#282A36", ink: "#F8F8F2" }).colours.fill);
  // Set to an accent, its words are the more readable of the panel and the text on it.
  const accent = derive("light", { sheet: "#FDF6E3", ink: "#586E75", selRow: "#268BD2" }).colours;
  expect(accent.onSelRow).toBe(contrastPick("#FDF6E3", "#586E75", "#268BD2"));
  const low = failures(accent).find((f) => f.key === "onSelRow")!;
  expect(low.on).toBe("a selected row");
  expect(low.ratio).toBeLessThan(4.5);
  expect(unreadable(accent).map((f) => f.key)).not.toContain("onSelRow");
  // Under 3:1 it joins the floor, as text on what is pressed does.
  const faint = derive("light", { selRow: "#9AA0A6", onSelRow: "#B0B5BA" }).colours;
  expect(unreadable(faint).map((f) => f.key)).toContain("onSelRow");
  // Its fix moves the row, not its words.
  const fixed = fixAll("light", { sheet: "#FDF6E3", ink: "#586E75", selRow: "#268BD2" });
  expect(failures(derive("light", fixed).colours).filter((f) => f.key === "onSelRow")).toEqual([]);
  expect(fixed.selRow).not.toBe("#268BD2");
});

const contrastPick = (sheet: string, ink: string, row: string) => (contrast(sheet, row) >= contrast(ink, row) ? sheet : ink);
