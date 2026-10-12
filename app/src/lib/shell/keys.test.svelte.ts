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

/**
 * `restoreTile` answers whether a tile was maximised (#250): `false` is the
 * ordinary room, and a test that wants rung 4 to have something to do says so.
 */
function at(hash: string, maximised = false) {
  location.hash = hash;
  const router = createRouter();
  routerTeardown = router.start();
  const openLauncher = vi.fn();
  const restoreTile = vi.fn(() => maximised);
  const toggleTimer = vi.fn();
  teardown = installKeys(router, { openLauncher, restoreTile, toggleTimer });
  return { router, openLauncher, restoreTile, toggleTimer };
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
  const { restoreTile } = at("#/ctx/src:jira");
  const before = location.hash;

  const event = press("Escape");
  expect(location.hash).toBe(before);
  expect(event.defaultPrevented).toBe(false);
  // Asked, and there was nothing to restore: that is what makes rung 5 the
  // rung it reached rather than one it skipped.
  expect(restoreTile).toHaveBeenCalledTimes(1);
});

/**
 * Rung 4 (#250): a maximised tile is the one thing left to unwind in a room
 * with no detail over it. The room does the restoring; what the ladder owns
 * is that the key reached it, that the press counted, and that the address
 * did not move -- restoring a tile is not a navigation.
 */
test("Esc restores a maximised tile when nothing else is open", () => {
  const { restoreTile } = at("#/ctx/src:jira", true);
  const before = location.hash;

  const event = press("Escape");
  expect(restoreTile).toHaveBeenCalledTimes(1);
  expect(location.hash).toBe(before);
  expect(event.defaultPrevented).toBe(true);
});

/**
 * The order of rungs 2 and 4: a detail open over a maximised tile closes
 * first, and the tile stays maximised for the next press. One keystroke, one
 * rung -- a ladder that restored the tile *and* closed the detail, or
 * restored the tile from under an open detail, would be unwinding two things
 * or the wrong one.
 */
test("Esc closes the detail before it restores the tile under it", () => {
  const { router, restoreTile } = at("#/ctx/src:jira", true);
  router.go("#/ticket/mock:PAY-231");

  press("Escape");
  expect(location.hash).toBe("#/ctx/src:jira");
  expect(restoreTile).not.toHaveBeenCalled();

  press("Escape");
  expect(restoreTile).toHaveBeenCalledTimes(1);
  expect(location.hash).toBe("#/ctx/src:jira");
});

