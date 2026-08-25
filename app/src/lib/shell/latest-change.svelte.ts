/**
 * The newest thing that happened, for the status bar's one-liner.
 *
 * Spec §2 asks the status bar to carry a *"latest change"* line — the ticker's
 * quiet replacement. It is fed from two places: `recentActivity(1)` seeds it
 * when the window opens, and `activity:new` moves it afterwards.
 *
 * ## Why it coalesces
 *
 * `activity:new` is already throttled to at most one per second by the emitter
 * (interfaces §2.3). This is the second wall, and it is not redundant: a sync
 * that writes two hundred lines must not make the bottom of the window
 * flicker two hundred times *whatever the backend does tomorrow*. The window
 * is **trailing** — the first line shows at once and a quiet arrival shows at
 * once; only a burst is held, and only its newest member is ever drawn.
 */
import type { ActivityRow } from "../ipc/entity";

/** At most one visible update per this many milliseconds. */
const DEFAULT_WINDOW_MS = 1000;

export interface LatestChange {
  /** The line on screen, or `null` before anything has happened. */
  readonly current: ActivityRow | null;
  /** Offer a line. It is shown now, or at the end of the current window. */
  push(row: ActivityRow): void;
  /** Cancel anything pending. Called from the component's teardown. */
  stop(): void;
}

export function createLatestChange(options: { windowMs?: number } = {}): LatestChange {
  const windowMs = options.windowMs ?? DEFAULT_WINDOW_MS;

  /**
   * A `$state` object with a field rather than a bare `$state(null)`: an
   * exported closure cannot reassign a `let` its caller holds, and the getter
   * below has to read through something stable.
   */
  const state = $state<{ current: ActivityRow | null }>({ current: null });

  /** The newest line held back by the window, if any. Plain: not rendered. */
  let held: ActivityRow | null = null;
  /** Armed only while a window is open. `undefined` means "show at once". */
  let timer: ReturnType<typeof setTimeout> | undefined;

  function release() {
    timer = undefined;
    if (held === null) return;
    const next = held;
    held = null;
    // Through `push` again, so the *next* burst is held too — a release that
    // wrote `state.current` directly would leave the window closed and let an
    // ongoing burst through one line at a time.
    push(next);
  }

  function push(row: ActivityRow) {
    if (timer !== undefined) {
      // Inside a window: the newest wins, and the ones in between are dropped
      // rather than queued. A queue would render every one of them, late.
      held = row;
      return;
    }
    state.current = row;
    timer = setTimeout(release, windowMs);
  }

  return {
    get current() {
      return state.current;
    },
    push,
    stop() {
      if (timer !== undefined) clearTimeout(timer);
      timer = undefined;
      held = null;
    },
  };
}
