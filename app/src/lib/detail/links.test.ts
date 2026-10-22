/** The sentence a refused link write shows, on its own, since both surfaces show it. */
import { expect, test } from "vitest";

import { linkFailureMessage } from "./links";

test("only conflict is rewritten", () => {
  expect(linkFailureMessage({ code: "conflict", message: "whatever", source_id: null })).toBe(
    "Already linked — this pair already carries that relation.",
  );
  expect(linkFailureMessage({ code: "invalid", message: "not an entity id", source_id: null })).toBe(
    "not an entity id",
  );
});

/** A rejection that is not an `IpcError` at all still produces a sentence. */
test("a rejection that is not an IpcError is still reported", () => {
  expect(linkFailureMessage(new Error("the bridge is not there"))).toBe("the bridge is not there");
});
