/** `filterKeys`: `Esc` in a box that narrows or finds means one thing everywhere. */
import { afterEach, expect, test, vi } from "vitest";

import { filterKeys } from "./filter-keys";

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
