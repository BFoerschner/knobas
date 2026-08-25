/**
 * The app frame: what the top strip carries, and what it deliberately does
 * not.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test, vi } from "vitest";

import Shell from "./Shell.fixture.svelte";
import { createRouter } from "./router.svelte";

let stopRouter: (() => void) | undefined;

afterEach(() => {
  stopRouter?.();
  stopRouter = undefined;
  vi.useRealTimers();
});

function render(hash = "#/ctx/all") {
  location.hash = hash;
  const router = createRouter();
  stopRouter = router.start();
  const onsearch = vi.fn();
  const target = document.createElement("div");
  document.body.append(target);
  const app = mount(Shell, { target, props: { router, onsearch } });
  flushSync();
  return {
    router,
    onsearch,
    target,
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

test("the gear navigates to the sources view and marks itself current", () => {
  const screen = render();
  const gear = screen.target.querySelector<HTMLButtonElement>('[aria-label="Sources"]');
  expect(gear).toBeDefined();
  expect(gear?.getAttribute("aria-current")).toBeNull();

  gear?.click();
  flushSync();

  expect(location.hash).toBe("#/sources");
  expect(screen.router.route).toEqual({ view: "sources" });
  expect(gear?.getAttribute("aria-current")).toBe("page");

  screen.done();
});

test("the search field opens the launcher rather than navigating", () => {
  const screen = render();
  screen.target.querySelector<HTMLButtonElement>(".searchfield")?.click();

  expect(screen.onsearch).toHaveBeenCalledOnce();
  expect(location.hash).toBe("#/ctx/all");

  screen.done();
});

/**
 * Spec §2 lists a timer, an inbox, Assets and Today/Day in the top strip. All
 * four are M2-M4. A slot reserved for a button that cannot work is dead chrome
 * that teaches the reader the app is unfinished and nothing else.
 */
test("carries no control for a milestone that has not landed", () => {
  const screen = render();
  const labels = [...screen.target.querySelectorAll("button")].map(
    (button) => `${button.textContent ?? ""} ${button.getAttribute("aria-label") ?? ""}`.trim(),
  );

  for (const absent of ["Assets", "Today", "Day", "Inbox", "timer"]) {
    expect(labels.join(" | ")).not.toContain(absent);
  }

  screen.done();
});

/**
 * The status-bar clock ticks from an `$effect`, not from module scope. A
 * module-level interval survives every remount and every hot reload, so a dev
 * session accumulates one per mount, all of them writing to components that
 * are gone.
 */
test("the clock's interval is torn down with the component", () => {
  vi.useFakeTimers();
  const screen = render();
  expect(vi.getTimerCount()).toBeGreaterThan(0);

  screen.done();
  expect(vi.getTimerCount()).toBe(0);
});
