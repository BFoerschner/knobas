/**
 * **When a sidebar's drag stops reflowing the page** (Fable's rulings under
 * Björn's delegation: the sidebar-resize pass, PR #2479,
 * `docs/decisions/2026-11-16-every-sidebar-drags.md`; and its start on the
 * PC, `docs/decisions/2026-11-17-a-hover-a-drag-and-the-web-views-menu-on-the-pc.md`).
 *
 * Every frame that writes a width is costed by the gap from its timestamp to
 * the next frame's; over {@link SLOW_MS} it is slow (the 55 fps line is
 * 18.2 ms, and 19 sits between the vsync multiples of 60, 100, 120, 144 and
 * 239 Hz panels).
 *
 * - **This drag** turns into a ghost when its first written frame is slow, or
 *   when two of any four written in a row are: only the edge's line follows
 *   the pointer, and the width is written once the pointer has rested within
 *   {@link REST_PX} for {@link REST_MS} (a landing), and on release (a
 *   landing). It stays a ghost until it lets go.
 * - **The sidebar** is marked slow when two of four written frames in a row
 *   are slow, a ghost's landings among them, so a 1x hiccup on a first frame
 *   costs that one drag and marks nothing. A marked sidebar starts its drags
 *   as a ghost, across restarts (`localStorage`, {@link STORE_PREFIX}), so a
 *   slow machine pays the first slow frame once per sidebar, not once a
 *   session.
 * - **The mark clears** after two landings in a row at {@link SLOW_MS} or
 *   under, at rest or on release; a slow one starts the count again.
 */
import type { SideId } from "./side-width.svelte";

export const SLOW_MS = 19;
/** Of how many written frames in a row two slow ones trip the ghost and mark the sidebar. */
export const TRIP_WINDOW = 4;
export const TRIP_SLOW = 2;
export const REST_MS = 150;
export const REST_PX = 2;
/** A marked sidebar's key: present means its drags start as a ghost. */
export const STORE_PREFIX = "knobas.side-pace.";

/** One drag's written frames: whether the next one should be a ghost's, and what they say of the sidebar. */
export class DragPace {
  #ghost: boolean;
  #side: SideId | null;
  #recent: boolean[] = [];
  #costed = 0;
  #wroteAt: number | null = null;
  #landing = false;
  #marked = false;

  /** `side`, when given, is marked and cleared through this module's memory; a test may leave it out. */
  constructor(ghost: boolean, side: SideId | null = null) {
    this.#ghost = ghost;
    this.#side = side;
  }

  get ghost(): boolean {
    return this.#ghost;
  }

  /** Whether this drag's frames marked the sidebar slow. */
  get marked(): boolean {
    return this.#marked;
  }

  /** A frame at `ts`: the frame before it that wrote is costed. True when this frame turned the drag into a ghost. */
  frame(ts: number): boolean {
    if (this.#wroteAt === null) return false;
    const gap = ts - this.#wroteAt;
    const landing = this.#landing;
    this.#wroteAt = null;
    this.#landing = false;
    return this.#cost(gap, landing);
  }

  /** This frame, at `ts`, wrote a width; `landing` when it is a ghost's (at rest). */
  wrote(ts: number, landing = false): void {
    this.#wroteAt = ts;
    this.#landing = landing;
  }

  /** The release wrote a width, and the frame that laid it out took `ms`: a landing, costed as any written frame. */
  released(ms: number): void {
    this.#cost(ms, true);
  }

  #cost(gap: number, landing: boolean): boolean {
    const slow = gap > SLOW_MS;
    this.#costed += 1;
    this.#recent.push(slow);
    if (this.#recent.length > TRIP_WINDOW) this.#recent.shift();
    const two = this.#recent.filter(Boolean).length >= TRIP_SLOW;
    if (two && !this.#marked) {
      this.#marked = true;
      if (this.#side !== null) markSlow(this.#side);
    }
    if (landing && this.#side !== null) landedIn(this.#side, gap);
    if (this.#ghost || !(two || (this.#costed === 1 && slow))) return false;
    this.#ghost = true;
    return true;
  }
}

/** The marked sidebars, and how many fast landings in a row each has had since. Read from storage once. */
let slow: Map<SideId, number> | null = null;

function storage(): Storage | null {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
}

function marks(): Map<SideId, number> {
  if (slow !== null) return slow;
  slow = new Map();
  const kept = storage();
  try {
    for (let index = 0; kept !== null && index < kept.length; index += 1) {
      const key = kept.key(index);
      if (key?.startsWith(STORE_PREFIX)) slow.set(key.slice(STORE_PREFIX.length) as SideId, 0);
    }
  } catch {
    // No storage to read: the session's memory alone, as before.
  }
  return slow;
}

export function startsAsGhost(side: SideId): boolean {
  return marks().has(side);
}

/** `side`'s frames were slow twice in four: its drags start as a ghost from now on, restarts included. */
export function markSlow(side: SideId): void {
  marks().set(side, 0);
  try {
    storage()?.setItem(STORE_PREFIX + side, "1");
  } catch {
    // Full or refused: kept for the session.
  }
}

/** A landing of `side`'s width took `ms`: two in a row at the line clear its mark. */
export function landedIn(side: SideId, ms: number): void {
  const memory = marks();
  if (!memory.has(side)) return;
  if (ms > SLOW_MS) memory.set(side, 0);
  else if (memory.get(side)! + 1 >= 2) {
    memory.delete(side);
    try {
      storage()?.removeItem(STORE_PREFIX + side);
    } catch {
      // Refused: the mark comes back next session and clears again.
    }
  } else memory.set(side, 1);
}

/** Tests only: no sidebar is marked, in memory or in storage. */
export function forgetPace(): void {
  const kept = storage();
  try {
    for (let index = (kept?.length ?? 0) - 1; kept !== null && index >= 0; index -= 1) {
      const key = kept.key(index);
      if (key?.startsWith(STORE_PREFIX)) kept.removeItem(key);
    }
  } catch {
    // Nothing stored.
  }
  slow = null;
}

/** Tests only: the memory dropped, as a restart drops it; the next read comes from storage. */
export function rereadPace(): void {
  slow = null;
}