/** ...and a non-room view is rung 3, which a tile in no room can sit above. */
test("Esc leaves a non-room view without asking about tiles", () => {
  const { router, restoreTile } = at("#/ctx/all", true);
  router.go("#/sources");

  press("Escape");
  expect(location.hash).toBe("#/ctx/all");
  expect(restoreTile).not.toHaveBeenCalled();
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

test("plain k is left alone", () => {
  const { openLauncher } = at("#/ctx/all");

  press("k");
  expect(openLauncher).not.toHaveBeenCalled();
});

/**
 * ⌘T reaches the timer, and the keystroke is swallowed (#278).
 *
 * `preventDefault` matters as much as the call: ⌘T is the browser's new-tab
 * on every platform this ships to, and a keystroke that means two things
 * depending on the build is one nobody can rely on.
 *
 * **What the key does is not decided here** — `timer.svelte.ts`'s `press()`
 * owns the three behaviours, and `timer.test.svelte.ts` drives them. This is
 * the binding: the key is heard, and it is heard once.
 */
test("Cmd-T and Ctrl-T both reach the timer, and the keystroke is swallowed", () => {
  const { toggleTimer, openLauncher } = at("#/ctx/all");

  const meta = press("t", { metaKey: true });
  const ctrl = press("T", { ctrlKey: true });
  expect(toggleTimer).toHaveBeenCalledTimes(2);
  expect(meta.defaultPrevented).toBe(true);
  expect(ctrl.defaultPrevented).toBe(true);
  expect(openLauncher, "⌘T opened the launcher as well").not.toHaveBeenCalled();
});

/** A bare `t` is typing, not a command. */
test("plain t is left alone", () => {
  const { toggleTimer } = at("#/ctx/all");

  press("t");
  expect(toggleTimer).not.toHaveBeenCalled();
});

/**
 * ⌘T is a verb, not a rung, so it does not unwind anything — a detail open
 * over a room stays open while the clock starts.
 */
test("Cmd-T does not move the address", () => {
  const { router } = at("#/ctx/src:jira");
  router.go("#/ticket/mock:PAY-231");

  press("t", { metaKey: true });
  expect(location.hash).toBe("#/ticket/mock:PAY-231");
});

/**
 * ...and a dialog that stops the key keeps it, exactly as `Esc` is kept: ⌘T
 * inside the picker must not start a second timer behind it.
 */
test("a Cmd-T whose propagation was stopped below window never reaches the timer", () => {
  const { toggleTimer } = at("#/ctx/all");

  const dialog = document.createElement("div");
  document.body.append(dialog);
  dialog.addEventListener("keydown", (event) => event.stopPropagation());

  dialog.dispatchEvent(new KeyboardEvent("keydown", { key: "t", metaKey: true, bubbles: true }));
  expect(toggleTimer).not.toHaveBeenCalled();

  dialog.remove();
});

/**
 * Back and forward from the keyboard and the mouse (#693): ⌘[ / ⌘] as a
 * browser binds them, ⌘← / ⌘→ too, and the mouse's own back and forward
 * buttons. A text field keeps its keys: ⌘[ outdents in an editor and ⌘←
 * moves to the start of the line, so none of them navigates from inside one.
 */
test("⌘[ and ⌘] step back and forward through the addresses", async () => {
  const { router } = at("#/ctx/all");
  router.go("#/ticket/mock:PAY-231");

  const back = press("[", { metaKey: true });
  expect(back.defaultPrevented).toBe(true);
  await vi.waitFor(() => expect(location.hash).toBe("#/ctx/all"));
  await vi.waitFor(() => expect(router.canGoForward).toBe(true));

  press("]", { metaKey: true });
  await vi.waitFor(() => expect(location.hash).toBe("#/ticket/mock:PAY-231"));
  await vi.waitFor(() => expect(router.canGoForward).toBe(false));

  press("ArrowLeft", { metaKey: true });
  await vi.waitFor(() => expect(location.hash).toBe("#/ctx/all"));
  await vi.waitFor(() => expect(router.canGoForward).toBe(true));
  press("ArrowRight", { metaKey: true });
  await vi.waitFor(() => expect(location.hash).toBe("#/ticket/mock:PAY-231"));
});

test("inside a text field the history keys are the field's", async () => {
  const { router } = at("#/ctx/all");
  router.go("#/ticket/mock:PAY-231");
  const field = document.createElement("textarea");
  document.body.append(field);
  field.focus();

  for (const key of ["[", "ArrowLeft"]) {
    const event = new KeyboardEvent("keydown", { key, metaKey: true, bubbles: true, cancelable: true });
    field.dispatchEvent(event);
    expect(event.defaultPrevented, key).toBe(false);
  }
  await new Promise((settle) => setTimeout(settle, 20));
  expect(location.hash).toBe("#/ticket/mock:PAY-231");
  expect(router.canGoBack).toBe(true);
  field.remove();
});

test("the mouse's back and forward buttons navigate, and nothing else happens", async () => {
  const { router } = at("#/ctx/all");
  router.go("#/ticket/mock:PAY-231");

  const click = (button: number) => {
    const up = new MouseEvent("mouseup", { button, bubbles: true, cancelable: true });
    document.body.dispatchEvent(up);
    return up;
  };
  expect(click(3).defaultPrevented).toBe(true);
  await vi.waitFor(() => expect(location.hash).toBe("#/ctx/all"));
  await vi.waitFor(() => expect(router.canGoForward).toBe(true));
  expect(click(4).defaultPrevented).toBe(true);
  await vi.waitFor(() => expect(location.hash).toBe("#/ticket/mock:PAY-231"));

  // The main and middle buttons are not history's.
  expect(click(0).defaultPrevented).toBe(false);
  expect(click(1).defaultPrevented).toBe(false);
});

/**
 * Two keys that look like history's and are not (#697 review): ⌘← that a
 * focused control already answered (a view's tab bar moves a tab on it), and
 * AltGr's bracket on a Windows or Linux layout, which arrives as Ctrl+Alt+[.
 * A Mac layout that types `[` with ⌥ (German) still goes back on ⌘⌥[.
 */
test("a key a control answered, or AltGr's bracket, does not step; ⌘⌥[ does", async () => {
  const { router } = at("#/ctx/all");
  router.go("#/ticket/mock:PAY-231");

  const tab = document.createElement("button");
  tab.addEventListener("keydown", (event) => event.preventDefault());
  document.body.append(tab);
  tab.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowLeft", metaKey: true, bubbles: true, cancelable: true }));
  press("[", { ctrlKey: true, altKey: true });
  await new Promise((settle) => setTimeout(settle, 20));
  expect(location.hash).toBe("#/ticket/mock:PAY-231");
  tab.remove();

  expect(press("[", { metaKey: true, altKey: true }).defaultPrevented).toBe(true);
  await vi.waitFor(() => expect(location.hash).toBe("#/ctx/all"));
});
