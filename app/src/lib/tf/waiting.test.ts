import { expect, test } from "vitest";

import { waitPhase } from "./waiting";

test("a first read with nothing drawn yet is skeleton rows, before it starts and while it runs (#2246)", () => {
  expect(waitPhase({ drawn: false, reading: false, failed: false })).toBe("skeleton");
  expect(waitPhase({ drawn: false, reading: true, failed: false })).toBe("skeleton");
});

test("a read that replaces what is drawn keeps it, dimmed, until the answer (#2246)", () => {
  expect(waitPhase({ drawn: true, reading: true, failed: false })).toBe("dimmed");
  expect(waitPhase({ drawn: true, reading: false, failed: false })).toBe("shown");
});

test("a failed read is the error, over dimmed content and over skeletons alike (#2246)", () => {
  expect(waitPhase({ drawn: true, reading: false, failed: true })).toBe("failed");
  expect(waitPhase({ drawn: false, reading: false, failed: true })).toBe("failed");
  expect(waitPhase({ drawn: true, reading: true, failed: true })).toBe("failed");
});
