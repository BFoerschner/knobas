/**
 * A ticket's **move** (ticket-edit decisions, sections 3, 11 and 12; ADR-0054): the one function every surface calls --
 * a drop on the board, a pick in a status menu, *Move to* in a card's menu, the ticket page's status -- and what the
 * board and the ticket page draw of it.
 *
 * A move is `Transition { entity, status }`, the write knobas already has: the status picked, which the adapter
 * resolves at write time and refuses by name. It is drawn at once as an echo (`queued.svelte.ts`), *queued* then
 * *sent*, gone once a read shows the status; the write queue does the rest.
 *
 * - **Notes** (`ticketNotes`): a ticket has one while the queue holds an open refused move of it, or one held because
 *   the ticket changed (not one held as an unconfirmed send). Read from the queue, so a note survives a restart and
 *   goes when its row is discarded, applied or edited. `dismissNote` discards the row.
 * - **Seeding** (`seedTicketEchoes`): a move still open in the queue is drawn again after a restart.
 * - **Undo** (`undoMove`, `undoLastMove`): while its toast stands. A write not yet sent is discarded, and nothing
 *   reached the source. One already sent can only be undone by another move: the source is asked where the ticket can
 *   go now, and the way back is queued where it is offered, or the toast says it is not.
 *
 * Which of Björn's open answers (section 34) this module carries, each in one place: `⌘Z` is the page's key handler's
 * call of {@link undoLastMove}; a refused card going back by itself is {@link MoveHow.onstop}, which the board answers
 * with its held order's `restore`.
 */
import { listen as tauriListen } from "@tauri-apps/api/event";

import { EVENTS, ipcErrorMessage } from "../../ipc";
import { submitWrite as realSubmitWrite, type MiniBoardCard } from "../../ipc/entity";
import { discardWrite as realDiscardWrite, pendingWrites as realPendingWrites, type QueuedWrite } from "../../ipc/sources";
import { mayHaveLanded, type WriteFollower } from "../../shell/follow-write";
import { moveWords, toastQueued } from "../../shell/queued-toast";
import { dismiss, push, toasts } from "../../shell/toasts.svelte";
import { noteWithdrawn, onWithdrawn } from "../../shell/write-queue.svelte";
import { echoOn, echoWrite, echoesOn, settleEchoes } from "../queued.svelte";
import { sourceRoles } from "../roles.svelte";
import { keyOf } from "../job/job";
import { sameStatus } from "./drop";
import { askReach, forgetReach, type ReachKey, type ReachPort } from "./reach.svelte";

export type MovedTicket = Pick<MiniBoardCard, "entity_id" | "source_id" | "key" | "status"> & Partial<Pick<MiniBoardCard, "issue_type" | "updated_at">>;

/** The reads and writes a move makes, injectable so a test needs no bridge. */
export interface MovePorts {
  submitWrite: (payload: { Transition: { entity: string; status: string } }) => Promise<QueuedWrite>;
  discardWrite: (id: number) => Promise<void>;
  /** The bridge's `reachable_transitions` when left out. */
  reachableTransitions?: ReachPort;
  follower?: WriteFollower;
}

const REAL: MovePorts = {
  submitWrite: (payload) => realSubmitWrite(payload),
  discardWrite: (id) => realDiscardWrite(id),
};

/** How a move is made. */
export interface MoveHow {
  /** The way back of an undo: *Moving … back to …*, and no second *Undo*. */
  undo?: boolean;
  ports?: Partial<MovePorts>;
  /** The surface drawing it, for `watchEchoes`: `tickets` unless said (the ticket page's is `job`). */
  surface?: string;
  /** The queue will not send it (held or refused): its echo is dropped; the surface puts the card back. */
  onstop?: () => void;
  /** Withdrawn, in the queue panel or by *Undo*: its echo is dropped; the surface puts the card back. */
  onwithdrawn?: () => void;
}

/** The echo kind a move draws in. */
export const STATUS = "status";

/** A move made this session, for *Undo* while its toast stands. */
interface Made {
  ticket: MovedTicket;
  from: string | null;
  to: string;
  write: number;
  toast: number;
  ports: MovePorts;
  how: MoveHow;
}
const made: Made[] = [];

