import { flushSync } from "svelte";
import { afterEach, expect, test, vi } from "vitest";

import { Windowed } from "./windowed.svelte";

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = "";
});

test("a windowed table freezes its columns again when its header cells change (#2209)", async () => {
  // jsdom lays nothing out: the table is 400 px wide and each header cell 100 px.
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const width = this.tagName === "TABLE" ? 400 : 100;
    return { width, height: 20, top: 0, bottom: 20, left: 0, right: width, x: 0, y: 0, toJSON: () => ({}) } as DOMRect;
  });
  document.body.innerHTML = "<table><thead><tr><th>a</th><th>b</th></tr></thead><tbody></tbody></table>";
  const table = document.querySelector("table")!;
  const widths = () => [...table.tHead!.rows[0]!.cells].map((cell) => cell.style.width);
  const win = new Windowed({ count: () => 400, key: (i) => i, estimate: 20 });
  const stop = $effect.root(() => {
    win.list(table.tBodies[0]!);
  });
  flushSync();
  expect(table.style.tableLayout).toBe("fixed");
  expect(widths()).toEqual(["25%", "25%"]);

  // The view's columns change: the new cell gets its share, not what the frozen ones left over.
  table.tHead!.rows[0]!.append(Object.assign(document.createElement("th"), { textContent: "c" }));
  await new Promise((done) => setTimeout(done));
  flushSync();
  expect(widths()).toEqual(["25%", "25%", "25%"]);
  stop();
});

test("a windowed table with no header row freezes its columns at its first drawn row's cells, and lets go when it stops windowing (#2210)", () => {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const width = this.tagName === "TABLE" ? 400 : this.classList.contains("wide") ? 300 : 100;
    return { width, height: 20, top: 0, bottom: 20, left: 0, right: width, x: 0, y: 0, toJSON: () => ({}) } as DOMRect;
  });
  document.body.innerHTML = '<table><tbody><tr><td class="wide">a</td><td>b</td></tr></tbody></table>';
  const table = document.querySelector("table")!;
  let count = $state(400);
  const win = new Windowed({ count: () => count, key: (i) => i, estimate: 20 });
  const stop = $effect.root(() => {
    win.list(table.tBodies[0]!);
  });
  flushSync();
  expect(table.style.tableLayout).toBe("fixed");
  expect([...table.querySelectorAll("colgroup > col")].map((col) => (col as HTMLElement).style.width)).toEqual(["75%", "25%"]);

  count = 10;
  flushSync();
  expect(table.style.tableLayout).toBe("");
  expect(table.querySelector("colgroup")).toBeNull();
  stop();
});

test("a list that scrolls itself reads the viewport from its own scroll, keeps its padding, sizes spacer rows for the room, and says where an undrawn row sits (#2210)", () => {
  // jsdom lays nothing out: the list is a 300 px box at the top of the window.
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
    () => ({ width: 200, height: 300, top: 0, bottom: 300, left: 0, right: 200, x: 0, y: 0, toJSON: () => ({}) }) as DOMRect,
  );
  document.body.innerHTML = '<ol style="overflow-y: auto; padding-top: 6px; padding-bottom: 6px"><li class="before"></li><li class="after"></li></ol>';
  const list = document.querySelector("ol")!;
  let scrolled = 0;
  Object.defineProperty(list, "scrollTop", { get: () => scrolled, set: (px: number) => (scrolled = px) });
  Object.defineProperty(list, "clientHeight", { value: 300 });
  // Rows of 20 px, 1 px apart: a slot is 21 px.
  const win = new Windowed({ count: () => 1000, key: (i) => i, estimate: 20, gap: 1, overscan: 0 });
  const stop = $effect.root(() => {
    win.list(list);
    win.room("before")(list.querySelector<HTMLElement>(".before")!);
    win.room("after")(list.querySelector<HTMLElement>(".after")!);
  });
  flushSync();
  expect([win.start, win.end]).toEqual([0, 15]);

  // Scrolled 2,000 px into itself: its 6 px of padding come first, so row 94 is the first drawn.
  scrolled = 2000;
  list.dispatchEvent(new Event("scroll"));
  flushSync();
  expect([win.start, win.end]).toEqual([94, 110]);
  // A scroller is never smaller than its padding, so the room is spacer rows, the gap beside each left out.
  expect(list.getAttribute("style")).toBe("overflow-y: auto; padding-top: 6px; padding-bottom: 6px; overflow-anchor: none;");
  expect(list.querySelector<HTMLElement>(".before")!.style.height).toBe(`${94 * 21 - 1}px`);
  expect(list.querySelector<HTMLElement>(".after")!.style.height).toBe(`${(1000 - 110) * 21 - 1}px`);
  // Row 500 is 10,500 px down the list: under the 6 px of padding, less the 2,000 scrolled.
  expect(win.topOf(500)).toBe(6 + 500 * 21 - 2000);
  stop();
});
