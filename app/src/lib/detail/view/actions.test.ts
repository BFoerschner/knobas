/**
 * What a view's action becomes on the write queue (#667): the payload each
 * declared op is queued as, and the gate that draws an action at all.
 */
import { expect, test } from "vitest";

import type { ViewAction } from "../../ipc/entity";
import { actionName, actionPayload, offeredActions } from "./actions";

const MERGE: ViewAction = {
  write_op: "merge_pull_request",
  label: "Merge…",
  options: ["merge", "rebase", "squash"],
  confirm: "Merge feature/x into main?",
};

test("a merge is queued as MergePullRequest with the style picked", () => {
  for (const style of ["merge", "rebase", "squash"] as const) {
    expect(actionPayload(MERGE, "gitea:o/r#7", style)).toEqual({
      MergePullRequest: { entity: "gitea:o/r#7", style },
    });
  }
});

test("a style the SPI cannot carry, and an op this shell cannot queue, build nothing", () => {
  expect(actionPayload(MERGE, "gitea:o/r#7", "fast-forward-only")).toBeNull();
  expect(actionPayload(MERGE, "gitea:o/r#7", null)).toBeNull();
  expect(actionPayload({ ...MERGE, write_op: "launch_rocket" }, "gitea:o/r#7", "merge")).toBeNull();
});

test("an action is offered only where the source declares its op and the shell can queue it", () => {
  const unknown: ViewAction = { ...MERGE, write_op: "launch_rocket" };
  expect(offeredActions([MERGE, unknown], ["comment", "merge_pull_request", "launch_rocket"])).toEqual([MERGE]);
  expect(offeredActions([MERGE], ["comment", "approve"])).toEqual([]);
});

test("an action's name is its label without the dialog's ellipsis", () => {
  expect(actionName("Merge…")).toBe("Merge");
  expect(actionName("Run...")).toBe("Run");
  expect(actionName("Rerun")).toBe("Rerun");
});
