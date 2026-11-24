/**
 * **A pull request's state, as a write this session queued leaves it** (the pr-state fix, 2026-11-24; Björn: "it takes
 * too long to update when I create a pullrequest or merge it, doesn't immediately show as open/closed/merged"; ruled
 * by Fable, `docs/decisions/2026-11-24-a-pull-requests-state-shows-at-once.md`).
 *
 * A merge, close or reopen pressed in knobas is drawn in its new state at once, in the draw its queue answer lands in,
 * on every surface that draws that pull request: its page's header, the repository's *Pull requests* (its rows, its
 * filter and its count), Code's rows, the Branches tab, a ticket's connections and Everything's monogram. Until now
 * each drew what its read said, and the read moved only once the forge had answered the write and the item had been
 * read back (ADR-0031), seconds later, minutes behind a long run. The ticket move does the same over the board
 * (`tickets/echo-board.ts`).
 *
 * **One clock: the window's.** Each move is kept for the session by pull request: the state it was in when the write
 * was queued (`base`) and the state the write leaves. It is drawn over a read that still says `base`, until the write's
 * read-back ended: `item:refreshed` naming the write, or else the end of the first run of its source after it was
 * sent. A surface that says when it asked its rows (`askedAt`) has rows asked before that drawn over and rows asked
 * after drawn as read; one that does not is drawn over until 10 s after it. The page's live view is drawn over while the
 * page's own echo of the write stands (`TfPr.svelte` drops it once its read moves). Never the forge's `updated_at`:
 * its clock is not the window's (Fable's veto).
 *
 * A write the queue will not send (held, refused, withdrawn) takes its move with it, so the read is drawn again.
 */
import { listen as tauriListen } from "@tauri-apps/api/event";

import { EVENTS } from "../../ipc";
import type { Tone } from "../../ipc/entity";
import type { RefreshEnded, SourceSyncStatus } from "../../ipc/sources";
import { onWithdrawn } from "../../shell/write-queue.svelte";
import { echoOn } from "../queued.svelte";

/** The writes that move a pull request's state, by the echo kind its page queues them under. */
export const STATE_MOVES = ["merge", "close", "reopen"] as const;
export type StateMove = (typeof STATE_MOVES)[number];

/** A state as a read draws it: the source's word and its tone. */
export interface PrState {
  text: string;
  tone: Tone | null;
}

/** The state each move leaves, in the forge's words (Gitea and Forgejo's `pr_state`, the view's *State*). */
const LEAVES: Readonly<Record<StateMove, PrState>> = {
  merge: { text: "merged", tone: "ok" },
  close: { text: "closed", tone: "muted" },
  reopen: { text: "open", tone: "info" },
};

/** How long after its read-back a move is still drawn on a surface that does not say when it asked. */
export const QUIET_AFTER_MS = 10_000;

/** One move, as a surface reads it. */
export interface PrMove {
  kind: StateMove;
  /** What the read said as the write was queued, lowercased; `null` where the page had not read it. */
  base: string | null;
  to: PrState;
  write: number | null;
  /** The source the write goes to, whose run's end after the send is a read-back too. */
  source: string | null;
  /** When the queue said it was sent (window clock, ms), or `null`. */
  sentAt: number | null;
  /** The first run of its source that began after the send, whose end is a read-back. */
  runAfter: number | null;
  /** When its read-back ended (window clock, ms), or `null` while it has not. */
  landedAt: number | null;
  /** 10 s after the read-back: surfaces that do not say when they asked draw their read. */
  quiet: boolean;
}

const moves: Record<string, PrMove> = $state({});
const stops = new Map<string, () => void>();
let listening: Promise<void> | null = null;
let unlisten: (() => void)[] = [];

const word = (text: string) => text.trim().toLowerCase();

/** Whether `kind` is an echo kind that moves the state. */
export function isStateMove(kind: string): kind is StateMove {
  return (STATE_MOVES as readonly string[]).includes(kind);
}

/** The state a queued `kind` leaves. */
export function stateLeft(kind: StateMove): PrState {
  return LEAVES[kind];
}

/** `prId`'s move, or `null`. */
export function prMoveOf(prId: string): PrMove | null {
  return moves[prId] ?? null;
}

