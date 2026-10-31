/**
 * What `<dialog>`-shaped behaviour a hand-rolled modal has to reproduce.
 *
 * The mockup got all of this for free from a browser `dialog()` and a single
 * global key handler over one re-rendered DOM. A component port loses every
 * one of them silently: nothing throws, the dialog just stops being a dialog.
 */
import { flushSync, mount, unmount } from "svelte";
import { expect, test, vi } from "vitest";

import { createLayers } from "../tf/layers.svelte";
import Modal from "./Modal.fixture.svelte";

/** An opener button, focused, as a real caller would have. */
function opener(): HTMLButtonElement {
  const button = document.createElement("button");
  button.textContent = "Add source";
  document.body.append(button);
  button.focus();
  return button;
}

function host(): HTMLDivElement {
  const target = document.createElement("div");
  document.body.append(target);
  return target;
}

function press(key: string, options: KeyboardEventInit = {}) {
  document.activeElement?.dispatchEvent(
    new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...options }),
  );
}

test("moves focus inside on mount and restores it on unmount", () => {
  const button = opener();
  const target = host();
  const app = mount(Modal, { target, props: { onclose: () => {} } });
  flushSync();

  expect(document.activeElement).not.toBe(button);
  expect(target.querySelector(".tf-dialog")?.contains(document.activeElement)).toBe(true);

  unmount(app);
  expect(document.activeElement).toBe(button);

  button.remove();
  target.remove();
});

test("Escape closes it exactly once and does not reach the shell", () => {
  const onclose = vi.fn();
  const target = host();
  const shell = vi.fn();
  document.addEventListener("keydown", shell);
  const app = mount(Modal, { target, props: { onclose } });
  flushSync();

  press("Escape");
  expect(onclose).toHaveBeenCalledTimes(1);
  // Rung 1 of the Esc ladder: a modal swallows the key, so the shell does not
  // also unwind the detail slide-over behind it.
  expect(shell).not.toHaveBeenCalled();

  document.removeEventListener("keydown", shell);
  unmount(app);
  target.remove();
});

test("a click on the scrim closes it; a click inside the dialog does not", () => {
  const onclose = vi.fn();
  const target = host();
  const app = mount(Modal, { target, props: { onclose } });
  flushSync();

  target.querySelector<HTMLElement>(".tf-dialog")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  expect(onclose).not.toHaveBeenCalled();

  target.querySelector<HTMLElement>(".tf-modal")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  expect(onclose).toHaveBeenCalledTimes(1);

  unmount(app);
  target.remove();
});

test("Tab cycles within the dialog instead of escaping to the page behind", () => {
  const behind = opener();
  const target = host();
  const app = mount(Modal, { target, props: { onclose: () => {} } });
  flushSync();

  const focusable = [...target.querySelectorAll<HTMLElement>("button")];
  expect(focusable.length).toBeGreaterThan(1);
  const first = focusable[0]!;
  const last = focusable[focusable.length - 1]!;

  last.focus();
  press("Tab");
  expect(document.activeElement).toBe(first);

  first.focus();
  press("Tab", { shiftKey: true });
  expect(document.activeElement).toBe(last);

  unmount(app);
  behind.remove();
  target.remove();
});

test("announces itself as a dialog labelled by its title", () => {
  const target = host();
  const app = mount(Modal, { target, props: { onclose: () => {} } });
  flushSync();

  const dialog = target.querySelector(".tf-dialog");
  expect(dialog?.getAttribute("role")).toBe("dialog");
  expect(dialog?.getAttribute("aria-modal")).toBe("true");
  const labelledBy = dialog?.getAttribute("aria-labelledby");
  expect(labelledBy).toBeTruthy();
  expect(document.getElementById(labelledBy!)?.textContent).toContain("Add source");

  unmount(app);
  target.remove();
});

/**
 * **Open-time focus is the body's** (#1431). The header's Close button comes
 * first in document order, and a dialog that rings it leaves a reader who opened
 * it to type typing into nothing.
 */
test("opens with focus on the first field in its body, not the header's Close", () => {
  const behind = opener();
  const target = host();
  const app = mount(Modal, { target, props: { onclose: () => {}, content: "input" } });
  flushSync();

  expect(document.activeElement).toBe(target.querySelector(".tf-body input"));

  unmount(app);
  behind.remove();
  target.remove();
});

/** A body with nothing to focus leaves the ring where it was before #1431: on Close. */
test("with nothing focusable in its body, focus goes to Close", () => {
  const behind = opener();
  const target = host();
  const app = mount(Modal, { target, props: { onclose: () => {}, content: "none" } });
  flushSync();

  expect(document.activeElement?.getAttribute("aria-label")).toBe("Close");

  unmount(app);
  behind.remove();
  target.remove();
});

/**
 * Under the ticket-first shell the window's `Esc` closes the topmost layer, and
 * history stays put while a dialog is up (#1086). A modal the stack cannot see
 * is skipped by both: `Esc` with focus outside it navigated the page behind
 * (#1527).
 */
test("sits on the layer stack while open, and the stack's close is its own", () => {
  const layers = createLayers();
  const onclose = vi.fn();
  const target = host();
  const app = mount(Modal, { target, props: { onclose, layers } });
  flushSync();

  expect(layers.open.map((layer) => layer.kind)).toEqual(["dialog"]);
  layers.open[0]!.close();
  expect(onclose).toHaveBeenCalledOnce();

  unmount(app);
  expect(layers.open).toEqual([]);
  target.remove();
});

/**
 * **Modal is the ticket-first dialog** (#2292). It was the last dialog drawn
 * with `unported.css`'s `.scrim` / `.dlg` and its old `×`; it now takes the
 * same sheet, header and canonical footer as `TfDialog` (design-system spec,
 * Controls, *Dialog footer*), and its Close is a Controls icon button.
 */
test("draws as the ticket-first dialog, with the canonical footer and no unported class", () => {
  const target = host();
  const app = mount(Modal, { target, props: { onclose: () => {} } });
  flushSync();

  const dialog = target.querySelector(".tf-modal > .tf-dialog[role=dialog]");
  expect(dialog, "the ticket-first sheet").not.toBeNull();
  expect(target.querySelector(".scrim, .dlg, .dlg-h, .dlg-b, .dlg-f, .x, .btn")).toBeNull();
  expect(dialog!.querySelector("header h2")?.textContent).toBe("Add source");
  expect(dialog!.querySelector("header .tf-dsub")?.textContent).toBe("Step 1 of 5");
  expect(dialog!.querySelector('header button[aria-label="Close"]')?.classList.contains("tf-iconbtn")).toBe(true);
  expect(dialog!.querySelector(".tf-body button")).not.toBeNull();
  expect(
    [...dialog!.querySelectorAll("footer .tf-btn")].map((button) => [button.textContent, button.className]),
  ).toEqual([
    ["Cancel", "tf-btn tf-ghost"],
    ["Add", "tf-btn tf-primary"],
  ]);

  unmount(app);
  target.remove();
});
