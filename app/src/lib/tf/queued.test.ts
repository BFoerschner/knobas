/**
 * The ticket-first pages' echoes (#1424, #1542): drawn at once, kept for
 * the session, dropped once the source's read shows the write landed, and
 * dropped with a toast when the queue holds or refuses the write.
 */
import { afterEach, beforeEach, expect, test } from "vitest";

import type { QueuedWrite } from "../ipc/sources";
import type { Hear, WriteFollower } from "../shell/follow-write";
import { noteWithdrawn } from "../shell/write-queue.svelte";
import { toasts } from "../shell/toasts.svelte";
import { EVENTS } from "../ipc";
import { askAgain, forgetEchoes, echoWrite, echoOn, echoesOn, sameWords, settleEchoes, takeAgain, watchEchoes } from "./queued.svelte";

/** A follower the test speaks for: `hear(id, row)` is the queue's next read of write `id`. */
function fakeFollower() {
  const hears = new Map<number, Hear>();
  const follower: WriteFollower = {
    follow(id, hear) {
      hears.set(id, hear);
      return () => hears.delete(id);
    },
  };
  return { follower, hears, hear: (id: number, row: QueuedWrite | undefined) => hears.get(id)?.(row) };
}

const row = (id: number, state: QueuedWrite["state"], detail: string | null = null) =>
  ({ id, state, detail, op: "comment", entity_id: "jira:PAY-231" }) as QueuedWrite;

beforeEach(() => {
  toasts.items = [];
});
afterEach(() => forgetEchoes());

test("a mark stays until the read shows it landed, and a later read does not bring it back", () => {
  echoWrite("jira:PAY-231", "status", "In Review", { base: "In Progress" });
  const moved = (status: string) => (mark: { kind: string; base: string | null }) => mark.kind === "status" && mark.base !== status;

  settleEchoes("jira:PAY-231", moved("In Progress"));
  expect(echoOn("jira:PAY-231", "status")?.text).toBe("In Review");

  settleEchoes("jira:PAY-231", moved("In Review"));
  expect(echoOn("jira:PAY-231", "status")).toBeNull();

  // The ticket moved back by somebody else: the mark is gone for good.
  settleEchoes("jira:PAY-231", moved("In Progress"));
  expect(echoOn("jira:PAY-231", "status")).toBeNull();
});

test("a kind marked again replaces the earlier mark; `many` keeps each", () => {
  echoWrite("gitea:pr/7", "close", "close queued");
  echoWrite("gitea:pr/7", "close", "close queued");
  echoWrite("jira:PAY-231", "status", "In Review", { base: "To Do" });
  echoWrite("jira:PAY-231", "status", "Done", { base: "To Do" });
  echoWrite("jira:PAY-231", "comment", "One", { many: true });
  echoWrite("jira:PAY-231", "comment", "Two", { many: true });

  expect(echoesOn("gitea:pr/7").map((m) => m.text)).toEqual(["close queued"]);
  expect(echoesOn("jira:PAY-231", "status").map((m) => m.text)).toEqual(["Done"]);
  expect(echoesOn("jira:PAY-231", "comment").map((m) => m.text)).toEqual(["One", "Two"]);
  expect(echoesOn("jira:PAY-999")).toEqual([]);
});

test("a held or refused write drops its own mark and toasts with Edit and resend", () => {
  const { follower, hear, hears } = fakeFollower();
  let resent = 0;
  const following = { name: "Comment", resend: () => void (resent += 1) };
  echoWrite("jira:PAY-231", "comment", "First", { many: true, write: { id: 1 }, following, follower });
  echoWrite("jira:PAY-231", "comment", "Second", { many: true, write: { id: 2 }, following, follower });

  hear(2, row(2, "pending"));
  expect(echoesOn("jira:PAY-231", "comment").map((m) => m.text)).toEqual(["First", "Second"]);
  expect(toasts.items).toEqual([]);

  hear(2, row(2, "refused", "403 Forbidden"));
  expect(echoesOn("jira:PAY-231", "comment").map((m) => m.text)).toEqual(["First"]);
  expect(toasts.items.map((t) => [t.text, t.tone])).toEqual([["Comment was refused: 403 Forbidden", "err"]]);
  const resend = toasts.items[0]!.more?.find((a) => a.label === "Edit and resend");
  resend!.run();
  expect(resent).toBe(1);
  expect(hears.has(2)).toBe(false);
});

