import { afterEach, expect, test, vi } from "vitest";

import { EVENTS } from "../ipc";
import type { Unread, WriteUnread } from "../ipc/sources";
import { reset, toasts } from "./toasts.svelte";
import { startSayingUnread, unreadNotes, unreadWords, type UnreadHow, type UnreadListen } from "./unread-toast";

const NOW = Date.parse("2026-11-28T10:00:00Z");

function line(over: Partial<Unread>): Unread {
  return { entity_id: "jira:PAY-1", title: "PAY-1", source_id: "jira", source_name: "Jira", by_hand: false, why: "not_offered", next_run_at: null, ...over };
}

function note(unread: Unread[], over: Partial<WriteUnread> = {}): WriteUnread {
  return { write_id: 7, entity_id: "gitea:tide/pay#7", title: "Retry SEPA payouts", unread, ...over };
}

/** A bridge whose events the test sends. */
function bridge() {
  const handlers = new Map<string, Set<(event: { payload: unknown }) => void>>();
  const listen: UnreadListen = async (event, handler) => {
    const set = handlers.get(event) ?? new Set();
    set.add(handler);
    handlers.set(event, set);
    return () => set.delete(handler);
  };
  const send = (event: string, payload: unknown) => {
    for (const handler of handlers.get(event) ?? []) handler({ payload });
  };
  const hearing = (event: string) => handlers.get(event)?.size ?? 0;
  return { listen, send, hearing };
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
const texts = () => toasts.items.map((toast) => toast.text);
const run = (source_id: string, running: boolean) => ({ source_id, running, last_outcome: running ? null : "ok" });

afterEach(() => reset());

/** Say `told` as the shell does, one note after another. */
function say(how: UnreadHow, ...told: WriteUnread[]) {
  const notes = unreadNotes(how);
  for (const one of told) notes.say(one);
  return notes.stop;
}

test("each case says what shows when, naming the source and, where known, when it runs next", () => {
  const say = (unread: Unread[], over: Partial<WriteUnread> = {}) => unreadWords(note(unread, over), unread, NOW);
  // Not offered by its adapter, at the write's own source: the run that follows the write brings it.
  expect(say([line({ entity_id: "surveys:survey-1", title: "Quay crane 4 inspection", source_id: "surveys", source_name: "Surveys" })])).toBe(
    "Quay crane 4 inspection shows after the next sync of Surveys.",
  );
  // Held past its patience by a run of its source.
  expect(say([line({ why: "run_in_flight" })])).toBe("PAY-1 shows once the sync of Jira under way ends.");
  // Another source, scheduled: when its next run comes.
  expect(say([line({ why: "failed", next_run_at: "2026-11-28T10:14:00Z" })])).toBe("PAY-1 shows after the next sync of Jira, in about 14 min.");
  expect(say([line({ next_run_at: "2026-11-28T13:00:00Z" })])).toBe("PAY-1 shows after the next sync of Jira, in about 3 h.");
  // Turned off.
  expect(say([line({ why: "turned_off" })])).toBe("PAY-1 shows after the next sync of Jira, which is turned off.");
  // Past the cap on views one write reads.
  expect(say([line({ entity_id: null, why: "left_out" })], { entity_id: "jira:PAY-1", title: "PAY-1" })).toBe(
    "The other boards and filters with PAY-1 show after the next sync of Jira.",
  );
  // A source that syncs by hand, and a source's own action beside its item.
  expect(say([line({ entity_id: null, title: "Retry SEPA payouts", source_id: "gitea", source_name: "Gitea", by_hand: true, why: "by_hand", next_run_at: "2026-11-28T10:14:00Z" })])).toBe(
    "What else the write to Retry SEPA payouts moved shows after Sync now on Gitea.",
  );
  const survey = { title: "Quay crane 4 inspection", source_id: "surveys", source_name: "Surveys" };
  expect(say([line({ ...survey, entity_id: "surveys:survey-1" }), line({ ...survey, entity_id: null, why: "unknown" })], { entity_id: "surveys:survey-1" })).toBe(
    "Quay crane 4 inspection and what else the write moved show after the next sync of Surveys.",
  );
});

test("one sentence per source and wording, naming one item and counting the rest", () => {
  const three = [line({}), line({ entity_id: "jira:PAY-2", title: "PAY-2" }), line({ entity_id: "jira:PAY-3", title: "PAY-3" })];
  expect(unreadWords(note(three), three, NOW)).toBe("PAY-1 and 2 more show after the next sync of Jira.");
  const two = [line({ why: "run_in_flight" }), line({ entity_id: "tc:b-9", title: "Payout build", source_id: "tc", source_name: "TeamCity" })];
  expect(unreadWords(note(two), two, NOW)).toBe("PAY-1 shows once the sync of Jira under way ends. Payout build shows after the next sync of TeamCity.");
});

test("the toast goes once everything it names has arrived: a refresh of the item, or a run of its source that began after it", async () => {
  const b = bridge();
  const stop = say({ listen: b.listen, now: () => NOW }, note([line({}), line({ entity_id: "tc:b-9", title: "Payout build", source_id: "tc", source_name: "TeamCity" })]));
  await settle();
  expect(texts()).toEqual(["PAY-1 shows after the next sync of Jira. Payout build shows after the next sync of TeamCity."]);
  // A failed refresh brings nothing; a run that began before the toast does not count.
  b.send(EVENTS.itemRefreshed, { entity_id: "jira:PAY-1", write_id: 7, ended: "failed" });
  b.send(EVENTS.syncState, run("tc", false));
  expect(texts()).toEqual(["PAY-1 shows after the next sync of Jira. Payout build shows after the next sync of TeamCity."]);
  b.send(EVENTS.itemRefreshed, { entity_id: "jira:PAY-1", write_id: 7, ended: "refreshed" });
  expect(texts()).toEqual(["Payout build shows after the next sync of TeamCity."]);
  b.send(EVENTS.syncState, run("tc", true));
  b.send(EVENTS.syncState, run("tc", false));
  expect(texts()).toEqual([]);
  expect(b.hearing(EVENTS.itemRefreshed)).toBe(0);
  stop();
});

test("held behind a run under way, it goes when that run ends", async () => {
  const b = bridge();
  say({ listen: b.listen, now: () => NOW }, note([line({ why: "run_in_flight" })]));
  await settle();
  b.send(EVENTS.syncState, run("jira", false));
  expect(texts()).toEqual([]);
});

test("a source that syncs by hand offers Sync now, which runs it", async () => {
  const b = bridge();
  const sync = vi.fn();
  say({ listen: b.listen, sync, now: () => NOW }, note([line({ entity_id: null, source_id: "gitea", source_name: "Gitea", by_hand: true, why: "by_hand" })]));
  await settle();
  expect(toasts.items[0]!.action?.label).toBe("Sync now");
  toasts.items[0]!.action!.run();
  expect(sync).toHaveBeenCalledWith("gitea");
});

test("the shell says each write:unread the backend tells, and nothing once torn down", async () => {
  const b = bridge();
  const stop = startSayingUnread({ listen: b.listen, now: () => NOW });
  await settle();
  b.send(EVENTS.writeUnread, note([line({ why: "run_in_flight" })]));
  expect(texts()).toEqual(["PAY-1 shows once the sync of Jira under way ends."]);
  stop();
  b.send(EVENTS.writeUnread, note([line({})], { write_id: 8 }));
  expect(texts()).toHaveLength(1);
});

test("a burst of landed writes is one toast, each line going as it arrives", async () => {
  const b = bridge();
  const gitea = { entity_id: null, source_id: "gitea", source_name: "Gitea", by_hand: true, why: "by_hand" as const };
  say(
    { listen: b.listen, now: () => NOW },
    note([line({ ...gitea, title: "Retry SEPA payouts" })], { write_id: 7, title: "Retry SEPA payouts" }),
    note([line({ ...gitea, title: "Fix rounding" })], { write_id: 8, entity_id: "gitea:tide/pay#8", title: "Fix rounding" }),
    note([line({ ...gitea, title: "Pin the toolchain" })], { write_id: 9, entity_id: "gitea:tide/pay#9", title: "Pin the toolchain" }),
    note([line({ why: "run_in_flight" })], { write_id: 10, entity_id: "jira:PAY-1", title: "PAY-1" }),
  );
  await settle();
  expect(texts()).toEqual([
    "What else the write to Retry SEPA payouts moved and 2 more show after Sync now on Gitea. PAY-1 shows once the sync of Jira under way ends.",
  ]);
  expect(toasts.items[0]!.action?.label).toBe("Sync now");
  // One listener a kind for the toast, not one a note.
  expect(b.hearing(EVENTS.itemRefreshed)).toBe(1);
  b.send(EVENTS.syncState, run("jira", false));
  expect(texts()).toEqual(["What else the write to Retry SEPA payouts moved and 2 more show after Sync now on Gitea."]);
  b.send(EVENTS.syncState, run("gitea", true));
  b.send(EVENTS.syncState, run("gitea", false));
  expect(texts()).toEqual([]);
  expect(b.hearing(EVENTS.itemRefreshed)).toBe(0);
  // Once it has gone, the next note is a toast of its own.
  say({ listen: b.listen, now: () => NOW }, note([line({})], { write_id: 11 }));
  await settle();
  expect(texts()).toEqual(["PAY-1 shows after the next sync of Jira."]);
});
