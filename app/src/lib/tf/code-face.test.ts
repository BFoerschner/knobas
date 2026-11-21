/**
 * Code is drawn in code's face everywhere (2026-11-16, Björn: "use monospace fonts for all code blocks, anywhere in
 * pages/docs/code etc."), and that face and its ligatures are one setting; since Settings › Fonts (2026-11-18) every
 * role is: Interface, Headings, Reading and Code each a family and a size (`fonts.svelte.ts`).
 *
 * Read off disk, every stylesheet and every component's `<style>` under `app/src/`, as the style gate does. It fails
 * when
 *
 * - (a) `--mono` does not start with the chosen family and fall back to the bundled face (Cascadia Mono, 2026-11-21, or
 *   Cascadia Code when the switch sets `--mono-bundled`), its stand-ins and the system's monospace, or
 *   `--mono-ligatures` is not `none` until turned on;
 * - (b) the base rule is missing: `pre`, `code`, `kbd`, `samp` and `tt` take `--mono` and the ligature switch at no
 *   specificity, so code in any rendered text (Markdown, a Confluence macro, Jira's `{code}`, `{noformat}` and
 *   `{{monospace}}`) is drawn in it with no rule of its own;
 * - (c) a rule sets the font of a code element (a selector whose subject is one of those elements, or a class the
 *   markup puts on one, or one of the code views' classes) to anything but `--mono`, outside `EXCEPTIONS`;
 * - (d) a rule that sets `--mono` does not set `font-variant-ligatures: var(--mono-ligatures)` after it (a `font`
 *   shorthand resets ligatures), or a rule turns ligatures off by hand, which the setting could not turn on;
 * - (e) a monospace family is named anywhere but `--mono`'s definition: a rule naming one would not follow the setting;
 * - (f) a role's family token does not start with the chosen family and fall back to the bundled face, or a size is not
 *   1 until set;
 * - (g) a rule that sets a role's family does not set `--fs` to that role's size, so its text would take another role's;
 * - (h) a font size is not multiplied by `--fs` (or, relative to the words around it, by code's size over theirs), so it
 *   would not follow its role's size;
 * - (i) a rule narrows with `"wdth"` and is not drawn in `--head` (the brand alone in `--brand`) with `font-stretch` at
 *   the same width, so a chosen Headings face would not narrow, or not be used;
 * - (j) a surface is not drawn in its role: Markdown's text in the interface's, its headings in Headings', Docs pages
 *   in Reading's;
 * - (k) a bundled proportional family is named anywhere but its token: a rule naming one would not follow the setting.
 */
import { readFileSync, readdirSync } from "node:fs";
import { join, relative } from "node:path";

import { expect, test } from "vitest";

/** `app/src/`, off `cwd`: Vitest's root is `app/`. */
const ROOT = join(process.cwd(), "src");
const TOKENS = "lib/tf/css/tokens.css";
/** The bundled faces' `@font-face` rules (2026-11-20). */
const FACES = "lib/tf/fonts/fonts.css";

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

test("(a) --mono is the chosen family, then the bundled face, its stand-ins and the system's monospace; ligatures are off until turned on", () => {
  const tokens = readFileSync(join(ROOT, TOKENS), "utf8");
  expect(tokens).toMatch(/--mono-bundled: "Cascadia Mono";/);
  expect(tokens).toMatch(
    /--mono: var\(--mono-chosen, var\(--mono-bundled\)\), var\(--mono-bundled\), "Cascadia over Menlo", "Cascadia over Consolas", ui-monospace, monospace;/,
  );
  expect(tokens).toMatch(/--mono-ligatures: none;/);
  // Both Cascadias at 105 %, and each stand-in a local face sized to its advance there, so nothing moves when one lands.
  const faces = readFileSync(join(ROOT, FACES), "utf8");
  for (const [name, folder, file] of [["Cascadia Mono", "cascadia-mono", "CascadiaMono"], ["Cascadia Code", "cascadia-code", "CascadiaCode"]] as const) {
    for (const [style, suffix] of [["normal", ""], ["italic", "Italic"]] as const) {
      expect(faces).toMatch(new RegExp(`@font-face \\{\\s*font-family: "${name}"; font-style: ${style};[^}]*src: url\\("\\./${folder}/${file}${suffix}\\.woff2"\\)[^}]*size-adjust: 105%;`));
    }
  }
  for (const stand of ["Menlo", "Consolas"]) {
    const face = new RegExp(`@font-face \\{\\s*font-family: "Cascadia over ${stand}"; src: local\\("${stand}"\\)[^}]*size-adjust: [\\d.]+%`);
    expect(faces).toMatch(face);
  }
});

