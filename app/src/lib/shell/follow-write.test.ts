/**
 * A write queued from a surface with nowhere to draw it (#775): the dialog
 * closed on the queue's answer, and the surface is told only if the write
 * will not go on its own -- held or refused -- with the way to the queue and,
 * where the surface can reopen, *Edit and resend*.
 *
 * Every follow shares one reader (#779): one `pending_writes` read per
 * `activity:new`, however many writes are followed, fanned out by write id.
 */
import { beforeEach, expect, test, vi } from "vitest";

import type { QueuedWrite } from "../ipc/sources";

const handlers: (() => void)[] = [];
let stopped = 0;
let open: QueuedWrite[] = [];
let reads = 0;
/** Set to hold every read open until `release` is called. */
let hold = false;
let release: () => void = () => {};

const ports = {
  listen: (_event: string, handler: () => void) => {
    handlers.push(handler);
    return Promise.resolve(() => {
      stopped += 1;
      handlers.splice(handlers.indexOf(handler), 1);
    });
  },
  pendingWrites: () => {
    reads += 1;
    const answer = open;
    if (!hold) return Promise.resolve(answer);
    return new Promise<QueuedWrite[]>((resolve) => {
      release = () => resolve(answer);
    });
  },
};

const { createFollower, followWrite, willNotGo } = await import("./follow-write");
const { toasts } = await import("./toasts.svelte");

function write(
  state: QueuedWrite["state"],
  detail: string | null = null,
  id = 7,
): QueuedWrite {
  return {
    id,
    state,
    detail,
    op: "comment",
    entity_id: "jira:PAY-1",
  } as QueuedWrite;
}

async function settle() {
  for (let i = 0; i < 8; i++) await Promise.resolve();
}

function event() {
  for (const handler of [...handlers]) handler();
}

let follower = createFollower(ports);

beforeEach(() => {
  handlers.length = 0;
  stopped = 0;
  open = [];
  reads = 0;
  hold = false;
  toasts.items = [];
  follower = createFollower(ports);
});

test("a write that went says nothing more and stops listening", async () => {
  open = [write("pending")];
  followWrite(write("pending"), { name: "Comment" }, follower);
  await settle();
  expect(handlers).toHaveLength(1);
  open = [];
  event();
  await settle();
  expect(toasts.items).toEqual([]);
  expect(stopped).toBe(1);
});

test("a write that left the open rows tells onleave once, and a held one does not (#1959)", async () => {
  open = [write("pending")];
  const onleave = vi.fn();
  followWrite(write("pending"), { name: "Monitor", onleave }, follower);
  await settle();
  expect(onleave).not.toHaveBeenCalled();
  open = [];
  event();
  await settle();
  event();
  await settle();
  expect(onleave).toHaveBeenCalledOnce();

  const held = vi.fn();
  open = [write("held", "moved", 8)];
  followWrite(write("pending", null, 8), { name: "Monitor", onleave: held }, follower);
  await settle();
  expect(held).not.toHaveBeenCalled();
});

test("a refused write says why, offers the queue and Edit and resend, and stops", async () => {
  open = [write("pending")];
  const resend = vi.fn();
  followWrite(write("pending"), { name: "Comment", resend }, follower);
  await settle();
  open = [write("refused", "403 Forbidden")];
  event();
  await settle();
  expect(toasts.items.map((t) => [t.text, t.tone])).toEqual([
    ["Comment was refused: 403 Forbidden", "err"],
  ]);
  const toast = toasts.items[0]!;
  expect(toast.action?.label).toBe("Open write queue");
  expect(toast.more?.map((a) => a.label)).toEqual(["Edit and resend"]);
  toast.more![0]!.run();
  expect(resend).toHaveBeenCalledOnce();
  expect(stopped).toBe(1);
  // Told once: a later movement is not news about this write.
  event();
  await settle();
  expect(toasts.items).toHaveLength(1);
});

test("a held write from a surface that cannot reopen offers only the queue", async () => {
  open = [write("held", "the ticket moved")];
  followWrite(write("pending"), { name: "Worklog" }, follower);
  await settle();
  // Settled before anything was heard: the first ask finds it.
  expect(toasts.items.map((t) => t.text)).toEqual([
    "Worklog was held: the ticket moved",
  ]);
  expect(toasts.items[0]!.more ?? []).toEqual([]);
});

test("a write held as unconfirmed is on its way: told once without Edit and resend, kept, and left on discard (#1966)", async () => {
  const unconfirmed = {
    ...write("held", "Jira did not answer"),
    held_snapshot: { op: "comment", unconfirmed: { source: "Jira", detail: "the connection closed" } },
  } as QueuedWrite;
  expect(willNotGo(unconfirmed)).toBe(false);
  expect(willNotGo(write("held", "moved"))).toBe(true);

  open = [write("pending")];
  const onstop = vi.fn();
  const onleave = vi.fn();
  const resend = vi.fn();
  followWrite(write("pending"), { name: "Ticket", resend, onstop, onleave }, follower);
  await settle();
  open = [unconfirmed];
  event();
  await settle();
  event();
  await settle();
  expect(toasts.items.map((t) => t.text)).toEqual([
    "Ticket may have arrived: Jira did not answer. Check Jira, then send it again or discard it.",
  ]);
  expect(toasts.items[0]!.action?.label).toBe("Open write queue");
  expect(toasts.items[0]!.more ?? []).toEqual([]);
  expect(onstop).not.toHaveBeenCalled();
  expect(stopped).toBe(0);

  open = [];
  event();
  await settle();
  expect(onleave).toHaveBeenCalledOnce();
  expect(onstop).not.toHaveBeenCalled();
  expect(stopped).toBe(1);
});

