/**
 * The theme editor's square-and-strip picker (`TfColourPicker.svelte`, #2516): a colour it picked that comes back as
 * its value moves nothing, and any other value moves its rings, a colour it picked before included.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test } from "vitest";

import TfColourPicker from "./TfColourPicker.svelte";

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target?.remove();
});

test("a value from elsewhere moves the rings, even one the picker sent before (#2516)", () => {
  const picked: string[] = [];
  const props = $state({ label: "Accent", value: "#FF0000", onpick: (next: string) => void picked.push(next) });
  target = document.body.appendChild(document.createElement("div"));
  view = mount(TfColourPicker, { target, props });
  const strip = target.querySelector<HTMLElement>('[role="slider"][aria-label="Accent: hue"]')!;
  strip.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", shiftKey: true, bubbles: true }));
  flushSync();
  const sent = picked.at(-1)!;
  expect(strip.getAttribute("aria-valuenow")).toBe("10");
  // The pick comes back as the value: the strip stays where the key left it.
  props.value = sent;
  flushSync();
  expect(strip.getAttribute("aria-valuenow")).toBe("10");
  // *Reset*, then the same colour typed into the Hex field: each moves the strip.
  props.value = "#00FF00";
  flushSync();
  expect(strip.getAttribute("aria-valuenow")).toBe("120");
  props.value = sent;
  flushSync();
  expect(strip.getAttribute("aria-valuenow")).toBe("10");
});
