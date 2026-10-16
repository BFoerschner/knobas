/**
 * The Inbox (#834, R-12.1 to R-12.3), mounted over a stand-in inbox store and
 * stand-in reads: the rows, the job each belongs to, *This job* over the
 * reach read, the kind chips, the actions and the snoozed line. What the
 * backend derives is `knobas-core`'s `inbox` tests; here the calls are counted.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import { createInbox, type Inbox } from "../../inbox/inbox.svelte";
import type { ContextRow, EntityDetail, InboxEntry, InboxItem, Reached } from "../../ipc/entity";
import type { QueuedWrite, WriteOpPayload } from "../../ipc/sources";
import { toasts } from "../../shell/toasts.svelte";
import { forgetFilters } from "../filter/filter-state.svelte";
import TfInbox from "./TfInbox.svelte";

const NOW = new Date(2026, 7, 22, 15, 0);
const at = (h: number, m: number, d = 22) => new Date(2026, 7, d, h, m).toISOString();

function entry(over: Partial<InboxItem>, actions: string[] = []): InboxEntry {
  return {
    item: {
      key: "k",
      category: "review_request",
      source_id: "gitea",
      entity_id: "gitea:payouts#142",
      kind: "pr",
      title: "Idempotent retry window",
      reason: "jonas asked for your review",
      occurred_at: at(13, 50),
      web_url: null,
      snoozed_until: null,
      ...over,
    },
    actions,
  };
}

const STREAM = [
  entry({ key: "review_request:gitea:payouts#142" }, ["approve", "comment"]),
  entry({ key: "alert:kuma:flows", category: "alert", source_id: "kuma", entity_id: "asset:gitea", kind: "asset",
    title: "flows-stage", reason: "Testenv / gitea is down", occurred_at: at(12, 40) }),
  entry({ key: "failed_build:tc:Payout#1187", category: "failed_build", source_id: "tc", entity_id: "tc:Payout#1187",
    kind: "build", title: "Payout_IntegrationTests #1187", reason: "this build failed: Tests failed: 3", occurred_at: at(10, 14) }, ["rerun_build"]),
  entry({ key: "new_assignment:jira:PAY-240", category: "new_assignment", source_id: "jira", entity_id: "jira:PAY-240",
    kind: "ticket", title: "Payout dashboard latency", reason: "assigned to you", occurred_at: at(16, 30, 21) }),
];
const SNOOZED = [
  entry({ key: "credential_expiry:jira", category: "credential_expiry", source_id: "jira", entity_id: null, kind: null,
    title: "Jira", reason: "the Jira credential expires on Monday 7 Sep 2026", snoozed_until: new Date(2026, 7, 29, 9).toISOString() }),
];

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let calls: string[];

beforeEach(() => {
  toasts.items.length = 0;
  forgetFilters();
  calls = [];
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

async function settle() {
  for (let turn = 0; turn < 10; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

function store(): Inbox {
  let stream = STREAM.slice();
  let snoozed = SNOOZED.slice();
  return createInbox({
    inboxItems: async (shelf) => (shelf === "snoozed" ? snoozed.slice() : stream.slice()),
    inboxCount: async () => stream.length,
    snoozeInboxItem: async (key, until) => {
      calls.push(`snooze ${key} ${until}`);
      const moved = stream.find((row) => row.item.key === key)!;
      stream = stream.filter((row) => row !== moved);
      snoozed = [...snoozed, { ...moved, item: { ...moved.item, snoozed_until: until } }];
    },
    completeInboxItem: async (key) => {
      calls.push(`done ${key}`);
      stream = stream.filter((row) => row.item.key !== key);
    },
    ackAlert: async (monitor) => {
      calls.push(`ack ${monitor}`);
      stream = stream.filter((row) => row.item.key !== `alert:${monitor}`);
    },
    listen: async () => () => {},
  });
}

const REACH: Record<string, string[]> = {
  "jira:PAY-231": ["gitea:payouts#142", "tc:Payout#1187", "asset:gitea"],
};

function context(anchor: string): ContextRow {
  return { id: `ctx:${anchor}`, kind: "ticket", title: anchor, anchor_id: anchor, created_at: at(9, 0), archived_at: null };
}

async function open(over: { job?: string | null; contexts?: ContextRow[] } = {}) {
  const ports = {
    listContexts: async () => over.contexts ?? [],
    listSources: async () => [{ id: "jira" }, { id: "gitea" }],
    getEntity: async (id: string) => {
      if (id !== "jira:PAY-231") throw { code: "not_found", message: "no", source_id: null };
      return { row: { entity_id: id, kind: "ticket" } } as unknown as EntityDetail;
    },
    jobReach: async (id: string): Promise<Reached[]> =>
      (REACH[id] ?? []).map((entity_id) => ({ entity_id, kind: "pr", step: entity_id, origin: { kind: "own" } })),
    submitWrite: async (payload: WriteOpPayload) => {
      calls.push(`write ${JSON.stringify(payload)}`);
      return { id: 7 } as unknown as QueuedWrite;
    },
  };
  view = mount(TfInbox, { target, props: { ready: true, job: over.job ?? null, inbox: store(), ports, now: NOW } });
  await settle();
}

const text = (node: Element | null) => node?.textContent?.replace(/\s+/g, " ").trim();
const rows = () => [...target.querySelectorAll(".tf-ib")];
const whats = () => rows().map((row) => text(row.querySelector(".tf-what")));
const ctxs = () => rows().map((row) => text(row.querySelector(".tf-ctx")));
function button(row: Element, label: string): HTMLButtonElement {
  const found = [...row.querySelectorAll("button")].find((candidate) => candidate.textContent?.trim() === label);
  if (!found) throw new Error(`no button ${label}`);
  return found as HTMLButtonElement;
}
function rowOf(words: string): Element {
  const found = rows().find((row) => row.textContent?.includes(words));
  if (!found) throw new Error(`no row ${words}`);
  return found;
}

test("each row is a time, a sentence, its actions and its kind and job", async () => {
  await open({ job: "PAY-231" });
  expect(whats()).toEqual([
    "Jonas asked for your review: Idempotent retry window",
    "Testenv / gitea is down: flows-stage",
    "This build failed: Tests failed: 3: Payout_IntegrationTests #1187",
    "Assigned to you: Payout dashboard latency",
  ]);
  expect(ctxs()).toEqual(["Review, job PAY-231", "Monitor, job PAY-231", "Build, job PAY-231", "Assignment, job PAY-240"]);
  expect(rows().map((row) => text(row.querySelector("time")))).toEqual(["13:50", "12:40", "10:14", "yest. 16:30"]);
  expect(rows().map((row) => row.querySelector(".tf-dot.tf-err") !== null)).toEqual([false, true, true, false]);
  expect(rows().map((row) => [...row.querySelectorAll(".tf-acts > *")].map((each) => text(each)).join(" "))).toEqual([
    "Review Snooze Done",
    "Open monitor Ack Snooze Done",
    "Open log Re-run Snooze Done",
    "Open job Snooze Done",
  ]);
  expect(rowOf("Idempotent").querySelector(".tf-ctx a")?.getAttribute("href")).toBe("#/job/PAY-231");
  expect(text(target.querySelector(".tf-ishelf"))).toBe(
    "Snoozed: the Jira credential expires on Monday 7 Sep 2026 (back 2026-08-29).",
  );
});

test("a row no open job reaches says no job; an open job's context names it", async () => {
  await open();
  expect(ctxs()[0]).toBe("Review, no job");
  if (view) unmount(view);
  target.innerHTML = "";
  await open({ contexts: [context("jira:PAY-231")] });
  expect(ctxs()[0]).toBe("Review, job PAY-231");
});

test("All is the first scope shown; This job keeps what the job reaches", async () => {
  await open({ job: "PAY-231" });
  const scopes = [...target.querySelectorAll(".tf-seg button")];
  expect(scopes.map((scope) => text(scope))).toEqual(["This job PAY-231", "All"]);
  expect(rows()).toHaveLength(4);
  (scopes[0] as HTMLButtonElement).click();
  flushSync();
  expect(ctxs()).toEqual(["Review, job PAY-231", "Monitor, job PAY-231", "Build, job PAY-231"]);
});

test("a kind chip keeps that kind, and the text filters the sentences", async () => {
  await open({ job: "PAY-231" });
  const chip = [...target.querySelectorAll(".tf-fchips button")].find((b) => b.textContent === "Build") as HTMLButtonElement;
  chip.click();
  flushSync();
  expect(ctxs()).toEqual(["Build, job PAY-231"]);
  chip.click();
  const field = target.querySelector<HTMLInputElement>(".tf-fsearch input")!;
  field.value = "dashboard";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  expect(ctxs()).toEqual(["Assignment, job PAY-240"]);
});

test("Re-run queues the source's write and says so", async () => {
  await open();
  button(rowOf("Payout_Integration"), "Re-run").click();
  await settle();
  expect(calls).toContain(`write ${JSON.stringify({ RerunBuild: { entity: "tc:Payout#1187" } })}`);
  expect(toasts.items.map((toast) => toast.text)).toContain("Re-run queued for Payout_IntegrationTests #1187");
});

test("Ack acks the monitor's alert and the row leaves", async () => {
  await open();
  button(rowOf("flows-stage"), "Ack").click();
  await settle();
  expect(calls).toEqual(["ack kuma:flows"]);
  expect(toasts.items.map((toast) => toast.text)).toContain("Acked. The alert stays open until the monitor recovers.");
  expect(rows().some((row) => row.textContent?.includes("flows-stage"))).toBe(false);
});

test("Done completes the item", async () => {
  await open();
  button(rowOf("dashboard"), "Done").click();
  await settle();
  expect(calls).toEqual(["done new_assignment:jira:PAY-240"]);
  expect(rows()).toHaveLength(3);
});

test("Snooze offers the presets, and one moves the item to the snoozed line", async () => {
  await open();
  const row = rowOf("Idempotent");
  button(row, "Snooze").click();
  flushSync();
  button(row, "Next Monday").click();
  await settle();
  const monday = new Date(2026, 7, 24, 9, 0).toISOString();
  expect(calls).toEqual([`snooze review_request:gitea:payouts#142 ${monday}`]);
  expect(toasts.items.map((toast) => toast.text)).toContain("Snoozed until Monday 09:00");
  expect(text(target.querySelector(".tf-ishelf"))).toBe(
    "Snoozed: the Jira credential expires on Monday 7 Sep 2026 (back 2026-08-29), jonas asked for your review: Idempotent retry window (back 2026-08-24).",
  );
});

test("an empty stream says so", async () => {
  view = mount(TfInbox, {
    target,
    props: {
      ready: true,
      job: null,
      now: NOW,
      inbox: createInbox({
        inboxItems: async () => [],
        inboxCount: async () => 0,
        snoozeInboxItem: async () => {},
        completeInboxItem: async () => {},
        ackAlert: async () => {},
        listen: async () => () => {},
      }),
      ports: {
        listContexts: async () => [],
        listSources: async () => [],
        getEntity: async () => ({}) as EntityDetail,
        jobReach: async () => [],
        submitWrite: async () => ({}) as QueuedWrite,
      },
    },
  });
  await settle();
  expect(text(target.querySelector(".tf-empty"))).toBe("Nothing here. New items appear as sources sync.");
  expect(target.querySelector(".tf-ishelf")).toBeNull();
});
