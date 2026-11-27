import { expect, test } from "vitest";

import { counted } from "./counted";

test("a count of one takes the singular noun, every other count the plural", () => {
  expect(counted(1, "page", "pages")).toBe("1 page");
  expect(counted(0, "page", "pages")).toBe("0 pages");
  expect(counted(2, "page", "pages")).toBe("2 pages");
});

test("thousands groups a whole number as en-US does", async () => {
  const { thousands } = await import("./counted");
  for (const n of [0, 7, 999, 1000, 12345, 999999, 1000000, 1234567890, -4321]) expect(thousands(n)).toBe(n.toLocaleString("en-US"));
});

test("a count can draw its figure grouped, the noun still in the number it earns (#2522)", async () => {
  const { thousands } = await import("./counted");
  expect(counted(12_345, "line", "lines", thousands)).toBe("12,345 lines");
  expect(counted(1, "file", "files", thousands)).toBe("1 file");
});
