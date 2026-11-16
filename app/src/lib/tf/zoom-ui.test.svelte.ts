import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import TfPdf from "./render/TfPdf.svelte";
import TfStatusBar from "./TfStatusBar.svelte";
import TfZoomLevel from "./TfZoomLevel.svelte";
import TfZoomSetting from "./settings/TfZoomSetting.svelte";
import { checkScale } from "../shell/device-scale.svelte";
import { createZoom, type Zoom } from "./zoom.svelte";

let target: HTMLElement;
let app: Record<string, unknown> | null = null;

beforeEach(() => {
  target = document.body.appendChild(document.createElement("div"));
});

afterEach(() => {
  if (app) unmount(app);
  app = null;
  target.remove();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

/** A zoom of its own, its frames at once, and the levels it set on the "webview". */
function ownZoom(): Zoom & { applied: number[] } {
  const applied: number[] = [];
  const zoom = createZoom({ storage: () => null, apply: (level) => void applied.push(level), frame: (draw) => draw() });
  return Object.assign(zoom, { applied });
}

test("the plate shows the level for about a second after each step, an end's included, then goes", () => {
  vi.useFakeTimers();
  const zoom = ownZoom();
  app = mount(TfZoomLevel, { target, props: { zoom, hold: 1200 } });
  const plate = () => target.querySelector(".tf-zoomlevel")!;
  flushSync();
  // Nothing at rest: the window's first draw shows no plate.
  expect(plate().textContent).toBe("");
  expect(plate().getAttribute("role")).toBe("status");
  zoom.step(1);
  flushSync();
  expect(plate().querySelector("b")?.textContent).toBe("110 %");
  expect(plate().textContent).toContain("for 100 %");
  vi.advanceTimersByTime(1000);
  // Another step starts its second again.
  zoom.step(1);
  flushSync();
  vi.advanceTimersByTime(1000);
  flushSync();
  expect(plate().querySelector("b")?.textContent).toBe("125 %");
  vi.advanceTimersByTime(200);
  flushSync();
  expect(plate().textContent).toBe("");
  // At 100 % it says the level and no way back to it.
  zoom.reset();
  flushSync();
  expect(plate().querySelector("b")?.textContent).toBe("100 %");
  expect(plate().textContent).not.toContain("for 100 %");
  // At the top a step changes nothing and still says where it is.
  zoom.set(2);
  vi.advanceTimersByTime(2000);
  flushSync();
  zoom.step(1);
  flushSync();
  expect(plate().querySelector("b")?.textContent).toBe("200 %");
});

test("Settings › Appearance › Zoom is the eight steps, the one in use pressed, and a press sets it", async () => {
  const zoom = ownZoom();
  app = mount(TfZoomSetting, { target, props: { zoom } });
  flushSync();
  const buttons = [...target.querySelectorAll<HTMLButtonElement>(".tf-seg button")];
  expect(buttons.map((button) => button.textContent)).toEqual(["80 %", "90 %", "100 %", "110 %", "125 %", "150 %", "175 %", "200 %"]);
  const pressed = () => buttons.filter((button) => button.getAttribute("aria-pressed") === "true").map((button) => button.textContent);
  expect(pressed()).toEqual(["100 %"]);
  buttons[5]!.click();
  flushSync();
  await Promise.resolve();
  await Promise.resolve();
  expect(zoom.level).toBe(1.5);
  expect(zoom.applied).toEqual([1.5]);
  expect(pressed()).toEqual(["150 %"]);
  // A key's step elsewhere shows here too.
  zoom.step(-1);
  flushSync();
  expect(pressed()).toEqual(["125 %"]);
  expect(target.querySelector(".tf-stg-l")?.textContent).toContain("Kept on this computer");
});

test("the status bar ends in the level while it is not 100 %, and a press on it goes back to 100 %", () => {
  const zoom = ownZoom();
  app = mount(TfStatusBar, { target, props: { readings: [], pending: 0, decisions: 0, onqueue: () => {}, zoom } });
  flushSync();
  expect(target.querySelector(".tf-zoomed")).toBeNull();
  zoom.step(1);
  flushSync();
  const item = target.querySelector<HTMLButtonElement>(".tf-status .tf-zoomed")!;
  expect(item.textContent).toBe("Zoom 110 %");
  expect(item.title).toContain("100 %");
  item.click();
  flushSync();
  expect(zoom.level).toBe(1);
  expect(target.querySelector(".tf-zoomed")).toBeNull();
});

/** A one-page PDF whose page records the scale each draw was at. */
function onePage(draws: number[]) {
  const page = {
    width: 816,
    height: 1056,
    draw: (_canvas: HTMLCanvasElement, scale: number) => {
      draws.push(scale);
      return { done: Promise.resolve(), cancel() {} };
    },
    text: () => Promise.resolve(),
    links: () => Promise.resolve([]),
    lines: () => Promise.resolve([]),
    print: () => Promise.resolve({ key: "page", unsure: false }),
    pixels: () => Promise.resolve(null),
  };
  return {
    highlight: () => ({ focus() {}, cancel() {} }),
    pdf: () => Promise.resolve({ pdf: { pages: 1, page: () => Promise.resolve(page), close() {} } }),
  };
}

async function settle() {
  for (let turn = 0; turn < 6; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

test("⌘ and the wheel over a PDF step the PDF's own zoom (its knobas-zoom event), and a window zoom draws its pages again", async () => {
  vi.stubGlobal("IntersectionObserver", undefined);
  const draws: number[] = [];
  app = mount(TfPdf, {
    target,
    props: { read: () => Promise.resolve(new Uint8Array([37, 80, 68, 70])), size: 4, render: onePage(draws) as never },
  });
  await settle();
  const pane = target.querySelector<HTMLElement>("[data-own-zoom]")!;
  expect(pane).not.toBeNull();
  expect(target.querySelector(".tf-pdf-zoom")?.textContent).toBe("100%");
  pane.dispatchEvent(new CustomEvent("knobas-zoom", { detail: 1 }));
  await settle();
  expect(target.querySelector(".tf-pdf-zoom")?.textContent).not.toBe("100%");
  const zoomed = target.querySelector(".tf-pdf-zoom")!.textContent;
  pane.dispatchEvent(new CustomEvent("knobas-zoom", { detail: -1 }));
  await settle();
  expect(target.querySelector(".tf-pdf-zoom")?.textContent).toBe("100%");
  expect(zoomed).not.toBe("100%");
  // The window zooms to 110 % on a 2x screen: the page is drawn again, at the same PDF zoom, for the new scale.
  const before = draws.length;
  vi.stubGlobal("devicePixelRatio", 2.2);
  checkScale();
  await settle();
  expect(draws.length).toBe(before + 1);
  expect(draws.at(-1)).toBe(1);
});
