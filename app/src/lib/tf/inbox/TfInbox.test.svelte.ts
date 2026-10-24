/**
 * The Inbox (#834, R-12.1 to R-12.3), mounted over a stand-in inbox store and
 * stand-in reads: the rows, the job each belongs to, *This job* over the
 * reach read, the kind chips, the actions and the snoozed line. What the
 * backend derives is `knobas-core`'s `inbox` tests; here the calls are counted.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { createInbox, type Inbox } from "../../inbox/inbox.svelte";
import type { ContextRow, EntityDetail, InboxEntry, InboxItem, Reached } from "../../ipc/entity";
import type { QueuedWrite, WriteOpPayload } from "../../ipc/sources";
import { health } from "../../shell/health.svelte";
import { toasts } from "../../shell/toasts.svelte";
import { writeQueuePanel } from "../../shell/write-queue.svelte";
import { forgetFilters } from "../filter/filter-state.svelte";
import TfInbox from "./TfInbox.svelte";
import { sourceFacts } from "../job/source-facts.fixture";

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
  health.replace([]);
  writeQueuePanel.open = false;
  if (view) unmount(view);
  view = null;
  target.remove();
});

async function settle() {
  for (let turn = 0; turn < 10; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

function store(extra: InboxEntry[] = []): Inbox {
  let stream = [...STREAM, ...extra];
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

async function open(
  over: { job?: string | null; contexts?: ContextRow[]; extra?: InboxEntry[]; clock?: boolean } = {},
) {
  const ports = {
    listContexts: async () => over.contexts ?? [],
    listSources: async () => [sourceFacts("jira"), sourceFacts("gitea"), sourceFacts("tc", { display_name: "TeamCity" })],
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
  view = mount(TfInbox, { target, props: { ready: true, job: over.job ?? null, inbox: store(over.extra), ports, ...(over.clock ? {} : { now: NOW }) } });
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
    "jonas asked for your review: Idempotent retry window",
    "Testenv / gitea is down: flows-stage",
    "This build failed: Tests failed: 3: Payout_IntegrationTests #1187",
    "Assigned to you: Payout dashboard latency",
  ]);
  expect(ctxs()).toEqual(["Review, job PAY-231", "Monitor, job PAY-231", "Build, job PAY-231", "Assignment, job PAY-240"]);
  expect(rows().map((row) => text(row.querySelector("time")))).toEqual(["13:50", "12:40", "10:14", "yesterday 16:30"]);
  expect(rows().map((row) => row.querySelector(".tf-dot.tf-err") !== null)).toEqual([false, true, true, false]);
  expect(rows().map((row) => [...row.querySelectorAll(".tf-acts > *")].map((each) => text(each)).join(" "))).toEqual([
    "Review Snooze Done",
    "Open monitor Ack Snooze Done",
    "Open log Re-run Snooze Done",
    "Open job Snooze Done",
  ]);
  expect(rowOf("Idempotent").querySelector(".tf-ctx a")?.getAttribute("href")).toBe("#/job/jira%3APAY-231");
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
  expect(toasts.items.at(-1)!.text).toBe("Queued: re-run Payout_IntegrationTests #1187.");
});

test("Re-run on a source whose sign-in fails says the write waits there, and Show queue opens the queue", async () => {
  health.patch({ source_id: "tc", state: "unauthorized", checked_at: at(14, 0), detail: null, secret_expires_at: null });
  await open();
  button(rowOf("Payout_Integration"), "Re-run").click();
  await settle();
  const toast = toasts.items.at(-1)!;
  expect(toast.text).toBe("Queued: re-run Payout_IntegrationTests #1187. TeamCity sign-in is failing, so it waits there.");
  expect(toast.action?.label).toBe("Show queue");
  toast.action!.run();
  expect(writeQueuePanel.open).toBe(true);
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

function picker(row: Element): HTMLInputElement {
  return row.querySelector<HTMLInputElement>('input[type="date"]')!;
}
function typeDate(row: Element, value: string) {
  const input = picker(row);
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  input.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
}
async function openPicker() {
  await open();
  const row = rowOf("Idempotent");
  button(row, "Snooze").click();
  flushSync();
  return row;
}
const snoozeCalls = () => calls.filter((call) => call.startsWith("snooze "));
const snoozeToasts = () => toasts.items.filter((toast) => toast.text.startsWith("Snoozed until"));

test("the date field has min set to tomorrow (#1437)", async () => {
  const row = await openPicker();
  expect(picker(row).min).toBe("2026-08-23");
});

test("a change event with a half-typed year snoozes nothing and keeps the picker open (#1437)", async () => {
  const row = await openPicker();
  typeDate(row, "0002-11-02");
  await settle();
  expect(snoozeCalls()).toEqual([]);
  expect(snoozeToasts()).toEqual([]);
  expect(row.querySelector(".tf-isnooze")).not.toBeNull();
});

test("a past date and a press of Snooze make no call and no success toast (#1437)", async () => {
  const row = await openPicker();
  typeDate(row, "2026-08-20");
  const press = button(row.querySelector(".tf-isnooze")!, "Snooze");
  press.disabled = false;
  press.click();
  picker(row).dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await settle();
  expect(snoozeCalls()).toEqual([]);
  expect(snoozeToasts()).toEqual([]);
  expect(row.querySelector(".tf-isnooze")).not.toBeNull();
});

test("a future date and a press of Snooze make one call at 09:00, close the picker and toast (#1437)", async () => {
  const row = await openPicker();
  typeDate(row, "2026-08-25");
  expect(snoozeCalls()).toEqual([]);
  button(row.querySelector(".tf-isnooze")!, "Snooze").click();
  await settle();
  expect(snoozeCalls()).toEqual([`snooze review_request:gitea:payouts#142 ${new Date(2026, 7, 25, 9, 0).toISOString()}`]);
  expect(snoozeToasts()).toHaveLength(1);
  expect(row.querySelector(".tf-isnooze")).toBeNull();
});

test("Enter in the date field commits a future date once (#1437)", async () => {
  const row = await openPicker();
  typeDate(row, "2026-08-25");
  picker(row).dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await settle();
  expect(snoozeCalls()).toEqual([`snooze review_request:gitea:payouts#142 ${new Date(2026, 7, 25, 9, 0).toISOString()}`]);
  expect(snoozeToasts()).toHaveLength(1);
});

test("a credential expiry also offers After it expires, the day after the expiry the item is dated from (#1086)", async () => {
  // The backend dates the item at the expiry minus WINDOW_DAYS (14): this one
  // arrived on 22 Aug 08:00, so the credential expires on 5 Sep 08:00.
  await open({
    extra: [
      entry({ key: "credential_expiry:tc", category: "credential_expiry", source_id: "tc", entity_id: null, kind: null,
        title: "TeamCity", reason: "the TeamCity credential expires on Saturday 5 Sep 2026", occurred_at: at(8, 0) }),
    ],
  });
  const review = rowOf("Idempotent");
  button(review, "Snooze").click();
  flushSync();
  expect(text(review.querySelector(".tf-isnooze")), "a review has no expiry to hang it on").not.toContain("After it expires");

  const row = rowOf("TeamCity credential");
  button(row, "Snooze").click();
  flushSync();
  button(row, "After it expires").click();
  await settle();
  expect(calls).toEqual([`snooze credential_expiry:tc ${new Date(2026, 8, 6, 9, 0).toISOString()}`]);
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

test("a row about a ticket links the job by its entity id, so a key two sources hold opens the right one (#1676)", async () => {
  await open({
    extra: [
      entry({ key: "new_assignment:gitea:PAY-231", category: "new_assignment", source_id: "gitea", entity_id: "gitea:PAY-231",
        kind: "ticket", title: "Same key, other source", reason: "assigned to you", occurred_at: at(17, 0) }),
    ],
  });
  expect(rowOf("Same key").querySelector(".tf-ctx a")?.getAttribute("href")).toBe("#/job/gitea%3APAY-231");
});

test("a row's Open job names the ticket's source, so a key two sources hold opens the one the row is about (#1977)", async () => {
  await open({
    extra: [
      entry({ key: "new_assignment:gitea:PAY-231", category: "new_assignment", source_id: "gitea", entity_id: "gitea:PAY-231",
        kind: "ticket", title: "Same key, other source", reason: "assigned to you", occurred_at: at(17, 0) }),
    ],
  });
  const openJob = [...rowOf("Same key").querySelectorAll(".tf-acts a")].find((each) => text(each) === "Open job");
  expect(openJob?.getAttribute("href")).toBe("#/job/gitea%3APAY-231");
});

test("an inbox left open across midnight ticks its clock: a row reads yesterday and Tomorrow moves a day on (#1681)", async () => {
  vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval"] });
  try {
    vi.setSystemTime(new Date(2026, 7, 22, 23, 50));
    await open({ clock: true });
    expect(text(rowOf("Idempotent").querySelector("time"))).toBe("13:50");

    vi.setSystemTime(new Date(2026, 7, 23, 0, 10));
    vi.advanceTimersByTime(30_000);
    flushSync();
    expect(text(rowOf("Idempotent").querySelector("time"))).toBe("yesterday 13:50");

    const row = rowOf("Idempotent");
    button(row, "Snooze").click();
    flushSync();
    button(row, "Tomorrow").click();
    await settle();
    expect(snoozeCalls()).toEqual([`snooze review_request:gitea:payouts#142 ${new Date(2026, 7, 24, 9, 0).toISOString()}`]);
  } finally {
    vi.useRealTimers();
  }
});
