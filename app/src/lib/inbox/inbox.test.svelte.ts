/**
 * The inbox as a reader sees it (issue #45).
 *
 * What is asserted is what appears, what a press does, and — the one that has
 * to be pinned on this side of the bridge — that **the badge is the backend's
 * count and not the length of whatever list this window is holding**. The two
 * differ exactly when something is snoozed, which is the case the number
 * exists to get right.
 */
import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type { InboxEntry, InboxShelf } from "../ipc/entity";

const calls: string[] = [];
let stream: InboxEntry[] = [];
let snoozed: InboxEntry[] = [];
let count = 0;
/** Set to make the next command reject, so the error path is reachable. */
let reject: unknown = null;

function record<T>(what: string, value: T): Promise<T> {
  calls.push(what);
  if (reject !== null) {
    const error = reject;
    reject = null;
    return Promise.reject(error);
  }
  return Promise.resolve(value);
}

vi.mock("../ipc/entity", () => ({
  inboxItems: (shelf: InboxShelf) =>
    record(`items ${shelf}`, shelf === "stream" ? stream : snoozed),
  inboxCount: () => record("count", count),
  snoozeInboxItem: (itemKey: string, until: string) =>
    record(`snooze ${itemKey} ${until}`, undefined),
  completeInboxItem: (itemKey: string) => record(`complete ${itemKey}`, undefined),
  submitWrite: (payload: unknown) => record(`write ${JSON.stringify(payload)}`, undefined),
}));

/** No Tauri bridge in jsdom; the view must still mount. */
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

vi.mock("../shell/open-external", () => ({
  openExternal: (url: string) => record(`open ${url}`, undefined),
}));

const { default: InboxView } = await import("./InboxView.svelte");
const { createInbox, snoozePresets } = await import("./inbox.svelte");
const { drawable, formFor } = await import("./actions");

/** A Wednesday, so "next Monday" is five days out and not a special case. */
const NOW = new Date("2026-08-26T12:00:00Z");

function entry(over: Partial<InboxEntry["item"]> = {}, actions: string[] = []): InboxEntry {
  return {
    item: {
      key: "review_request:gitea:acme/payouts#144",
      category: "review_request",
      source_id: "gitea",
      entity_id: "gitea:acme/payouts#144",
      kind: "pr",
      title: "Add payout CSV export",
      reason: "jonas.becker asked for your review",
      occurred_at: "2026-08-26T09:00:00Z",
      web_url: "https://gitea.example/acme/payouts/pulls/144",
      snoozed_until: null,
      ...over,
    },
    actions,
  };
}

const router = {
  route: { view: "inbox" as const },
  ctx: "all",
  hash: "",
  go(hash: string) {
    this.hash = hash;
  },
  back() {},
  start: () => () => {},
};

beforeEach(() => {
  calls.length = 0;
  stream = [];
  snoozed = [];
  count = 0;
  reject = null;
  router.hash = "";
});

async function draw(inbox: ReturnType<typeof createInbox>) {
  const target = document.createElement("div");
  document.body.append(target);
  const app = mount(InboxView, { target, props: { router, inbox, now: NOW } });
  await Promise.resolve();
  flushSync();
  return {
    target,
    text: () => (target.textContent ?? "").replace(/\s+/g, " "),
    button: (label: string) =>
      [...target.querySelectorAll<HTMLButtonElement>("button")].find(
        (button) => button.textContent?.trim() === label,
      ),
    stop: () => {
      unmount(app);
      target.remove();
    },
  };
}

// -- the count ---------------------------------------------------------------

/**
 * Story 19, on this side of the bridge. The stream holds one row and the
 * backend says one; a second item is on the snoozed shelf and the number does
 * **not** move.
 *
 * A badge computed as `stream.length + snoozed.length` would read 2 here, and
 * a badge computed from any list at all would be a second definition of "needs
 * me now" living in the window.
 */
