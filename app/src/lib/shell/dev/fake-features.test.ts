/**
 * The fixture's feature modules (#908): each one under `fake/` exports its
 * `handlers`, and no two of them answer the same command.
 */
import { expect, test } from "vitest";

import { demoHandlers } from "./fake-tauri";

const MODULES = import.meta.glob<{ handlers?: unknown }>("./fake/*.ts", { eager: true });

test("every module under fake/ is a feature with handlers", () => {
  const paths = Object.keys(MODULES);
  expect(paths.length).toBeGreaterThan(10);
  for (const [path, module] of Object.entries(MODULES)) {
    expect(typeof module.handlers, `${path} exports no handlers`).toBe("function");
  }
});

test("the features answer disjoint commands, all of which demoHandlers serves", () => {
  const params = new URLSearchParams();
  const counted = Object.values(MODULES)
    .map((module) => Object.keys((module.handlers as (p: URLSearchParams) => object)(params)).length)
    .reduce((sum, count) => sum + count, 0);
  expect(Object.keys(demoHandlers(params))).toHaveLength(counted);
});
