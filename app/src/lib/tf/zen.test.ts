import { afterEach, expect, test, vi } from "vitest";
import { ZEN_HIDES, ZEN_KEY, ZEN_KEY_LABEL, flipZen, isZenKey, pinIn, zen } from "./zen.svelte";
import { SHORTCUTS } from "./settings/sections";
import { readFileSync } from "node:fs";
import { join } from "node:path";

afterEach(() => {
  zen.on = false;
  zen.here = false;
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

test("its key is z alone, either case, and only when no one answered it", () => {
  const key = (init: KeyboardEventInit) => new KeyboardEvent("keydown", { cancelable: true, ...init });
  expect(isZenKey(key({ key: "z" }))).toBe(true);
  expect(isZenKey(key({ key: "Z", shiftKey: true }))).toBe(true);
  for (const extra of [{ metaKey: true }, { ctrlKey: true }, { altKey: true }]) expect(isZenKey(key({ key: "z", ...extra }))).toBe(false);
  expect(isZenKey(key({ key: "y" }))).toBe(false);
  const answered = key({ key: "z" });
  answered.preventDefault();
  expect(isZenKey(answered)).toBe(false);
});

test("it shows only where the reader asked and the page is one of Code's", () => {
  expect(zen.shown).toBe(false);
  zen.toggle();
  expect(zen.shown).toBe(false);
  zen.here = true;
  expect(zen.shown).toBe(true);
  zen.toggle();
  expect(zen.shown).toBe(false);
});

test("Settings › Keyboard names the key from the one place it is kept, for any page in Code", () => {
  expect(ZEN_KEY_LABEL).toBe(ZEN_KEY.toUpperCase());
  const row = SHORTCUTS.find(([, , what]) => what.startsWith("Zen mode"));
  expect(row?.[0]).toBe(ZEN_KEY_LABEL);
  expect(row?.[1]).toBe("Any page in Code");
});

test("the stylesheet hides what ZEN_HIDES names, under main's tf-zen, and nothing else", () => {
  const css = readFileSync(join(process.cwd(), "src/lib/tf/css/code.css"), "utf8");
  const hidden = [...css.matchAll(/\.tf-main\.tf-zen :is\(([^)]*)\) \{ display: none; \}/g)].flatMap((m) => m[1]!.split(",").map((x) => x.trim()));
  expect(hidden.sort()).toEqual(ZEN_HIDES.split(",").map((x) => x.trim()).sort());
});

/** A scroller with a header that Zen hides and rows under it; `hiddenBy` moves the rows up while Zen shows. */
function page(hiddenBy: number) {
  document.body.innerHTML = `<main class="tf-main" tabindex="-1"><div class="tf-full"><header class="tf-fhead"><button>Merge</button></header><div class="tf-cbody"><div class="row" id="r1">one</div><div class="row" id="r2">two</div></div></div></main>`;
  const main = document.querySelector<HTMLElement>("main")!;
  const header = document.querySelector<HTMLElement>("header")!;
  const r2 = document.getElementById("r2")!;
  main.scrollTop = 300;
  const rect = (top: number, height: number) => ({ top, bottom: top + height, left: 0, right: 1000, width: 1000, height, x: 0, y: top, toJSON: () => ({}) }) as DOMRect;
  vi.spyOn(main, "getBoundingClientRect").mockImplementation(() => rect(0, 800));
  // r2 stands at 40 px in the view; once the header goes it would stand `hiddenBy` higher, less what the scroll took back.
  vi.spyOn(r2, "getBoundingClientRect").mockImplementation(() => rect(40 - (zen.on ? hiddenBy : 0) + (300 - main.scrollTop), 22));
  vi.spyOn(header, "getBoundingClientRect").mockImplementation(() => rect(0, 30));
  (document as Document & { elementsFromPoint: (x: number, y: number) => Element[] }).elementsFromPoint = (_x, y) => (y < 30 ? [header, main] : y < 62 ? [r2, main] : [main]);
  return { main, header, r2 };
}

test("a flip keeps the topmost row the reader sees where it stood, the hidden header's height taken back by the scroll", () => {
  const { main, r2 } = page(64);
  zen.here = true;
  const pin = pinIn(main);
  // The header is what Zen hides: the row under it is the one held.
  expect(pin?.el).toBe(r2);
  const resized = vi.fn();
  window.addEventListener("resize", resized);
  flipZen(main, () => {});
  expect(zen.on).toBe(true);
  expect(main.scrollTop).toBe(300 - 64);
  expect(r2.getBoundingClientRect().top).toBe(40);
  // The panes fitted to the window fit again in the flip's frame.
  expect(resized).toHaveBeenCalledTimes(1);
  flipZen(main, () => {});
  expect(zen.on).toBe(false);
  expect(main.scrollTop).toBe(300);
  window.removeEventListener("resize", resized);
});

test("a focus inside what Zen hides goes to the scroller first, so the keys keep working", () => {
  const { main, header } = page(64);
  const merge = header.querySelector("button")!;
  merge.focus();
  expect(document.activeElement).toBe(merge);
  flipZen(main, () => {});
  expect(document.activeElement).toBe(main);
});

test("at the page's top nothing is pinned: the header Zen brings back stands in view", () => {
  const { main } = page(64);
  main.scrollTop = 0;
  flipZen(main, () => {});
  expect(main.scrollTop).toBe(0);
  flipZen(main, () => {});
  expect(main.scrollTop).toBe(0);
});
