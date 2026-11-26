/**
 * A colour picker's drag works the theme's colours out once a pick (the ux30 end gate, 2026-11-24): the store does it
 * to tell whether the theme is still readable, and the editor's preview takes those (`themes.coloursOf`), kept per theme
 * object, instead of working them out again.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

const counted = vi.hoisted(() => ({ derives: 0 }));
vi.mock("../theme/app-derive", async (original) => {
  const real = await original<typeof import("../theme/app-derive")>();
  return { ...real, derive: (...args: Parameters<typeof real.derive>) => ((counted.derives += 1), real.derive(...args)) };
});

import type { KeptStorage } from "../kept-value";
import { createTheme } from "../theme.svelte";
import { APP_SLOT_KEY, createThemes, THEMES_KEY, type ThemeRoot } from "../themes.svelte";
import TfThemeSection from "./TfThemeSection.svelte";

const MINE = { id: "own-mine0001", name: "Mine", kind: "light", from: "knobas Light", app: { sheet: "#FFFFFF", ink: "#202020" }, syntax: {} };

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.body.appendChild(document.createElement("div"));
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

test("each pick of a drag works the colours out once", async () => {
  const data = new Map(Object.entries({ [APP_SLOT_KEY.light]: MINE.id, [THEMES_KEY]: JSON.stringify([MINE]) }));
  const storage: KeptStorage = { getItem: (k) => data.get(k) ?? null, setItem: (k, v) => void data.set(k, v), removeItem: (k) => void data.delete(k) };
  const root: ThemeRoot = { style: { setProperty: () => {}, removeProperty: () => "" }, setAttribute: () => {}, removeAttribute: () => {} };
  const themes = createThemes(() => storage, () => root, { set: setTimeout, clear: clearTimeout, frame: (run) => run() });
  view = mount(TfThemeSection, { target, props: { themes, mode: createTheme(), prefersDark: () => false } });
  flushSync();
  target.querySelector<HTMLButtonElement>('[data-row="paper"] button.tf-thm-sw')!.click();
  flushSync();
  const input = target.querySelector<HTMLInputElement>('.tf-thm-pop input[type="color"]')!;
  const frame = () => new Promise((done) => requestAnimationFrame(() => done(true)));
  // The first pick starts the hold; count the ones after it.
  input.value = "#d0e0f0";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  await frame();
  flushSync();
  for (const value of ["#c0d0e0", "#b0c0d0", "#a0b0c0"]) {
    counted.derives = 0;
    input.value = value;
    input.dispatchEvent(new Event("input", { bubbles: true }));
    await frame();
    flushSync();
    expect(counted.derives).toBe(1);
  }
});
