import { afterEach, expect, test } from "vitest";
import { ZEN_KEY, ZEN_KEY_LABEL, isZenKey, zen, zenApplies } from "./zen.svelte";
import { SHORTCUTS } from "./settings/sections";

afterEach(() => {
  zen.on = false;
  zen.here = false;
});

test("Zen mode applies to the renderers that want the room, not to source or a file with no renderer", () => {
  for (const id of ["markdown", "pdf", "jar", "image", "openapi", "mermaid", "d2", "graphviz", "html"]) expect(zenApplies(id)).toBe(true);
  for (const id of ["source", "", null]) expect(zenApplies(id)).toBe(false);
});

test("its key is z alone, either case, and only when no one answered it", () => {
  const key = (init: KeyboardEventInit) => new KeyboardEvent("keydown", { cancelable: true, ...init });
  expect(isZenKey(key({ key: "z" }))).toBe(true);
  expect(isZenKey(key({ key: "Z", shiftKey: true }))).toBe(true);
  for (const extra of [{ metaKey: true }, { ctrlKey: true }, { altKey: true }]) expect(isZenKey(key({ key: "z", ...extra }))).toBe(false);
  expect(isZenKey(key({ key: "y" }))).toBe(false);
  const answered = key({ key: "z" });
  answered.preventDefault();
  expect(isZenKey(answered)).toBe(false);
});

test("it shows only where the reader asked and the file view holds a file it applies to", () => {
  expect(zen.shown).toBe(false);
  zen.toggle();
  expect(zen.shown).toBe(false);
  zen.here = true;
  expect(zen.shown).toBe(true);
  zen.toggle();
  expect(zen.shown).toBe(false);
});

test("Settings › Keyboard names the key from the one place it is kept", () => {
  expect(ZEN_KEY_LABEL).toBe(ZEN_KEY.toUpperCase());
  expect(SHORTCUTS.find(([, , what]) => what.startsWith("Zen mode"))?.[0]).toBe(ZEN_KEY_LABEL);
});
