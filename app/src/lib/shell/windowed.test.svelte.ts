import { flushSync } from "svelte";
import { afterEach, expect, test, vi } from "vitest";

import { checkScale } from "./device-scale.svelte";
import { gridLength } from "./windowed";
import { growWindowsNow, laidOut, landOn, listScrolled, placeWindowsNow, Windowed } from "./windowed.svelte";

/**
 * Draws, then reads where each windowed list sits, as the end of the flush that attached it does, and draws again;
 * then draws the overscan below the view, as the second frame after the read does.
 */
function placed() {
  flushSync();
  placeWindowsNow();
  flushSync();
  growWindowsNow();
  flushSync();
}

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
  placed();
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
  placed();
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
  placed();
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
  placed();
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
  placed();
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
  placed();
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
  placed();
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

test("a jump holds the row it lands on, not the row above it still showing under a band, while what is above it grows (the compare-jump pass)", () => {
  // Björn, 2026-11-17: in Compare some smaller files were not scrolled to. A file picked lands under the diff's band,
  // which covers the scroller's top; the file before it still shows there, and as the topmost row on screen it was the
  // one held, so its lines arriving pushed the picked file down.
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
  document.body.innerHTML = '<div class="main" style="overflow-y: auto"><div class="l"><div class="ra"></div><div class="rb"></div></div></div>';
  const main = document.querySelector<HTMLElement>(".main")!;
  const [ra, rb] = [document.querySelector<HTMLElement>(".ra")!, document.querySelector<HTMLElement>(".rb")!];
  let scrolled = 0;
  // The file before is 500 px tall until its lines come; the picked one is a line long. The list starts 1,000 px down.
  let above = 500;
  Object.defineProperty(main, "scrollTop", { get: () => scrolled, set: (px: number) => (scrolled = px) });
  Object.defineProperty(main, "clientHeight", { value: 1_000 });
  Object.defineProperty(main, "scrollHeight", { value: 50_000 });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const at = (top: number, height: number) => ({ width: 200, height, top, bottom: top + height, left: 0, right: 200, x: 0, y: top, toJSON: () => ({}) }) as DOMRect;
    if (this === main) return at(0, 1_000);
    if (this === ra) return at(1_000 - scrolled, above);
    if (this === rb) return at(1_000 + above + 12 - scrolled, 63);
    return at(1_000 - scrolled, above + 75);
  });
  const win = new Windowed({ count: () => 1000, key: (i) => i, estimate: 20 });
  const stop = $effect.root(() => {
    win.list(document.querySelector(".l")!);
    win.row(0)(ra);
    win.row(1)(rb);
  });
  placed();
  // The reader scrolls down to the diff: the file before is the top row on screen.
  scrolled = 900;
  main.dispatchEvent(new Event("scroll"));
  // The pick: the picked file's top to the band's foot, 44 px down.
  landOn(rb, main, rb.getBoundingClientRect().top - 44);
  expect(rb.getBoundingClientRect().top).toBe(44);
  // The scroll that raised is the lists' own, not the reader's: it holds nothing new.
  const landing = new Event("scroll");
  main.dispatchEvent(landing);
  expect(listScrolled(landing)).toBe(true);
  // The file before draws its lines, 300 px more of them: the picked file stays at the band's foot.
  above = 800;
  const sizes = observers.find((o) => o.targets.includes(ra))!;
  sizes.report([{ target: ra, borderBoxSize: [{ blockSize: 800, inlineSize: 200 }], contentRect: { height: 800 } } as unknown as ResizeObserverEntry]);
  expect(rb.getBoundingClientRect().top).toBe(44);
  // The reader's own scroll is not the lists'.
  scrolled += 40;
  const reader = new Event("scroll");
  main.dispatchEvent(reader);
  expect(listScrolled(reader)).toBe(false);
  stop();
  vi.unstubAllGlobals();
});

