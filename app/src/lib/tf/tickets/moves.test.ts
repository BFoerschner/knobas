/** A ticket's move: the echo, the queue's notes, seeding after a restart, and Undo (ticket-edit decisions, sections 3, 11, 12). */
import { beforeEach, expect, test, vi } from "vitest";

import type { QueuedWrite } from "../../ipc/sources";
import type { WriteFollower } from "../../shell/follow-write";
import { toasts } from "../../shell/toasts.svelte";
import { echoOn, echoesOn } from "../queued.svelte";
import { sourceRoles } from "../roles.svelte";
import { dismissNote, moveTicket, notesOf, seedTicketEchoes, ticketNotes, undoLastMove, watchTicketNotes, type MovePorts } from "./moves.svelte";
import { forgetReach } from "./reach.svelte";

const ticket = { entity_id: "jira:PAY-231", source_id: "jira", key: "PAY-231", status: "In Progress", issue_type: "Story", updated_at: "2026-11-20T10:00:00Z" };
const quiet: WriteFollower = { follow: () => () => {} };

function row(over: Partial<QueuedWrite> = {}): QueuedWrite {
  return {
    id: 7,
    source_id: "jira",
    entity_id: "jira:PAY-231",
    op: "transition",
    payload: { Transition: { entity: "jira:PAY-231", status: "Done" } },
    target_snapshot: { op: "transition", live: true, text: "In Progress" },
    state: "pending",
    wait_reason: null,
    detail: null,
    queued_at: "2026-11-21T10:00:00Z",
    attempted_at: null,
    attempts: 0,
    held_snapshot: null,
    settled_at: null,
    source_enabled: true,
    ...over,
  } as QueuedWrite;
}

