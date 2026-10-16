/**
 * What a view's action becomes on the write queue (#667): the payload each
 * declared op is queued as, and the gate that draws an action at all; and
 * for a run (#669), which values the dialog sends.
 */
import { expect, test } from "vitest";

import type { ActionField, Setting, ViewAction } from "../../ipc/entity";
import type { ReviewComment } from "../../ipc/sources";
import {
  actionName,
  actionPayload,
  answeredAsDrawn,
  changedParameters,
  initialValues,
  missingFields,
  offeredActions,
  opensDialog,
  queueable,
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
    values: {},
    entity: null,
    line_comments: "none",
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
      MergePullRequest: { entity: "gitea:o/r#7", style, delete_branch: false },
    });
  }
});

test("a merge deletes its branch only when the flag came back true (#708)", () => {
  const merge = (delete_branch?: string) =>
    actionPayload(MERGE, "gitea:o/r#7", delete_branch === undefined ? { style: "squash" } : { style: "squash", delete_branch });
  expect(merge("true")).toEqual({ MergePullRequest: { entity: "gitea:o/r#7", style: "squash", delete_branch: true } });
  expect(merge("false")).toEqual({ MergePullRequest: { entity: "gitea:o/r#7", style: "squash", delete_branch: false } });
  // A Merge… drawn without the flag keeps the branch, as every merge did before.
  expect(merge()).toEqual({ MergePullRequest: { entity: "gitea:o/r#7", style: "squash", delete_branch: false } });
});

test("a squash carries the reader's message; any other style, or a blank one, keeps the source's (#843)", () => {
  const merge = (style: string, message: string) => actionPayload(MERGE, "gitea:o/r#7", { style, message });
  expect(merge("squash", "  Idempotent retry window (#142)\n\nRetries on 503. ")).toEqual({
    MergePullRequest: {
      entity: "gitea:o/r#7",
      style: "squash",
      delete_branch: false,
      message: "Idempotent retry window (#142)\n\nRetries on 503.",
    },
  });
  expect(merge("merge", "Idempotent retry window (#142)")).toEqual({
    MergePullRequest: { entity: "gitea:o/r#7", style: "merge", delete_branch: false },
  });
  expect(merge("squash", "   ")).toEqual({
    MergePullRequest: { entity: "gitea:o/r#7", style: "squash", delete_branch: false },
  });
});