test("the count is the backend's number, and a snoozed item is not in it", async () => {
  stream = [entry()];
  snoozed = [
    entry({
      key: "mention:jira:PAY-231",
      category: "mention",
      source_id: "jira",
      entity_id: "jira:PAY-231",
      kind: "ticket",
      title: "Payout retry storm",
      snoozed_until: "2026-08-28T09:00:00Z",
    }),
  ];
  count = 1;

  const inbox = createInbox();
  await inbox.refresh();
  expect(inbox.count).toBe(1);
  expect(inbox.stream).toHaveLength(1);
  expect(inbox.snoozed).toHaveLength(1);
});

/**
 * A failed read leaves the number where it was rather than zeroing it: a badge
 * that blinked to zero during a hiccup is a review request the reader stops
 * looking for.
 */
test("a failed read does not zero the count", async () => {
  count = 3;
  const inbox = createInbox();
  await inbox.refresh();
  expect(inbox.count).toBe(3);

  reject = { code: "internal", message: "the database went away" };
  await inbox.refresh();
  expect(inbox.count).toBe(3);
  expect(inbox.error).toBe("the database went away");
});

// -- what the stream draws ---------------------------------------------------

/** Story 7: which source, and what it is about, without opening it. */
test("an item says where it came from and why it is here", async () => {
  stream = [entry()];
  count = 1;
  const inbox = createInbox();
  await inbox.refresh();
  const view = await draw(inbox);

  expect(view.text()).toContain("Add payout CSV export");
  expect(view.text()).toContain("jonas.becker asked for your review");
  expect(view.text()).toContain("GI");
  view.stop();
});

/**
 * Story 23: only the actions the backend offered are drawn. The entry below
 * offers `comment` alone — a source that cannot approve — and *Approve* must
 * not be on screen.
 */
test("only the offered actions are drawn", async () => {
  stream = [entry({}, ["comment"])];
  count = 1;
  const inbox = createInbox();
  await inbox.refresh();
  const view = await draw(inbox);

  expect(view.button("Comment")).toBeTruthy();
  expect(view.button("Approve")).toBeFalsy();
  view.stop();
});

/** Story 12: approving is one press, and it goes through `submit_write`. */
test("approving an item submits the op the backend offered", async () => {
  stream = [entry({}, ["approve", "comment"])];
  count = 1;
  const inbox = createInbox();
  await inbox.refresh();
  const view = await draw(inbox);

  view.button("Approve")!.click();
  await Promise.resolve();
  await Promise.resolve();
  flushSync();

  expect(
    calls.some((call) =>
      call.startsWith('write {"Approve":{"entity":"gitea:acme/payouts#144","body":""}}'),
    ),
    `no approve reached the write queue: ${calls.join(" | ")}`,
  ).toBe(true);
  view.stop();
});

/** Story 8: the inbox is a way in, not a dead end. */
test("opening an item navigates to its entity, with the key encoded", async () => {
  stream = [entry()];
  count = 1;
  const inbox = createInbox();
  await inbox.refresh();
  const view = await draw(inbox);

  view.button("Open")!.click();
  flushSync();
  // `#` truncates a fragment at the browser level, so it must be encoded and
  // the namespace colon must not be.
  expect(router.hash).toBe("#/entity/gitea:acme%2Fpayouts%23144");
  view.stop();
});

/**
 * The one item with no entity behind it. *Open* is absent rather than pointing
 * somewhere wrong, and there is nothing to ask a source for — but it is still
 * a row, with snooze and done.
 */
test("a credential expiry has no open and no write action, and is still a row", async () => {
  stream = [
    entry({
      key: "credential_expiry:jira",
      category: "credential_expiry",
      source_id: "jira",
      entity_id: null,
      kind: null,
      title: "Tidewater Jira",
      reason: "the Tidewater Jira credential expires on Friday 4 Sep 2026",
      web_url: null,
    }),
  ];
  count = 1;
  const inbox = createInbox();
  await inbox.refresh();
  const view = await draw(inbox);

  expect(view.text()).toContain("credential expires");
  expect(view.button("Open")).toBeFalsy();
  expect(view.button("In browser")).toBeFalsy();
  expect(view.button("Snooze")).toBeTruthy();
  expect(view.button("Done")).toBeTruthy();
  view.stop();
});

