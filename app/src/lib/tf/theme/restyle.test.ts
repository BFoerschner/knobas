/**
 * A theme switch restyles the window and lays nothing out (ADR-0055, `testenv/perf/theme-switch.mjs`). Chromium lays a
 * table with collapsed borders out again whenever a border's colour changes, so no stylesheet collapses one: the
 * Markdown and Docs tables and the view editor's list draw the same lines with separate borders or outlines
 * (2026-11-23).
 */
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "vitest";

const APP = process.cwd();

function files(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    const path = join(dir, e.name);
    if (e.isDirectory()) return e.name === "node_modules" ? [] : files(path);
    return /\.(css|svelte)$/.test(e.name) ? [path] : [];
  });
}

test("no stylesheet collapses a table's borders", () => {
  const collapsing = files(join(APP, "src")).filter((path) => /border-collapse\s*:\s*collapse/.test(readFileSync(path, "utf8").replace(/\/\*[\s\S]*?\*\//g, "")));
  expect(collapsing.map((path) => path.slice(APP.length + 1))).toEqual([]);
});
