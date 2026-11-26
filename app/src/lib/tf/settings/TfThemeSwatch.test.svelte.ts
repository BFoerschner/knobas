/**
 * A swatch's colour picker dragged (`TfThemeSwatch.svelte`; the themes decisions' risk 9): the system's picker sends an
 * `input` for every move, faster than the screen draws, and the swatch hands on one colour a frame, the last picked,
 * as a preview, and a drag's end at once, which colours the window. Each colour handed on re-reads the whole editor (the ux30 end gate, 2026-11-23).
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test, vi } from "vitest";

import TfThemeSwatch from "./TfThemeSwatch.svelte";

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target?.remove();
});

function open(onchange: (next: string | undefined) => void): HTMLInputElement {
  target = document.body.appendChild(document.createElement("div"));
  view = mount(TfThemeSwatch, { target, props: { label: "Accent", value: "#112233", fallback: "#000000", editable: true, onchange } });
  target.querySelector<HTMLButtonElement>('button[aria-label="Change Accent"]')!.click();
  flushSync();
  return target.querySelector<HTMLInputElement>('input[type="color"]')!;
}

const send = (input: HTMLInputElement, type: "input" | "change", value: string) => {
  input.value = value;
  input.dispatchEvent(new Event(type, { bubbles: true }));
};

const frame = () => new Promise((done) => requestAnimationFrame(() => done(true)));

test("a drag's picks are handed on once a frame, the last one picked, as a preview", async () => {
  const onchange = vi.fn();
  const input = open(onchange);
  for (const value of ["#aa0000", "#bb0000", "#cc0000", "#dd0000"]) send(input, "input", value);
  expect(onchange).not.toHaveBeenCalled();
  await frame();
  expect(onchange.mock.calls).toEqual([["#DD0000", true]]);
  send(input, "input", "#ee0000");
  send(input, "input", "#ff0000");
  await frame();
  expect(onchange.mock.calls).toEqual([["#DD0000", true], ["#FF0000", true]]);
});

test("a drag's end is handed on at once, not as a preview, and nothing after it", async () => {
  const onchange = vi.fn();
  const input = open(onchange);
  send(input, "input", "#aa0000");
  send(input, "change", "#ab0000");
  expect(onchange.mock.calls).toEqual([["#AB0000", false]]);
  await frame();
  expect(onchange.mock.calls).toEqual([["#AB0000", false]]);
});

test("WebKit's change in the task of its input is a move, handed on as a preview; a change in a later task ends the drag", async () => {
  const onchange = vi.fn();
  const input = open(onchange);
  // WebKit (the Mac's and Linux's windows) sends both for every pick, in one task, and nothing when its panel closes.
  for (const value of ["#aa0000", "#bb0000"]) {
    send(input, "input", value);
    send(input, "change", value);
  }
  expect(onchange).not.toHaveBeenCalled();
  await frame();
  expect(onchange.mock.calls).toEqual([["#BB0000", true]]);
  // Chromium's change comes as its picker closes, in a task of its own.
  send(input, "input", "#cc0000");
  await new Promise((done) => setTimeout(done));
  send(input, "change", "#cc0000");
  expect(onchange.mock.calls).toEqual([["#BB0000", true], ["#CC0000", false]]);
});

test("the panel's Hex field and the swatch show the moving colour", async () => {
  const input = open(() => {});
  send(input, "input", "#aa0000");
  await frame();
  flushSync();
  expect(target.querySelector<HTMLInputElement>('input[aria-label="Accent as hex"]')!.value).toBe("#AA0000");
  expect(target.querySelector<HTMLButtonElement>('button[aria-label="Change Accent"]')!.style.getPropertyValue("background")).toMatch(/170, 0, 0|#aa0000/i);
});

test("a pick waiting for its frame is dropped with the swatch", async () => {
  const onchange = vi.fn();
  const input = open(onchange);
  send(input, "input", "#aa0000");
  unmount(view!);
  view = null;
  await frame();
  expect(onchange).not.toHaveBeenCalled();
});

/** A pointer event at a point of the page: jsdom has no `PointerEvent`, and the picker reads only the position. */
const at = (el: Element, type: string, clientX: number, clientY: number) => el.dispatchEvent(new MouseEvent(type, { bubbles: true, clientX, clientY }));
const box = (el: Element, width: number, height: number) =>
  ((el as HTMLElement).getBoundingClientRect = () => ({ left: 0, top: 0, x: 0, y: 0, width, height, right: width, bottom: height, toJSON: () => ({}) }));

test("the panel's square picks saturation and brightness under the strip's hue: a drag hands on one colour a frame, its end at once (#2516)", async () => {
  const onchange = vi.fn();
  open(onchange);
  const square = target.querySelector<HTMLElement>('[role="slider"][aria-label="Accent: saturation and brightness"]')!;
  box(square, 200, 100);
  // #112233's hue is 210°: the square's top right corner is that hue at its fullest.
  at(square, "pointerdown", 200, 0);
  at(square, "pointermove", 150, 20);
  at(square, "pointermove", 100, 50);
  expect(onchange).not.toHaveBeenCalled();
  await frame();
  expect(onchange.mock.calls).toEqual([["#406080", true]]);
  at(square, "pointerup", 200, 0);
  expect(onchange.mock.calls).toEqual([["#406080", true], ["#0080FF", false]]);
  await frame();
  expect(onchange.mock.calls).toHaveLength(2);
});

test("the strip picks the hue, and the arrow keys step the strip and the square, each step handed on at once (#2516)", async () => {
  const onchange = vi.fn();
  open(onchange);
  const strip = target.querySelector<HTMLElement>('[role="slider"][aria-label="Accent: hue"]')!;
  expect(strip.getAttribute("aria-valuenow")).toBe("210");
  strip.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", shiftKey: true, bubbles: true }));
  flushSync();
  expect(strip.getAttribute("aria-valuenow")).toBe("220");
  expect(onchange.mock.calls).toEqual([["#111C33", false]]);
  box(strip, 360, 12);
  at(strip, "pointerdown", 0, 6);
  at(strip, "pointerup", 0, 6);
  flushSync();
  expect(onchange.mock.calls.at(-1)).toEqual(["#331111", false]);
  const square = target.querySelector<HTMLElement>('[role="slider"][aria-label="Accent: saturation and brightness"]')!;
  square.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowUp", shiftKey: true, bubbles: true }));
  flushSync();
  // Brightness from 20 % to 30 %, hue and saturation kept.
  expect(onchange.mock.calls.at(-1)).toEqual(["#4D1A1A", false]);
});
