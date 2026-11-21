/**
 * The sizes diagrams drew at, kept between sessions (#2510): what a size is kept by, the store a new session reads,
 * and the engine versions the keys carry.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { ENGINE_VERSIONS, diagramKey, forgetDiagramSizes, keepDiagramSize, keptDiagramSize, reloadDiagramSizes } from "./diagram-sizes";

/** This runner's jsdom has no storage (`test-no-storage.ts`): one of the test's own, as the webview's. */
const items = new Map<string, string>();
const storage = { getItem: (k: string) => items.get(k) ?? null, setItem: (k: string, v: string) => void items.set(k, v), removeItem: (k: string) => void items.delete(k) };

beforeEach(() => {
  vi.useFakeTimers();
  items.clear();
  Object.defineProperty(window, "localStorage", { value: storage, configurable: true, writable: true });
  forgetDiagramSizes();
});

afterEach(() => {
  forgetDiagramSizes();
  Object.defineProperty(window, "localStorage", { value: undefined, configurable: true, writable: true });
  vi.useRealTimers();
});

const FLOW = "flowchart LR\n  a --> b";

test("a size is kept by its engine and that engine's version, the text scale and the source", () => {
  const key = diagramKey("mermaid", FLOW, "1");
  expect(diagramKey("mermaid", FLOW, "1")).toBe(key);
  expect(key).toContain(`mermaid@${ENGINE_VERSIONS.mermaid}`);
  const others = [
    diagramKey("graphviz", FLOW, "1"),
    diagramKey("mermaid", FLOW, "1.15"),
    diagramKey("mermaid", "flowchart LR\n  a --> c", "1"),
    diagramKey("mermaid", `${FLOW} `, "1"),
  ];
  expect(new Set([key, ...others]).size).toBe(5);
  // The source is kept as a digest, never whole: a README's diagram can be kilobytes.
  expect(diagramKey("mermaid", "x".repeat(50_000), "1").length).toBeLessThan(80);
});

test("the engine versions the sizes are kept by are the installed packages'", () => {
  const installed = (name: string) =>
    (JSON.parse(readFileSync(join(process.cwd(), "node_modules", name, "package.json"), "utf8")) as { version: string }).version;
  expect(ENGINE_VERSIONS).toEqual({ mermaid: installed("mermaid"), d2: installed("@terrastruct/d2"), graphviz: installed("@viz-js/viz") });
});

test("a kept size is there at once, and a new session reads it from the store", () => {
  const key = diagramKey("mermaid", FLOW, "1");
  expect(keptDiagramSize(key)).toBeUndefined();
  keepDiagramSize(key, { w: 320, h: 86 });
  expect(keptDiagramSize(key)).toEqual({ w: 320, h: 86 });
  // Not written yet: a page that draws ten diagrams writes the store once.
  expect(storage.getItem("knobas.diagram-sizes")).toBeNull();
  vi.advanceTimersByTime(1000);
  expect(JSON.parse(storage.getItem("knobas.diagram-sizes")!)).toEqual([[key, 320, 86]]);
  reloadDiagramSizes();
  expect(keptDiagramSize(key)).toEqual({ w: 320, h: 86 });
});

test("a diagram drawn at another size replaces what was kept; the latest 400 are remembered", () => {
  const key = diagramKey("mermaid", FLOW, "1");
  keepDiagramSize(key, { w: 320, h: 86 });
  keepDiagramSize(key, { w: 400, h: 120 });
  expect(keptDiagramSize(key)).toEqual({ w: 400, h: 120 });
  for (let n = 0; n < 400; n += 1) keepDiagramSize(`k${n}`, { w: 10 + n, h: 10 });
  expect(keptDiagramSize(key)).toBeUndefined();
  expect(keptDiagramSize("k0")).toEqual({ w: 10, h: 10 });
  expect(keptDiagramSize("k399")).toEqual({ w: 409, h: 10 });
  // One drawn again at another size is the newest: the next to go is the one after it.
  keepDiagramSize("k0", { w: 11, h: 11 });
  keepDiagramSize("one more", { w: 1, h: 1 });
  expect([keptDiagramSize("k0"), keptDiagramSize("k1")]).toEqual([{ w: 11, h: 11 }, undefined]);
});

test("a store that cannot be read, or holds something else, holds no size", () => {
  storage.setItem("knobas.diagram-sizes", "{not json");
  reloadDiagramSizes();
  expect(keptDiagramSize("a")).toBeUndefined();
  storage.setItem("knobas.diagram-sizes", JSON.stringify([["a", 0, 40], ["b", 120, 40], "c", ["d", 5]]));
  reloadDiagramSizes();
  expect(keptDiagramSize("a")).toBeUndefined();
  expect(keptDiagramSize("b")).toEqual({ w: 120, h: 40 });
  expect(keptDiagramSize("d")).toBeUndefined();
});
