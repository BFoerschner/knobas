/**
 * The Inbox (#834, R-12.1 to R-12.3), mounted over a stand-in inbox store and
 * stand-in reads: the rows, the job each belongs to, *This job* over the
 * reach read, the kind chips, the actions and the snoozed line. What the
 * backend derives is `knobas-core`'s `inbox` tests; here the calls are counted.
 */
import { pageContext } from "../kept-pages.svelte";
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, onTestFinished, test, vi } from "vitest";

import { createInbox, type Inbox } from "../../inbox/inbox.svelte";
import type { ContextRow, EntityDetail, InboxEntry, InboxItem, Reached, RefusedWrite, WriteAsk } from "../../ipc/entity";
import type { QueuedWrite, WriteOpPayload } from "../../ipc/sources";
import { health } from "../../shell/health.svelte";
import { dismiss, toasts } from "../../shell/toasts.svelte";
import { writeQueuePanel } from "../../shell/write-queue.svelte";
import TfInbox from "./TfInbox.svelte";
import { sourceFacts } from "../job/source-facts.fixture";
import { PRESS_HOLD_MS } from "../press-hold.svelte";

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
    title: "flows-stage", reason: "gitea is down", detail: "Testenv", occurred_at: at(12, 40) }),
  entry({ key: "failed_build:tc:Payout#1187", category: "failed_build", source_id: "tc", entity_id: "tc:Payout#1187",
    kind: "build", title: "Payout_IntegrationTests #1187", reason: "Payout_IntegrationTests #1187 failed", detail: "Tests failed: 3", occurred_at: at(10, 14) }, ["rerun_build"]),
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
/** Each `refused_writes` the view sent, as JSON. */
let asked: string[];

