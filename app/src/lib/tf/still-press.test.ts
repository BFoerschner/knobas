/**
 * What ends `still-press.ts`'s hold. A person's press can't be sent from here (`isTrusted`), so the trips press it with real
 * keys (`aimed-after-close`), and this says which keys let go of it.
 */
import { expect, test, vi } from "vitest";

import { holdStillPress } from "./still-press";

function holding(key: string): boolean {
  const off = vi.spyOn(window, "removeEventListener");
  holdStillPress();
  window.dispatchEvent(new KeyboardEvent("keydown", { key }));
  const held = !(off.mock.calls as unknown[][]).some(([type]) => type === "click");
  // Let go, so one test's hold doesn't outlive it.
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab" }));
  off.mockRestore();
  return held;
}

test("a second press holds, and a key that aims lets go", () => {
  expect(holding("Enter")).toBe(true);
  expect(holding(" ")).toBe(true);
  expect(holding("Shift")).toBe(true);
  expect(holding("Tab")).toBe(false);
  expect(holding("ArrowDown")).toBe(false);
  expect(holding("j")).toBe(false);
});
