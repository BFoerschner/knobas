/**
 * The inbox, live — one stream, one count, one home (issue #45).
 *
 * Two things live here rather than in a component, and for the reason
 * `write-queue.svelte.ts` gives for the same split: the **count** is drawn in
 * the top strip and the **stream** in the view it opens, and a badge that
 * disagrees with the list behind it is worse than either being wrong on its
 * own.
 *
 * ## Why the count is read and not counted
 *
 * `inboxCount()` is the backend's own statement counted, and the stream is
 * that same statement's rows. Deriving the badge from `items.length` would be
 * a second definition of "needs me now" living on this side of the bridge --
 * and the first thing it would get wrong is snoozing, because the shelf a
 * component happens to be holding is not the predicate the count is about.
 *
 * ## Why there is no `inbox:*` event to listen to
 *
 * The inbox is derived from the mirror, so it moves when the mirror moves and
 * when the user answers something. Both already have signals: `sync:state`
 * fires when a run finishes and `activity:new` fires on every queue transition
 * and on every inbox answer. A new channel would be one more thing to keep in
 * step, which is the argument `commands::sources::pending_writes` records for
 * the write queue. A failed sign-in (#1571) is read off a source's credential
 * health, which *Test connection* or *Re-enter* can move with no run behind
 * it, so the store also re-reads on the existing `source:health`.
 */
import { listen as tauriListen } from "@tauri-apps/api/event";

import { EVENTS, ipcErrorMessage } from "../ipc";
import { ackAlert as realAck } from "../ipc/assets";
import {
  completeInboxItem as realComplete,
  inboxCount as realCount,
  inboxItems as realItems,
  snoozeInboxItem as realSnooze,
  type InboxEntry,
  type InboxShelf,
} from "../ipc/entity";
import { coalesce } from "../shell/coalesce";
import { keyOf } from "../time/format";

/** The IPC this store needs, injectable so a test needs no Tauri bridge. */
export interface InboxPorts {
  inboxItems: (shelf: InboxShelf) => Promise<InboxEntry[]>;
  inboxCount: () => Promise<number>;
  snoozeInboxItem: (itemKey: string, until: string) => Promise<void>;
  completeInboxItem: (itemKey: string) => Promise<void>;
  /**
   * Ack the open alert of one monitor (#446).
   *
   * The one answer on this store that is **not** an inbox command: acking is
   * an estate write (`ack_alert`), and what clears the item is the alert
   * reading acked rather than an `inbox_state` row. It is here anyway because
   * the row that offers it is an inbox row and the re-read afterwards is this
   * store's — a second store for one button would be a second answer to "what
   * is in the inbox now".
   */
  ackAlert: (monitorId: string) => Promise<void>;
  listen: (event: string, handler: () => void) => Promise<() => void>;
}

export interface Inbox {
  /** What needs you now, newest first. */
  readonly stream: InboxEntry[];
  /** What you deferred, and when each comes back. */
  readonly snoozed: InboxEntry[];
  /** How many need you now. **Not** `stream.length` — see the module note. */
  readonly count: number;
  /** Set when the last read or answer failed, so a surface can say so. */
  readonly error: string | null;
  /**
   * Whether {@link refresh} has ever succeeded — *the stream on this store is
   * a read and not the empty list it was built with*.
   *
   * The one caller is the notifier (#290), and the distinction is the whole of
   * why it exists: the first stream the inbox answers with is the backlog, and
   * a notifier primed against the empty list this store holds *before* that
   * read would announce every line of it. `false` after a failed read, because
   * a read that failed says nothing about what is in the inbox.
   */
  readonly answered: boolean;
  /** True while an answer is in flight, so a button cannot be double-fired. */
  readonly busy: boolean;
  /** Read the count alone — what the strip needs and all it needs. */
  refreshCount(): Promise<void>;
  /** Read both shelves and the count, together. */
  refresh(): Promise<void>;
  snooze(itemKey: string, until: Date): Promise<void>;
  complete(itemKey: string): Promise<void>;
  /**
   * Seen, not fixed: clears this alert's item and leaves the alert open.
   *
   * Takes the **item key**, like {@link snooze} and {@link complete}, although
   * the command underneath takes the monitor: an alert's subject *is* its
   * monitor, and the one place that inverse is written on this side of the
   * bridge is here — beside the other two key-takers — rather than in a
   * component.
   */
  ack(itemKey: string): Promise<void>;
  /**
   * Subscribe to the two signals the inbox moves on. Returns the teardown;
   * calling `start` twice is harmless.
   *
   * **Subscribing only.** The seed is {@link refresh}, and it is the shell's,
   * because only the shell knows when the database can answer — the same
   * division `health.start()`/`health.reseed()` makes.
   */
  start(): () => void;
  /** Tests only: unsubscribe and forget every list, count and answer in flight (#1743). */
  reset(): void;
}

