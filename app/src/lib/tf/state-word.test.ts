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
  expect(stateWord("warn")).toBe("Warning");
});

test("an acronym keeps its capitals, and nothing stays nothing", () => {
  expect(stateWord("HTTP down")).toBe("HTTP down");
  expect(stateWord("waiting on CI")).toBe("Waiting on CI");
  expect(stateWord("  ")).toBe("");
});

test("a capital word among other words is an acronym, and a shouted state keeps only the known ones", () => {
  expect(stateWord("Ready for SRE")).toBe("Ready for SRE");
  expect(stateWord("In QA")).toBe("In QA");
  expect(stateWord("READY FOR QA")).toBe("Ready for QA");
  expect(stateWord("READY FOR SRE")).toBe("Ready for sre");
});
