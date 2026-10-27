/**
 * The inbox store (issue #45). The view that drew it went with the rooms UI
 * (#881); the new shell's Inbox has its own tests (`tf/inbox`).
 *
 * The one that has to be pinned on this side of the bridge: **the badge is the
 * backend's count and not the length of whatever list this window is
 * holding**. The two differ exactly when something is snoozed, which is the
 * case the number exists to get right.
 */
import { beforeEach, expect, test, vi } from "vitest";

import type { InboxEntry, InboxShelf } from "../ipc/entity";
import { waitFor } from "../shell/test-wait";

const calls: string[] = [];
let stream: InboxEntry[] = [];
let snoozed: InboxEntry[] = [];
let count = 0;
/** Set to make the next command reject, so the error path is reachable. */
let reject: unknown = null;
/** Set to hold the next answer (a write) open until it resolves (#775). */
let hold: Promise<void> | null = null;

function record<T>(what: string, value: T): Promise<T> {
  calls.push(what);
  if (hold !== null) {
    const gate = hold;
    hold = null;
    return gate.then(() => value);
  }
  if (reject !== null) {
    const error = reject;
    reject = null;
    return Promise.reject(error);
  }
  return Promise.resolve(value);
}

vi.mock("../ipc/entity", () => ({
  // Contexts (#47): one context whose only member is the fixture's review
  // request, so the per-context filter has something to keep and something to
  // drop.
  listContexts: () =>
    Promise.resolve([
      {
        id: "ctx:pay",
        kind: "adhoc",
        title: "Payout retries",
        anchor_id: null,
        created_at: "2026-08-25T12:00:00Z",
        archived_at: null,
      },
    ]),
  promoteContext: () => Promise.reject(new Error("no promotion in this test")),
  inboxItems: (shelf: InboxShelf) =>
    record(`items ${shelf}`, shelf === "stream" ? stream : snoozed),
  inboxCount: () => record("count", count),
  snoozeInboxItem: (itemKey: string, until: string) =>
    record(`snooze ${itemKey} ${until}`, undefined),
  completeInboxItem: (itemKey: string) => record(`complete ${itemKey}`, undefined),
  submitWrite: (payload: unknown) =>
    record(`write ${JSON.stringify(payload)}`, { id: 900 + calls.length, state: "pending" }),
}));

/** What the view handed the queue's follower (#775), and how it would reopen. */
const followed: { write: { id: number }; following: { name: string; resend?: () => void } }[] = [];
vi.mock("../shell/follow-write", () => ({
  followWrite: (write: { id: number }, following: { name: string; resend?: () => void }) => {
    followed.push({ write, following });
    return () => {};
  },
}));

// The ack is an **estate** command and not an inbox one (#446): what clears
// the item is the alert reading acked, which is `assets::ack_alert`'s write.
// Only the one export this store imports, so a call to anything else in that
// module from this window would fail here rather than reach a real bridge.
vi.mock("../ipc/assets", () => ({
  ackAlert: (monitorId: string) => record(`ack ${monitorId}`, undefined),
}));

/** No Tauri bridge in jsdom; the view must still mount. */
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

vi.mock("../shell/open-external", () => ({
  openExternal: (url: string) => record(`open ${url}`, undefined),
}));

const { createInbox, pickedMorning, snoozePresets, subjectOf } = await import("./inbox.svelte");
const { drawable, formFor } = await import("./actions");

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

beforeEach(() => {
  calls.length = 0;
  stream = [];
  snoozed = [];
  count = 0;
  reject = null;
  hold = null;
  followed.length = 0;
});

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

/**
 * **`answered` says whether the stream on this store is a read** (#290).
 *
 * The notifier's whole backlog rule hangs off it: an empty stream that has
 * never been read is not an empty inbox, and a notifier primed against it
 * would take the reader's first real read for news. A failed read leaves it
 * false, which is the same rule the count follows for the same reason — a read
 * that failed says nothing about what is in the inbox.
 */
