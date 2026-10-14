/**
 * A write queued from a surface with nowhere to draw it (#775): the dialog
 * closed on the queue's answer, and the surface is told only if the write
 * will not go on its own -- held or refused -- with the way to the queue and,
 * where the surface can reopen, *Edit and resend*.
 */
import { beforeEach, expect, test, vi } from "vitest";

import type { QueuedWrite } from "../ipc/sources";

const handlers: (() => void)[] = [];
let stopped = 0;
let open: QueuedWrite[] = [];

const ports = {
  listen: (_event: string, handler: () => void) => {
    handlers.push(handler);
    return Promise.resolve(() => {
      stopped += 1;
    });
  },
  pendingWrites: () => Promise.resolve(open),
};

const { followWrite } = await import("./follow-write");
const { toasts } = await import("./toasts.svelte");

function write(state: QueuedWrite["state"], detail: string | null = null): QueuedWrite {
  return { id: 7, state, detail, op: "comment", entity_id: "jira:PAY-1" } as QueuedWrite;
}

async function settle() {
  for (let i = 0; i < 5; i++) await Promise.resolve();
}

beforeEach(() => {
  handlers.length = 0;
  stopped = 0;
  open = [];
  toasts.items = [];
});

test("a write that went says nothing more and stops listening", async () => {
  open = [write("pending")];
  followWrite(write("pending"), { name: "Comment" }, ports);
  await settle();
  expect(handlers).toHaveLength(1);
  open = [];
  handlers[0]!();
  await settle();
  expect(toasts.items).toEqual([]);
  expect(stopped).toBe(1);
});

test("a refused write says why, offers the queue and Edit and resend, and stops", async () => {
  open = [write("pending")];
  const resend = vi.fn();
  followWrite(write("pending"), { name: "Comment", resend }, ports);
  await settle();
  open = [write("refused", "403 Forbidden")];
  handlers[0]!();
  await settle();
  expect(toasts.items.map((t) => [t.text, t.tone])).toEqual([["Comment was refused: 403 Forbidden", "err"]]);
  const toast = toasts.items[0]!;
  expect(toast.action?.label).toBe("Open write queue");
  expect(toast.more?.map((a) => a.label)).toEqual(["Edit and resend"]);
  toast.more![0]!.run();
  expect(resend).toHaveBeenCalledOnce();
  expect(stopped).toBe(1);
  // Told once: a later movement is not news about this write.
  handlers[0]!();
  await settle();
  expect(toasts.items).toHaveLength(1);
});

test("a held write from a surface that cannot reopen offers only the queue", async () => {
  open = [write("held", "the ticket moved")];
  followWrite(write("pending"), { name: "Worklog" }, ports);
  await settle();
  // Settled before anything was heard: the first ask finds it.
  expect(toasts.items.map((t) => t.text)).toEqual(["Worklog was held: the ticket moved"]);
  expect(toasts.items[0]!.more ?? []).toEqual([]);
});

test("a write still pending keeps listening", async () => {
  open = [write("pending")];
  followWrite(write("pending"), { name: "Comment" }, ports);
  await settle();
  handlers[0]!();
  await settle();
  expect(toasts.items).toEqual([]);
  expect(stopped).toBe(0);
});
