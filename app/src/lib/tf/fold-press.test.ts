/**
 * The shared rule for a tree row that opens something and holds more (`fold-press.ts`, 2026-11-21), on a bare
 * `<details><summary><a>`: a first click opens, a click on the open place toggles, the chevron folds alone, and `Enter`
 * and modified clicks keep the link's own way. The Docs tree's tests drive it through the page.
 */
import { afterEach, beforeEach, expect, test } from "vitest";

import { foldKeyed, foldPress } from "./fold-press";

let fold: HTMLDetailsElement;
let summary: HTMLElement;
let link: HTMLAnchorElement;
let stored: boolean[];
let links: MouseEvent[];

beforeEach(() => {
  fold = document.createElement("details");
  summary = document.createElement("summary");
  link = document.createElement("a");
  link.href = "#/page/x";
  link.textContent = "Runbooks";
  summary.append(link);
  fold.append(summary, document.createElement("div"));
  document.body.append(fold);
  // The title starts 22 px in, past the chevron's column.
  link.getBoundingClientRect = () => ({ left: 22, right: 200, top: 0, bottom: 20, width: 178, height: 20, x: 22, y: 0, toJSON: () => ({}) });
  stored = [];
  links = [];
  link.addEventListener("click", (e) => links.push(e));
});

afterEach(() => fold.remove());

/** A click on `el` as the summary's handler sees it, `here` saying whether the link's place is open now. */
function press(el: Element, here: boolean, more: MouseEventInit = {}): MouseEvent {
  const event = new MouseEvent("click", { bubbles: true, cancelable: true, detail: 1, button: 0, clientX: 100, ...more });
  const handler = (e: Event) => foldPress(e as MouseEvent & { currentTarget: HTMLElement }, here, (open) => stored.push(open));
  summary.addEventListener("click", handler);
  el.dispatchEvent(event);
  summary.removeEventListener("click", handler);
  return event;
}

test("a first click on the link opens the fold, stores it first, and leaves the link to open its place", () => {
  press(link, false);
  expect(fold.open).toBe(true);
  expect(stored).toEqual([true]);
  expect(links.map((e) => e.detail)).toEqual([1]);
  press(link, false);
  expect(fold.open, "a first click never closes it").toBe(true);
});

test("a click on the open place's row toggles the fold and opens nothing more", () => {
  fold.open = true;
  const event = press(summary, true, { clientX: 260 });
  expect(event.defaultPrevented).toBe(true);
  expect(fold.open).toBe(false);
  press(link, true);
  expect(fold.open).toBe(true);
  expect(stored).toEqual([false, true]);
});

test("the row's free end opens the place through the link", () => {
  const event = press(summary, false, { clientX: 260 });
  expect(event.defaultPrevented, "not the browser's own fold").toBe(true);
  expect(fold.open).toBe(true);
  expect(links).toHaveLength(1);
});

test("the chevron folds alone, open and closed, and opens no place", () => {
  press(summary, false, { clientX: 10 });
  expect(fold.open).toBe(true);
  press(summary, false, { clientX: 10 });
  expect(fold.open).toBe(false);
  expect(stored).toEqual([true, false]);
  expect(links).toEqual([]);
});

test("Enter, a modified click and a middle click keep the link's own way", () => {
  for (const more of [{ detail: 0 }, { ctrlKey: true }, { metaKey: true }, { shiftKey: true }, { altKey: true }, { button: 1 }]) {
    const event = press(link, false, more);
    expect(event.defaultPrevented, JSON.stringify(more)).toBe(false);
  }
  expect(fold.open).toBe(false);
  expect(stored).toEqual([]);
});

test("→ and ← bring the stored state along with the fold's", () => {
  const key = (name: string) => {
    const handler = (e: Event) => foldKeyed(e as KeyboardEvent & { currentTarget: HTMLElement }, (open) => stored.push(open));
    summary.addEventListener("keydown", handler);
    link.dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true }));
    summary.removeEventListener("keydown", handler);
  };
  fold.open = true;
  key("ArrowRight");
  fold.open = false;
  key("ArrowLeft");
  key("Enter");
  expect(stored).toEqual([true, false]);
});