/**
 * The subject half of an item's key.
 *
 * `knobas_core::inbox` composes every key as `<category>:<subject>` and both
 * halves are stable across syncs; this is that format's inverse, and for an
 * alert the subject is the **monitor's entity id** — which is what `ack_alert`
 * takes, since a monitor has at most one open alert. A category is one word
 * with no colon in it, so the first colon is the boundary whatever the subject
 * carries (`gitea:acme/payouts#144` has two more).
 *
 * Exported so its own test can read it; nothing outside this module needs it.
 */
export function subjectOf(key: string): string {
  return keyOf(key);
}

export function createInbox(ports?: InboxPorts): Inbox {
  // Wrapped rather than bound. The module-level `inbox` below is constructed
  // at import time, which for a component test is *before* it has had a chance
  // to do anything -- and a partial `vi.mock` of `../ipc/entity` throws on the
  // first access to an export it did not declare. Reading each function inside
  // its own call defers that to a call no such test ever makes, so a test that
  // mounts the shell for an unrelated reason does not have to widen its mock
  // to keep the window standing.
  const io: InboxPorts = ports ?? {
    inboxItems: (shelf) => realItems(shelf),
    inboxCount: () => realCount(),
    snoozeInboxItem: (itemKey, until) => realSnooze(itemKey, until),
    completeInboxItem: (itemKey) => realComplete(itemKey),
    ackAlert: async (monitorId) => {
      await realAck(monitorId);
    },
    listen: (event, handler) => tauriListen(event, () => handler()),
  };

  const state = $state<{
    stream: InboxEntry[];
    snoozed: InboxEntry[];
    count: number;
    error: string | null;
    busy: boolean;
    answered: boolean;
  }>({ stream: [], snoozed: [], count: 0, error: null, busy: false, answered: false });

  let live = false;
  let stopLive: (() => void) | null = null;

  async function refreshCount(): Promise<void> {
    try {
      state.count = await io.inboxCount();
      state.error = null;
    } catch (error) {
      // The number is left where it was rather than zeroed. A failed read is
      // not evidence that nothing needs you, and a badge that blinked to zero
      // during a hiccup is a review request the reader stops looking for.
      state.error = ipcErrorMessage(error);
    }
  }

  async function refresh(): Promise<void> {
    const seen = answers;
    try {
      // All three together: a badge reading 4 over a list of three rows is the
      // two reads having been taken at different moments.
      const [stream, snoozed, count] = await Promise.all([
        io.inboxItems("stream"),
        io.inboxItems("snoozed"),
        io.inboxCount(),
      ]);
      // An answer made while this read was out is drawn already, and the
      // read after the last one in flight is the one that replaces it (#775).
      // A read that started before an answer was drawn or landed is older
      // than that read, whichever of the two comes back first.
      if (inFlight > 0 || seen !== answers) return;
      state.stream = stream;
      state.snoozed = snoozed;
      state.count = count;
      state.error = null;
      // Last, and only on the way through: `answered` is what tells the
      // notifier that this stream is a read rather than the list this store
      // was built with.
      state.answered = true;
    } catch (error) {
      state.error = ipcErrorMessage(error);
    }
  }

  /** Answers written and not yet answered; the re-read waits for the last. */
  let inFlight = 0;
  /** Bumped when an answer is drawn and when it lands: a read from before either is stale. */
  let answers = 0;
  /** Why an answer of the batch in flight failed, said once the batch is re-read. */
  let refusal: string | null = null;

  /**
   * Run one answer, then re-read — including after a failure.
   *
   * What the answer does is drawn **first** (#775): the row leaves the
   * stream, or moves to the shelf, before the write is made, so the reader
   * goes on to the next row at their own pace and nothing is disabled
   * meanwhile. The re-read then replaces the guess with the backend's answer,
   * once the last answer in flight has landed -- a re-read between two would
   * draw back the row the second one is still taking away. A failed answer
   * comes back with that re-read, and its reason stays said.
   */
  async function act(draw: () => void, run: () => Promise<void>): Promise<void> {
    draw();
    answers += 1;
    inFlight += 1;
    state.busy = true;
    try {
      await run();
    } catch (error) {
      // `not_found` here means the item was resolved at the source while this
      // list was on screen, so the list is the stale thing that caused it —
      // which the re-read below fixes.
      refusal = ipcErrorMessage(error);
    }
    answers += 1;
    inFlight -= 1;
    if (inFlight > 0) return;
    state.busy = false;
    await refresh();
    // The re-read's success is not the answer's: a reason said stays said.
    if (refusal !== null) state.error = refusal;
    refusal = null;
  }

  /** Take the entry for `key` off either shelf, and answer it. */
  function take(key: string): InboxEntry | undefined {
    const streamed = state.stream.find((e) => e.item.key === key);
    const shelved = state.snoozed.find((e) => e.item.key === key);
    state.stream = state.stream.filter((e) => e.item.key !== key);
    state.snoozed = state.snoozed.filter((e) => e.item.key !== key);
    if (streamed) state.count = Math.max(0, state.count - 1);
    return streamed ?? shelved;
  }

  /** A snooze drawn: onto the shelf until `until`, or back into the stream if that is now. */
  function drawSnooze(key: string, until: Date) {
    const entry = take(key);
    if (!entry) return;
    if (until.getTime() <= Date.now()) {
      state.stream = [{ ...entry, item: { ...entry.item, snoozed_until: null } }, ...state.stream];
      state.count += 1;
    } else {
      state.snoozed = [...state.snoozed, { ...entry, item: { ...entry.item, snoozed_until: until.toISOString() } }];
    }
  }

  return {
    get stream() {
      return state.stream;
    },
    get snoozed() {
      return state.snoozed;
    },
    get count() {
      return state.count;
    },
    get error() {
      return state.error;
    },
    get busy() {
      return state.busy;
    },
    get answered() {
      return state.answered;
    },
    refreshCount,
    refresh,
    snooze: (itemKey, until) =>
      act(
        () => drawSnooze(itemKey, until),
        () => io.snoozeInboxItem(itemKey, until.toISOString()),
      ),
    complete: (itemKey) => act(() => void take(itemKey), () => io.completeInboxItem(itemKey)),
    ack: (itemKey) => act(() => void take(itemKey), () => io.ackAlert(subjectOf(itemKey))),
    start() {
      if (live) {
        // Already subscribed. Handing back a teardown that unwinds the *first*
        // subscription would strand it if the second caller stops first.
        return () => {};
      }
      live = true;
      const offs: (() => void)[] = [];

      // Coalesced (#753): a burst of events while a read is in flight makes
      // one more read after it, not one per event.
      const reread = coalesce(refresh);
      for (const event of [EVENTS.activityNew, EVENTS.syncState, EVENTS.sourceHealth]) {
        void io
          .listen(event, () => {
            if (live) void reread();
          })
          .then((unlisten) => {
            if (live) offs.push(unlisten);
            else unlisten();
          })
          .catch(() => {
            // A failed subscription is not a failed window: the seed still
            // renders and the number simply stops moving.
          });
      }

      const stop = () => {
        live = false;
        for (const off of offs) off();
        offs.length = 0;
      };
      stopLive = stop;
      return stop;
    },
    reset() {
      stopLive?.();
      stopLive = null;
      state.stream = [];
      state.snoozed = [];
      state.count = 0;
      state.error = null;
      state.busy = false;
      state.answered = false;
      // A read or answer still out lands on a store that is no longer its own.
      answers += 1;
      inFlight = 0;
      refusal = null;
    },
  };
}


