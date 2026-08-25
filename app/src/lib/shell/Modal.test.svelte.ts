/**
 * What `<dialog>`-shaped behaviour a hand-rolled modal has to reproduce.
 *
 * The mockup got all of this for free from a browser `dialog()` and a single
 * global key handler over one re-rendered DOM. A component port loses every
 * one of them silently: nothing throws, the dialog just stops being a dialog.
 */
import { flushSync, mount, unmount } from "svelte";
import { expect, test, vi } from "vitest";

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
  expect(target.querySelector(".dlg")?.contains(document.activeElement)).toBe(true);

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

  target.querySelector<HTMLElement>(".dlg")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  expect(onclose).not.toHaveBeenCalled();

  target.querySelector<HTMLElement>(".scrim")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
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

  const dialog = target.querySelector(".dlg");
  expect(dialog?.getAttribute("role")).toBe("dialog");
  expect(dialog?.getAttribute("aria-modal")).toBe("true");
  const labelledBy = dialog?.getAttribute("aria-labelledby");
  expect(labelledBy).toBeTruthy();
  expect(document.getElementById(labelledBy!)?.textContent).toContain("Add source");

  unmount(app);
  target.remove();
});
