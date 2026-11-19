import { expect, test } from "vitest";

import { marksIn } from "./marks";

test("marks move into the title the row draws", () => {
  // As sent: the same title.
  expect(marksIn([7, 12], "acme/ADProxy", "acme/ADProxy")).toEqual([7, 12]);
  // A repository's owner moves to the key cell: the title drawn is its name.
  expect(marksIn([0, 4, 7, 12], "acme/ADProxy", "ADProxy")).toEqual([2, 7]);
  // A pull request's number goes before its title.
  expect(marksIn([0, 5], "Proxy timeouts", "#142 Proxy timeouts")).toEqual([5, 10]);
  // Nothing sent, or a title the row rewrote otherwise: none.
  expect(marksIn(undefined, "a", "a")).toEqual([]);
  expect(marksIn([0, 1], "abc", "xyz")).toEqual([]);
});