test("a sent write keeps its mark until the read shows it, and is no longer followed", () => {
  const { follower, hear, hears } = fakeFollower();
  echoWrite("jira:PAY-231", "comment", "Sent", { many: true, write: { id: 3 }, following: { name: "Comment" }, follower });
  hear(3, undefined);
  expect(hears.has(3)).toBe(false);
  expect(echoesOn("jira:PAY-231", "comment").map((m) => m.text)).toEqual(["Sent"]);
  expect(toasts.items).toEqual([]);
});

test("a write the answer carried no id for is drawn and not followed", () => {
  const { follower, hears } = fakeFollower();
  echoWrite("jira:PAY-231", "comment", "Kept", { many: true, write: {} as { id: number }, following: { name: "Comment" }, follower });
  expect(hears.size).toBe(0);
  expect(echoOn("jira:PAY-231", "comment")?.write).toBeNull();
});

test("Edit and resend's request is taken once, by its entity and kind", () => {
  askAgain("jira:PAY-231", "comment", "Looking at the jitter now");
  expect(takeAgain("jira:PAY-999", "comment")).toBeNull();
  expect(takeAgain("jira:PAY-231", "status")).toBeNull();
  expect(takeAgain("jira:PAY-231", "comment")).toBe("Looking at the jitter now");
  expect(takeAgain("jira:PAY-231", "comment")).toBeNull();
});

test("forgetting stops every follow", () => {
  const { follower, hears } = fakeFollower();
  echoWrite("jira:PAY-231", "status", "Done", { write: { id: 4 }, following: { name: "Move" }, follower });
  expect(hears.has(4)).toBe(true);
  forgetEchoes();
  expect(hears.has(4)).toBe(false);
  expect(echoesOn("jira:PAY-231")).toEqual([]);
});

test("a read holds a comment's words whatever it did to the spaces, and only those words", () => {
  expect(sameWords("Looking at\n the jitter now ", "Looking at the jitter now")).toBe(true);
  expect(sameWords("Looking at the jitter", "Looking at the jitter now")).toBe(false);
});

test("a write withdrawn in the queue panel takes its mark with it, quietly", () => {
  const { follower } = fakeFollower();
  echoWrite("tc:build/1", "run", "run queued", { write: { id: 5 }, following: { name: "Run" }, follower });
  echoWrite("tc:build/1", "other", "kept", { write: { id: 6 }, following: { name: "Run" }, follower });

  noteWithdrawn(5);

  expect(echoOn("tc:build/1", "run")).toBeNull();
  expect(echoOn("tc:build/1", "other")?.text).toBe("kept");
  expect(toasts.items).toHaveLength(0);
});

test("a mark with no follow still goes when its write is withdrawn", () => {
  echoWrite("tc:build/2", "run", "run queued", { write: { id: 8 } });
  noteWithdrawn(8);
  expect(echoOn("tc:build/2", "run")).toBeNull();
});

// ---- the echo's one lifecycle (#2102) ----

test("an echo reads queued while its write is open, sent once it is sent, and never queued after", () => {
  const { follower, hear } = fakeFollower();
  echoWrite("confluence:1", "comment", "Hello", { many: true, write: { id: 10 }, following: { name: "Comment" }, follower });
  expect(echoOn("confluence:1", "comment")?.state).toBe("queued");

  hear(10, row(10, "pending"));
  expect(echoOn("confluence:1", "comment")?.state).toBe("queued");

  hear(10, undefined);
  expect(echoOn("confluence:1", "comment")?.state).toBe("sent");

  // The follow is over: a stray read of an open row does not take it back to queued.
  hear(10, row(10, "pending"));
  expect(echoOn("confluence:1", "comment")?.state).toBe("sent");
});

test("an echo of a write already sent starts sent", () => {
  echoWrite("tc:run/1", "ticket", "Flaky", { write: { id: 11, state: "sent" } });
  expect(echoOn("tc:run/1", "ticket")?.state).toBe("sent");
});

test("a sent echo goes when the surface's read shows the write, and a withdrawn one goes before it is sent", () => {
  const { follower, hear } = fakeFollower();
  echoWrite("confluence:1", "page", "Runbook", { many: true, write: { id: 12 }, following: { name: "Page" }, follower });
  echoWrite("confluence:1", "page", "Notes", { many: true, write: { id: 13 }, following: { name: "Page" }, follower });
  hear(12, undefined);
  settleEchoes("confluence:1", (mark) => mark.text === "Runbook");
  noteWithdrawn(13);
  expect(echoesOn("confluence:1")).toEqual([]);
});

