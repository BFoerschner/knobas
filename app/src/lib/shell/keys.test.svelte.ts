/**
 * The `Esc` unwind ladder and `⌘K`.
 *
 * The mockup got this from one global handler over one re-rendered DOM. A
 * component port keeps the same rungs but has to write them down, because
 * nothing about a Svelte tree enforces an order.
 */
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { installKeys } from "./keys";
import { createRouter } from "./router.svelte";

let teardown: (() => void) | undefined;
let routerTeardown: (() => void) | undefined;

function at(hash: string) {
  location.hash = hash;
  const router = createRouter();
  routerTeardown = router.start();
  const openLauncher = vi.fn();
  teardown = installKeys(router, { openLauncher });
  return { router, openLauncher };
}

function press(key: string, options: KeyboardEventInit = {}) {
  const event = new KeyboardEvent("keydown", {
    key,
    bubbles: true,
    cancelable: true,
    ...options,
  });
  window.dispatchEvent(event);
  return event;
}

beforeEach(() => {
  location.hash = "";
});

afterEach(() => {
  teardown?.();
  routerTeardown?.();
  teardown = undefined;
  routerTeardown = undefined;
});

test("Esc closes the detail slide-over, back to the room it was opened over", () => {
  const { router } = at("#/ctx/src:jira");
  router.go("#/ticket/mock:PAY-231");
  expect(router.route).toEqual({
    view: "room",
    ctx: "src:jira",
    detail: { kind: "ticket", entityId: "mock:PAY-231" },
  });

  press("Escape");
  expect(location.hash).toBe("#/ctx/src:jira");
});

test("Esc leaves a non-room view for the room", () => {
  const { router } = at("#/ctx/all");
  router.go("#/sources");
  expect(router.route).toEqual({ view: "sources" });

  press("Escape");
  expect(location.hash).toBe("#/ctx/all");
});

test("Esc leaves an M2+ view too, rather than stranding the reader on it", () => {
  const { router } = at("#/ctx/all");
  router.go("#/inbox");

  press("Escape");
  expect(location.hash).toBe("#/ctx/all");
});

/**
 * Rung 4, and the one worth a test of its own: `Esc` in a plain room does
 * nothing. Not "go home", not "quit" — a key that closes things must be safe
 * to press when there is nothing to close.
 */
test("Esc in a plain room does nothing at all", () => {
  at("#/ctx/src:jira");
  const before = location.hash;

  const event = press("Escape");
  expect(location.hash).toBe(before);
  expect(event.defaultPrevented).toBe(false);
});

/**
 * Rung 1: a modal stops the key before it reaches here, so the shell does not
 * *also* unwind the detail behind the dialog on a single press. `Modal` owns
 * that half; what this asserts is that the shell listens where a stopped
 * propagation actually stops it.
 */
test("a key whose propagation was stopped below window never reaches the ladder", () => {
  const { router } = at("#/ctx/all");
  router.go("#/ticket/mock:PAY-231");

  const dialog = document.createElement("div");
  document.body.append(dialog);
  dialog.addEventListener("keydown", (event) => event.stopPropagation());

  dialog.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  expect(location.hash).toBe("#/ticket/mock:PAY-231");

  dialog.remove();
});

test("Cmd-K and Ctrl-K both open the launcher", () => {
  const { openLauncher } = at("#/ctx/all");

  press("k", { metaKey: true });
  press("K", { ctrlKey: true });
  expect(openLauncher).toHaveBeenCalledTimes(2);
});

/** ⌘T is M3's. Binding it now would train a habit the app cannot honour. */
test("plain k, and Cmd-T, are left alone", () => {
  const { openLauncher } = at("#/ctx/all");

  press("k");
  press("t", { metaKey: true });
  expect(openLauncher).not.toHaveBeenCalled();
});
