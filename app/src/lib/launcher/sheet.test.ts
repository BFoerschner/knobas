/**
 * The Tab sheet's table (#2426): which actions a row has, in which order, and
 * what each one does, read as data. The component tests drive the same table
 * through the keyboard; these pin the per-kind rows without a DOM.
 */
import { expect, test } from "vitest";

import type { InboxItem } from "../ipc/entity";
import type { LinkSetRow } from "../ipc/entity";
import { favouriteKeys, kindOf, narrowActions, sheetActions, type SheetContext, type SheetEffect, type SheetSubject } from "./sheet";

const JOB = { entityId: "jira:PAY-999", label: "PAY-999" };
const CTX: SheetContext = {
  job: JOB,
  canTime: true,
  apps: [
    { id: "code", name: "VS Code" },
    { id: "idea", name: "IntelliJ" },
  ],
  favourites: new Set(),
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

test("a ticket: Work on this, the timer, the link and Copy key, in one order whatever the job (F-637)", () => {
  expect(ids(entity("ticket", "jira:PAY-231"))).toEqual(["work", "timer", "link", "fav", "copy"]);
  const actions = sheetActions(entity("ticket", "jira:PAY-231"), CTX);
  expect(actions[0]?.label).toBe("Work on this");
  expect(ids(entity("ticket", "jira:PAY-231"), { ...CTX, job: null })).toEqual(["work", "timer", "fav", "copy"]);
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
    "Review the files",
    "Conversation",
    "Approve",
    "Link to PAY-999",
    "Open in VS Code",
    "Open in IntelliJ",
    "Favourite",
    "Copy title",
  ]);
  expect(actions[0]?.does).toEqual({ kind: "go", hash: "#/pr/gitea%3Aacme%2Fsvc%23142/diff" });
  expect(actions[2]?.does).toMatchObject({ kind: "write", payload: { Approve: { entity: "gitea:acme/svc#142", body: "" } } });
  expect(actions[5]?.does).toEqual({ kind: "open-in", application: "idea", name: "IntelliJ" });
});

test("a repository: Pull requests, Branches, Open in, Clone", () => {
  expect(ids(entity("repo", "gitea:acme/svc"))).toEqual(["prs", "branches", "link", "open-code", "open-idea", "clone", "fav", "copy"]);
});

test("no application configured: no Open in row", () => {
  expect(ids(entity("repo", "gitea:acme/svc"), { ...CTX, apps: [] })).toEqual(["prs", "branches", "link", "clone", "fav", "copy"]);
});

test("a branch opens in each application; a pipeline runs; a monitor is acknowledged", () => {
  expect(ids(entity("branch", "gitea:acme/svc@main"))).toEqual(["link", "open-code", "open-idea", "fav", "copy"]);
  const run = sheetActions(entity("pipeline", "teamcity:Pay_Build"), CTX).find((a) => a.id === "run");
  // The run it starts can be reached from its toast, as a page's *Re-run* can (F-644).
  expect(run?.does).toMatchObject({ where: { label: "Show in runs" } });
  expect(run?.does).toMatchObject({ kind: "write", payload: { TriggerBuild: { entity: "teamcity:Pay_Build" } } });
  const ack = sheetActions(entity("monitor", "kuma:7"), CTX).find((a) => a.id === "ack");
  expect(ack?.does).toMatchObject({ kind: "ack", monitorId: "kuma:7" });
});

test("a page, an asset or anything else: the pair and Copy title", () => {
  expect(ids(entity("page", "confluence:ENG:SEPA"))).toEqual(["link", "fav", "copy"]);
});