test("(b) pre, code, kbd, samp and tt take --mono, code's size and the ligature switch at no specificity", () => {
  const tokens = sheets.find(({ file }) => file === TOKENS)!.rules;
  const base = tokens.find(({ selectors }) => selectors.length === 1 && selectors[0] === ":where(pre, code, kbd, samp, tt)");
  expect(base?.decls).toEqual([
    ["--fs", "var(--size-code)"],
    ["font-family", "var(--mono)"],
    ["font-variant-ligatures", "var(--mono-ligatures)"],
    // Code's size over the interface's: a bare `pre` in a sentence is as big as code's size makes it.
    ["font-size", "calc(1em * var(--size-code) / var(--size-ui))"],
  ]);
  // Code inside code takes the size once.
  const nested = tokens.find(({ selectors }) => selectors.length === 1 && selectors[0] === ":where(pre, code, kbd, samp, tt) :where(pre, code, kbd, samp, tt)");
  expect(nested?.decls).toEqual([["font-size", "inherit"]]);
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
  const custom = rules(readFileSync(join(ROOT, TOKENS), "utf8")).flatMap(({ decls }) => decls.filter(([prop, value]) => prop.startsWith("--") && prop !== "--mono" && prop !== "--mono-bundled" && MONO_FAMILY.test(value)));
  expect(named).toEqual([]);
  expect(custom).toEqual([]);
});

/** Each role's family token, its chosen family's property, its size and its bundled face. */
const ROLE_TOKENS = [
  { token: "--sans", role: "ui", chosen: "--ui-chosen", bundled: '"Inter"', rest: "system-ui, sans-serif" },
  { token: "--head", role: "head", chosen: "--head-chosen", bundled: '"Inter"', rest: "system-ui, sans-serif" },
  { token: "--serif", role: "read", chosen: "--read-chosen", bundled: '"Inter"', rest: "system-ui, sans-serif" },
] as const;
const ROLE_OF: Record<string, string> = { sans: "ui", head: "head", serif: "read", mono: "code" };

/** The role a family value draws in: the last of the role tokens it names; `null` for none. */
function roleOf(value: string): string | null {
  const named = [...value.matchAll(/var\(--(sans|head|serif|mono)\)/g)];
  return named.length === 0 ? null : ROLE_OF[named.at(-1)![1]!]!;
}

