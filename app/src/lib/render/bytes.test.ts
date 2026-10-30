/** Bytes as base64, the one encoder the renderers share (review of spec #2234, Standards 2). */
import { expect, test } from "vitest";

import { base64Of } from "./bytes";

test("bytes become base64, past the chunk the encoder works in", () => {
  expect(base64Of(new TextEncoder().encode("é"))).toBe("w6k=");
  expect(base64Of(new Uint8Array(0))).toBe("");
  const long = new Uint8Array(0x8000 * 2 + 2).fill(0xff);
  expect(atob(base64Of(long)).length).toBe(long.length);
  expect(base64Of(long).slice(-4)).toBe("////");
});
