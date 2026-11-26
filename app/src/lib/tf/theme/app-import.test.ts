/** A VS Code theme's `colors`, a Zed theme's `style` and a Base16 scheme read onto the window's colours (ADR-0055, 3.10). */
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "vitest";

import { failures, fixAll } from "./app-check";
import { derive } from "./app-derive";
import { appFromVsCode, appFromZed, kindOfVsCode, readBase16 } from "./app-import";
import { contrast } from "./colour";
import { hasApp, hasCode, importThemes, type ImportedTheme } from "./themes";

const FIXTURES = join(process.cwd(), "src/lib/tf/theme/fixtures");
const read = (name: string) => readFileSync(join(FIXTURES, name), "utf8");
const themesOf = (text: string): ImportedTheme[] & { includes?: string } => {
  const r = importThemes(text);
  if ("refused" in r) throw new Error(r.refused);
  return Object.assign(r.themes, { includes: r.includes });
};

/** A small VS Code theme where several keys of a row are present, so the first one present must win. */
const VSCODE = {
  name: "Forty Lines",
  type: "dark",
  colors: {
    "editor.background": "#1E1E1E",
    "editor.foreground": "#D4D4D4",
    foreground: "#FFFFFF",
    "sideBar.background": "#181818",
    "activityBar.background": "#333333",
    "list.hoverBackground": "#2A2D2E",
    "editor.lineHighlightBackground": "#282828",
    "input.background": "#252525",
    "dropdown.background": "#3C3C3C",
    "editorGroup.border": "#444444",
    "panel.border": "#808080",
    "button.background": "#4A9CE8",
    "button.foreground": "#FFFFFF",
    focusBorder: "#007FD4",
    "textLink.foreground": "#3794FF",
    "editor.findMatchHighlightBackground": "#EA5C0055",
    "editor.selectionBackground": "#264F78",
    "terminal.ansiGreen": "#0DBC79",
    "gitDecoration.addedResourceForeground": "#81B88B",
    "terminal.ansiYellow": "#E5E510",
    "terminal.ansiRed": "#CD3131",
    errorForeground: "#F48771",
    "terminal.ansiBlue": "#2472C8",
  },
};

test("a VS Code theme's colours are read by the table, each from the first key of its row that is present", () => {
  const { kind, set, from } = appFromVsCode(VSCODE);
  expect(kind).toBe("dark");
  expect(set.sheet).toBe("#1E1E1E");
  expect([set.ink, from.ink]).toEqual(["#D4D4D4", "editor.foreground"]);
  expect([set.paper, from.paper]).toEqual(["#181818", "sideBar.background"]);
  expect([set.sheet2, from.sheet2]).toEqual(["#2A2D2E", "list.hoverBackground"]);
  expect([set.field, from.field]).toEqual(["#252525", "input.background"]);
  expect([set.rule, from.rule]).toEqual(["#444444", "editorGroup.border"]);
  expect([set.primary, set.onPrimary]).toEqual(["#4A9CE8", "#FFFFFF"]);
  // A button that does not stand off the panel at 3:1 is refused, and its words with it.
  expect(appFromVsCode({ ...VSCODE, colors: { ...VSCODE.colors, "button.background": "#0E639C" } }).set.primary).toBeUndefined();
  expect([set.ok, from.ok]).toEqual(["#0DBC79", "terminal.ansiGreen"]);
  expect([set.err, from.err]).toEqual(["#CD3131", "terminal.ansiRed"]);
  expect(set.info).toBe("#2472C8");
});

test("a translucent colour is laid over the panel: white at 25 % on Dracula's panel is #5E5F68, not white", () => {
  const { set } = appFromVsCode({ type: "dark", colors: { "editor.background": "#282A36", "editor.foreground": "#F8F8F2", "editor.findMatchHighlightBackground": "#FFFFFF40" } });
  expect(set.mark).toBe("#5E5F68");
});

