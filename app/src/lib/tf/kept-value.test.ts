import { expect, test } from "vitest";

import { keptValue, type KeptStorage } from "./kept-value";

function memory(): KeptStorage & { held: Map<string, string> } {
  const held = new Map<string, string>();
  return {
    held,
    getItem: (key) => held.get(key) ?? null,
    setItem: (key, value) => void held.set(key, value),
    removeItem: (key) => void held.delete(key),
  };
}

const blocked: KeptStorage = {
  getItem: () => {
    throw new Error("SecurityError");
  },
  setItem: () => {
    throw new Error("QuotaExceededError");
  },
  removeItem: () => {
    throw new Error("SecurityError");
  },
};

test("a kept value reads back what was written under its key, and nothing once cleared", () => {
  const storage = memory();
  const kept = keptValue(() => storage, "knobas.test");
  expect(kept.read()).toBeNull();
  kept.write("455");
  expect(storage.held.get("knobas.test")).toBe("455");
  expect(kept.read()).toBe("455");
  kept.clear();
  expect(kept.read()).toBeNull();
});

test("storage that throws or is missing reads as nothing kept and never throws to the caller", () => {
  for (const storage of [blocked, null]) {
    const kept = keptValue(() => storage, "knobas.test");
    expect(kept.read()).toBeNull();
    expect(() => kept.write("1")).not.toThrow();
    expect(() => kept.clear()).not.toThrow();
  }
});
