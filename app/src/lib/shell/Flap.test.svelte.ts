import { flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test, vi } from "vitest";

import Flap from "./Flap.svelte";

function host(): HTMLDivElement {
  const target = document.createElement("div");
  document.body.append(target);
  return target;
}

/** Force the next `matchMedia` answer, as a component reads it on mount. */
function reducedMotion(reduce: boolean): void {
  vi.spyOn(window, "matchMedia").mockImplementation(
    (query: string) =>
      ({
        matches: reduce && query.includes("reduce"),
        media: query,
        onchange: null,
        addEventListener: () => {},
        removeEventListener: () => {},
        dispatchEvent: () => false,
      }) as unknown as MediaQueryList,
  );
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
});

test("shows the value and flips to a new one", () => {
  vi.useFakeTimers();
  reducedMotion(false);
  const target = host();
  const props = $state({ value: "4:59" });
  const app = mount(Flap, { target, props });
  flushSync();

  expect(target.querySelector(".flap")?.textContent).toContain("4:59");
  expect(target.querySelector(".leaf")).toBeNull();

  props.value = "4:58";
  flushSync();

  // Both halves are already on the new value; the leaf carrying the old one is
  // the transient part.
  expect(target.querySelector(".ft")?.textContent).toBe("4:58");
  expect(target.querySelector(".fb")?.textContent).toBe("4:58");
  expect(target.querySelector(".leaf.top")?.textContent).toBe("4:59");

  // ...and it is cleaned up rather than left in the DOM for ever.
  vi.advanceTimersByTime(400);
  flushSync();
  expect(target.querySelector(".leaf")).toBeNull();

  unmount(app);
  target.remove();
});

/**
 * Spec §2/§14. The CSS `@media (prefers-reduced-motion: reduce)` hides the
 * leaves, but a component that keeps scheduling them still runs a timer per
 * tick for an animation nobody sees -- and the sync countdown ticks once a
 * second for the life of the window.
 */
test("skips the leaf entirely under reduced motion", () => {
  reducedMotion(true);
  const target = host();
  const props = $state({ value: "4:59" });
  const app = mount(Flap, { target, props });
  flushSync();

  props.value = "4:58";
  flushSync();

  expect(target.querySelector(".ft")?.textContent).toBe("4:58");
  expect(target.querySelector(".leaf")).toBeNull();

  unmount(app);
  target.remove();
});

/** The tone and width map to the stylesheet's own modifier classes. */
test("carries its tone and width as classes, not as styles", () => {
  reducedMotion(false);
  const target = host();
  const app = mount(Flap, {
    target,
    props: { value: "401", tone: "fail" as const, width: "s" as const, big: true },
  });
  flushSync();

  const cell = target.querySelector(".flap");
  expect(cell?.classList.contains("fail")).toBe(true);
  expect(cell?.classList.contains("w-s")).toBe(true);
  expect(cell?.classList.contains("big")).toBe(true);
  expect(cell?.getAttribute("style")).toBeNull();
  // `plain` is the absence of a tone, not a class of its own.
  expect(cell?.classList.contains("plain")).toBe(false);

  unmount(app);
  target.remove();
});

/**
 * A flap that re-mounted (a room switch, a tab change) must not animate from
 * the value the *previous* instance held.
 */
test("does not flip on first render", () => {
  vi.useFakeTimers();
  reducedMotion(false);
  const target = host();
  const app = mount(Flap, { target, props: { value: "4:59" } });
  // Without this the assertion runs before the mount effect does, and would
  // hold even for a component that flips on every first render.
  flushSync();

  expect(target.querySelector(".flap")?.textContent).toContain("4:59");
  expect(target.querySelector(".leaf")).toBeNull();

  unmount(app);
  target.remove();
});
