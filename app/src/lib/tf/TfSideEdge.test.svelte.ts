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
import { DragPace, forgetPace, landedIn, markSlow, rereadPace, REST_MS, SLOW_MS, startsAsGhost, STORE_PREFIX } from "./side-pace";
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
    // End on a grid not laid out: no bound, so the default rather than an endless width.
    expect(clampWidth("builds", Number.POSITIVE_INFINITY)).toBe(300);
    // At 5120 px the share still decides, not a fixed cap.
    expect(clampWidth("builds", 4000, 5120)).toBe(2304);
    // The ticket card keeps its cap (#2402) as well as its share.
    expect(clampWidth("ticket-card", 4000, 5120)).toBe(720);
    expect(clampWidth("ticket-card", 4000, 1000)).toBe(600);
    expect(maxWidth("docs-outline", 1044)).toBe(328);
    expect(maxWidth("docs-outline", 2400)).toBe(1200);
    // A grid narrower than the floor's share still gives the floor.
    expect(maxWidth("assets", 300)).toBe(240);
  });

  test("the property holds the width clamped in CSS too, so a window made narrower later keeps the main column", () => {
    expect(cssWidth("builds", 420)).toBe("clamp(180px, 420px, 45%)");
    // The outline leaves Docs' page its reading column (660 px and the gap), so a wide outline never folds itself away.
    expect(cssWidth("docs-outline", 300)).toBe("clamp(180px, 300px, min(50%, 100% - 716px))");
    expect(sideProperty("docs-tree")).toBe("--side-docs-tree");
  });

  test("the width kept for Tickets' preview, which is gone (ux18), is cleared at the next load", () => {
    const storage = memoryStorage();
    storage.setItem("knobas.side.tickets-preview", "420");
    storage.setItem("knobas.side.tickets-views", "260");
    const widths = createSideWidths(() => storage);
    widths.load();
    expect(storage.getItem("knobas.side.tickets-preview")).toBeNull();
    expect(widths.px("tickets-views")).toBe(260);
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
  /** The frames asked for, by id, and the frame clock: each `frame()` runs them at the clock moved on by its gap. */
  let frames: Map<number, FrameRequestCallback>;
  let nextFrame: number;
  let clock: number;
  let storage: Storage;

  beforeEach(() => {
    frames = new Map();
    nextFrame = 1;
    clock = 1000;
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      frames.set(nextFrame, callback);
      return nextFrame++;
    });
    vi.stubGlobal("cancelAnimationFrame", (id: number) => void frames.delete(id));
    storage = memoryStorage();
    Object.defineProperty(window, "localStorage", { value: storage, configurable: true, writable: true });
  });

  afterEach(() => {
    if (app) void unmount(app);
    app = undefined;
    target?.remove();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    forgetPace();
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
  /** One frame, `gap` ms after the last (16.7 ms, a 60 Hz panel's pace, by default). */
  const frame = (gap = 16.7) => {
    clock += gap;
    const now = [...frames.values()];
    frames.clear();
    for (const callback of now) callback(clock);
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
    expect(frames.size, "one frame asked for however many moves came").toBe(1);
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

  test("a drag that ends where it started keeps nothing", async () => {
    await draw("settings", "230px 1210px");
    pointer("pointerdown", 230, edge());
    pointer("pointermove", 231);
    pointer("pointermove", 230);
    pointer("pointerup", 230);
    flushSync();
    expect(sideWidths.px("settings")).toBeNull();
    expect(drawn("settings")).toBe("");
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

  const transform = () => edge().style.getPropertyValue("transform");

  test("a page that lays itself out slower than a frame turns the drag into a ghost: the line follows, the width waits", async () => {
    await draw("builds", "300px 1140px");
    pointer("pointerdown", 300, edge());
    pointer("pointermove", 320);
    frame();
    expect(drawn("builds")).toBe(cssWidth("builds", 320));
    // The first written frame took 30 ms to lay out: it alone turns this drag into a ghost (2026-11-17).
    pointer("pointermove", 340);
    frame(30);
    expect(drawn("builds"), "the ghost writes no width").toBe(cssWidth("builds", 320));
    expect(transform()).toBe("translateX(20px)");
    expect(edge().getAttribute("aria-valuenow")).toBe("340");
    expect(startsAsGhost("builds"), "one slow frame marks no sidebar").toBe(false);
    pointer("pointermove", 380);
    frame();
    expect(drawn("builds")).toBe(cssWidth("builds", 320));
    expect(transform()).toBe("translateX(60px)");
    // The pointer rests: the width lands, the line back on the edge in the same frame.
    for (let at = 0; at < REST_MS; at += 16.7) frame();
    frame();
    expect(drawn("builds")).toBe(cssWidth("builds", 380));
    expect(transform()).toBe("");
    pointer("pointermove", 430);
    frame();
    expect(transform()).toBe("translateX(50px)");
    pointer("pointerup", 450);
    flushSync();
    expect(drawn("builds")).toBe(cssWidth("builds", 450));
    expect(transform()).toBe("");
    expect(sideWidths.px("builds")).toBe(450);
  });

  test("one slow frame after the first keeps the drag live", async () => {
    await draw("builds", "300px 1140px");
    pointer("pointerdown", 300, edge());
    for (const [x, gap] of [[320, 16.7], [330, 16.7], [340, 40], [360, 16.7], [380, 16.7], [400, 16.7], [420, 40], [440, 16.7]] as const) {
      pointer("pointermove", x);
      frame(gap);
    }
    frame();
    expect(drawn("builds")).toBe(cssWidth("builds", 440));
    expect(transform()).toBe("");
    pointer("pointerup", 440);
    expect(startsAsGhost("builds")).toBe(false);
  });

  test("a sidebar that tripped starts its next drag as a ghost, until two releases land within a frame", async () => {
    await draw("tickets-views", "230px 870px 340px");
    markSlow("tickets-views");
    pointer("pointerdown", 230, edge());
    pointer("pointermove", 280);
    frame();
    expect(drawn("tickets-views"), "a ghost from the first frame").toBe("");
    expect(transform()).toBe("translateX(50px)");
    pointer("pointerup", 280);
    expect(drawn("tickets-views")).toBe(cssWidth("tickets-views", 280));
    // The release's frame, and the one after it: laid out within the line.
    frame();
    frame();
    expect(startsAsGhost("tickets-views"), "one fast release is not enough").toBe(true);
    pointer("pointerdown", 280, edge());
    pointer("pointermove", 300);
    pointer("pointerup", 300);
    frame();
    frame();
    expect(startsAsGhost("tickets-views")).toBe(false);
  });

  test("a landing at rest counts toward clearing a mark, as a release does", async () => {
    await draw("builds", "300px 1140px");
    markSlow("builds");
    pointer("pointerdown", 300, edge());
    pointer("pointermove", 360);
    frame();
    expect(transform()).toBe("translateX(60px)");
    // The pointer rests: the width lands, and the frame after it comes within the line.
    for (let at = 0; at < REST_MS; at += 16.7) frame();
    frame();
    expect(drawn("builds")).toBe(cssWidth("builds", 360));
    frame();
    expect(startsAsGhost("builds"), "one fast landing is not enough").toBe(true);
    pointer("pointerup", 380);
    frame();
    frame();
    expect(startsAsGhost("builds"), "the release is the second").toBe(false);
  });

  test("a slow machine's first drag marks the sidebar: its next drag starts as a ghost", async () => {
    await draw("tickets-views", "230px 870px 340px");
    pointer("pointerdown", 230, edge());
    pointer("pointermove", 250);
    frame();
    pointer("pointermove", 270);
    frame(40);
    expect(transform(), "a ghost after the first slow frame").toBe("translateX(20px)");
    pointer("pointerup", 290);
    // The release lays the page out as slowly: the second slow frame in four.
    frame();
    frame(40);
    expect(startsAsGhost("tickets-views")).toBe(true);
    expect(storage.getItem(STORE_PREFIX + "tickets-views")).toBe("1");
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

describe("the pace", () => {
  test("two slow frames within four written in a row trip the ghost; spread wider they do not", () => {
    const run = (gaps: number[]) => {
      const pace = new DragPace(false);
      let ts = 0;
      pace.wrote(ts);
      for (const gap of gaps) {
        ts += gap;
        pace.frame(ts);
        pace.wrote(ts);
      }
      return pace.ghost;
    };
    expect(run([16, 30, 30])).toBe(true);
    expect(run([16, 30, 16, 16, 30])).toBe(true);
    expect(run([16, 30, 16, 16, 16, 30])).toBe(false);
    expect(run([19, 19, 19, 19])).toBe(false);
    expect(run([16, 19.5, 19.5])).toBe(true);
  });

  test("the first written frame alone turns its drag into a ghost when it is slow, and marks nothing", () => {
    const first = (gap: number) => {
      const pace = new DragPace(false);
      pace.wrote(0);
      const turned = pace.frame(gap);
      return { turned, ghost: pace.ghost, marked: pace.marked };
    };
    expect(first(SLOW_MS + 0.5)).toEqual({ turned: true, ghost: true, marked: false });
    expect(first(SLOW_MS)).toEqual({ turned: false, ghost: false, marked: false });
    expect(first(16.7)).toEqual({ turned: false, ghost: false, marked: false });
  });

  test("two slow frames in four mark the sidebar, a ghost's landings among them", () => {
    const pace = new DragPace(false, "builds");
    pace.wrote(0);
    pace.frame(30);
    expect(pace.ghost).toBe(true);
    expect(startsAsGhost("builds")).toBe(false);
    // The ghost lands at rest, and that frame is slow too.
    pace.wrote(500, true);
    pace.frame(540);
    expect(pace.marked).toBe(true);
    expect(startsAsGhost("builds")).toBe(true);
    forgetPace();
  });

  test("a release's landing is costed: slow, it marks a sidebar whose first frame was slow", () => {
    const pace = new DragPace(false, "repo-builds");
    pace.wrote(0);
    pace.frame(25);
    pace.released(45);
    expect(startsAsGhost("repo-builds")).toBe(true);
    forgetPace();
  });

  test("a mark outlives a restart, and clearing it clears what is stored", () => {
    const held = new Map<string, string>();
    const kept = {
      get length() {
        return held.size;
      },
      clear: () => held.clear(),
      getItem: (key: string) => held.get(key) ?? null,
      key: (index: number) => [...held.keys()][index] ?? null,
      removeItem: (key: string) => void held.delete(key),
      setItem: (key: string, value: string) => void held.set(key, value),
    } as Storage;
    Object.defineProperty(window, "localStorage", { value: kept, configurable: true, writable: true });
    try {
      rereadPace();
      markSlow("docs-tree");
      expect(held.has(STORE_PREFIX + "docs-tree")).toBe(true);
      rereadPace();
      expect(startsAsGhost("docs-tree"), "read back after a restart").toBe(true);
      expect(startsAsGhost("builds")).toBe(false);
      landedIn("docs-tree", 12);
      landedIn("docs-tree", 12);
      expect(held.has(STORE_PREFIX + "docs-tree")).toBe(false);
      rereadPace();
      expect(startsAsGhost("docs-tree")).toBe(false);
    } finally {
      forgetPace();
      Object.defineProperty(window, "localStorage", { value: undefined, configurable: true, writable: true });
    }
  });

  test("a storage that throws keeps the mark for the session", () => {
    const refusing = {
      get length(): number {
        throw new Error("blocked");
      },
      key: () => {
        throw new Error("blocked");
      },
      getItem: () => null,
      setItem: () => {
        throw new Error("blocked");
      },
      removeItem: () => {
        throw new Error("blocked");
      },
      clear: () => {},
    } as unknown as Storage;
    Object.defineProperty(window, "localStorage", { value: refusing, configurable: true, writable: true });
    try {
      rereadPace();
      markSlow("compare");
      expect(startsAsGhost("compare")).toBe(true);
      landedIn("compare", 10);
      landedIn("compare", 10);
      expect(startsAsGhost("compare")).toBe(false);
    } finally {
      forgetPace();
      Object.defineProperty(window, "localStorage", { value: undefined, configurable: true, writable: true });
    }
  });

  test("a frame that wrote nothing is not costed", () => {
    const pace = new DragPace(false);
    pace.wrote(0);
    pace.frame(10);
    pace.frame(200);
    pace.frame(400);
    expect(pace.ghost).toBe(false);
  });

  test("a mark clears after two fast releases in a row; a slow one starts the count again", () => {
    markSlow("docs-tree");
    landedIn("docs-tree", 12);
    landedIn("docs-tree", 40);
    landedIn("docs-tree", 12);
    expect(startsAsGhost("docs-tree")).toBe(true);
    landedIn("docs-tree", 19);
    expect(startsAsGhost("docs-tree")).toBe(false);
    forgetPace();
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

  /** Each sidebar's grid, by the selector of its rules. */
  const GRIDS: Record<SideId, string> = {
    "tickets-views": ".tf-tickets",
    "ticket-card": ".tf-ticketpage .tf-ws",
    builds: ".tf-bv",
    "repo-tree": ".tf-repo",
    "repo-builds": ".tf-rbv",
    compare: ".tf-compare",
    "diff-files": ".tf-dgrid",
    "docs-tree": ".tf-rd",
    "docs-outline": ".tf-rd-sheet",
    settings: ".tf-stg",
    pr: ".tf-cbody.tf-pr",
    set: ".tf-cbody.tf-set",
    assets: ".tree",
    monitor: ".tf-cbody.tf-mon-body",
    "oas-rail": ".tf-oas",
    "doc-rail": ".tf-oas.tf-xv",
    "xsd-rail": ".tf-oas.tf-xsd",
  };
  /** Every rule's selector, its declarations and the at-rules around it, from CSS (a component's `<style>` alone). */
  function rules(text: string, component: boolean): { selector: string; body: string; within: string[] }[] {
    const css = component ? (text.match(/<style[^>]*>([\s\S]*?)<\/style>/)?.[1] ?? "") : text;
    const clean = css.replace(/\/\*[\s\S]*?\*\//g, "");
    const out: { selector: string; body: string; within: string[] }[] = [];
    const stack: string[] = [];
    let start = 0;
    for (let i = 0; i < clean.length; i++) {
      const ch = clean[i];
      if (ch === "{") {
        const prelude = clean.slice(start, i).trim();
        const close = clean.indexOf("}", i);
        const next = clean.indexOf("{", i + 1);
        if (!prelude.startsWith("@") && (next === -1 || close < next)) {
          out.push({ selector: prelude, body: clean.slice(i + 1, close), within: [...stack] });
          i = close;
        } else stack.push(prelude);
        start = i + 1;
      } else if (ch === "}") {
        stack.pop();
        start = i + 1;
      } else if (ch === ";" && stack.length === 0) start = i + 1;
    }
    return out;
  }
  const styled = sources.filter(({ path }) => path.endsWith(".css") || path.endsWith("AssetsView.svelte")).flatMap(({ path, text }) => rules(text, path.endsWith(".svelte")));

  test.each(SIDE_IDS)("%s: its grid's own rule reads the width, and no rule above 900 px holds a fixed sidebar track", (id) => {
    const mine = styled.filter((rule) => rule.selector === GRIDS[id] && /grid-template-columns:/.test(rule.body));
    expect(mine.length, `no rule of ${GRIDS[id]} sets its columns`).toBeGreaterThan(0);
    const wide = mine.filter((rule) => !rule.within.some((at) => /max-width:\s*(8\d\d|9[0-0]\d)px/.test(at)));
    expect(wide.some((rule) => rule.body.includes(`var(--side-${id}, `))).toBe(true);
    for (const rule of wide) {
      const columns = rule.body.match(/grid-template-columns:\s*([^;]+)/)![1]!.replace(/var\(--side-[\w-]+, \d+px\)/g, "SIDE");
      expect(columns, `${GRIDS[id]} { grid-template-columns: ${columns} }`).not.toMatch(/(^|\s)\d+px/);
    }
  });

  test("no sidebar is left out of the list", () => {
    const named = new Set(sources.flatMap(({ text }) => [...text.matchAll(/<TfSideEdge side="([\w-]+)"/g)].map((match) => match[1])));
    expect([...named].sort()).toEqual([...SIDE_IDS].sort());
    // Tickets' preview of the selected ticket went in ux18, and its edge with it.
    expect(Object.keys(SIDES)).toHaveLength(17);
    expect(Object.keys(SIDES)).not.toContain("tickets-preview");
  });
});
