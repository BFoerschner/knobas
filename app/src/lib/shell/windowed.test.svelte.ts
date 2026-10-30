import { flushSync } from "svelte";
import { afterEach, expect, test, vi } from "vitest";

import { Windowed } from "./windowed.svelte";

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = "";
});

test("a windowed table freezes its columns again when its header cells change (#2209)", async () => {
  // jsdom lays nothing out: the table is 400 px wide and each header cell 100 px.
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
    function (this: HTMLElement) {
      const width = this.tagName === "TABLE" ? 400 : 100;
      return {
        width,
        height: 20,
        top: 0,
        bottom: 20,
        left: 0,
        right: width,
        x: 0,
        y: 0,
        toJSON: () => ({}),
      } as DOMRect;
    },
  );
  document.body.innerHTML =
    "<table><thead><tr><th>a</th><th>b</th></tr></thead><tbody></tbody></table>";
  const table = document.querySelector("table")!;
  const widths = () =>
    [...table.tHead!.rows[0]!.cells].map((cell) => cell.style.width);
  const win = new Windowed({ count: () => 400, key: (i) => i, estimate: 20 });
  const stop = $effect.root(() => {
    win.list(table.tBodies[0]!);
  });
  flushSync();
  expect(table.style.tableLayout).toBe("fixed");
  expect(widths()).toEqual(["25%", "25%"]);

  // The view's columns change: the new cell gets its share, not what the frozen ones left over.
  table.tHead!.rows[0]!.append(
    Object.assign(document.createElement("th"), { textContent: "c" }),
  );
  await new Promise((done) => setTimeout(done));
  flushSync();
  expect(widths()).toEqual(["25%", "25%", "25%"]);
  stop();
});

test("a windowed table with no header row freezes its columns at its first drawn row's cells, and lets go when it stops windowing (#2210)", () => {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
    function (this: HTMLElement) {
      const width =
        this.tagName === "TABLE"
          ? 400
          : this.classList.contains("wide")
            ? 300
            : 100;
      return {
        width,
        height: 20,
        top: 0,
        bottom: 20,
        left: 0,
        right: width,
        x: 0,
        y: 0,
        toJSON: () => ({}),
      } as DOMRect;
    },
  );
  document.body.innerHTML =
    '<table><tbody><tr><td class="wide">a</td><td>b</td></tr></tbody></table>';
  const table = document.querySelector("table")!;
  let count = $state(400);
  const win = new Windowed({ count: () => count, key: (i) => i, estimate: 20 });
  const stop = $effect.root(() => {
    win.list(table.tBodies[0]!);
  });
  flushSync();
  expect(table.style.tableLayout).toBe("fixed");
  expect(
    [...table.querySelectorAll("colgroup > col")].map(
      (col) => (col as HTMLElement).style.width,
    ),
  ).toEqual(["75%", "25%"]);

  count = 10;
  flushSync();
  expect(table.style.tableLayout).toBe("");
  expect(table.querySelector("colgroup")).toBeNull();
  stop();
});

test("a list that scrolls itself reads the viewport from its own scroll, keeps its padding, sizes spacer rows for the room, and says where an undrawn row sits (#2210)", () => {
  // jsdom lays nothing out: the list is a 300 px box at the top of the window.
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
    () =>
      ({
        width: 200,
        height: 300,
        top: 0,
        bottom: 300,
        left: 0,
        right: 200,
        x: 0,
        y: 0,
        toJSON: () => ({}),
      }) as DOMRect,
  );
  document.body.innerHTML =
    '<ol style="overflow-y: auto; padding-top: 6px; padding-bottom: 6px"><li class="before"></li><li class="after"></li></ol>';
  const list = document.querySelector("ol")!;
  let scrolled = 0;
  Object.defineProperty(list, "scrollTop", {
    get: () => scrolled,
    set: (px: number) => (scrolled = px),
  });
  Object.defineProperty(list, "clientHeight", { value: 300 });
  // Rows of 20 px, 1 px apart: a slot is 21 px.
  const win = new Windowed({
    count: () => 1000,
    key: (i) => i,
    estimate: 20,
    gap: 1,
    overscan: 0,
    scrolls: true,
  });
  const stop = $effect.root(() => {
    win.list(list);
    win.room("before")(list.querySelector<HTMLElement>(".before")!);
    win.room("after")(list.querySelector<HTMLElement>(".after")!);
  });
  flushSync();
  // Its 6 px of padding sit above row 0, so 294 px of rows show: 14 of them (#2269).
  expect([win.start, win.end]).toEqual([0, 14]);

  // Scrolled 2,000 px into itself: its 6 px of padding come first, so row 94 is the first drawn.
  scrolled = 2000;
  list.dispatchEvent(new Event("scroll"));
  flushSync();
  expect([win.start, win.end]).toEqual([94, 110]);
  // A scroller is never smaller than its padding, so the room is spacer rows, the gap beside each left out.
  expect(list.getAttribute("style")).toBe(
    "overflow-y: auto; padding-top: 6px; padding-bottom: 6px; overflow-anchor: none;",
  );
  expect(list.querySelector<HTMLElement>(".before")!.style.height).toBe(
    `${94 * 21 - 1}px`,
  );
  expect(list.querySelector<HTMLElement>(".after")!.style.height).toBe(
    `${(1000 - 110) * 21 - 1}px`,
  );
  // Row 500 is 10,500 px down the list: under the 6 px of padding, less the 2,000 scrolled.
  expect(win.topOf(500)).toBe(6 + 500 * 21 - 2000);
  stop();
});

