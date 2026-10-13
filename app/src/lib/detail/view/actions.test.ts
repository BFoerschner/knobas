/**
 * What a view's action becomes on the write queue (#667): the payload each
 * declared op is queued as, and the gate that draws an action at all; and
 * for a run (#669), which values the dialog sends.
 */
import { expect, test } from "vitest";

import type { ActionField, Setting, ViewAction } from "../../ipc/entity";
import {
  actionName,
  actionPayload,
  changedParameters,
  initialValues,
  missingFields,
  offeredActions,
  opensDialog,
} from "./actions";

function action(write_op: string, label: string, more: Partial<ViewAction> = {}): ViewAction {
  return {
    write_op,
    label,
    fields: [],
    confirm: null,
    primary: false,
    danger: false,
    disabled: null,
    with_settings: false,
    ...more,
  };
}

function field(name: string, kind: ActionField["kind"], more: Partial<ActionField> = {}): ActionField {
  return { name, label: name, kind, value: null, required: false, placeholder: null, ...more };
}

const MERGE = action("merge_pull_request", "Merge…", {
  fields: [
    field(
      "style",
      {
        type: "choice",
        options: [
          { value: "merge", label: "Merge commit" },
          { value: "rebase", label: "Rebase" },
          { value: "squash", label: "Squash" },
        ],
      },
      { required: true },
    ),
  ],
  confirm: "Merge feature/x into main?",
  primary: true,
});

test("a merge is queued as MergePullRequest with the style picked", () => {
  for (const style of ["merge", "rebase", "squash"] as const) {
    expect(actionPayload(MERGE, "gitea:o/r#7", { style })).toEqual({
      MergePullRequest: { entity: "gitea:o/r#7", style },
    });
  }
});

test("a style the SPI cannot carry, and an op this shell cannot queue, build nothing", () => {
  expect(actionPayload(MERGE, "gitea:o/r#7", { style: "fast-forward-only" })).toBeNull();
  expect(actionPayload(MERGE, "gitea:o/r#7", {})).toBeNull();
  expect(actionPayload({ ...MERGE, write_op: "launch_rocket" }, "gitea:o/r#7", { style: "merge" })).toBeNull();
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

test("a dialog starts from the declared values: a choice's first option unless one is named, a flag unchecked unless true", () => {
  const form = action("x", "X", {
    fields: [
      field("title", { type: "text", suggestions: [] }, { value: "PAY-231: retry" }),
      field("body", { type: "long_text" }),
      field("style", { type: "choice", options: [{ value: "a", label: "A" }, { value: "b", label: "B" }] }),
      field("base", { type: "choice", options: [{ value: "main", label: "main" }, { value: "dev", label: "dev" }] }, {
        value: "dev",
      }),
      field("delete_branch", { type: "flag" }, { value: "true" }),
      field("force", { type: "flag" }),
      field("ghost", { type: "choice", options: [] }),
    ],
  });
  expect(initialValues(form)).toEqual({
    title: "PAY-231: retry",
    body: "",
    style: "a",
    base: "dev",
    delete_branch: "true",
    force: "false",
    ghost: "",
  });
});

test("a required field is missing while blank, unpicked or unchecked; an optional one never", () => {
  const form = action("x", "X", {
    fields: [
      field("title", { type: "text", suggestions: [] }, { required: true }),
      field("body", { type: "long_text" }, { required: true }),
      field("style", { type: "choice", options: [{ value: "a", label: "A" }] }, { required: true }),
      field("sure", { type: "flag" }, { required: true }),
      field("note", { type: "text", suggestions: [] }),
    ],
  });
  expect(missingFields(form, { title: "  ", body: "", style: "", sure: "false", note: "" })).toEqual([
    "title",
    "body",
    "style",
    "sure",
  ]);
  expect(missingFields(form, { title: "x", body: "y", style: "a", sure: "true", note: "" })).toEqual([]);
});

test("only an action with nothing to ask and nothing to warn about queues on the button", () => {
  expect(opensDialog(action("x", "X"))).toBe(false);
  expect(opensDialog(action("x", "X", { confirm: "Sure?" }))).toBe(true);
  expect(opensDialog(action("x", "X", { danger: true }))).toBe(true);
  expect(opensDialog(action("x", "X", { with_settings: true }))).toBe(true);
  expect(opensDialog(MERGE)).toBe(true);
});

// --- run_build (#669) -----------------------------------------------------

const RUN = action("run_build", "Run…", {
  fields: [field("branch", { type: "text", suggestions: ["feature/PAY-231-sepa-retry"] }, { placeholder: "default branch" })],
  confirm: "Queue one build of this configuration.",
  with_settings: true,
});
const CONFIG = "teamcity:buildType:Payout_IntegrationTests";

test("a run is queued as RunBuild with its branch and overrides; an empty branch is none", () => {
  expect(
    actionPayload(RUN, CONFIG, { branch: "feature/PAY-231-sepa-retry" }, [{ name: "env.SEPA_RETRIES", value: "9" }]),
  ).toEqual({
    RunBuild: {
      entity: CONFIG,
      branch: "feature/PAY-231-sepa-retry",
      parameters: [{ name: "env.SEPA_RETRIES", value: "9" }],
    },
  });
  for (const values of [{ branch: "" }, { branch: "   " }, {}]) {
    expect(actionPayload(RUN, CONFIG, values)).toEqual({ RunBuild: { entity: CONFIG, branch: null, parameters: [] } });
  }
  expect(actionPayload(RUN, CONFIG, { branch: "  fix/PAY-228 " })).toEqual({
    RunBuild: { entity: CONFIG, branch: "fix/PAY-228", parameters: [] },
  });
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