/**
 * Move `ticket` to `status`: drawn at once as an echo, written behind. Answers once the write is queued, or `null`
 * with a toast saying why it was not.
 */
export async function moveTicket(ticket: MovedTicket, status: string, how: MoveHow = {}): Promise<QueuedWrite | null> {
  const ports = { ...REAL, ...how.ports };
  const entity = ticket.entity_id;
  const surface = how.surface ?? "tickets";
  const source = ticket.source_id;
  // In the caller's frame: the card stands in its new column before the queue answers.
  echoWrite(entity, STATUS, status, { base: ticket.status, surface, source });
  let write: QueuedWrite;
  try {
    write = await ports.submitWrite({ Transition: { entity, status } });
  } catch (cause) {
    settleEchoes(entity, (echo) => echo.kind === STATUS && echo.write === null);
    push({ text: `Could not move ${ticket.key}: ${ipcErrorMessage(cause)}`, tone: "err" });
    return null;
  }
  // A newer move of the same ticket replaced this one's echo meanwhile: it is that one's to draw.
  const still = echoOn(entity, STATUS);
  if (still !== null && still.write !== write.id && (still.write !== null || still.text !== status)) return write;
  echoWrite(entity, STATUS, status, {
    write,
    base: ticket.status,
    surface,
    source,
    following: { name: "Move" },
    ...(ports.follower ? { follower: ports.follower } : {}),
    onsent: () => forgetReach(entity),
    onstop: () => how.onstop?.(),
  });
  onWithdrawn(write.id, () => how.onwithdrawn?.());
  const name = sourceRoles.nameOf(source);
  const record: Made = { ticket, from: ticket.status, to: status, write: write.id, toast: -1, ports, how };
  record.toast = toastQueued(`${ticket.key} to ${status}`, { id: source, display_name: name }, {
    write,
    // The way back is a toast of its own, so it takes no *Undo* from the move it undoes.
    subject: how.undo ? `move-back:${entity}` : `move:${entity}`,
    words: moveWords(ticket.key, status, how.undo === true),
    ...(how.undo ? {} : { undo: () => void undoMove(record) }),
    ...(ports.follower ? { follower: ports.follower } : {}),
  });
  if (!how.undo) made.push(record);
  if (made.length > 20) made.splice(0, made.length - 20);
  return write;
}

/** *Undo* one move (section 12). */
async function undoMove(record: Made): Promise<void> {
  const index = made.indexOf(record);
  if (index >= 0) made.splice(index, 1);
  const { ticket, from, to, ports } = record;
  const echo = echoesOn(ticket.entity_id, STATUS).find((one) => one.write === record.write);
  if (echo?.state === "queued") {
    try {
      await ports.discardWrite(record.write);
      // As the queue panel's *Discard* says it: the echo goes and the card stands where it stood.
      noteWithdrawn(record.write);
      return;
    } catch {
      // Gone from the queue meanwhile: it was sent, and the way back is another move.
    }
  }
  if (from === null) return;
  // Its *Undo* is spent: what follows says what happened in a toast of its own.
  dismiss(record.toast);
  const source = sourceRoles.nameOf(ticket.source_id);
  const now: ReachKey = {
    entity_id: ticket.entity_id,
    source_id: ticket.source_id,
    key: ticket.key,
    status: to,
    issue_type: ticket.issue_type ?? null,
    updated_at: `undo:${record.write}`,
  };
  const reach = await askReach(now, "pickup", ports.reachableTransitions);
  if (reach === null || reach.standing !== "exact") {
    push({ text: `${source} did not answer. ${ticket.key} stays in ${to}.`, tone: "err" });
    return;
  }
  const back = reach.statuses.find((status) => sameStatus(status, from));
  if (back === undefined) {
    push({ text: `${source} offers no move from ${to} back to ${from}. ${ticket.key} stays in ${to}.` });
    return;
  }
  await moveTicket({ ...ticket, status: to }, back, { ...record.how, undo: true });
}