test("a list that only clips sideways (the run log's `overflow: auto`, #2211) still takes its room as padding: it grows with its rows and the page scrolls it (#2210)", () => {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
    () =>
      ({
        width: 200,
        height: 300,
        top: 0,
        bottom: 300,
        left: 0,
        right: 200,
        x: 0,
        y: 0,
        toJSON: () => ({}),
      }) as DOMRect,
  );
  // jsdom does not expand the shorthand; a browser computes `overflow: auto` as both of these.
  document.body.innerHTML =
    '<div style="overflow-x: auto; overflow-y: auto"></div>';
  const list = document.querySelector("div")!;
  const win = new Windowed({
    count: () => 1000,
    key: (i) => i,
    estimate: 20,
    overscan: 0,
  });
  const stop = $effect.root(() => {
    win.list(list);
  });
  flushSync();
  expect(win.after).toBeGreaterThan(0);
  expect(list.style.paddingBottom).toBe(`${win.after}px`);
  stop();
});

test("a list that starts far below the viewport draws one row, and one partway down draws only what reaches the screen (#2269)", () => {
  let listTop = 5_000;
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
    () =>
      ({ width: 200, height: 20_000, top: listTop, bottom: listTop + 20_000, left: 0, right: 200, x: 0, y: listTop, toJSON: () => ({}) }) as DOMRect,
  );
  vi.spyOn(window, "innerHeight", "get").mockReturnValue(1_000);
  document.body.innerHTML = "<div></div>";
  const list = document.querySelector("div")!;
  const win = new Windowed({ count: () => 1000, key: (i) => i, estimate: 20, overscan: 100 });
  const stop = $effect.root(() => {
    win.list(list);
  });
  flushSync();
  // A diff's card 4,000 px under the screen: nothing of it is near.
  expect(win.end - win.start).toBe(1);
  // Its top 600 px down the screen: 400 px of it show, 100 px more drawn.
  listTop = 600;
  window.dispatchEvent(new Event("resize"));
  flushSync();
  expect([win.start, win.end]).toEqual([0, 25]);
  stop();
});

test("a list inside a box that clips it (a diff's file card), the box below the screen, draws one row (#2269)", () => {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
    () => ({ width: 200, height: 300, top: 5_000, bottom: 5_300, left: 0, right: 200, x: 0, y: 5_000, toJSON: () => ({}) }) as DOMRect,
  );
  vi.spyOn(window, "innerHeight", "get").mockReturnValue(1_000);
  document.body.innerHTML = '<section style="overflow: hidden; overflow-y: hidden"><div></div></section>';
  const list = document.querySelector("div")!;
  Object.defineProperty(list.parentElement!, "clientHeight", { value: 300 });
  const win = new Windowed({ count: () => 1000, key: (i) => i, estimate: 20, overscan: 600 });
  const stop = $effect.root(() => {
    win.list(list);
  });
  flushSync();
  expect(win.end - win.start).toBe(1);
  stop();
});

test("a row held on a page keep-alive hid does not scroll the page shown after it when that page's rows are measured (#2267 merger)", () => {
  // Every ResizeObserver, so a row's measurement can be reported by hand.
  const observers: { targets: Element[]; report: (entries: ResizeObserverEntry[]) => void }[] = [];
  vi.stubGlobal(
    "ResizeObserver",
    class {
      targets: Element[] = [];
      constructor(report: (entries: ResizeObserverEntry[]) => void) {
        observers.push({ targets: this.targets, report });
      }
      observe(el: Element) {
        this.targets.push(el);
      }
      unobserve() {}
      disconnect() {}
    },
  );
  // A hidden page lays out nothing; a shown row sits 100 px down the screen.
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const [top, height] = this.closest("[hidden]") ? [0, 0] : this.classList.contains("main") ? [0, 1_000] : [100, 20];
    return { width: 200, height, top, bottom: top + height, left: 0, right: 200, x: 0, y: top, toJSON: () => ({}) } as DOMRect;
  });
  document.body.innerHTML =
    '<div class="main" style="overflow-y: auto"><div class="a"><div class="la"><div class="ra"></div></div></div><div class="b" hidden><div class="lb"><div class="rb"></div></div></div></div>';
  const main = document.querySelector<HTMLElement>(".main")!;
  let scrolled = 500;
  Object.defineProperty(main, "scrollTop", { get: () => scrolled, set: (px: number) => (scrolled = px) });
  Object.defineProperty(main, "clientHeight", { value: 1_000 });
  Object.defineProperty(main, "scrollHeight", { value: 50_000 });
  const a = new Windowed({ count: () => 1000, key: (i) => i, estimate: 20 });
  const b = new Windowed({ count: () => 1000, key: (i) => i, estimate: 20 });
  const stop = $effect.root(() => {
    a.list(document.querySelector(".la")!);
    a.row(0)(document.querySelector(".ra")!);
    b.list(document.querySelector(".lb")!);
    b.row(0)(document.querySelector(".rb")!);
  });
  flushSync();
  // The reader scrolls page A: its row is held.
  main.dispatchEvent(new Event("scroll"));
  // Page B is shown instead, and its row comes out 30 px, not 20.
  document.querySelector<HTMLElement>(".a")!.hidden = true;
  document.querySelector<HTMLElement>(".b")!.hidden = false;
  const rb = document.querySelector(".rb")!;
  const sizes = observers.find((o) => o.targets.includes(rb))!;
  sizes.report([{ target: rb, borderBoxSize: [{ blockSize: 30, inlineSize: 200 }], contentRect: { height: 30 } } as unknown as ResizeObserverEntry]);
  expect(scrolled).toBe(500);
  stop();
  vi.unstubAllGlobals();
});
