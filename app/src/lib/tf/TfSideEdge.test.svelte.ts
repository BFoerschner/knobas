/**
 * **Every sidebar's edge** (`TfSideEdge.svelte`, `side-width.svelte.ts`; the
 * sidebar-resize pass): a drag sets the width one frame at a time and keeps it
 * when the pointer lets go, the keys step it, a double click returns it to the
 * default, the floor and the sidebar's share of its grid hold, and the width is
 * kept per sidebar. Then that every sidebar the app draws is wired to it.
 *
 * jsdom lays nothing out, so the grid's drawn columns and its width are stood
 * in for: an inline `grid-template-columns` is what `getComputedStyle` reads
 * back, and the grid's box is stubbed.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import Fixture from "./TfSideEdge.fixture.svelte";
import {
  clampWidth,
  createSideWidths,
  cssWidth,
  maxWidth,
  SIDE_IDS,
  sideKey,
  sideProperty,
  sideWidths,
  SIDES,
  type SideId,
} from "./side-width.svelte";

/** A storage for the widths, which this runner's jsdom lacks (`test-no-storage.ts`). */
function memoryStorage(): Storage {
  const held = new Map<string, string>();
  return {
    get length() {
      return held.size;
    },
    clear: () => held.clear(),
    getItem: (key) => held.get(key) ?? null,
    key: (at) => [...held.keys()][at] ?? null,
    removeItem: (key) => void held.delete(key),
    setItem: (key, value) => void held.set(key, value),
  };
}

describe("the widths", () => {
  test("a width clamps to the floor, the sidebar's share of its grid and its cap, in whole pixels", () => {
    expect(clampWidth("builds", 90)).toBe(180);
    expect(clampWidth("builds", 412.4)).toBe(412);
    // No grid laid out: no share to hold, so a wide width stands.
    expect(clampWidth("builds", 4000)).toBe(4000);
    expect(clampWidth("builds", 4000, 1440)).toBe(648);
    // At 5120 px the share still decides, not a fixed cap.
    expect(clampWidth("builds", 4000, 5120)).toBe(2304);
    // The ticket card keeps its cap (#2402) as well as its share.
    expect(clampWidth("ticket-card", 4000, 5120)).toBe(720);
    expect(clampWidth("ticket-card", 4000, 1000)).toBe(600);
    // A grid narrower than the floor's share still gives the floor.
    expect(maxWidth("assets", 300)).toBe(240);
  });

  test("the property holds the width clamped in CSS too, so a window made narrower later keeps the main column", () => {
    expect(cssWidth("builds", 420)).toBe("clamp(180px, 420px, 45%)");
    expect(cssWidth("docs-outline", 300)).toBe("clamp(180px, 300px, 35%)");
    expect(sideProperty("docs-tree")).toBe("--side-docs-tree");
  });

  test("each sidebar keeps its own width under its own key, read back clamped, and a reset forgets it", () => {
    const storage = memoryStorage();
    const widths = createSideWidths(() => storage);
    widths.set("builds", 412);
    widths.set("docs-tree", 90);
    expect(storage.getItem("knobas.side.builds")).toBe("412");
    expect(storage.getItem("knobas.side.docs-tree")).toBe("180");
    expect(widths.px("settings")).toBeNull();
    storage.setItem("knobas.ticket.side", "9000");
    storage.setItem("knobas.side.pr", "wide");
    const again = createSideWidths(() => storage);
    again.load();
    expect(again.px("builds")).toBe(412);
    expect(again.px("docs-tree")).toBe(180);
    expect(again.px("ticket-card")).toBe(720);
    expect(again.px("pr")).toBeNull();
    again.reset("builds");
    expect(again.px("builds")).toBeNull();
    expect(storage.getItem("knobas.side.builds")).toBeNull();
    expect(again.px("docs-tree")).toBe(180);
  });

  test("a storage that throws costs the memory, never the width", () => {
    const widths = createSideWidths(() => {
      throw new Error("blocked");
    });
    widths.set("compare", 400);
    expect(widths.px("compare")).toBe(400);
    widths.load();
    expect(widths.px("compare")).toBeNull();
  });

  test("the ticket card keeps the key it had (#2402); every other sidebar its own", () => {
    expect(sideKey("ticket-card")).toBe("knobas.ticket.side");
    const keys = SIDE_IDS.map(sideKey);
    expect(new Set(keys).size).toBe(keys.length);
  });
});