test("the stream is not an answer until a read has succeeded", async () => {
  const inbox = createInbox();
  expect(inbox.answered, "an empty inbox and an unread one are not the same thing").toBe(false);

  reject = { code: "not_ready", message: "the database is still starting" };
  await inbox.refresh();
  expect(inbox.answered, "a read that failed is not an answer").toBe(false);

  stream = [entry()];
  count = 1;
  await inbox.refresh();
  expect(inbox.answered).toBe(true);
});

/**
 * The perf audit after #2200: both shelves are held as read, not as deep
 * state. A proxy over 20,000 rows subscribed every field each row draws read,
 * and tore those reactions down one by one on the next keystroke (1.7 s for
 * one *Done*). Every answer replaces a shelf whole, so nothing needs the
 * proxy.
 */
test("the shelves hold the backend's rows as read, not a deep proxy of them", async () => {
  const inbox = createInbox();
  stream = [entry()];
  snoozed = [entry({ key: "mention:jira:PAY-231", snoozed_until: "2026-09-01T07:00:00Z" })];
  count = 1;
  await inbox.refresh();
  expect(inbox.stream[0]).toBe(stream[0]);
  expect(inbox.snoozed[0]).toBe(snoozed[0]);
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

/**
 * The key's inverse, on the one part of it that is not obvious: a subject may
 * itself carry colons and slashes and hashes (`gitea:acme/payouts#144`), and
 * only the **first** colon is the category boundary.
 *
 * `knobas_core::inbox` composes the key; this is the only place on this side
 * of the bridge that takes one apart, which is why it is here and not in a
 * component.
 */
test("the subject of a key is everything after the first colon", () => {
  expect(subjectOf("alert:kuma:7")).toBe("kuma:7");
  expect(subjectOf("review_request:gitea:acme/payouts#144")).toBe("gitea:acme/payouts#144");
  expect(subjectOf("credential_expiry:jira")).toBe("jira");
});

/**
 * A credential verdict moves the inbox (#1571): a failed sign-in is an item, and
 * *Test connection* or *Re-enter* settles a verdict with no run behind it, so
 * no `sync:state` would ever say so.
 */
test("the inbox re-reads when a source's credential health moves", async () => {
  const handlers = new Map<string, () => void>();
  let reads = 0;
  const store = createInbox({
    inboxItems: () => Promise.resolve([]),
    inboxCount: () => {
      reads += 1;
      return Promise.resolve(0);
    },
    snoozeInboxItem: () => Promise.resolve(),
    completeInboxItem: () => Promise.resolve(),
    ackAlert: () => Promise.resolve(),
    listen: (event, handler) => {
      handlers.set(event, handler);
      return Promise.resolve(() => {});
    },
  });
  const stop = store.start();
  await waitFor(() => expect(handlers.has("source:health")).toBe(true));
  handlers.get("source:health")!();
  await waitFor(() => expect(reads).toBe(1));
  stop();
});

/**
 * The inbox re-reads on `activity:new`, `sync:state` and `source:health`, at
 * three statements a read, and a *Sync all* is a burst of events (#753): while
 * a read is in flight, the burst makes one more read after it, not one per
 * event.
 */
test("a burst of events during a read makes one more read, not one per event", async () => {
  const handlers: (() => void)[] = [];
  let reads = 0;
  let land!: () => void;
  const store = createInbox({
    inboxItems: () => Promise.resolve([]),
    inboxCount: () => {
      reads += 1;
      return reads === 1 ? new Promise<number>((resolve) => (land = () => resolve(0))) : Promise.resolve(0);
    },
    snoozeInboxItem: () => Promise.resolve(),
    completeInboxItem: () => Promise.resolve(),
    ackAlert: () => Promise.resolve(),
    listen: (_event, handler) => {
      handlers.push(handler);
      return Promise.resolve(() => {});
    },
  });
  const stop = store.start();
  await waitFor(() => expect(handlers).toHaveLength(3));

  for (let n = 0; n < 6; n++) for (const handler of handlers) handler();
  expect(reads).toBe(1);
  land();
  await waitFor(() => expect(reads).toBe(2));
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(reads).toBe(2);
  stop();
});

// -- answered at once (#775) ---------------------------------------------------

test("a refused done puts the row back with the re-read and says why", async () => {
  stream = [entry()];
  count = 1;
  const inbox = createInbox();
  await inbox.refresh();
  reject = { code: "internal", message: "pool timed out", source_id: null };
  await inbox.complete("review_request:gitea:acme/payouts#144");
  expect(inbox.stream.map((e) => e.item.key)).toEqual(["review_request:gitea:acme/payouts#144"]);
  expect(inbox.error).toBe("pool timed out");
});

test("a snooze moves the row to the shelf at once", async () => {
  stream = [entry()];
  count = 1;
  const inbox = createInbox();
  await inbox.refresh();
  hold = new Promise(() => {});
  const until = new Date(Date.now() + 86_400_000);
  void inbox.snooze("review_request:gitea:acme/payouts#144", until);
  expect(inbox.stream).toEqual([]);
  expect(inbox.snoozed.map((e) => [e.item.key, e.item.snoozed_until])).toEqual([
    ["review_request:gitea:acme/payouts#144", until.toISOString()],
  ]);
  expect(inbox.count).toBe(0);
});

test("a re-read that lands while an answer is out does not draw the row back", async () => {
  stream = [entry()];
  count = 1;
  const inbox = createInbox();
  await inbox.refresh();
  let release!: () => void;
  hold = new Promise((resolve) => (release = resolve));
  const done = inbox.complete("review_request:gitea:acme/payouts#144");
  // An event's re-read, answered by a backend that has not seen the answer yet.
  await inbox.refresh();
  expect(inbox.stream).toEqual([]);
  stream = [];
  count = 0;
  release();
  await done;
  expect(inbox.stream).toEqual([]);
  expect(inbox.count).toBe(0);
});

test("a re-read taken while an answer was out cannot draw the row back after the answer's own re-read", async () => {
  stream = [entry()];
  count = 1;
  const inbox = createInbox();
  await inbox.refresh();
  let releaseAnswer!: () => void;
  hold = new Promise((resolve) => (releaseAnswer = resolve));
  const done = inbox.complete("review_request:gitea:acme/payouts#144");
  // An event's re-read, taken before the backend saw the answer, and slow.
  let releaseRead!: () => void;
  hold = new Promise((resolve) => (releaseRead = resolve));
  const early = inbox.refresh();
  // The answer lands, and its own re-read draws the backend's empty stream.
  stream = [];
  count = 0;
  releaseAnswer();
  await done;
  expect(inbox.stream).toEqual([]);
  // The early read comes back last, with the row it saw: it is older, so it is dropped.
  releaseRead();
  await early;
  expect(inbox.stream).toEqual([]);
  expect(inbox.count).toBe(0);
});

test("pickedMorning is 09:00 local on a day after today, and null for anything else (#1437)", () => {
  const now = new Date(2026, 7, 22, 15, 0);
  expect(pickedMorning("2026-08-23", now)).toEqual(new Date(2026, 7, 23, 9, 0));
  expect(pickedMorning("0002-11-02", now), "a two-digit year stays literal, not 1902").toBeNull();
  expect(pickedMorning("1902-11-02", now), "a past day").toBeNull();
  expect(pickedMorning("2026-08-22", now), "today").toBeNull();
  expect(pickedMorning("2026-08-21", now), "yesterday").toBeNull();
  expect(pickedMorning("", now)).toBeNull();
  expect(pickedMorning("2026-08", now)).toBeNull();
  expect(pickedMorning("0999-01-01", now)).toBeNull();
  expect(pickedMorning("20261-01-01", now), "a five-digit year is not a four-digit year").toBeNull();
});
