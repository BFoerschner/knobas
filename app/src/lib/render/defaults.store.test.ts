/**
 * The app's copy of the renderer defaults (#2263) is a saved list like the
 * pipelines' (review of spec #2234, Standards 7): it holds what the backend
 * stored, answers it from a save, and keeps its list through a refusal.
 */
import { expect, test } from "vitest";

import type { RendererDefault } from "../ipc/app";
import { createRendererDefaults } from "./defaults.svelte";

const PUML: RendererDefault = { pattern: "*.puml", renderer: "command:plantuml", pipeline: null };

test("a save holds and answers what the backend stored; a refusal changes nothing", async () => {
  let refuse = false;
  const defaults = createRendererDefaults({
    read: async () => [PUML],
    save: async (list) => {
      if (refuse) throw new Error("no");
      return list.map((row) => ({ ...row, pattern: row.pattern.trim() }));
    },
  });
  await defaults.load();
  expect(defaults.list).toEqual([PUML]);

  const stored = await defaults.save([{ pattern: " *.md ", renderer: "markdown", pipeline: null }]);
  expect(stored).toEqual([{ pattern: "*.md", renderer: "markdown", pipeline: null }]);
  expect(defaults.list).toEqual(stored);

  refuse = true;
  await expect(defaults.save([])).rejects.toThrow("no");
  expect(defaults.list).toEqual(stored);
});
