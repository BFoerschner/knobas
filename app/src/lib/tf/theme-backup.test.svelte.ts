/** Themes in backups (Björn, 2026-11-22): the store mirrored into the database, and a restore putting it back. */
import { flushSync } from "svelte";
import { expect, test, vi } from "vitest";

import type { KeptStorage } from "./kept-value";
import { KEEP_AFTER, mirrorThemes, restoreThemes } from "./theme-backup.svelte";
import { APP_SLOT_KEY, createThemes, SLOT_KEY, THEMES_KEY } from "./themes.svelte";

function memory(): KeptStorage & { data: Map<string, string> } {
  const data = new Map<string, string>();
  return { data, getItem: (k) => data.get(k) ?? null, setItem: (k, v) => void data.set(k, v), removeItem: (k) => void data.delete(k) };
}

test("a change is kept in the database a second after it settles, and the same snapshot is not written twice", async () => {
  vi.useFakeTimers();
  try {
    const themes = createThemes(memory, () => null);
    const keep = vi.fn(async (_text: string | null) => {});
    const stop = mirrorThemes(themes, keep);
    flushSync();
    vi.advanceTimersByTime(KEEP_AFTER);
    await Promise.resolve();
    expect(keep).toHaveBeenCalledTimes(1);
    themes.chooseApp("dracula");
    themes.choose("one-dark");
    flushSync();
    vi.advanceTimersByTime(KEEP_AFTER);
    await Promise.resolve();
    expect(keep).toHaveBeenCalledTimes(2);
    const kept = JSON.parse(keep.mock.calls[1]![0]!);
    expect([kept.app.dark, kept.code.dark]).toEqual(["dracula", "one-dark"]);
    stop();
  } finally {
    vi.useRealTimers();
  }
});

test("a restore puts back what the backup kept: a person's own themes and the slots", async () => {
  const from = createThemes(memory, () => null);
  const mine = from.add({ name: "Mine", kind: "dark", app: { sheet: "#202020" }, syntax: { kw: "#FF79C6" } })!;
  from.chooseApp(mine.id);
  from.choose("one-dark");
  const text = JSON.stringify(from.snapshot());
  const storage = memory();
  const to = createThemes(() => storage, () => null);
  expect(await restoreThemes(to, async () => text)).toBe(true);
  expect(to.own.map((t) => t.name)).toEqual(["Mine"]);
  expect(to.appDrawn("dark").id).toBe(mine.id);
  expect(to.drawn("dark").id).toBe("one-dark");
  expect(storage.data.get(APP_SLOT_KEY.dark)).toBe(mine.id);
  expect(storage.data.get(SLOT_KEY.dark)).toBe("one-dark");
  expect(JSON.parse(storage.data.get(THEMES_KEY)!)[0].app).toEqual({ sheet: "#202020" });
  // Nothing kept, or something that is not a snapshot: nothing changes.
  expect(await restoreThemes(to, async () => null)).toBe(false);
  expect(await restoreThemes(to, async () => "[1, 2]")).toBe(false);
  expect(to.own).toHaveLength(1);
});
