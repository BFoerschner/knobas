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
  amendWrite,
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
  /**
   * The **version** this side of the comparison is of, where the op has one.
   *
   * A held page edit is the one write whose two sides can look identical while
   * differing in what matters: a wiki page whose body was reformatted, or
   * moved, or whose macro re-rendered, reads the same stripped down to text.
   * The number is what says the mirror moved (#286), so the panel shows it
   * beside each side rather than asking the reader to spot the difference.
   *
   * `null` for every other op. Read only from an `update_page` snapshot, which
   * one adapter declares and no other — so this is gated on the op the same
   * way `storage-format.ts`'s reads are gated on the adapter kind, and it
   * misses to `null` like them.
   */
  version: number | null;
}

export function readSnapshot(snapshot: unknown): SnapshotView {
  if (typeof snapshot !== "object" || snapshot === null) {
    return {
      text: null,
      live: false,
      raw: snapshot === undefined ? null : JSON.stringify(snapshot),
      version: null,
    };
  }
  const bag = snapshot as Record<string, unknown>;
  const live = bag.live === true;
  const version = pageVersion(bag);
  if (typeof bag.text === "string") return { text: bag.text, live, raw: null, version };
  if (bag.text === null) return { text: null, live, raw: null, version };
  // A projection this build has no reader for. Shown verbatim rather than
  // hidden: the user is being asked to decide, and "I cannot show you what
  // changed" is information they need in order to answer honestly.
  return { text: null, live, raw: JSON.stringify(snapshot, null, 2), version };
}

/**
 * `version.number` out of an `update_page` snapshot's mirrored payload.
 *
 * The whole-record projection carries the payload verbatim, and a Confluence
 * page keeps its version there. Nothing else is read out of it, and nothing
 * but `update_page` is read at all: a `version` key means different things in
 * different sources, and guessing from a payload's shape is the guess ADR-0007
 * forbids.
 */
function pageVersion(bag: Record<string, unknown>): number | null {
  if (bag.op !== "update_page") return null;
  const payload = bag.payload;
  if (typeof payload !== "object" || payload === null || Array.isArray(payload)) return null;
  const version = (payload as Record<string, unknown>).version;
  if (typeof version !== "object" || version === null || Array.isArray(version)) return null;
  const number = (version as Record<string, unknown>).number;
  return typeof number === "number" && Number.isSafeInteger(number) ? number : null;
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

/** What a queued `log_work` write would put on a ticket, read off its payload. */
export interface WithdrawnWorklog {
  /** The ticket the time was logged against. */
  entity: string;
  /** RFC 3339 -- when the logged span began. */
  started: string;
  /** How long was worked, in the seconds that cross the wire. */
  seconds: number;
  comment: string;
}

/**
 * The worklog a discard would withdraw, when withdrawing it can bill the same
 * hour twice. `null` when the withdrawal is an ordinary one.
 *
 * ## Why `log_work` and no other op
 *
 * Since #328 a discarded worklog **gives its blocks back**: the local copy
 * goes with the write and the hour is unlogged again, offered by *Log all*
 * and editable in the day review. Set that beside the race
 * `knobas_core::write_queue::discard` names in its own doc comment — the
 * flush loop's per-source lock does not hold a discard back, and every writer
 * of `attempts` bumps it *after* the call, so a write in flight is
 * indistinguishable from one never tried — and the window is one HTTP
 * round-trip wide. Inside it, the hour is at Jira *and* back on knobas'
 * timesheet, one click from being sent again.
 *
 * No other op has that shape, because no other op hands a resource back:
 *
 * * `comment`, `approve`, `transition`, `trigger_build`, `rerun_build` —
 *   withdrawing one in flight leaves the reply posted or the status moved,
 *   which is the write the user asked for arriving. Nothing is re-offered and
 *   nothing is charged twice.
 * * `create_ticket` — ADR-0012 names it as the create that is *not* naturally
 *   idempotent, but the duplicate it warns about is the **queue re-sending**,
 *   which a discard prevents rather than causes. What a withdrawn
 *   `create_ticket` can leave is one unclaimed ticket at the source: a
 *   residue a person can see and delete, not an hour on an invoice, and not
 *   something knobas then offers to do again.
 * * `create_branch`, `create_pull_request` — the source refuses the duplicate
 *   with a 409 (ADR-0012), so a re-send cannot make a second one.
 *
 * ## Why the state is not part of the question
 *
 * It is tempting to ask only about a `pending` row, on the grounds that
 * `knobas_core::write_queue::due` yields an entity's head only when it is
 * pending and so the flush loop cannot be holding a held or refused one. That
 * reasoning is wrong in both directions, and `discard` releases the blocks of
 * all three states alike:
 *
 * * **Refused.** `knobas_source_jira::write::log_work` answers
 *   `SourceError::Protocol` "if Jira accepted the worklog but did not name
 *   it", and a bare `Protocol` is not retryable, so that row lands in
 *   `refused`. It is a refusal whose worklog is *at Jira*.
 * * **Held.** Hold detection runs *before* the send, so a write that arrived
 *   and whose `sent` never landed sits `pending` until the next flush -- which
 *   re-reads the target, finds it moved, and holds it. The hour is at Jira and
 *   the row says the target changed.
 *
 * The honest rule is the one `discard`'s own doc comment states: nothing in
 * the queue tells a write that arrived and was never settled from one that
 * never left -- not the state, and not `attempts`, which every writer bumps
 * *after* the call. So every open worklog is asked about, and the dialog says
 * exactly that rather than pretending to know which.
 *
 * Only *open* rows ever reach this: `pendingWrites` returns `pending`, `held`
 * and `refused` and nothing else, so a settled row is not a case to guard.
 *
 * `WriteOp` grows per milestone (ADR-0006), so the payload is narrowed rather
 * than asserted: a build that cannot read this worklog cannot describe it
 * either, and a dialog with blanks where the hours go is worse than none.
 */
export function withdrawnWorklog(row: QueuedWrite): WithdrawnWorklog | null {
  if (row.op !== "log_work") return null;
  if (typeof row.payload !== "object" || row.payload === null) return null;
  const log = (row.payload as { LogWork?: Record<string, unknown> }).LogWork;
  if (!log) return null;
  if (typeof log.entity !== "string" || typeof log.started !== "string") return null;
  if (typeof log.seconds !== "number") return null;
  return {
    entity: log.entity,
    started: log.started,
    seconds: log.seconds,
    comment: typeof log.comment === "string" ? log.comment : "",
  };
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
