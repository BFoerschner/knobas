import { afterEach, describe, expect, test, vi } from "vitest";

import type { KeptStorage } from "./kept-value";
import { createZoom, installZoomInput, LEVELS, nearestLevel, percent, stepped, ZOOM_KEY, zoomKey, type Zoom } from "./zoom.svelte";

/** A `localStorage` of its own. */
function memory(seed: Record<string, string> = {}): KeptStorage & { kept: Map<string, string> } {
  const kept = new Map(Object.entries(seed));
  return {
    kept,
    getItem: (key) => kept.get(key) ?? null,
    setItem: (key, value) => void kept.set(key, value),
    removeItem: (key) => void kept.delete(key),
  };
}

/** A zoom whose frames run when the test says, and whose webview is a list of the levels set. */
function rig(seed: Record<string, string> = {}) {
  const storage = memory(seed);
  const applied: number[] = [];
  const frames: (() => void)[] = [];
  const zoom = createZoom({ storage: () => storage, apply: (level) => void applied.push(level), frame: (draw) => void frames.push(draw) });
  const frame = async () => {
    for (const draw of frames.splice(0)) draw();
    await Promise.resolve();
    await Promise.resolve();
  };
  return { zoom, storage, applied, frame };
}

const press = (key: string, more: Partial<KeyboardEvent> = {}) =>
  ({ key, code: "", metaKey: false, ctrlKey: true, altKey: false, defaultPrevented: false, ...more }) as KeyboardEvent;

afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
  document.body.innerHTML = "";
});

describe("the keys", () => {
  test("⌘+ and ⌘= and the keypad's + zoom in, ⌘− and the keypad's − out, ⌘0 goes back, with Ctrl or ⌘", () => {
    expect(zoomKey(press("+"))).toBe(1);
    expect(zoomKey(press("="))).toBe(1);
    expect(zoomKey(press("+", { code: "NumpadAdd" }))).toBe(1);
    expect(zoomKey(press("-"))).toBe(-1);
    expect(zoomKey(press("_"))).toBe(-1);
    expect(zoomKey(press("-", { code: "NumpadSubtract" }))).toBe(-1);
    expect(zoomKey(press("0"))).toBe(0);
    expect(zoomKey(press("Insert", { code: "Numpad0" }))).toBe(0);
    expect(zoomKey(press("=", { ctrlKey: false, metaKey: true }))).toBe(1);
  });

  test("without a modifier, with ⌥ (AltGr off the Mac), or answered by a control already, a key is not zoom", () => {
    expect(zoomKey(press("+", { ctrlKey: false }))).toBeNull();
    expect(zoomKey(press("+", { altKey: true }))).toBeNull();
    expect(zoomKey(press("0", { defaultPrevented: true }))).toBeNull();
    expect(zoomKey(press("k"))).toBeNull();
    expect(zoomKey(press("9"))).toBeNull();
  });
});

describe("the steps", () => {
  test("run 80 to 200 %, held at each end", () => {
    expect(LEVELS).toEqual([0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2]);
    expect(stepped(1, 1)).toBe(1.1);
    expect(stepped(1, -1)).toBe(0.9);
    expect(stepped(1.5, 2)).toBe(2);
    expect(stepped(2, 1)).toBe(2);
    expect(stepped(0.8, -1)).toBe(0.8);
    expect(stepped(1, -9)).toBe(0.8);
  });

  test("a level kept that is not a step reads as the nearest one, and nonsense as 100 %", () => {
    expect(nearestLevel(1.07)).toBe(1.1);
    expect(nearestLevel(5)).toBe(2);
    expect(nearestLevel(0.1)).toBe(0.8);
    expect(nearestLevel(Number.NaN)).toBe(1);
    expect(percent(1.25)).toBe("125 %");
  });
});

