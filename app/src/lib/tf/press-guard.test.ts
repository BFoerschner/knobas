/**
 * A second press lands only where it was aimed (`press-guard.ts`). Events here carry `trusted` by the test's say-so, since a
 * test can't send a trusted event; the real browser presses are `trips.mjs`'s.
 */
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { BY_KEY_MS, BY_POINTER_MS, installPressGuard } from "./press-guard";

let off: () => void;
let pressed: string[];

function button(name: string): HTMLButtonElement {
  const el = document.createElement("button");
  el.textContent = name;
  el.addEventListener("click", () => pressed.push(name));
  document.body.append(el);
  return el;
}

/** An `Enter` or `Space` as the browser sends it: the key, then, unless something cancelled it, the click on what has focus. */
function key(el: Element, name = "Enter", init: KeyboardEventInit = {}): void {
  const down = new KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true, ...init });
  el.dispatchEvent(down);
  if (!down.defaultPrevented) el.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, detail: 0 }));
}

/** A whole pointer press at (`x`, 10). */
function pointer(el: Element, x = 10): void {
  const at = { bubbles: true, cancelable: true, clientX: x, clientY: 10, button: 0 };
  el.dispatchEvent(new MouseEvent("pointerdown", at));
  el.dispatchEvent(new MouseEvent("mousedown", at));
  el.dispatchEvent(new MouseEvent("mouseup", at));
  el.dispatchEvent(new MouseEvent("click", { ...at, detail: 1 }));
}

function moveTo(x: number): void {
  window.dispatchEvent(new MouseEvent("pointermove", { clientX: x, clientY: 10 }));
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(1_000_000);
  pressed = [];
  off = installPressGuard(() => true);
});

afterEach(() => {
  off();
  document.body.replaceChildren();
  vi.useRealTimers();
});

test("a second Enter on the control focus moved to does nothing, and walks once the wait is over (F-156)", () => {
  const [one, two] = [button("one"), button("two")];
  key(one);
  key(two);
  expect(pressed).toEqual(["one"]);
  vi.advanceTimersByTime(BY_KEY_MS);
  key(two);
  expect(pressed).toEqual(["one", "two"]);
});

test("the same control pressed twice by key is untouched", () => {
  const one = button("one");
  key(one);
  key(one);
  expect(pressed).toEqual(["one", "one"]);
});

test("mashing the key starts the wait over, so it finishes one row", () => {
  const [one, two] = [button("one"), button("two")];
  key(one);
  for (let press = 0; press < 3; press += 1) {
    vi.advanceTimersByTime(BY_KEY_MS - 100);
    key(two);
  }
  expect(pressed).toEqual(["one"]);
});

test.each(["Tab", "ArrowDown", "j"])("%s ends the wait", (aimed) => {
  const [one, two] = [button("one"), button("two")];
  key(one);
  one.dispatchEvent(new KeyboardEvent("keydown", { key: aimed, bubbles: true }));
  key(two);
  expect(pressed).toEqual(["one", "two"]);
});

test("Shift alone doesn't end the wait, and a modified Enter is sent on purpose", () => {
  const [one, two] = [button("one"), button("two")];
  key(one);
  one.dispatchEvent(new KeyboardEvent("keydown", { key: "Shift", bubbles: true }));
  key(two);
  expect(pressed).toEqual(["one"]);
  key(two, "Enter", { metaKey: true });
  expect(pressed).toEqual(["one", "two"]);
});

test("Space in a field is typed, and Space on a button is a press", () => {
  const [one, two] = [button("one"), button("two")];
  const field = document.createElement("input");
  document.body.append(field);
  key(one);
  const typed = new KeyboardEvent("keydown", { key: " ", bubbles: true, cancelable: true });
  field.dispatchEvent(typed);
  expect(typed.defaultPrevented).toBe(false);
  key(one);
  key(two, " ");
  expect(pressed).toEqual(["one", "one"]);
});

test("Enter in a field presses on as the same control, and what it moved focus to waits", () => {
  const field = document.createElement("input");
  document.body.append(field);
  const two = button("two");
  const first = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true });
  field.dispatchEvent(first);
  const second = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true });
  field.dispatchEvent(second);
  expect([first.defaultPrevented, second.defaultPrevented]).toEqual([false, false]);
  key(two);
  expect(pressed).toEqual([]);
});

