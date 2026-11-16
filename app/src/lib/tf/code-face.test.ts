/**
 * Code is drawn in code's face everywhere (2026-11-16, Björn: "use monospace fonts for all code blocks, anywhere in
 * pages/docs/code etc."), and that face and its ligatures are one setting (`code-face.svelte.ts`).
 *
 * Read off disk, every stylesheet and every component's `<style>` under `app/src/`, as the style gate does. It fails
 * when
 *
 * - (a) `--mono` does not start with the chosen family and fall back to the bundled JetBrains Mono, its stand-ins and
 *   the system's monospace, or `--mono-ligatures` is not `none` until set;
 * - (b) the base rule is missing: `pre`, `code`, `kbd`, `samp` and `tt` take `--mono` and the ligature switch at no
 *   specificity, so code in any rendered text (Markdown, a Confluence macro, Jira's `{code}`, `{noformat}` and
 *   `{{monospace}}`) is drawn in it with no rule of its own;
 * - (c) a rule sets the font of a code element (a selector whose subject is one of those elements, or a class the
 *   markup puts on one, or one of the code views' classes) to anything but `--mono`, outside `EXCEPTIONS`;
 * - (d) a rule that sets `--mono` does not set `font-variant-ligatures: var(--mono-ligatures)` after it (a `font`
 *   shorthand resets ligatures), or a rule turns ligatures off by hand, which the setting could not turn on;
 * - (e) a monospace family is named anywhere but `--mono`'s definition: a rule naming one would not follow the setting.
 */
import { readFileSync, readdirSync } from "node:fs";
import { join, relative } from "node:path";

import { expect, test } from "vitest";

/** `app/src/`, off `cwd`: Vitest's root is `app/`. */
const ROOT = join(process.cwd(), "src");
const TOKENS = "lib/tf/css/tokens.css";

/** The code views' own classes: each draws code whatever element carries it. */
const CODE_CLASSES = [
  "tf-code",
  "tf-diff",
  "tf-logv",
  "tf-log",
  "tf-editor",
  "tf-item-log",
  "tf-mono",
  "tf-pmono",
  "tf-sf-mono",
  "tf-fence",
  "tf-vw-source-query",
  "tf-stg-det",
];

/** Rules that draw a code element in another face on purpose: `<file> <selector>` and why. */
const EXCEPTIONS: Record<string, string> = {
  "lib/tf/css/write-queue.css .tf-wq-pre":
    "a pending write's old and new text: what a person wrote in a field, kept in a `pre` only for its line breaks",
};

const CODE_ELEMENT = /^(?:pre|code|kbd|samp|tt)(?![\w-])/;
const MONO_FAMILY = /\b(?:JetBrains Mono|Menlo|Consolas|Monaco|Courier(?: New)?|SF Mono|Fira (?:Code|Mono)|Cascadia|DejaVu Sans Mono|Liberation Mono|ui-monospace|monospace)\b/i;

function files(root: string, pattern: RegExp): string[] {
  const found: string[] = [];
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (pattern.test(entry.name)) found.push(relative(root, path));
    }
  };
  walk(root);
  return found.sort();
}

function stylesheet(file: string, text: string): string {
  if (file.endsWith(".css")) return text;
  return [...text.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)].map((m) => m[1]).join("\n");
}

/** Split on `sep` outside parentheses and brackets. */
function splitTop(text: string, sep: RegExp): string[] {
  const parts: string[] = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < text.length; i++) {
    const ch = text[i]!;
    if (ch === "(" || ch === "[") depth++;
    else if (ch === ")" || ch === "]") depth--;
    else if (depth === 0 && sep.test(ch)) {
      parts.push(text.slice(start, i));
      start = i + 1;
    }
  }
  parts.push(text.slice(start));
  return parts;
}

type Rule = { selectors: string[]; decls: [string, string][] };

