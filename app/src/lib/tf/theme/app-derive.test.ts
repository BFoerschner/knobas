/** The window's colours a theme leaves out, derived from the ones it sets (ADR-0055, the decisions' 3.4). */
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "vitest";

import { failures } from "./app-check";
import { type Colours, derive, KNOBAS } from "./app-derive";
import { APP_KEYS, KEYS, ROW_KEYS } from "./app-roles";
import { contrast, deltaE, toOklch } from "./colour";

const tokens = readFileSync(join(process.cwd(), "src/lib/tf/css/tokens.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
const blockOf = (selector: string, from = 0) => {
  const at = tokens.indexOf(`${selector} {`, from);
  return tokens.slice(at, tokens.indexOf("}", at));
};

test("knobas's own two themes are the empty set: derived from nothing, every colour is knobas's", () => {
  for (const kind of ["light", "dark"] as const) {
    const { colours, derived } = derive(kind, {});
    expect(colours).toEqual(KNOBAS[kind]);
    expect(derived.size).toBe(KEYS.length - 1);
    expect("selection" in colours).toBe(false);
  }
});

test("tokens.css gives every key its token in all three blocks, reading its side's variable, with KNOBAS's value behind it", () => {
  const media = tokens.indexOf("@media (prefers-color-scheme: dark)");
  const blocks = [
    ["light", blockOf(":root")],
    ["dark", blockOf(':root:not([data-theme="light"])', media)],
    ["dark", blockOf(':root[data-theme="dark"]')],
  ] as const;
  for (const [side, body] of blocks) {
    const decls = Object.fromEntries([...body.matchAll(/(--[a-z0-9-]+):\s*([^;]+);/g)].map((m) => [m[1]!, m[2]!.trim()]));
    /** A fallback that is another token resolves through that token's own fallback in the same block. */
    const resolve = (value: string): string => {
      const ref = /^var\((--[a-z0-9-]+)\)$/.exec(value);
      if (!ref) return value;
      const inner = decls[ref[1]!];
      expect(inner, `${ref[1]} is in the ${side} block`).toBeDefined();
      return resolve(/^var\(--[a-z0-9-]+-(?:light|dark)(?:, (.+))?\)$/.exec(inner!)?.[1] ?? inner!);
    };
    for (const [key, token] of APP_KEYS) {
      const value = decls[token];
      expect(value, `${side} ${token}`).toBeDefined();
      const m = new RegExp(`^var\\(${token}-${side}(?:, (.+))?\\)$`).exec(value!);
      expect(m, `${token} reads ${token}-${side}: ${value}`).not.toBeNull();
      if (key === "selection") expect(m![1], "selected text has no fallback: the system's own draws").toBeUndefined();
      else expect(resolve(m![1]!).toUpperCase(), `${side} ${token}`).toBe(KNOBAS[side][key]);
    }
  }
});

test("from knobas's panel and text alone, every derived colour is near knobas's own, the mean well under a step an eye sees", () => {
  for (const kind of ["light", "dark"] as const) {
    const { colours } = derive(kind, { sheet: KNOBAS[kind].sheet, ink: KNOBAS[kind].ink });
    const distances = KEYS.filter((k) => k !== "selection").map((k) => deltaE(colours[k as keyof Colours]!, KNOBAS[kind][k as keyof Colours]!));
    expect(Math.max(...distances), kind).toBeLessThan(0.09);
    expect(distances.reduce((a, b) => a + b, 0) / distances.length, kind).toBeLessThan(0.01);
    expect(failures(colours), kind).toEqual([]);
  }
});

/** The fifteen pairs of the decisions' 3.4, each its text on its panel at 4.5:1 or more. */
const PAIRS: [string, string, string][] = [
  ["black on white", "#FFFFFF", "#000000"],
  ["white on black", "#000000", "#FFFFFF"],
  ["Dracula", "#282A36", "#F8F8F2"],
  ["Solarized Light", "#FDF6E3", "#586E75"],
  ["Solarized Dark", "#002B36", "#93A1A1"],
  ["Gruvbox Light", "#FBF1C7", "#3C3836"],
  ["Gruvbox Dark", "#282828", "#EBDBB2"],
  ["Nord", "#2E3440", "#D8DEE9"],
  ["GitHub Light", "#FFFFFF", "#24292E"],
  ["One Dark", "#282C34", "#ABB2BF"],
  ["Tokyo Night", "#1A1B26", "#A9B1D6"],
  ["purple on lilac", "#F5E6FF", "#3B1F5E"],
  ["mint on near-black", "#101418", "#7FD1B9"],
  ["black on grey", "#808080", "#000000"],
  ["grey on near-white", "#EEEEEE", "#666666"],
];

test("a theme of two colours has no contrast failure, for each of fifteen pairs", () => {
  for (const [name, sheet, ink] of PAIRS) {
    expect(contrast(sheet, ink), name).toBeGreaterThanOrEqual(4.5);
    const kind = contrast(sheet, "#000000") > contrast(sheet, "#FFFFFF") ? "light" : "dark";
    expect(failures(derive(kind, { sheet, ink }).colours), name).toEqual([]);
  }
});

test("a colour a theme sets is never changed by the derivation", () => {
  const { colours } = derive("dark", { sheet: "#1E1E2E", ink: "#CDD6F4" });
  const set = Object.fromEntries(ROW_KEYS.map((k) => [k, colours[k as keyof Colours] ?? "#3B3D4F"]));
  // A secondary text too faint to pass is still the one the theme set.
  set.ink2 = "#4A4D60";
  expect(ROW_KEYS).toHaveLength(32);
  const again = derive("dark", set).colours;
  for (const key of ROW_KEYS) expect(again[key as keyof Colours], key).toBe(set[key]);
});

test("selected text is left to the system unless a theme sets it", () => {
  expect(derive("light", { sheet: "#FFFFFF" }).colours.selection).toBeUndefined();
  expect(derive("light", { selection: "#DAE9F9" }).colours.selection).toBe("#DAE9F9");
});

test("a copy of knobas Light with a purple Passed changes Passed's tint, its row colour and its word mark, and nothing else", () => {
  const { colours } = derive("light", { ok: "#7A3EB1" });
  const changed = KEYS.filter((k) => k !== "selection" && k !== "ok" && colours[k as keyof Colours] !== KNOBAS.light[k as keyof Colours]);
  expect(changed.sort()).toEqual(["okBg", "okWord", "onInkOk"].sort());
});

test("a theme's card is its own: derived from the Attention it sets, knobas's yellow where it sets none", () => {
  const dracula = derive("dark", { sheet: "#282A36", ink: "#F8F8F2", warn: "#F1FA8C" }).colours;
  const [, , hue] = toOklch(dracula.card);
  expect(Math.abs((hue * 180) / Math.PI - (toOklch("#F1FA8C")[2] * 180) / Math.PI)).toBeLessThan(2);
  expect(dracula.card).not.toBe(KNOBAS.dark.card);
  expect(failures(dracula)).toEqual([]);
  expect(derive("dark", { sheet: "#282A36", ink: "#F8F8F2" }).colours.card).toBe(KNOBAS.dark.card);
  // A card set by the theme is the theme's, whatever its Attention.
  expect(derive("light", { warn: "#905C0C", card: "#C8E6FF" }).colours.card).toBe("#C8E6FF");
});
