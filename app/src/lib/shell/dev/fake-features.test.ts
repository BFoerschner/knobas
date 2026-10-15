/**
 * The fixture's feature modules (#908): each one under `fake/` exports its
 * `handlers`, and no two of them answer the same command.
 */
import { expect, test } from "vitest";

import { demoHandlers } from "./fake-tauri";

const MODULES = import.meta.glob<{ handlers?: unknown }>("./fake/*.ts", { eager: true });

test("every module under fake/ is a feature with handlers", () => {
  // The glob found the modules at all, so the loop below is not vacuous.
  expect(Object.keys(MODULES)).toContain("./fake/corpus.ts");
  for (const [path, module] of Object.entries(MODULES)) {
    expect(typeof module.handlers, `${path} exports no handlers`).toBe("function");
  }
});

test("no two features answer one command, and none answers nothing", () => {
  const params = new URLSearchParams();
  const counts = Object.values(MODULES).map(
    (module) => Object.keys((module.handlers as (p: URLSearchParams) => object)(params)).length,
  );
  expect(counts).not.toContain(0);
  const counted = counts.reduce((sum, count) => sum + count, 0);
  expect(Object.keys(demoHandlers(params))).toHaveLength(counted);
});
