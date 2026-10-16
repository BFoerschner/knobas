import { expect, test } from "vitest";

import { homePath, keyCaps, notesLine } from "./notes";

const NOW = new Date("2026-10-16T10:00:00Z");

test("the notes line counts the notes and dates the last one", () => {
  expect(notesLine(37, "2026-08-12T09:00:00Z", NOW)).toBe("37 notes, the last from 12 August");
  expect(notesLine(1, "2026-08-12T09:00:00Z", NOW)).toBe("1 note, from 12 August");
  expect(notesLine(3, "2025-12-01T09:00:00Z", NOW)).toBe("3 notes, the last from 1 December 2025");
  expect(notesLine(0, null, NOW)).toBe("No notes");
});

test("an accelerator draws as key caps in the order macOS draws them", () => {
  expect(keyCaps("CmdOrCtrl+Alt+N")).toEqual(["⌥", "⌘", "N"]);
  expect(keyCaps("Shift+CmdOrCtrl+space")).toEqual(["⇧", "⌘", "SPACE"]);
});

test("a folder under the home directory is written from ~", () => {
  expect(homePath("/Users/mara/Downloads/knobas-notes")).toBe("~/Downloads/knobas-notes");
  expect(homePath("/home/mara/Downloads/knobas-notes")).toBe("~/Downloads/knobas-notes");
  expect(homePath("/Volumes/x/knobas-notes")).toBe("/Volumes/x/knobas-notes");
});