beforeEach(() => {
  toasts.items.length = 0;
  failing = {};
  calls = [];
  asked = [];
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

/** Answers that fail, by item key, with what the bridge throws. */
let failing: Record<string, unknown> = {};

function store(extra: InboxEntry[] = []): Inbox {
  let stream = [...STREAM, ...extra];
  let snoozed = SNOOZED.slice();
  let done: InboxEntry[] = [];
  let acked: InboxEntry[] = [];
  return createInbox({
    inboxItems: async (shelf) => (shelf === "snoozed" ? snoozed.slice() : stream.slice()),
    inboxCount: async () => stream.length,
    snoozeInboxItem: async (key, until) => {
      calls.push(`snooze ${key} ${until}`);
      if (key in failing) throw failing[key];
      // The backend's upsert: whichever shelf holds it, and a date now brings it back, from *Done* too.
      const moved = [...stream, ...snoozed, ...done].find((row) => row.item.key === key)!;
      stream = stream.filter((row) => row.item.key !== key);
      snoozed = snoozed.filter((row) => row.item.key !== key);
      done = done.filter((row) => row.item.key !== key);
      // These tests' dates are their own clock's, so only a date of the real now is a wake.
      if (Math.abs(new Date(until).getTime() - Date.now()) > 5_000) snoozed = [...snoozed, { ...moved, item: { ...moved.item, snoozed_until: until } }];
      else stream = [...stream, { ...moved, item: { ...moved.item, snoozed_until: null } }];
    },
    completeInboxItem: async (key) => {
      calls.push(`done ${key}`);
      if (key in failing) throw failing[key];
      done = [...done, ...stream.filter((row) => row.item.key === key)];
      stream = stream.filter((row) => row.item.key !== key);
    },
    ackAlert: async (monitor) => {
      calls.push(`ack ${monitor}`);
      acked = [...acked, ...stream.filter((row) => row.item.key === `alert:${monitor}`)];
      stream = stream.filter((row) => row.item.key !== `alert:${monitor}`);
    },
    unackAlert: async (monitor) => {
      calls.push(`unack ${monitor}`);
      stream = [...stream, ...acked.filter((row) => row.item.key === `alert:${monitor}`)];
      acked = acked.filter((row) => row.item.key !== `alert:${monitor}`);
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
  over: {
    job?: string | null;
    contexts?: ContextRow[];
    extra?: InboxEntry[];
    clock?: boolean;
    refused?: (asks: WriteAsk[]) => Promise<RefusedWrite[]>;
    pipelines?: () => Promise<never>;
    sent?: boolean;
  } = {},
) {
  const ports = {
    refusedWrites: async (asks: WriteAsk[]) => {
      asked.push(JSON.stringify(asks));
      return over.refused ? over.refused(asks) : [];
    },
    listContexts: async () => over.contexts ?? [],
    listSources: async () => [sourceFacts("jira"), sourceFacts("gitea"), sourceFacts("tc", { display_name: "TeamCity" }), sourceFacts("kuma", { auth_kind: "ApiToken" })],
    getEntity: async (id: string) => {
      if (id !== "jira:PAY-231") throw { code: "not_found", message: "no", source_id: null };
      return { row: { entity_id: id, kind: "ticket" } } as unknown as EntityDetail;
    },
    jobReach: async (id: string): Promise<Reached[]> =>
      (REACH[id] ?? []).map((entity_id) => ({ entity_id, kind: "pr", step: entity_id, origin: { kind: "own" } })),
    submitWrite: async (payload: WriteOpPayload) => {
      calls.push(`write ${JSON.stringify(payload)}`);
      return (over.sent ? { id: 7, state: "sent" } : { id: 7 }) as unknown as QueuedWrite;
    },
    ...(over.pipelines ? { pipelines: over.pipelines } : {}),
  };
  view = mount(TfInbox, { target, context: hiddenable, props: { ready: true, job: over.job ?? null, inbox: store(over.extra), ports, ...(over.clock ? {} : { now: NOW }) } });
  await settle();
}

const text = (node: Element | null) => node?.textContent?.replace(/\s+/g, " ").trim();
const rows = () => [...target.querySelectorAll(".tf-inbox > .tf-ib")];
const shelfRows = () => [...target.querySelectorAll(".tf-ishelf-rows > .tf-ib")];
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
    "Idempotent retry window: jonas asked for your review",
    "flows-stage: gitea is down",
    "Payout_IntegrationTests #1187 failed",
    "Payout dashboard latency: assigned to you",
  ]);
  expect(ctxs()).toEqual(["Review, job PAY-231", "Monitor, job PAY-231 · Testenv", "Build, job PAY-231 · 3 failed", "Assignment, job PAY-240"]);
  expect(rows().map((row) => text(row.querySelector("time")))).toEqual(["13:50", "12:40", "10:14", "yesterday 16:30"]);
  expect(rows().map((row) => row.querySelector(".tf-dot.tf-err") !== null)).toEqual([false, true, true, false]);
  expect(rows().map((row) => [...row.querySelectorAll(".tf-acts > *")].map((each) => text(each)).join(" "))).toEqual([
    "Snooze… Done",
    "Ack Snooze… Done",
    "Re-run Snooze… Done",
    "Snooze… Done",
  ]);
  // The sentence is the link to the row's place (F-132), so a button is only for what a link can't do.
  expect(rows().map((row) => text(row.querySelector(".tf-what a")))).toEqual(rows().map((row) => text(row.querySelector(".tf-what"))));
  expect(rowOf("Payout dashboard").querySelector(".tf-what a")?.getAttribute("href")).toBe("#/ticket/jira%3APAY-240");

  expect(rowOf("Idempotent").querySelector(".tf-ctx a")?.getAttribute("href")).toBe("#/ticket/jira%3APAY-231");
  expect(text(target.querySelector(".tf-ishelf summary"))).toBe("Snoozed 1");
  expect(shelfRows().map((row) => text(row.querySelector(".tf-what")))).toEqual(["The Jira credential expires on Monday 7 Sep 2026"]);
  expect(shelfRows().map((row) => text(row.querySelector(".tf-ctx")))).toEqual(["Credential, no job, back Saturday 29 Aug 2026"]);
  expect(shelfRows().map((row) => text(row.querySelector(".tf-acts")))).toEqual(["Wake"]);
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
  expect(scopes.map((scope) => text(scope))).toEqual(["This job 3", "All"]);
  expect(rows()).toHaveLength(4);
  (scopes[0] as HTMLButtonElement).click();
  flushSync();
  expect(ctxs()).toEqual(["Review, job PAY-231", "Monitor, job PAY-231 · Testenv", "Build, job PAY-231 · 3 failed"]);
});

test("a kind chip keeps that kind", async () => {
  await open({ job: "PAY-231" });
  const chip = [...target.querySelectorAll(".tf-fchips button")].find((b) => b.textContent === "Build") as HTMLButtonElement;
  chip.click();
  flushSync();
  expect(ctxs()).toEqual(["Build, job PAY-231 · 3 failed"]);
});

test("the Inbox draws no search field and leaves / to the shell, which opens the launcher on #i (#2425)", async () => {
  await open({ job: "PAY-231" });
  expect(target.querySelector(".tf-fsearch")).toBeNull();
  expect(target.querySelector(".tf-fbar input")).toBeNull();
  expect(target.querySelectorAll(".tf-fbar .tf-seg button")).toHaveLength(2);
  const slash = new KeyboardEvent("keydown", { key: "/", bubbles: true, cancelable: true });
  window.dispatchEvent(slash);
  expect(slash.defaultPrevented, "the shell's TfShell test witnesses what / opens").toBe(false);
});

test("Re-run queues the source's write and says so", async () => {
  await open();
  button(rowOf("Payout_Integration"), "Re-run").click();
  await settle();
  expect(calls).toContain(`write ${JSON.stringify({ RerunBuild: { entity: "tc:Payout#1187" } })}`);
  expect(toasts.items.at(-1)!.text).toBe("Queued: re-run Payout_IntegrationTests #1187.");
});

test("Re-run holds about two seconds once it answers, so a second press queues nothing, and a later one a new run (F-150)", async () => {
  await open();
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  try {
    const run = async (ms: number) => {
      await vi.advanceTimersByTimeAsync(ms);
      flushSync();
    };
    const writes = () => calls.filter((call) => call.startsWith("write ")).length;
    const before = writes();
    button(rowOf("Payout_Integration"), "Re-run").click();
    await run(20);
    const held = rowOf("Payout_Integration").querySelector<HTMLButtonElement>("[data-act=rerun_build]")!;
    expect(held.textContent?.trim(), "it says what the write is doing, as Builds' does").toMatch(/^(Queued|Sent)$/);
    expect(held.getAttribute("aria-disabled")).toBe("true");
    held.click();
    await run(20);
    expect(writes() - before, "the second press lands on the hold").toBe(1);
    await run(PRESS_HOLD_MS);
    expect(held.textContent?.trim()).toBe("Re-run");
    button(rowOf("Payout_Integration"), "Re-run").click();
    await run(20);
    expect(writes() - before).toBe(2);
  } finally {
    vi.useRealTimers();
  }
});

test("Re-run the source's credential cannot make is drawn disabled saying why, and queues nothing (#2040)", async () => {
  const reason = "This TeamCity user cannot run builds in Payout: it lacks the Run build permission";
  await open({ refused: async (asks) => asks.map((ask) => ({ ...ask, reason })) });
  expect(asked).toEqual([JSON.stringify([{ entity: "tc:Payout#1187", op: "rerun_build" }])]);
  const rerun = button(rowOf("Payout_Integration"), "Re-run");
  expect(rerun.getAttribute("aria-disabled")).toBe("true");
  expect(rerun.title).toBe(reason);
  rerun.click();
  await settle();
  expect(calls.filter((call) => call.startsWith("write"))).toEqual([]);
});

test("Re-run stays enabled where the source refuses nothing, or cannot tell (#2040)", async () => {
  await open({ refused: async () => { throw { code: "not_ready", message: "no", source_id: null }; } });
  const rerun = button(rowOf("Payout_Integration"), "Re-run");
  expect(rerun.getAttribute("aria-disabled")).toBeNull();
  expect(rerun.title).toBe("");
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

test("Undo on an ack's toast takes it back, and the re-read brings the row back", async () => {
  await open();
  button(rowOf("flows-stage"), "Ack").click();
  await settle();
  const toast = toasts.items.find((each) => each.text.startsWith("Acked."))!;
  expect(toast.action?.label).toBe("Undo");
  toast.action!.run();
  await settle();
  expect(calls).toEqual(["ack kuma:flows", "unack kuma:flows"]);
  expect(rows().some((row) => row.textContent?.includes("flows-stage"))).toBe(true);
});

test("Done completes the item", async () => {
  await open();
  button(rowOf("dashboard"), "Done").click();
  await settle();
  expect(calls).toEqual(["done new_assignment:jira:PAY-240"]);
  expect(rows()).toHaveLength(3);
});

test("a click on a row with no selection follows its link, and one that selected words doesn't (F-142)", async () => {
  await open();
  const row = rowOf("dashboard");
  const link = row.querySelector<HTMLAnchorElement>(".tf-what a")!;
  const followed: string[] = [];
  link.addEventListener("click", (event) => {
    if (!event.defaultPrevented) followed.push(link.getAttribute("href")!);
    event.preventDefault();
  });
  row.querySelector("time")!.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 }));
  expect(followed).toEqual(["#/ticket/jira%3APAY-240"]);

  const selection = window.getSelection()!;
  selection.selectAllChildren(row.querySelector(".tf-ctx")!);
  row.querySelector("time")!.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 }));
  expect(followed, "the drag selected words").toHaveLength(1);
  selection.removeAllRanges();

  button(row, "Snooze…").click();
  flushSync();
  expect(followed, "a button answers for itself").toHaveLength(1);
});

