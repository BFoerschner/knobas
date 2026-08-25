import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import Toast from "./Toast.svelte";
import { push, toasts } from "./toasts.svelte";

let target: HTMLDivElement;
let app: ReturnType<typeof mount>;

beforeEach(() => {
  vi.useFakeTimers();
  toasts.items = [];
  target = document.createElement("div");
  document.body.append(target);
  app = mount(Toast, { target });
  flushSync();
});

afterEach(() => {
  unmount(app);
  target.remove();
  vi.useRealTimers();
});

/**
 * A toast routinely carries an `IpcError.message` — a string a source system
 * produced. Interpolating it is the whole defence (gotcha 7), so what lands in
 * the DOM has to be a text node and not markup.
 */
test("renders its text as text, never as markup", () => {
  push({ text: '<img src=x onerror="boom()"> failed' });
  flushSync();

  const toast = target.querySelector(".toast");
  expect(toast?.textContent).toContain('<img src=x onerror="boom()"> failed');
  expect(toast?.querySelector("img")).toBeNull();
});

test("an action runs and takes the toast with it", () => {
  const run = vi.fn();
  push({ text: "Sync failed", tone: "err", action: { label: "Retry", run } });
  flushSync();

  expect(target.querySelector(".toast")?.classList.contains("err")).toBe(true);

  const action = [...target.querySelectorAll("button")].find((b) => b.textContent === "Retry");
  action?.click();
  flushSync();

  expect(run).toHaveBeenCalledOnce();
  expect(target.querySelector(".toast")).toBeNull();
});

test("the close control dismisses without running anything", () => {
  const run = vi.fn();
  push({ text: "Sync failed", action: { label: "Retry", run } });
  flushSync();

  target.querySelector<HTMLButtonElement>(".toast .x")?.click();
  flushSync();

  expect(run).not.toHaveBeenCalled();
  expect(target.querySelector(".toast")).toBeNull();
});

test("the stack shows every live toast, oldest first", () => {
  push({ text: "first" });
  push({ text: "second" });
  flushSync();

  expect([...target.querySelectorAll(".toast span")].map((s) => s.textContent)).toEqual([
    "first",
    "second",
  ]);
});
