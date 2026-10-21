/**
 * The ticket-first pages' queued marks (#1424, #1542): drawn at once, kept for
 * the session, dropped once the source's read shows the write landed, and
 * dropped with a toast when the queue holds or refuses the write.
 */
import { afterEach, beforeEach, expect, test } from "vitest";

import type { QueuedWrite } from "../ipc/sources";
import type { Hear, WriteFollower } from "../shell/follow-write";
import { toasts } from "../shell/toasts.svelte";
import { askAgain, forgetQueued, markQueued, queuedMark, queuedMarks, sameWords, settleQueued, takeAgain } from "./queued.svelte";

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
afterEach(() => forgetQueued());

test("a mark stays until the read shows it landed, and a later read does not bring it back", () => {
  markQueued("jira:PAY-231", "status", "In Review", { base: "In Progress" });
  const moved = (status: string) => (mark: { kind: string; base: string | null }) => mark.kind === "status" && mark.base !== status;

  settleQueued("jira:PAY-231", moved("In Progress"));
  expect(queuedMark("jira:PAY-231", "status")?.text).toBe("In Review");

  settleQueued("jira:PAY-231", moved("In Review"));
  expect(queuedMark("jira:PAY-231", "status")).toBeNull();

  // The ticket moved back by somebody else: the mark is gone for good.
  settleQueued("jira:PAY-231", moved("In Progress"));
  expect(queuedMark("jira:PAY-231", "status")).toBeNull();
});

test("a kind marked again replaces the earlier mark; `many` keeps each", () => {
  markQueued("gitea:pr/7", "close", "close queued");
  markQueued("gitea:pr/7", "close", "close queued");
  markQueued("jira:PAY-231", "status", "In Review", { base: "To Do" });
  markQueued("jira:PAY-231", "status", "Done", { base: "To Do" });
  markQueued("jira:PAY-231", "comment", "One", { many: true });
  markQueued("jira:PAY-231", "comment", "Two", { many: true });

  expect(queuedMarks("gitea:pr/7").map((m) => m.text)).toEqual(["close queued"]);
  expect(queuedMarks("jira:PAY-231", "status").map((m) => m.text)).toEqual(["Done"]);
  expect(queuedMarks("jira:PAY-231", "comment").map((m) => m.text)).toEqual(["One", "Two"]);
  expect(queuedMarks("jira:PAY-999")).toEqual([]);
});

test("a held or refused write drops its own mark and toasts with Edit and resend", () => {
  const { follower, hear, hears } = fakeFollower();
  let resent = 0;
  const following = { name: "Comment", resend: () => void (resent += 1) };
  markQueued("jira:PAY-231", "comment", "First", { many: true, write: { id: 1 }, following, follower });
  markQueued("jira:PAY-231", "comment", "Second", { many: true, write: { id: 2 }, following, follower });

  hear(2, row(2, "pending"));
  expect(queuedMarks("jira:PAY-231", "comment").map((m) => m.text)).toEqual(["First", "Second"]);
  expect(toasts.items).toEqual([]);

  hear(2, row(2, "refused", "403 Forbidden"));
  expect(queuedMarks("jira:PAY-231", "comment").map((m) => m.text)).toEqual(["First"]);
  expect(toasts.items.map((t) => [t.text, t.tone])).toEqual([["Comment was refused: 403 Forbidden", "err"]]);
  const resend = toasts.items[0]!.more?.find((a) => a.label === "Edit and resend");
  resend!.run();
  expect(resent).toBe(1);
  expect(hears.has(2)).toBe(false);
});

test("a sent write keeps its mark until the read shows it, and is no longer followed", () => {
  const { follower, hear, hears } = fakeFollower();
  markQueued("jira:PAY-231", "comment", "Sent", { many: true, write: { id: 3 }, following: { name: "Comment" }, follower });
  hear(3, undefined);
  expect(hears.has(3)).toBe(false);
  expect(queuedMarks("jira:PAY-231", "comment").map((m) => m.text)).toEqual(["Sent"]);
  expect(toasts.items).toEqual([]);
});

test("a write the answer carried no id for is drawn and not followed", () => {
  const { follower, hears } = fakeFollower();
  markQueued("jira:PAY-231", "comment", "Kept", { many: true, write: {} as { id: number }, following: { name: "Comment" }, follower });
  expect(hears.size).toBe(0);
  expect(queuedMark("jira:PAY-231", "comment")?.write).toBeNull();
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
  markQueued("jira:PAY-231", "status", "Done", { write: { id: 4 }, following: { name: "Move" }, follower });
  expect(hears.has(4)).toBe(true);
  forgetQueued();
  expect(hears.has(4)).toBe(false);
  expect(queuedMarks("jira:PAY-231")).toEqual([]);
});

test("a read holds a comment's words whatever it did to the spaces, and only those words", () => {
  expect(sameWords("Looking at\n the jitter now ", "Looking at the jitter now")).toBe(true);
  expect(sameWords("Looking at the jitter", "Looking at the jitter now")).toBe(false);
});
