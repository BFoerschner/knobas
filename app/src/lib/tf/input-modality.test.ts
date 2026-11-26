/** `input-modality`: `<html data-input>` and `<html data-focus-ring>` drive the transient keyboard focus ring. */
import { afterEach, expect, test, vi } from "vitest";

import { FOCUS_FADE_MS, FOCUS_HOLD_MS, handOfKey, installInputModality } from "./input-modality";

let off: () => void = () => {};
const realMatchMedia = window.matchMedia;
afterEach(() => {
  off();
  off = () => {};
  vi.useRealTimers();
  Object.defineProperty(window, "matchMedia", { configurable: true, writable: true, value: realMatchMedia });
});

function setReducedMotion(reduce: boolean): void {
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    writable: true,
    value: (query: string) => ({
      matches: reduce && query === "(prefers-reduced-motion: reduce)",
      media: query,
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    }),
  });
}

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

test("uninstalling stops the listening and clears the flags and timers", () => {
  vi.useFakeTimers();
  off = installInputModality();
  key("Tab");
  expect(document.documentElement.dataset.focusRing).toBe("on");
  off();
  off = () => {};
  expect(document.documentElement.dataset.input).toBeUndefined();
  expect(document.documentElement.dataset.focusRing).toBeUndefined();
  vi.advanceTimersByTime(FOCUS_HOLD_MS + FOCUS_FADE_MS + 100);
  key("Tab");
  press();
  expect(document.documentElement.dataset.input).toBeUndefined();
  expect(document.documentElement.dataset.focusRing).toBeUndefined();
});

test("keyboard focus holds solid for 1.2s, fades over 0.3s, then clears (~1.5s total)", () => {
  vi.useFakeTimers();
  setReducedMotion(false);
  off = installInputModality();
  const root = document.documentElement;
  const button = document.createElement("button");
  document.body.append(button);
  try {
    expect(FOCUS_HOLD_MS).toBe(1200);
    expect(FOCUS_FADE_MS).toBe(300);

    key("Tab");
    button.focus();
    expect(root.dataset.focusRing).toBe("on");

    vi.advanceTimersByTime(1199);
    expect(root.dataset.focusRing).toBe("on");

    vi.advanceTimersByTime(1);
    expect(root.dataset.focusRing).toBe("fade");

    vi.advanceTimersByTime(299);
    expect(root.dataset.focusRing).toBe("fade");

    vi.advanceTimersByTime(1);
    expect(root.dataset.focusRing).toBeUndefined();
  } finally {
    button.remove();
  }
});

test("programmatic focus on initial load or after a keyboard shortcut starts the transient ring", () => {
  vi.useFakeTimers();
  setReducedMotion(false);
  off = installInputModality();
  const root = document.documentElement;
  const input = document.createElement("input");
  const row = document.createElement("div");
  row.tabIndex = -1;
  document.body.append(input, row);
  try {
    // Programmatic focus before any pointer event (e.g. initial landing focus).
    row.focus();
    expect(root.dataset.focusRing).toBe("on");
    vi.advanceTimersByTime(FOCUS_HOLD_MS + FOCUS_FADE_MS);
    expect(root.dataset.focusRing).toBeUndefined();

    // Shortcut (e.g. T or Mod+F) followed by programmatic input.focus().
    press();
    key("f");
    input.focus();
    expect(root.dataset.focusRing).toBe("on");
    vi.advanceTimersByTime(FOCUS_HOLD_MS);
    expect(root.dataset.focusRing).toBe("fade");
    vi.advanceTimersByTime(FOCUS_FADE_MS);
    expect(root.dataset.focusRing).toBeUndefined();
  } finally {
    input.remove();
    row.remove();
  }
});

test("moving focus to another element or pressing Tab restarts the hold timer immediately (even mid-fade)", () => {
  vi.useFakeTimers();
  setReducedMotion(false);
  off = installInputModality();
  const root = document.documentElement;
  const first = document.createElement("button");
  const second = document.createElement("button");
  document.body.append(first, second);
  try {
    key("Tab");
    first.focus();
    vi.advanceTimersByTime(FOCUS_HOLD_MS + 150);
    expect(root.dataset.focusRing).toBe("fade");

    // Moving focus to another element snaps immediately back to solid for a full hold.
    second.focus();
    expect(root.dataset.focusRing).toBe("on");
    vi.advanceTimersByTime(FOCUS_HOLD_MS - 1);
    expect(root.dataset.focusRing).toBe("on");

    // Pressing Tab again while still in hold restarts the full hold duration.
    key("Tab");
    vi.advanceTimersByTime(FOCUS_HOLD_MS - 1);
    expect(root.dataset.focusRing).toBe("on");
    vi.advanceTimersByTime(1);
    expect(root.dataset.focusRing).toBe("fade");

    // Pressing Tab after expiry restarts the ring from hidden.
    vi.advanceTimersByTime(FOCUS_FADE_MS);
    expect(root.dataset.focusRing).toBeUndefined();
    key("Tab");
    expect(root.dataset.focusRing).toBe("on");
  } finally {
    first.remove();
    second.remove();
  }
});

test("pointer clicks never trigger the ring and immediately clear an active ring", () => {
  vi.useFakeTimers();
  setReducedMotion(false);
  off = installInputModality();
  const root = document.documentElement;
  const input = document.createElement("input");
  const button = document.createElement("button");
  document.body.append(input, button);
  try {
    // Active keyboard ring clears immediately on pointer press.
    key("Tab");
    button.focus();
    expect(root.dataset.focusRing).toBe("on");

    press(input);
    input.focus();
    expect(root.dataset.input).toBe("pointer");
    expect(root.dataset.focusRing).toBeUndefined();

    // Typing inside the clicked field does not re-trigger the blue ring.
    key("a", input);
    key("b", input);
    expect(root.dataset.focusRing).toBeUndefined();

    // Clicking another control stays silent too.
    press(button);
    button.focus();
    expect(root.dataset.focusRing).toBeUndefined();
  } finally {
    input.remove();
    button.remove();
  }
});

test("under prefers-reduced-motion: reduce, the ring holds solid then hides cleanly without fade", () => {
  vi.useFakeTimers();
  setReducedMotion(true);
  off = installInputModality();
  const root = document.documentElement;
  const button = document.createElement("button");
  document.body.append(button);
  try {
    key("Tab");
    button.focus();
    expect(root.dataset.focusRing).toBe("on");

    vi.advanceTimersByTime(FOCUS_HOLD_MS - 1);
    expect(root.dataset.focusRing).toBe("on");

    vi.advanceTimersByTime(1);
    expect(root.dataset.focusRing).toBeUndefined();
  } finally {
    button.remove();
  }
});

test("blurring to nothing clears the ring immediately", () => {
  vi.useFakeTimers();
  setReducedMotion(false);
  off = installInputModality();
  const root = document.documentElement;
  const button = document.createElement("button");
  document.body.append(button);
  try {
    key("Tab");
    button.focus();
    expect(root.dataset.focusRing).toBe("on");

    button.blur();
    expect(root.dataset.focusRing).toBeUndefined();
  } finally {
    button.remove();
  }
});
