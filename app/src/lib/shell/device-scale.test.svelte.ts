import { flushSync } from "svelte";
import { afterEach, expect, test, vi } from "vitest";

import { checkScale, deviceScale, onScaleChange } from "./device-scale.svelte";

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

/** A `matchMedia` that remembers each query asked and who listens to it. */
function queries() {
  const asked: { media: string; listening: number }[] = [];
  vi.stubGlobal("matchMedia", (media: string) => {
    const query = { media, listening: 0 };
    asked.push(query);
    return {
      media,
      matches: true,
      addEventListener: () => void query.listening++,
      removeEventListener: () => void query.listening--,
    };
  });
  return asked;
}

test("tells each listener once a change of the scale, a zoom step's or a new screen's, and nothing when it is the same", () => {
  vi.stubGlobal("devicePixelRatio", 1.25);
  const asked = queries();
  const heard = vi.fn();
  const stop = onScaleChange(heard);
  expect(asked.at(-1)).toEqual({ media: "(resolution: 1.25dppx)", listening: 1 });
  checkScale();
  expect(heard).not.toHaveBeenCalled();
  vi.stubGlobal("devicePixelRatio", 1.375);
  // A zoom resizes the CSS viewport: the resize alone is enough to hear it.
  window.dispatchEvent(new Event("resize"));
  expect(heard).toHaveBeenCalledTimes(1);
  // Heard again through a query on the new scale, the old one let go.
  expect(asked.at(-1)).toEqual({ media: "(resolution: 1.375dppx)", listening: 1 });
  expect(asked.filter((query) => query.listening > 0)).toHaveLength(1);
  stop();
});

test("hears the window only while someone listens", () => {
  queries();
  const add = vi.spyOn(window, "addEventListener");
  const remove = vi.spyOn(window, "removeEventListener");
  const first = onScaleChange(() => {});
  const second = onScaleChange(() => {});
  expect(add.mock.calls.filter(([type]) => String(type) === "resize")).toHaveLength(1);
  first();
  expect(remove.mock.calls.filter(([type]) => String(type) === "resize")).toHaveLength(0);
  second();
  expect(remove.mock.calls.filter(([type]) => String(type) === "resize")).toHaveLength(1);
});

test("an effect that reads the ratio runs again when it changes, and is a listener while it runs", () => {
  vi.stubGlobal("devicePixelRatio", 2);
  queries();
  const seen: number[] = [];
  const stop = $effect.root(() => {
    $effect(() => void seen.push(deviceScale.ratio));
  });
  flushSync();
  vi.stubGlobal("devicePixelRatio", 2.2);
  // Nobody else listens: the effect's read is what hears the window's resize.
  window.dispatchEvent(new Event("resize"));
  flushSync();
  expect(seen).toEqual([2, 2.2]);
  stop();
});
