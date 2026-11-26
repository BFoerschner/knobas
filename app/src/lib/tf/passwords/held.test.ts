import { afterEach, expect, test, vi } from "vitest";

import type { Held } from "../../ipc/passwords";
import { forgetHeld, readHeld } from "./held.svelte";

const read = (title: string): Held => ({ sources: [], folders: [], entries: [["pw:1", 0, null, title, null, null, null, 0, null, []]], recent: [] });

afterEach(() => forgetHeld());

test("the first read makes no text of what it read: it is on the first open's path, and nothing is there to compare", async () => {
  const stringify = vi.spyOn(JSON, "stringify");
  try {
    const first = read("one");
    expect(await readHeld(async () => first)).toBe(first);
    expect(stringify).not.toHaveBeenCalled();
  } finally {
    stringify.mockRestore();
  }
});

test("a later read that changed nothing answers the kept read; one that changed answers the new one", async () => {
  const first = read("one");
  await readHeld(async () => first);
  expect(await readHeld(async () => read("one"))).toBe(first);
  const changed = read("two");
  expect(await readHeld(async () => changed)).toBe(changed);
  expect(await readHeld(async () => ({ ...read("two"), recent: ["pw:1"] }))).not.toBe(changed);
});
