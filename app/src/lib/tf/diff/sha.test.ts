/**
 * The shell's sha checks, one place (review of spec #2234, Standards 10),
 * mirroring `knobas_core::checkout::open::{is_sha, is_full_sha}`: a sha is 7
 * to 64 hex digits, a whole one exactly 40 or 64; a diff's base may end in
 * one `^`.
 */
import { expect, test } from "vitest";

import { isFullSha, isSha, shortEnd } from "./sha";

test("a sha is 7 to 64 hex digits, a whole one exactly 40 or 64", () => {
  expect([isSha("abc1234"), isSha("ABC1234"), isSha("abc123"), isSha("a".repeat(65)), isSha("abc123g")]).toEqual([true, true, false, false, false]);
  expect([isFullSha("a".repeat(40)), isFullSha("a".repeat(64)), isFullSha("a".repeat(41)), isFullSha("abc1234")]).toEqual([true, true, false, false]);
});

test("a whole sha end is shortened as the source shortens it, its caret kept; anything else is as written", () => {
  const sha = "0123456789abcdef0123456789abcdef01234567";
  expect(shortEnd(sha)).toBe("0123456789");
  expect(shortEnd(`${sha}^`)).toBe("0123456789^");
  expect(shortEnd("main")).toBe("main");
  expect(shortEnd("abc1234^")).toBe("abc1234^");
});
