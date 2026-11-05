import { expect, test } from "vitest";

import { stateWord } from "./state-word";

test("a chip's word is in sentence case, whichever way the source wrote it", () => {
  expect(stateWord("In Progress")).toBe("In progress");
  expect(stateWord("TO DO")).toBe("To do");
  expect(stateWord("merged")).toBe("Merged");
  expect(stateWord("open")).toBe("Open");
});

test("a source's word for a result reads as the filters' word", () => {
  expect(stateWord("SUCCESS")).toBe("Passed");
  expect(stateWord("failure")).toBe("Failed");
  expect(stateWord("Failed")).toBe("Failed");
});

test("an acronym keeps its capitals, and nothing stays nothing", () => {
  expect(stateWord("HTTP down")).toBe("HTTP down");
  expect(stateWord("waiting on CI")).toBe("Waiting on CI");
  expect(stateWord("  ")).toBe("");
});
