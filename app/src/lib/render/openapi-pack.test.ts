/**
 * An open's answer as it crosses from the spec worker (the ux30 end gate): the operation rows as columns, made rows
 * again on the page's side, the same as the model made them.
 */
import { expect, test } from "vitest";

import { SpecFiles, type Opened } from "./openapi";
import { bigSpec, MODEL_31_JSON, PETS_30_YAML } from "./openapi-fixture";
import { packOpened, unpackOpened } from "./openapi-pack";

test("an index's rows cross as columns, not one object each, and come back the same", () => {
  for (const [path, text] of [["api/openapi.yaml", PETS_30_YAML], ["api/model.json", MODEL_31_JSON], ["api/big.yaml", bigSpec(500)]] as const) {
    const opened = new SpecFiles(path).open(text);
    expect(opened.ok).toBe(true);
    const packed = packOpened(opened) as unknown as { index: Record<string, unknown>; cols: Record<string, unknown[]> };
    expect(packed.index.ops).toBeUndefined();
    expect(Object.values(packed.cols).every((col) => col.every((cell) => cell === null || typeof cell !== "object"))).toBe(true);
    // As the worker's message carries it: cloned.
    expect(unpackOpened(structuredClone(packed))).toEqual(opened);
  }
});

test("an operation's tags come back as they were: none, one with no name, several", () => {
  const opened = new SpecFiles("t.yaml").open(
    ["openapi: 3.0.3", "info: { title: T, version: '1' }", "paths:", "  /a:", "    get: { responses: {} }", "    put: { tags: [''], responses: {} }", "    post: { tags: [x, '', y], responses: {} }"].join("\n"),
  );
  const back = unpackOpened(structuredClone(packOpened(opened))) as Opened;
  expect(back.ok && back.index.ops.map((op) => op.tags)).toEqual([[], [""], ["x", "", "y"]]);
});

test("an answer that is not an index passes as it is", () => {
  const bad: Opened = { ok: false, error: "no", line: 3 };
  expect(packOpened(bad)).toBe(bad);
  expect(unpackOpened(bad)).toBe(bad);
  expect(unpackOpened(null)).toBeNull();
});
