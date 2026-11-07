/** The one menu (design system, *Layers*): where it draws, its keys, and where focus goes when it closes. */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, expect, test } from "vitest";

import { createLayers } from "./layers.svelte";
import Fixture from "./TfMenu.fixture.svelte";

let target: HTMLElement;
let app: Record<string, unknown>;
const layers = createLayers();

async function draw(props: { host?: boolean; ready?: boolean; inDialog?: boolean } = {}) {
  target = document.createElement("div");
  document.body.append(target);
  app = mount(Fixture, { target, props: { layers, ...props } });
  flushSync();
  await tick();
}

afterEach(() => {
  void unmount(app);
  target.remove();
  layers.reset();
});

const menu = () => document.querySelector<HTMLElement>("[role=menu]");
const face = () => target.querySelector<HTMLButtonElement>(".face")!;
const key = (name: string, options: KeyboardEventInit = {}) => {
  const event = new KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true, ...options });
  (document.activeElement ?? document.body).dispatchEvent(event);
  flushSync();
  return event;
};

async function open() {
  face().click();
  flushSync();
  await tick();
}

test("draws in the host at the shell's root, not where it was opened, and leaves with its owner", async () => {
  await draw({ host: true });
  await open();
  expect(menu()!.parentElement?.className).toBe("tf-floats");
  expect(target.querySelector(".wrap [role=menu]")).toBeNull();
  face().click();
  flushSync();
  expect(menu()).toBeNull();
});

test("with no shell it stays where it stands", async () => {
  await draw();
  await open();
  expect(target.querySelector(".wrap [role=menu]")).not.toBeNull();
});

test("a press on a moved item still answers, through the shell's delegated events", async () => {
  await draw({ host: true });
  await open();
  menu()!.querySelector<HTMLButtonElement>(".one")!.click();
  flushSync();
  expect(menu()).toBeNull();
});

test("is on the layer stack while open, as a dialog, so the page's keys wait", async () => {
  await draw();
  await open();
  expect(layers.open.map((layer) => layer.kind)).toEqual(["dialog"]);
  layers.open[0]!.close();
  flushSync();
  expect(menu()).toBeNull();
  expect(layers.open).toEqual([]);
  expect(document.activeElement).toBe(face());
});

test("focus goes to the first item it can take, and the arrows skip a disabled one", async () => {
  await draw();
  await open();
  const one = menu()!.querySelector(".one");
  const three = menu()!.querySelector(".three");
  expect(document.activeElement).toBe(one);
  key("ArrowDown");
  expect(document.activeElement).toBe(three);
  key("ArrowDown");
  expect(document.activeElement).toBe(one);
  key("End");
  expect(document.activeElement).toBe(three);
  key("Home");
  expect(document.activeElement).toBe(one);
});

test("Esc and Tab close it and focus its opener, and neither reaches the page", async () => {
  await draw();
  for (const name of ["Escape", "Tab"]) {
    await open();
    expect(key(name).defaultPrevented, name).toBe(true);
    expect(menu(), name).toBeNull();
    expect(document.activeElement, name).toBe(face());
  }
});

test("a click anywhere else closes it and takes no focus; a click on its opener does not open it again", async () => {
  await draw();
  await open();
  target.querySelector<HTMLButtonElement>(".after")!.focus();
  flushSync();
  expect(menu(), "focus leaving closes it").toBeNull();

  await open();
  document.body.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
  flushSync();
  expect(menu()).toBeNull();

  await open();
  face().focus();
  flushSync();
  expect(menu(), "focus going to its opener is the press about to answer").not.toBeNull();
  face().click();
  flushSync();
  expect(menu()).toBeNull();
});

test("a move to another address closes it, so it never stands over a kept page", async () => {
  await draw();
  await open();
  window.dispatchEvent(new HashChangeEvent("hashchange"));
  flushSync();
  expect(menu()).toBeNull();
});

test("a pick that opens a dialog gives the dialog its opener to return to", async () => {
  await draw();
  await open();
  menu()!.querySelector<HTMLButtonElement>(".three")!.focus();
  menu()!.querySelector<HTMLButtonElement>(".three")!.click();
  flushSync();
  await tick();
  const field = target.querySelector<HTMLInputElement>("input[aria-label=Field]")!;
  expect(document.activeElement).toBe(field);
  target.querySelector<HTMLButtonElement>(".tf-dclose")!.click();
  flushSync();
  expect(document.activeElement).toBe(face());
});

test("not ready: focus waits on the menu, whose keys still work", async () => {
  await draw({ ready: false });
  await open();
  expect(document.activeElement).toBe(menu());
  key("Escape");
  expect(menu()).toBeNull();
  expect(document.activeElement).toBe(face());
});

test("opened from a dialog, Esc and Tab close the menu alone and focus stays in the dialog", async () => {
  await draw({ host: true, inDialog: true });
  const inner = target.querySelector<HTMLButtonElement>(".inner")!;
  const dialog = target.querySelector(".tf-dialog")!;
  expect(layers.open).toHaveLength(1);
  for (const name of ["Escape", "Tab"]) {
    inner.click();
    flushSync();
    await tick();
    expect(menu()!.parentElement?.className, "over the dialog, in the host").toBe("tf-floats");
    expect(layers.open, name).toHaveLength(2);
    key(name);
    expect(menu(), name).toBeNull();
    expect(layers.open, `${name}: the dialog stays`).toHaveLength(1);
    expect(document.activeElement, name).toBe(inner);
    expect(dialog.isConnected, name).toBe(true);
  }
});