function ports(over: Partial<MovePorts> = {}) {
  const sent: unknown[] = [];
  const discarded: number[] = [];
  let next = 7;
  const made: MovePorts = {
    submitWrite: vi.fn((payload) => {
      sent.push(payload);
      return Promise.resolve(row({ id: next++, payload }));
    }),
    discardWrite: vi.fn((id: number) => {
      discarded.push(id);
      return Promise.resolve();
    }),
    reachableTransitions: () => Promise.resolve(["In Progress", "In Review"]),
    follower: quiet,
    ...over,
  };
  return { sent, discarded, ports: made };
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

beforeEach(() => {
  forgetReach();
  sourceRoles.take([{ id: "jira", display_name: "Tidewater Jira", roles: [], role_misses: [], write_ops: ["transition"] }]);
});

test("the echo stands before the queue answers, in the caller's frame", () => {
  void moveTicket(ticket, "Done", { ports: { ...ports().ports, submitWrite: () => new Promise(() => {}) } });
  expect(echoOn("jira:PAY-231", "status")?.text).toBe("Done");
  expect(echoOn("jira:PAY-231", "status")?.write).toBeNull();
});

test("the queue's answer replaces the echo with one carrying the write, and the toast offers Undo", async () => {
  const { sent, ports: p } = ports();
  await moveTicket(ticket, "Done", { ports: p });
  expect(sent).toEqual([{ Transition: { entity: "jira:PAY-231", status: "Done" } }]);
  expect(echoesOn("jira:PAY-231", "status")).toHaveLength(1);
  expect(echoOn("jira:PAY-231", "status")?.write).toBe(7);
  const toast = toasts.items.at(-1)!;
  expect(toast.text).toBe("Moving PAY-231 to Done.");
  expect(toast.action?.label).toBe("Undo");
  expect(toast.more?.map((one) => one.label)).toEqual(["Show queue"]);
});

test("a rejected submit leaves no echo and one error toast", async () => {
  const { ports: p } = ports({ submitWrite: () => Promise.reject({ code: "invalid", message: "not a ticket" }) });
  expect(await moveTicket(ticket, "Done", { ports: p })).toBeNull();
  expect(echoesOn("jira:PAY-231", "status")).toHaveLength(0);
  expect(toasts.items.filter((one) => one.tone === "err").map((one) => one.text)).toEqual(["Could not move PAY-231: not a ticket"]);
});

test("a refused row is a note in the source's words; a drift hold a note of knobas's; an unconfirmed hold none", () => {
  const refused = row({ state: "refused", detail: 'PAY-231 cannot move to "Done" from where it stands: this workflow offers ["In Review"]' });
  expect(notesOf([refused])["jira:PAY-231"]).toEqual({ write: 7, tone: "err", lead: "Not moved to Done.", words: refused.detail });
  const drift = row({ state: "held", held_snapshot: { op: "transition", live: true, text: "Blocked" } });
  expect(notesOf([drift])["jira:PAY-231"]).toEqual({ write: 7, tone: "warn", lead: "Not moved to Done.", words: "PAY-231 changed in Tidewater Jira since you moved it." });
  const unconfirmed = row({ state: "held", held_snapshot: { unconfirmed: { source: "Tidewater Jira", detail: "timed out" } } });
  expect(notesOf([unconfirmed])).toEqual({});
  expect(notesOf([row({ op: "comment", payload: { Comment: { entity: "jira:PAY-231", body: "x" } }, state: "refused" })])).toEqual({});
});

test("the notes are read from the queue, and survive a restart; × discards the write", async () => {
  const refused = row({ state: "refused", detail: "No." });
  const stop = watchTicketNotes({ pendingWrites: () => Promise.resolve([refused]), listen: () => Promise.resolve(() => {}) });
  await flush();
  expect(ticketNotes["jira:PAY-231"]?.words).toBe("No.");
  const discard = vi.fn(() => Promise.resolve());
  await dismissNote("jira:PAY-231", discard);
  expect(discard).toHaveBeenCalledWith(7);
  expect(ticketNotes["jira:PAY-231"]).toBeUndefined();
  stop();
});

test("a move the queue still holds open is drawn again from the queue", () => {
  seedTicketEchoes([row({ id: 11 }), row({ id: 12, entity_id: "jira:PAY-1", state: "refused" })]);
  expect(echoOn("jira:PAY-231", "status")).toMatchObject({ text: "Done", write: 11, state: "queued" });
  expect(echoOn("jira:PAY-1", "status")).toBeNull();
  // Drawn once: a second read draws nothing again.
  seedTicketEchoes([row({ id: 11 })]);
  expect(echoesOn("jira:PAY-231", "status")).toHaveLength(1);
});

test("Undo before the send discards the write, puts the card back and queues nothing", async () => {
  const { sent, discarded, ports: p } = ports();
  const back = vi.fn();
  await moveTicket(ticket, "Done", { ports: p, onwithdrawn: back });
  expect(undoLastMove()).toBe(true);
  await flush();
  expect(discarded).toEqual([7]);
  expect(sent).toHaveLength(1);
  expect(echoesOn("jira:PAY-231", "status")).toHaveLength(0);
  expect(back).toHaveBeenCalledOnce();
});

test("Undo after the send asks the source and queues the way back where it is offered", async () => {
  const { sent, ports: p } = ports();
  await moveTicket(ticket, "Done", { ports: p });
  echoesOn("jira:PAY-231", "status")[0]!.state = "sent";
  undoLastMove();
  await flush();
  await flush();
  expect(sent).toEqual([{ Transition: { entity: "jira:PAY-231", status: "Done" } }, { Transition: { entity: "jira:PAY-231", status: "In Progress" } }]);
  expect(toasts.items.at(-1)?.text).toBe("Moving PAY-231 back to In Progress.");
  expect(toasts.items.at(-1)?.action?.label).toBe("Show queue");
});

test("Undo after the send queues nothing where the source offers no way back, and says so", async () => {
  const { sent, ports: p } = ports({ reachableTransitions: () => Promise.resolve(["Closed"]) });
  await moveTicket(ticket, "Done", { ports: p });
  echoesOn("jira:PAY-231", "status")[0]!.state = "sent";
  undoLastMove();
  await flush();
  await flush();
  expect(sent).toHaveLength(1);
  expect(toasts.items.at(-1)?.text).toBe("Tidewater Jira offers no move from Done back to In Progress. PAY-231 stays in Done.");
});

test("Undo by key does nothing once the toast has gone", async () => {
  const { ports: p } = ports();
  await moveTicket(ticket, "Done", { ports: p });
  toasts.items.length = 0;
  expect(undoLastMove()).toBe(false);
});
