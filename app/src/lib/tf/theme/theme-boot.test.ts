/** The head's script: the window's side and theme before the first paint (ADR-0055, the decisions' 3.12). */
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "vitest";

const APP = process.cwd();
const SCRIPT = readFileSync(join(APP, "public/theme-boot.js"), "utf8");

function boot(kept: Record<string, string>, globals: Record<string, unknown> = {}) {
  const props = new Map<string, string>();
  const attrs = new Map<string, string>();
  const html = { style: { setProperty: (k: string, v: string) => void props.set(k, v) }, setAttribute: (k: string, v: string) => void attrs.set(k, v) };
  const window = { localStorage: { getItem: (k: string) => kept[k] ?? null }, ...globals };
  new Function("window", "document", SCRIPT)(window, { documentElement: html });
  return { props, attrs };
}

const CACHE = JSON.stringify({ v: 1, vars: { "--sheet-dark": "#282A36", "--hl-kw-dark": "#FF79C6", color: "red", "--x": "#000000" }, sel: "dark" });

test("a kept mode and cache give data-theme, the properties and data-sel, and nothing that is not a side's property", () => {
  const { props, attrs } = boot({ "knobas.theme.mode": "dark", "knobas.paint": CACHE });
  expect(attrs.get("data-theme")).toBe("dark");
  expect(Object.fromEntries(props)).toEqual({ "--sheet-dark": "#282A36", "--hl-kw-dark": "#FF79C6" });
  expect(attrs.get("data-sel")).toBe("dark");
});

test("a cache that is not JSON, or of another version, writes nothing and throws nothing", () => {
  for (const paint of ["{nope", JSON.stringify({ v: 2, vars: { "--sheet-dark": "#000000" } }), "null"]) {
    const { props, attrs } = boot({ "knobas.paint": paint });
    expect(props.size, paint).toBe(0);
    expect(attrs.size, paint).toBe(0);
  }
  expect(() => new Function("window", "document", SCRIPT)({}, {})).not.toThrow();
});

test("started with knobas's own themes, only the side is written", () => {
  const { props, attrs } = boot({ "knobas.theme.mode": "light", "knobas.paint": CACHE }, { __knobasDefaultTheme: true });
  expect(props.size).toBe(0);
  expect([...attrs.keys()]).toEqual(["data-theme"]);
});

test("both documents load the script in their head, a classic script before their module", () => {
  for (const name of ["index.html", "capture.html"]) {
    const html = readFileSync(join(APP, name), "utf8");
    const head = html.slice(0, html.indexOf("</head>"));
    const tag = /<script([^>]*)src="\/theme-boot\.js"([^>]*)><\/script>/.exec(head);
    expect(tag, name).not.toBeNull();
    expect(`${tag![1]}${tag![2]}`, name).not.toMatch(/type=|defer|async/);
    expect(html.indexOf("/theme-boot.js"), name).toBeLessThan(html.indexOf('type="module"'));
  }
});
