/** A theme's model: reading what was kept, export and import (ADR-0051). */
import { expect, test } from "vitest";

import { BUILTINS } from "./builtins";
import { colour, exportTheme, hasApp, hasCode, importThemes, readTheme, readThemes, REFUSED, ruleFor, uniqueName, type ImportedTheme, type Theme } from "./themes";

const imported = (text: string): ImportedTheme[] => {
  const result = importThemes(text);
  if ("refused" in result) throw new Error(result.refused);
  return result.themes;
};

test("export then import gives the same theme", () => {
  const theme = { id: "own-abcdefgh", name: "Mine", kind: "dark" as const, syntax: { kw: "#FF79C6", str: "#F1FA8C" }, diff: { added: "#123456" }, italicComments: false };
  const { fileName, text } = exportTheme(theme);
  expect(fileName).toBe("Mine.knobas-theme.json");
  expect(imported(text)).toEqual([{ name: "Mine", kind: "dark", syntax: theme.syntax, diff: theme.diff, italicComments: false, from: "Mine" }]);
});

/** A VS Code theme, thirty lines: the rules the reading has to choose between. */
const VSCODE = JSON.stringify({
  name: "Fixture Light",
  type: "light",
  colors: { "editor.background": "#FFFFFF" },
  tokenColors: [
    { scope: "comment", settings: { foreground: "#6A737D", fontStyle: "italic" } },
    { scope: ["keyword", "storage"], settings: { foreground: "#D73A49" } },
    { scope: "keyword.control", settings: { foreground: "#AA0000" } },
    { scope: "string", settings: { foreground: "#032F62" } },
    { scope: "string", settings: { foreground: "#0A3069" } },
    { scope: "constant.numeric, constant.language", settings: { foreground: "#005CC5" } },
    { scope: "entity.name.function", settings: { foreground: "#6F42C1" } },
    { scope: "meta.function entity.name.type", settings: { foreground: "#FF0000" } },
    { scope: "entity.name.type", settings: { foreground: "#E36209" } },
    { scope: "entity.name.tag", settings: { foreground: "#22863A" } },
    { scope: "variable.parameter", settings: { foreground: "#24292EFF" } },
    { scope: "punctuation", settings: { foreground: "not-a-colour" } },
  ],
});

test("a VS Code theme's roles: the longest selector wins, a later rule wins a tie, a bad colour is dropped", () => {
  const [theme] = imported(VSCODE);
  expect(theme!.kind).toBe("light");
  expect(theme!.name).toBe("Fixture Light");
  expect(theme!.syntax).toEqual({
    com: "#6A737D",
    kw: "#D73A49",
    // `string.regexp` is a string where no rule names it; `string` twice, the later rule wins.
    str: "#0A3069",
    esc: "#0A3069",
    num: "#005CC5",
    con: "#005CC5",
    fn: "#6F42C1",
    // Not the red of a selector on a parent scope.
    ty: "#E36209",
    tag: "#22863A",
    key: "#22863A",
    // `storage.type.annotation` falls to `storage`, `keyword.operator` to `keyword`.
    attr: "#D73A49",
    op: "#D73A49",
    var: "#24292E",
    link: "#0A3069",
  });
  expect(theme!.italicComments).toBeUndefined();
  // `keyword.control` is longer than `keyword`: a control keyword's rule.
  const rules = JSON.parse(VSCODE).tokenColors;
  expect(ruleFor(rules, "keyword.control.flow")).toBe("#AA0000");
  expect(ruleFor(rules, "keyword.other")).toBe("#D73A49");
});

test("a VS Code theme with no type takes its side from its name, then its background", () => {
  const dark = JSON.parse(VSCODE);
  delete dark.type;
  dark.name = "Fixture";
  dark.colors["editor.background"] = "#1E1E1E";
  expect(imported(JSON.stringify(dark))[0]!.kind).toBe("dark");
  dark.colors["editor.background"] = "#FAFAFA";
  expect(imported(JSON.stringify(dark))[0]!.kind).toBe("light");
});

test("a VS Code theme written with comments and trailing commas is read", () => {
  const text = '{\n  // a comment\n  "name": "C", "type": "dark",\n  "tokenColors": [ { "scope": "string", "settings": { "foreground": "#98C379" } }, ],\n}';
  expect(imported(text)[0]!.syntax.str).toBe("#98C379");
});