/**
 * Story 15: a deferred item is dimmed and says when it comes back — it does
 * not vanish. An item that disappeared entirely would be indistinguishable
 * from one that was deleted, which is what snooze must never mean.
 */
test("a snoozed item is on the shelf, not gone", async () => {
  snoozed = [entry({ snoozed_until: "2026-08-28T09:00:00Z" })];
  const inbox = createInbox();
  await inbox.refresh();
  const view = await draw(inbox);

  expect(view.text()).toContain("Snoozed");
  expect(view.text()).toContain("Add payout CSV export");
  expect(view.target.querySelector(".inbox-item.dim")).toBeTruthy();
  view.stop();
});

/** Story 16, through the interface. */
test("done sends the item key the backend gave, never a composed one", async () => {
  stream = [entry()];
  count = 1;
  const inbox = createInbox();
  await inbox.refresh();
  const view = await draw(inbox);

  view.button("Done")!.click();
  await Promise.resolve();
  flushSync();
  expect(calls).toContain("complete review_request:gitea:acme/payouts#144");
  view.stop();
});

// -- snooze presets ----------------------------------------------------------

/**
 * Story 14. *Tomorrow* is the start of the next day, not "in 24 hours": an
 * item snoozed at 16:40 that came back at 16:40 would arrive at the end of a
 * day rather than at the beginning of one.
 */
test("tomorrow is the next morning, not twenty-four hours later", () => {
  const evening = new Date(2026, 7, 26, 16, 40);
  const [tomorrow] = snoozePresets(evening);
  expect(tomorrow!.label).toBe("Tomorrow");
  expect(tomorrow!.at.getDate()).toBe(27);
  expect(tomorrow!.at.getHours()).toBe(9);
});

/**
 * **Never today.** Asked on a Monday, *next Monday* is the one in seven days'
 * time — an item that came back the instant it was deferred would make the
 * button read as a bug, and a preset that resolves into the past is worse
 * still.
 */
test("next monday is never today, whatever day it is asked on", () => {
  for (let day = 0; day < 7; day += 1) {
    const at = new Date(2026, 7, 24 + day, 12, 0);
    const monday = snoozePresets(at).find((preset) => preset.label === "Next Monday")!;
    expect(monday.at.getDay(), `asked on day ${at.getDay()}`).toBe(1);
    expect(monday.at.getTime(), `asked on day ${at.getDay()}`).toBeGreaterThan(at.getTime());
  }
});

/**
 * *After it expires* is offered only where there is a deadline to hang it on.
 * On the four categories that have none it would compute to the same thing as
 * *Tomorrow*, and a button that duplicates its neighbour teaches nothing.
 */
test("the expiry preset appears only when there is a deadline", () => {
  const at = new Date(2026, 7, 26, 12, 0);
  expect(snoozePresets(at).map((preset) => preset.label)).toEqual(["Tomorrow", "Next Monday"]);

  const withDeadline = snoozePresets(at, new Date(2026, 8, 4, 12, 0));
  expect(withDeadline.map((preset) => preset.label)).toEqual([
    "Tomorrow",
    "Next Monday",
    "After it expires",
  ]);
  expect(withDeadline[2]!.at.getDate()).toBe(5);
});

/** A deadline already past is not a snooze target — it is a return to now. */
test("a deadline in the past offers no expiry preset", () => {
  const at = new Date(2026, 7, 26, 12, 0);
  expect(snoozePresets(at, new Date(2026, 7, 20, 12, 0))).toHaveLength(2);
  expect(snoozePresets(at, new Date(Number.NaN))).toHaveLength(2);
});

// -- the action forms --------------------------------------------------------

/**
 * An op this build has no form for is **skipped**, not drawn dead. `WriteOp`
 * grows per milestone (ADR-0006), so a newer backend can offer an identifier
 * this window has never heard of, and a button that cannot work is exactly
 * what story 23 forbids.
 */
test("an action this build cannot draw is skipped rather than shown", () => {
  expect(formFor("transition")).toBeNull();
  expect(drawable(["approve", "transition", "comment"]).map((entry) => entry.op)).toEqual([
    "approve",
    "comment",
  ]);
});