test("lists attached together each hold their rows' room before any of them reads the page (the compare-jump pass)", () => {
  // A file's lines came in as a windowed list holding no room for its rows past the first screen until it read where it
  // sat; another list's read before then laid the page out 5,000 px short, and a page scrolled near its end was cut
  // back for good.
  document.body.innerHTML = '<div class="main" style="overflow-y: auto"><div class="la"></div><div class="lb"></div></div>';
  const [la, lb] = [document.querySelector<HTMLElement>(".la")!, document.querySelector<HTMLElement>(".lb")!];
  const seen: string[] = [];
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    // Each read of the page notes whether both lists held their room then.
    seen.push(`${la.style.paddingBottom !== ""} ${lb.style.paddingBottom !== ""}`);
    return { width: 200, height: 800, top: 0, bottom: 800, left: 0, right: 200, x: 0, y: 0, toJSON: () => ({}) } as DOMRect;
  });
  const a = new Windowed({ count: () => 1000, key: (i) => i, estimate: 20 });
  const b = new Windowed({ count: () => 1000, key: (i) => i, estimate: 20 });
  const stop = $effect.root(() => {
    a.list(la);
    b.list(lb);
  });
  flushSync();
  placeWindowsNow();
  expect(seen.length).toBeGreaterThan(0);
  expect(new Set(seen)).toEqual(new Set(["true true"]));
  stop();
});

test("the drawn rows hold still while the view moves inside them, and move to it once it comes within the margin of their edge (PR #2479)", () => {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
    () => ({ width: 200, height: 300, top: 0, bottom: 300, left: 0, right: 200, x: 0, y: 0, toJSON: () => ({}) }) as DOMRect,
  );
  document.body.innerHTML = '<ol style="overflow-y: auto"><li class="before"></li><li class="after"></li></ol>';
  const list = document.querySelector("ol")!;
  let scrolled = 0;
  Object.defineProperty(list, "scrollTop", { get: () => scrolled, set: (px: number) => (scrolled = px) });
  Object.defineProperty(list, "clientHeight", { value: 300 });
  // Rows of 20 px, no gap; 200 px of overscan each side, so the margin is 100 px.
  const win = new Windowed({ count: () => 1000, key: (i) => i, estimate: 20, overscan: 200, scrolls: true });
  const stop = $effect.root(() => {
    win.list(list);
    win.room("before")(list.querySelector<HTMLElement>(".before")!);
    win.room("after")(list.querySelector<HTMLElement>(".after")!);
  });
  const scrollTo = (px: number) => {
    scrolled = px;
    list.dispatchEvent(new Event("scroll"));
    flushSync();
    return [win.start, win.end];
  };
  placed();
  expect([win.start, win.end]).toEqual([0, 25]);
  // Every 20 px a row would come and go; the drawn rows (0 to 500 px) still hold the view and 100 px below it.
  expect(scrollTo(20)).toEqual([0, 25]);
  expect(scrollTo(100)).toEqual([0, 25]);
  // 101 px down, the view's bottom (401) is under 100 px from the rows' end: the window moves to it, overscan again.
  expect(scrollTo(101)).toEqual([0, 31]);
  expect(scrollTo(150)).toEqual([0, 31]);
  // Far away, the rows go to the view at once.
  expect(scrollTo(10_000)).toEqual([490, 525]);
  // The room above and below follows the drawn rows.
  expect(list.querySelector<HTMLElement>(".before")!.style.height).toBe(`${490 * 20}px`);
  // `reveal` brings a row near the window's edge in, and the window around it.
  win.reveal(530);
  flushSync();
  expect(win.start <= 530 && win.end > 530).toBe(true);
  stop();
});

test("a row the window takes out while it holds focus leaves focus on the list, and gets it back when drawn again (the extremes pass)", async () => {
  document.body.innerHTML = '<div class="list"><div class="row"><a href="#a">a</a><button type="button">b</button></div></div>';
  const list = document.querySelector<HTMLElement>(".list")!;
  const first = document.querySelector<HTMLElement>(".row")!;
  const win = new Windowed({ count: () => 400, key: (i) => i, estimate: 20 });
  let undraw: (() => void) | void = undefined;
  const stop = $effect.root(() => {
    win.list(list);
    undraw = win.row(390)(first);
  });
  placed();
  first.querySelector("button")!.focus();
  expect(document.activeElement?.textContent).toBe("b");

  // Scrolled far off: the window takes the row out of the DOM (row 390 is past the rows a 768 px view draws).
  first.remove();
  (undraw as unknown as () => void)();
  expect(document.activeElement).toBe(list);
  expect(list.dataset["heldFocus"]).toBe("");

  // Scrolled back: the same row, drawn anew, has focus on the same control.
  const again = Object.assign(document.createElement("div"), { innerHTML: '<a href="#a">a</a><button type="button">b</button>' });
  list.append(again);
  win.row(390)(again);
  await Promise.resolve();
  expect(document.activeElement).toBe(again.querySelector("button"));
  expect(list.dataset["heldFocus"]).toBeUndefined();
  stop();
});

