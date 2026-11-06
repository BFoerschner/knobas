import { expect, test } from "vitest";

import { withHolds } from "./row-hold";

const hold = (at: number, entry: string) => ({ at, entry });

test("each hold goes back at its index in what was drawn, whichever was pressed first (F-159)", () => {
  expect(withHolds(["d"], [hold(2, "c"), hold(0, "a"), hold(1, "b")])).toEqual(["a", "b", "c", "d"]);
  expect(withHolds(["a", "c"], [hold(1, "b"), hold(3, "d")])).toEqual(["a", "b", "c", "d"]);
});

test("a row a failed answer brings back takes its own place, and the holds around it keep theirs (F-159)", () => {
  // a b c d drawn, b and c held. b comes back to the list, so only c still holds.
  expect(withHolds(["a", "b", "d"], [hold(2, "c")])).toEqual(["a", "b", "c", "d"]);
  // c comes back instead.
  expect(withHolds(["a", "c", "d"], [hold(1, "b")])).toEqual(["a", "b", "c", "d"]);
});

test("nothing held is the list itself, and a hold past the end stands last", () => {
  const list = ["a"];
  expect(withHolds(list, [])).toBe(list);
  expect(withHolds(list, [hold(5, "b")])).toEqual(["a", "b"]);
});