test("a plain press on the sentence lifts its href until the button comes up, so a drag can select (F-142)", async () => {
  await open();
  const link = rowOf("dashboard").querySelector<HTMLAnchorElement>(".tf-what a")!;
  const href = link.getAttribute("href");
  link.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, button: 0 }));
  expect(link.hasAttribute("href"), "a browser won't start a selection on a link").toBe(false);
  window.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
  expect(link.getAttribute("href")).toBe(href);
  link.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, button: 0, metaKey: true }));
  expect(link.getAttribute("href"), "⌘-click stays the browser's").toBe(href);
});

test("a row a mouse press took away keeps its height and its words, with no controls, until the pointer has left the list and the toast has gone (F-138, F-064, F-159, F-163)", async () => {
  await open();
  const row = rowOf("dashboard");
  const at = rows().indexOf(row);
  button(row, "Done").dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 }));
  await settle();
  const held = target.querySelector<HTMLElement>(".tf-ib-held")!;
  expect(held).not.toBeNull();
  expect(rows().indexOf(held), "where it stood").toBe(at);
  expect(held.textContent).toContain("dashboard");
  expect(held.querySelector("button, a"), "a second press lands on nothing").toBeNull();
  expect(calls).toEqual(["done new_assignment:jira:PAY-240"]);

  window.dispatchEvent(new PointerEvent("pointermove", { clientX: 5000, clientY: 5000 }));
  await settle();
  expect(target.querySelector(".tf-ib-held"), "*Undo* is on the toast, and the way to it leaves the list").not.toBeNull();
  dismiss(toasts.items.at(-1)!.id);
  window.dispatchEvent(new PointerEvent("pointermove", { clientX: 5000, clientY: 5000 }));
  await settle();
  expect(target.querySelector(".tf-ib-held")).toBeNull();
  expect(rows()).toHaveLength(3);
});