// The Windows pass (2026-11-14): a pipeline's runs, half way down the screen under its chart, drew a screenful from the
// top before the list read where it sat, and kept those rows, since rows that cover the view stay.
test("rows drawn before a list reads where it sits are not held: a list half way down draws only what reaches the screen", () => {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
    () => ({ width: 200, height: 20_000, top: 500, bottom: 20_500, left: 0, right: 200, x: 0, y: 500, toJSON: () => ({}) }) as DOMRect,
  );
  vi.spyOn(window, "innerHeight", "get").mockReturnValue(1_000);
  document.body.innerHTML = "<div></div>";
  const list = document.querySelector("div")!;
  const win = new Windowed({ count: () => 1000, key: (i) => i, estimate: 20, overscan: 100 });
  const stop = $effect.root(() => {
    // The page draws its rows before the list is attached.
    void win.end;
    win.list(list);
  });
  placed();
  // 500 px of it show, 100 px more drawn: 30 rows, not the 55 a screen from the top would hold.
  expect([win.start, win.end]).toEqual([0, 30]);
  stop();
});

test("a list too short to window reads nothing of the page; lists that window read where they sit together, after the update that attached them", async () => {
  const rect = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect");
  document.body.innerHTML = '<div class="a"></div><div class="b"></div><div class="c"></div>';
  const short = new Windowed({ count: () => 10, key: (i) => i, estimate: 20 });
  const a = new Windowed({ count: () => 1000, key: (i) => i, estimate: 20 });
  const b = new Windowed({ count: () => 1000, key: (i) => i, estimate: 20 });
  const stop = $effect.root(() => {
    short.list(document.querySelector<HTMLElement>(".c")!);
    a.list(document.querySelector<HTMLElement>(".a")!);
    b.list(document.querySelector<HTMLElement>(".b")!);
  });
  flushSync();
  expect(rect).not.toHaveBeenCalled();
  await Promise.resolve();
  flushSync();
  const read = rect.mock.contexts.map((el) => (el as HTMLElement).className);
  expect(read).toContain("a");
  expect(read).toContain("b");
  expect(read).not.toContain("c");
  stop();
});

test("a list placed while its page is hidden draws a screenful from its top, and one shown before reads its hidden page as it always did (the ux13-overs pass)", () => {
  // Each list sits in a scroller of its page's own (Builds' runs, the Tickets list). A hidden page lays out nothing;
  // shown, the scroller is the 1,000 px screen and the list sits at its top.
  const scrolled = 0;
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const [top, height] = this.closest("[hidden]") ? [0, 0] : this.classList.contains("clip") ? [0, 1_000] : [-scrolled, 20_000];
    return { width: 200, height, top, bottom: top + height, left: 0, right: 200, x: 0, y: top, toJSON: () => ({}) } as DOMRect;
  });
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockImplementation(function (this: HTMLElement) {
    return this.closest("[hidden]") || !this.classList.contains("clip") ? 0 : 1_000;
  });
  vi.spyOn(window, "innerHeight", "get").mockReturnValue(1_000);
  document.body.innerHTML =
    '<div class="page"><section class="clip" style="overflow-y: auto"><div class="list"></div></section></div>' +
    '<div class="ahead" hidden><section class="clip" style="overflow-y: auto"><div class="list"></div></section></div>';
  const [shownList, aheadList] = [...document.querySelectorAll<HTMLElement>(".list")];
  // Rows of 20 px, 100 px of overscan.
  const shown = new Windowed({ count: () => 1000, key: (i) => i, estimate: 20, overscan: 100 });
  const ahead = new Windowed({ count: () => 1000, key: (i) => i, estimate: 20, overscan: 100 });
  const stop = $effect.root(() => {
    shown.list(shownList!);
    ahead.list(aheadList!);
  });
  placed();
  expect([shown.start, shown.end], "shown: the screen and its overscan").toEqual([0, 55]);
  expect([ahead.start, ahead.end], "placed hidden: as if the screen were over its top, not its overscan alone").toEqual([0, 55]);
  // The shown page hides: its list reads the nothing a hidden box measures, as before, which its drawn rows cover.
  document.querySelector<HTMLElement>(".page")!.hidden = true;
  window.dispatchEvent(new Event("resize"));
  flushSync();
  expect([shown.start, shown.end]).toEqual([0, 55]);
  // The page held ahead is shown: the rows it drew hidden cover the screen already.
  document.querySelector<HTMLElement>(".ahead")!.hidden = false;
  window.dispatchEvent(new Event("resize"));
  flushSync();
  expect([ahead.start, ahead.end]).toEqual([0, 55]);
  stop();
});

