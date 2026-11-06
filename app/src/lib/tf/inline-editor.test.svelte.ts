/**
 * `inlineEditor`: what an editor drawn in the page owes the reader. Focus goes in,
 * `Esc` reaches it through the layer stack, and focus comes back, also when the
 * button that opened it was replaced by the editor and returns as a new element.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { expect, test, vi } from "vitest";

import { inlineEditor, isSendKey, sendKey } from "./inline-editor";
import { createLayers } from "./layers.svelte";
import Fixture from "./inline-editor.fixture.svelte";

function host(): HTMLDivElement {
  const target = document.createElement("div");
  document.body.append(target);
  return target;
}

test("focus goes to its first field, with the caret after what is there", () => {
  const target = host();
  const app = mount(Fixture, { target, props: { layers: createLayers(), text: "typed" } });
  flushSync();
  target.querySelector<HTMLButtonElement>("[data-open]")!.click();
  flushSync();

  const box = target.querySelector("textarea")!;
  expect(document.activeElement).toBe(box);
  expect(box.selectionStart).toBe(5);

  unmount(app);
  target.remove();
});

test("it sits on the stack as a panel while open, and the stack's close is its own", () => {
  const layers = createLayers();
  const target = host();
  const app = mount(Fixture, { target, props: { layers } });
  flushSync();
  target.querySelector<HTMLButtonElement>("[data-open]")!.click();
  flushSync();

  expect(layers.open.map((layer) => layer.kind)).toEqual(["panel"]);
  layers.open[0]!.close();
  flushSync();
  expect(target.querySelector("textarea")).toBeNull();
  expect(layers.open).toEqual([]);

  unmount(app);
  target.remove();
});

test("inside a dialog it is a dialog layer, so one Esc closes the editor and not what holds it", () => {
  const layers = createLayers();
  const target = host();
  target.className = "tf-dialog";
  const app = mount(Fixture, { target, props: { layers } });
  flushSync();
  target.querySelector<HTMLButtonElement>("[data-open]")!.click();
  flushSync();

  expect(layers.open.map((layer) => layer.kind)).toEqual(["dialog"]);

  unmount(app);
  target.remove();
});

test("focus comes back to the opener a redraw replaced, once the page has redrawn", async () => {
  const target = host();
  const app = mount(Fixture, { target, props: { layers: createLayers() } });
  flushSync();
  const first = target.querySelector<HTMLButtonElement>("[data-open]")!;
  first.focus();
  first.click();
  flushSync();
  expect(target.querySelector("[data-open]")).toBeNull();

  target.querySelector<HTMLButtonElement>("[data-close]")!.click();
  flushSync();
  await tick();

  const again = target.querySelector<HTMLButtonElement>("[data-open]")!;
  expect(again).not.toBe(first);
  expect(document.activeElement).toBe(again);

  unmount(app);
  target.remove();
});

test("focus a reader moved elsewhere stays there", async () => {
  const elsewhere = document.createElement("input");
  document.body.append(elsewhere);
  const target = host();
  const app = mount(Fixture, { target, props: { layers: createLayers() } });
  flushSync();
  target.querySelector<HTMLButtonElement>("[data-open]")!.click();
  flushSync();

  elsewhere.focus();
  target.querySelector<HTMLButtonElement>("[data-close]")!.click();
  flushSync();
  await tick();
  expect(document.activeElement).toBe(elsewhere);

  unmount(app);
  target.remove();
  elsewhere.remove();
});

test("a selector can name what takes focus", () => {
  const node = document.createElement("div");
  node.innerHTML = '<input id="a" /><input id="b" />';
  document.body.append(node);
  const layers = createLayers();
  const close = vi.fn();
  const done = inlineEditor(close, { layers, focus: "#b" })(node);
  expect(document.activeElement?.id).toBe("b");
  if (typeof done === "function") done();
  expect(layers.open).toEqual([]);
  node.remove();
});

function keyed(init: KeyboardEventInit): KeyboardEvent {
  return new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
}

test("\u2318Enter sends what it was told to, in an editor and in a composer of its own", () => {
  const node = document.createElement("div");
  node.innerHTML = "<textarea></textarea>";
  document.body.append(node);
  const send = vi.fn();
  const off = sendKey(send)(node) as () => void;
  node.querySelector("textarea")!.dispatchEvent(keyed({ key: "Enter" }));
  expect(send).not.toHaveBeenCalled();
  node.querySelector("textarea")!.dispatchEvent(keyed({ key: "Enter", metaKey: true }));
  node.querySelector("textarea")!.dispatchEvent(keyed({ key: "Enter", ctrlKey: true }));
  expect(send).toHaveBeenCalledTimes(2);
  off();
  node.remove();
});

test("\u2318Enter with nothing named presses the primary, unless it is off, and doesn't press twice what a handler took", () => {
  const node = document.createElement("div");
  node.innerHTML = '<textarea></textarea><button class="tf-primary">Send</button>';
  document.body.append(node);
  const press = vi.fn();
  node.querySelector("button")!.addEventListener("click", press);
  const off = sendKey()(node) as () => void;
  const box = node.querySelector("textarea")!;
  box.dispatchEvent(keyed({ key: "Enter", metaKey: true }));
  expect(press).toHaveBeenCalledOnce();
  node.querySelector("button")!.disabled = true;
  box.dispatchEvent(keyed({ key: "Enter", metaKey: true }));
  expect(press).toHaveBeenCalledOnce();
  node.querySelector("button")!.disabled = false;
  const took = keyed({ key: "Enter", metaKey: true });
  box.addEventListener("keydown", (event) => event.preventDefault(), { once: true });
  box.dispatchEvent(took);
  expect(isSendKey(took)).toBe(false);
  expect(press).toHaveBeenCalledOnce();
  off();
  node.remove();
});

test("escCloses: one Esc from inside a field closes an editor that keeps its words, and the shell doesn't hear it", () => {
  const node = document.createElement("div");
  node.innerHTML = "<textarea></textarea>";
  document.body.append(node);
  const close = vi.fn();
  const layers = createLayers();
  const off = inlineEditor(close, { layers, escCloses: true })(node) as () => void;
  const shell = vi.fn();
  window.addEventListener("keydown", shell);
  node.querySelector("textarea")!.dispatchEvent(keyed({ key: "Escape" }));
  window.removeEventListener("keydown", shell);
  expect(close).toHaveBeenCalledOnce();
  expect(shell).not.toHaveBeenCalled();
  off();
  node.remove();
});