test("rows pressed down a list by mouse each hold where they stood, and close together once the pointer has left it, the gutter being no way out (F-159, F-164)", async () => {
  await open();
  const keys = () => rows().map((row) => (row as HTMLElement).dataset["row"]);
  const before = keys();
  const list = target.querySelector<HTMLElement>(".tf-inbox")!;
  list.getBoundingClientRect = () => new DOMRect(0, 0, 800, 600);
  const pressDone = (index: number) => button(rows()[index]!, "Done").dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 }));

  pressDone(0);
  await settle();
  window.dispatchEvent(new PointerEvent("pointermove", { clientX: 700, clientY: 70 }));
  await settle();
  expect(target.querySelectorAll(".tf-ib-held"), "moving down the list closes nothing").toHaveLength(1);

  pressDone(1);
  await settle();
  expect(target.querySelectorAll(".tf-ib-held")).toHaveLength(2);
  expect(keys(), "no row moved").toEqual(before);
  expect(calls).toHaveLength(2);
  expect(calls[1], "the row aimed at finished").toContain(String(before[1]));
  expect(rows()[0]!.classList.contains("tf-ib-held") && rows()[1]!.classList.contains("tf-ib-held")).toBe(true);

  dismiss(toasts.items.at(-1)!.id);
  window.dispatchEvent(new PointerEvent("pointermove", { clientX: 5000, clientY: 70 }));
  await settle();
  expect(target.querySelectorAll(".tf-ib-held"), "right of the list, level with its rows").toHaveLength(2);

  window.dispatchEvent(new PointerEvent("pointermove", { clientX: 700, clientY: 5000 }));
  await settle();
  expect(target.querySelector(".tf-ib-held")).toBeNull();
  expect(keys()).toEqual(before.slice(2));
});

test("a press on a Snoozed row during a hold is a press in the list, so nothing closes under it; a press elsewhere closes once it has landed (F-163)", async () => {
  await open();
  const list = target.querySelector<HTMLElement>(".tf-inbox")!;
  list.getBoundingClientRect = () => new DOMRect(0, 0, 800, 600);
  button(rows()[0]!, "Done").dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 }));
  await settle();
  const at = (type: string, clientX: number, clientY: number) => window.dispatchEvent(new PointerEvent(type, { clientX, clientY }));
  const fold = document.createElement("details");
  fold.className = "tf-ishelf";
  fold.open = true;
  const shelf = document.createElement("div");
  shelf.className = "tf-ishelf-rows";
  shelf.getBoundingClientRect = () => new DOMRect(0, 700, 800, 100);
  fold.append(shelf);
  target.append(fold);
  at("pointermove", 400, 750);
  at("pointerdown", 400, 750);
  at("pointerup", 400, 750);
  await new Promise((resolve) => setTimeout(resolve, 5));
  await settle();
  expect(target.querySelector(".tf-ib-held"), "on a row of the shelf").not.toBeNull();

  at("pointermove", 400, 900);
  at("pointerdown", 400, 900);
  await settle();
  expect(target.querySelector(".tf-ib-held"), "not under the press").not.toBeNull();
  at("pointerup", 400, 900);
  await new Promise((resolve) => setTimeout(resolve, 5));
  await settle();
  expect(target.querySelector(".tf-ib-held")).toBeNull();
  fold.remove();
});

