/**
 * A list read again lands without moving what the reader is on (the third
 * smoothness pass of PR #2479): the first row on screen stays where it was,
 * and a reader at the top stays at the top.
 */
import { afterEach, expect, test, vi } from "vitest";

import { landInPlace } from "./land-in-place";

afterEach(() => vi.restoreAllMocks());

/** A scroller holding rows 30 px high, laid out by hand since jsdom lays nothing out. */
function rig(names: string[]) {
  const scroller = document.createElement("div");
  scroller.className = "tf-main";
  const list = document.createElement("div");
  scroller.appendChild(list);
  document.body.appendChild(scroller);
  const draw = (all: string[]) => {
    const kept = new Map([...list.children].map((el) => [el.textContent, el]));
    list.replaceChildren(
      ...all.map((name) => {
        const row = kept.get(name) ?? document.createElement("div");
        row.textContent = name;
        row.setAttribute("data-place", "");
        return row;
      }),
    );
  };
  draw(names);
  vi.spyOn(Element.prototype, "getClientRects").mockReturnValue([new DOMRect()] as unknown as DOMRectList);
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
    if (this === scroller) return new DOMRect(0, 0, 800, 90);
    const n = [...list.children].indexOf(this);
    return new DOMRect(0, n * 30 - scroller.scrollTop, 800, 30);
  });
  const row = (name: string) => [...list.children].find((el) => el.textContent === name)!;
  return { scroller, list, draw, row, done: () => scroller.remove() };
}

test("rows added above the reader's place move the scroller by as much, so the row they were on stays put", async () => {
  const { scroller, list, draw, row, done } = rig(["a", "b", "c", "d", "e"]);
  scroller.scrollTop = 65;
  const was = row("c").getBoundingClientRect().top;
  await landInPlace(list, () => draw(["new 1", "new 2", "a", "b", "c", "d", "e"]));
  expect(row("c").getBoundingClientRect().top).toBe(was);
  expect(scroller.scrollTop).toBe(125);
  done();
});

test("a reader at the top stays at the top and sees the new row come in", async () => {
  const { scroller, list, draw, done } = rig(["a", "b"]);
  await landInPlace(list, () => draw(["new", "a", "b"]));
  expect(scroller.scrollTop).toBe(0);
  done();
});

test("with nothing to hold on to, the read just lands", async () => {
  let landed = false;
  await landInPlace(undefined, () => (landed = true));
  expect(landed).toBe(true);
  const loose = document.createElement("div");
  await landInPlace(loose, () => (landed = false));
  expect(landed).toBe(false);
});
