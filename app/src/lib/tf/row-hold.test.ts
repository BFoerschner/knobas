import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { push, reset, undoable } from "../shell/toasts.svelte";
import { holdUnderPointer, withHolds } from "./row-hold";

const hold = (at: number, entry: string) => ({ at, entry });

test("each hold goes back at its index in what was drawn, whichever was pressed first (F-159)", () => {
  expect(withHolds(["d"], [hold(2, "c"), hold(0, "a"), hold(1, "b")])).toEqual(["a", "b", "c", "d"]);
  expect(withHolds(["a", "c"], [hold(1, "b"), hold(3, "d")])).toEqual(["a", "b", "c", "d"]);
});

test("a row a failed answer brings back takes its own place, and the holds around it keep theirs (F-159)", () => {
  // a b c d drawn, b and c held. b comes back to the list, so only c still holds.
  expect(withHolds(["a", "b", "d"], [hold(2, "c")])).toEqual(["a", "b", "c", "d"]);
  // c comes back instead.
  expect(withHolds(["a", "c", "d"], [hold(1, "b")])).toEqual(["a", "b", "c", "d"]);
});

test("nothing held is the list itself, and a hold past the end stands last", () => {
  const list = ["a"];
  expect(withHolds(list, [])).toBe(list);
  expect(withHolds(list, [hold(5, "b")])).toEqual(["a", "b"]);
});

describe("holdUnderPointer", () => {
  let box: HTMLElement;
  let released = 0;
  let stop: () => void = () => {};
  const at = (type: string, clientX: number, clientY: number) => window.dispatchEvent(new PointerEvent(type, { clientX, clientY }));
  const hold = (list: boolean) => {
    released = 0;
    stop = holdUnderPointer(() => box, () => (released += 1), list);
  };

  beforeEach(() => {
    vi.useFakeTimers();
    box = document.createElement("div");
    box.getBoundingClientRect = () => new DOMRect(100, 100, 800, 400);
    document.body.append(box);
  });
  afterEach(() => {
    stop();
    box.remove();
    reset();
    vi.useRealTimers();
  });

  test("a row's hold lets go as the pointer leaves the row, by any side, or presses elsewhere (Monitors, the drawer)", () => {
    hold(false);
    undoable("Hid it", () => {});
    at("pointermove", 500, 200);
    expect(released).toBe(0);
    at("pointermove", 950, 200);
    expect(released).toBe(1);
    at("pointerdown", 50, 50);
    expect(released).toBe(2);
  });

  test("a list's hold runs to the window's right edge, so the gutter is no way out of it (F-164)", () => {
    hold(true);
    at("pointermove", 1500, 200);
    expect(released).toBe(0);
    at("pointermove", 50, 200);
    expect(released).toBe(1);
  });

  test("a list's hold stands on the way to *Undo*, and lets go once the toast has gone, with no further move (F-163)", () => {
    hold(true);
    undoable("Done", () => {});
    at("pointermove", 500, 800);
    vi.advanceTimersByTime(6_000);
    expect(released, "the toast is still up").toBe(0);
    vi.advanceTimersByTime(1_000);
    expect(released).toBeGreaterThan(0);
  });

  test("a toast with no *Undo* gives nothing to reach for", () => {
    hold(true);
    push({ text: "Could not finish", tone: "err" });
    at("pointermove", 500, 800);
    expect(released).toBe(1);
  });

  test("back in the list when the toast goes, the hold stands", () => {
    hold(true);
    undoable("Done", () => {});
    at("pointermove", 500, 800);
    at("pointermove", 500, 200);
    vi.advanceTimersByTime(7_000);
    expect(released).toBe(0);
  });

  test("a press elsewhere closes the rows only once it has landed (F-163)", () => {
    hold(true);
    undoable("Done", () => {});
    at("pointermove", 500, 800);
    at("pointerdown", 500, 800);
    expect(released, "not under the press").toBe(0);
    at("pointerup", 500, 800);
    expect(released, "nor before its click").toBe(0);
    vi.advanceTimersByTime(0);
    expect(released).toBe(1);
  });

  test("a press in the list is no reason to close", () => {
    hold(true);
    at("pointerdown", 500, 200);
    at("pointerup", 500, 200);
    vi.advanceTimersByTime(1_000);
    expect(released).toBe(0);
  });

  test("a wheel or a key ends a list's hold whatever the toast says", () => {
    hold(true);
    undoable("Done", () => {});
    window.dispatchEvent(new WheelEvent("wheel"));
    expect(released).toBe(1);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "j" }));
    expect(released).toBe(2);
  });

  test("taken off, it listens to nothing", () => {
    hold(true);
    stop();
    at("pointermove", 50, 50);
    vi.advanceTimersByTime(1_000);
    expect(released).toBe(0);
  });
});