/** Every style rule in `css`; at-rule groups are entered, `@font-face` and `@keyframes` skipped, comments dropped. */
function rules(css: string): Rule[] {
  const text = css.replace(/\/\*[\s\S]*?\*\//g, "");
  const found: Rule[] = [];
  let start = 0;
  for (let i = 0; i < text.length; i++) {
    const ch = text[i];
    if (ch === ";" || ch === "}") start = i + 1;
    else if (ch === "{") {
      const prelude = text.slice(start, i).trim();
      start = i + 1;
      if (prelude.startsWith("@")) {
        if (/^@(keyframes|font-face)/.test(prelude)) {
          i = /^@keyframes/.test(prelude) ? text.indexOf("}", text.indexOf("}", i) + 1) : text.indexOf("}", i);
          start = i + 1;
        }
        continue;
      }
      const end = text.indexOf("}", i);
      const decls = text
        .slice(i + 1, end)
        .split(";")
        .map((decl) => decl.split(":"))
        .filter((pair) => pair.length >= 2)
        .map(([prop, ...value]) => [prop!.trim().toLowerCase(), value.join(":").trim()] as [string, string]);
      const selectors = splitTop(prelude, /,/)
        .map((s) => s.trim().replace(/\s+/g, " "))
        .filter((s) => s !== "");
      found.push({ selectors, decls });
      i = end;
      start = end + 1;
    }
  }
  return found;
}

/** A selector's last compound, `:is()`/`:where()` lists opened: the subjects it can match. */
function subjects(selector: string): string[] {
  const parts = splitTop(selector.replace(/\s*([>+~])\s*/g, " "), /\s/).filter((part) => part !== "");
  const last = parts.at(-1) ?? "";
  const list = /^:(?:is|where)\((.*)\)$/.exec(last);
  return list ? splitTop(list[1]!, /,/).flatMap((inner) => subjects(inner.trim())) : [last];
}

/** The family a rule sets, from `font-family` or the `font` shorthand; `null` if it sets none. */
function family(decls: [string, string][]): string | null {
  let found: string | null = null;
  for (const [prop, value] of decls) {
    if (prop === "font-family") found = value;
    else if (prop === "font" && value !== "inherit" && !/^var\(--[\w-]+\)$/.test(value)) found = value;
  }
  return found;
}

/** The last index whose item passes `test`; -1 for none. */
function lastIndex<T>(items: T[], test: (item: T) => boolean): number {
  for (let i = items.length - 1; i >= 0; i--) if (test(items[i]!)) return i;
  return -1;
}

/** The classes the markup puts on a code element, from static `class="…"` attributes. */
function markupCodeClasses(markup: string): string[] {
  const found: string[] = [];
  for (const m of markup.matchAll(/<(?:pre|code|kbd|samp|tt)\b[^>]*?\sclass="([^"{}]*)"/g)) found.push(...m[1]!.split(/\s+/).filter(Boolean));
  return found;
}

const sources = files(ROOT, /\.(css|svelte)$/).map((file) => ({ file, text: readFileSync(join(ROOT, file), "utf8") }));
const sheets = sources.map(({ file, text }) => ({ file, rules: rules(stylesheet(file, text)) }));
const codeClasses = new Set([...CODE_CLASSES, ...sources.filter(({ file }) => file.endsWith(".svelte")).flatMap(({ text }) => markupCodeClasses(text))]);

test("(a) --mono is the chosen family, then the bundled face, its stand-ins and the system's monospace; ligatures are off until set", () => {
  const tokens = readFileSync(join(ROOT, TOKENS), "utf8");
  expect(tokens).toMatch(
    /--mono: var\(--mono-chosen, "JetBrains Mono"\), "JetBrains Mono", "JetBrains Mono over Menlo", "JetBrains Mono over Consolas", ui-monospace, monospace;/,
  );
  expect(tokens).toMatch(/--mono-ligatures: none;/);
  // Each stand-in is a local face sized to JetBrains Mono's advance, so nothing moves when the bundled face lands.
  for (const stand of ["Menlo", "Consolas"]) {
    const face = new RegExp(`@font-face \\{\\s*font-family: "JetBrains Mono over ${stand}"; src: local\\("${stand}"\\)[^}]*size-adjust: [\\d.]+%`);
    expect(tokens).toMatch(face);
  }
});

test("(b) pre, code, kbd, samp and tt take --mono and the ligature switch at no specificity", () => {
  const base = sheets
    .find(({ file }) => file === TOKENS)!
    .rules.find(({ selectors }) => selectors.length === 1 && selectors[0] === ":where(pre, code, kbd, samp, tt)");
  expect(base?.decls).toEqual([
    ["font-family", "var(--mono)"],
    ["font-variant-ligatures", "var(--mono-ligatures)"],
  ]);
});

test("(c) no rule draws a code element in a face other than --mono", () => {
  const wrong: string[] = [];
  const used = new Set<string>();
  for (const { file, rules: found } of sheets) {
    for (const { selectors, decls } of found) {
      const set = family(decls);
      if (set === null || set.includes("var(--mono)")) continue;
      for (const selector of selectors) {
        const code = subjects(selector).some(
          (subject) => CODE_ELEMENT.test(subject) || [...subject.matchAll(/\.([\w-]+)/g)].some((m) => codeClasses.has(m[1]!)),
        );
        if (!code) continue;
        const key = `${file} ${selector}`;
        if (key in EXCEPTIONS) used.add(key);
        else wrong.push(`${key}: ${set}`);
      }
    }
  }
  expect(wrong).toEqual([]);
  // An exception that matches nothing is stale.
  expect([...used].sort()).toEqual(Object.keys(EXCEPTIONS).sort());
});

test("(d) every rule that sets --mono sets the ligature switch after it, and none turns ligatures off by hand", () => {
  const bare: string[] = [];
  const byHand: string[] = [];
  let mono = 0;
  for (const { file, rules: found } of sheets) {
    for (const { selectors, decls } of found) {
      const lastFont = lastIndex(decls, ([prop, value]) => (prop === "font" || prop === "font-family") && value.includes("var(--mono)"));
      const lastSwitch = lastIndex(decls, ([prop, value]) => prop === "font-variant-ligatures" && value === "var(--mono-ligatures)");
      if (lastFont >= 0) mono++;
      if (lastFont >= 0 && lastSwitch < lastFont) bare.push(`${file} ${selectors.join(", ")}`);
      if (decls.some(([prop, value]) => prop === "font-variant-ligatures" && value !== "var(--mono-ligatures)")) byHand.push(`${file} ${selectors.join(", ")}`);
    }
  }
  expect(mono).toBeGreaterThan(60);
  expect(bare).toEqual([]);
  expect(byHand).toEqual([]);
});

test("(e) no stylesheet, component or module names a monospace family but --mono's definition", () => {
  const named: string[] = [];
  for (const file of files(ROOT, /\.(css|svelte|ts)$/)) {
    // Tests name families as data; the dev server's fake products draw their own pages.
    if (/\.test\.|\/dev\/|ipc\/generated\//.test(file)) continue;
    const text = readFileSync(join(ROOT, file), "utf8").replace(/\/\*[\s\S]*?\*\/|^\s*\/\/.*$|<!--[\s\S]*?-->/gm, "");
    const css = file.endsWith(".ts") ? text.replace(/^\s*\*.*$/gm, "") : stylesheet(file, text);
    for (const { selectors, decls } of rules(file.endsWith(".ts") ? "" : css)) {
      const set = family(decls);
      if (set !== null && MONO_FAMILY.test(set)) named.push(`${file} ${selectors.join(", ")}: ${set}`);
    }
    if (file.endsWith(".ts") && /font(?:-f|F)amily["']?\s*[:=]\s*["'`][^"'`]*(?:JetBrains|Menlo|Consolas|Courier|monospace)/.test(css)) named.push(file);
  }
  const custom = rules(readFileSync(join(ROOT, TOKENS), "utf8")).flatMap(({ decls }) => decls.filter(([prop, value]) => prop.startsWith("--") && prop !== "--mono" && MONO_FAMILY.test(value)));
  expect(named).toEqual([]);
  expect(custom).toEqual([]);
});
