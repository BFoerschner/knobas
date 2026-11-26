/** Colour-blind mode (Björn, 2026-11-22): the pairs of states that read alike, named with their roles and the vision. */
import { expect, test } from "vitest";

import { confusions, createColourBlind, COLOUR_BLIND_KEY } from "./colour-blind.svelte";
import type { KeptStorage } from "./kept-value";
import { KNOBAS } from "./theme/app-derive";
import { BUILTINS } from "./theme/builtins";
import { derive } from "./theme/app-derive";

test("knobas Light's Attention and Failed read the same under deuteranopia, and the sentence names both roles", () => {
  const pairs = confusions(KNOBAS.light);
  const deutan = pairs.find((p) => p.vision === "deutan" && p.a === "warn" && p.b === "err")!;
  expect(deutan.sentence).toBe("Attention and Failed read the same under deuteranopia.");
  expect(deutan.delta).toBeCloseTo(0.018, 3);
  // Under the mark of 0.08 but not by much: Passed and Failed, 0.046 apart under deuteranopia, are named too.
  const okErr = pairs.find((p) => p.vision === "deutan" && p.a === "ok" && p.b === "err")!;
  expect(okErr.sentence).toBe("Passed and Failed read the same under deuteranopia.");
  expect(okErr.delta).toBeGreaterThan(0.04);
  // Closest first.
  expect(pairs.map((p) => p.delta)).toEqual([...pairs.map((p) => p.delta)].sort((a, b) => a - b));
});

test("knobas for colour blindness has no state pair that reads alike, under any of the three", () => {
  for (const id of ["knobas-light-cb", "knobas-dark-cb"]) {
    const theme = BUILTINS.find((t) => t.id === id)!;
    // Unknown too: the mode names a pair with it, so the theme it offers has none (the final suite, 2026-11-22).
    expect(confusions(derive(theme.kind, theme.app).colours), id).toEqual([]);
  }
});

test("the mode is off until turned on, and kept", () => {
  const data = new Map<string, string>();
  const storage: KeptStorage = { getItem: (k) => data.get(k) ?? null, setItem: (k, v) => void data.set(k, v), removeItem: (k) => void data.delete(k) };
  const mode = createColourBlind(() => storage);
  expect(mode.on).toBe(false);
  mode.set(true);
  expect(data.get(COLOUR_BLIND_KEY)).toBe("on");
  expect(createColourBlind(() => storage).on).toBe(true);
});