describe("the edge", () => {
  let target: HTMLElement;
  let app: Record<string, unknown> | undefined;
  let frames: FrameRequestCallback[];
  let storage: Storage;

  beforeEach(() => {
    frames = [];
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => frames.push(callback));
    vi.stubGlobal("cancelAnimationFrame", (id: number) => {
      frames[id - 1] = () => {};
    });
    storage = memoryStorage();
    Object.defineProperty(window, "localStorage", { value: storage, configurable: true, writable: true });
  });

  afterEach(() => {
    if (app) void unmount(app);
    app = undefined;
    target?.remove();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    Object.defineProperty(window, "localStorage", { value: undefined, configurable: true, writable: true });
  });

  /** The grid as laid out: `width` wide, its columns `columns`. */
  async function draw(side: SideId, columns: string, width = 1440, onfit?: (grid: HTMLElement) => number | null) {
    target = document.createElement("div");
    document.body.append(target);
    app = mount(Fixture, { target, props: { side, onfit } });
    flushSync();
    await tick();
    grid().style.gridTemplateColumns = columns;
    vi.spyOn(grid(), "getBoundingClientRect").mockReturnValue({ width, left: 0, right: width, top: 0, bottom: 0, height: 0, x: 0, y: 0, toJSON: () => ({}) } as DOMRect);
  }

  const grid = () => target.querySelector<HTMLElement>(".grid")!;
  const edge = () => target.querySelector<HTMLElement>("[role=separator]")!;
  const drawn = (side: SideId) => grid().style.getPropertyValue(sideProperty(side));
  const frame = () => {
    const now = frames.splice(0);
    for (const callback of now) callback(performance.now());
  };
  const pointer = (type: string, x: number, on: EventTarget = window) =>
    on.dispatchEvent(new MouseEvent(type, { button: 0, clientX: x, bubbles: true, cancelable: true }));
  const key = (name: string, options: KeyboardEventInit = {}) =>
    edge().dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true, ...options }));

  test("a separator a keyboard reaches, named for its sidebar, its numbers the drawn width and the bounds", async () => {
    await draw("builds", "300px 1140px");
    const at = edge();
    expect(at.tabIndex).toBe(0);
    expect(at.getAttribute("aria-orientation")).toBe("vertical");
    expect(at.getAttribute("aria-label")).toBe("Resize the pipelines");
    expect(at.title).toBe("Drag to resize, double-click for the default width");
    expect(at.dataset["at"]).toBe("start");
    expect(at.getAttribute("aria-valuemin")).toBe("180");
    at.dispatchEvent(new FocusEvent("focus"));
    flushSync();
    expect(at.getAttribute("aria-valuenow")).toBe("300");
    expect(at.getAttribute("aria-valuemax")).toBe("648");
    // Nothing kept: the page's CSS draws its default.
    expect(drawn("builds")).toBe("");
  });

  test("a drag writes the grid's property once a frame however many moves came, and keeps the width only when it lets go", async () => {
    await draw("builds", "300px 1140px");
    pointer("pointerdown", 300, edge());
    expect(document.querySelector(".tf-sdrag-cover"), "the cover holds the cursor").not.toBeNull();
    flushSync();
    expect(edge().classList.contains("tf-sedge-on")).toBe(true);
    pointer("pointermove", 310);
    pointer("pointermove", 330);
    pointer("pointermove", 360);
    expect(frames).toHaveLength(1);
    expect(drawn("builds"), "nothing is written before the frame").toBe("");
    frame();
    expect(drawn("builds")).toBe(cssWidth("builds", 360));
    expect(edge().getAttribute("aria-valuenow")).toBe("360");
    // Nothing Svelte draws hears of the drag before it settles.
    expect(sideWidths.px("builds")).toBeNull();
    pointer("pointermove", 420);
    pointer("pointerup", 420);
    flushSync();
    expect(drawn("builds")).toBe(cssWidth("builds", 420));
    expect(sideWidths.px("builds")).toBe(420);
    expect(storage.getItem("knobas.side.builds")).toBe("420");
    expect(document.querySelector(".tf-sdrag-cover")).toBeNull();
    expect(edge().classList.contains("tf-sedge-on")).toBe(false);
    // A move once the pointer is up drags nothing.
    pointer("pointermove", 600);
    frame();
    expect(drawn("builds")).toBe(cssWidth("builds", 420));
  });

  test("a right column widens as the edge moves left", async () => {
    await draw("compare", "1118px 300px");
    expect(edge().dataset["at"]).toBe("end");
    pointer("pointerdown", 1140, edge());
    pointer("pointermove", 1040);
    pointer("pointerup", 1040);
    flushSync();
    expect(sideWidths.px("compare")).toBe(400);
  });

  test("a drag stops at the floor and at the sidebar's share of the grid", async () => {
    await draw("builds", "300px 1140px");
    pointer("pointerdown", 300, edge());
    pointer("pointermove", 20);
    frame();
    expect(drawn("builds")).toBe(cssWidth("builds", 180));
    pointer("pointermove", 1400);
    frame();
    expect(drawn("builds")).toBe(cssWidth("builds", 648));
    pointer("pointerup", 1400);
    flushSync();
    expect(sideWidths.px("builds")).toBe(648);
  });

  test("a cancelled drag puts back what was drawn and keeps nothing", async () => {
    await draw("docs-tree", "300px 1140px");
    pointer("pointerdown", 300, edge());
    pointer("pointermove", 380);
    frame();
    expect(drawn("docs-tree")).toBe(cssWidth("docs-tree", 380));
    pointer("pointercancel", 380);
    flushSync();
    expect(drawn("docs-tree")).toBe("");
    expect(sideWidths.px("docs-tree")).toBeNull();
    expect(document.querySelector(".tf-sdrag-cover")).toBeNull();
  });

  test("a press with no move keeps nothing, so a double click's presses do not", async () => {
    await draw("settings", "230px 1210px");
    pointer("pointerdown", 230, edge());
    pointer("pointerup", 230);
    flushSync();
    expect(sideWidths.px("settings")).toBeNull();
    expect(storage.getItem("knobas.side.settings")).toBeNull();
  });

  test("a double click returns the sidebar to its default: nothing kept, the CSS draws it", async () => {
    await draw("repo-tree", "280px 1142px");
    pointer("pointerdown", 280, edge());
    pointer("pointerup", 400);
    flushSync();
    expect(drawn("repo-tree")).toBe(cssWidth("repo-tree", 400));
    edge().dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    flushSync();
    expect(drawn("repo-tree")).toBe("");
    expect(sideWidths.px("repo-tree")).toBeNull();
    expect(storage.getItem("knobas.side.repo-tree")).toBeNull();
  });

  test("a sidebar with a fit of its own takes that width from a double click and from Enter", async () => {
    await draw("ticket-card", "320px 1120px", 1440, () => 455);
    edge().dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    flushSync();
    expect(sideWidths.px("ticket-card")).toBe(455);
    sideWidths.reset("ticket-card");
    key("Enter");
    flushSync();
    expect(sideWidths.px("ticket-card")).toBe(455);
  });

  test("the keys: the arrows step 16 px, 64 with Shift, Home and End go to the bounds, Enter to the default", async () => {
    await draw("diff-files", "240px 1182px");
    key("ArrowRight");
    flushSync();
    expect(sideWidths.px("diff-files")).toBe(256);
    key("ArrowLeft");
    key("ArrowLeft");
    flushSync();
    expect(sideWidths.px("diff-files")).toBe(224);
    key("ArrowRight", { shiftKey: true });
    flushSync();
    expect(sideWidths.px("diff-files")).toBe(288);
    expect(edge().getAttribute("aria-valuenow")).toBe("288");
    key("End");
    flushSync();
    expect(sideWidths.px("diff-files")).toBe(648);
    key("Home");
    flushSync();
    expect(sideWidths.px("diff-files")).toBe(160);
    key("Enter");
    flushSync();
    expect(sideWidths.px("diff-files")).toBeNull();
    expect(drawn("diff-files")).toBe("");
  });

  test("on a right column ArrowLeft widens it", async () => {
    await draw("pr", "1118px 300px");
    key("ArrowLeft");
    flushSync();
    expect(sideWidths.px("pr")).toBe(316);
  });

  test("a kept width draws on the grid as the page mounts, as after a restart", async () => {
    storage.setItem("knobas.side.assets", "512");
    sideWidths.load();
    await draw("assets", "928px 512px");
    expect(drawn("assets")).toBe(cssWidth("assets", 512));
    expect(edge().getAttribute("aria-valuenow")).toBe("512");
  });

  test("below 900 px, where every sidebar stacks or folds behind its bar, there is no edge", async () => {
    const real = window.matchMedia;
    window.matchMedia = ((query: string) => ({
      matches: query === "(max-width: 900px)",
      media: query,
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    })) as typeof window.matchMedia;
    try {
      await draw("tickets-views", "230px 870px 340px");
      expect(target.querySelector("[role=separator]")).toBeNull();
    } finally {
      window.matchMedia = real;
    }
  });
});