test("a wheel or a key closes the held rows together, and a row a press took above a held one keeps both in their places (F-159)", async () => {
  await open();
  const keys = () => rows().map((row) => (row as HTMLElement).dataset["row"]);
  const before = keys();
  const list = target.querySelector<HTMLElement>(".tf-inbox")!;
  list.getBoundingClientRect = () => new DOMRect(0, 0, 800, 600);
  button(rows()[2]!, "Done").dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 }));
  await settle();
  button(rows()[0]!, "Done").dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 }));
  await settle();
  expect(keys(), "going back up the list moves nothing").toEqual(before);
  expect(target.querySelectorAll(".tf-ib-held")).toHaveLength(2);
  window.dispatchEvent(new WheelEvent("wheel"));
  await settle();
  expect(target.querySelector(".tf-ib-held")).toBeNull();

  button(rows()[0]!, "Done").dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 }));
  await settle();
  expect(target.querySelectorAll(".tf-ib-held")).toHaveLength(1);
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "j" }));
  await settle();
  expect(target.querySelector(".tf-ib-held")).toBeNull();
});

test("a woken row is drawn as itself in the list above while its place holds in the shelf (F-064)", async () => {
  await open();
  button(shelfRows()[0]!, "Wake").dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 }));
  await settle();
  expect(target.querySelectorAll(".tf-ib-held")).toHaveLength(1);
  expect(target.querySelector(".tf-ishelf .tf-ib-held")).not.toBeNull();
});

test("a key's press closes the list at once and puts focus on the next row's same action (F-138)", async () => {
  await open();
  const row = rowOf("dashboard");
  const press = button(row, "Done");
  press.focus();
  press.click();
  await settle();
  expect(target.querySelector(".tf-ib-held")).toBeNull();
  const active = document.activeElement as HTMLElement;
  expect(active.textContent).toBe("Done");
  expect(active.closest<HTMLElement>(".tf-ib")!.dataset["row"]).not.toBe((row as HTMLElement).dataset["row"]);
});

test("j and k walk the rows, and focus goes to each row's link (F-143)", async () => {
  await open();
  const key = (name: string) => window.dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true }));
  key("j");
  flushSync();
  expect(document.activeElement).toBe(rows()[0]!.querySelector(".tf-what a"));
  key("j");
  flushSync();
  expect(document.activeElement).toBe(rows()[1]!.querySelector(".tf-what a"));
  key("k");
  flushSync();
  expect(document.activeElement).toBe(rows()[0]!.querySelector(".tf-what a"));
});

test("with no row focused, j and k start from the rows in view, so the first key keeps the place (F-075)", async () => {
  await open();
  const key = (name: string) => window.dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true }));
  const all = rows();
  // Row 0 is scrolled away, and rows 1 and 2 are all that is in view.
  const inView = all.slice(1, 3);
  for (const [at, row] of all.entries()) row.getBoundingClientRect = () => ({ left: 0, width: 100, height: 60, top: at * 100, bottom: at * 100 + 60 }) as DOMRect;
  const hit = document.elementFromPoint;
  document.elementFromPoint = (_x, y) => inView.find((row) => { const box = row.getBoundingClientRect(); return y > box.top && y < box.bottom; }) ?? null;
  onTestFinished(() => {
    document.elementFromPoint = hit;
  });
  (document.activeElement as HTMLElement | null)?.blur();
  key("j");
  flushSync();
  expect(document.activeElement).toBe(all[1]!.querySelector(".tf-what a"));
  (document.activeElement as HTMLElement | null)?.blur();
  key("k");
  flushSync();
  expect(document.activeElement).toBe(all[2]!.querySelector(".tf-what a"));
  // From a focused row it is an ordinary step.
  key("k");
  flushSync();
  expect(document.activeElement).toBe(all[1]!.querySelector(".tf-what a"));
});

test("Tab goes through a row's words before its actions: the job's key comes before them (F-143)", async () => {
  await open();
  const row = rowOf("dashboard");
  const stops = [...row.querySelectorAll<HTMLElement>("a, button")].map((stop) => stop.closest(".tf-ctx") ? "key" : stop.closest(".tf-what") ? "link" : "action");
  expect(stops.indexOf("key")).toBeLessThan(stops.indexOf("action"));
});

test("after the last Wake, focus goes to the woken row itself, so another Enter opens nothing (F-148)", async () => {
  await open();
  const wake = button(shelfRows()[0]!, "Wake");
  wake.focus();
  wake.click();
  await tick();
  expect(shelfRows()).toHaveLength(0);
  expect((document.activeElement as HTMLElement).matches(".tf-inbox .tf-ib")).toBe(true);
});

test("a key's press lands on the next row itself where it has no such action, so a second Enter opens nothing", async () => {
  await open({
    extra: [entry({ key: "alert:kuma:3", category: "alert", title: "knobas-jira", reason: "knobas-jira is down", occurred_at: at(23, 50), entity_id: null, source_id: "kuma" }, [])],
  });
  const row = rowOf("knobas-jira");
  const place = rows().indexOf(row);
  const next = (rows()[place + 1] ?? rows()[place - 1]) as HTMLElement;
  expect(next.querySelector("[data-act=ack]"), "the next row isn't an alert").toBeNull();
  const ack = button(row, "Ack");
  ack.focus();
  ack.click();
  await settle();
  expect(document.activeElement).toBe(next);
});

