/**
 * The one dialog shell (`TfDialog`), and what `<dialog>`-shaped behaviour it
 * reproduces by hand. These tests were `shell/Modal.svelte`'s, the second shell
 * the asset tree drew until the review of #2238 moved its callers here.
 *
 * The mockup got all of this for free from a browser `dialog()` and a single
 * global key handler over one re-rendered DOM. A component port loses every
 * one of them silently: nothing throws, the dialog just stops being a dialog.
 */
import { readFileSync, readdirSync } from "node:fs";
import { join, relative } from "node:path";

import { flushSync, mount, unmount } from "svelte";
import { expect, test, vi } from "vitest";

import { createLayers } from "../layers.svelte";
import Modal from "./TfDialog.fixture.svelte";

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

/**
 * A layer opened over this dialog (a draft over the write queue panel, #1474) is
 * the one `Esc` closes, even with focus still in this dialog: the press goes on
 * to the shell, which closes the topmost layer.
 */
test("Escape inside it, with another layer on top, is left to the shell", () => {
  const layers = createLayers();
  const onclose = vi.fn();
  const target = host();
  const app = mount(Modal, { target, props: { onclose, layers } });
  flushSync();
  const above = vi.fn();
  const off = layers.add({ kind: "dialog", close: above });
  const shell = vi.fn();
  document.addEventListener("keydown", shell);

  press("Escape");
  expect(onclose).not.toHaveBeenCalled();
  expect(shell).toHaveBeenCalledOnce();

  document.removeEventListener("keydown", shell);
  off();
  unmount(app);
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

/** F-616: what was typed is not lost to a click beside the sheet. A dialog with a footer has something to lose. */
test("a click on the scrim leaves a dialog with a footer open, and focus where it was", () => {
  const onclose = vi.fn();
  const target = host();
  const app = mount(Modal, { target, props: { onclose, content: "input" } });
  flushSync();
  const field = target.querySelector<HTMLInputElement>(".tf-body input")!;
  expect(document.activeElement).toBe(field);

  const scrim = target.querySelector<HTMLElement>(".tf-modal")!;
  const down = new MouseEvent("mousedown", { bubbles: true, cancelable: true });
  scrim.dispatchEvent(down);
  expect(down.defaultPrevented, "the press does not take focus off the field").toBe(true);
  scrim.dispatchEvent(new MouseEvent("click", { bubbles: true }));

  expect(onclose).not.toHaveBeenCalled();
  expect(document.activeElement).toBe(field);

  unmount(app);
  target.remove();
});

test("a viewer has no footer and nothing to lose: a click on the scrim closes it, a click inside does not", () => {
  const onclose = vi.fn();
  const target = host();
  const app = mount(Modal, { target, props: { onclose, viewer: true } });
  flushSync();

  target.querySelector<HTMLElement>(".tf-dialog")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  expect(onclose).not.toHaveBeenCalled();

  target.querySelector<HTMLElement>(".tf-modal")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  expect(onclose).toHaveBeenCalledTimes(1);

  unmount(app);
  target.remove();
});

/** F-035: `Enter` on arrival must not press what the dialog is asking about. */
test("focus never arrives on a button in the body: with no other control it goes to the ×", () => {
  const behind = opener();
  const target = host();
  const app = mount(Modal, { target, props: { onclose: () => {}, content: "button" } });
  flushSync();

  expect(document.activeElement?.getAttribute("aria-label")).toBe("Close");

  unmount(app);
  behind.remove();
  target.remove();
});

test("the first field is the first of any kind, a select or a textarea too, ahead of a button in the body", () => {
  const behind = opener();
  const target = host();
  const app = mount(Modal, { target, props: { onclose: () => {}, content: "select" } });
  flushSync();

  expect(document.activeElement?.tagName).toBe("SELECT");

  unmount(app);
  behind.remove();
  target.remove();
});

test("with no field, a radio takes focus ahead of a button, and Enter on it presses the primary, once (the key table, F-240)", () => {
  const behind = opener();
  const onsubmit = vi.fn();
  const onprimary = vi.fn();
  const target = host();
  const app = mount(Modal, { target, props: { onclose: () => {}, content: "radios", onsubmit, onprimary } });
  flushSync();

  const radio = target.querySelector<HTMLInputElement>("input[type=radio]")!;
  expect(document.activeElement).toBe(radio);
  const enter = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true });
  radio.dispatchEvent(enter);
  expect(enter.defaultPrevented, "implicit submission is the keydown's default").toBe(true);
  expect(onprimary).toHaveBeenCalledOnce();

  unmount(app);
  behind.remove();
  target.remove();
});

test("⌘Enter presses the primary from anywhere in the dialog (the key table)", () => {
  const behind = opener();
  const onprimary = vi.fn();
  const target = host();
  const app = mount(Modal, { target, props: { onclose: () => {}, content: "radios", onprimary } });
  flushSync();
  const send = new KeyboardEvent("keydown", { key: "Enter", metaKey: true, bubbles: true, cancelable: true });
  target.querySelector("input")!.dispatchEvent(send);
  expect(send.defaultPrevented).toBe(true);
  expect(onprimary).toHaveBeenCalledOnce();
  unmount(app);
  behind.remove();
  target.remove();
});

test.each(["danger", "guarded", "off"] as const)("⌘Enter doesn't reach a primary that is %s", (primary) => {
  const behind = opener();
  const onprimary = vi.fn();
  const target = host();
  const app = mount(Modal, { target, props: { onclose: () => {}, content: "radios", onprimary, primary } });
  flushSync();
  target.querySelector("input")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", metaKey: true, bubbles: true, cancelable: true }));
  expect(onprimary).not.toHaveBeenCalled();
  unmount(app);
  behind.remove();
  target.remove();
});

