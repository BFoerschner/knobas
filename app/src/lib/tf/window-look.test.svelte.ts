/** The bare window is told the mode and each side's page colour, once a change settles (ADR-0055, 3.12). */
import { flushSync } from "svelte";
import { expect, test, vi } from "vitest";

import type { KeptStorage } from "./kept-value";
import { createTheme } from "./theme.svelte";
import { createThemes, defaultTimers } from "./themes.svelte";
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

test("the default timers are called as the window's own: a browser refuses setTimeout with another `this`", () => {
  const real = { set: globalThis.setTimeout, clear: globalThis.clearTimeout };
  // As Chrome and WebKit do: "Illegal invocation" unless `this` is the global or nothing.
  const strict = <F extends (...args: never[]) => unknown>(f: F) =>
    function (this: unknown, ...args: Parameters<F>) {
      if (this !== undefined && this !== globalThis) throw new TypeError("Illegal invocation");
      return f(...args);
    };
  globalThis.setTimeout = strict(real.set) as unknown as typeof setTimeout;
  globalThis.clearTimeout = strict(real.clear) as unknown as typeof clearTimeout;
  try {
    // Captured at load, so the global swapped here cannot reach them: they must be wrappers, not the globals.
    expect(defaultTimers.set).not.toBe(real.set);
    expect(defaultTimers.clear).not.toBe(real.clear);
    const themes = createThemes(memory, () => null);
    const stop = watchWindowLook(themes, createTheme(), vi.fn(async () => {}));
    flushSync();
    themes.chooseApp("dracula");
    flushSync();
    stop();
    const own = themes.add({ name: "Mine", kind: "dark", app: {}, syntax: {} })!;
    themes.update(own.id, (t) => ({ ...t, app: { sheet: "#101010" } }));
    themes.flush();
  } finally {
    globalThis.setTimeout = real.set;
    globalThis.clearTimeout = real.clear;
  }
});
