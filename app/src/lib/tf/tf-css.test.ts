/**
 * What the ticket-first stylesheet has to keep (#809), checked against the
 * design system file rather than against a copy of itself: the colour and type
 * tables are read out of `docs/specs/2026-10-15-ticket-first-design-system.md`,
 * so a token that drifts from the spec fails here.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { brotliDecompressSync } from "node:zlib";

import { expect, test } from "vitest";

const APP = process.cwd();
const css = readFileSync(join(APP, "src/lib/tf/tf.css"), "utf8");
const appCss = readFileSync(join(APP, "src/app.css"), "utf8");
const spec = readFileSync(join(APP, "../docs/specs/2026-10-15-ticket-first-design-system.md"), "utf8");

/** `css` with its comments removed, so prose about a rule cannot satisfy it. */
const code = css.replace(/\/\*[\s\S]*?\*\//g, "");

/** The body of the first rule whose selector is exactly `selector`. */
function block(source: string, selector: string): string {
  const at = source.indexOf(`${selector} {`);
  expect(at, `no rule for ${selector}`).toBeGreaterThan(-1);
  return source.slice(at, source.indexOf("}", at));
}

/** `--token` → [light, dark], from the spec's colour table. */
const COLOURS: [string, string, string][] = [...spec.matchAll(/^\| `(--[a-z0-9-]+)`(?: \/ `(--[a-z0-9-]+)`)? \| `(#[0-9A-F]{6})`(?: \/ `(#[0-9A-F]{6})`)? \| `(#[0-9A-F]{6})`(?: \/ `(#[0-9A-F]{6})`)? \|/gm)].flatMap(
  (row) => {
    const pairs: [string, string, string][] = [[row[1]!, row[3]!, row[5]!]];
    if (row[2]) pairs.push([row[2], row[4]!, row[6]!]);
    return pairs;
  },
);

test("the spec's colour table was read, so the next two tests check something", () => {
  expect(COLOURS.length).toBeGreaterThanOrEqual(23);
});

test("light tokens are the spec's, on the shell root", () => {
  const light = block(code, ".tf");
  const wrong = COLOURS.filter(([token, value]) => !light.includes(`${token}: ${value};`));
  expect(wrong).toEqual([]);
});

test("dark tokens are the spec's, by the OS unless forced light, and when forced dark", () => {
  const media = code.slice(code.indexOf("@media (prefers-color-scheme: dark)"));
  const byOs = block(media, '.tf:not([data-theme="light"])');
  const forced = block(code, '.tf[data-theme="dark"]');
  for (const dark of [byOs, forced]) {
    const wrong = COLOURS.filter(([token, , value]) => !dark.includes(`${token}: ${value};`));
    expect(wrong).toEqual([]);
  }
});

test("the two dark blocks are one set of tokens, so an edit cannot reach only one", () => {
  const media = code.slice(code.indexOf("@media (prefers-color-scheme: dark)"));
  const tokens = (body: string) => body.slice(body.indexOf("{") + 1).replace(/\s+/g, " ").trim();
  expect(tokens(block(media, '.tf:not([data-theme="light"])'))).toBe(tokens(block(code, '.tf[data-theme="dark"]')));
});

test("no token is set on :root, and the old stylesheet gained no rule for the new shell", () => {
  expect(code).not.toMatch(/:root/);
  expect(appCss).not.toMatch(/\.tf\b|tf-/);
});

test("the type scale is the spec's", () => {
  const scale = [...spec.matchAll(/^\| `(--t-[a-z0-9]+)` \| (\d+) px \|/gm)].map((row) => [row[1]!, row[2]!]);
  expect(scale.length).toBe(11);
  const root = block(code, ".tf");
  expect(scale.filter(([token, px]) => !root.includes(`${token}: ${px}px;`))).toEqual([]);
});

test("every class is tf- prefixed, so app.css's bare class rules cannot reach the new shell", () => {
  const rules = code.replace(/"[^"]*"/g, '""');
  const classes = [...rules.matchAll(/\.(-?[_a-zA-Z][\w-]*)/g)].map((match) => match[1]!);
  expect(classes.length).toBeGreaterThan(20);
  expect([...new Set(classes.filter((name) => name !== "tf" && !name.startsWith("tf-")))]).toEqual([]);
});

test("width comes from font-variation-settings only, never font-stretch", () => {
  expect(code).not.toMatch(/font-stretch/);
  expect(block(code, ".tf-jobkey")).toMatch(/font-variation-settings: "wdth" 62/);
});

test("focus is 2px --focus, and --card-ink on a card surface", () => {
  expect(block(code, ".tf :focus-visible")).toMatch(/outline: 2px solid var\(--focus\)/);
  expect(code).toMatch(/\.tf \.tf-card:focus-visible, \.tf \.tf-card :focus-visible \{ outline-color: var\(--card-ink\); \}/);
});

test("reduced motion stops the pulse and every other animation in the shell", () => {
  expect(block(code, ".tf-dot.tf-run")).toMatch(/animation: tf-pulse 1\.4s/);
  const reduced = code.slice(code.indexOf("@media (prefers-reduced-motion: reduce)"));
  expect(reduced).toMatch(/\.tf \*[^{]*\{ transition: none !important; animation: none !important; \}/);
});

test("the three families are vendored from npm, Archivo from its width-axis file", () => {
  expect(code).toMatch(/@import "@fontsource-variable\/archivo\/wdth\.css";/);
  expect(code).toMatch(/@import "@fontsource-variable\/source-serif-4\//);
  expect(code).toMatch(/@import "@fontsource\/jetbrains-mono\//);
  expect(code).not.toMatch(/https?:/);
});

/**
 * The axis is in the font file, not synthesised: the latin Archivo the
 * stylesheet imports has an `fvar` axis record `wdth` whose minimum is 62.0.
 *
 * WOFF2 keeps its table data in one Brotli stream after a variable-length
 * table directory; an `fvar` axis record is the tag followed by min, default
 * and max as 16.16 fixed. The default-only file (`index.css`) carries `wght`
 * alone and fails this.
 */
test("the vendored Archivo carries a wdth axis from 62", () => {
  const face = readFileSync(join(APP, "node_modules/@fontsource-variable/archivo/wdth.css"), "utf8");
  const latin = face.match(/archivo-latin-wdth-normal \*\/[\s\S]*?url\(\.\/(files\/[^)]+)\)/);
  expect(latin, "no latin face in wdth.css").not.toBeNull();
  const font = readFileSync(join(APP, "node_modules/@fontsource-variable/archivo", latin![1]!));
  expect(font.subarray(0, 4).toString("latin1")).toBe("wOF2");
  expect(axes(font)).toContainEqual({ tag: "wdth", min: 62 });
});

function axes(woff2: Buffer): { tag: string; min: number }[] {
  const tables = woff2.readUInt16BE(12);
  const compressed = woff2.readUInt32BE(20);
  let at = 48;
  const base128 = () => {
    let value = 0;
    for (;;) {
      const byte = woff2[at++]!;
      value = value * 128 + (byte & 0x7f);
      if ((byte & 0x80) === 0) return value;
    }
  };
  for (let index = 0; index < tables; index += 1) {
    const flags = woff2[at++]!;
    const known = flags & 0x3f;
    if (known === 63) at += 4;
    base128();
    const version = flags >> 6;
    // glyf (10) and loca (11) are transformed at version 0; every other table at a non-zero one.
    const transformed = known === 10 || known === 11 ? version === 0 : version !== 0;
    if (transformed) base128();
  }
  const data = brotliDecompressSync(woff2.subarray(at, at + compressed));
  const found: { tag: string; min: number }[] = [];
  for (let index = data.indexOf("wdth"); index !== -1; index = data.indexOf("wdth", index + 1)) {
    if (index + 16 <= data.length) found.push({ tag: "wdth", min: data.readInt32BE(index + 4) / 65536 });
  }
  return found;
}