/** `⌘Z` / `Ctrl+Z` on Tickets and a ticket page: the newest move whose toast still stands, undone. Answers whether there was one. */
export function undoLastMove(): boolean {
  for (let at = made.length - 1; at >= 0; at--) {
    const record = made[at]!;
    if (toasts.items.some((toast) => toast.id === record.toast)) {
      void undoMove(record);
      return true;
    }
  }
  return false;
}

// ---- notes, from the queue (section 12) ----

/** What a card or the ticket page says of a move the source refused or knobas holds. */
export interface TicketNote {
  /** The queue row, which `dismissNote` discards. */
  write: number;
  tone: "err" | "warn";
  /** knobas's words: *Not moved to Done.* */
  lead: string;
  /** The source's sentence for a refusal, as it gave it; knobas's for a hold. */
  words: string;
}

/** Each ticket's note, by entity id. */
export const ticketNotes = $state<Record<string, TicketNote>>({});

/** Why an open move waits, by its write's id, in the queue's words: what its card's *Queued* says as its title. */
export const moveWaits = $state<Record<number, string>>({});

/** Why `row` waits, or `null` while it is simply on its way. */
export function waitOf(row: QueuedWrite): string | null {
  if (row.state !== "pending" || row.attempted_at === null) return null;
  const source = sourceRoles.nameOf(row.source_id);
  if (row.wait_reason === "unauthorized") return `Waiting: ${source} refused the credential`;
  if (row.wait_reason === "unreachable") return row.detail?.startsWith("keychain:") ? "Waiting: the keychain would not open" : `Waiting: ${source} is unreachable`;
  return null;
}

/** The status a queued row's `Transition` sets, or `null` for any other row. */
function movedTo(row: QueuedWrite): string | null {
  const payload = row.payload as { Transition?: { status?: unknown } } | null;
  const status = payload?.Transition?.status;
  return row.op === "transition" && typeof status === "string" ? status : null;
}

/** The notes `rows` (the queue's open rows) give, by entity: the newest refused or drift-held move of each. */
export function notesOf(rows: readonly QueuedWrite[]): Record<string, TicketNote> {
  const out: Record<string, TicketNote> = {};
  for (const row of [...rows].sort((a, b) => a.id - b.id)) {
    const status = movedTo(row);
    if (status === null) continue;
    const lead = `Not moved to ${status}.`;
    if (row.state === "refused") out[row.entity_id] = { write: row.id, tone: "err", lead, words: row.detail ?? "" };
    else if (row.state === "held" && !mayHaveLanded(row)) {
      out[row.entity_id] = { write: row.id, tone: "warn", lead, words: `${keyOf(row.entity_id)} changed in ${sourceRoles.nameOf(row.source_id)} since you moved it.` };
    }
  }
  return out;
}

/** Draw `notes` in place of the ones drawn: an entity's note changes only when its row does. */
function takeNotes(notes: Record<string, TicketNote>): void {
  for (const entity of Object.keys(ticketNotes)) if (!(entity in notes)) delete ticketNotes[entity];
  for (const [entity, note] of Object.entries(notes)) {
    const had = ticketNotes[entity];
    if (had?.write !== note.write || had.tone !== note.tone || had.words !== note.words) ticketNotes[entity] = note;
  }
}

export interface NotePorts {
  pendingWrites: () => Promise<QueuedWrite[]>;
  listen: (event: string, handler: () => void) => Promise<() => void>;
}

const NOTE_PORTS: NotePorts = {
  pendingWrites: () => realPendingWrites(),
  listen: (event, handler) => tauriListen(event, handler),
};

let watchers = 0;
let unlisten: (() => void) | null = null;
let reading = false;
let again = false;
let notePorts: NotePorts = NOTE_PORTS;
/** A watch began: its first read also draws the moves the queue still holds (once a mount, not on every movement). */
let seeding = false;

async function readNotes(): Promise<void> {
  if (reading) {
    again = true;
    return;
  }
  reading = true;
  try {
    do {
      again = false;
      try {
        const rows = await notePorts.pendingWrites();
        takeNotes(notesOf(rows));
        const waits: Record<number, string> = {};
        for (const row of rows) {
          const wait = movedTo(row) === null ? null : waitOf(row);
          if (wait !== null) waits[row.id] = wait;
        }
        for (const id of Object.keys(moveWaits)) if (!(Number(id) in waits)) delete moveWaits[Number(id)];
        for (const [id, wait] of Object.entries(waits)) if (moveWaits[Number(id)] !== wait) moveWaits[Number(id)] = wait;
        if (seeding) seedTicketEchoes(rows);
        seeding = false;
      } catch {
        // Not known; the next queue movement asks again.
      }
    } while (again && watchers > 0);
  } finally {
    reading = false;
  }
}