// The sizes pass (2026-11-15): a first open builds what the screen shows; the rows below the fold wait for the frame
// that shows the rows on screen, and the one after it.
test("a list that reads where it sits draws the view, the overscan above it, a quarter of the view below and a row past it, and the overscan below once two frames have drawn", async () => {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
    () => ({ width: 200, height: 20_000, top: -1_000, bottom: 19_000, left: 0, right: 200, x: 0, y: -1_000, toJSON: () => ({}) }) as DOMRect,
  );
  vi.spyOn(window, "innerHeight", "get").mockReturnValue(1_000);
  const frames: FrameRequestCallback[] = [];
  vi.spyOn(window, "requestAnimationFrame").mockImplementation((fn) => frames.push(fn));
  document.body.innerHTML = "<div></div>";
  const list = document.querySelector("div")!;
  const win = new Windowed({ count: () => 1000, key: (i) => i, estimate: 20, overscan: 400 });
  const stop = $effect.root(() => {
    win.list(list);
  });
  flushSync();
  placeWindowsNow();
  flushSync();
  // Scrolled 1,000 px into it: the view is rows 50 to 100 and 400 px above it are drawn; below it a quarter of the
  // view (250 px, rows 100 to 112) and one row past that, so the last drawn row is off screen.
  expect([win.start, win.end]).toEqual([30, 114]);
  // The frame that shows them, the one after, then a task: the 400 px below it too.
  frames.shift()!(0);
  frames.shift()!(0);
  flushSync();
  expect(win.end).toBe(114);
  await new Promise((done) => setTimeout(done));
  flushSync();
  expect([win.start, win.end]).toEqual([30, 120]);
  stop();
});

test("rows drawn before a list reads where it sits stay until the overscan below is drawn, where the overscan would hold them", () => {
  // The list's top on screen, the view cut 100 px down by a box that clips it (a footer over a rail's end).
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const box = this.tagName === "SECTION" ? { top: 0, height: 100 } : { top: 0, height: 20_000 };
    return { width: 200, height: box.height, top: box.top, bottom: box.top + box.height, left: 0, right: 200, x: 0, y: box.top, toJSON: () => ({}) } as DOMRect;
  });
  vi.spyOn(window, "requestAnimationFrame").mockImplementation(() => 0);
  document.body.innerHTML = '<section style="overflow-y: hidden"><div></div></section>';
  Object.defineProperty(document.querySelector("section")!, "clientHeight", { value: 100 });
  const list = document.querySelector("div")!;
  const keys = $state({ of: Array.from({ length: 1000 }, (_, i) => i) });
  const win = new Windowed({ count: () => keys.of.length, key: (i) => keys.of[i]!, estimate: 20, overscan: 200 });
  const stop = $effect.root(() => {
    void win.end;
    win.list(list);
  });
  flushSync();
  // Before the read, the overscan's 200 px from the top: rows 0 to 10.
  expect([win.start, win.end]).toEqual([0, 10]);
  // The page's rows are built again (a new array of the same keys) before the read.
  keys.of = [...keys.of];
  flushSync();
  placeWindowsNow();
  flushSync();
  // The view is rows 0 to 5, a quarter of it below and a row past: the ten drawn before the read stay, as the
  // overscan below would hold them.
  expect([win.start, win.end]).toEqual([0, 10]);
  stop();
});