test("a view opens and copies its address; a set opens; a saved list applies, renames, deletes and takes a key", () => {
  const view = sheetActions({ kind: "view", id: "my-open", title: "My open" }, CTX);
  expect(view.map((a) => a.label)).toEqual(["Open", "Copy address"]);
  expect(view[1]?.does).toMatchObject({ kind: "copy", text: "#/tickets/my-open" });
  expect(ids({ kind: "set", id: "payments", title: "Payments" })).toEqual(["open"]);
  expect(ids({ kind: "list", id: "sepa", label: "sepa payouts", text: "sepa payouts", refusal: null })).toEqual([
    "apply",
    "rename",
    "delete",
    "fav",
    "key",
  ]);
  // The panel that binds the chord (#2429).
  expect(sheetActions({ kind: "list", id: "sepa", label: "sepa payouts", text: "sepa payouts", refusal: null }, CTX).at(-1)).toMatchObject({
    label: "Assign key…",
    does: { kind: "bind" },
  });
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

test("an inbox line: Open, Re-run where the source offers it, Acknowledge on an alert, Snooze, Done", () => {
  expect(ids({ kind: "inbox", item: LINE, ops: ["rerun_build"] })).toEqual(["open", "rerun", "snooze", "done"]);
  expect(ids({ kind: "inbox", item: LINE, ops: [] })).toEqual(["open", "snooze", "done"]);
  const alert = { ...LINE, key: "alert:kuma:7", category: "alert", kind: "monitor", entity_id: "kuma:7" } as InboxItem;
  expect(ids({ kind: "inbox", item: alert, ops: [] })).toEqual(["open", "ack", "snooze", "done"]);
});

/** Run a flow's steps with `picks` and say what its last pick does. */
async function finish(does: SheetEffect, picks: string[]): Promise<{ choices: string[][]; does: SheetEffect }> {
  if (does.kind !== "flow") throw new Error(`not a flow: ${does.kind}`);
  const choices: string[][] = [];
  for (const [at, step] of does.steps.entries()) choices.push((await step.choices(picks.slice(0, at))).map((choice) => choice.id));
  return { choices, does: does.finish(picks) };
}

const SET = {
  id: "payments",
  name: "Payments",
  description: "",
  members: [{ kind: "page", entity_id: "confluence:ENG:SEPA", title: null }],
  patterns: [],
  rules: [{ kind: "manual", value: "jira:PAY-231", title: null }],
} as unknown as LinkSetRow;

test("the set row sits with the pair, after the link, and a member or a manual rule already there is not added twice", async () => {
  const ctx = { ...CTX, sets: [SET] };
  expect(ids(entity("monitor", "kuma:7"), ctx).slice(0, 3)).toEqual(["ack", "link", "set"]);
  expect(ids(entity("ticket", "jira:PAY-231"), ctx).slice(0, 4)).toEqual(["work", "timer", "link", "set"]);
  for (const kind of ["pr", "branch", "pipeline", "page", "asset", "repo"]) {
    expect(ids(entity(kind, `x:${kind}`), ctx).includes("set"), kind).toBe(kind !== "pr" && kind !== "branch");
  }
  const page = sheetActions(entity("page", "confluence:ENG:SEPA"), ctx).find((a) => a.id === "set")!;
  const added = await finish(page.does, ["payments"]);
  expect(added.choices).toEqual([["payments"]]);
  expect(added.does).toMatchObject({ kind: "save-set", draft: { members: [{ kind: "page", entity_id: "confluence:ENG:SEPA" }] } });
  const ticket = sheetActions(entity("ticket", "jira:PAY-231"), ctx).find((a) => a.id === "set")!;
  expect((await finish(ticket.does, ["payments"])).does).toMatchObject({ kind: "save-set", draft: { rules: [{ kind: "manual", value: "jira:PAY-231" }] } });
});

test("Create pull request: the head from every branch, the default first; the base without the head; the compare page with both", async () => {
  const branches = async () => [
    { name: "feature/x", isDefault: false },
    { name: "main", isDefault: true },
  ];
  const row = sheetActions(entity("repo", "gitea:acme/svc"), { ...CTX, branches }).find((a) => a.id === "newpr")!;
  expect(row.label).toBe("Create pull request…");
  const run = await finish(row.does, ["feature/x", "main"]);
  expect(run.choices).toEqual([["main", "feature/x"], ["main"]]);
  expect(run.does).toEqual({ kind: "go", hash: "#/repo/gitea%3Aacme%2Fsvc/compare/main...feature%2Fx" });
  expect(ids(entity("repo", "gitea:acme/svc"))).not.toContain("newpr");
});


test("the narrowing keeps every typed letter in order, the tightest match first", () => {
  const actions = sheetActions(entity("repo", "gitea:acme/svc"), CTX);
  expect(narrowActions(actions, "").map((a) => a.id)).toEqual(actions.map((a) => a.id));
  // "br" is tight in "Branches", looser in "Open in IntelliJ"'s neighbours or nowhere at all.
  expect(narrowActions(actions, "br")[0]?.id).toBe("branches");
  expect(narrowActions(actions, "cln")[0]?.id).toBe("clone");
  // The tightest span wins over the earliest start: "co" is one letter apart in "Copy", two in "Clone".
  expect(narrowActions(actions, "co").map((a) => a.id).slice(0, 2)).toEqual(["copy", "open-code"]);
  expect(narrowActions(actions, "zzz")).toEqual([]);
  expect(narrowActions(actions, "O I").map((a) => a.id)).toContain("open-idea");
});

test("Favourite on an entity and a saved list, Unfavourite on one that is a favourite (#2428)", () => {
  const ticket = entity("ticket", "jira:PAY-231");
  const list: SheetSubject = { kind: "list", id: "sepa", label: "sepa payouts", text: "sepa payouts", refusal: null };
  const fav = (subject: SheetSubject, ctx: SheetContext = CTX) => sheetActions(subject, ctx).find((a) => a.id === "fav");
  expect(fav(ticket)).toMatchObject({ label: "Favourite", does: { kind: "favourite", target: { entity: "jira:PAY-231" }, on: true } });
  expect(fav(list)).toMatchObject({ label: "Favourite", does: { kind: "favourite", target: { list: "sepa" }, on: true } });

  // The keys the board's block holds: a list row by its list id, any other row by its entity id.
  const kept = favouriteKeys([
    { kind: "list", source_id: "list", entity_id: "sepa" },
    { kind: "ticket", source_id: "jira", entity_id: "jira:PAY-231" },
  ]);
  expect(fav(ticket, { ...CTX, favourites: kept })).toMatchObject({ label: "Unfavourite", does: { on: false } });
  expect(fav(list, { ...CTX, favourites: kept })).toMatchObject({ label: "Unfavourite", does: { on: false } });
  // A ticket whose id is a list's id is still not that list.
  expect(fav(entity("ticket", "sepa"), { ...CTX, favourites: kept })?.label).toBe("Favourite");
  // Views, sets and inbox lines are not favourites.
  expect(ids({ kind: "view", id: "my-open", title: "My open" })).not.toContain("fav");
  expect(ids({ kind: "set", id: "payments", title: "Payments" })).not.toContain("fav");
  expect(ids({ kind: "inbox", item: LINE, ops: [] })).not.toContain("fav");
});

test("a set: Open, then Show on the chosen job, which shows the set on the job's ticket by a manual rule (story 38)", () => {
  const ctx = { ...CTX, sets: [SET] };
  const set = { kind: "set", id: "payments", title: "Payments" } as const;
  const actions = sheetActions(set, ctx);
  expect(actions.map((a) => a.id)).toEqual(["open", "show"]);
  expect(actions[1]).toMatchObject({
    label: "Apply to PAY-999",
    does: { kind: "save-set", said: "Payments now shows on PAY-999", draft: { rules: [{ kind: "manual", value: "jira:PAY-231" }, { kind: "manual", value: "jira:PAY-999" }] } },
  });
  // Already shown there: saved as it is, no second rule.
  const held = sheetActions(set, { ...ctx, job: { entityId: "jira:PAY-231", label: "PAY-231" } })[1]!;
  expect(held.does).toMatchObject({ kind: "save-set", draft: { rules: [{ kind: "manual", value: "jira:PAY-231" }] } });
  if (held.does.kind === "save-set") expect(held.does.draft.rules).toHaveLength(1);
  // No job chosen, or the set not read yet: Open alone.
  expect(ids(set, { ...ctx, job: null })).toEqual(["open"]);
  expect(ids(set, CTX)).toEqual(["open"]);
});

test("a key is kept per kind: an entity's own kind, else the row's", () => {
  expect(kindOf({ kind: "entity", entityId: "gitea:r", entityKind: "repo", title: "r", sourceId: "gitea", path: null })).toBe("repo");
  expect(kindOf({ kind: "list", id: "l", label: "L", text: "#t", refusal: null })).toBe("list");
  expect(kindOf({ kind: "view", id: "v", title: "V" })).toBe("view");
});
