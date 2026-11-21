/** The bare window is told the mode and each side's page colour, once a change settles (ADR-0055, 3.12). */
import { flushSync } from "svelte";
import { expect, test, vi } from "vitest";

import type { KeptStorage } from "./kept-value";
import { createTheme } from "./theme.svelte";
import { createThemes } from "./themes.svelte";
import { TELL_AFTER, watchWindowLook } from "./window-look.svelte";

const memory = (): KeptStorage => {
  const data = new Map<string, string>();
  return { getItem: (k) => data.get(k) ?? null, setItem: (k, v) => void data.set(k, v), removeItem: (k) => void data.delete(k) };
};

test("a slot change and a mode change each tell the window once, after they settle; nothing unchanged is told again", () => {
  vi.useFakeTimers();
  try {
    const themes = createThemes(memory, () => null);
    const mode = createTheme();
    const tell = vi.fn(async () => {});
    const stop = watchWindowLook(themes, mode, tell);
    flushSync();
    vi.advanceTimersByTime(TELL_AFTER);
    expect(tell).toHaveBeenLastCalledWith("system", "#E4E8EC", "#12171C");
    themes.chooseApp("dracula");
    mode.set("dark");
    flushSync();
    vi.advanceTimersByTime(TELL_AFTER);
    expect(tell).toHaveBeenCalledTimes(2);
    expect(tell).toHaveBeenLastCalledWith("dark", "#E4E8EC", "#21222C");
    themes.choose("one-dark");
    flushSync();
    vi.advanceTimersByTime(TELL_AFTER);
    expect(tell).toHaveBeenCalledTimes(2);
    stop();
  } finally {
    vi.useRealTimers();
  }
});

test("a run started with knobas's own themes tells the window nothing", () => {
  const themes = createThemes(memory, () => null, undefined, { defaultTheme: true });
  const tell = vi.fn(async () => {});
  watchWindowLook(themes, createTheme(), tell);
  flushSync();
  expect(tell).not.toHaveBeenCalled();
});