test("a row's height is its box's on the device's grid, not the observer's cut to a 64th of a CSS pixel (#2508)", () => {
  vi.stubGlobal("devicePixelRatio", 1.25);
  const row = document.createElement("div");
  let box = 40.79999923706055;
  vi.spyOn(row, "getBoundingClientRect").mockImplementation(() => ({ height: box }) as DOMRect);
  // A 51-device-pixel row at 1.25, which the observer tells as 40.796875.
  const entry = { target: row, borderBoxSize: [{ blockSize: 40.796875, inlineSize: 100 }], contentRect: { height: 40.796875 } } as unknown as ResizeObserverEntry;
  expect(laidOut(entry) * 1.25).toBeCloseTo(51, 12);
  // A row a transform scales: its box is no measure of its layout, and the observer's height stands, on the grid.
  box = 81.6;
  expect(laidOut(entry) * 1.25).toBeCloseTo(51, 12);
  vi.unstubAllGlobals();
});

test("a zoom step (a new device scale) observes each drawn row afresh and writes the room again on the new grid, once (the zoom, 2026-11-16)", () => {
  vi.stubGlobal("devicePixelRatio", 1.25);
  const observed: Element[] = [];
  const unobserved: Element[] = [];
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe(el: Element) {
        observed.push(el);
      }
      unobserve(el: Element) {
        unobserved.push(el);
      }
      disconnect() {}
    },
  );
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
    () => ({ width: 200, height: 300, top: 0, bottom: 300, left: 0, right: 200, x: 0, y: 0, toJSON: () => ({}) }) as DOMRect,
  );
  document.body.innerHTML = '<ol style="overflow-y: auto"><li class="before"></li><li class="row"></li><li class="after"></li></ol>';
  const list = document.querySelector("ol")!;
  let scrolled = 0;
  Object.defineProperty(list, "scrollTop", { get: () => scrolled, set: (px: number) => (scrolled = px) });
  Object.defineProperty(list, "clientHeight", { value: 300 });
  // Rows of 18.85 px (13 px text at a line height of 1.45): off every grid but the device's own.
  const win = new Windowed({ count: () => 1000, key: (i) => i, estimate: 18.85, overscan: 200, scrolls: true });
  const row = list.querySelector<HTMLElement>(".row")!;
  const detached: (() => void)[] = [];
  const stop = $effect.root(() => {
    detached.push(win.list(list));
    win.room("before")(list.querySelector<HTMLElement>(".before")!);
    win.room("after")(list.querySelector<HTMLElement>(".after")!);
    win.row(0)(row);
  });
  placed();
  scrolled = 10_000;
  list.dispatchEvent(new Event("scroll"));
  flushSync();
  const before = list.querySelector<HTMLElement>(".before")!;
  expect(before.style.height).toBe(gridLength(win.before, 1.25));
  const seen = observed.filter((el) => el === row).length;

  // 110 % on that screen: 1.375.
  vi.stubGlobal("devicePixelRatio", 1.25 * 1.1);
  checkScale();
  flushSync();
  expect(unobserved.filter((el) => el === row).length).toBe(1);
  expect(observed.filter((el) => el === row).length).toBe(seen + 1);
  expect(before.style.height).toBe(gridLength(win.before, 1.25 * 1.1));
  expect(before.style.height).not.toBe(gridLength(win.before, 1.25));
  // The same scale told again (a resize with no zoom): nothing is read again.
  checkScale();
  flushSync();
  expect(unobserved.filter((el) => el === row).length).toBe(1);

  // Gone, the list hears no more steps.
  for (const detach of detached) detach();
  stop();
  const gone = observed.length;
  vi.stubGlobal("devicePixelRatio", 2);
  checkScale();
  expect(observed.length).toBe(gone);
  vi.unstubAllGlobals();
});