test("a form's own click on its submit button from Enter in a field doesn't change which control was pressed", () => {
  const field = document.createElement("input");
  document.body.append(field);
  const submit = button("submit");
  field.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
  submit.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, detail: 0 }));
  expect(pressed).toEqual(["submit"]);
  const again = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true });
  field.dispatchEvent(again);
  expect(again.defaultPrevented).toBe(false);
});

test("a held key's repeat presses no button, and still types in a field", () => {
  const one = button("one");
  const field = document.createElement("textarea");
  document.body.append(field);
  key(one);
  key(one, "Enter", { repeat: true });
  expect(pressed).toEqual(["one"]);
  const line = new KeyboardEvent("keydown", { key: "Enter", repeat: true, bubbles: true, cancelable: true });
  field.dispatchEvent(line);
  expect(line.defaultPrevented).toBe(false);
});

test("a double click whose second press lands on what moved under a still pointer does nothing (F-358)", () => {
  const [one, log] = [button("Add to PAY-231"), button("Log…")];
  pointer(one);
  vi.advanceTimersByTime(BY_POINTER_MS - 100);
  pointer(log);
  expect(pressed).toEqual(["Add to PAY-231"]);
});

test("a swallowed press takes the mouse's press with it, so focus doesn't move and no dblclick answers", () => {
  const [one, two] = [button("one"), button("two")];
  pointer(one);
  const down = new MouseEvent("pointerdown", { bubbles: true, cancelable: true, button: 0, clientX: 10, clientY: 10 });
  two.dispatchEvent(down);
  const mouse = new MouseEvent("mousedown", { bubbles: true, cancelable: true, button: 0 });
  two.dispatchEvent(mouse);
  expect(mouse.defaultPrevented).toBe(true);
  const heard = vi.fn();
  two.addEventListener("dblclick", heard);
  two.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, cancelable: true }));
  expect(heard).not.toHaveBeenCalled();
});

test("a press on nothing under a still pointer is stopped too, so a backdrop doesn't close what just opened", () => {
  const one = button("one");
  const backdrop = document.createElement("div");
  const closed = vi.fn();
  backdrop.addEventListener("mousedown", closed);
  document.body.append(backdrop);
  pointer(one);
  pointer(backdrop);
  expect(closed).not.toHaveBeenCalled();
});

test("the same control under a still pointer is untouched, a child of it included", () => {
  const one = button("one");
  const inner = document.createElement("span");
  one.append(inner);
  pointer(one);
  pointer(inner);
  expect(pressed).toEqual(["one", "one"]);
});

test("a label and the field it presses are one press", () => {
  const label = document.createElement("label");
  const box = document.createElement("input");
  box.type = "checkbox";
  const picked = vi.fn();
  box.addEventListener("click", picked);
  label.append("Label", box);
  document.body.append(label);
  // The label's click reaches the box, and pressing the box itself next is the same control.
  pointer(label);
  pointer(box);
  expect(picked).toHaveBeenCalledTimes(2);
});

test("a pointer moved past a hand's tremble ends the wait, and a tremble doesn't", () => {
  const [one, two] = [button("one"), button("two")];
  pointer(one, 10);
  moveTo(13);
  pointer(two, 13);
  expect(pressed).toEqual(["one"]);
  moveTo(40);
  pointer(two, 40);
  expect(pressed).toEqual(["one", "two"]);
});

test("a wheel ends the wait, because the reader moved the page themselves", () => {
  const [one, two] = [button("one"), button("two")];
  pointer(one);
  window.dispatchEvent(new WheelEvent("wheel"));
  pointer(two);
  expect(pressed).toEqual(["one", "two"]);
});

test("after the wait a still pointer can press what it rests on", () => {
  const [one, two] = [button("one"), button("two")];
  pointer(one);
  vi.advanceTimersByTime(BY_POINTER_MS);
  pointer(two);
  expect(pressed).toEqual(["one", "two"]);
});

test("only a person's press: a script's click goes through", () => {
  off();
  off = installPressGuard();
  const [one, two] = [button("one"), button("two")];
  one.click();
  two.click();
  expect(pressed).toEqual(["one", "two"]);
});