/** A bridge the test emits on: `emit(event, payload)` reaches every listener of `event`. */
function fakeListen() {
  const handlers = new Set<{ event: string; handler: (event: { payload: unknown }) => void }>();
  return {
    listen: (event: string, handler: (event: { payload: unknown }) => void) => {
      const entry = { event, handler };
      handlers.add(entry);
      return Promise.resolve(() => void handlers.delete(entry));
    },
    emit: (event: string, payload: unknown) => {
      for (const entry of [...handlers]) if (entry.event === event) entry.handler({ payload });
    },
    count: () => handlers.size,
  };
}

const refreshed = (entity_id: string, write_id: number | null) => ({ entity_id, write_id, ended: "refreshed" });
const runEnded = (source_id: string) => ({ source_id, running: false });

test("a surface re-reads on the refresh event naming its echo's write or its entity, and not on another's", async () => {
  const bridge = fakeListen();
  let reads = 0;
  const stop = watchEchoes("docs", () => void (reads += 1), { listen: bridge.listen });
  await Promise.resolve();
  echoWrite("confluence:1", "page", "Runbook", { many: true, write: { id: 20 }, surface: "docs", source: "confluence" });
  echoWrite("confluence:2", "comment", "Hi", { many: true, write: { id: 21 }, surface: "docs", source: "confluence" });
  echoWrite("jira:PAY-1", "comment", "Elsewhere", { many: true, write: { id: 22 }, surface: "job", source: "jira" });

  bridge.emit(EVENTS.itemRefreshed, refreshed("confluence:new", 20)); // the new page: by its write
  expect(reads).toBe(1);
  bridge.emit(EVENTS.itemRefreshed, refreshed("confluence:2", null)); // the commented page: by its entity
  expect(reads).toBe(2);
  bridge.emit(EVENTS.itemRefreshed, refreshed("confluence:9", 99)); // nothing Docs echoes
  bridge.emit(EVENTS.itemRefreshed, refreshed("jira:PAY-1", 22)); // another surface's echo
  expect(reads).toBe(2);

  stop();
  expect(bridge.count()).toBe(0);
});

test("a surface re-reads when a run of its echo's source ends, and not on another source's run or a run still going", async () => {
  const bridge = fakeListen();
  let reads = 0;
  watchEchoes("docs", () => void (reads += 1), { listen: bridge.listen });
  await Promise.resolve();
  echoWrite("confluence:1", "comment", "Hi", { many: true, write: { id: 30 }, surface: "docs", source: "confluence" });

  bridge.emit(EVENTS.syncState, { source_id: "confluence", running: true });
  bridge.emit(EVENTS.syncState, runEnded("jira"));
  expect(reads).toBe(0);
  bridge.emit(EVENTS.syncState, runEnded("confluence"));
  expect(reads).toBe(1);

  // Once the echo is gone, a run's end has nothing to settle.
  settleEchoes("confluence:1", () => true);
  bridge.emit(EVENTS.syncState, runEnded("confluence"));
  expect(reads).toBe(1);
});

test("a surface that reads on every run's end says so, and hears each", async () => {
  const bridge = fakeListen();
  let reads = 0;
  watchEchoes("docs", () => void (reads += 1), { listen: bridge.listen, everyRun: true });
  await Promise.resolve();
  bridge.emit(EVENTS.syncState, runEnded("jira"));
  expect(reads).toBe(1);
});

test("the refresh event naming an echo's write says it landed: the echo reads sent before its surface re-reads (#2103)", async () => {
  const bridge = fakeListen();
  let seen: (string | undefined)[] = [];
  watchEchoes("pr", () => void (seen = [echoOn("gitea:pr/1", "edit")?.state, echoOn("gitea:pr/1", "close")?.state]), { listen: bridge.listen });
  await Promise.resolve();
  const { follower } = fakeFollower();
  echoWrite("gitea:pr/1", "edit", "edit", { write: { id: 40 }, following: { name: "Edit" }, follower, surface: "pr", source: "gitea" });
  echoWrite("gitea:pr/1", "close", "close", { write: { id: 41 }, following: { name: "Close" }, follower, surface: "pr", source: "gitea" });
  bridge.emit(EVENTS.itemRefreshed, refreshed("gitea:pr/1", 40));
  expect(seen).toEqual(["sent", "queued"]);
});
