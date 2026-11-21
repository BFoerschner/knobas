import { afterEach, expect, test, vi } from "vitest";

import { forgetPageMakers, knownPageMakers, pageMakers } from "./page-makers";

afterEach(() => forgetPageMakers());

test("the sources that can create pages are read once a session and kept for the next mount", async () => {
  const read = vi.fn(async () => ["live-confluence"]);
  expect(knownPageMakers()).toBeNull();
  expect(await pageMakers(read)).toEqual(["live-confluence"]);
  expect(await pageMakers(read)).toEqual(["live-confluence"]);
  expect(read).toHaveBeenCalledTimes(1);
  expect([...knownPageMakers()!]).toEqual(["live-confluence"]);
});

test("a failed read is forgotten, so the next mount asks again", async () => {
  const read = vi.fn().mockRejectedValueOnce(new Error("no bridge")).mockResolvedValueOnce(["c"]);
  await expect(pageMakers(read)).rejects.toThrow("no bridge");
  expect(knownPageMakers()).toBeNull();
  expect(await pageMakers(read)).toEqual(["c"]);
  expect(read).toHaveBeenCalledTimes(2);
});