test("a Zed family of two imports two", () => {
  const zed = {
    name: "Pair",
    themes: [
      { name: "Pair Light", appearance: "light", style: { syntax: { keyword: { color: "#7a3eb1" }, "string.escape": { color: "#8a5600" }, comment: { color: "#5f6975", font_style: "normal" } } } },
      { name: "Pair Dark", appearance: "dark", style: { syntax: { keyword: { color: "#c79bf2ff" }, property: { color: "#e5b04d" } } } },
    ],
  };
  const themes = imported(JSON.stringify(zed));
  expect(themes.map((t) => [t.name, t.kind])).toEqual([
    ["Pair Light", "light"],
    ["Pair Dark", "dark"],
  ]);
  expect(themes[0]!.syntax).toEqual({ kw: "#7A3EB1", esc: "#8A5600", com: "#5F6975" });
  expect(themes[0]!.italicComments).toBe(false);
  expect(themes[1]!.syntax).toEqual({ kw: "#C79BF2", prop: "#E5B04D" });
});

test("a file with none of the three keys is refused with the sentence", () => {
  expect(importThemes('{"name": "x"}')).toEqual({ refused: REFUSED });
  expect(importThemes("not json")).toEqual({ refused: REFUSED });
  expect(importThemes("[]")).toEqual({ refused: REFUSED });
  expect("refused" in importThemes(" ".repeat(1024 * 1024 + 1))).toBe(true);
});

test("a kept theme with a bad colour keeps the theme and drops the colour; an unreadable one is left out", () => {
  const kept = JSON.stringify([
    { id: "own-abcdefgh", name: "A", kind: "light", syntax: { kw: "#ABC", str: "red", fn: "#12345678" } },
    { id: "nope", name: "B", kind: "light", syntax: {} },
    { id: "own-zzzzzzzz", name: "C", kind: "purple", syntax: {} },
  ]);
  expect(readThemes(kept)).toEqual([{ id: "own-abcdefgh", name: "A", kind: "light", syntax: { kw: "#AABBCC", fn: "#123456" } }]);
  expect(readThemes("{")).toEqual([]);
  expect(readThemes(null)).toEqual([]);
});

test("colours and names", () => {
  expect(colour("#abc")).toBe("#AABBCC");
  expect(colour("#A1B2C3")).toBe("#A1B2C3");
  expect(colour("#A1B2C3DD")).toBe("#A1B2C3");
  expect(colour("rgb(1,2,3)")).toBeNull();
  expect(uniqueName("One Dark", BUILTINS.map((t) => t.name))).toBe("One Dark 2");
  expect(uniqueName("One Dark", ["One Dark", "One Dark 2"])).toBe("One Dark 3");
  expect(uniqueName("Mine", ["One Dark"])).toBe("Mine");
});

test("seven themes come with knobas, the knobas pair first", () => {
  expect(BUILTINS.map((t) => t.name)).toEqual(["knobas Light", "knobas Dark", "GitHub Light", "GitHub Dark", "One Dark", "Dracula", "Catppuccin Mocha"]);
});

// ---------- the window's part (ADR-0055, the decisions' 3.1 and 3.5) ----------

test("export then import gives the same theme, window colours and all", () => {
  const theme: Theme = { id: "own-roundtr1", name: "Round", kind: "dark", app: { sheet: "#282A36", ok: "#50FA7B" }, syntax: { kw: "#FF79C6" }, italicComments: false };
  const back = importThemes(exportTheme(theme).text);
  expect("themes" in back && back.themes[0]).toEqual({ name: "Round", kind: "dark", app: { sheet: "#282A36", ok: "#50FA7B" }, syntax: { kw: "#FF79C6" }, from: "Round", italicComments: false });
});

test("a file with window colours and no code colours has one part; a version 1 file with no app imports as before", () => {
  const appOnly = importThemes(JSON.stringify({ knobasTheme: 1, name: "A", kind: "light", app: { sheet: "#FFFFFF" }, syntax: {} }));
  const t = "themes" in appOnly ? appOnly.themes[0]! : null;
  expect(t && hasApp(t)).toBe(true);
  expect(t && hasCode(t)).toBe(false);
  const old = importThemes(JSON.stringify({ knobasTheme: 1, name: "B", kind: "dark", syntax: { kw: "#FF79C6" } }));
  const o = "themes" in old ? old.themes[0]! : null;
  expect(o && hasApp(o)).toBe(false);
  expect(o?.syntax).toEqual({ kw: "#FF79C6" });
});

test("a kept theme with a bad colour in its window part loses that key, not the theme; a font, radius or density is never kept", () => {
  const kept = readTheme({ id: "own-badcolor", name: "Bad", kind: "light", app: { sheet: "#FFFFFF", ink: "black", nope: "#000000" }, syntax: {}, font: "Comic Sans", radius: 9, density: "loose" });
  expect(kept?.app).toEqual({ sheet: "#FFFFFF" });
  expect(Object.keys(kept!)).not.toContain("font");
  const file = importThemes(JSON.stringify({ knobasTheme: 1, name: "F", kind: "light", syntax: {}, font: "Comic Sans", radius: 9, density: "loose" }));
  const f = "themes" in file ? file.themes[0]! : null;
  expect(Object.keys(f!).sort()).toEqual(["from", "kind", "name", "syntax"]);
});