describe("every sidebar is wired", () => {
  const LIB = join(import.meta.dirname, "..");
  const files = (dir: string): string[] =>
    readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) return files(path);
      return /\.(svelte|css)$/.test(entry.name) && !entry.name.includes(".fixture.") ? [path] : [];
    });
  const sources = files(LIB).map((path) => ({ path, text: readFileSync(path, "utf8") }));
  const edgeCss = readFileSync(join(LIB, "tf/css/side-edge.css"), "utf8");

  test.each(SIDE_IDS)("%s: a grid reads its width, an edge drags it, the property is not inherited", (id) => {
    const reads = sources.filter(({ text }) => text.includes(`var(--side-${id},`));
    expect(reads.length, `no grid reads --side-${id}`).toBeGreaterThan(0);
    const drawsEdge = sources.filter(({ text }) => text.includes(`<TfSideEdge side="${id}"`));
    expect(drawsEdge.length, `no page draws the edge of ${id}`).toBe(1);
    // The edge's own grid carries the width: the property is set where it is read.
    expect(drawsEdge[0]!.text).toContain(`sideWidth("${id}")`);
    expect(edgeCss).toMatch(new RegExp(`@property --side-${id} \\{ syntax: "\\*"; inherits: false; \\}`));
    // Each read has the default as its fallback, so a page at its defaults draws as it did.
    for (const { text } of reads) expect(text).toMatch(new RegExp(`var\\(--side-${id}, \\d+px\\)`));
  });

  test("no sidebar is left out of the list", () => {
    const named = new Set(sources.flatMap(({ text }) => [...text.matchAll(/<TfSideEdge side="([\w-]+)"/g)].map((match) => match[1])));
    expect([...named].sort()).toEqual([...SIDE_IDS].sort());
    expect(Object.keys(SIDES)).toHaveLength(14);
  });
});
