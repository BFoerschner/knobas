/**
 * **The `?fake-ipc` backend starts each test from its seed** (#2004).
 *
 * The fake keeps what it was told in module state, as a server would, and a
 * test file loads it once. `test-setup.ts` runs `resetFake()` after every test
 * (through `test-resets.ts`), so the tests below are ordered on purpose: the
 * first writes, the second reads what a freshly loaded fake answers.
 */
import { expect, test } from "vitest";

import { demoHandlers } from "./fake-tauri";
import { FIXTURE_NOTE_ID } from "./fake/notes";

type Calls = Record<string, (args: Record<string, unknown>) => unknown>;

test("a test writes to the fake", () => {
  const h = demoHandlers() as Calls;
  expect(h["delete_note"]!({ noteId: FIXTURE_NOTE_ID })).toBe(true);
});

test("the next test reads the fake's seed", () => {
  const h = demoHandlers() as Calls;
  expect(h["get_note"]!({ noteId: FIXTURE_NOTE_ID })).toMatchObject({ note: { title: "Retry backoff, thinking aloud" } });
});