test("a write still pending keeps listening", async () => {
  open = [write("pending")];
  followWrite(write("pending"), { name: "Comment" }, follower);
  await settle();
  event();
  await settle();
  expect(toasts.items).toEqual([]);
  expect(stopped).toBe(0);
});

test("N followed writes cost one queue read per event, and one listener", async () => {
  open = [1, 2, 3, 4, 5].map((id) => write("pending", null, id));
  for (const id of [1, 2, 3, 4, 5])
    followWrite({ id }, { name: "Worklog" }, follower);
  await settle();
  expect(handlers).toHaveLength(1);
  const before = reads;
  event();
  await settle();
  expect(reads - before).toBe(1);
  // The fan-out still reaches each one by its id.
  open = [
    write("refused", "no", 3),
    ...[1, 2, 4, 5].map((id) => write("pending", null, id)),
  ];
  event();
  await settle();
  expect(reads - before).toBe(2);
  expect(toasts.items.map((t) => t.text)).toEqual(["Worklog was refused: no"]);
});

test("events that land while a read is out are coalesced into one more read", async () => {
  open = [write("pending")];
  followWrite(write("pending"), { name: "Comment" }, follower);
  await settle();
  hold = true;
  const before = reads;
  event();
  event();
  event();
  await settle();
  expect(reads - before).toBe(1);
  hold = false;
  release();
  await settle();
  expect(reads - before).toBe(2);
});

test("a follow stopped by its surface leaves, and the last one out stops the listener", async () => {
  open = [write("pending", null, 1), write("pending", null, 2)];
  const one = followWrite({ id: 1 }, { name: "Comment" }, follower);
  const two = followWrite({ id: 2 }, { name: "Comment" }, follower);
  await settle();
  one();
  expect(stopped).toBe(0);
  two();
  expect(stopped).toBe(1);
  // Refused after it left: nobody is told, because nobody is listening.
  open = [write("refused", "no", 1)];
  event();
  await settle();
  expect(toasts.items).toEqual([]);
});

test("a raw follower hears its write's row on every read, and nothing once it left", async () => {
  open = [write("pending")];
  const heard: (string | undefined)[] = [];
  const stop = follower.follow(7, (row) => heard.push(row?.state));
  await settle();
  open = [];
  event();
  await settle();
  expect(heard).toEqual(["pending", undefined]);
  stop();
  event();
  await settle();
  expect(heard).toHaveLength(2);
});

test("a write held while the listener is still registering is toasted at once (FE-12)", async () => {
  // A bridge whose `listen` answers late: the registration is in flight while
  // the queue moves. No event reaches a handler registered after the move.
  let register: () => void = () => {};
  const slow = {
    listen: (_event: string, handler: () => void) => {
      handlers.push(handler);
      return new Promise<() => void>((resolve) => {
        register = () => resolve(() => {});
      });
    },
    pendingWrites: ports.pendingWrites,
  };
  open = [write("pending")];
  followWrite(write("pending"), { name: "Comment" }, createFollower(slow));
  await settle();
  // The write is held in the window; the event for it fired before the listener existed.
  open = [write("held", "no network")];
  register();
  await settle();
  expect(toasts.items).toHaveLength(1);
  expect(toasts.items[0]?.text).toBe("Comment was held: no network");
});

test("a listener that fails to register still gets the follow its read (FE-12)", async () => {
  const broken = {
    listen: () => Promise.reject(new Error("no channel")),
    pendingWrites: ports.pendingWrites,
  };
  open = [write("held", "no network")];
  followWrite(write("pending"), { name: "Comment" }, createFollower(broken));
  await settle();
  expect(toasts.items).toHaveLength(1);
});

test("two follows during one registration both get their read (FE-12)", async () => {
  let register: () => void = () => {};
  const slow = {
    listen: () =>
      new Promise<() => void>(
        (resolve) => (register = () => resolve(() => {})),
      ),
    pendingWrites: ports.pendingWrites,
  };
  const f = createFollower(slow);
  open = [write("held", "a", 1), write("held", "b", 2)];
  followWrite(write("pending", null, 1), { name: "One" }, f);
  followWrite(write("pending", null, 2), { name: "Two" }, f);
  await settle();
  expect(toasts.items).toHaveLength(0);
  register();
  await settle();
  expect(toasts.items.map((t) => t.text).sort()).toEqual([
    "One was held: a",
    "Two was held: b",
  ]);
});
