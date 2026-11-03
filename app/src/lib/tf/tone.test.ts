/** The one tone class a state chip, dot or editor chip draws with (spec #2463). */
import { expect, test } from "vitest";

import { toneClass } from "./tone";

test("a state's tone is its class; no tone, or no state, is the plain neutral chip or muted dot", () => {
  expect(toneClass({ tone: "err" })).toBe("tf-err");
  expect(toneClass({ tone: "ok" })).toBe("tf-ok");
  expect(toneClass({ tone: null })).toBe("");
  expect(toneClass(null)).toBe("");
  expect(toneClass(undefined)).toBe("");
});
