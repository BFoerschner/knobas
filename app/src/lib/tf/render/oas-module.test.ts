/** The explorer's component, held once loaded (the ux30 end gate): a spec opened after the first draws in its own task. */
import { expect, test } from "vitest";

import { explorerModule } from "./oas-module";

test("the explorer's module is a promise the first time, then the module itself", async () => {
  const first = explorerModule();
  expect(first).toBeInstanceOf(Promise);
  const loaded = await first;
  const again = explorerModule();
  expect(again).not.toBeInstanceOf(Promise);
  expect(again).toBe(loaded);
});
