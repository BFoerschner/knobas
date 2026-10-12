/**
 * What a view's action becomes on the write queue (#667): the payload each
 * declared op is queued as, and the gate that draws an action at all; and
 * for a run (#669), which values the dialog sends.
 */
import { expect, test } from "vitest";

import type { Setting, ViewAction } from "../../ipc/entity";
import { actionForm, actionName, actionPayload, changedParameters, offeredActions } from "./actions";

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

// --- run_build (#669) -----------------------------------------------------

const RUN: ViewAction = {
  write_op: "run_build",
  label: "Run…",
  options: ["feature/PAY-231-sepa-retry"],
  confirm: "Queue one build of this configuration.",
};
const CONFIG = "teamcity:buildType:Payout_IntegrationTests";

test("a run is queued as RunBuild with its branch and overrides; an empty branch is none", () => {
  expect(actionPayload(RUN, CONFIG, "feature/PAY-231-sepa-retry", [{ name: "env.SEPA_RETRIES", value: "9" }])).toEqual({
    RunBuild: {
      entity: CONFIG,
      branch: "feature/PAY-231-sepa-retry",
      parameters: [{ name: "env.SEPA_RETRIES", value: "9" }],
    },
  });
  for (const empty of ["", "   ", null]) {
    expect(actionPayload(RUN, CONFIG, empty)).toEqual({ RunBuild: { entity: CONFIG, branch: null, parameters: [] } });
  }
  expect(actionPayload(RUN, CONFIG, "  fix/PAY-228 ")).toEqual({
    RunBuild: { entity: CONFIG, branch: "fix/PAY-228", parameters: [] },
  });
});

test("a run's branch is typed with its options as suggestions; a merge picks one of its options", () => {
  expect(actionForm(RUN)).toEqual({ choice: "suggest", parameters: true });
  expect(actionForm(MERGE)).toEqual({ choice: "pick", parameters: false });
});

function setting(name: string, value: string, more: Partial<Setting> = {}): Setting {
  return { name, value, inherited: false, secret: false, editable: true, description: null, ...more };
}

test("only a value the reader changed is an override, in the order drawn", () => {
  const values = [
    setting("env.SEPA_RETRIES", "5"),
    setting("tidewater.region", "eu-central-1", { inherited: true }),
    setting("env.EMPTY", ""),
  ];
  expect(changedParameters(values, {})).toEqual([]);
  expect(
    changedParameters(values, { "env.SEPA_RETRIES": "5", "tidewater.region": "eu-west-1", "env.EMPTY": "" }),
  ).toEqual([{ name: "tidewater.region", value: "eu-west-1" }]);
  // Cleared is a change too: the run takes an empty value.
  expect(changedParameters(values, { "env.SEPA_RETRIES": "", "env.EMPTY": "x" })).toEqual([
    { name: "env.SEPA_RETRIES", value: "" },
    { name: "env.EMPTY", value: "x" },
  ]);
});

test("a secret is sent only when the reader typed a value; a value the source would not let change never", () => {
  const values = [
    setting("env.SEPA_SANDBOX_KEY", "", { secret: true }),
    setting("vcs.mirror", "https://git.example/p.git", { editable: false }),
  ];
  expect(changedParameters(values, { "env.SEPA_SANDBOX_KEY": "" })).toEqual([]);
  expect(changedParameters(values, { "env.SEPA_SANDBOX_KEY": "typed", "vcs.mirror": "https://other.example/p.git" })).toEqual([
    { name: "env.SEPA_SANDBOX_KEY", value: "typed" },
  ]);
});
