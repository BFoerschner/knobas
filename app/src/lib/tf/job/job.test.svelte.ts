import { expect, test } from "vitest";
import { weekLine } from "./job";

test("durations are whole minutes, floored: 163 s logged as 120 s leaves 43 s that are not a minute to log", () => {
  expect(weekLine({ days: [], logged: 120, unlogged: 43 })).toBe("2m logged");
  expect(weekLine({ days: [], logged: 0, unlogged: 43 })).toBe("Nothing yet");
});
