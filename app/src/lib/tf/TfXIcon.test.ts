// The drawn ✕ (`TfXIcon.svelte`, 2026-11-23): Inter has no U+2715, and the first one a window draws makes Blink search
// the system's fonts for it (49 ms at CPU 4x, in the frame of the board's first drop, whose toast drew the first ✕).
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { expect, test } from "vitest";

const LIB = join(__dirname, "..");

function sources(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return sources(path);
    return name.endsWith(".svelte") ? [path] : [];
  });
}

test("no component draws U+2715 as text: each ✕ is TfXIcon", () => {
  const drawn = sources(LIB).filter((path) => {
    // What a component draws, without its comments.
    const markup = readFileSync(path, "utf8").replace(/<!--[\s\S]*?-->/g, "").replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|\s)\/\/.*$/gm, "");
    return markup.includes("✕");
  });
  expect(drawn.map((path) => relative(LIB, path))).toEqual([]);
});
