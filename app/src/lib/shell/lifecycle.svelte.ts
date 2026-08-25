/**
 * The boot state machine: what the window knows about the database.
 *
 * **Two channels, deliberately.** The event (`db:state`) makes a transition
 * instant; the poll (`app_status`) makes it correct. An emit that raced the
 * listener, a dropped event, a backend that never reached its `emit` — all
 * three end in the same place, a poll that keeps asking until the answer is
 * `ready` or `failed`. Neither channel alone is both live and reliable.
 */
import { listen } from "@tauri-apps/api/event";

import { EVENTS } from "../ipc";
import {
  appStatus,
  frontendReady,
  retryDatabase,
  type AppStatus,
  type DbState,
} from "../ipc/app";

/** How often to ask, while the answer is still provisional. */
const DEFAULT_POLL_MS = 500;

export interface Lifecycle {
  readonly db: DbState;
  readonly status: AppStatus | null;
  readonly ready: boolean;
  /** The failure message, or `null` — never a code, never a guess. */
  readonly error: string | null;
  start(): Promise<void>;
  stop(): void;
  retry(): Promise<void>;
}

/** Whether there is any point asking again. */
function settled(db: DbState): boolean {
  return db.state === "ready" || db.state === "failed";
}

export function createLifecycle(options: { pollMs?: number } = {}): Lifecycle {
  const pollMs = options.pollMs ?? DEFAULT_POLL_MS;
  const state = $state<{ db: DbState; status: AppStatus | null }>({
    db: { state: "starting", detail: null },
    status: null,
  });

  let timer: ReturnType<typeof setTimeout> | undefined;
  let unlisten: (() => void) | undefined;
  let stopped = false;

  async function poll(): Promise<void> {
    if (stopped) return;
    try {
      const status = await appStatus();
      state.status = status;
      state.db = status.db;
    } catch {
      // Not a failed database: `invoke` rejects until Tauri has injected its
      // internals, and on the very first frames it has not. Reporting that as
      // a failure would flash an error screen on every cold start. The next
      // tick asks again.
    }
    if (!stopped && !settled(state.db)) {
      timer = setTimeout(() => void poll(), pollMs);
    }
  }

  return {
    get db() {
      return state.db;
    },
    get status() {
      return state.status;
    },
    get ready() {
      return state.db.state === "ready";
    },
    get error() {
      return state.db.state === "failed" ? state.db.message : null;
    },

    async start() {
      stopped = false;
      // Listener first, then `frontend_ready` — that ordering is the entire
      // reason the command exists (gotcha 9). Reversed, the replay lands
      // before anything is listening and the boot screen sits on "starting"
      // until its next poll.
      // `listen` is itself an `invoke` (`plugin:event|listen`), so it can
      // reject — the bridge not up yet, the event plugin refused. That must
      // degrade to "no live updates", never to "no updates at all": the poll
      // below is the channel that is allowed to be slow but not the one that
      // is allowed to be missing.
      let off: (() => void) | undefined;
      try {
        off = await listen<DbState>(EVENTS.dbState, (event) => {
          state.db = event.payload;
          // `ready` also means the counts are now answerable, and they arrive
          // on `app_status`, not on the event.
          if (event.payload.state === "ready") void poll();
        });
      } catch {
        off = undefined;
      }
      if (stopped) {
        // Unmounted inside the await. `listen` resolves its teardown
        // asynchronously, so without this the subscription outlives the thing
        // that made it — one leak per mount, for the life of the process.
        off?.();
        return;
      }
      unlisten = off;

      // Swallowed on purpose: if the replay cannot be armed there is nothing
      // to do about it here, and the poll below already covers the case.
      await frontendReady().catch(() => undefined);
      await poll();
    },

    stop() {
      stopped = true;
      clearTimeout(timer);
      unlisten?.();
      unlisten = undefined;
    },

    async retry() {
      stopped = false;
      // The backend has to start the database again. A retry that only
      // re-polled would redraw the same failure for ever, which is a button
      // that lies about what it does.
      await retryDatabase();
      state.db = { state: "starting", detail: null };
      await poll();
    },
  };
}

/** The one the window uses. Tests build their own with `createLifecycle`. */
export const lifecycle = createLifecycle();
