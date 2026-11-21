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
