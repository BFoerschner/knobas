/** What ⌘R presses when a page draws more than one button (#2156). */
import { expect, test } from "vitest";

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
