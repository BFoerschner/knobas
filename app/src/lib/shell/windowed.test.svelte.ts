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
