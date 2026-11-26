/** The window's 51 keys and how the Theme page lists them (ADR-0055, the decisions' 3.3). */
import { expect, test } from "vitest";

import { APP_KEYS, FOLD, HEADINGS, HIDDEN, KEYS, NAME, ROW_KEYS, TOKEN } from "./app-roles";

test("every key has a token, a name and exactly one place: a row, the fold, or hidden", () => {
  expect(KEYS).toHaveLength(53);
  expect(new Set(KEYS).size).toBe(53);
  expect(new Set(APP_KEYS.map(([, token]) => token)).size).toBe(53);
  for (const key of KEYS) {
    expect(TOKEN[key], key).toMatch(/^--[a-z0-9-]+$/);
    expect(NAME[key], key).toBeTruthy();
    const places = [ROW_KEYS.includes(key), FOLD.includes(key), HIDDEN.includes(key)].filter(Boolean);
    expect(places, key).toHaveLength(1);
  }
});

test("the rows hold 32 keys in 23 rows under seven headings, and the fold 19", () => {
  expect(HEADINGS.map((h) => h.name)).toEqual(["Surfaces", "Text", "Lines", "Controls", "Marks", "States", "Current ticket"]);
  expect(HEADINGS.flatMap((h) => h.rows)).toHaveLength(23);
  expect(ROW_KEYS).toHaveLength(32);
  expect(FOLD).toHaveLength(19);
  expect(HIDDEN).toEqual(["cardShade", "cardLift"]);
});
