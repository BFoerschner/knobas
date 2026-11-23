// What a stylesheet draws with `content:` is drawn in Inter (the ux30 end gate, Windows, 2026-11-23): a character none of
// the bundled Inter files holds makes Blink search the system's fonts the first time a window draws it, three round trips
// to the browser's font service. The list of every ticket drew ▾ (U+25BE) on each row's status, a sixth-speed 48 ms of
// its first open in WebView2; the ticket page drew it on its status and facts. Those are shapes now.
import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";

import { expect, test } from "vitest";

const SRC = join(process.cwd(), "src");

/**
 * Left for a pass of their own, as the card-drag note's sweep left them (`docs/decisions/2026-11-23-a-held-card-reads-
 * nothing-and-the-x-is-drawn.md`): a diff's ran mark and a done task's tick (✓), and a link out of a page (↗). Neither is
 * on the path of a page the gate times as over.
 */
const LEFT = new Set(["✓", "↗"]);

function walk(dir: string, found: string[] = []): string[] {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) walk(path, found);
    else if (/\.(css|svelte)$/.test(entry.name)) found.push(path);
  }
  return found;
}

/** The code points the bundled Inter faces cover, from their `unicode-range`s in `fonts.css`. */
function interRanges(): [number, number][] {
  const css = readFileSync(join(SRC, "lib/tf/fonts/fonts.css"), "utf8");
  const ranges: [number, number][] = [];
  for (const face of css.match(/@font-face\s*{[^}]*}/g) ?? []) {
    if (!/font-family:\s*"Inter"/.test(face)) continue;
    const range = /unicode-range:\s*([^;]+);/.exec(face);
    for (const part of range?.[1]?.split(",") ?? []) {
      const [from, to] = part.trim().replace(/^U\+/i, "").split("-");
      ranges.push([parseInt(from!, 16), parseInt(to ?? from!, 16)]);
    }
  }
  return ranges;
}

/** A CSS string's characters, its escapes (`\25BE`, `\"`) read. */
function chars(value: string): string[] {
  const read = value.replace(/\\([0-9a-fA-F]{1,6})\s?|\\(.)/g, (_, hex: string | undefined, other: string | undefined) =>
    hex !== undefined ? String.fromCodePoint(parseInt(hex, 16)) : other!,
  );
  return [...read];
}

test("the bundled Inter files are read: Latin is in them", () => {
  const ranges = interRanges();
  expect(ranges.some(([from, to]) => from <= 0x41 && 0x41 <= to)).toBe(true);
});

test("every character a stylesheet draws with `content:` is one a bundled Inter file holds", () => {
  const ranges = interRanges();
  const held = (c: string) => ranges.some(([from, to]) => from <= c.codePointAt(0)! && c.codePointAt(0)! <= to);
  const outside: string[] = [];
  for (const path of walk(SRC)) {
    const text = readFileSync(path, "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
    for (const match of text.matchAll(/content:\s*((?:"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|\s)+)/g)) {
      for (const literal of match[1]!.matchAll(/"((?:[^"\\]|\\.)*)"|'((?:[^'\\]|\\.)*)'/g))
        for (const c of chars(literal[1] ?? literal[2] ?? ""))
          if (!held(c) && !LEFT.has(c)) outside.push(`${relative(SRC, path)}: U+${c.codePointAt(0)!.toString(16).toUpperCase()} ${c}`);
    }
  }
  expect(outside).toEqual([]);
});