test("after a mouse's press, focus rests on the next row, not on its link", async () => {
  await open();
  const row = rowOf("dashboard");
  const place = rows().indexOf(row);
  const next = (rows()[place + 1] ?? rows()[place - 1]) as HTMLElement;
  button(row, "Done").focus();
  button(row, "Done").dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 }));
  await settle();
  expect(document.activeElement).toBe(next);
});

test("Show in runs on a sent Re-run goes to the pipeline that lists the failed run", async () => {
  await open({ pipelines: async () => [{ entity_id: "tc:Payout", recent: [{ entity_id: "tc:Payout#1187" }] }] as never, sent: true });
  button(rowOf("Payout_Integration"), "Re-run").click();
  await settle();
  const toast = toasts.items.at(-1)!;
  expect([toast.text, toast.action?.label]).toEqual(["Sent: re-run Payout_IntegrationTests #1187.", "Show in runs"]);
  toast.action!.run();
  await settle();
  expect(location.hash).toBe("#/builds/tc%3APayout");
  location.hash = "";
});

test("Snooze offers the presets, and one moves the item to the snoozed line", async () => {
  await open();
  const row = rowOf("Idempotent");
  button(row, "Snooze…").click();
  flushSync();
  button(row, "Next Monday").click();
  await settle();
  const monday = new Date(2026, 7, 24, 9, 0).toISOString();
  expect(calls).toEqual([`snooze review_request:gitea:payouts#142 ${monday}`]);
  expect(toasts.items.map((toast) => toast.text)).toContain("Snoozed until Monday 09:00");
  expect(shelfRows().map((row) => text(row.querySelector(".tf-ctx")))).toEqual([
    "Credential, no job, back Saturday 29 Aug 2026",
    "Review, no job, back Monday 24 Aug 2026",
  ]);
});

test("a snooze's toast takes it back with Undo", async () => {
  await open({ clock: true });
  const row = rowOf("Idempotent");
  button(row, "Snooze…").click();
  flushSync();
  button(row, "Tomorrow").click();
  await settle();
  expect(rows().some((one) => one.textContent?.includes("Idempotent"))).toBe(false);
  toasts.items.at(-1)!.action!.run();
  await settle();
  expect(rows().some((one) => one.textContent?.includes("Idempotent"))).toBe(true);
});

test("rapid Done is one toast, and its Undo brings them all back", async () => {
  await open();
  for (const words of ["dashboard", "Idempotent", "flows-stage"]) button(rowOf(words), "Done").click();
  flushSync();
  expect(toasts.items.map((toast) => toast.text)).toEqual(["3 items done"]);
  await settle();
  expect(rows()).toHaveLength(1);
  toasts.items[0]!.action!.run();
  await settle();
  expect(rows()).toHaveLength(4);
});

test("a tally starts again once its toast has gone", async () => {
  await open();
  button(rowOf("dashboard"), "Done").click();
  expect(toasts.items.map((toast) => toast.text)).toEqual(["Done: Payout dashboard latency"]);
  dismiss(toasts.items[0]!.id);
  button(rowOf("Idempotent"), "Done").click();
  expect(toasts.items.map((toast) => toast.text)).toEqual(["Done: Idempotent retry window"]);
});

test("a Wake that fails says so in a toast with a way to try again, and draws no line above the list", async () => {
  failing["credential_expiry:jira"] = { code: "internal", message: "the database is away", source_id: null };
  await open();
  button(shelfRows()[0]!, "Wake").click();
  await settle();
  const toast = toasts.items.at(-1)!;
  expect(toast.tone).toBe("err");
  expect(toast.text).toBe("Could not wake The Jira credential expires on Monday 7 Sep 2026: the database is away");
  expect(toast.action?.label).toBe("Try again");
  expect(target.querySelector(".tf-ifail")).toBeNull();
  expect(shelfRows()).toHaveLength(1);
  delete failing["credential_expiry:jira"];
  toast.action!.run();
  await settle();
  expect(shelfRows()).toHaveLength(0);
});

test("an item that left at its source says so in words, not its key", async () => {
  failing["new_assignment:jira:PAY-240"] = { code: "not_found", message: "no inbox item new_assignment:jira:PAY-240", source_id: null };
  await open();
  button(rowOf("dashboard"), "Done").click();
  await settle();
  expect(toasts.items.at(-1)!.text).toBe("Payout dashboard latency has already left the inbox. It was resolved at its source.");
  expect(target.textContent).not.toContain("no inbox item");
});

