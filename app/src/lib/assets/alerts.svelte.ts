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
import { openAlerts as realOpenAlerts, type OpenAlert } from "../ipc/assets";

/** The IPC this store needs, injectable so a test needs no Tauri bridge. */
export interface AlertPorts {
  openAlerts: () => Promise<OpenAlert[]>;
  listen: (event: string, handler: () => void) => Promise<() => void>;
}

export interface Alerts {
  /** Every open alert, newest first. */
  readonly open: OpenAlert[];
  /** How many. `open.length` — see the module note. */
  readonly count: number;
  /** Set when the last read failed, so a surface can say so. */
  readonly error: string | null;
  /** Read them all. */
  refresh(): Promise<void>;
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

export function createAlerts(ports?: AlertPorts): Alerts {
  // Wrapped rather than bound, for the reason `createInbox` records: this
  // module's singleton is constructed at import time, and a partial
  // `vi.mock` of `../ipc/assets` throws on the first access to an export it
  // did not declare. Reading the function inside its own call defers that to
  // a call no such test makes.
  const io: AlertPorts = ports ?? {
    openAlerts: () => realOpenAlerts(),
    listen: (event, handler) => tauriListen(event, () => handler()),
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
