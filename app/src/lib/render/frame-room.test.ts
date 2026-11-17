/**
 * A renderer's page in its frame (2026-11-17): the room it takes in its pane, and ⌘ or Ctrl letting the wheel through
 * to the window's zoom. That the page then scrolls by wheel, keys and scrollbar is the press trips' `frame-scroll`, in
 * a browser.
 */
import { afterEach, expect, test } from "vitest";

import { fitRoom, ROOM_FLOOR, ROOM_GAP, roomIn, ZOOM_THROUGH, zoomThrough } from "./frame-room";

const boxAt = (el: HTMLElement, top: number) => (el.getBoundingClientRect = () => ({ top }) as DOMRect);
const sized = (el: HTMLElement, clientHeight: number, scrollTop = 0) => {
  Object.defineProperty(el, "clientHeight", { configurable: true, value: clientHeight });
  el.scrollTop = scrollTop;
};

const made: HTMLElement[] = [];
afterEach(() => {
  for (const el of made.splice(0)) el.remove();
});

function paneWithFrame() {
  const pane = document.createElement("div");
  pane.style.overflowY = "auto";
  const frame = document.createElement("iframe");
  pane.append(frame);
  document.body.append(pane);
  made.push(pane);
  return { pane, frame };
}

test("the frame takes the room from its top to the pane's bottom, wherever the pane is scrolled", () => {
  const { pane, frame } = paneWithFrame();
  boxAt(pane, 49);
  sized(pane, 802);
  boxAt(frame, 263);
  // 49 + 802 is the pane's bottom edge; the frame starts 214 px into it.
  expect(roomIn(frame, pane)).toBe(49 + 802 - 263 - ROOM_GAP);
  // Scrolled by 150, the frame stands 150 px higher: the same room.
  sized(pane, 802, 150);
  boxAt(frame, 113);
  expect(roomIn(frame, pane)).toBe(49 + 802 - 263 - ROOM_GAP);
});

test("a short pane, or a frame that starts low, still gives the frame its floor", () => {
  const { pane, frame } = paneWithFrame();
  boxAt(pane, 49);
  sized(pane, 400);
  boxAt(frame, 300);
  expect(roomIn(frame, pane)).toBe(ROOM_FLOOR);
});

test("the attachment sets --tf-room on the frame before its first frame, from the nearest pane that scrolls", () => {
  const { pane, frame } = paneWithFrame();
  boxAt(pane, 0);
  sized(pane, 900);
  boxAt(frame, 200);
  const done = fitRoom()(frame);
  expect(frame.style.getPropertyValue("--tf-room")).toBe(`${900 - 200 - ROOM_GAP}px`);
  if (typeof done === "function") done();
});

const key = (type: "keydown" | "keyup", over: KeyboardEventInit) => window.dispatchEvent(new KeyboardEvent(type, { bubbles: true, ...over }));

test("⌘ or Ctrl held takes the pointer off the frame, so the wheel over it zooms the window; letting go gives it back", () => {
  const { frame } = paneWithFrame();
  const done = zoomThrough(frame)!;
  const through = () => frame.classList.contains(ZOOM_THROUGH);
  expect(through()).toBe(false);
  key("keydown", { key: "Control", ctrlKey: true });
  expect(through()).toBe(true);
  key("keyup", { key: "Control", ctrlKey: false });
  expect(through()).toBe(false);
  key("keydown", { key: "Meta", metaKey: true });
  expect(through()).toBe(true);
  // The window losing focus (to the frame, or another app) lets go: the key's release may never be heard.
  window.dispatchEvent(new Event("blur"));
  expect(through()).toBe(false);
  // Held as a wheel came (the key's press went to another window): the wheel tells.
  window.dispatchEvent(new WheelEvent("wheel", { deltaY: 100, ctrlKey: true }));
  expect(through()).toBe(true);
  // A plain wheel the app hears means the key was let go unheard: the next one goes to the frame again.
  window.dispatchEvent(new WheelEvent("wheel", { deltaY: 100 }));
  expect(through()).toBe(false);
  key("keydown", { key: "Control", ctrlKey: true });
  if (typeof done === "function") done();
  expect(through()).toBe(false);
});