test("with no job chosen the bar draws no scope control", async () => {
  await open({ job: null });
  expect(target.querySelector(".tf-fbar .tf-seg")).toBeNull();
  expect(rows()).toHaveLength(4);
});

test("a credential row says its fix in the status bar's words and opens the prompt", async () => {
  health.replace([{ source_id: "kuma", state: "missing_secret", checked_at: at(9, 0), detail: null } as never]);
  await open({
    extra: [
      entry({ key: "failed_sign_in:kuma", category: "failed_sign_in", source_id: "kuma", entity_id: null, kind: null, title: "Kuma", reason: "no credential is stored for Kuma" }),
      entry({ key: "failed_sign_in:tc", category: "failed_sign_in", source_id: "tc", entity_id: null, kind: null, title: "TeamCity", reason: "the TeamCity sign-in was rejected" }),
    ],
  });
  const link = (words: string) => rowOf(words).querySelector<HTMLAnchorElement>(".tf-acts a")!;
  expect(text(link("No credential"))).toBe("Enter token…");
  expect(link("No credential").getAttribute("href")).toBe("#/settings/source/kuma/reenter");
  expect(text(link("was rejected"))).toBe("Re-enter password…");
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
  button(row, "Snooze…").click();
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
  button(row.querySelector(".tf-isnooze")!, "Snooze").click();
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

test("Snooze with no day picked goes to the date field and says what it wants (F-148)", async () => {
  const row = await openPicker();
  const press = button(row.querySelector(".tf-isnooze")!, "Snooze");
  expect(press.disabled).toBe(false);
  press.click();
  flushSync();
  expect(document.activeElement).toBe(picker(row));
  expect(row.querySelector(".tf-isnooze")!.textContent).toContain("Pick a day after today");
  expect(snoozeCalls()).toEqual([]);
});

test("a pointer's pick leaves an empty box under the pointer until it moves, so a second press lands on nothing (F-154)", async () => {
  const row = await openPicker();
  button(row, "Tomorrow").dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 }));
  await settle();
  const spent = document.querySelector(".tf-isnooze-spent")!;
  expect(spent.querySelector("button")).toBeNull();
  window.dispatchEvent(new PointerEvent("pointermove", { clientX: 5000, clientY: 5000 }));
  await settle();
  expect(document.querySelector(".tf-isnooze-spent")).toBeNull();
});

test("a pointer's pick guards only its spot, so a press on the next row's action under the float lands (F-154, F-159)", async () => {
  const row = await openPicker();
  button(row, "Tomorrow").dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1, clientX: 400, clientY: 300 }));
  await settle();
  const spent = document.querySelector<HTMLElement>(".tf-isnooze-spent")!;
  expect([spent.style.left, spent.style.top, spent.style.width, spent.style.height]).toEqual(["394px", "294px", "12px", "12px"]);
});

test("a pointer's pick holds its row too, so a row held above it keeps its index (F-159)", async () => {
  const row = await openPicker();
  const key = (row as HTMLElement).dataset["row"];
  const at = rows().indexOf(row);
  button(row, "Tomorrow").dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 }));
  await settle();
  const held = target.querySelector<HTMLElement>(".tf-ib-held")!;
  expect(held.dataset["row"]).toBe(key);
  expect(rows().indexOf(held)).toBe(at);
});

test("a key's pick leaves no box: focus has moved on with it (F-154)", async () => {
  const row = await openPicker();
  button(row, "Tomorrow").click();
  await settle();
  expect(document.querySelector(".tf-isnooze-spent")).toBeNull();
});

test("↓ walks the Snooze choices, and Tab past the last closes them (F-154)", async () => {
  const row = await openPicker();
  const float = row.querySelector(".tf-isnooze")!;
  const keydown = (key: string) => document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
  expect(document.activeElement?.textContent).toBe("Tomorrow");
  keydown("ArrowDown");
  expect(document.activeElement?.textContent).toBe("Next Monday");
  button(float, "Snooze").focus();
  keydown("Tab");
  flushSync();
  expect(row.querySelector(".tf-isnooze")).toBeNull();
});