/** `prId`'s `kind`, queued as write `id` to `source` over a read that said `base`: drawn on every surface from now. */
export function notePrMove(prId: string, kind: StateMove, base: string | null, id: number | null, source: string | null = null): void {
  // As `echoWrite` reads it: an answer that carried no id follows nothing.
  const write = typeof id === "number" ? id : null;
  stops.get(prId)?.();
  stops.delete(prId);
  moves[prId] = { kind, base: base === null ? null : word(base), to: LEAVES[kind], write, source, sentAt: null, runAfter: null, landedAt: null, quiet: false };
  if (write !== null) stops.set(prId, onWithdrawn(write, () => dropPrMove(prId, write)));
  listenOnce();
}

/** The queue sent `prId`'s write `write`. */
export function notePrSent(prId: string, write: number | null, now = Date.now()): void {
  const move = moves[prId];
  if (move !== undefined && move.write === write && move.sentAt === null) move.sentAt = now;
}

/** The queue will not send `write`, or the forge took it and the state did not move: `prId`'s move goes, if it is that write's. */
export function dropPrMove(prId: string, write: number | null): void {
  const move = moves[prId];
  if (move === undefined || move.write !== write) return;
  delete moves[prId];
  stops.get(prId)?.();
  stops.delete(prId);
}

function landed(prId: string, move: PrMove, now: number): void {
  if (move.landedAt !== null) return;
  move.landedAt = now;
  setTimeout(() => {
    if (moves[prId] === move) move.quiet = true;
  }, QUIET_AFTER_MS);
}

/** A refresh ended (`item:refreshed`): the one that read back a move's write ends its wait. */
export function heardRefresh(ended: Pick<RefreshEnded, "entity_id" | "write_id" | "ended">, now = Date.now()): void {
  if (ended.ended === "failed" || ended.write_id === null) return;
  for (const [prId, move] of Object.entries(moves)) if (move.write === ended.write_id) landed(prId, move, now);
}

/**
 * A run's state moved (`sync:state`): the end of the first run of a move's source that began after its send is its
 * read-back too, for a source whose adapter reads no one item. A run already going at the send does not count: the
 * write's own read-back waits behind it (ADR-0031).
 */
export function heardRun(row: Pick<SourceSyncStatus, "source_id" | "running" | "run_id" | "started_at">, now = Date.now()): void {
  for (const [prId, move] of Object.entries(moves)) {
    if (move.source !== row.source_id || move.sentAt === null || row.run_id === null) continue;
    if (row.running) {
      const began = row.started_at === null ? now : Date.parse(row.started_at);
      if (move.runAfter === null && began >= move.sentAt) move.runAfter = row.run_id;
    } else if (row.run_id === move.runAfter) landed(prId, move, now);
  }
}

function listenOnce(): void {
  if (listening !== null) return;
  const hear = (event: string, handler: (payload: unknown) => void) =>
    tauriListen(event, ({ payload }) => handler(payload)).then(
      (off) => void unlisten.push(off),
      // No bridge (a test): the test speaks for the events.
      () => {},
    );
  listening = Promise.all([
    hear(EVENTS.itemRefreshed, (payload) => heardRefresh(payload as RefreshEnded)),
    hear(EVENTS.syncState, (payload) => heardRun(payload as SourceSyncStatus)),
  ]).then(() => {});
}

/**
 * What to draw of `prId`'s state, given what a read said (`read`). `askedAt` is when the surface asked that read
 * (`Date.now()`), where it says; `live` marks the page's live view, which its own echo settles.
 */
export function prStateOver<S extends PrState>(prId: string, read: S | null, how: { askedAt?: number; live?: boolean } = {}): S | PrState | null {
  const move = moves[prId];
  if (move === undefined) return read;
  if (read !== null && move.base !== null && word(read.text) !== move.base) return read;
  if (how.live) return echoOn(prId, move.kind) === null ? read : move.to;
  if (how.askedAt !== undefined) return move.landedAt === null || how.askedAt < move.landedAt ? move.to : read;
  return move.quiet ? read : move.to;
}

/** `rows` with each moved pull request's state laid over, asked at `askedAt`: the same array where none moved. */
export function withPrStates<R extends { entity_id: string; state: PrState | null }>(rows: R[], askedAt?: number): R[] {
  let changed = false;
  const drawn = rows.map((row) => {
    if (moves[row.entity_id] === undefined) return row;
    const state = prStateOver(row.entity_id, row.state, askedAt === undefined ? {} : { askedAt });
    if (state === row.state) return row;
    changed = true;
    return { ...row, state };
  });
  return changed ? drawn : rows;
}

/** Forget every move: a new session, which is what each test is. */
export function forgetPrMoves(): void {
  for (const stop of stops.values()) stop();
  stops.clear();
  for (const key of Object.keys(moves)) delete moves[key];
  for (const off of unlisten.splice(0)) off();
  listening = null;
}
