/** `filterKeys`: `Esc` in a box that narrows or finds means one thing everywhere. */
import { afterEach, expect, test, vi } from "vitest";

import { filterKeys, installFindKey } from "./filter-keys";
import { layers } from "./layers.svelte";

afterEach(() => {
  document.body.replaceChildren();
});

function box(value: string, options?: Parameters<typeof filterKeys>[0]): { input: HTMLInputElement; off: () => void; heard: string[] } {
  const input = document.createElement("input");
  input.value = value;
  const heard: string[] = [];
  input.addEventListener("input", () => heard.push(input.value));
  document.body.append(input);
  input.focus();
  const off = filterKeys(options)(input) as () => void;
  return { input, off, heard };
}

function press(input: HTMLInputElement): { prevented: boolean; passed: boolean } {
  let passed = false;
  const hear = () => {
    passed = true;
  };
  window.addEventListener("keydown", hear);
  const event = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
  input.dispatchEvent(event);
  window.removeEventListener("keydown", hear);
  return { prevented: event.defaultPrevented, passed };
}

test("a filter with text clears through its own input event, and focus stays", () => {
  const { input, heard, off } = box("pay");
  expect(press(input)).toEqual({ prevented: true, passed: false });
  expect(input.value).toBe("");
  expect(heard).toEqual([""]);
  expect(document.activeElement).toBe(input);
  off();
});

test("an empty filter falls through, marked so the shell's ladder knows", () => {
  const { input, off } = box("");
  expect(input.hasAttribute("data-filter")).toBe(true);
  expect(press(input)).toEqual({ prevented: false, passed: true });
  off();
});

test("a find bar closes on the first press, with its words in it", () => {
  const close = vi.fn();
  const { input, off } = box("pay", { close });
  expect(press(input)).toEqual({ prevented: true, passed: false });
  expect(close).toHaveBeenCalledOnce();
  expect(input.value).toBe("pay");
  off();
});

test("the input's own handler can take the key first", () => {
  const input = document.createElement("input");
  input.value = "pay";
  document.body.append(input);
  input.addEventListener("keydown", (event) => event.preventDefault());
  const off = filterKeys()(input) as () => void;
  press(input);
  expect(input.value).toBe("pay");
  off();
});

/** A box as the browser draws it: jsdom lays nothing out, so a drawn box says so. */
function drawn(label: string, options?: Parameters<typeof filterKeys>[0], shown = true): { input: HTMLInputElement; off: () => void } {
  const input = document.createElement("input");
  input.setAttribute("aria-label", label);
  input.getClientRects = () => (shown ? [{}] : []) as unknown as DOMRectList;
  document.body.append(input);
  return { input, off: filterKeys(options)(input) as () => void };
}

function findKey(): KeyboardEvent {
  const event = new KeyboardEvent("keydown", { key: "f", metaKey: true, bubbles: true, cancelable: true });
  window.dispatchEvent(event);
  return event;
}

test("⌘F goes to the first box drawn, with its words selected, and pressed again selects them again", () => {
  const stop = installFindKey();
  const hidden = drawn("Hidden", {}, false);
  const { input, off } = drawn("Find in log");
  input.value = "retry";
  expect(findKey().defaultPrevented).toBe(true);
  expect(document.activeElement).toBe(input);
  expect([input.selectionStart, input.selectionEnd]).toEqual([0, 5]);
  input.setSelectionRange(5, 5);
  findKey();
  expect(document.activeElement).toBe(input);
  expect([input.selectionStart, input.selectionEnd]).toEqual([0, 5]);
  off();
  hidden.off();
  stop();
});

test("⌘F takes the page's own box over one beside the page, and one beside it where it is the only box", () => {
  const stop = installFindKey();
  const aside = drawn("Go to file", { aside: true });
  const own = drawn("Find in file");
  findKey();
  expect(document.activeElement).toBe(own.input);
  own.off();
  own.input.remove();
  findKey();
  expect(document.activeElement).toBe(aside.input);
  aside.off();
  stop();
});

test("⌘F leaves an event something took, and a dialog or a drawer owns the key", () => {
  const stop = installFindKey();
  const { input, off } = drawn("Filter");
  const take = (event: Event) => event.preventDefault();
  window.addEventListener("keydown", take, { capture: true });
  findKey();
  window.removeEventListener("keydown", take, { capture: true });
  expect(document.activeElement).not.toBe(input);
  const close = layers.add({ kind: "dialog", close: () => {} });
  findKey();
  expect(document.activeElement).not.toBe(input);
  close();
  const panel = layers.add({ kind: "panel", close: () => {} });
  findKey();
  expect(document.activeElement).toBe(input);
  panel();
  off();
  stop();
});

test("with no box, ⌘F asks the page, and a modified F is left alone", () => {
  const nobox = vi.fn();
  const stop = installFindKey(nobox);
  const event = findKey();
  expect(nobox).toHaveBeenCalledOnce();
  expect(event.defaultPrevented).toBe(false);
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "f", metaKey: true, shiftKey: true, bubbles: true }));
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "f", bubbles: true }));
  expect(nobox).toHaveBeenCalledOnce();
  stop();
});

test("Esc in a box ⌘F went to clears its words, then gives focus back, and only then falls through", () => {
  const stop = installFindKey();
  const button = document.createElement("button");
  document.body.append(button);
  button.focus();
  const { input, off } = drawn("Find in log");
  findKey();
  input.value = "retry";
  expect(press(input)).toEqual({ prevented: true, passed: false });
  expect(input.value).toBe("");
  expect(document.activeElement).toBe(input);
  expect(press(input)).toEqual({ prevented: true, passed: false });
  expect(document.activeElement).toBe(button);
  // Come to by hand afterwards, an empty box falls through as any other does.
  input.focus();
  expect(press(input)).toEqual({ prevented: false, passed: true });
  off();
  stop();
});