test("a press elsewhere or another address closes the Snooze choices, and a press on Snooze… is its own toggle (F-154)", async () => {
  const row = await openPicker();
  document.body.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
  flushSync();
  expect(row.querySelector(".tf-isnooze")).toBeNull();

  button(row, "Snooze…").click();
  flushSync();
  window.dispatchEvent(new HashChangeEvent("hashchange"));
  flushSync();
  expect(row.querySelector(".tf-isnooze")).toBeNull();

  button(row, "Snooze…").click();
  flushSync();
  button(row, "Snooze…").dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
  flushSync();
  expect(row.querySelector(".tf-isnooze"), "the press's own click closes it").not.toBeNull();
  button(row, "Snooze…").click();
  flushSync();
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
  button(review, "Snooze…").click();
  flushSync();
  expect(text(review.querySelector(".tf-isnooze")), "a review has no expiry to hang it on").not.toContain("After it expires");

  const row = rowOf("TeamCity credential");
  button(row, "Snooze…").click();
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
        unackAlert: async () => {},
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

test("a row about a ticket links its ticket page by its entity id, so a key two sources hold opens the right one (#1676)", async () => {
  await open({
    extra: [
      entry({ key: "new_assignment:gitea:PAY-231", category: "new_assignment", source_id: "gitea", entity_id: "gitea:PAY-231",
        kind: "ticket", title: "Same key, other source", reason: "assigned to you", occurred_at: at(17, 0) }),
    ],
  });
  expect(rowOf("Same key").querySelector(".tf-ctx a")?.getAttribute("href")).toBe("#/ticket/gitea%3APAY-231");
});

test("a row's sentence names the ticket's source, so a key two sources hold opens the one the row is about (#1977, spec #2227)", async () => {
  await open({
    extra: [
      entry({ key: "new_assignment:gitea:PAY-231", category: "new_assignment", source_id: "gitea", entity_id: "gitea:PAY-231",
        kind: "ticket", title: "Same key, other source", reason: "assigned to you", occurred_at: at(17, 0) }),
    ],
  });
  expect(rowOf("Same key").querySelector(".tf-what a")?.getAttribute("href")).toBe("#/ticket/gitea%3APAY-231");
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
    button(row, "Snooze…").click();
    flushSync();
    button(row, "Tomorrow").click();
    await settle();
    expect(snoozeCalls()).toEqual([`snooze review_request:gitea:payouts#142 ${new Date(2026, 7, 24, 9, 0).toISOString()}`]);
  } finally {
    vi.useRealTimers();
  }
});

/** `n` older review requests, the oldest last. */
const many = (n: number) =>
  Array.from({ length: n }, (_, i) =>
    entry({ key: `review_request:bulk#${i}`, entity_id: `gitea:bulk#${i}`, title: `Bulk change ${i}`, occurred_at: at(9, 0, 20) }),
  );
const room = (side: "top" | "bottom") => target.querySelector<HTMLElement>(".tf-inbox")!.style.getPropertyValue(`padding-${side}`);

test("a long inbox draws the rows near the viewport and keeps the room of the rest below them (#2209)", async () => {
  await open({ extra: many(2000) });
  const drawn = rows().length;
  expect(drawn).toBeGreaterThan(5);
  expect(drawn).toBeLessThan(100);
  expect(whats()[0]).toContain("Idempotent retry window");
  expect(room("top")).toBe("");
  // Every undrawn row at its estimate: 63 px and the 6 px gap before it.
  expect(room("bottom")).toBe(`${(2004 - drawn) * 69}px`);

  // A filter that keeps few rows draws them all, with no room around them.
  ([...target.querySelectorAll(".tf-fchips button")].find((b) => b.textContent === "Build") as HTMLButtonElement).click();
  await settle();
  expect(rows()).toHaveLength(1);
  expect(room("bottom")).toBe("");
});

// A person's inbox of about a hundred items (Björn's held 102) windows too: drawn whole it was a 181 ms task on its
// first open in WebView2 at 4x, and its rows were drawn 231 ms after the press (the Windows pass, 2026-11-14).
test("an inbox of a hundred rows draws the rows near the viewport, as Monitors and Code do from 100", async () => {
  await open({ extra: many(100) });
  const drawn = rows().length;
  expect(drawn).toBeGreaterThan(5);
  expect(drawn).toBeLessThan(100);
  expect(room("bottom")).toBe(`${(104 - drawn) * 69}px`);
});

test("an inbox under 40 rows draws every row, as before windowing (#2209)", async () => {
  await open({ extra: many(35) });
  expect(rows()).toHaveLength(39);
  expect(target.querySelector<HTMLElement>(".tf-inbox")!.getAttribute("style")).toBeNull();
});

// A page keep-alive holds hidden (#2249): the components above mount inside it.
const page = $state({ shown: true });
const hiddenable = pageContext(() => page.shown);


test("an inbox whose page is hidden stops its clock, and shown again reads it at once (#2249)", async () => {
  vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval"] });
  try {
    vi.setSystemTime(new Date(2026, 7, 22, 23, 50));
    await open({ clock: true });
    page.shown = false;
    flushSync();
    vi.setSystemTime(new Date(2026, 7, 23, 0, 10));
    vi.advanceTimersByTime(90_000);
    flushSync();
    expect(text(rowOf("Idempotent").querySelector("time"))).toBe("13:50");

    page.shown = true;
    flushSync();
    expect(text(rowOf("Idempotent").querySelector("time"))).toBe("yesterday 13:50");
  } finally {
    vi.useRealTimers();
    page.shown = true;
  }
});