/** The size part of a `font` shorthand: what stands before its `/line-height` or family. */
function shorthandSize(value: string): string | null {
  const parts = splitTop(value.trim(), /\s/).filter((part) => part !== "");
  for (const part of parts) {
    const size = splitTop(part, /\//)[0]!;
    if (/^(?:calc|round|var)\(|^[\d.]+(?:px|em|rem|%)$/.test(size)) return size;
  }
  return null;
}

/** A font size that follows its role: times `--fs`, or em-relative times code's size over the role around it. */
const FOLLOWS = [
  /^calc\((?:var\(--t-[\w-]+(?:, [\d.]+px)?\)|[\d.]+px) \* var\(--fs\)\)$/,
  /^(?:round\()?calc\([\d.]+em \* var\(--size-code\) \/ var\(--size-(?:ui|read)\)\)(?:, 1px\))?$/,
];
/** Font sizes that follow nothing on purpose: `<file> <selector>` and why. */
const FIXED_SIZES: Record<string, string> = {
  "lib/tf/css/tokens.css :where(pre, code, kbd, samp, tt) :where(pre, code, kbd, samp, tt)": "code inside code: the outer one took code's size",
  "lib/tf/css/code.css .tf-pdf-text > span": "a PDF's text layer: sized by pdf.js to the page under it",
};

test("(f) each role's token is the chosen family, then the bundled face; every size is 1 until set", () => {
  const tokens = readFileSync(join(ROOT, TOKENS), "utf8");
  for (const { token, chosen, bundled, rest } of ROLE_TOKENS) {
    expect(tokens).toContain(`${token}: var(${chosen}, ${bundled}), ${bundled}, ${rest};`);
  }
  expect(tokens).toContain("--size-ui: 1; --size-head: 1; --size-read: 1; --size-code: 1;");
  expect(tokens).toContain("--fs: var(--size-ui);");
  // The form's label column holds one line of interface text, so it follows the interface's size.
  expect(tokens).toContain("--tf-label: calc(132px * var(--size-ui));");
  // Archivo's opened word space went with it (2026-11-20): Inter keeps its own.
  expect(tokens).not.toMatch(/--ui-letter|--ui-word/);
});

test("(g) every rule that sets a role's family sets --fs to that role's size", () => {
  const wrong: string[] = [];
  let roles = 0;
  for (const { file, rules: found } of sheets) {
    if (file === "unported.css") continue;
    for (const { selectors, decls } of found) {
      const set = family(decls) ?? decls.find(([prop]) => prop === "font-family")?.[1] ?? null;
      const role = set === null ? null : roleOf(set);
      if (role === null) continue;
      roles++;
      const fs = decls.filter(([prop]) => prop === "--fs").at(-1)?.[1];
      if (fs !== `var(--size-${role})`) wrong.push(`${file} ${selectors.join(", ")}: ${role}, --fs ${fs ?? "unset"}`);
    }
  }
  expect(roles).toBeGreaterThan(150);
  expect(wrong).toEqual([]);
});

test("(h) every font size follows its role's size", () => {
  const wrong: string[] = [];
  const used = new Set<string>();
  let sizes = 0;
  for (const { file, rules: found } of sheets) {
    if (file === "unported.css") continue;
    for (const { selectors, decls } of found) {
      for (const [prop, value] of decls) {
        const size = prop === "font-size" ? value : prop === "font" && value !== "inherit" && !/^var\(--[\w-]+\)$/.test(value) ? shorthandSize(value) : null;
        if (size === null) continue;
        sizes++;
        if (FOLLOWS.some((form) => form.test(size))) continue;
        const key = `${file} ${selectors.join(", ")}`;
        if (key in FIXED_SIZES) used.add(key);
        else wrong.push(`${key}: ${size}`);
      }
    }
  }
  expect(sizes).toBeGreaterThan(650);
  expect(wrong).toEqual([]);
  expect([...used].sort()).toEqual(Object.keys(FIXED_SIZES).sort());
});

test("(i) every rule that narrows is drawn in --head at Headings' size, with font-stretch at the same width", () => {
  const wrong: string[] = [];
  let narrowed = 0;
  for (const { file, rules: found } of sheets) {
    if (file === "unported.css") continue;
    for (const { selectors, decls } of found) {
      const wdth = decls.find(([prop, value]) => prop === "font-variation-settings" && /"wdth"/.test(value));
      if (!wdth) continue;
      narrowed++;
      const n = /"wdth"\s*([\d.]+)/.exec(wdth[1])![1];
      const set = family(decls) ?? decls.find(([prop]) => prop === "font-family")?.[1] ?? "";
      const stretch = decls.find(([prop]) => prop === "font-stretch")?.[1];
      const brand = set === "var(--brand)";
      const fs = decls.find(([prop]) => prop === "--fs")?.[1];
      if (stretch !== `${n}%` || !(brand ? fs === "1" : set.includes("var(--head)") && fs === "var(--size-head)"))
        wrong.push(`${file} ${selectors.join(", ")}: ${set || "no family"}, stretch ${stretch ?? "none"}, --fs ${fs ?? "unset"}`);
    }
  }
  expect(narrowed).toBeGreaterThan(30);
  expect(wrong).toEqual([]);
});

test("(j) Markdown's text is the interface's, its headings Headings', and a Docs page Reading's", () => {
  const roleAt = (file: string, selector: string): string | null => {
    // The last rule for the selector that sets a family: `html, body` sets none before `body` does.
    const rule = sheets
      .find((sheet) => sheet.file === file)!
      .rules.filter(({ selectors, decls }) => selectors.includes(selector) && decls.some(([prop]) => prop === "font" || prop === "font-family"))
      .at(-1);
    if (!rule) return `no rule ${selector}`;
    const set = family(rule.decls) ?? rule.decls.find(([prop]) => prop === "font-family")?.[1] ?? null;
    return set === null ? null : roleOf(set);
  };
  const want: [string, string, string][] = [
    ["lib/tf/css/markdown.css", ".tf-md", "ui"],
    ...["h1", "h2", "h3", "h4"].map((h): [string, string, string] => ["lib/tf/css/markdown.css", `.tf-md ${h}`, "head"]),
    ["lib/tf/css/markdown.css", ".tf-md code", "code"],
    ["lib/tf/css/markdown.css", ".tf-md .tf-fence pre", "code"],
    ["lib/tf/css/docs.css", ".tf-doc", "read"],
    ["lib/tf/css/docs.css", ".tf-doc h2", "read"],
    ["lib/tf/css/docs.css", ".tf-doc code", "code"],
    ["lib/tf/css/chrome.css", ".tf-jobkey", "head"],
    ["lib/tf/css/settings.css", ".tf-stg-h h1", "head"],
    ["lib/tf/css/tokens.css", ".tf .tf-mono", "code"],
  ];
  for (const [file, selector, role] of want) expect(roleAt(file, selector), `${file} ${selector}`).toBe(role);
  // The interface's own root and the document's body draw in it.
  expect(roleAt("lib/tf/css/tokens.css", "body")).toBe("ui");
});

test("(k) no stylesheet or component names a bundled proportional family but its token", () => {
  const named: string[] = [];
  for (const { file, rules: found } of sheets) {
    if (file === TOKENS) continue;
    for (const { selectors, decls } of found) {
      const set = family(decls);
      if (set !== null && /Inter\b|Archivo|Source Serif/.test(set)) named.push(`${file} ${selectors.join(", ")}: ${set}`);
    }
  }
  expect(named).toEqual([]);
});

test("(l) a column of one line of text follows its role's size, and the job key never pushes its card wider", () => {
  // The first rule for the selector: the ones after it are narrow windows' (a media or container query).
  const decl = (file: string, selector: string, prop: string) =>
    sheets
      .find((sheet) => sheet.file === file)!
      .rules.filter(({ selectors }) => selectors.includes(selector))
      .flatMap(({ decls }) => decls.filter(([p]) => p === prop).map(([, value]) => value))
      .at(0);
  const want: [string, string, string, RegExp][] = [
    ["lib/tf/css/code.css", ".tf-code .tf-l", "grid-template-columns", /^calc\(48px \* var\(--size-code\)\) 1fr$/],
    ["lib/tf/css/code.css", ".tf-diff .tf-ln", "grid-template-columns", /^calc\(46px \* var\(--size-code\)\) calc\(46px \* var\(--size-code\)\)/],
    ["lib/tf/css/chains.css", ".tf-ll", "grid-template-columns", /^calc\(52px \* var\(--size-code\)\) calc\(70px \* var\(--size-code\)\)/],
    ["lib/tf/css/tickets.css", ".tf-list .tf-c-key", "width", /^calc\(116px \* var\(--size-code\)\)$/],
    ["lib/tf/css/tickets.css", ".tf-list .tf-c-status", "width", /^calc\(92px \* var\(--size-ui\)\)$/],
    ["lib/tf/css/inbox.css", ".tf-ib", "grid-template-columns", /^calc\(96px \* var\(--size-ui\)\)/],
    ["lib/tf/css/time.css", ".tf-tm-row", "grid-template-columns", /^calc\(150px \* var\(--size-ui\)\)/],
    ["lib/tf/css/job.css", ".tf-ticketcard .tf-facts", "grid-template-columns", /^calc\(68px \* var\(--size-ui\)\)/],
    ["lib/tf/css/job.css", ".tf-jobcard .tf-jobkey", "text-overflow", /^ellipsis$/],
  ];
  for (const [file, selector, prop, value] of want) expect(decl(file, selector, prop), `${file} ${selector} ${prop}`).toMatch(value);
});

test("(m) a syntax role changes a token's colour, slant or weight and nothing else, so colours land without moving a line (ADR-0051)", () => {
  // The bundled Cascadia face keeps one advance in its italic and bold, so a comment's slant and a heading's weight
  // never widen a line; a size, a family, spacing or a box would.
  const allowed = new Set(["--hl", "color", "font-style", "font-weight"]);
  const found: string[] = [];
  for (const { file, rules: all } of sheets)
    for (const { selectors, decls } of all)
      if (selectors.some((selector) => subjects(selector).some((subject) => /(?:^|[.\]])tf-hl-|\[class\*="tf-hl-"\]/.test(subject))))
        for (const [prop] of decls) if (!allowed.has(prop)) found.push(`${file} ${selectors.join(", ")}: ${prop}`);
  expect(found).toEqual([]);
});
