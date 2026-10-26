/** What ⌘R presses when a page draws more than one button (#2156). */
import { expect, test } from "vitest";

import { refreshKey } from "../shell/keys";
import { createPageRefresh } from "./page-refresh.svelte";

function pressed() {
  const log: string[] = [];
  return { log, as: (name: string) => () => void log.push(name) };
}

test("a detail's button wins over the role view's header, whichever registered first", () => {
  const registry = createPageRefresh();
  const { log, as } = pressed();
  registry.add({ kind: "detail", press: as("run") });
  registry.add({ kind: "role", press: as("builds") });
  registry.press();
  expect(log).toEqual(["run"]);
});

test("the role view's header is pressed once the detail's button is gone, and nothing once both are", () => {
  const registry = createPageRefresh();
  const { log, as } = pressed();
  const offRole = registry.add({ kind: "role", press: as("docs") });
  const offDetail = registry.add({ kind: "detail", press: as("page") });
  offDetail();
  registry.press();
  offRole();
  registry.press();
  expect(log).toEqual(["docs"]);
});

test("of two of a kind, the newest is pressed", () => {
  const registry = createPageRefresh();
  const { log, as } = pressed();
  registry.add({ kind: "detail", press: as("pipeline") });
  registry.add({ kind: "detail", press: as("run") });
  registry.press();
  expect(log).toEqual(["run"]);
});

test("⌘R is R with ⌘ or Ctrl, ⇧ and ⌥ too, though ⌥R types ® on the Mac", () => {
  const key = (init: KeyboardEventInit) => refreshKey(new KeyboardEvent("keydown", init));
  expect(key({ key: "r", metaKey: true })).toBe(true);
  expect(key({ key: "r", ctrlKey: true })).toBe(true);
  expect(key({ key: "R", metaKey: true, shiftKey: true })).toBe(true);
  expect(key({ key: "®", code: "KeyR", metaKey: true, altKey: true })).toBe(true);
  expect(key({ key: "‰", code: "KeyR", metaKey: true, altKey: true, shiftKey: true })).toBe(true);
  expect(key({ key: "r" })).toBe(false);
  expect(key({ key: "®", code: "KeyR", altKey: true })).toBe(false);
  expect(key({ key: "p", code: "KeyR", metaKey: true })).toBe(false);
});
