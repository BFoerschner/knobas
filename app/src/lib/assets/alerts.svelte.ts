/**
 * The estate's open alerts, live — one read, one list, one count (issue #444).
 *
 * Two trees draw this: the top strip's Assets badge and the Assets view's list
 * of what is wrong. A store rather than a fetch in each, for the reason
 * `inbox.svelte.ts` gives for the same split — a badge that disagrees with the
 * list behind it is worse than either being wrong on its own.
 *
 * ## Why the count *is* counted here, where the inbox's is not
 *
 * The inbox reads its badge from a statement of its own, because "needs me
 * now" excludes what is snoozed and the shelf a component happens to be
 * holding is not that predicate. An alert has no such second predicate: spec
 * #427 story 58 is *"every open alert visible in the Assets view and counted
 * in the top strip"*, one set and one number over it. So `open_alerts` is one
 * command, `count` is `open.length`, and there is no second statement for the
 * two to disagree about — which is the same guarantee by the opposite route.
 *
 * ## Why there is no `alert:*` event to listen to
 *
 * An alert opens and closes inside a sync run and is acked by the reader
 * (#446), and both already have signals: `sync:state` fires when a run
 * finishes and `activity:new` fires on every ack's history line. A third
 * channel would be a third thing to keep in step — the argument
 * `commands::sources::pending_writes` records for the write queue, and
 * `commands::assets::open_alerts` records again for this read.
 */
import { listen as tauriListen } from "@tauri-apps/api/event";

import { EVENTS, ipcErrorMessage } from "../ipc";
import {
  ackAlert as realAckAlert,
  openAlerts as realOpenAlerts,
  type OpenAlert,
} from "../ipc/assets";

/** The IPC this store needs, injectable so a test needs no Tauri bridge. */
export interface AlertPorts {
  openAlerts: () => Promise<OpenAlert[]>;
  /**
   * Ack the open alert of one monitor (#446, #449's cards).
   *
   * On this store rather than in the component that draws the button, for the
   * reason `createInbox` keeps its own ack: what an ack changes is *this list*
   * — the alert stays open and starts reading acked — so the write and the
   * re-read after it are one statement. A card that acked through its own port
   * would leave the top strip's badge and the Assets view's strip reading the
   * pre-ack answer until the next event arrived.
   */
  ackAlert: (monitorId: string) => Promise<void>;
  listen: (event: string, handler: () => void) => Promise<() => void>;
}

export interface Alerts {
  /** Every open alert, newest first. */
  readonly open: OpenAlert[];
  /** How many. `open.length` — see the module note. */
  readonly count: number;
  /**
   * Set when the last read or ack failed, so a surface can say so.
   *
   * An **ack's** failure outlives the re-read that follows it: the re-read
   * clears `error` when it succeeds, and a message wiped a tick after it was
   * written is a write the reader is never told failed. The fresh list is
   * evidence about the alerts, not about the ack.
   */
  readonly error: string | null;
  /** Read them all. */
  refresh(): Promise<void>;
  /**
   * Ack one monitor's open alert — **seen, not fixed**.
   *
   * The alert stays open (only a return to `up` closes one) and comes back
   * carrying `acked_at`; what it clears is the reader's inbox item. The
   * re-read afterwards happens whether the write succeeded or failed, the
   * inbox's rule: a `not_found` means the alert closed while the card was on
   * screen, so the stale list is what caused it.
   *
   * **No in-flight flag here.** Two acks on two monitors are independent
   * writes, and a store-wide guard would drop the second silently; the card
   * keeps its own button down, which is the only race there is.
   */
  ack(monitorId: string): Promise<void>;
  /**
   * Subscribe to the two signals an alert moves on. Returns the teardown;
   * calling `start` twice is harmless.
   *
   * **Subscribing only.** The seed is {@link refresh}, and it is the shell's,
   * because only the shell knows when the database can answer — the division
   * `inbox.start()`/`inbox.refresh()` already makes.
   */
  start(): () => void;
}

export function createAlerts(ports?: Partial<AlertPorts>): Alerts {
  // Wrapped rather than bound, for the reason `createInbox` records: this
  // module's singleton is constructed at import time, and a partial
  // `vi.mock` of `../ipc/assets` throws on the first access to an export it
  // did not declare. Reading the function inside its own call defers that to
  // a call no such test makes -- which is what lets the defaults be built
  // even when a caller overrides them, the `Partial` this takes since #449.
  const io: AlertPorts = {
    openAlerts: () => realOpenAlerts(),
    ackAlert: async (monitorId) => {
      await realAckAlert(monitorId);
    },
    listen: (event, handler) => tauriListen(event, () => handler()),
    ...ports,
  };

  const state = $state<{ open: OpenAlert[]; error: string | null }>({
    open: [],
    error: null,
  });

  let live = false;

  async function refresh(): Promise<void> {
    try {
      state.open = await io.openAlerts();
      state.error = null;
    } catch (error) {
      // The list is left where it was rather than emptied. A failed read is
      // not evidence that the estate is well, and a badge that blinked to
      // zero during a hiccup is an outage the reader stops looking for.
      state.error = ipcErrorMessage(error);
    }
  }

  return {
    get open() {
      return state.open;
    },
    get count() {
      return state.open.length;
    },
    get error() {
      return state.error;
    },
    refresh,
    async ack(monitorId: string): Promise<void> {
      let failed: string | null = null;
      try {
        await io.ackAlert(monitorId);
      } catch (error) {
        failed = ipcErrorMessage(error);
      }
      // Always, whether it worked or not: a `not_found` means the alert closed
      // while the card was on screen, so the stale list is what caused it.
      await refresh();
      // And after the re-read, not before it. `refresh` clears `error` on
      // success, so writing the ack's failure first would erase it with the
      // very read that proves the reader is looking at a live list.
      state.error = failed ?? state.error;
    },
    start() {
      if (live) {
        // Already subscribed. Handing back a teardown that unwinds the *first*
        // subscription would strand it if the second caller stops first.
        return () => {};
      }
      live = true;
      const offs: (() => void)[] = [];

      for (const event of [EVENTS.activityNew, EVENTS.syncState]) {
        void io
          .listen(event, () => {
            if (live) void refresh();
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

      return () => {
        live = false;
        for (const off of offs) off();
        offs.length = 0;
      };
    },
  };
}

/**
 * The one the window uses.
 *
 * Module-level, like `inbox` and `health`, because two trees read it: the top
 * strip's count and the Assets view's list. Tests build their own with
 * {@link createAlerts}.
 */
export const alerts = createAlerts();