describe("the level", () => {
  test("is read from what this computer kept, and a step keeps the new one; 100 % keeps nothing", () => {
    const { zoom, storage } = rig({ [ZOOM_KEY]: "1.25" });
    expect(zoom.level).toBe(1.25);
    zoom.step(1);
    expect(zoom.level).toBe(1.5);
    expect(storage.kept.get(ZOOM_KEY)).toBe("1.5");
    zoom.reset();
    expect(zoom.level).toBe(1);
    expect(storage.kept.has(ZOOM_KEY)).toBe(false);
  });

  test("lands on the webview once a frame, at the newest level, however many steps the frame took", async () => {
    const { zoom, applied, frame } = rig();
    zoom.step(1);
    zoom.step(1);
    zoom.step(1);
    expect(applied).toEqual([]);
    await frame();
    expect(applied).toEqual([1.5]);
    // Back where the webview already is: nothing more to set.
    zoom.step(1);
    zoom.step(-1);
    await frame();
    expect(applied).toEqual([1.5]);
  });

  test("a step at an end changes nothing and still counts as asked, so the plate shows where it is", async () => {
    const { zoom, applied, frame } = rig({ [ZOOM_KEY]: "2" });
    const before = zoom.asked;
    zoom.step(1);
    await frame();
    expect(zoom.level).toBe(2);
    expect(zoom.asked).toBe(before + 1);
    expect(applied).toEqual([]);
  });

  test("boot sets the kept level before the first draw, and nothing at 100 %", async () => {
    const at100 = rig();
    await at100.zoom.boot();
    expect(at100.applied).toEqual([]);
    const zoomed = rig({ [ZOOM_KEY]: "1.1" });
    await zoomed.zoom.boot();
    expect(zoomed.applied).toEqual([1.1]);
  });

  test("boot waits a beat at most for a webview that does not answer", async () => {
    vi.useFakeTimers();
    const zoom = createZoom({ storage: () => memory({ [ZOOM_KEY]: "1.5" }), apply: () => new Promise(() => {}), frame: () => {} });
    let done = false;
    void zoom.boot().then(() => (done = true));
    await vi.advanceTimersByTimeAsync(249);
    expect(done).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    expect(done).toBe(true);
  });

  test("a webview that refuses leaves the level as asked, and the next frame tries again with the next", async () => {
    let calls = 0;
    const frames: (() => void)[] = [];
    const zoom = createZoom({
      storage: () => memory(),
      apply: () => {
        calls++;
        throw new Error("refused");
      },
      frame: (draw) => void frames.push(draw),
    });
    zoom.step(1);
    for (const draw of frames.splice(0)) draw();
    await Promise.resolve();
    await Promise.resolve();
    expect(zoom.level).toBe(1.1);
    zoom.step(1);
    for (const draw of frames.splice(0)) draw();
    await Promise.resolve();
    await Promise.resolve();
    expect(calls).toBe(2);
  });
});

describe("the wheel", () => {
  const wheel = (deltaY: number, timeStamp: number, deltaMode = 0) => ({ deltaY, deltaMode, timeStamp });

  test("a mouse's notch is one step, up is in, however far it says it goes", () => {
    const { zoom } = rig();
    expect(zoom.wheelSteps(wheel(-100, 0))).toBe(1);
    expect(zoom.wheelSteps(wheel(100, 10))).toBe(-1);
    expect(zoom.wheelSteps(wheel(-300, 20))).toBe(1);
    expect(zoom.wheelSteps(wheel(-3, 30, 1))).toBe(1);
  });

  test("a trackpad's small deltas add up to a step, start again after a pause, and again when they turn", () => {
    const { zoom } = rig();
    const steps = [];
    for (let i = 0; i < 9; i++) steps.push(zoom.wheelSteps(wheel(-6, i * 16)));
    expect(steps).toEqual([0, 0, 0, 0, 0, 0, 0, 0, 1]);
    // 48 px, then a pause: the next 6 px are a start, not a step.
    for (let i = 0; i < 8; i++) zoom.wheelSteps(wheel(-6, 1_000 + i * 16));
    expect(zoom.wheelSteps(wheel(-6, 2_000))).toBe(0);
    // 42 px in, then out: the out starts from nothing.
    for (let i = 0; i < 6; i++) zoom.wheelSteps(wheel(-6, 3_000 + i * 16));
    expect(zoom.wheelSteps(wheel(10, 3_200))).toBe(0);
  });

  test("a WebKit pinch walks a step each time its scale grows by an eighth more, back as it shrinks", () => {
    const { zoom } = rig();
    expect(zoom.pinchSteps(1, true)).toBe(0);
    expect(zoom.pinchSteps(1.1, false)).toBe(0);
    expect(zoom.pinchSteps(1.13, false)).toBe(1);
    expect(zoom.pinchSteps(1.4, false)).toBe(1);
    expect(zoom.pinchSteps(0.9, false)).toBe(-2);
  });
});

