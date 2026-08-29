/**
 * What knobas still owes each source, for the status bar and the queue panel
 * (issue #42).
 *
 * Two things live here rather than in the component: the **loading** of the
 * queue, because the badge and the panel must never disagree about it, and the
 * **reading** of a snapshot, because "what changed" is per-op and a component
 * should not be the place that knows it.
 *
 * ## Why there is no `write:*` event to listen to
 *
 * Every queue transition already writes an activity line, so `activity:new`
 * is the signal that something moved. A second channel saying the same thing
 * would be a second thing to keep in step. The component subscribes and calls
 * {@link WriteQueue.refresh}; this module does not listen on its own, for the
 * reason `latest-change` does not either — a subscription belongs to the
 * lifetime of a component, not of a module.
 *
 * ## The rule this module must not break
 *
 * A held write is terminal until the user acts. **Nothing here may release
 * one on a timer, on a refresh, or as part of a flush** — `flushWrites` cannot
 * do it (the backend will not offer a held write to its flush loop), and
 * nothing in this file may do it either. `write-queue.test.svelte.ts` runs the
 * clock forward over a held write and asserts it is still held.
 */
import {
  applyHeldWrite,
  discardWrite,
  flushWrites,
  pendingWrites,
  writeQueueCounts,
  type QueuedWrite,
  type QueueCounts,
  type WriteOpPayload,
  type WriteState,
} from "../ipc/sources";
import { amendWrite } from "../ipc/sources";

/** Nothing owed, and nothing asked yet — the counts before the first read. */
const NONE: QueueCounts = { pending: 0, held: 0, refused: 0 };

/**
 * How many writes need a **decision** rather than patience.
 *
 * Held and refused both do, and for the same reason from the user's side:
 * neither will ever move on its own. Kept as one number because the badge has
 * one slot for "you owe somebody an answer", and split back apart in the panel
 * where there is room to say which.
 */
export function decisionsIn(counts: QueueCounts): number {
  return counts.held + counts.refused;
}

/**
 * What one row needs from the reader, in one word.
 *
 * The distinction story 18 turns on: `waiting` is patience, `decide` is a
 * decision. A UI that showed one badge over both would be telling the user
 * that a conflict is something that resolves itself.
 */
export type Demand = "waiting" | "decide" | "settled";

export function demandOf(state: WriteState): Demand {
  switch (state) {
    case "pending":
      return "waiting";
    case "held":
    case "refused":
      return "decide";
    // `sent` and `discarded` never reach the panel, but a total switch is what
    // makes a *new* state a type error here rather than a row that silently
    // renders as patience.
    case "sent":
    case "discarded":
      return "settled";
  }
}

/**
 * One side of the two-versions comparison, in a shape a template can render.
 *
 * A snapshot is `knobas_core::write_queue::project`'s output, and its shape is
 * **per op**: `{op, live, text}` for a comment, and the whole mirrored record
 * for an op with no stated projection. So this narrows rather than assumes,
 * and says plainly when it cannot read one — a dialog that rendered
 * `[object Object]` beside a comment would be worse than one that says it does
 * not know how to show this op.
 */
export interface SnapshotView {
  /** The target's text, when the projection carries one. */
  text: string | null;
  /** False when the mirror has no live item — withdrawn, or never synced. */
  live: boolean;
  /** Set when the snapshot is not a shape this build knows how to render. */
  raw: string | null;
}

export function readSnapshot(snapshot: unknown): SnapshotView {
  if (typeof snapshot !== "object" || snapshot === null) {
    return { text: null, live: false, raw: snapshot === undefined ? null : JSON.stringify(snapshot) };
  }
  const bag = snapshot as Record<string, unknown>;
  const live = bag.live === true;
  if (typeof bag.text === "string") return { text: bag.text, live, raw: null };
  if (bag.text === null) return { text: null, live, raw: null };
  // A projection this build has no reader for. Shown verbatim rather than
  // hidden: the user is being asked to decide, and "I cannot show you what
  // changed" is information they need in order to answer honestly.
  return { text: null, live, raw: JSON.stringify(snapshot, null, 2) };
}

/**
 * The editable text of a write, when there is one.
 *
 * `WriteOp` grows per milestone (ADR-0006), and a build meeting an op it does
 * not know must still be able to *apply* and *discard* it — only *edit* needs
 * to know where the words are. `null` therefore means "no edit box", not "no
 * actions".
 */
export function editableBody(payload: unknown): string | null {
  if (typeof payload !== "object" || payload === null) return null;
  const comment = (payload as { Comment?: { body?: unknown } }).Comment;
  return comment && typeof comment.body === "string" ? comment.body : null;
}

/** The same payload with its text replaced, ready for `amendWrite`. */
export function withBody(payload: unknown, body: string): WriteOpPayload | null {
  if (typeof payload !== "object" || payload === null) return null;
  const comment = (payload as { Comment?: { entity?: unknown } }).Comment;
  if (!comment || typeof comment.entity !== "string") return null;
  return { Comment: { entity: comment.entity, body } };
}

export interface WriteQueue {
  /** The counts, `{0,0,0}` until the first read answers. */
  readonly counts: QueueCounts;
  /** Every open write, newest first. */
  readonly rows: QueuedWrite[];
  /** Set when the last read or action failed, so the panel can say so. */
  readonly error: string | null;
  /** True while an action is in flight, so a button cannot be double-fired. */
  readonly busy: boolean;
  refresh(): Promise<void>;
  flush(sourceId: string | null): Promise<void>;
  apply(id: number): Promise<void>;
  discard(id: number): Promise<void>;
  amend(id: number, payload: WriteOpPayload): Promise<void>;
}

export function createWriteQueue(): WriteQueue {
  const state = $state<{
    counts: QueueCounts;
    rows: QueuedWrite[];
    error: string | null;
    busy: boolean;
  }>({ counts: NONE, rows: [], error: null, busy: false });

  async function refresh(): Promise<void> {
    try {
      // Both, and in one place: a badge that said "1 needs you" over a panel
      // listing nothing is the two reads having been taken at different times.
      const [counts, rows] = await Promise.all([writeQueueCounts(), pendingWrites()]);
      state.counts = counts;
      state.rows = rows;
      state.error = null;
    } catch (error) {
      // The counts are left where they were rather than zeroed: a failed read
      // is not evidence that nothing is owed, and a badge that blinked to zero
      // during a hiccup is a held write the user stops looking for.
      state.error = message(error);
    }
  }

  /**
   * Run one action, then re-read.
   *
   * Always re-reads, including after a failure: an action that failed with
   * `conflict` failed *because* the queue moved, so the list on screen is the
   * stale thing that caused it.
   */
  async function act(run: () => Promise<void>): Promise<void> {
    if (state.busy) return;
    state.busy = true;
    try {
      await run();
      state.error = null;
    } catch (error) {
      state.error = message(error);
    } finally {
      state.busy = false;
      await refresh();
    }
  }

  return {
    get counts() {
      return state.counts;
    },
    get rows() {
      return state.rows;
    },
    get error() {
      return state.error;
    },
    get busy() {
      return state.busy;
    },
    refresh,
    flush: (sourceId) => act(() => flushWrites(sourceId)),
    apply: (id) => act(() => applyHeldWrite(id)),
    discard: (id) => act(() => discardWrite(id)),
    amend: (id, payload) => act(() => amendWrite(id, payload)),
  };
}

/** Whatever a rejection was, as something displayable. */
function message(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const text = (error as { message: unknown }).message;
    if (typeof text === "string") return text;
  }
  return error instanceof Error ? error.message : String(error);
}
