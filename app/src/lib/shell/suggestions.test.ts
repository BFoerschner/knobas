/**
 * The two display rules the tray rests on.
 *
 * `classBadge` is the whole of "a similarity suggestion is distinguishable from
 * an exact-key one" on screen (#41 story 16), so the case that matters is not
 * that each class has a word — it is that exactly one of them is marked as a
 * guess.
 */
import { expect, test } from "vitest";

import { classBadge, waitingLabel } from "./suggestions";

test("every class the backend can send has a word", () => {
  expect(classBadge("exact_key")?.label).toBe("exact");
  expect(classBadge("similarity")?.label).toBe("guess");
  expect(classBadge("source_relation")?.label).toBe("recorded");
});

test("only the speculative class is marked as one", () => {
  expect(classBadge("similarity")?.speculative).toBe(true);
  expect(classBadge("exact_key")?.speculative).toBe(false);
  expect(classBadge("source_relation")?.speculative).toBe(false);
});

/**
 * A link a person drew carries no class, and the tray never holds one — but the
 * wire type allows `null`, and a badge that threw on it would take the row with
 * it.
 */
test("a link with no class renders no badge rather than an empty one", () => {
  expect(classBadge(null)).toBeNull();
});

test("the count reads as an answer to 'is it worth looking'", () => {
  expect(waitingLabel(0)).toBe("none waiting");
  expect(waitingLabel(1)).toBe("1 waiting");
  expect(waitingLabel(7)).toBe("7 waiting");
});
