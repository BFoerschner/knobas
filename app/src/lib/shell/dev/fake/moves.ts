/**
 * `?fake-ipc`: a ticket's move (ADR-0054; ticket-edit decisions, section 23). A `Transition` the fixture takes is a
 * queued row that turns sent after a moment and moves the fixture's ticket, so the board's echo settles on the next
 * read, as against a real source. `?fake-move=` drives the other outcomes, so the press trips and `just moves` can
 * draw every state of section 12 without a source:
 *
 * - `refused`: the source says no, in a sentence of its own;
 * - `held`: somebody else moved the ticket meanwhile (the queue holds it, both statuses side by side);
 * - `waiting`: the source does not answer, and the move waits *Queued*;
 * - `slow`: as sent, five seconds later (the *Queued* chip stands to be looked at).
 *
 * A `CreatePullRequest` (#2584) is the same open row, sent after a moment and read back as the repository's newest pull
 * request the fixture holds, so its page takes that pull request's address; `?fake-pr-create=slow` keeps it open a minute
 * (the page's waiting state, to be looked at) and `refused` has the source say no.
 *
 * `submit_write` stays `monitors.ts`'s; it hands a `Transition` and a `CreatePullRequest` here. `pending_writes` stays `write-queue.ts`'s and
 * reads the open moves here; `discard_write` is answered here.
 */
import type { QueuedWrite } from "../../../ipc/sources";
import type { FakeBridge, Handlers } from "../fake-tauri";
import { keepSeed, onFakeReset } from "../fake-seed";
import { EVENTS } from "../../../ipc";
import { CORPUS } from "./corpus";
import { keyOf } from "../../../time/format";

type Mode = "sent" | "refused" | "held" | "waiting" | "slow";

let mode: Mode = "sent";
let createMode: "sent" | "slow" | "refused" = "sent";
let bridge: FakeBridge | null = null;
/** The moves the fixture's queue holds open, oldest first. */
const open: QueuedWrite[] = [];

export function seed(params: URLSearchParams): void {
  const asked = params.get("fake-move");
  mode = asked === "refused" || asked === "held" || asked === "waiting" || asked === "slow" ? asked : "sent";
  const create = params.get("fake-pr-create");
  createMode = create === "slow" || create === "refused" ? create : "sent";
  open.length = 0;
}

export function attach(installed: FakeBridge): void {
  bridge = installed;
}

/** `discard_write`: a move leaves the queue; any other row is answered, never performed (`write-queue.ts`'s rule). */
export function handlers(): Handlers {
  return {
    discard_write: (args) => {
      discardMove(Number(args["id"]));
      return null;
    },
  };
}

keepSeed(open);
onFakeReset(() => {
  mode = "sent";
  createMode = "sent";
  bridge = null;
});

/** The status a fixture ticket stands in: a forge's issue its `state`, every other its `status`. */
function statusOf(entityId: string): string | null {
  const entry = CORPUS.find((one) => one.entity_id === entityId);
  if (entry === undefined) return null;
  const value = entityId.startsWith("gitea:") ? entry.payload["state"] : entry.payload["status"];
  return typeof value === "string" ? value : null;
}

function setStatus(entityId: string, status: string): void {
  const entry = CORPUS.find((one) => one.entity_id === entityId);
  if (entry === undefined) return;
  if (entityId.startsWith("gitea:")) entry.payload["state"] = status.toLowerCase();
  else entry.payload["status"] = status;
}


/** `submit_write` of a `Transition`: the queue's row, open, settled by the mode a moment later. */
export function fakeMove(row: QueuedWrite): QueuedWrite {
  const status = (row.payload as { Transition?: { status?: string } }).Transition?.status ?? "";
  const was = statusOf(row.entity_id);
  const pending: QueuedWrite = {
    ...row,
    op: "transition",
    state: "pending",
    attempted_at: null,
    attempts: 0,
    settled_at: null,
    target_snapshot: { op: "transition", live: true, text: was },
  };
  open.push(pending);
  const emit = (event: string, payload: unknown) => bridge?.emit(event, payload);
  const moved = () => emit(EVENTS.activityNew, { id: pending.id });
  setTimeout(
    () => {
      const at = open.findIndex((one) => one.id === pending.id);
      if (at < 0) return;
      const now = new Date().toISOString();
      const one = open[at]!;
      if (mode === "refused") {
        open[at] = { ...one, state: "refused", attempted_at: now, attempts: 1, detail: `${keyOf(row.entity_id)} cannot move to "${status}" from where it stands: this workflow offers ["In Review"]` };
      } else if (mode === "held") {
        setStatus(row.entity_id, "Blocked");
        open[at] = { ...one, state: "held", held_snapshot: { op: "transition", live: true, text: "Blocked" } };
      } else if (mode === "waiting") {
        open[at] = { ...one, attempted_at: now, attempts: 1, wait_reason: "unreachable", detail: "connection timed out" };
      } else {
        open.splice(at, 1);
        setStatus(row.entity_id, status);
        moved();
        // The refresh after a landed write (ADR-0031): the board reads again, and the echo settles.
        setTimeout(() => emit(EVENTS.itemRefreshed, { entity_id: row.entity_id, write_id: pending.id, ended: "refreshed" }), 150);
        return;
      }
      moved();
    },
    mode === "slow" ? 5_000 : 600,
  );
  return pending;
}

/** `submit_write` of a `CreatePullRequest` (#2584): open, then sent and read back, or refused, as `?fake-pr-create=` says. */
export function fakeCreatePr(row: QueuedWrite): QueuedWrite {
  const pending: QueuedWrite = { ...row, state: "pending", attempted_at: null, attempts: 0, settled_at: null };
  open.push(pending);
  const emit = (event: string, payload: unknown) => bridge?.emit(event, payload);
  setTimeout(
    () => {
      const at = open.findIndex((one) => one.id === pending.id);
      if (at < 0) return;
      if (createMode === "refused") {
        open[at] = { ...open[at]!, state: "refused", attempts: 1, detail: "pull request already exists for these targets" };
        emit(EVENTS.activityNew, { id: pending.id });
        return;
      }
      open.splice(at, 1);
      emit(EVENTS.activityNew, { id: pending.id });
      // The read-back of what it made (ADR-0031): the newest pull request of the repository the fixture holds.
      const [source, path = ""] = row.entity_id.split(":");
      const prefix = `${source}:${path.split("/").at(-1)}#`;
      const made = CORPUS.filter((one) => one.kind === "pr" && one.entity_id.startsWith(prefix)).map((one) => one.entity_id).sort().at(-1);
      if (made !== undefined) setTimeout(() => emit(EVENTS.itemRefreshed, { entity_id: made, write_id: pending.id, ended: "refreshed" }), 150);
    },
    createMode === "slow" ? 60_000 : 600,
  );
  return pending;
}

/** The moves the fixture's queue holds open. */
export function openMoves(): QueuedWrite[] {
  return open;
}

/** `discard_write` of a move: it leaves the queue. Answers whether it was one. */
export function discardMove(id: number): boolean {
  const at = open.findIndex((one) => one.id === id);
  if (at < 0) return false;
  open.splice(at, 1);
  bridge?.emit(EVENTS.activityNew, { id });
  return true;
}