describe("the window's input", () => {
  function wired() {
    const steps: number[] = [];
    const zoom = { step: (n: number) => void steps.push(n), wheelSteps: (event: WheelEvent) => (event.deltaY < 0 ? 1 : -1), pinchSteps: (scale: number) => (scale > 1 ? 1 : 0) } as unknown as Zoom;
    const cancelling: unknown[] = [];
    const add = vi.spyOn(window, "addEventListener");
    const stop = installZoomInput(zoom);
    const blocking = () => add.mock.calls.filter(([type, , options]) => String(type) === "wheel" && typeof options === "object" && options.passive === false).length;
    return { steps, stop, blocking, cancelling };
  }
  const wheelAt = (target: EventTarget, init: WheelEventInit) => {
    const event = new WheelEvent("wheel", { bubbles: true, cancelable: true, deltaY: -100, ...init });
    target.dispatchEvent(event);
    return event;
  };

  test("listens to the wheel passively until ⌘ or Ctrl is down: a plain scroll meets no listener that may cancel", () => {
    const { steps, stop, blocking } = wired();
    expect(blocking()).toBe(0);
    const plain = wheelAt(document.body, {});
    expect(plain.defaultPrevented).toBe(false);
    expect(steps).toEqual([]);
    stop();
  });

  test("with Ctrl held the wheel zooms and scrolls nothing; the key's release takes the cancelling listener away", () => {
    const { steps, stop, blocking } = wired();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Control", ctrlKey: true }));
    expect(blocking()).toBe(1);
    const zoomed = wheelAt(document.body, { ctrlKey: true });
    expect(zoomed.defaultPrevented).toBe(true);
    expect(steps).toEqual([1]);
    const remove = vi.spyOn(window, "removeEventListener");
    window.dispatchEvent(new KeyboardEvent("keyup", { key: "Control" }));
    expect(remove.mock.calls.some(([type]) => String(type) === "wheel")).toBe(true);
    const plain = wheelAt(document.body, {});
    expect(plain.defaultPrevented).toBe(false);
    expect(steps).toEqual([1]);
    stop();
  });

  test("a pinch (a wheel with Ctrl and no key down) zooms from the passive listener, and the next plain wheel lets go", () => {
    const { steps, stop, blocking } = wired();
    wheelAt(document.body, { ctrlKey: true, deltaY: -6 });
    expect(steps).toEqual([1]);
    // The rest of the pinch is cancelled, so no scroll comes with it.
    expect(blocking()).toBe(1);
    expect(wheelAt(document.body, { ctrlKey: true, deltaY: -6 }).defaultPrevented).toBe(true);
    expect(steps).toEqual([1, 1]);
    const remove = vi.spyOn(window, "removeEventListener");
    wheelAt(document.body, { deltaY: 40 });
    expect(remove.mock.calls.some(([type]) => String(type) === "wheel")).toBe(true);
    stop();
  });

  test("the window losing focus lets go of the cancelling listener, a key held into another app included", () => {
    const { stop } = wired();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Meta", metaKey: true }));
    const remove = vi.spyOn(window, "removeEventListener");
    window.dispatchEvent(new Event("blur"));
    expect(remove.mock.calls.some(([type]) => String(type) === "wheel")).toBe(true);
    stop();
  });

  test("over something with its own zoom (a PDF) the wheel steps that, not the window", () => {
    const { steps, stop } = wired();
    document.body.innerHTML = '<div data-own-zoom><p class="in">page</p></div>';
    const own = document.querySelector<HTMLElement>("[data-own-zoom]")!;
    const heard: number[] = [];
    own.addEventListener("knobas-zoom", (event) => heard.push((event as CustomEvent<number>).detail));
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Control", ctrlKey: true }));
    wheelAt(document.querySelector(".in")!, { ctrlKey: true });
    expect(heard).toEqual([1]);
    expect(steps).toEqual([]);
    stop();
  });

  test("WebKit's gesture events walk the steps on the Mac", () => {
    const { steps, stop } = wired();
    const gesture = Object.assign(new Event("gesturechange", { cancelable: true }), { scale: 1.2 });
    window.dispatchEvent(gesture);
    expect(steps).toEqual([1]);
    expect(gesture.defaultPrevented).toBe(true);
    stop();
  });

  test("taken away whole when the shell goes", () => {
    const { steps, stop } = wired();
    stop();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Control", ctrlKey: true }));
    expect(wheelAt(document.body, { ctrlKey: true }).defaultPrevented).toBe(false);
    window.dispatchEvent(Object.assign(new Event("gesturechange"), { scale: 2 }));
    expect(steps).toEqual([]);
  });
});

describe("a level on its way", () => {
  test("holds the next steps until the page is laid out at it, then lands the newest of them once", async () => {
    const applied: number[] = [];
    const frames: (() => void)[] = [];
    const settles: (() => void)[] = [];
    const zoom = createZoom({
      storage: () => memory(),
      apply: (level) => void applied.push(level),
      frame: (draw) => void frames.push(draw),
      settled: (done) => void settles.push(done),
    });
    const frame = async () => {
      for (const draw of frames.splice(0)) draw();
      await Promise.resolve();
      await Promise.resolve();
    };
    zoom.step(1);
    await frame();
    expect(applied).toEqual([1.1]);
    // Two more notches while 110 % lays the page out: no frame lands them.
    zoom.step(1);
    zoom.step(1);
    await frame();
    expect(applied).toEqual([1.1]);
    expect(zoom.level, "the plate says where it is going").toBe(1.5);
    // 110 % heard and drawn: the newest level, once.
    for (const done of settles.splice(0)) done();
    await frame();
    expect(applied).toEqual([1.1, 1.5]);
    for (const done of settles.splice(0)) done();
    await frame();
    expect(applied).toEqual([1.1, 1.5]);
  });
});
