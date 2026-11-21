/** The highlighter setting (the highlighter re-ruling, 2026-11-21, test 11). */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { plainPort, type RenderPort } from "../render/port";
import { createHighlighter, HIGHLIGHTER_KEY } from "./highlighter.svelte";
import type { KeptStorage } from "./kept-value";
import TfThemesSection from "./settings/TfThemesSection.svelte";
import { createSyntaxThemes } from "./syntax-theme.svelte";
import { createTheme } from "./theme.svelte";

function memory(seed: Record<string, string> = {}): KeptStorage & { data: Map<string, string> } {
  const data = new Map(Object.entries(seed));
  return { data, getItem: (k) => data.get(k) ?? null, setItem: (k, v) => void data.set(k, v), removeItem: (k) => void data.delete(k) };
}

test("a stored TanStack is read at boot; an unknown value is Pierre", () => {
  expect(createHighlighter(() => memory({ [HIGHLIGHTER_KEY]: "tanstack" }), () => null).chosen).toBe("tanstack");
  expect(createHighlighter(() => memory({ [HIGHLIGHTER_KEY]: "arborium" }), () => null).chosen).toBe("pierre");
  expect(createHighlighter(() => memory(), () => null).chosen).toBe("pierre");
});

test("choosing writes the key, clears it for Pierre, and colours what is open again once", () => {
  const storage = memory();
  const rehighlight = vi.fn();
  const port: RenderPort = { ...plainPort, rehighlight };
  const h = createHighlighter(() => storage, () => port);
  h.choose("shiki");
  expect(storage.data.get(HIGHLIGHTER_KEY)).toBe("shiki");
  expect(rehighlight).toHaveBeenCalledTimes(1);
  h.choose("shiki");
  expect(rehighlight).toHaveBeenCalledTimes(1);
  h.choose("pierre");
  expect(storage.data.has(HIGHLIGHTER_KEY)).toBe(false);
  expect(rehighlight).toHaveBeenCalledTimes(2);
});

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

function draw(seed: Record<string, string> = {}) {
  const storage = memory(seed);
  const rehighlight = vi.fn();
  const warm = vi.fn();
  const port: RenderPort = { ...plainPort, rehighlight, warm };
  const highlighter = createHighlighter(() => storage, () => port);
  const themes = createSyntaxThemes(() => memory(), () => null);
  view = mount(TfThemesSection, { target, props: { themes, mode: createTheme(), highlighter, render: port, prefersDark: () => false } });
  flushSync();
  return { storage, highlighter, rehighlight, warm };
}

test("Code colours draws the three highlighters with their lines, the chosen one checked, and a press chooses", () => {
  const { storage, rehighlight } = draw();
  const group = target.querySelector('[role="radiogroup"][aria-label="Highlighter"]')!;
  const labels = [...group.querySelectorAll("label")];
  expect(labels.map((l) => l.querySelector("span")!.firstChild!.textContent)).toEqual(["Pierre Highlights", "TanStack Highlight", "Shiki"]);
  expect(labels[0]!.querySelector("small")!.textContent).toMatch(/^Fastest\./);
  expect(labels.map((l) => l.querySelector("input")!.checked)).toEqual([true, false, false]);
  const tanstack = labels[1]!.querySelector("input")!;
  tanstack.checked = true;
  tanstack.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  expect(storage.data.get(HIGHLIGHTER_KEY)).toBe("tanstack");
  expect(rehighlight).toHaveBeenCalledTimes(1);
});

test("an engine that could not be loaded says so, and Try again clears it and loads it again", () => {
  const { highlighter, warm, rehighlight } = draw({ [HIGHLIGHTER_KEY]: "shiki" });
  highlighter.heardFailed("shiki");
  flushSync();
  const status = target.querySelector(".tf-thm-engfail")!;
  expect(status.textContent).toContain("Shiki could not be loaded. Pierre Highlights is colouring instead.");
  status.querySelector("button")!.click();
  flushSync();
  expect(target.querySelector(".tf-thm-engfail")).toBeNull();
  expect(warm).toHaveBeenCalledWith("shiki");
  expect(rehighlight).toHaveBeenCalledTimes(1);
});
