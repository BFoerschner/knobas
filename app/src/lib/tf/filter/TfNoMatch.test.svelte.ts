import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { FilterState } from "./filter";
import TfNoMatch from "./TfNoMatch.svelte";

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

const state = (over: Partial<FilterState>): FilterState => ({ text: "", chips: {}, dropdowns: {}, scope: null, ...over }) as FilterState;
const clear = () => target.querySelector<HTMLButtonElement>("button");

test("says what matched nothing, and Clear the filter empties what narrows it and leaves the scope", () => {
  const filter = state({ text: "zzz", chips: { state: ["down"] }, dropdowns: { type: "TCP" }, scope: "job" });
  const onclear = vi.fn();
  view = mount(TfNoMatch, { target, props: { noun: "monitors", filter, onclear } });
  flushSync();
  expect(target.querySelector("p")?.textContent).toBe("No monitors match.");
  clear()!.click();
  flushSync();
  expect(filter.text).toBe("");
  expect(filter.chips).toEqual({});
  expect(filter.dropdowns).toEqual({});
  expect(filter.scope).toBe("job");
  expect(onclear).toHaveBeenCalledOnce();
});

test("offers no Clear when only the scope narrows, since there is nothing of the filter's to clear", () => {
  view = mount(TfNoMatch, { target, props: { noun: "runs", filter: state({ scope: "job" }) } });
  flushSync();
  expect(target.querySelector("p")?.textContent).toBe("No runs match.");
  expect(clear()).toBeNull();
});