/**
 * Keep {@link ticketNotes} as the queue says, and draw a move the queue still holds after a restart: read once now and
 * again on every queue movement (`activity:new`), one listener however many pages watch. Answers the stop.
 */
export function watchTicketNotes(ports: Partial<NotePorts> = {}): () => void {
  notePorts = { ...NOTE_PORTS, ...ports };
  watchers += 1;
  if (watchers === 1) {
    void notePorts
      .listen(EVENTS.activityNew, () => void readNotes())
      .then((off) => {
        if (watchers > 0 && unlisten === null) unlisten = off;
        else off();
      })
      .catch(() => {});
  }
  seeding = true;
  void readNotes();
  let stopped = false;
  return () => {
    if (stopped) return;
    stopped = true;
    watchers -= 1;
    if (watchers > 0) return;
    unlisten?.();
    unlisten = null;
  };
}

/** `×` on a note: its write discarded, which is what takes the note away. */
export async function dismissNote(entityId: string, discard: (id: number) => Promise<void> = realDiscardWrite): Promise<void> {
  const note = ticketNotes[entityId];
  if (note === undefined) return;
  try {
    await discard(note.write);
    delete ticketNotes[entityId];
    noteWithdrawn(note.write);
  } catch (cause) {
    push({ text: `Could not discard it: ${ipcErrorMessage(cause)}`, tone: "err" });
  }
}

/**
 * Draw again each move the queue still holds open (pending, or held as a send that may have landed) whose write no
 * echo holds: a card moved while the source was away stands in its new column, *Queued*, after knobas starts again.
 */
export function seedTicketEchoes(rows: readonly QueuedWrite[], surface = "tickets"): void {
  for (const row of rows) {
    const status = movedTo(row);
    if (status === null) continue;
    if (row.state !== "pending" && !(row.state === "held" && mayHaveLanded(row))) continue;
    if (echoesOn(row.entity_id).some((echo) => echo.write === row.id)) continue;
    echoWrite(row.entity_id, STATUS, status, { write: row, base: null, surface, source: row.source_id, following: { name: "Move" } });
  }
}

/** The status a ticket's move echo draws, or `null`. */
export function movedStatus(entityId: string): string | null {
  return echoOn(entityId, STATUS)?.text ?? null;
}

// ---- the right-click menu's *Move to* (section 10) ----

import type { MoveOffer } from "../context/items";
export type { MoveOffer };

/** What a page that knows its tickets offers the right-click menu: absent, a ticket's menu offers no *Move to*. */
export interface MoveProvider {
  offer(entityId: string): MoveOffer | null;
  /** The menu opened on it: ask where it can go. */
  ask(entityId: string): void;
  move(entityId: string, status: string): void;
}

const providers: MoveProvider[] = [];
/** Moves when a provider comes or goes, so a reader of {@link moveProvider} is reactive. */
let offered = $state(0);

/** A page offers *Move to* while it is shown; answers the stop. The newest one answers. */
export function offerMoves(provider: MoveProvider): () => void {
  providers.push(provider);
  offered += 1;
  return () => {
    const at = providers.indexOf(provider);
    if (at >= 0) providers.splice(at, 1);
    offered += 1;
  };
}

/** The page offering *Move to* now, or `null`. */
export function moveProvider(): MoveProvider | null {
  void offered;
  return providers.at(-1) ?? null;
}

/** Forget every move, note and provider: a new session, which is what each test is. */
export function forgetMoves(): void {
  made.length = 0;
  for (const entity of Object.keys(ticketNotes)) delete ticketNotes[entity];
  for (const id of Object.keys(moveWaits)) delete moveWaits[Number(id)];
  providers.length = 0;
  watchers = 0;
  unlisten = null;
  reading = false;
  again = false;
  notePorts = NOTE_PORTS;
}
