/** *Run pipelines on diffs* (#2270): on by default, kept across a restart, rubbish in storage reads as on. */
import { expect, test } from "vitest";

import { createDiffPipelines, PIPELINES_KEY } from "./diff-pipelines.svelte";

function memoryStorage() {
  const kept = new Map<string, string>();
  return {
    kept,
    getItem: (key: string) => kept.get(key) ?? null,
    setItem: (key: string, value: string) => void kept.set(key, value),
    removeItem: (key: string) => void kept.delete(key),
  };
}

test("on until turned off, and off again after a restart", () => {
  const storage = memoryStorage();
  const first = createDiffPipelines(() => storage);
  expect(first.on).toBe(true);
  first.set(false);
  expect(first.on).toBe(false);
  expect(storage.kept.get(PIPELINES_KEY)).toBe("off");
  expect(createDiffPipelines(() => storage).on).toBe(false);
});

test("storage that throws or holds rubbish costs the memory, not the setting", () => {
  const storage = memoryStorage();
  storage.kept.set(PIPELINES_KEY, "maybe");
  expect(createDiffPipelines(() => storage).on).toBe(true);
  const broken = createDiffPipelines(() => {
    throw new Error("blocked");
  });
  broken.set(false);
  expect(broken.on).toBe(false);
});