test("the guards: Dracula's purple border is refused and derived; a light theme's dark side bar is no page", () => {
  const dracula = JSON.parse(read("dracula.colors.json"));
  const { set, from } = appFromVsCode(dracula);
  expect(dracula.colors["panel.border"]).toMatch(/^#BD93F9/i);
  expect(from.rule).not.toBe("panel.border");
  expect(Math.max(contrast(set.rule ?? derive("dark", set).colours.rule, set.sheet!), 1)).toBeLessThanOrEqual(2.2);
  const light = appFromVsCode({ type: "light", colors: { "editor.background": "#FFFFFF", "editor.foreground": "#333333", "sideBar.background": "#2D3E4C" } });
  expect(light.set.paper).toBeUndefined();
  expect(derive("light", light.set).derived.has("paper")).toBe(true);
});

test("a type that says dark gives way to a light editor background", () => {
  expect(kindOfVsCode({ type: "dark", colors: { "editor.background": "#FAFAFA" } })).toBe("light");
  expect(kindOfVsCode({ colors: { "editor.background": "#101010" } })).toBe("dark");
  expect(kindOfVsCode({ name: "Some Light" })).toBe("light");
});

test("a file with colors only has no code part, and one with tokenColors only no window part", () => {
  const [appOnly] = themesOf(JSON.stringify({ name: "A", type: "dark", colors: { "editor.background": "#101010" } }));
  expect(hasApp(appOnly!)).toBe(true);
  expect(hasCode(appOnly!)).toBe(false);
  const [codeOnly] = themesOf(JSON.stringify({ name: "C", type: "dark", tokenColors: [{ scope: "keyword", settings: { foreground: "#FF0000" } }] }));
  expect(hasApp(codeOnly!)).toBe(false);
  expect(codeOnly!.syntax.kw).toBe("#FF0000");
});

test("an include is not followed, and the import says which file it was", () => {
  const got = themesOf(JSON.stringify({ name: "Inc", type: "dark", include: "./dark-defaults.json", colors: { "editor.background": "#101010" } }));
  expect(got.includes).toBe("dark-defaults.json");
});

test("Zed's own One and Ayu families: every theme has both parts, read from the keys Zed names, and none fails after Fix all", () => {
  for (const [file, names] of [
    ["zed-one.json", ["One Dark", "One Light"]],
    ["zed-ayu.json", ["Ayu Dark", "Ayu Light", "Ayu Mirage"]],
  ] as const) {
    const json = JSON.parse(read(file));
    const got = themesOf(read(file));
    expect(got.map((t) => t.name), file).toEqual(names);
    got.forEach((t, i) => {
      expect(hasApp(t) && hasCode(t), t.name).toBe(true);
      const { from } = appFromZed(json.themes[i].style, t.kind);
      expect(from.sheet, t.name).toBe("editor.background");
      expect(from.ink, t.name).toBe("editor.foreground");
      expect([from.ok, from.warn, from.err, from.info], t.name).toEqual(["success", "warning", "error", "info"]);
      expect(failures(derive(t.kind, fixAll(t.kind, t.app!)).colours), t.name).toEqual([]);
    });
  }
});

const BASE16 = `scheme: "Tomorrow Night"
author: "Chris Kempson"
base00: "1d1f21"
base01: "282a2e"
base02: "373b41"
base03: "969896"
base04: "b4b7b4"
base05: "c5c8c6"
base06: "e0e0e0"
base07: "ffffff"
base08: "cc6666"
base09: "de935f"
base0A: "f0c674"
base0B: "b5bd68"
base0C: "8abeb7"
base0D: "81a2be"
base0E: "b294bb"
base0F: "a3685a"
`;

test("a Base16 scheme of sixteen lines, as YAML, gives a theme with both parts and no failure after Fix all", () => {
  expect(readBase16(BASE16)?.name).toBe("Tomorrow Night");
  const [t] = themesOf(BASE16);
  expect(t!.name).toBe("Tomorrow Night");
  expect(t!.kind).toBe("dark");
  expect(t!.app?.sheet).toBe("#1D1F21");
  expect(t!.syntax.kw).toBe("#B294BB");
  expect(failures(derive(t!.kind, fixAll(t!.kind, t!.app!)).colours)).toEqual([]);
  // Fifteen lines are not a scheme.
  expect(readBase16(BASE16.replace(/base0F.*\n/, ""))).toBeNull();
});

test("a theme's own selected row is offered, never set: VS Code's list selection, Zed's selected element", () => {
  const { set, offers } = appFromVsCode({ type: "dark", colors: { "editor.background": "#1E1E1E", "editor.foreground": "#D4D4D4", "list.activeSelectionBackground": "#04395E", "list.activeSelectionForeground": "#FFFFFF" } });
  expect(set.selRow).toBeUndefined();
  expect(offers).toEqual({ selRow: "#04395E", onSelRow: "#FFFFFF" });
  const [t] = themesOf(JSON.stringify({ name: "Sel", type: "dark", colors: { "editor.background": "#1E1E1E", "list.activeSelectionBackground": "#04395E" } }));
  expect(t!.offers).toEqual({ selRow: "#04395E" });
  expect(t!.app?.selRow).toBeUndefined();
  const zed = JSON.parse(read("zed-one.json"));
  expect(appFromZed(zed.themes[0].style, "dark").offers.selRow).toMatch(/^#[0-9A-F]{6}$/);
});
