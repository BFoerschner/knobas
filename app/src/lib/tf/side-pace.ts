/**
 * **When a sidebar's drag stops reflowing the page** (Fable's ruling under
 * Björn's delegation, the sidebar-resize pass, PR #2479;
 * `docs/decisions/2026-11-16-every-sidebar-drags.md`).
 *
 * Every drag starts live: each frame writes the new width and the page lays
 * itself out at it. A frame that wrote a width costs the gap from its
 * timestamp to the next frame's; over {@link SLOW_MS} it is slow (the 55 fps
 * line is 18.2 ms, and 19 sits between the vsync multiples of 60, 100, 120,
 * 144 and 239 Hz panels). Two slow frames within any four written in a row
 * switch the drag to a ghost until it lets go: only the edge's line follows
 * the pointer, and the width is written once the pointer has rested within
 * {@link REST_PX} for {@link REST_MS}, and on release. One slow frame alone
 * never trips, so a collection or a sync landing mid-drag does not.
 *
 * A sidebar that tripped starts its later drags as a ghost, for the session.
 * The frame that applies the width on release is timed the same way; two
 * releases in a row at {@link SLOW_MS} or under, each having changed the width,
 * clear the mark.
 */
import type { SideId } from "./side-width.svelte";

export const SLOW_MS = 19;
/** Of how many written frames in a row two slow ones trip the ghost. */
export const TRIP_WINDOW = 4;
export const TRIP_SLOW = 2;
export const REST_MS = 150;
export const REST_PX = 2;

/** One drag's written frames: whether the next one should be a ghost's. */
export class DragPace {
  #ghost: boolean;
  #recent: boolean[] = [];
  #wroteAt: number | null = null;

  constructor(ghost: boolean) {
    this.#ghost = ghost;
  }

  get ghost(): boolean {
    return this.#ghost;
  }

  /** A frame at `ts`: the frame before it that wrote is costed. True when this frame turned the drag into a ghost. */
  frame(ts: number): boolean {
    if (this.#wroteAt === null) return false;
    const slow = ts - this.#wroteAt > SLOW_MS;
    this.#wroteAt = null;
    this.#recent.push(slow);
    if (this.#recent.length > TRIP_WINDOW) this.#recent.shift();
    if (this.#ghost || this.#recent.filter(Boolean).length < TRIP_SLOW) return false;
    this.#ghost = true;
    return true;
  }

  /** This frame, at `ts`, wrote a width. */
  wrote(ts: number): void {
    this.#wroteAt = ts;
  }
}

/** The sidebars whose drags start as a ghost this session, and how many fast releases each has had since. */
const slow = new Map<SideId, number>();

export function startsAsGhost(side: SideId): boolean {
  return slow.has(side);
}

/** A drag of `side` tripped its ghost. */
export function trippedOn(side: SideId): void {
  slow.set(side, 0);
}

/** A release that changed `side`'s width applied in `ms`: two in a row at the line clear the mark. */
export function releasedIn(side: SideId, ms: number): void {
  if (!slow.has(side)) return;
  if (ms > SLOW_MS) slow.set(side, 0);
  else if (slow.get(side)! + 1 >= 2) slow.delete(side);
  else slow.set(side, 1);
}

/** Tests only: no sidebar starts as a ghost. */
export function forgetPace(): void {
  slow.clear();
}