/**
 * The one the window uses.
 *
 * Module-level, like `health` and `toasts`, because two trees read it: the top
 * strip's count and the inbox view. Tests build their own with
 * {@link createInbox}.
 */
export const inbox = createInbox();

/** One snooze offer: what the button says, and when the item comes back. */
export interface SnoozePreset {
  label: string;
  at: Date;
}

/**
 * Tomorrow morning, next Monday morning, and — for an item whose subject has
 * a deadline — the day after it (#45, story 14).
 *
 * **09:00 local, not "in 24 hours".** *Tomorrow* means the start of the next
 * working day, and an item snoozed at 16:40 that returns at 16:40 comes back
 * at the end of a day rather than at the beginning of one.
 *
 * The third preset is offered only when there is a date to hang it on, which
 * for M2 is a credential expiry: *after it expires* is a real answer for a
 * token and nonsense for a review request, and a preset that computes to the
 * same thing as *tomorrow* on five of the six categories is a button that
 * teaches the reader nothing.
 */
export function snoozePresets(now: Date, deadline?: Date | null): SnoozePreset[] {
  const presets: SnoozePreset[] = [
    { label: "Tomorrow", at: morningAfter(now, 1) },
    { label: "Next Monday", at: morningAfter(now, daysToNextMonday(now)) },
  ];
  if (deadline && !Number.isNaN(deadline.getTime()) && deadline.getTime() > now.getTime()) {
    presets.push({ label: "After it expires", at: morningAfter(deadline, 1) });
  }
  return presets;
}

