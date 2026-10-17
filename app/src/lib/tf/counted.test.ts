import { expect, test } from "vitest";

import { counted } from "./counted";

test("a count of one takes the singular noun, every other count the plural", () => {
  expect(counted(1, "page", "pages")).toBe("1 page");
  expect(counted(0, "page", "pages")).toBe("0 pages");
  expect(counted(2, "page", "pages")).toBe("2 pages");
});
