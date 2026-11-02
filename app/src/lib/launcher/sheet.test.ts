/**
 * The Tab sheet's table (#2426): which actions a row has, in which order, and
 * what each one does, read as data. The component tests drive the same table
 * through the keyboard; these pin the per-kind rows without a DOM.
 */
import { expect, test } from "vitest";

import type { InboxItem } from "../ipc/entity";
import { narrowActions, sheetActions, type SheetContext, type SheetSubject } from "./sheet";

const JOB = { entityId: "jira:PAY-999", label: "PAY-999" };
const CTX: SheetContext = {
  job: JOB,
  canTime: true,
  apps: [
    { id: "code", name: "VS Code" },
    { id: "idea", name: "IntelliJ" },
  ],
};

const entity = (entityKind: string, entityId: string, title = "Title"): SheetSubject => ({
  kind: "entity",
  entityId,
  entityKind,
  title,
  sourceId: entityId.split(":")[0] ?? "",
  path: null,
});

const ids = (subject: SheetSubject, ctx: SheetContext = CTX) => sheetActions(subject, ctx).map((action) => action.id);

test("a ticket: the link first, then the timer, Work on this and Copy key", () => {
  expect(ids(entity("ticket", "jira:PAY-231"))).toEqual(["link", "timer", "work", "copy"]);
  const actions = sheetActions(entity("ticket", "jira:PAY-231"), CTX);
  expect(actions[0]?.label).toBe("Link to PAY-999");
  expect(actions.find((a) => a.id === "work")?.does).toEqual({ kind: "go", hash: "#/job/jira%3APAY-231" });
  expect(actions.find((a) => a.id === "copy")).toMatchObject({ label: "Copy key", does: { kind: "copy", text: "PAY-231" } });
});

test("the job's own ticket has no link, and no job means no link anywhere", () => {
  expect(ids(entity("ticket", "jira:PAY-999"))).not.toContain("link");
  expect(ids(entity("repo", "gitea:acme/svc"), { ...CTX, job: null })).not.toContain("link");
});

test("Start timer is a ticket's alone", () => {
  for (const kind of ["pr", "repo", "pipeline", "monitor", "page", "branch"]) {
    expect(ids(entity(kind, `x:${kind}`)), kind).not.toContain("timer");
  }
  expect(ids(entity("ticket", "jira:PAY-231"), { ...CTX, canTime: false })).not.toContain("timer");
  // A stored context is never a timer target (`shell/timer.ts`).
  expect(ids(entity("ticket", "ctx:5b1c0f1e"))).not.toContain("timer");
});

test("a pull request: the files, the conversation, Approve, one Open in per application, Copy title", () => {
  const actions = sheetActions(entity("pr", "gitea:acme/svc#142", "Backoff"), CTX);
  expect(actions.map((a) => a.label)).toEqual([
    "Link to PAY-999",
    "Review the files",
    "Conversation",
    "Approve",
    "Open in VS Code",
    "Open in IntelliJ",
    "Copy title",
  ]);
  expect(actions[1]?.does).toEqual({ kind: "go", hash: "#/pr/gitea%3Aacme%2Fsvc%23142/diff" });
  expect(actions[3]?.does).toMatchObject({ kind: "write", payload: { Approve: { entity: "gitea:acme/svc#142", body: "" } } });
  expect(actions[5]?.does).toEqual({ kind: "open-in", application: "idea", name: "IntelliJ" });
});

test("a repository: Pull requests, Branches, Open in, Clone", () => {
  expect(ids(entity("repo", "gitea:acme/svc"))).toEqual(["link", "prs", "branches", "open-code", "open-idea", "clone", "copy"]);
});

test("no application configured: no Open in row", () => {
  expect(ids(entity("repo", "gitea:acme/svc"), { ...CTX, apps: [] })).toEqual(["link", "prs", "branches", "clone", "copy"]);
});

test("a branch opens in each application; a pipeline runs; a monitor is acknowledged", () => {
  expect(ids(entity("branch", "gitea:acme/svc@main"))).toEqual(["link", "open-code", "open-idea", "copy"]);
  const run = sheetActions(entity("pipeline", "teamcity:Pay_Build"), CTX).find((a) => a.id === "run");
  expect(run?.does).toMatchObject({ kind: "write", payload: { TriggerBuild: { entity: "teamcity:Pay_Build" } } });
  const ack = sheetActions(entity("monitor", "kuma:7"), CTX).find((a) => a.id === "ack");
  expect(ack?.does).toMatchObject({ kind: "ack", monitorId: "kuma:7" });
});

test("a page, an asset or anything else: the pair and Copy title", () => {
  expect(ids(entity("page", "confluence:ENG:SEPA"))).toEqual(["link", "copy"]);
});

test("a view opens and copies its address; a set opens; a saved list applies, renames and deletes", () => {
  const view = sheetActions({ kind: "view", id: "my-open", title: "My open" }, CTX);
  expect(view.map((a) => a.label)).toEqual(["Open", "Copy address"]);
  expect(view[1]?.does).toMatchObject({ kind: "copy", text: "#/tickets/my-open" });
  expect(ids({ kind: "set", id: "payments", title: "Payments" })).toEqual(["open"]);
  expect(ids({ kind: "list", id: "sepa", label: "sepa payouts", text: "sepa payouts", refusal: null })).toEqual([
    "apply",
    "rename",
    "delete",
  ]);
});

const LINE: InboxItem = {
  key: "failed_build:teamcity:1187",
  category: "failed_build",
  source_id: "teamcity",
  entity_id: "teamcity:1187",
  kind: "build",
  title: "Pay build failed",
  reason: "",
  occurred_at: "2026-08-25T11:00:00Z",
  snoozed_until: null,
} as unknown as InboxItem;

test("an inbox line: Open, Re-run where the source offers it, Acknowledge on an alert, Done", () => {
  expect(ids({ kind: "inbox", item: LINE, ops: ["rerun_build"] })).toEqual(["open", "rerun", "done"]);
  expect(ids({ kind: "inbox", item: LINE, ops: [] })).toEqual(["open", "done"]);
  const alert = { ...LINE, key: "alert:kuma:7", category: "alert", kind: "monitor", entity_id: "kuma:7" } as InboxItem;
  expect(ids({ kind: "inbox", item: alert, ops: [] })).toEqual(["open", "ack", "done"]);
});

test("the narrowing keeps every typed letter in order, the tightest match first", () => {
  const actions = sheetActions(entity("repo", "gitea:acme/svc"), CTX);
  expect(narrowActions(actions, "").map((a) => a.id)).toEqual(actions.map((a) => a.id));
  // "br" is tight in "Branches", looser in "Open in IntelliJ"'s neighbours or nowhere at all.
  expect(narrowActions(actions, "br")[0]?.id).toBe("branches");
  expect(narrowActions(actions, "cln")[0]?.id).toBe("clone");
  expect(narrowActions(actions, "zzz")).toEqual([]);
  expect(narrowActions(actions, "O I").map((a) => a.id)).toContain("open-idea");
});