test.each(["danger", "guarded", "off"] as const)("Enter on a radio doesn't reach a primary that is %s", (primary) => {
  const behind = opener();
  const onprimary = vi.fn();
  const target = host();
  const app = mount(Modal, { target, props: { onclose: () => {}, content: "radios", onprimary, primary } });
  flushSync();

  const enter = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true });
  target.querySelector<HTMLInputElement>("input[type=radio]")!.dispatchEvent(enter);
  expect(enter.defaultPrevented).toBe(true);
  expect(onprimary).not.toHaveBeenCalled();

  unmount(app);
  behind.remove();
  target.remove();
});

/** F-030: a control removed while it held focus drops it on the body, and `Tab` from there walks the page behind. */
test("focus lost to a removed control comes back into the dialog", async () => {
  const behind = opener();
  const target = host();
  const app = mount(Modal, { target, props: { onclose: () => {}, content: "rows" } });
  flushSync();

  const row = target.querySelector<HTMLButtonElement>("[data-row]")!;
  row.focus();
  row.click();
  flushSync();
  await Promise.resolve();

  expect(target.querySelector("[data-row]")).toBeNull();
  expect(document.activeElement?.getAttribute("aria-label")).toBe("Close");

  unmount(app);
  behind.remove();
  target.remove();
});

/** The enabled rule: `why` is the one markup for what a disabled primary waits for, just left of *Cancel*. */
test("why draws the missing line in the footer, and nothing when there is none", () => {
  const target = host();
  const app = mount(Modal, { target, props: { onclose: () => {}, why: "Name it first." } });
  flushSync();

  const line = target.querySelector("footer > p.tf-dwhy[role=status]");
  expect(line?.textContent).toBe("Name it first.");

  unmount(app);
  target.remove();

  const clear = host();
  const none = mount(Modal, { target: clear, props: { onclose: () => {} } });
  flushSync();
  expect(clear.querySelector(".tf-dwhy")).toBeNull();
  unmount(none);
  clear.remove();
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

/** The header's Close is drawn only when asked for: the Settings dialogs close from their footer. */
test("draws no Close unless asked", () => {
  const target = host();
  const app = mount(Modal, { target, props: { onclose: () => {}, closeButton: false } });
  flushSync();

  expect(target.querySelector('header button[aria-label="Close"]')).toBeNull();
  expect(target.querySelector("header h2")?.textContent).toBe("Add source");

  unmount(app);
  target.remove();
});

/** A title with markup in it (the worklog draft's ticket key in mono, #2346) is a snippet, still the dialog's name. */
test("a snippet title draws its markup inside the heading that names the dialog", () => {
  const target = host();
  const app = mount(Modal, { target, props: { onclose: () => {}, markupTitle: true } });
  flushSync();

  const dialog = target.querySelector("[role=dialog]")!;
  const heading = target.querySelector(`#${dialog.getAttribute("aria-labelledby")}`);
  expect(heading?.textContent).toBe("Log time to PROJ-7");
  expect(heading?.querySelector(".tf-mono")?.textContent).toBe("PROJ-7");

  unmount(app);
  target.remove();
});

/** A dialog of its own width passes a class beside `tf-dialog` (Shell conventions); the import dialog's is `tf-dwide`. */
test("a dialog of its own width carries its class beside tf-dialog", () => {
  const target = host();
  const app = mount(Modal, { target, props: { onclose: () => {}, className: "tf-dwide" } });
  flushSync();

  expect(target.querySelector(".tf-modal > .tf-dialog.tf-dwide[role=dialog]")).not.toBeNull();

  unmount(app);
  target.remove();
});

/**
 * **One dialog shell** (review of #2238). `TfDialog` is the only component that
 * draws the scrim and the sheet; a dialog of another width passes `className`.
 * Hand-drawn `.tf-dialog` markup inside a scrim of its own is a second shell.
 */
test("TfDialog is the only component that draws the dialog's scrim", () => {
  const root = join(process.cwd(), "src");
  const walk = (dir: string): string[] =>
    readdirSync(dir, { withFileTypes: true }).flatMap((entry) =>
      entry.isDirectory() ? walk(join(dir, entry.name)) : entry.name.endsWith(".svelte") ? [join(dir, entry.name)] : [],
    );
  const shells = walk(root)
    .filter((file) => !file.endsWith(".fixture.svelte"))
    .filter((file) => /class="tf-modal"/.test(readFileSync(file, "utf8")))
    .map((file) => relative(root, file));
  expect(shells).toEqual([
    "lib/tf/settings/TfDialog.svelte",
  ]);
});

/**
 * **The ticket-first dialog** (#2292). The asset tree's dialogs were the last
 * drawn with `unported.css`'s `.scrim` / `.dlg` and its old `×`; they take the
 * same sheet, header and canonical footer as every dialog (design-system spec,
 * Controls, *Dialog footer*), and their Close is a Controls icon button.
 */
test("draws as the ticket-first dialog, with the canonical footer", () => {
  const target = host();
  const app = mount(Modal, { target, props: { onclose: () => {} } });
  flushSync();

  const dialog = target.querySelector(".tf-modal > .tf-dialog[role=dialog]");
  expect(dialog, "the ticket-first sheet").not.toBeNull();
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
