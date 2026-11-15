/**
 * Keep-alive's cache of pages (#2247): which pages the page host keeps
 * mounted and hidden, under *Pages kept open*.
 */
import { expect, test } from "vitest";

import { createKeptPagesSetting, hold, holdBehind, type Held } from "./kept-pages.svelte";

/** Visit `keys` in order under `cap`, as the page host does. */
function visitAll(keys: string[], cap: number, from: Held<string>[] = []): Held<string>[] {
  let held = from;
  for (const key of keys) held = hold(held, { key, value: key }, cap);
  return held;
}
const keysOf = (held: Held<string>[]) => held.map((page) => page.key);

test("the shown page is last, and the five visited before it are kept behind it", () => {
  expect(keysOf(visitAll(["a", "b", "c", "d", "e", "f"], 5))).toEqual(["a", "b", "c", "d", "e", "f"]);
});

test("going back to a kept page keeps it, moves it last and counts the showing", () => {
  const held = visitAll(["a", "b", "c", "d", "e", "f", "b"], 5);
  expect(keysOf(held)).toEqual(["a", "c", "d", "e", "f", "b"]);
  expect(held.at(-1)!.shows).toBe(1);
});

test("past the cap the least recently used page is dropped, and comes back fresh", () => {
  const held = visitAll(["a", "b", "c", "d", "e", "f", "g"], 5);
  expect(keysOf(held)).toEqual(["b", "c", "d", "e", "f", "g"]);
  const again = hold(held, { key: "a", value: "a" }, 5);
  expect(again.at(-1)).toEqual({ key: "a", value: "a", shows: 0, visible: true });
  expect(keysOf(again)).toEqual(["c", "d", "e", "f", "g", "a"]);
});

test("a new address of the shown page updates it without counting a showing", () => {
  const held = hold(visitAll(["a", "b"], 5), { key: "b", value: "b, another tab" }, 5);
  expect(held.at(-1)).toEqual({ key: "b", value: "b, another tab", shows: 0, visible: true });
});

test("with none kept open only the shown page is held: today's behaviour", () => {
  expect(keysOf(visitAll(["a", "b", "c"], 0))).toEqual(["c"]);
});

test("a lower cap drops the hidden pages down to it at once", () => {
  const held = visitAll(["a", "b", "c", "d", "e", "f"], 5);
  expect(keysOf(hold(held, null, 2))).toEqual(["e", "f"]);
  expect(keysOf(hold(held, { key: "f", value: "f" }, 2))).toEqual(["d", "e", "f"]);
});

test("an address with no page hides them all, and the cap counts every one", () => {
  const held = hold(visitAll(["a", "b", "c"], 2), null, 2);
  expect(keysOf(held)).toEqual(["b", "c"]);
  expect(held.map((page) => page.visible)).toEqual([false, false]);
  // Back to the page that was in front before: it was hidden, so it is shown again.
  expect(hold(held, { key: "c", value: "c" }, 2).at(-1)!.shows).toBe(1);
});

test("only the shown page is visible", () => {
  const held = visitAll(["a", "b", "c"], 5);
  expect(held.map((page) => page.visible)).toEqual([false, false, true]);
});

test("Pages kept open runs from 0 to 15, 5 until set, whole numbers only", () => {
  const setting = createKeptPagesSetting();
  expect(setting.value).toBe(5);
  setting.set(0);
  expect(setting.value).toBe(0);
  setting.set(40);
  expect(setting.value).toBe(15);
  setting.set(-3);
  expect(setting.value).toBe(0);
  setting.set(2.6);
  expect(setting.value).toBe(3);
  setting.set(Number.NaN);
  expect(setting.value).toBe(3);
  setting.reset();
  expect(setting.value).toBe(5);
});

test("a page held ahead goes behind every page held, hidden, and only into a free room", () => {
  const held = visitAll(["a", "b"], 3);
  const ahead = holdBehind(held, { key: "x", value: "x" }, 3);
  expect(keysOf(ahead), "least recently used: the first a later visit drops").toEqual(["x", "a", "b"]);
  expect(ahead[0]!.visible).toBe(false);
  expect(ahead[0]!.shows).toBe(0);
  expect(keysOf(holdBehind(ahead, { key: "a", value: "a" }, 3)), "a page held already stays where it is").toEqual(["x", "a", "b"]);
  const full = visitAll(["a", "b", "c", "d"], 3);
  expect(keysOf(holdBehind(full, { key: "x", value: "x" }, 3)), "no room: it drops no page the reader left").toEqual(["a", "b", "c", "d"]);
  expect(keysOf(holdBehind(visitAll(["a"], 0), { key: "x", value: "x" }, 0)), "Pages kept open at 0 holds none ahead").toEqual(["a"]);
});

test("pressing a page held ahead shows that page, counted as a return", () => {
  const ahead = holdBehind(visitAll(["a"], 5), { key: "x", value: "x" }, 5);
  const shown = hold(ahead, { key: "x", value: "x" }, 5);
  expect(keysOf(shown)).toEqual(["a", "x"]);
  expect(shown.at(-1)!.visible).toBe(true);
  expect(shown.at(-1)!.shows).toBe(1);
});