/**
 * The day a date input produced (`2026-08-24`), at 09:00 local -- the
 * presets' own convention -- or `null` for an empty or malformed value, a
 * year below 1000 (a half-typed year) or a day that is not strictly after
 * `now`'s day (a past `until` puts the item straight back in the stream).
 */
export function pickedMorning(value: string, now: Date = new Date()): Date | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) return null;
  const [y, m, d] = [Number(match[1]), Number(match[2]), Number(match[3])];
  if (y < 1000 || m < 1 || m > 12 || d < 1 || d > 31) return null;
  // `new Date(y, ...)` maps years 0-99 to 1900-1999; setFullYear keeps them literal.
  const at = new Date(2000, 0, 1, 9, 0, 0, 0);
  at.setFullYear(y, m - 1, d);
  if (at.getMonth() !== m - 1) return null; // 31 Feb rolled over
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const day = new Date(at.getFullYear(), at.getMonth(), at.getDate());
  return day.getTime() > today.getTime() ? at : null;
}

/** Local `YYYY-MM-DD` of the day after `now`: the date field's `min` hint. */
export function tomorrowValue(now: Date): string {
  const t = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${t.getFullYear()}-${pad(t.getMonth() + 1)}-${pad(t.getDate())}`;
}

/** 09:00 local, `days` after the day `from` falls on. */
function morningAfter(from: Date, days: number): Date {
  const at = new Date(from.getFullYear(), from.getMonth(), from.getDate() + days, 9, 0, 0, 0);
  return at;
}

/**
 * How many days until the next Monday. **Never 0**: asked on a Monday, *next
 * Monday* is the one in seven days' time, not this morning — an item that came
 * back the instant it was deferred would make the button read as a bug.
 */
function daysToNextMonday(now: Date): number {
  const ahead = (8 - now.getDay()) % 7;
  return ahead === 0 ? 7 : ahead;
}
