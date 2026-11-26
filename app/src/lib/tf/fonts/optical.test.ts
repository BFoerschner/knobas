// Optical sizes for headings only (the ux30 Windows pass, 2026-11-23, `fonts.css`): every size drawn with optical sizing
// on is a font instance of its own, which WebView2 on Windows shapes from cold on a page's first open. So it is off
// everywhere, a `font` shorthand included, and on wherever the heading face is set, for the heading and its words.
import { readFileSync, readdirSync } from "node:fs";
import { join, relative } from "node:path";

import { expect, test } from "vitest";

const SRC = join(process.cwd(), "src");

function walk(dir: string, found: string[] = []): string[] {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) walk(path, found);
    else if (/\.(css|svelte)$/.test(entry.name)) found.push(path);
  }
  return found;
}

/** Each declaration block (`{ … }` with no block inside) that sets the heading face. */
function headingBlocks(source: string): string[] {
  return [...source.matchAll(/\{([^{}]*)\}/g)].map((m) => m[1]!).filter((block) => /(^|[;\s])font(-family)?:[^;]*var\(--head\)/.test(block));
}

test("optical sizing is off everywhere, a font shorthand included, unless a rule turns it on", () => {
  const fonts = readFileSync(join(SRC, "lib", "tf", "fonts", "fonts.css"), "utf8");
  expect(fonts).toMatch(/\*,\s*\*::before,\s*\*::after\s*\{\s*font-optical-sizing:\s*var\(--optical,\s*none\)\s*!important;\s*\}/);
});

test("every rule that sets the heading face turns optical sizing on for it and its words", () => {
  const files = walk(SRC);
  const blocks = files.flatMap((file) => headingBlocks(readFileSync(file, "utf8")).map((block) => ({ file: relative(SRC, file), block })));
  expect(blocks.length, "the heading face is set somewhere").toBeGreaterThan(20);
  const off = blocks.filter(({ block }) => !/--optical:\s*auto/.test(block)).map(({ file, block }) => `${file}: ${block.trim().slice(0, 80)}`);
  expect(off).toEqual([]);
});