test("Update branch… is queued as UpdatePullRequestBranch with merge or rebase, and nothing else (#708)", () => {
  const update = action("update_pull_request_branch", "Update branch…");
  for (const style of ["merge", "rebase"] as const) {
    expect(actionPayload(update, "gitea:o/r#7", { style })).toEqual({
      UpdatePullRequestBranch: { entity: "gitea:o/r#7", style },
    });
  }
  expect(actionPayload(update, "gitea:o/r#7", { style: "squash" })).toBeNull();
  expect(actionPayload(update, "gitea:o/r#7", {})).toBeNull();
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

// -- Comment… and the review actions (#705) ----------------------------------

const COMMENT = action("comment", "Comment…", {
  fields: [field("body", { type: "long_text" }, { required: true })],
});

test("Comment… is queued as Comment with the words typed, and nothing without words", () => {
  expect(actionPayload(COMMENT, "gitea:o/r#7", { body: "bounded in c90d11\n" })).toEqual({
    Comment: { entity: "gitea:o/r#7", body: "bounded in c90d11\n" },
  });
  expect(actionPayload(COMMENT, "gitea:o/r#7", { body: "  \n" })).toBeNull();
  expect(actionPayload(COMMENT, "gitea:o/r#7", {})).toBeNull();
});

function review(verdict: string, label: string, required: boolean): ViewAction {
  return action("submit_review", label, {
    fields: [field("body", { type: "long_text" }, { required })],
    values: { verdict },
  });
}

test("Approve… and Request changes… queue SubmitReview with the verdict the source fixed", () => {
  expect(actionPayload(review("approve", "Approve…", false), "gitea:o/r#7", { body: "" })).toEqual({
    SubmitReview: { entity: "gitea:o/r#7", verdict: "approve", body: "", comments: [] },
  });
  expect(
    actionPayload(review("request_changes", "Request changes…", true), "gitea:o/r#7", { body: "bound it" }),
  ).toEqual({
    SubmitReview: { entity: "gitea:o/r#7", verdict: "request_changes", body: "bound it", comments: [] },
  });
  // The label is the reader's; the verdict is read from the fixed value alone.
  expect(actionPayload(review("approve", "Request changes…", true), "gitea:o/r#7", { body: "x" })).toMatchObject({
    SubmitReview: { verdict: "approve" },
  });
});

test("a verdict the SPI cannot carry, or none, builds nothing", () => {
  expect(actionPayload(review("APPROVED", "Approve…", false), "gitea:o/r#7", { body: "" })).toBeNull();
  expect(actionPayload(action("submit_review", "Review…"), "gitea:o/r#7", { body: "x" })).toBeNull();
});

test("the fixed values lie under the dialog's: a value the reader gave under the same name wins", () => {
  const both = action("submit_review", "Review…", { values: { verdict: "comment", body: "fixed" } });
  expect(actionPayload(both, "gitea:o/r#7", { body: "typed" })).toEqual({
    SubmitReview: { entity: "gitea:o/r#7", verdict: "comment", body: "typed", comments: [] },
  });
});

// -- a branch's life (#707) -------------------------------------------------

const BRANCH = "gitea:o/r@refs/heads/feature/x";

test("a new branch is created in the repository the action names, from the ref picked", () => {
  const fromBranch = action("create_branch", "New branch…", { entity: "gitea:o/r" });
  expect(actionPayload(fromBranch, BRANCH, { name: " feature/y ", from_ref: "feature/x" })).toEqual({
    CreateBranch: { entity: "gitea:o/r", name: "feature/y", from_ref: "feature/x" },
  });
  // On the repository's own view the action names no other entity.
  const onRepo = action("create_branch", "New branch…");
  expect(actionPayload(onRepo, "gitea:o/r", { name: "feature/y", from_ref: "main" })).toEqual({
    CreateBranch: { entity: "gitea:o/r", name: "feature/y", from_ref: "main" },
  });
  expect(actionPayload(onRepo, "gitea:o/r", { name: "  ", from_ref: "main" })).toBeNull();
  expect(actionPayload(onRepo, "gitea:o/r", { name: "y", from_ref: "" })).toBeNull();
});

test("a pull request is opened in the named repository with its title, description, head and base", () => {
  const open = action("create_pull_request", "Open pull request…", { entity: "gitea:o/r" });
  expect(
    actionPayload(open, BRANCH, { title: " SEPA retry ", body: "- one\n- two", head: "feature/x", base: "main" }),
  ).toEqual({
    CreatePullRequest: { entity: "gitea:o/r", title: "SEPA retry", body: "- one\n- two", head: "feature/x", base: "main" },
  });
  // A description may be empty; a title, a head and a base may not.
  expect(actionPayload(open, BRANCH, { title: "t", head: "feature/x", base: "main" })).toEqual({
    CreatePullRequest: { entity: "gitea:o/r", title: "t", body: "", head: "feature/x", base: "main" },
  });
  for (const missing of ["title", "head", "base"]) {
    const values: Record<string, string> = { title: "t", body: "", head: "feature/x", base: "main", [missing]: " " };
    expect(actionPayload(open, BRANCH, values), missing).toBeNull();
  }
});

test("a branch is deleted as itself", () => {
  const remove = action("delete_branch", "Delete branch…", { danger: true, confirm: "Delete feature/x?" });
  expect(actionPayload(remove, BRANCH, {})).toEqual({ DeleteBranch: { entity: BRANCH } });
});

// --- edit_pull_request (#706) ---------------------------------------------

const PR = "gitea:o/r#7";
const EDIT = action("edit_pull_request", "Edit…", {
  fields: [
    field("title", { type: "text", suggestions: [] }, { value: "SEPA retry", required: true }),
    field("body", { type: "long_text" }, { value: "Refs PAY-231\r\n\r\nBounded." }),
  ],
});
const CLOSE = action("edit_pull_request", "Close", {
  confirm: "Close feature/x → main without merging it?",
  danger: true,
  values: { state: "closed" },
});
const REOPEN = action("edit_pull_request", "Reopen", { values: { state: "open" } });

test("an edit sends only the fields the reader changed from what was drawn", () => {
  expect(actionPayload(EDIT, PR, { title: "SEPA retry, bounded", body: "Refs PAY-231\r\n\r\nBounded." })).toEqual({
    EditPullRequest: { entity: PR, title: "SEPA retry, bounded", body: null, state: null },
  });
  expect(actionPayload(EDIT, PR, { title: "SEPA retry", body: "Refs PAY-231" })).toEqual({
    EditPullRequest: { entity: PR, title: null, body: "Refs PAY-231", state: null },
  });
  expect(actionPayload(EDIT, PR, { title: "New", body: "" })).toEqual({
    EditPullRequest: { entity: PR, title: "New", body: "", state: null },
  });
});

test("a description the text area gave back with its line ends rewritten is not a change", () => {
  // A text area holds `\n` whatever Gitea sent; an untouched `\r\n` body
  // must not go back and overwrite somebody else's edit (#706 ruling).
  expect(actionPayload(EDIT, PR, { title: "Retitled", body: "Refs PAY-231\n\nBounded." })).toEqual({
    EditPullRequest: { entity: PR, title: "Retitled", body: null, state: null },
  });
});

test("an edit that changes nothing, or blanks the title, builds nothing", () => {
  expect(actionPayload(EDIT, PR, { title: "SEPA retry", body: "Refs PAY-231\r\n\r\nBounded." })).toBeNull();
  expect(actionPayload(EDIT, PR, { title: "  ", body: "x" })).toBeNull();
});

test("Close and Reopen send only the state their action fixes", () => {
  expect(actionPayload(CLOSE, PR, {})).toEqual({
    EditPullRequest: { entity: PR, title: null, body: null, state: "closed" },
  });
  expect(actionPayload(REOPEN, PR, {})).toEqual({
    EditPullRequest: { entity: PR, title: null, body: null, state: "open" },
  });
  expect(actionPayload({ ...REOPEN, values: { state: "merged" } }, PR, {})).toBeNull();
});

test("a dialog answered exactly as drawn is told apart from one that cannot be sent", () => {
  expect(answeredAsDrawn(EDIT, { title: "SEPA retry", body: "Refs PAY-231\n\nBounded." })).toBe(true);
  expect(answeredAsDrawn(EDIT, { title: "SEPA retry!", body: "Refs PAY-231\r\n\r\nBounded." })).toBe(false);
  // Nothing to answer is not an answer left alone.
  expect(answeredAsDrawn(REOPEN, {})).toBe(false);
  // Only an op that sends changes has "unchanged" to say: an untouched
  // Approve… whose verdict this build cannot carry is a fault, not a no-op.
  const approve = review("APPROVED", "Approve…", false);
  expect(actionPayload(approve, "gitea:o/r#7", { body: "" })).toBeNull();
  expect(answeredAsDrawn(approve, { body: "" })).toBe(false);
});

// -- pending line comments (#709) ---------------------------------------------

const PENDING: ReviewComment[] = [
  { path: "src/retry.rs", line: 2, side: "old", body: "why drop it?" },
  { path: "src/retry.rs", line: 3, side: "new", body: "bound it" },
];

test("a review that sends line comments carries the pending ones, both sides", () => {
  const approve = { ...review("approve", "Approve…", false), line_comments: "sends" as const };
  expect(actionPayload(approve, "gitea:o/r#7", { body: "" }, [], PENDING)).toEqual({
    SubmitReview: { entity: "gitea:o/r#7", verdict: "approve", body: "", comments: PENDING },
  });
  // With none pending it is the review it was.
  expect(actionPayload(approve, "gitea:o/r#7", { body: "" }, [], [])).toEqual({
    SubmitReview: { entity: "gitea:o/r#7", verdict: "approve", body: "", comments: [] },
  });
});

test("an action that does not send line comments never carries them", () => {
  const plain = review("approve", "Approve…", false);
  expect(actionPayload(plain, "gitea:o/r#7", { body: "" }, [], PENDING)).toEqual({
    SubmitReview: { entity: "gitea:o/r#7", verdict: "approve", body: "", comments: [] },
  });
});

test("Submit review… is drawn only while comments are pending, and sends nothing without them", () => {
  const submit = action("submit_review", "Submit review…", {
    fields: [field("body", { type: "long_text" }, {})],
    values: { verdict: "comment" },
    line_comments: "requires",
  });
  const approve = { ...review("approve", "Approve…", false), line_comments: "sends" as const };
  expect(offeredActions([submit, approve], ["submit_review"], 0)).toEqual([approve]);
  expect(offeredActions([submit, approve], ["submit_review"], 2)).toEqual([submit, approve]);
  expect(actionPayload(submit, "gitea:o/r#7", { body: "" }, [], [])).toBeNull();
  expect(actionPayload(submit, "gitea:o/r#7", { body: "" }, [], PENDING)).toEqual({
    SubmitReview: { entity: "gitea:o/r#7", verdict: "comment", body: "", comments: PENDING },
  });
});

test("a diff takes comments only through an op the source declares and the shell can queue", () => {
  expect(queueable("submit_review", ["submit_review"])).toBe(true);
  expect(queueable("submit_review", ["comment"])).toBe(false);
  expect(queueable("launch_rocket", ["launch_rocket"])).toBe(false);
});
