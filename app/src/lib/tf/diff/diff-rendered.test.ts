/** The band's *Rendered* (ux26): off until set, kept as `"1"` or `"0"`, storage that fails costs only the memory. */
import { expect, test } from "vitest";

import { createDiffRendered, RENDERED_KEY } from "./diff-rendered.svelte";
import { LAYOUT_KEY } from "./diff-layout.svelte";

function store(start: Record<string, string> = {}) {
  const kept = new Map(Object.entries(start));
  return { kept, storage: { getItem: (k: string) => kept.get(k) ?? null, setItem: (k: string, v: string) => void kept.set(k, v), removeItem: (k: string) => void kept.delete(k) } };
}

test("off until set, and kept across a restart under its own key", () => {
  const { kept, storage } = store();
  const first = createDiffRendered(() => storage);
  expect(first.on).toBe(false);
  first.set(true);
  expect(kept.get(RENDERED_KEY)).toBe("1");
  expect(kept.has(LAYOUT_KEY)).toBe(false);
  expect(createDiffRendered(() => storage).on).toBe(true);
  first.set(false);
  expect(kept.get(RENDERED_KEY)).toBe("0");
  expect(createDiffRendered(() => storage).on).toBe(false);
});

test("rubbish kept reads as off, and storage that throws keeps the toggle for this run", () => {
  expect(createDiffRendered(() => store({ [RENDERED_KEY]: "yes" }).storage).on).toBe(false);
  const throwing = { getItem: () => { throw new Error("blocked"); }, setItem: () => { throw new Error("blocked"); }, removeItem: () => {} };
  const choice = createDiffRendered(() => throwing);
  expect(choice.on).toBe(false);
  choice.set(true);
  expect(choice.on).toBe(true);
});
