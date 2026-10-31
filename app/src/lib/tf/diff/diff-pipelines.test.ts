/**
 * *Run pipelines on diffs* (#2270): on by default, a `knobas.setting` row
 * like the rest of Settings › Renderers (review of spec #2234, Standards 6):
 * held from the shell's read, saved through the backend, which answers what
 * it stored; a refused save changes nothing.
 */
import { expect, test } from "vitest";

import { createDiffPipelines } from "./diff-pipelines.svelte";

/** A backend row: what was saved is what the next read answers, as after a restart. */
function storedRow(start: boolean | null = null) {
  let row = start;
  let refuse = false;
  return {
    port: {
      read: async () => row ?? true,
      save: async (on: boolean) => {
        if (refuse) throw new Error("the database is gone");
        row = on;
        return on;
      },
    },
    refuse: () => void (refuse = true),
  };
}

test("on until turned off, and off again after a restart", async () => {
  const backend = storedRow();
  const first = createDiffPipelines(backend.port);
  expect(first.on).toBe(true);
  await first.set(false);
  expect(first.on).toBe(false);

  const restarted = createDiffPipelines(backend.port);
  restarted.hold(await backend.port.read());
  expect(restarted.on).toBe(false);
});

test("a refused save rejects and leaves the setting as it was", async () => {
  const backend = storedRow(true);
  const setting = createDiffPipelines(backend.port);
  backend.refuse();
  await expect(setting.set(false)).rejects.toThrow("the database is gone");
  expect(setting.on).toBe(true);
});
