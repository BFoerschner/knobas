/** `input-modality`: `<html data-input>` names the hand that moved last, so a select rings after a key and not a press. */
import { afterEach, expect, test } from "vitest";

import { handOfKey, installInputModality } from "./input-modality";

let off: () => void = () => {};
afterEach(() => {
  off();
  off = () => {};
});

const press = (target: EventTarget = document.body): void => {
  target.dispatchEvent(new Event("pointerdown", { bubbles: true }));
};
const key = (name: string, target: EventTarget = document.body): void => {
  target.dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true }));
};

test("nothing is set before the first press or key, so a select rings for the keyboard by default", () => {
  off = installInputModality();
  expect(document.documentElement.dataset.input).toBeUndefined();
});

test("a press says pointer, and a key after it says keys", () => {
  off = installInputModality();
  press();
  expect(document.documentElement.dataset.input).toBe("pointer");
  key("Tab");
  expect(document.documentElement.dataset.input).toBe("keys");
  press();
  expect(document.documentElement.dataset.input).toBe("pointer");
});

test("a bare modifier changes nothing: ⌘ held before a click is still the pointer's", () => {
  off = installInputModality();
  press();
  for (const name of ["Shift", "Control", "Alt", "Meta"]) key(name);
  expect(document.documentElement.dataset.input).toBe("pointer");
  key("ArrowDown");
  expect(document.documentElement.dataset.input).toBe("keys");
});

test("handOfKey: a modifier is no hand, every other key is the keys'", () => {
  expect(handOfKey("Meta")).toBeNull();
  expect(handOfKey("Shift")).toBeNull();
  expect(handOfKey("Enter")).toBe("keys");
  expect(handOfKey("a")).toBe("keys");
  expect(handOfKey(" ")).toBe("keys");
});

test("a handler that stops the event can't hide it: the listeners capture on window", () => {
  off = installInputModality();
  const button = document.createElement("button");
  document.body.append(button);
  const stop = (event: Event): void => event.stopPropagation();
  button.addEventListener("pointerdown", stop);
  button.addEventListener("keydown", stop);
  press(button);
  expect(document.documentElement.dataset.input).toBe("pointer");
  key("Tab", button);
  expect(document.documentElement.dataset.input).toBe("keys");
  button.remove();
});

test("a run of the same hand writes once", () => {
  off = installInputModality();
  const root = document.documentElement;
  press();
  // Every write of the attribute is a mutation record, a write of the same value too.
  const seen: string[] = [];
  const watch = new MutationObserver((records) => records.forEach((r) => seen.push(String(root.getAttribute(r.attributeName!)))));
  watch.observe(root, { attributes: true, attributeFilter: ["data-input"] });
  press();
  press();
  key("Tab");
  key("Tab");
  key("j");
  return Promise.resolve().then(() => {
    watch.disconnect();
    expect(seen).toEqual(["keys"]);
  });
});

test("uninstalling stops the listening and clears the flag", () => {
  off = installInputModality();
  press();
  off();
  off = () => {};
  expect(document.documentElement.dataset.input).toBeUndefined();
  key("Tab");
  press();
  expect(document.documentElement.dataset.input).toBeUndefined();
});
